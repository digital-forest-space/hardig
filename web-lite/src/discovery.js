import { PublicKey } from '@solana/web3.js';
import {
  PROGRAM_ID,
  MPL_CORE_PROGRAM_ID,
  KEY_STATE_SIZE,
  derivePositionPda,
  deriveKeyStatePda,
  deriveMarketConfigPda,
  DEFAULT_NAV_SOL_MINT,
} from './constants.js';
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
import { shortPubkey, permissionsName, PERM_MANAGE_KEYS } from './utils.js';
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

  // Bytes 33: UpdateAuthority tag (1 = Address)
  const uaTag = data[33];
  if (uaTag !== 1 || data.length < 66) return null;
  const updateAuthority = new PublicKey(data.slice(34, 66));

  return { owner, updateAuthority };
}

/**
 * Read the permissions attribute from an MPL-Core asset's Attributes plugin.
 * This does a simple scan for the "permissions" key in the serialized plugin data.
 * Returns the permissions u8 value, or null if not found.
 */
function readPermissionsFromAssetData(data) {
  // Search for the "permissions" string in the asset data.
  // Attributes plugin stores attribute_list as borsh-serialized Vec<Attribute>.
  // Each Attribute has { key: String, value: String } where String is len(u32) + utf8.
  // We search for the byte pattern of the "permissions" key.
  const needle = 'permissions';
  const needleBytes = new TextEncoder().encode(needle);

  for (let i = 66; i < data.length - needleBytes.length - 8; i++) {
    // Look for u32 length prefix matching "permissions" length (11)
    const view = new DataView(data.buffer, data.byteOffset);
    if (i + 4 + needleBytes.length + 4 > data.length) break;

    const keyLen = view.getUint32(i, true);
    if (keyLen !== needleBytes.length) continue;

    // Check if the string matches
    let match = true;
    for (let j = 0; j < needleBytes.length; j++) {
      if (data[i + 4 + j] !== needleBytes[j]) {
        match = false;
        break;
      }
    }
    if (!match) continue;

    // Read the value string that follows
    const valueOffset = i + 4 + needleBytes.length;
    if (valueOffset + 4 > data.length) continue;
    const valueLen = view.getUint32(valueOffset, true);
    if (valueLen === 0 || valueLen > 3 || valueOffset + 4 + valueLen > data.length) continue;

    const valueStr = new TextDecoder().decode(data.slice(valueOffset + 4, valueOffset + 4 + valueLen));
    const parsed = parseInt(valueStr, 10);
    if (!isNaN(parsed) && parsed >= 0 && parsed <= 255) {
      return parsed;
    }
  }

  return null;
}

