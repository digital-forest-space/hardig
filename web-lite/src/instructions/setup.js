import {
  createAssociatedTokenAccountInstruction,
} from '@solana/spl-token';
import {
  deriveProgramPda,
  getAta,
  WSOL_MINT,
  NAV_SOL_MINT,
  TOKEN_PROGRAM_ID,
} from '../constants.js';
import { mayflowerInitialized, atasExist } from '../state.js';
import { buildInitMayflowerPosition } from './initMayflowerPosition.js';
import { shortPubkey } from '../utils.js';

export async function buildSetup(program, wallet) {
  const instructions = [];
  const description = ['Setup Mayflower Accounts'];

  // Step 1: Init Mayflower position if needed
  if (!mayflowerInitialized.value) {
    const initResult = await buildInitMayflowerPosition(program, wallet);
    instructions.push(...initResult.instructions);
    description.push(...initResult.description.slice(1));
  }

  // Step 2: Create ATAs if needed
  if (!atasExist.value) {
    const [programPda] = deriveProgramPda();
    const wsolAta = getAta(programPda, WSOL_MINT);
    const navAta = getAta(programPda, NAV_SOL_MINT);

    instructions.push(
      createAssociatedTokenAccountInstruction(
        wallet,
        wsolAta,
        programPda,
        WSOL_MINT,
        TOKEN_PROGRAM_ID
      )
    );
    instructions.push(
      createAssociatedTokenAccountInstruction(
        wallet,
        navAta,
        programPda,
        NAV_SOL_MINT,
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
