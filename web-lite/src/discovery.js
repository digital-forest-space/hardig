import { PublicKey } from '@solana/web3.js';
import {
  PROGRAM_ID,
  KEY_STATE_SIZE,
  derivePositionPda,
  deriveKeyStatePda,
  deriveMarketConfigPda,
  DEFAULT_NAV_SOL_MINT,
} from './constants.js';
import { parseKeyState } from './rateLimits.js';
import {
  positionPda,
  position,
  myPermissions,
  myKeyAsset,
  myNftMint,
  keyring,
  protocolExists,
  collection,
  mayflowerInitialized,
  marketConfigPda,
  marketConfig,
  pushLog,
  resetPositionState,
  discoveredPositions,
  activePositionIndex,
  discoveredPromos,
} from './state.js';
import { shortPubkey, permissionsName, PERM_MANAGE_KEYS, PRESET_ADMIN } from './utils.js';
import { deriveConfigPda } from './constants.js';

export async function checkProtocol(connection) {
  try {
    const [configPda] = deriveConfigPda();
    const info = await connection.getAccountInfo(configPda);
    protocolExists.value = info !== null;
    if (info && info.data.length >= 72) {
      // ProtocolConfig layout: discriminator(8) + admin(32) + collection(32) + bump(1)
      // collection is at bytes 40-72
      const collectionPubkey = new PublicKey(info.data.slice(40, 72));
      collection.value = collectionPubkey.equals(PublicKey.default)
        ? null
        : collectionPubkey;
    } else {
      collection.value = null;
    }
  } catch (e) {
    protocolExists.value = false;
    collection.value = null;
  }
}

/**
 * Parse an MPL-Core AssetV1 account to extract owner, update_authority, and permissions.
 * Returns null if the account is not a valid MPL-Core asset.
 */
function parseMplCoreAsset(data) {
  if (!data || data.length < 66) return null;

  // First byte: Key enum. AssetV1 = 1
  if (data[0] !== 1) return null;

  // Bytes 1..33: owner
  const owner = new PublicKey(data.slice(1, 33));

  // Bytes 33: UpdateAuthority tag (0 = None, 1 = Address, 2 = Collection)
  const uaTag = data[33];
  let updateAuthority = null;
  if ((uaTag === 1 || uaTag === 2) && data.length >= 66) {
    updateAuthority = new PublicKey(data.slice(34, 66));
  } else if (uaTag !== 0) {
    return null; // unknown tag
  }

  return { owner, updateAuthority };
}

/**
 * Read a named attribute value from an MPL-Core asset's Attributes plugin.
 * Scans the serialized plugin data for a borsh-encoded Attribute { key: String, value: String }.
 * Returns the value string, or null if not found.
 */
function readAttributeFromAssetData(data, attributeName) {
  const needleBytes = new TextEncoder().encode(attributeName);

  for (let i = 66; i < data.length - needleBytes.length - 8; i++) {
    const view = new DataView(data.buffer, data.byteOffset);
    if (i + 4 + needleBytes.length + 4 > data.length) break;

    const keyLen = view.getUint32(i, true);
    if (keyLen !== needleBytes.length) continue;

    let match = true;
    for (let j = 0; j < needleBytes.length; j++) {
      if (data[i + 4 + j] !== needleBytes[j]) {
        match = false;
        break;
      }
    }
    if (!match) continue;

    const valueOffset = i + 4 + needleBytes.length;
    if (valueOffset + 4 > data.length) continue;
    const valueLen = view.getUint32(valueOffset, true);
    if (valueLen === 0 || valueLen > 200 || valueOffset + 4 + valueLen > data.length) continue;

    return new TextDecoder().decode(data.slice(valueOffset + 4, valueOffset + 4 + valueLen));
  }

  return null;
}

/**
 * Read the permissions attribute from an MPL-Core asset.
 * Returns the permissions u8 value, or null if not found.
 */
function readPermissionsFromAssetData(data) {
  const value = readAttributeFromAssetData(data, 'permissions');
  if (value === null) return null;
  const parsed = parseInt(value, 10);
  return (!isNaN(parsed) && parsed >= 0 && parsed <= 255) ? parsed : null;
}

