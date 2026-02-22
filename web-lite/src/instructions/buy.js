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
import { myKeyAsset, positionPda, myPermissions, position, marketConfigPda, marketConfig, mfFloorPrice } from '../state.js';
import { shortPubkey, lamportsToSol, permissionsName } from '../utils.js';

export async function buildBuy(program, wallet, amountLamports) {
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

  // Pre-IXs: wrap SOL + sync native
  const transferIx = SystemProgram.transfer({
    fromPubkey: wallet,
    toPubkey: wsolAta,
    lamports: amountLamports,
  });
  const syncIx = createSyncNativeInstruction(wsolAta, TOKEN_PROGRAM_ID);

  // Slippage protection: estimate min_out using floor price.
  // buy: input SOL lamports -> output navSOL lamports
  // expected_nav = amount * 1e9 / floor_price, then apply 1% slippage
  const floorPrice = mfFloorPrice.value;
  let minOut;
  if (floorPrice > 0) {
    const expected = BigInt(amountLamports) * BigInt(1_000_000_000) / BigInt(floorPrice);
    minOut = new BN((expected * BigInt(99) / BigInt(100)).toString());
  } else {
    minOut = new BN(0); // floor price unavailable; no slippage protection
  }

  const buyIx = await program.methods
    .buy(new BN(amountLamports), minOut)
    .accounts({
      signer: wallet,
      keyAsset: keyAsset,
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
      `Permissions: ${permissionsName(myPermissions.value)}`,
    ],
    instructions: [transferIx, syncIx, buyIx],
    extraSigners: [],
  };
}
