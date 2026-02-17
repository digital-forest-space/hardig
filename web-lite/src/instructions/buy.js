import { SystemProgram } from '@solana/web3.js';
import { createSyncNativeInstruction } from '@solana/spl-token';
import { BN } from '@coral-xyz/anchor';
import {
  deriveProgramPda,
  derivePersonalPosition,
  derivePersonalPositionEscrow,
  deriveLogAccount,
  getAta,
  WSOL_MINT,
  NAV_SOL_MINT,
  MAYFLOWER_TENANT,
  MARKET_GROUP,
  MARKET_META,
  MAYFLOWER_MARKET,
  MARKET_BASE_VAULT,
  MARKET_NAV_VAULT,
  FEE_VAULT,
  MAYFLOWER_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda, myRole } from '../state.js';
import { shortPubkey, lamportsToSol, roleName } from '../utils.js';

export async function buildBuy(program, wallet, amountLamports) {
  const nftMint = myNftMint.value;
  const keyAuth = myKeyAuthPda.value;
  const posPda = positionPda.value;
  const nftAta = getAta(wallet, nftMint);
  const [programPda] = deriveProgramPda();
  const [ppPda] = derivePersonalPosition(programPda);
  const [escrowPda] = derivePersonalPositionEscrow(ppPda);
  const [logPda] = deriveLogAccount();
  const wsolAta = getAta(programPda, WSOL_MINT);
  const navAta = getAta(programPda, NAV_SOL_MINT);

  // Pre-IXs: wrap SOL + sync native
  const transferIx = SystemProgram.transfer({
    fromPubkey: wallet,
    toPubkey: wsolAta,
    lamports: amountLamports,
  });
  const syncIx = createSyncNativeInstruction(wsolAta, TOKEN_PROGRAM_ID);

  const buyIx = await program.methods
    .buy(new BN(amountLamports))
    .accounts({
      signer: wallet,
      keyNftAta: nftAta,
      keyAuth: keyAuth,
      position: posPda,
      programPda: programPda,
      personalPosition: ppPda,
      userShares: escrowPda,
      userNavSolAta: navAta,
      userWsolAta: wsolAta,
      tenant: MAYFLOWER_TENANT,
      marketGroup: MARKET_GROUP,
      marketMeta: MARKET_META,
      mayflowerMarket: MAYFLOWER_MARKET,
      navSolMint: NAV_SOL_MINT,
      marketBaseVault: MARKET_BASE_VAULT,
      marketNavVault: MARKET_NAV_VAULT,
      feeVault: FEE_VAULT,
      wsolMint: WSOL_MINT,
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
