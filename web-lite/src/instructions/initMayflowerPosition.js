import {
  deriveProgramPda,
  derivePersonalPosition,
  derivePersonalPositionEscrow,
  deriveLogAccount,
  deriveMarketConfigPda,
  DEFAULT_MARKET_META,
  DEFAULT_NAV_SOL_MINT,
  MAYFLOWER_PROGRAM_ID,
} from '../constants.js';
import { myKeyAsset, positionPda, position, marketConfigPda, marketConfig } from '../state.js';
import { shortPubkey } from '../utils.js';

export async function buildInitMayflowerPosition(program, wallet) {
  const adminKeyAsset = myKeyAsset.value;
  const posPda = positionPda.value;

  // Use loaded market config or derive default
  const mcPda = marketConfigPda.value || deriveMarketConfigPda(DEFAULT_NAV_SOL_MINT)[0];
  const mc = marketConfig.value;
  const marketMeta = mc ? mc.marketMeta : DEFAULT_MARKET_META;
  const navMint = mc ? mc.navMint : DEFAULT_NAV_SOL_MINT;

  const [programPda] = deriveProgramPda(position.value.adminAsset);
  const [ppPda] = derivePersonalPosition(programPda, marketMeta);
  const [escrowPda] = derivePersonalPositionEscrow(ppPda);
  const [logPda] = deriveLogAccount();

  const ix = await program.methods
    .initMayflowerPosition()
    .accounts({
      admin: wallet,
      adminKeyAsset: adminKeyAsset,
      position: posPda,
      marketConfig: mcPda,
      programPda: programPda,
      mayflowerPersonalPosition: ppPda,
      mayflowerUserShares: escrowPda,
      mayflowerMarketMeta: marketMeta,
      navSolMint: navMint,
      mayflowerLog: logPda,
      mayflowerProgram: MAYFLOWER_PROGRAM_ID,
    })
    .instruction();

  return {
    description: [
      'Init Mayflower Position',
      `PersonalPosition: ${shortPubkey(ppPda)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
