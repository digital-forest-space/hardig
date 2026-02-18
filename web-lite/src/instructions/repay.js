import { SystemProgram } from '@solana/web3.js';
import { createSyncNativeInstruction } from '@solana/spl-token';
import { BN } from '@coral-xyz/anchor';
import {
  deriveProgramPda,
  derivePersonalPosition,
  deriveLogAccount,
  getAta,
  MAYFLOWER_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  DEFAULT_WSOL_MINT,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda, position, marketConfigPda, marketConfig } from '../state.js';
import { shortPubkey, lamportsToSol } from '../utils.js';

export async function buildRepay(program, wallet, amountLamports) {
  const nftMint = myNftMint.value;
  const keyAuth = myKeyAuthPda.value;
  const posPda = positionPda.value;
  const nftAta = getAta(wallet, nftMint);
  const mc = marketConfig.value;
  const mcPda = marketConfigPda.value;
  const baseMint = mc ? mc.baseMint : DEFAULT_WSOL_MINT;
  const marketMeta = mc ? mc.marketMeta : undefined;
  const [programPda] = deriveProgramPda(position.value.adminNftMint);
  const [ppPda] = derivePersonalPosition(programPda, marketMeta);
  const [logAccount] = deriveLogAccount();
  const wsolAta = getAta(programPda, baseMint);

  // Pre-IXs: wrap SOL + sync native
  const transferIx = SystemProgram.transfer({
    fromPubkey: wallet,
    toPubkey: wsolAta,
    lamports: amountLamports,
  });
  const syncIx = createSyncNativeInstruction(wsolAta, TOKEN_PROGRAM_ID);

  const repayIx = await program.methods
    .repay(new BN(amountLamports))
    .accounts({
      signer: wallet,
      keyNftAta: nftAta,
      keyAuth: keyAuth,
      position: posPda,
      marketConfig: mcPda,
      programPda: programPda,
      personalPosition: ppPda,
      userBaseTokenAta: wsolAta,
      marketMeta: mc.marketMeta,
      marketBaseVault: mc.marketBaseVault,
      wsolMint: mc.baseMint,
      mayflowerMarket: mc.mayflowerMarket,
      mayflowerProgram: MAYFLOWER_PROGRAM_ID,
      logAccount: logAccount,
    })
    .instruction();

  return {
    description: [
      'Repay',
      `Amount: ${lamportsToSol(amountLamports)} SOL`,
      `Position: ${shortPubkey(posPda)}`,
    ],
    instructions: [transferIx, syncIx, repayIx],
    extraSigners: [],
  };
}
