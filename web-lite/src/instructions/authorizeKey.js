import { Keypair, PublicKey } from '@solana/web3.js';
import { BN } from '@coral-xyz/anchor';
import {
  deriveKeyAuthPda,
  deriveProgramPda,
  getAta,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda, position } from '../state.js';
import { shortPubkey, permissionsName } from '../utils.js';

export async function buildAuthorizeKey(program, wallet, targetWalletStr, permissionsU8, sellCapacity = 0, sellRefillSlots = 0, borrowCapacity = 0, borrowRefillSlots = 0) {
  const targetWallet = new PublicKey(targetWalletStr);
  const posPda = positionPda.value;
  const adminNftMint = myNftMint.value;
  const adminKeyAuth = myKeyAuthPda.value;
  const adminNftAta = getAta(wallet, adminNftMint);
  const [programPda] = deriveProgramPda(position.value.adminNftMint);

  const mintKp = Keypair.generate();
  const newMint = mintKp.publicKey;
  const targetAta = getAta(targetWallet, newMint);
  const [newKeyAuth] = deriveKeyAuthPda(posPda, newMint);

  const ix = await program.methods
    .authorizeKey(permissionsU8, new BN(sellCapacity), new BN(sellRefillSlots), new BN(borrowCapacity), new BN(borrowRefillSlots))
    .accounts({
      admin: wallet,
      adminNftAta: adminNftAta,
      adminKeyAuth: adminKeyAuth,
      position: posPda,
      newKeyMint: newMint,
      targetNftAta: targetAta,
      targetWallet: targetWallet,
      newKeyAuth: newKeyAuth,
      programPda: programPda,
    })
    .instruction();

  return {
    description: [
      'Authorize Key',
      `Target: ${shortPubkey(targetWallet)}`,
      `Permissions: ${permissionsName(permissionsU8)} (0x${permissionsU8.toString(16).padStart(2, '0')})`,
      `Key NFT Mint: ${shortPubkey(newMint)}`,
    ],
    instructions: [ix],
    extraSigners: [mintKp],
  };
}
