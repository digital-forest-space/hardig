import { PublicKey } from '@solana/web3.js';
import { deriveKeyStatePda, KEY_STATE_SIZE } from './constants.js';

/**
 * Parse a RateBucket from raw account bytes.
 *
 * Layout (32 bytes total, all little-endian u64):
 *   capacity      (8 bytes)
 *   refill_period (8 bytes)
 *   level         (8 bytes)
 *   last_update   (8 bytes)
 *
 * @param {Uint8Array} data  Raw bytes starting at the bucket offset.
 * @returns {{ capacity: number, refillPeriod: number, level: number, lastUpdate: number }}
 */
function parseBucket(data) {
  const view = new DataView(data.buffer, data.byteOffset);
  return {
    capacity: Number(view.getBigUint64(0, true)),
    refillPeriod: Number(view.getBigUint64(8, true)),
    level: Number(view.getBigUint64(16, true)),
    lastUpdate: Number(view.getBigUint64(24, true)),
  };
}

/**
 * Parse a KeyState account into its component fields.
 *
 * KeyState layout (137 bytes):
 *   discriminator    (8 bytes)
 *   authority_seed   (32 bytes)  [offset 8]   — memcmp filterable
 *   asset            (32 bytes)  [offset 40]
 *   bump             (1 byte)    [offset 72]
 *   sell_bucket      (32 bytes)  [offset 73]
 *   borrow_bucket    (32 bytes)  [offset 105]
 *
 * @param {Uint8Array} data  Raw account data (must be >= KEY_STATE_SIZE).
 * @returns {{ sellBucket: object, borrowBucket: object, authoritySeed: PublicKey } | null}
 */
export function parseKeyState(data) {
  if (!data || data.length < KEY_STATE_SIZE) return null;
  return {
    authoritySeed: new PublicKey(data.slice(8, 40)),
    sellBucket: parseBucket(data.slice(73, 105)),
    borrowBucket: parseBucket(data.slice(105, 137)),
  };
}

/**
 * Compute the currently available tokens in a rate bucket without mutation.
 *
 * Replicates the on-chain refill logic:
 *   elapsed   = currentSlot - lastUpdate
 *   refill    = min(capacity, capacity * elapsed / refillPeriod)
 *   available = min(capacity, level + refill)
 *
 * @param {{ capacity: number, refillPeriod: number, level: number, lastUpdate: number }} bucket
 * @param {number} currentSlot
 * @returns {number} Available tokens (shares for sell, lamports for borrow).
 */
export function bucketAvailableNow(bucket, currentSlot) {
  if (bucket.capacity === 0) return 0;

  const elapsed = Math.max(0, currentSlot - bucket.lastUpdate);

  let refill;
  if (elapsed >= bucket.refillPeriod) {
    refill = bucket.capacity;
  } else {
    // Use BigInt to avoid overflow on large capacity * elapsed products
    refill = Number(
      (BigInt(bucket.capacity) * BigInt(elapsed)) / BigInt(bucket.refillPeriod)
    );
  }

  return Math.min(bucket.capacity, bucket.level + refill);
}

/**
 * Fetch and compute the available sell/borrow allowance for a limited key.
 *
 * @param {import('@solana/web3.js').Connection} connection
 * @param {import('@solana/web3.js').PublicKey} assetPubkey  The MPL-Core asset pubkey of the key.
 * @returns {Promise<{ sellAvailable: number, sellCapacity: number, sellRefillPeriod: number,
 *                      borrowAvailable: number, borrowCapacity: number, borrowRefillPeriod: number } | null>}
 */
export async function getKeyAllowance(connection, assetPubkey) {
  const [keyStatePda] = deriveKeyStatePda(assetPubkey);

  const [accountInfo, currentSlot] = await Promise.all([
    connection.getAccountInfo(keyStatePda),
    connection.getSlot('confirmed'),
  ]);

  if (!accountInfo) return null;

  const ks = parseKeyState(accountInfo.data);
  if (!ks) return null;

  return {
    sellAvailable: bucketAvailableNow(ks.sellBucket, currentSlot),
    sellCapacity: ks.sellBucket.capacity,
    sellRefillPeriod: ks.sellBucket.refillPeriod,
    borrowAvailable: bucketAvailableNow(ks.borrowBucket, currentSlot),
    borrowCapacity: ks.borrowBucket.capacity,
    borrowRefillPeriod: ks.borrowBucket.refillPeriod,
  };
}
