import { ComputeBudgetProgram } from '@solana/web3.js';
import { BN } from '@coral-xyz/anchor';
import {
  deriveProgramPda,
  derivePersonalPosition,
  derivePersonalPositionEscrow,
  deriveLogAccount,
  getAta,
  MAYFLOWER_TENANT,
  MAYFLOWER_PROGRAM_ID,
  DEFAULT_WSOL_MINT,
  DEFAULT_NAV_SOL_MINT,
} from '../constants.js';
import { myKeyAsset, positionPda, myPermissions, position, marketConfigPda, marketConfig } from '../state.js';
import { shortPubkey, permissionsName } from '../utils.js';

export async function buildReinvest(program, wallet) {
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

  const computeIx = ComputeBudgetProgram.setComputeUnitLimit({
    units: 400_000,
  });

  const ix = await program.methods
    .reinvest(new BN(0)) // min_out = 0 (no slippage protection)
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
      userBaseTokenAta: wsolAta,
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
      'Reinvest',
      `Position: ${shortPubkey(posPda)}`,
      `Permissions: ${permissionsName(myPermissions.value)}`,
      'Borrows available capacity and buys more navSOL',
    ],
    instructions: [computeIx, ix],
    extraSigners: [],
  };
}
