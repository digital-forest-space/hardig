import {
  createAssociatedTokenAccountInstruction,
} from '@solana/spl-token';
import {
  deriveProgramPda,
  getAta,
  deriveConfigPda,
  deriveMarketConfigPda,
  DEFAULT_WSOL_MINT,
  DEFAULT_NAV_SOL_MINT,
  DEFAULT_MARKET_GROUP,
  DEFAULT_MARKET_META,
  DEFAULT_MAYFLOWER_MARKET,
  DEFAULT_MARKET_BASE_VAULT,
  DEFAULT_MARKET_NAV_VAULT,
  DEFAULT_FEE_VAULT,
  TOKEN_PROGRAM_ID,
} from '../constants.js';
import { mayflowerInitialized, atasExist, position, marketConfig } from '../state.js';
import { buildInitMayflowerPosition } from './initMayflowerPosition.js';
import { shortPubkey } from '../utils.js';

export async function buildSetup(program, wallet) {
  const instructions = [];
  const description = ['Setup Mayflower Accounts'];

  // Step 0: Create MarketConfig if it doesn't exist on-chain yet
  if (!marketConfig.value) {
    const [configPda] = deriveConfigPda();
    const [mcPda] = deriveMarketConfigPda(DEFAULT_NAV_SOL_MINT);
    const ix = await program.methods
      .createMarketConfig(
        DEFAULT_NAV_SOL_MINT,
        DEFAULT_WSOL_MINT,
        DEFAULT_MARKET_GROUP,
        DEFAULT_MARKET_META,
        DEFAULT_MAYFLOWER_MARKET,
        DEFAULT_MARKET_BASE_VAULT,
        DEFAULT_MARKET_NAV_VAULT,
        DEFAULT_FEE_VAULT,
      )
      .accounts({
        admin: wallet,
        config: configPda,
        marketConfig: mcPda,
      })
      .instruction();
    instructions.push(ix);
    description.push(`Create MarketConfig: ${shortPubkey(mcPda)}`);
  }

  // Step 1: Init Mayflower position if needed
  if (!mayflowerInitialized.value) {
    const initResult = await buildInitMayflowerPosition(program, wallet);
    instructions.push(...initResult.instructions);
    description.push(...initResult.description.slice(1));
  }

  // Step 2: Create ATAs if needed
  if (!atasExist.value) {
    const mc = marketConfig.value;
    const baseMint = mc ? mc.baseMint : DEFAULT_WSOL_MINT;
    const navMint = mc ? mc.navMint : DEFAULT_NAV_SOL_MINT;
    const [programPda] = deriveProgramPda(position.value.adminNftMint);
    const wsolAta = getAta(programPda, baseMint);
    const navAta = getAta(programPda, navMint);

    instructions.push(
      createAssociatedTokenAccountInstruction(
        wallet,
        wsolAta,
        programPda,
        baseMint,
        TOKEN_PROGRAM_ID
      )
    );
    instructions.push(
      createAssociatedTokenAccountInstruction(
        wallet,
        navAta,
        programPda,
        navMint,
        TOKEN_PROGRAM_ID
      )
    );
    description.push(`Create wSOL ATA: ${shortPubkey(wsolAta)}`);
    description.push(`Create navSOL ATA: ${shortPubkey(navAta)}`);
  }

  if (instructions.length === 0) {
    return null; // Already setup
  }

  return {
    description,
    instructions,
    extraSigners: [],
  };
}
