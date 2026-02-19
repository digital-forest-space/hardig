import { Keypair } from '@solana/web3.js';
import {
  derivePositionPda,
  deriveProgramPda,
  MPL_CORE_PROGRAM_ID,
} from '../constants.js';
import { shortPubkey } from '../utils.js';

export async function buildCreatePosition(program, wallet) {
  const assetKp = Keypair.generate();
  const adminAsset = assetKp.publicKey;
  const [positionPda] = derivePositionPda(adminAsset);
  const [programPda] = deriveProgramPda(adminAsset);

  const ix = await program.methods
    .createPosition(0)
    .accounts({
      admin: wallet,
      adminAsset: adminAsset,
      position: positionPda,
      programPda: programPda,
      mplCoreProgram: MPL_CORE_PROGRAM_ID,
    })
    .instruction();

  return {
    description: [
      'Create Position',
      `Admin Asset: ${shortPubkey(adminAsset)}`,
      `Position PDA: ${shortPubkey(positionPda)}`,
    ],
    instructions: [ix],
    extraSigners: [assetKp],
  };
}
