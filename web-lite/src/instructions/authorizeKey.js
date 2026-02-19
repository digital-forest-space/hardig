import { Keypair, PublicKey } from '@solana/web3.js';
import { BN } from '@coral-xyz/anchor';
import {
  deriveKeyStatePda,
  deriveConfigPda,
  MPL_CORE_PROGRAM_ID,
} from '../constants.js';
import { myKeyAsset, positionPda, collection } from '../state.js';
import { shortPubkey, permissionsName } from '../utils.js';

export async function buildAuthorizeKey(program, wallet, targetWalletStr, permissionsU8, sellCapacity = 0, sellRefillSlots = 0, borrowCapacity = 0, borrowRefillSlots = 0) {
  const targetWallet = new PublicKey(targetWalletStr);
  const posPda = positionPda.value;
  const adminKeyAsset = myKeyAsset.value;
  const [configPda] = deriveConfigPda();

  const assetKp = Keypair.generate();
  const newKeyAsset = assetKp.publicKey;
  const [keyStatePda] = deriveKeyStatePda(newKeyAsset);

  const ix = await program.methods
    .authorizeKey(permissionsU8, new BN(sellCapacity), new BN(sellRefillSlots), new BN(borrowCapacity), new BN(borrowRefillSlots), null)
    .accounts({
      admin: wallet,
      adminKeyAsset: adminKeyAsset,
      position: posPda,
      newKeyAsset: newKeyAsset,
      targetWallet: targetWallet,
      keyState: keyStatePda,
      config: configPda,
      collection: collection.value,
      mplCoreProgram: MPL_CORE_PROGRAM_ID,
    })
    .instruction();

  return {
    description: [
      'Authorize Key',
      `Target: ${shortPubkey(targetWallet)}`,
      `Permissions: ${permissionsName(permissionsU8)} (0x${permissionsU8.toString(16).padStart(2, '0')})`,
      `Key Asset: ${shortPubkey(newKeyAsset)}`,
    ],
    instructions: [ix],
    extraSigners: [assetKp],
  };
}
