import {
  getAta,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda } from '../state.js';
import { shortPubkey, permissionsName } from '../utils.js';

export async function buildRevokeKey(program, wallet, targetKeyEntry) {
  const posPda = positionPda.value;
  const adminNftMint = myNftMint.value;
  const adminKeyAuth = myKeyAuthPda.value;
  const adminNftAta = getAta(wallet, adminNftMint);

  const ix = await program.methods
    .revokeKey()
    .accounts({
      admin: wallet,
      adminNftAta: adminNftAta,
      adminKeyAuth: adminKeyAuth,
      position: posPda,
      targetKeyAuth: targetKeyEntry.pda,
    })
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