/**
 * Read the "position" attribute from an MPL-Core asset.
 * Returns the admin_asset pubkey string that this key is bound to, or null.
 */
function readPositionBindingFromAssetData(data) {
  return readAttributeFromAssetData(data, 'position');
}

export async function discoverPosition(connection, wallet) {
  resetPositionState();

  // Discovery strategy: scan PositionState and KeyState accounts from the hardig program
  // (small account set), then load specific MPL-Core assets by pubkey.
  // This avoids getProgramAccounts on MPL Core which most RPC providers reject.

  const POSITION_SIZE = 205; // PositionState account size (8+32+32+32+8+8+2+8+1+1+32+32+8+1)

  const [positionAccounts, keyStateAccounts] = await Promise.all([
    connection.getProgramAccounts(PROGRAM_ID, {
      filters: [{ dataSize: POSITION_SIZE }],
      commitment: 'confirmed',
    }),
    connection.getProgramAccounts(PROGRAM_ID, {
      filters: [{ dataSize: KEY_STATE_SIZE }],
      commitment: 'confirmed',
    }),
  ]);

  // Parse KeyState accounts: extract asset pubkey, rate-limit buckets, and authority_seed
  // Layout: discriminator(8) + authority_seed(32) + asset(32) + bump(1) + sell_bucket(32) + borrow_bucket(32)
  const keyStates = [];
  for (const { pubkey, account } of keyStateAccounts) {
    const data = account.data;
    if (data.length < KEY_STATE_SIZE) continue;
    const asset = new PublicKey(data.slice(40, 72));
    const buckets = parseKeyState(data);
    keyStates.push({ pubkey, asset, buckets, authoritySeed: buckets?.authoritySeed });
  }

  // Parse all PositionState accounts to get admin_asset pubkeys
  const positions = []; // { posPda, adminAsset }
  for (const { pubkey, account } of positionAccounts) {
    const data = account.data;
    if (data.length < POSITION_SIZE) continue;
    const adminAsset = new PublicKey(data.slice(8, 40));
    positions.push({ posPda: pubkey, adminAsset });
  }

  if (positions.length === 0 && keyStates.length === 0) {
    pushLog('No positions found on-chain.');
    return;
  }

  // Build lookup: admin_asset string -> posPda
  const adminAssetToPos = new Map();
  for (const p of positions) {
    adminAssetToPos.set(p.adminAsset.toString(), p.posPda);
  }

  // Step 1: Check admin keys — load each position's admin_asset and check owner
  const adminAssetPubkeys = positions.map((p) => p.adminAsset);
  const adminAssetInfos = await connection.getMultipleAccountsInfo(adminAssetPubkeys);

  let bestPos = null;
  let best = null;
  const allFound = []; // collect all wallet-accessible positions for multi-position UI

  function popcount(n) { let c = 0; while (n) { c += n & 1; n >>= 1; } return c; }

  for (let i = 0; i < positions.length; i++) {
    const info = adminAssetInfos[i];
    if (!info) continue;
    const parsed = parseMplCoreAsset(info.data);
    if (!parsed) continue;
    if (!parsed.owner.equals(wallet)) continue;

    const permissions = readPermissionsFromAssetData(info.data) ?? PRESET_ADMIN;
    const posPda = positions[i].posPda;

    allFound.push({ posPda, permissions, assetPubkey: adminAssetPubkeys[i], isAdmin: true, adminAsset: positions[i].adminAsset });

    let isBetter = !best;
    if (!isBetter) {
      const newPop = popcount(permissions);
      const oldPop = popcount(best.permissions);
      isBetter = newPop > oldPop
        || (newPop === oldPop && (permissions & PERM_MANAGE_KEYS) !== 0 && (best.permissions & PERM_MANAGE_KEYS) === 0);
    }
    if (isBetter) {
      bestPos = posPda;
      best = { permissions, assetPubkey: adminAssetPubkeys[i] };
    }
  }

  // Step 2: If no admin key found, check delegated keys via KeyState assets
  if (!bestPos && keyStates.length > 0) {
    const delegatedPubkeys = keyStates.map((ks) => ks.asset);
    const delegatedInfos = await connection.getMultipleAccountsInfo(delegatedPubkeys);

    for (let i = 0; i < keyStates.length; i++) {
      const info = delegatedInfos[i];
      if (!info) continue;
      const parsed = parseMplCoreAsset(info.data);
      if (!parsed) continue;
      if (!parsed.owner.equals(wallet)) continue;

      const permissions = readPermissionsFromAssetData(info.data);
      if (permissions === null) continue;
      const positionBinding = readPositionBindingFromAssetData(info.data);
      if (!positionBinding) continue;

      const posPda = adminAssetToPos.get(positionBinding);
      if (!posPda) continue;

      const bindingPk = new PublicKey(positionBinding);
      allFound.push({ posPda, permissions, assetPubkey: delegatedPubkeys[i], isAdmin: false, adminAsset: bindingPk });

      let isBetter = !best;
      if (!isBetter) {
        const newPop = popcount(permissions);
        const oldPop = popcount(best.permissions);
        isBetter = newPop > oldPop
          || (newPop === oldPop && (permissions & PERM_MANAGE_KEYS) !== 0 && (best.permissions & PERM_MANAGE_KEYS) === 0);
      }
      if (isBetter) {
        bestPos = posPda;
        best = { permissions, assetPubkey: delegatedPubkeys[i] };
      }
    }
  }

  if (!bestPos || !best) {
    pushLog('No position found for this wallet.');
    return;
  }

  positionPda.value = bestPos;
  myPermissions.value = best.permissions;
  myKeyAsset.value = best.assetPubkey;
  myNftMint.value = best.assetPubkey; // For backwards compat in UI (used in KeyringTable)

  // Load position account data
  try {
    const posInfo = await connection.getAccountInfo(bestPos);
    if (posInfo) {
      const data = posInfo.data;
      // Parse PositionState: discriminator(8) + admin_asset(32) + position_pda(32) + market_config(32)
      // + deposited_nav(8) + user_debt(8) + max_reinvest_spread_bps(2)
      // + last_admin_activity(8) + bump(1) + authority_bump(1)
      const view = new DataView(data.buffer, data.byteOffset);
      const adminAsset = new PublicKey(data.slice(8, 40));
      const mfPositionPda = new PublicKey(data.slice(40, 72));
      const mcPda = new PublicKey(data.slice(72, 104));
      const depositedNav = Number(view.getBigUint64(104, true));
      const userDebt = Number(view.getBigUint64(112, true));
      // bytes 120-121: max_reinvest_spread_bps (unused, skip)
      const lastAdminActivity = Number(view.getBigInt64(122, true));
      const bump = data[130];

      const posData = {
        adminAsset,
        authoritySeed: adminAsset, // alias: same bytes 8-40, used by derivePromoPda
        positionPda: mfPositionPda,
        marketConfig: mcPda,
        depositedNav,
        userDebt,
        lastAdminActivity,
        bump,
      };

      position.value = posData;
      mayflowerInitialized.value =
        !mfPositionPda.equals(PublicKey.default);

      // Fetch MarketConfig: from position if set, otherwise try default
      const mcToFetch = !mcPda.equals(PublicKey.default)
        ? mcPda
        : deriveMarketConfigPda(DEFAULT_NAV_SOL_MINT)[0];
      try {
        const mcInfo = await connection.getAccountInfo(mcToFetch);
        if (mcInfo && mcInfo.data.length >= 265) {
          const mcData = mcInfo.data;
          marketConfigPda.value = mcToFetch;
          marketConfig.value = {
            navMint: new PublicKey(mcData.slice(8, 40)),
            baseMint: new PublicKey(mcData.slice(40, 72)),
            marketGroup: new PublicKey(mcData.slice(72, 104)),
            marketMeta: new PublicKey(mcData.slice(104, 136)),
            mayflowerMarket: new PublicKey(mcData.slice(136, 168)),
            marketBaseVault: new PublicKey(mcData.slice(168, 200)),
            marketNavVault: new PublicKey(mcData.slice(200, 232)),
            feeVault: new PublicKey(mcData.slice(232, 264)),
          };
        }
      } catch (e) {
        pushLog('Failed to load MarketConfig: ' + e.message);
      }
    }
  } catch (e) {
    pushLog('Failed to load position: ' + e.message);
  }

  // Build keyring: admin key + all delegated keys for this position
  const posKeys = [];
  const posAdminAsset = position.value?.adminAsset;
  if (posAdminAsset) {
    const adminAssetStr = posAdminAsset.toString();

    // Build a map of asset pubkey -> KeyState data for bucket info
    const keyStateMap = new Map();
    for (const ks of keyStates) {
      keyStateMap.set(ks.asset.toString(), ks);
    }

    function attachBuckets(assetPubkey) {
      const ks = keyStateMap.get(assetPubkey.toString());
      if (!ks || !ks.buckets) return { sellBucket: null, borrowBucket: null };
      const { sellBucket, borrowBucket } = ks.buckets;
      return {
        sellBucket: sellBucket && sellBucket.capacity > 0 ? sellBucket : null,
        borrowBucket: borrowBucket && borrowBucket.capacity > 0 ? borrowBucket : null,
      };
    }

    // Admin key: we already loaded it above; re-read owner to check if signer holds it
    const adminIdx = adminAssetPubkeys.findIndex((pk) => pk.equals(posAdminAsset));
    if (adminIdx >= 0 && adminAssetInfos[adminIdx]) {
      const parsed = parseMplCoreAsset(adminAssetInfos[adminIdx].data);
      const held = parsed && parsed.owner.equals(wallet);
      posKeys.push({
        pda: null,
        mint: posAdminAsset,
        permissions: PRESET_ADMIN,
        heldBySigner: !!held,
      });
    }

    // Delegated keys: filter by authority_seed (on-chain field), then batch-load MPL-Core assets
    const positionKeyStates = keyStates.filter(
      (ks) => ks.authoritySeed && ks.authoritySeed.equals(posAdminAsset)
    );
    if (positionKeyStates.length > 0) {
      const delegatedPks = positionKeyStates.map((ks) => ks.asset);
      const delegatedInfos = await connection.getMultipleAccountsInfo(delegatedPks);
      for (let i = 0; i < positionKeyStates.length; i++) {
        const info = delegatedInfos[i];
        if (!info) continue;

        const permissions = readPermissionsFromAssetData(info.data);
        if (permissions === null) continue;

        const parsed = parseMplCoreAsset(info.data);
        const held = parsed && parsed.owner.equals(wallet);

        posKeys.push({
          pda: null,
          mint: delegatedPks[i],
          permissions,
          heldBySigner: !!held,
          ...attachBuckets(delegatedPks[i]),
        });
      }
    }
  }
  keyring.value = posKeys;

  // Populate discoveredPositions for multi-position UI
  const enriched = allFound.map((f) => ({
    ...f,
    depositedNav: f.posPda.equals(bestPos) ? (position.value?.depositedNav ?? 0) : 0,
  }));
  discoveredPositions.value = enriched;
  activePositionIndex.value = enriched.findIndex((f) => f.posPda.equals(bestPos));

  pushLog(
    `Found position ${shortPubkey(bestPos)} (permissions: ${permissionsName(best.permissions)}${
      mayflowerInitialized.value ? ', Nirvana OK' : ''
    })`
  );

  // Discover promos for the active position
  await discoverPromos(connection);
}

