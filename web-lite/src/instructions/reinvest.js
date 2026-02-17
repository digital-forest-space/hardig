import { ComputeBudgetProgram } from '@solana/web3.js';
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
import { myNftMint, myKeyAuthPda, positionPda, myRole } from '../state.js';
import { shortPubkey, roleName } from '../utils.js';

export async function buildReinvest(program, wallet) {
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

  const computeIx = ComputeBudgetProgram.setComputeUnitLimit({
    units: 400_000,
  });

  const ix = await program.methods
    .reinvest()
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
      userBaseTokenAta: wsolAta,
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
      'Reinvest',
      `Position: ${shortPubkey(posPda)}`,
      `Role: ${roleName(myRole.value)}`,
      'Borrows available capacity and buys more navSOL',
    ],
    instructions: [computeIx, ix],
    extraSigners: [],
  };
}
