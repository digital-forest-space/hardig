import {
  deriveProgramPda,
  derivePersonalPosition,
  derivePersonalPositionEscrow,
  deriveLogAccount,
  getAta,
  MARKET_META,
  NAV_SOL_MINT,
  MAYFLOWER_PROGRAM_ID,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda } from '../state.js';
import { shortPubkey } from '../utils.js';

export async function buildInitMayflowerPosition(program, wallet) {
  const nftMint = myNftMint.value;
  const keyAuth = myKeyAuthPda.value;
  const posPda = positionPda.value;
  const nftAta = getAta(wallet, nftMint);
  const [programPda] = deriveProgramPda();
  const [ppPda] = derivePersonalPosition(programPda);
  const [escrowPda] = derivePersonalPositionEscrow(ppPda);
  const [logPda] = deriveLogAccount();

  const ix = await program.methods
    .initMayflowerPosition()
    .accounts({
      admin: wallet,
      adminNftAta: nftAta,
      adminKeyAuth: keyAuth,
      position: posPda,
      programPda: programPda,
      mayflowerPersonalPosition: ppPda,
      mayflowerUserShares: escrowPda,
      mayflowerMarketMeta: MARKET_META,
      navSolMint: NAV_SOL_MINT,
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
