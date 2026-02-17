import { PublicKey } from '@solana/web3.js';
import {
  PROGRAM_ID,
  KEY_AUTH_SIZE,
  getAta,
  derivePositionPda,
  deriveMarketConfigPda,
  DEFAULT_NAV_SOL_MINT,
} from './constants.js';
import {
  positionPda,
  position,
  myRole,
  myKeyAuthPda,
  myNftMint,
  keyring,
  protocolExists,
  mayflowerInitialized,
  marketConfigPda,
  marketConfig,
  pushLog,
  resetPositionState,
} from './state.js';
import { shortPubkey, roleName } from './utils.js';
import { deriveConfigPda } from './constants.js';

export async function checkProtocol(connection) {
  try {
    const [configPda] = deriveConfigPda();
    const info = await connection.getAccountInfo(configPda);
    protocolExists.value = info !== null;
  } catch (e) {
    protocolExists.value = false;
  }
}

// Read a token balance from an ATA (u64 LE at offset 64)
async function readTokenBalance(connection, ata) {
  try {
    const info = await connection.getAccountInfo(ata);
    if (!info || info.data.length < 72) return null;
    const view = new DataView(info.data.buffer, info.data.byteOffset);
    return Number(view.getBigUint64(64, true));
  } catch {
    return null;
  }
}

function checkHoldsNft(balanceMap, wallet, mint) {
  const ata = getAta(wallet, mint);
  return balanceMap.get(ata.toString()) === 1;
}

export async function discoverPosition(connection, wallet) {
  resetPositionState();

  // Fetch all KeyAuthorization accounts
  const accounts = await connection.getProgramAccounts(PROGRAM_ID, {
    filters: [{ dataSize: KEY_AUTH_SIZE }],
    commitment: 'confirmed',
  });

  if (accounts.length === 0) {
    pushLog('No positions found on-chain.');
    return;
  }

  // Parse all KeyAuthorizations
  const keyAuths = [];
  for (const { pubkey, account } of accounts) {
    const data = account.data;
    if (data.length < KEY_AUTH_SIZE) continue;
    const posKey = new PublicKey(data.slice(8, 40));
    const nftMint = new PublicKey(data.slice(40, 72));
    const role = data[72];
    keyAuths.push({ pubkey, position: posKey, nftMint, role });
  }

  // Batch-check which NFTs the wallet holds
  const atasToCheck = keyAuths.map((ka) => getAta(wallet, ka.nftMint));
  const balanceMap = new Map();

  // Fetch ATAs in batches of 100
  for (let i = 0; i < atasToCheck.length; i += 100) {
    const batch = atasToCheck.slice(i, i + 100);
    const infos = await connection.getMultipleAccountsInfo(batch);
    for (let j = 0; j < batch.length; j++) {
      const info = infos[j];
      if (info && info.data.length >= 72) {
        const view = new DataView(info.data.buffer, info.data.byteOffset);
        balanceMap.set(batch[j].toString(), Number(view.getBigUint64(64, true)));
      }
    }
  }

  // Find best key (lowest role number) held by the wallet
  let bestPos = null;
  let best = null;

  for (const ka of keyAuths) {
    if (checkHoldsNft(balanceMap, wallet, ka.nftMint)) {
      const isBetter = !best || ka.role < best.role;
      if (isBetter) {
        bestPos = ka.position;
        best = { role: ka.role, pubkey: ka.pubkey, nftMint: ka.nftMint };
      }
    }
  }

  if (!bestPos || !best) {
    pushLog('No position found for this wallet.');
    return;
  }

  positionPda.value = bestPos;
  myRole.value = best.role;
  myKeyAuthPda.value = best.pubkey;
  myNftMint.value = best.nftMint;

  // Load position account data via Anchor deserialization
  try {
    const posInfo = await connection.getAccountInfo(bestPos);
    if (posInfo) {
      const data = posInfo.data;
      // Parse PositionNFT: discriminator(8) + admin_nft_mint(32) + position_pda(32) + market_config(32)
      // + deposited_nav(8) + user_debt(8) + protocol_debt(8) + max_reinvest_spread_bps(2)
      // + last_admin_activity(8) + bump(1) + authority_bump(1)
      const view = new DataView(data.buffer, data.byteOffset);
      const adminNftMint = new PublicKey(data.slice(8, 40));
      const mfPositionPda = new PublicKey(data.slice(40, 72));
      const mcPda = new PublicKey(data.slice(72, 104));
      const depositedNav = Number(view.getBigUint64(104, true));
      const userDebt = Number(view.getBigUint64(112, true));
      const protocolDebt = Number(view.getBigUint64(120, true));
      // bytes 128-129: max_reinvest_spread_bps (unused, skip)
      const lastAdminActivity = Number(view.getBigInt64(130, true));
      const bump = data[138];

      const posData = {
        adminNftMint,
        positionPda: mfPositionPda,
        marketConfig: mcPda,
        depositedNav,
        userDebt,
        protocolDebt,
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

  // Load all keys for this position
  const posKeys = [];
  for (const ka of keyAuths) {
    if (ka.position.equals(bestPos)) {
      posKeys.push({
        pda: ka.pubkey,
        mint: ka.nftMint,
        role: ka.role,
        heldBySigner: checkHoldsNft(balanceMap, wallet, ka.nftMint),
      });
    }
  }
  keyring.value = posKeys;

  pushLog(
    `Found position ${shortPubkey(bestPos)} (role: ${roleName(best.role)}${
      mayflowerInitialized.value ? ', Mayflower OK' : ''
    })`
  );
}
