import {
  deriveKeyStatePda,
  deriveProgramPda,
  MPL_CORE_PROGRAM_ID,
} from '../constants.js';
import { myKeyAsset, positionPda, position } from '../state.js';
import { shortPubkey, permissionsName } from '../utils.js';

export async function buildRevokeKey(program, wallet, targetKeyEntry) {
  const posPda = positionPda.value;
  const adminKeyAsset = myKeyAsset.value;
  const [programPda] = deriveProgramPda(position.value.adminAsset);
  const [targetKeyState] = deriveKeyStatePda(targetKeyEntry.mint);

  const ix = await program.methods
    .revokeKey()
    .accounts({
      admin: wallet,
      adminKeyAsset: adminKeyAsset,
      position: posPda,
      targetAsset: targetKeyEntry.mint,
      targetKeyState: targetKeyState,
      programPda: programPda,
      mplCoreProgram: MPL_CORE_PROGRAM_ID,
    })
    .instruction();

  return {
    description: [
      'Revoke Key',
      `Key Asset: ${shortPubkey(targetKeyEntry.mint)}`,
      `Permissions: ${permissionsName(targetKeyEntry.permissions)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
