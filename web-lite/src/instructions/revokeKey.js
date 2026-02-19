import {
  deriveKeyStatePda,
  deriveConfigPda,
  MPL_CORE_PROGRAM_ID,
} from '../constants.js';
import { myKeyAsset, positionPda, collection } from '../state.js';
import { shortPubkey, permissionsName } from '../utils.js';

export async function buildRevokeKey(program, wallet, targetKeyEntry) {
  const posPda = positionPda.value;
  const adminKeyAsset = myKeyAsset.value;
  const [configPda] = deriveConfigPda();
  const [targetKeyState] = deriveKeyStatePda(targetKeyEntry.mint);

  const ix = await program.methods
    .revokeKey()
    .accounts({
      admin: wallet,
      adminKeyAsset: adminKeyAsset,
      position: posPda,
      targetAsset: targetKeyEntry.mint,
      targetKeyState: targetKeyState,
      config: configPda,
      collection: collection.value,
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
