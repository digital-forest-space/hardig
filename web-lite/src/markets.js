import { signal } from '@preact/signals';
import { PublicKey } from '@solana/web3.js';

const MAYFLOWER_PROGRAM_ID = new PublicKey('MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA');
const SPL_TOKEN_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const TOKEN_2022_ID = new PublicKey('TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb');

/** Verified market entries loaded from /markets.json */
export const availableMarkets = signal([]);

/**
 * Load markets from /markets.json, verify against on-chain state,
 * and store only verified entries in the signal.
 * Gracefully handles 404 (no file -> empty array).
 */
export async function loadMarkets(connection) {
  try {
    const resp = await fetch('/markets.json');
    if (!resp.ok) {
      // 404 or other error — no markets file, use default navSOL
      availableMarkets.value = [];
      return;
    }
    const entries = await resp.json();
    if (!Array.isArray(entries) || entries.length === 0) {
      availableMarkets.value = [];
      return;
    }

    // Verify each market's addresses against on-chain state
    const keysToFetch = [];
    const validIndices = [];
    for (let i = 0; i < entries.length; i++) {
      try {
        const mfMarket = new PublicKey(entries[i].mayflowerMarket);
        const navMint = new PublicKey(entries[i].navMint);
        validIndices.push({ idx: i, offset: keysToFetch.length });
        keysToFetch.push(mfMarket);
        keysToFetch.push(navMint);
      } catch {
        // Invalid pubkey, skip
      }
    }

    if (keysToFetch.length === 0) {
      availableMarkets.value = [];
      return;
    }

    const accounts = await connection.getMultipleAccountsInfo(keysToFetch);
    const verified = [];
    for (const { idx, offset } of validIndices) {
      const mfAcc = accounts[offset];
      const mintAcc = accounts[offset + 1];
      const mfOk = mfAcc && mfAcc.owner.equals(MAYFLOWER_PROGRAM_ID);
      const mintOk = mintAcc && (mintAcc.owner.equals(SPL_TOKEN_ID) || mintAcc.owner.equals(TOKEN_2022_ID));
      if (mfOk && mintOk) {
        verified.push(entries[idx]);
      } else {
        console.warn(`Market ${entries[idx].navSymbol} failed verification (mf=${!!mfOk}, mint=${!!mintOk}), skipping`);
      }
    }

    availableMarkets.value = verified;
  } catch (e) {
    console.warn('Failed to load markets:', e);
    availableMarkets.value = [];
  }
}

/**
 * Convert a market entry from the JSON to PublicKey objects matching
 * what buildCreateMarketConfig / buildCreatePosition expect.
 */
export function marketEntryToPubkeys(entry) {
  return {
    navMint: new PublicKey(entry.navMint),
    baseMint: new PublicKey(entry.baseMint),
    marketGroup: new PublicKey(entry.marketGroup),
    marketMeta: new PublicKey(entry.marketMetadata),
    mayflowerMarket: new PublicKey(entry.mayflowerMarket),
    marketBaseVault: new PublicKey(entry.baseVault),
    marketNavVault: new PublicKey(entry.navVault),
    feeVault: new PublicKey(entry.feeVault),
    navSymbol: entry.navSymbol,
    floorPrice: entry.floorPrice || 0,
  };
}
