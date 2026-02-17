import {
  deriveConfigPda,
  deriveMarketConfigPda,
} from '../constants.js';
import { shortPubkey } from '../utils.js';

export async function buildCreateMarketConfig(
  program,
  wallet,
  navMint,
  baseMint,
  marketGroup,
  marketMeta,
  mayflowerMarket,
  marketBaseVault,
  marketNavVault,
  feeVault
) {
  const [configPda] = deriveConfigPda();
  const [mcPda] = deriveMarketConfigPda(navMint);

  const ix = await program.methods
    .createMarketConfig(
      navMint,
      baseMint,
      marketGroup,
      marketMeta,
      mayflowerMarket,
      marketBaseVault,
      marketNavVault,
      feeVault
    )
    .accounts({
      admin: wallet,
      config: configPda,
      marketConfig: mcPda,
    })
    .instruction();

  return {
    description: [
      'Create Market Config',
      `Nav Mint: ${shortPubkey(navMint)}`,
      `MarketConfig PDA: ${shortPubkey(mcPda)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
