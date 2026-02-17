import { Keypair } from '@solana/web3.js';
import {
  derivePositionPda,
  deriveKeyAuthPda,
  deriveProgramPda,
  getAta,
} from '../constants.js';
import { shortPubkey } from '../utils.js';

export async function buildCreatePosition(program, wallet, maxSpreadBps) {
  const mintKp = Keypair.generate();
  const mint = mintKp.publicKey;
  const adminAta = getAta(wallet, mint);
  const [positionPda] = derivePositionPda(mint);
  const [keyAuthPda] = deriveKeyAuthPda(positionPda, mint);
  const [programPda] = deriveProgramPda();

  const ix = await program.methods
    .createPosition(maxSpreadBps)
    .accounts({
      admin: wallet,
      adminNftMint: mint,
      adminNftAta: adminAta,
      position: positionPda,
      adminKeyAuth: keyAuthPda,
      programPda: programPda,
    })
    .instruction();

  return {
    description: [
      'Create Position',
      `Admin NFT Mint: ${shortPubkey(mint)}`,
      `Position PDA: ${shortPubkey(positionPda)}`,
      `Max Reinvest Spread: ${maxSpreadBps} bps`,
    ],
    instructions: [ix],
    extraSigners: [mintKp],
  };
}
