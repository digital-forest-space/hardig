import { PublicKey } from '@solana/web3.js';

/**
 * Fetch Mayflower market configs and resolve a market by name.
 * Returns an object with all 8 pubkeys needed for createMarketConfig.
 *
 * @param {string} marketsUrl - URL of the markets API endpoint.
 * @param {string} name - Market name (e.g. "navSOL"), case-insensitive.
 * @returns {Promise<{navMint, baseMint, marketGroup, marketMeta, mayflowerMarket, marketBaseVault, marketNavVault, feeVault}>}
 */
export async function resolveMarket(marketsUrl, name) {
  const res = await fetch(marketsUrl);
  if (!res.ok) {
    throw new Error(`Failed to fetch markets: ${res.status} ${res.statusText}`);
  }
  const data = await res.json();
  const markets = data.markets;
  if (!Array.isArray(markets)) {
    throw new Error('Unexpected API response: missing markets array');
  }

  const needle = name.toLowerCase();
  const market = markets.find((m) => m.name?.toLowerCase() === needle);

  if (!market) {
    const available = markets.map((m) => m.name).join(', ');
    throw new Error(
      `Market "${name}" not found. Available markets: ${available}`
    );
  }

  return {
    navMint: new PublicKey(market.navMint),
    baseMint: new PublicKey(market.baseMint),
    marketGroup: new PublicKey(market.marketGroup),
    marketMeta: new PublicKey(market.marketMetadata),
    mayflowerMarket: new PublicKey(market.mayflowerMarket),
    marketBaseVault: new PublicKey(market.marketSolVault),
    marketNavVault: new PublicKey(market.marketNavVault),
    feeVault: new PublicKey(market.feeVault),
  };
}
