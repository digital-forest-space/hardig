import { Keypair } from '@solana/web3.js';
import {
  derivePositionPda,
  deriveKeyAuthPda,
  deriveProgramPda,
  deriveMetadataPda,
  deriveMasterEditionPda,
  getAta,
  METADATA_PROGRAM_ID,
  RENT_SYSVAR,
} from '../constants.js';
import { shortPubkey } from '../utils.js';

export async function buildCreatePosition(program, wallet) {
  const mintKp = Keypair.generate();
  const mint = mintKp.publicKey;
  const adminAta = getAta(wallet, mint);
  const [positionPda] = derivePositionPda(mint);
  const [keyAuthPda] = deriveKeyAuthPda(positionPda, mint);
  const [programPda] = deriveProgramPda(mint);
  const metadata = deriveMetadataPda(mint);
  const masterEdition = deriveMasterEditionPda(mint);

  const ix = await program.methods
    .createPosition(0)
    .accounts({
      admin: wallet,
      adminNftMint: mint,
      adminNftAta: adminAta,
      position: positionPda,
      adminKeyAuth: keyAuthPda,
      programPda: programPda,
      metadata: metadata,
      masterEdition: masterEdition,
      tokenMetadataProgram: METADATA_PROGRAM_ID,
      rent: RENT_SYSVAR,
    })
    .instruction();

  return {
    description: [
      'Create Position',
      `Admin NFT Mint: ${shortPubkey(mint)}`,
      `Position PDA: ${shortPubkey(positionPda)}`,
    ],
    instructions: [ix],
    extraSigners: [mintKp],
  };
}
