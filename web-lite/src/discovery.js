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
  discoveredPositions,
  activePositionIndex,
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
  let nameOffset = 34; // tag 0 (None): no pubkey follows
  if ((uaTag === 1 || uaTag === 2) && data.length >= 66) {
    updateAuthority = new PublicKey(data.slice(34, 66));
    nameOffset = 66;
  } else if (uaTag !== 0) {
    return null; // unknown tag
  }

  // Name: borsh String (u32 length + utf8 bytes)
  let name = null;
  if (nameOffset + 4 <= data.length) {
    const view = new DataView(data.buffer, data.byteOffset);
    const nameLen = view.getUint32(nameOffset, true);
    if (nameLen > 0 && nameLen <= 200 && nameOffset + 4 + nameLen <= data.length) {
      name = new TextDecoder().decode(data.slice(nameOffset + 4, nameOffset + 4 + nameLen));
    }
  }

  return { owner, updateAuthority, name };
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

// Module-level cache from last discovery — used by selectActivePosition when
// switching positions without a full re-discovery.
let _lastDiscoveryCache = null;

export async function discoverPosition(connection, wallet) {
  resetPositionState();
  _lastDiscoveryCache = null;

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

  // Step 2: Also load ALL delegated asset infos for multi-position discovery
  const delegatedPubkeys = keyStates.map((ks) => ks.asset);
  const delegatedInfos = keyStates.length > 0
    ? await connection.getMultipleAccountsInfo(delegatedPubkeys)
    : [];

  // Collect ALL positions the wallet has access to
  const discovered = [];
  const seenPositions = new Set();

  // Admin positions
  for (let i = 0; i < positions.length; i++) {
    const info = adminAssetInfos[i];
    if (!info) continue;
    const parsed = parseMplCoreAsset(info.data);
    if (!parsed) continue;
    if (!parsed.owner.equals(wallet)) continue;

    const permissions = readPermissionsFromAssetData(info.data) ?? PRESET_ADMIN;
    const posPda = positions[i].posPda;
    const posKey = posPda.toString();
    if (seenPositions.has(posKey)) continue;
    seenPositions.add(posKey);

    // Read deposited_nav and user_debt from position account
    const posAcc = positionAccounts.find(({ pubkey }) => pubkey.equals(posPda));
    let depositedNav = 0, userDebt = 0;
    if (posAcc) {
      const view = new DataView(posAcc.account.data.buffer, posAcc.account.data.byteOffset);
      depositedNav = Number(view.getBigUint64(104, true));
      userDebt = Number(view.getBigUint64(112, true));
    }

    discovered.push({
      posPda,
      adminAsset: adminAssetPubkeys[i],
      permissions,
      keyAsset: adminAssetPubkeys[i],
      keyStatePda: null,
      depositedNav,
      userDebt,
      isAdmin: true,
    });
  }

  // Delegated positions
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
    const posKey = posPda.toString();
    if (seenPositions.has(posKey)) continue;
    seenPositions.add(posKey);

    const posAcc = positionAccounts.find(({ pubkey }) => pubkey.equals(posPda));
    let depositedNav = 0, userDebt = 0;
    if (posAcc) {
      const view = new DataView(posAcc.account.data.buffer, posAcc.account.data.byteOffset);
      depositedNav = Number(view.getBigUint64(104, true));
      userDebt = Number(view.getBigUint64(112, true));
    }

    discovered.push({
      posPda,
      adminAsset: new PublicKey(positionBinding),
      permissions,
      keyAsset: delegatedPubkeys[i],
      keyStatePda: keyStates[i].pubkey,
      depositedNav,
      userDebt,
      isAdmin: false,
    });
  }

  // Sort: admin positions first, then by deposited_nav descending
  discovered.sort((a, b) => {
    if (a.isAdmin !== b.isAdmin) return a.isAdmin ? -1 : 1;
    return b.depositedNav - a.depositedNav;
  });

  discoveredPositions.value = discovered;

  if (discovered.length === 0) {
    pushLog('No position found for this wallet.');
    return;
  }

  // Store cache for position switching without re-discovery
  _lastDiscoveryCache = {
    positions, keyStates, adminAssetPubkeys, adminAssetInfos, delegatedPubkeys, delegatedInfos,
  };

  // Auto-select the first position
  await selectActivePosition(0, connection, _lastDiscoveryCache);

  pushLog(
    `Found ${discovered.length} position(s).`
  );
}

/**
 * Select a specific discovered position and load its full state.
 * Exported so UI components can call it when switching positions.
 */
export async function selectActivePosition(index, connection, cache) {
  const dp = discoveredPositions.value[index];
  if (!dp) return;

  // Use provided cache or fall back to module-level cache from last discovery
  const effectiveCache = cache || _lastDiscoveryCache;
  if (!effectiveCache) {
    pushLog('No discovery cache available — please refresh first.');
    return;
  }

  activePositionIndex.value = index;
  positionPda.value = dp.posPda;
  myPermissions.value = dp.permissions;
  myKeyAsset.value = dp.keyAsset;
  myNftMint.value = dp.keyAsset;

  const { positions, keyStates, adminAssetPubkeys, adminAssetInfos, delegatedPubkeys, delegatedInfos } = effectiveCache;

  // Load position account data
  try {
    const posInfo = await connection.getAccountInfo(dp.posPda);
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

      // Fetch MarketConfig
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

    // Admin key
    const adminIdx = adminAssetPubkeys.findIndex((pk) => pk.equals(posAdminAsset));
    if (adminIdx >= 0 && adminAssetInfos[adminIdx]) {
      const parsed = parseMplCoreAsset(adminAssetInfos[adminIdx].data);
      posKeys.push({
        pda: null,
        mint: posAdminAsset,
        permissions: PRESET_ADMIN,
        heldBySigner: dp.isAdmin,
        name: parsed?.name || null,
      });
    }

    // Delegated keys: use cached delegatedInfos when available, fall back to RPC
    for (let i = 0; i < keyStates.length; i++) {
      let info = delegatedInfos[i];
      if (!info) {
        try { info = await connection.getAccountInfo(keyStates[i].asset); }
        catch { continue; }
      }
      if (!info) continue;

      const permissions = readPermissionsFromAssetData(info.data);
      if (permissions === null) continue;
      const binding = readPositionBindingFromAssetData(info.data);
      if (binding !== adminAssetStr) continue;

      const parsed = parseMplCoreAsset(info.data);
      // Check if this delegated key's asset is the one we discovered for this position
      const isOurKey = keyStates[i].asset.equals(dp.keyAsset);

      posKeys.push({
        pda: null,
        mint: keyStates[i].asset,
        permissions,
        heldBySigner: isOurKey && !dp.isAdmin,
        name: parsed?.name || null,
        ...attachBuckets(keyStates[i].asset),
      });
    }
  }
  keyring.value = posKeys;
}
