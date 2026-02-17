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
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda, position } from '../state.js';
import { shortPubkey, lamportsToSol } from '../utils.js';

export async function buildWithdraw(program, wallet, amountLamports) {
  const nftMint = myNftMint.value;
  const keyAuth = myKeyAuthPda.value;
  const posPda = positionPda.value;
  const nftAta = getAta(wallet, nftMint);
  const [programPda] = deriveProgramPda(position.value.adminNftMint);
  const [ppPda] = derivePersonalPosition(programPda);
  const [escrowPda] = derivePersonalPositionEscrow(ppPda);
  const [logPda] = deriveLogAccount();
  const wsolAta = getAta(programPda, WSOL_MINT);
  const navAta = getAta(programPda, NAV_SOL_MINT);

  const ix = await program.methods
    .withdraw(new BN(amountLamports))
    .accounts({
      admin: wallet,
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
      'Sell navSOL',
      `Amount: ${lamportsToSol(amountLamports)} SOL`,
      `Position: ${shortPubkey(posPda)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
