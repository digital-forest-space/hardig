import { deriveConfigPda } from '../constants.js';

export async function buildInitializeProtocol(program, wallet) {
  const [configPda] = deriveConfigPda();

  const ix = await program.methods
    .initializeProtocol()
    .accounts({
      admin: wallet,
      config: configPda,
    })
    .instruction();

  return {
    description: ['Initialize Protocol', `Config PDA: ${configPda}`, `Admin: ${wallet}`],
    instructions: [ix],
    extraSigners: [],
  };
}
