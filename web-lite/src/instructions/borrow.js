import { BN } from '@coral-xyz/anchor';
import {
  deriveProgramPda,
  derivePersonalPosition,
  deriveLogAccount,
  getAta,
  MAYFLOWER_TENANT,
  MAYFLOWER_PROGRAM_ID,
  DEFAULT_WSOL_MINT,
} from '../constants.js';
import { myNftMint, myKeyAuthPda, positionPda, position, marketConfigPda, marketConfig } from '../state.js';
import { shortPubkey, lamportsToSol } from '../utils.js';

export async function buildBorrow(program, wallet, amountLamports) {
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
  const [logPda] = deriveLogAccount();
  const wsolAta = getAta(programPda, baseMint);

  const ix = await program.methods
    .borrow(new BN(amountLamports))
    .accounts({
      admin: wallet,
      keyNftAta: nftAta,
      keyAuth: keyAuth,
      position: posPda,
      marketConfig: mcPda,
      programPda: programPda,
      personalPosition: ppPda,
      userBaseTokenAta: wsolAta,
      tenant: MAYFLOWER_TENANT,
      marketGroup: mc.marketGroup,
      marketMeta: mc.marketMeta,
      marketBaseVault: mc.marketBaseVault,
      marketNavVault: mc.marketNavVault,
      feeVault: mc.feeVault,
      wsolMint: mc.baseMint,
      mayflowerMarket: mc.mayflowerMarket,
      mayflowerProgram: MAYFLOWER_PROGRAM_ID,
      logAccount: logPda,
    })
    .instruction();

  return {
    description: [
      'Borrow',
      `Amount: ${lamportsToSol(amountLamports)} SOL`,
      `Position: ${shortPubkey(posPda)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