/**
 * Switch to a different discovered position by index.
 * Re-loads the position data and keyring for the selected position.
 */
export async function selectActivePosition(index, connection) {
  const positions = discoveredPositions.value;
  if (index < 0 || index >= positions.length) {
    throw new Error('Invalid position index');
  }
  const selected = positions[index];
  activePositionIndex.value = index;

  positionPda.value = selected.posPda;
  myPermissions.value = selected.permissions;
  myKeyAsset.value = selected.assetPubkey;
  myNftMint.value = selected.assetPubkey;

  // Re-load position account data
  const posInfo = await connection.getAccountInfo(selected.posPda);
  if (posInfo) {
    const data = posInfo.data;
    const view = new DataView(data.buffer, data.byteOffset);
    const adminAsset = new PublicKey(data.slice(8, 40));
    const mfPositionPda = new PublicKey(data.slice(40, 72));
    const mcPda = new PublicKey(data.slice(72, 104));
    const depositedNav = Number(view.getBigUint64(104, true));
    const userDebt = Number(view.getBigUint64(112, true));
    const lastAdminActivity = Number(view.getBigInt64(122, true));
    const bump = data[130];

    position.value = {
      adminAsset,
      authoritySeed: adminAsset,
      positionPda: mfPositionPda,
      marketConfig: mcPda,
      depositedNav,
      userDebt,
      lastAdminActivity,
      bump,
    };
    mayflowerInitialized.value = !mfPositionPda.equals(PublicKey.default);
  }

  // Re-discover promos for the newly selected position
  await discoverPromos(connection);
}

