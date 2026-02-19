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

  // Discovery strategy: scan PositionNFT and KeyState accounts from the hardig program
  // (small account set), then load specific MPL-Core assets by pubkey.
  // This avoids getProgramAccounts on MPL Core which most RPC providers reject.

  const POSITION_SIZE = 132; // PositionNFT account size

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

  // Parse KeyState accounts: extract asset pubkey and rate-limit buckets
  const keyStates = [];
  for (const { pubkey, account } of keyStateAccounts) {
    const data = account.data;
    if (data.length < KEY_STATE_SIZE) continue;
    const asset = new PublicKey(data.slice(8, 40));
    const buckets = parseKeyState(data);
    keyStates.push({ pubkey, asset, buckets });
  }

  // Parse all PositionNFT accounts to get admin_asset pubkeys
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

  function popcount(n) { let c = 0; while (n) { c += n & 1; n >>= 1; } return c; }

  for (let i = 0; i < positions.length; i++) {
    const info = adminAssetInfos[i];
    if (!info) continue;
    const parsed = parseMplCoreAsset(info.data);
    if (!parsed) continue;
    if (!parsed.owner.equals(wallet)) continue;

    const permissions = readPermissionsFromAssetData(info.data) ?? PRESET_ADMIN;
    const posPda = positions[i].posPda;

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
      // Parse PositionNFT: discriminator(8) + admin_asset(32) + position_pda(32) + market_config(32)
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

    // Delegated keys: load all KeyState assets, check position binding
    for (const ks of keyStates) {
      let info;
      // Reuse already-loaded data if this key was loaded above (i.e., wallet held it)
      const delegIdx = keyStates.indexOf(ks);
      // We need to load it — delegated infos were only loaded in step 2 if bestPos wasn't found via admin.
      // Safest: just load them now.
      try {
        info = await connection.getAccountInfo(ks.asset);
      } catch { continue; }
      if (!info) continue;

      const permissions = readPermissionsFromAssetData(info.data);
      if (permissions === null) continue;
      const binding = readPositionBindingFromAssetData(info.data);
      if (binding !== adminAssetStr) continue;

      const parsed = parseMplCoreAsset(info.data);
      const held = parsed && parsed.owner.equals(wallet);

      posKeys.push({
        pda: null,
        mint: ks.asset,
        permissions,
        heldBySigner: !!held,
        ...attachBuckets(ks.asset),
      });
    }
  }
  keyring.value = posKeys;

  pushLog(
    `Found position ${shortPubkey(bestPos)} (permissions: ${permissionsName(best.permissions)}${
      mayflowerInitialized.value ? ', Nirvana OK' : ''
    })`
  );
}
