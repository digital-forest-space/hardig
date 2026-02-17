import { Keypair, PublicKey } from '@solana/web3.js';
import {
  deriveKeyAuthPda,
  deriveProgramPda,
  getAta,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda } from '../state.js';
import { shortPubkey, roleName } from '../utils.js';

export async function buildAuthorizeKey(program, wallet, targetWalletStr, roleU8) {
  const targetWallet = new PublicKey(targetWalletStr);
  const posPda = positionPda.value;
  const adminNftMint = myNftMint.value;
  const adminKeyAuth = myKeyAuthPda.value;
  const adminNftAta = getAta(wallet, adminNftMint);
  const [programPda] = deriveProgramPda();

  const mintKp = Keypair.generate();
  const newMint = mintKp.publicKey;
  const targetAta = getAta(targetWallet, newMint);
  const [newKeyAuth] = deriveKeyAuthPda(posPda, newMint);

  const ix = await program.methods
    .authorizeKey(roleU8)
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
      `Role: ${roleName(roleU8)} (${roleU8})`,
      `Key NFT Mint: ${shortPubkey(newMint)}`,
    ],
    instructions: [ix],
    extraSigners: [mintKp],
  };
}
