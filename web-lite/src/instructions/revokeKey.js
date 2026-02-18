import {
  getAta,
  deriveMetadataPda,
  deriveMasterEditionPda,
  METADATA_PROGRAM_ID,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda } from '../state.js';
import { shortPubkey, permissionsName } from '../utils.js';

export async function buildRevokeKey(program, wallet, targetKeyEntry) {
  const posPda = positionPda.value;
  const adminNftMint = myNftMint.value;
  const adminKeyAuth = myKeyAuthPda.value;
  const adminNftAta = getAta(wallet, adminNftMint);

  // If admin holds the target NFT, include metadata accounts for Metaplex burn
  const accounts = {
    admin: wallet,
    adminNftAta: adminNftAta,
    adminKeyAuth: adminKeyAuth,
    position: posPda,
    targetKeyAuth: targetKeyEntry.pda,
    targetNftMint: targetKeyEntry.mint,
  };

  if (targetKeyEntry.heldBySigner) {
    accounts.targetNftAta = getAta(wallet, targetKeyEntry.mint);
    accounts.metadata = deriveMetadataPda(targetKeyEntry.mint);
    accounts.masterEdition = deriveMasterEditionPda(targetKeyEntry.mint);
    accounts.tokenMetadataProgram = METADATA_PROGRAM_ID;
  }

  const ix = await program.methods
    .revokeKey()
    .accounts(accounts)
    .instruction();

  return {
    description: [
      'Revoke Key',
      `Key Mint: ${shortPubkey(targetKeyEntry.mint)}`,
      `Permissions: ${permissionsName(targetKeyEntry.permissions)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
