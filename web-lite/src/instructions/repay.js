import { BN } from '@coral-xyz/anchor';
import {
  deriveProgramPda,
  derivePersonalPosition,
  deriveLogAccount,
  getAta,
  WSOL_MINT,
  MAYFLOWER_TENANT,
  MARKET_GROUP,
  MARKET_META,
  MAYFLOWER_MARKET,
  MARKET_BASE_VAULT,
  MARKET_NAV_VAULT,
  FEE_VAULT,
  MAYFLOWER_PROGRAM_ID,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda, position } from '../state.js';
import { shortPubkey, lamportsToSol } from '../utils.js';

export async function buildRepay(program, wallet, amountLamports) {
  const nftMint = myNftMint.value;
  const keyAuth = myKeyAuthPda.value;
  const posPda = positionPda.value;
  const nftAta = getAta(wallet, nftMint);
  const [programPda] = deriveProgramPda(position.value.adminNftMint);
  const [ppPda] = derivePersonalPosition(programPda);
  const [logPda] = deriveLogAccount();
  const wsolAta = getAta(programPda, WSOL_MINT);

  const ix = await program.methods
    .repay(new BN(amountLamports))
    .accounts({
      signer: wallet,
      keyNftAta: nftAta,
      keyAuth: keyAuth,
      position: posPda,
      programPda: programPda,
      personalPosition: ppPda,
      userBaseTokenAta: wsolAta,
      tenant: MAYFLOWER_TENANT,
      marketGroup: MARKET_GROUP,
      marketMeta: MARKET_META,
      marketBaseVault: MARKET_BASE_VAULT,
      marketNavVault: MARKET_NAV_VAULT,
      feeVault: FEE_VAULT,
      wsolMint: WSOL_MINT,
      mayflowerMarket: MAYFLOWER_MARKET,
      mayflowerProgram: MAYFLOWER_PROGRAM_ID,
      logAccount: logPda,
    })
    .instruction();

  return {
    description: [
      'Repay',
      `Amount: ${lamportsToSol(amountLamports)} SOL`,
      `Position: ${shortPubkey(posPda)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
