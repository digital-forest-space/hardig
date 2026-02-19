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
import { shortPubkey } from '../utils.js';

export async function buildCreatePosition(program, wallet) {
  const assetKp = Keypair.generate();
  const adminAsset = assetKp.publicKey;
  const [positionPda] = derivePositionPda(adminAsset);
  const [programPda] = deriveProgramPda(adminAsset);
  const [configPda] = deriveConfigPda();

  // Use loaded market config or derive default
  const mcPda = marketConfigPda.value || deriveMarketConfigPda(DEFAULT_NAV_SOL_MINT)[0];
  const mc = marketConfig.value;
  const marketMeta = mc ? mc.marketMeta : DEFAULT_MARKET_META;
  const navMint = mc ? mc.navMint : DEFAULT_NAV_SOL_MINT;

  const [ppPda] = derivePersonalPosition(programPda, marketMeta);
  const [escrowPda] = derivePersonalPositionEscrow(ppPda);
  const [logPda] = deriveLogAccount();

  const ix = await program.methods
    .createPosition(0, null)
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
      `Admin Asset: ${shortPubkey(adminAsset)}`,
      `Position PDA: ${shortPubkey(positionPda)}`,
    ],
    instructions: [computeIx, ix],
    extraSigners: [assetKp],
  };
}
