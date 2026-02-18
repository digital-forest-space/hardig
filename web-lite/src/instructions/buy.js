import { SystemProgram } from '@solana/web3.js';
import { createSyncNativeInstruction } from '@solana/spl-token';
import { BN } from '@coral-xyz/anchor';
import {
  deriveProgramPda,
  derivePersonalPosition,
  derivePersonalPositionEscrow,
  deriveLogAccount,
  getAta,
  MAYFLOWER_TENANT,
  MAYFLOWER_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  DEFAULT_WSOL_MINT,
  DEFAULT_NAV_SOL_MINT,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda, myRole, position, marketConfigPda, marketConfig } from '../state.js';
import { shortPubkey, lamportsToSol, roleName } from '../utils.js';

export async function buildBuy(program, wallet, amountLamports) {
  const nftMint = myNftMint.value;
  const keyAuth = myKeyAuthPda.value;
  const posPda = positionPda.value;
  const nftAta = getAta(wallet, nftMint);
  const mc = marketConfig.value;
  const mcPda = marketConfigPda.value;
  const baseMint = mc ? mc.baseMint : DEFAULT_WSOL_MINT;
  const navMint = mc ? mc.navMint : DEFAULT_NAV_SOL_MINT;
  const marketMeta = mc ? mc.marketMeta : undefined;
  const [programPda] = deriveProgramPda(position.value.adminNftMint);
  const [ppPda] = derivePersonalPosition(programPda, marketMeta);
  const [escrowPda] = derivePersonalPositionEscrow(ppPda);
  const [logPda] = deriveLogAccount();
  const wsolAta = getAta(programPda, baseMint);
  const navAta = getAta(programPda, navMint);

  // Pre-IXs: wrap SOL + sync native
  const transferIx = SystemProgram.transfer({
    fromPubkey: wallet,
    toPubkey: wsolAta,
    lamports: amountLamports,
  });
  const syncIx = createSyncNativeInstruction(wsolAta, TOKEN_PROGRAM_ID);

  const buyIx = await program.methods
    .buy(new BN(amountLamports), new BN(0)) // min_out = 0 (no slippage protection)
    .accounts({
      signer: wallet,
      keyNftAta: nftAta,
      keyAuth: keyAuth,
      position: posPda,
      marketConfig: mcPda,
      programPda: programPda,
      personalPosition: ppPda,
      userShares: escrowPda,
      userNavSolAta: navAta,
      userWsolAta: wsolAta,
      tenant: MAYFLOWER_TENANT,
      marketGroup: mc.marketGroup,
      marketMeta: mc.marketMeta,
      mayflowerMarket: mc.mayflowerMarket,
      navSolMint: mc.navMint,
      marketBaseVault: mc.marketBaseVault,
      marketNavVault: mc.marketNavVault,
      feeVault: mc.feeVault,
      wsolMint: mc.baseMint,
      mayflowerProgram: MAYFLOWER_PROGRAM_ID,
      logAccount: logPda,
    })
    .instruction();

  return {
    description: [
      'Buy navSOL',
      `Amount: ${lamportsToSol(amountLamports)} SOL`,
      `Position: ${shortPubkey(posPda)}`,
      `Role: ${roleName(myRole.value)}`,
    ],
    instructions: [transferIx, syncIx, buyIx],
    extraSigners: [],
  };
}
