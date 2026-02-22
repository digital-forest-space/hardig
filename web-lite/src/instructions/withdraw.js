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
import { myKeyAsset, positionPda, position, marketConfigPda, marketConfig, mfFloorPrice } from '../state.js';
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

  // Slippage protection: estimate min_out using floor price.
  // sell/withdraw: input navSOL lamports -> output SOL lamports
  // expected_sol = amount * floor_price / 1e9, then apply 1% slippage
  const floorPrice = mfFloorPrice.value;
  let minOut;
  if (floorPrice > 0) {
    const expected = BigInt(amountLamports) * BigInt(floorPrice) / BigInt(1_000_000_000);
    minOut = new BN((expected * BigInt(99) / BigInt(100)).toString());
  } else {
    minOut = new BN(0); // floor price unavailable; no slippage protection
  }

  const ix = await program.methods
    .withdraw(new BN(amountLamports), minOut)
    .accounts({
      signer: wallet,
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
