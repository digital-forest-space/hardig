import { Keypair, ComputeBudgetProgram } from '@solana/web3.js';
import {
  derivePositionPda,
  deriveProgramPda,
  deriveConfigPda,
  deriveMarketConfigPda,
  derivePersonalPosition,
  derivePersonalPositionEscrow,
  deriveLogAccount,
  MPL_CORE_PROGRAM_ID,
  MAYFLOWER_PROGRAM_ID,
  DEFAULT_NAV_SOL_MINT,
  DEFAULT_MARKET_META,
} from '../constants.js';
import { collection, marketConfig, marketConfigPda } from '../state.js';
import { shortPubkey, navTokenName } from '../utils.js';

/**
 * @param {object} program - Anchor program instance
 * @param {PublicKey} wallet - Payer/admin wallet
 * @param {string|null} name - Optional label suffix
 * @param {object|null} marketEntry - Optional market entry from marketEntryToPubkeys()
 *   { navMint, baseMint, marketGroup, marketMeta, mayflowerMarket, marketBaseVault, marketNavVault, feeVault, navSymbol }
 */
export async function buildCreatePosition(program, wallet, name = null, marketEntry = null) {
  const assetKp = Keypair.generate();
  const adminAsset = assetKp.publicKey;
  const [positionPda] = derivePositionPda(adminAsset);
  const [programPda] = deriveProgramPda(adminAsset);
  const [configPda] = deriveConfigPda();

  // Use market entry if provided, otherwise fall back to loaded market config or default
  let mcPda, marketMeta, navMint, marketName;
  if (marketEntry) {
    [mcPda] = deriveMarketConfigPda(marketEntry.navMint);
    marketMeta = marketEntry.marketMeta;
    navMint = marketEntry.navMint;
    marketName = marketEntry.navSymbol;
  } else {
    mcPda = marketConfigPda.value || deriveMarketConfigPda(DEFAULT_NAV_SOL_MINT)[0];
    const mc = marketConfig.value;
    marketMeta = mc ? mc.marketMeta : DEFAULT_MARKET_META;
    navMint = mc ? mc.navMint : DEFAULT_NAV_SOL_MINT;
    marketName = navTokenName(navMint);
  }

  const [ppPda] = derivePersonalPosition(programPda, marketMeta);
  const [escrowPda] = derivePersonalPositionEscrow(ppPda);
  const [logPda] = deriveLogAccount();

  const ix = await program.methods
    .createPosition(name, marketName)
    .accounts({
      admin: wallet,
      adminAsset: adminAsset,
      position: positionPda,
      programPda: programPda,
      config: configPda,
      collection: collection.value,
      marketConfig: mcPda,
      mplCoreProgram: MPL_CORE_PROGRAM_ID,
      mayflowerPersonalPosition: ppPda,
      mayflowerUserShares: escrowPda,
      mayflowerMarketMeta: marketMeta,
      navSolMint: navMint,
      mayflowerLog: logPda,
      mayflowerProgram: MAYFLOWER_PROGRAM_ID,
    })
    .instruction();

  // MPL-Core CreateV2 + Mayflower init_personal_position need extra compute
  const computeIx = ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 });

  return {
    description: [
      'Create Position',
      `Market: ${marketName}`,
      `Admin Asset: ${shortPubkey(adminAsset)}`,
      `Position PDA: ${shortPubkey(positionPda)}`,
    ],
    instructions: [computeIx, ix],
    extraSigners: [assetKp],
  };
}
