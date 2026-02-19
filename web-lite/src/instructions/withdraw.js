import { BN } from '@coral-xyz/anchor';
import {
  deriveProgramPda,
  derivePersonalPosition,
  derivePersonalPositionEscrow,
  deriveLogAccount,
  deriveKeyStatePda,
  getAta,
  MAYFLOWER_TENANT,
  MAYFLOWER_PROGRAM_ID,
  DEFAULT_WSOL_MINT,
  DEFAULT_NAV_SOL_MINT,
} from '../constants.js';
import { myKeyAsset, positionPda, position, marketConfigPda, marketConfig } from '../state.js';
import { shortPubkey, lamportsToSol, PERM_LIMITED_SELL } from '../utils.js';

export async function buildWithdraw(program, wallet, amountLamports) {
  const keyAsset = myKeyAsset.value;
  const posPda = positionPda.value;
  const mc = marketConfig.value;
  const mcPda = marketConfigPda.value;
  const baseMint = mc ? mc.baseMint : DEFAULT_WSOL_MINT;
  const navMint = mc ? mc.navMint : DEFAULT_NAV_SOL_MINT;
  const marketMeta = mc ? mc.marketMeta : undefined;
  const [programPda] = deriveProgramPda(position.value.adminAsset);
  const [ppPda] = derivePersonalPosition(programPda, marketMeta);
  const [escrowPda] = derivePersonalPositionEscrow(ppPda);
  const [logPda] = deriveLogAccount();
  const wsolAta = getAta(programPda, baseMint);
  const navAta = getAta(programPda, navMint);

  // Include keyState if the key might be rate-limited
  const [keyStatePda] = deriveKeyStatePda(keyAsset);

  const ix = await program.methods
    .withdraw(new BN(amountLamports), new BN(0)) // min_out = 0 (no slippage protection)
    .accounts({
      admin: wallet,
      keyAsset: keyAsset,
      keyState: keyStatePda,
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
      'Sell navSOL',
      `Amount: ${lamportsToSol(amountLamports)} SOL`,
      `Position: ${shortPubkey(posPda)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