/**
 * Discover PromoConfig accounts for the active position's authority_seed.
 * PromoConfig layout (327 bytes total):
 *   discriminator(8) + authority_seed(32) + permissions(1) + borrow_capacity(8) +
 *   borrow_refill_period(8) + sell_capacity(8) + sell_refill_period(8) +
 *   min_deposit_lamports(8) + claims_count(4) + max_claims(4) + active(1) +
 *   name_suffix: String(4+max64) + image_uri: String(4+max128) + market_name: String(4+max32) + bump(1)
 */
const PROMO_CONFIG_SIZE = 327;

function parseBorshString(data, offset) {
  if (offset + 4 > data.length) return { value: '', bytesRead: 4 };
  const view = new DataView(data.buffer, data.byteOffset);
  const len = view.getUint32(offset, true);
  const end = offset + 4 + len;
  if (end > data.length) return { value: '', bytesRead: 4 };
  const value = new TextDecoder().decode(data.slice(offset + 4, end));
  return { value, bytesRead: 4 + len };
}

export async function discoverPromos(connection) {
  const pos = position.value;
  if (!pos || !pos.authoritySeed) {
    discoveredPromos.value = [];
    return;
  }

  try {
    const promoAccounts = await connection.getProgramAccounts(PROGRAM_ID, {
      filters: [
        { dataSize: PROMO_CONFIG_SIZE },
        { memcmp: { offset: 8, bytes: pos.authoritySeed.toBase58() } },
      ],
      commitment: 'confirmed',
    });

    const promos = [];
    for (const { pubkey, account } of promoAccounts) {
      const data = account.data;
      if (data.length < PROMO_CONFIG_SIZE) continue;

      const view = new DataView(data.buffer, data.byteOffset);
      let offset = 8; // skip discriminator

      const authoritySeed = new PublicKey(data.slice(offset, offset + 32));
      offset += 32;

      const permissions = data[offset];
      offset += 1;

      const borrowCapacity = Number(view.getBigUint64(offset, true));
      offset += 8;

      const borrowRefillPeriod = Number(view.getBigUint64(offset, true));
      offset += 8;

      const sellCapacity = Number(view.getBigUint64(offset, true));
      offset += 8;

      const sellRefillPeriod = Number(view.getBigUint64(offset, true));
      offset += 8;

      const minDepositLamports = Number(view.getBigUint64(offset, true));
      offset += 8;

      const claimsCount = view.getUint32(offset, true);
      offset += 4;

      const maxClaims = view.getUint32(offset, true);
      offset += 4;

      const active = data[offset] !== 0;
      offset += 1;

      const nameSuffixResult = parseBorshString(data, offset);
      const nameSuffix = nameSuffixResult.value;
      offset += nameSuffixResult.bytesRead;

      const imageUriResult = parseBorshString(data, offset);
      const imageUri = imageUriResult.value;
      offset += imageUriResult.bytesRead;

      const marketNameResult = parseBorshString(data, offset);
      const marketName = marketNameResult.value;
      offset += marketNameResult.bytesRead;

      const bump = data[offset];

      promos.push({
        pda: pubkey,
        config: {
          authoritySeed,
          permissions,
          borrowCapacity,
          borrowRefillPeriod,
          sellCapacity,
          sellRefillPeriod,
          minDepositLamports,
          claimsCount,
          maxClaims,
          active,
          nameSuffix,
          imageUri,
          marketName,
          bump,
        },
      });
    }

    discoveredPromos.value = promos;
    if (promos.length > 0) {
      pushLog(`Found ${promos.length} promo(s) for this position.`);
    }
  } catch (e) {
    pushLog('Promo discovery failed: ' + e.message);
    discoveredPromos.value = [];
  }
}