export async function discoverPosition(connection, wallet) {
  resetPositionState();

  // Fetch all KeyState accounts (used for delegated keys)
  const keyStateAccounts = await connection.getProgramAccounts(PROGRAM_ID, {
    filters: [{ dataSize: KEY_STATE_SIZE }],
    commitment: 'confirmed',
  });

  // Parse KeyState accounts: extract the asset pubkey
  const keyStates = [];
  for (const { pubkey, account } of keyStateAccounts) {
    const data = account.data;
    if (data.length < KEY_STATE_SIZE) continue;
    const asset = new PublicKey(data.slice(8, 40));
    keyStates.push({ pubkey, asset });
  }

  // Also discover admin keys by scanning MPL-Core assets owned by the wallet.
  // We need to find all MPL-Core assets where:
  //   1. owner == wallet
  //   2. update_authority is a Hardig program PDA
  //
  // Strategy: fetch all MPL-Core assets owned by this wallet, then check update_authority.
  const mplCoreAssets = await connection.getProgramAccounts(MPL_CORE_PROGRAM_ID, {
    filters: [
      { memcmp: { offset: 0, bytes: 'A' } }, // Key::AssetV1 = 1 (base58 'A' maps to 0x01 not right)
    ],
    commitment: 'confirmed',
  }).catch(() => []);

  // The above filter won't work well. Instead, let's use a different approach:
  // For each KeyState, load the asset and check if the wallet owns it.
  // For admin keys (which have no KeyState), we search differently.
  //
  // Better approach: Use getTokenAccountsByOwner equivalent for MPL-Core.
  // MPL-Core assets store owner at bytes 1-33. We can filter on that.
  const walletAssets = await connection.getProgramAccounts(MPL_CORE_PROGRAM_ID, {
    filters: [
      { memcmp: { offset: 1, bytes: wallet.toBase58() } },
    ],
    commitment: 'confirmed',
  }).catch(() => []);

  if (walletAssets.length === 0 && keyStates.length === 0) {
    pushLog('No positions found on-chain.');
    return;
  }

  // Parse all wallet-held MPL-Core assets
  const heldAssets = []; // { assetPubkey, updateAuthority, permissions }
  for (const { pubkey, account } of walletAssets) {
    const parsed = parseMplCoreAsset(account.data);
    if (!parsed) continue;
    const permissions = readPermissionsFromAssetData(account.data);
    if (permissions === null) continue; // Not a Hardig key NFT
    heldAssets.push({
      assetPubkey: pubkey,
      updateAuthority: parsed.updateAuthority,
      permissions,
    });
  }

  if (heldAssets.length === 0) {
    pushLog('No position found for this wallet.');
    return;
  }

  // For each held asset, try to find the position it belongs to.
  // The update_authority is the program_pda = [b"authority", admin_asset].
  // We need to find which admin_asset maps to this program_pda.
  // For admin keys: the asset itself IS the admin_asset, so position = [b"position", asset].
  // For delegated keys: the asset has a KeyState, and we need to find the position
  //   by checking all positions or by trying the admin_asset from the position data.
  //
  // Simplified approach: for each held asset, try deriving a position PDA assuming it's the admin key.
  // If that position exists on-chain, it's an admin key. Otherwise, check if there's a KeyState for it
  // and load the corresponding position.

  // Batch: try position PDAs for all held assets (to find admin keys)
  const positionPdaCandidates = heldAssets.map((a) => derivePositionPda(a.assetPubkey)[0]);
  const positionInfos = await connection.getMultipleAccountsInfo(positionPdaCandidates);

  // Build a map of update_authority -> admin_asset by checking found positions
  const uaToAdminAsset = new Map();

  // Also collect which held assets are admin keys vs delegated keys
  let bestPos = null;
  let best = null;

  function popcount(n) { let c = 0; while (n) { c += n & 1; n >>= 1; } return c; }

  for (let i = 0; i < heldAssets.length; i++) {
    const asset = heldAssets[i];
    const posInfo = positionInfos[i];

    if (posInfo) {
      // This is an admin key — the position PDA exists for this asset
      const posPda = positionPdaCandidates[i];
      let isBetter = !best;
      if (!isBetter) {
        const newPop = popcount(asset.permissions);
        const oldPop = popcount(best.permissions);
        isBetter = newPop > oldPop
          || (newPop === oldPop && (asset.permissions & PERM_MANAGE_KEYS) !== 0 && (best.permissions & PERM_MANAGE_KEYS) === 0);
      }
      if (isBetter) {
        bestPos = posPda;
        best = { permissions: asset.permissions, assetPubkey: asset.assetPubkey };
      }
      // Record the update_authority -> admin_asset mapping
      uaToAdminAsset.set(asset.updateAuthority.toString(), asset.assetPubkey);
    }
  }

  // For delegated keys (non-admin), find their position by loading the position account
  // that matches the update_authority.
  // If we haven't found a position yet via admin key, check delegated keys.
  if (!bestPos) {
    // For each held asset, check if its update_authority corresponds to a known position.
    // The update_authority is the program_pda = [b"authority", admin_asset].
    // We don't directly know the admin_asset, so we need to scan positions.
    // Alternatively, look at all PositionNFT accounts (size = 132) to find the matching one.
    const POSITION_SIZE = 132; // 8 + 32 + 32 + 32 + 8 + 8 + 2 + 8 + 1 + 1
    const positionAccounts = await connection.getProgramAccounts(PROGRAM_ID, {
      filters: [{ dataSize: POSITION_SIZE }],
      commitment: 'confirmed',
    });

    // Build a map from program_pda (authority) to position data
    const authToPosition = new Map();
    for (const { pubkey, account } of positionAccounts) {
      const data = account.data;
      if (data.length < POSITION_SIZE) continue;
      const adminAsset = new PublicKey(data.slice(8, 40));
      const [programPda] = PublicKey.findProgramAddressSync(
        [Buffer.from('authority'), adminAsset.toBuffer()],
        PROGRAM_ID
      );
      authToPosition.set(programPda.toString(), { posPda: pubkey, adminAsset });
    }

    for (const asset of heldAssets) {
      const posMatch = authToPosition.get(asset.updateAuthority.toString());
      if (!posMatch) continue;

      let isBetter = !best;
      if (!isBetter) {
        const newPop = popcount(asset.permissions);
        const oldPop = popcount(best.permissions);
        isBetter = newPop > oldPop
          || (newPop === oldPop && (asset.permissions & PERM_MANAGE_KEYS) !== 0 && (best.permissions & PERM_MANAGE_KEYS) === 0);
      }
      if (isBetter) {
        bestPos = posMatch.posPda;
        best = { permissions: asset.permissions, assetPubkey: asset.assetPubkey };
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

  // Load all keys for this position by finding all held assets with matching update_authority
  const posKeys = [];
  const posAdminAsset = position.value?.adminAsset;
  if (posAdminAsset) {
    const [expectedProgramPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('authority'), posAdminAsset.toBuffer()],
      PROGRAM_ID
    );

    for (const asset of heldAssets) {
      if (asset.updateAuthority.equals(expectedProgramPda)) {
        posKeys.push({
          pda: null, // No more KeyAuthorization PDA
          mint: asset.assetPubkey,
          permissions: asset.permissions,
          heldBySigner: true,
        });
      }
    }

    // Also include delegated keys (those with KeyState) that belong to this position
    // but are NOT held by the current wallet
    for (const ks of keyStates) {
      // Skip if already in heldAssets
      if (heldAssets.some((a) => a.assetPubkey.equals(ks.asset))) continue;

      // Load the asset to check its update_authority
      try {
        const assetInfo = await connection.getAccountInfo(ks.asset);
        if (!assetInfo) continue;
        const parsed = parseMplCoreAsset(assetInfo.data);
        if (!parsed) continue;
        if (!parsed.updateAuthority.equals(expectedProgramPda)) continue;
        const permissions = readPermissionsFromAssetData(assetInfo.data);
        if (permissions === null) continue;
        posKeys.push({
          pda: null,
          mint: ks.asset,
          permissions,
          heldBySigner: false,
        });
      } catch {
        // Skip assets we can't load
      }
    }
  }
  keyring.value = posKeys;

  pushLog(
    `Found position ${shortPubkey(bestPos)} (permissions: ${permissionsName(best.permissions)}${
      mayflowerInitialized.value ? ', Mayflower OK' : ''
    })`
  );
}
