import { PublicKey, TransactionInstruction } from '@solana/web3.js';
import {
  PROGRAM_ID,
  derivePromoPda,
  deriveConfigPda,
  SYSTEM_PROGRAM_ID,
} from '../constants.js';
import { myKeyAsset, positionPda, position } from '../state.js';
import { shortPubkey, permissionsName, lamportsToSol } from '../utils.js';

// Anchor discriminator: sha256("global:create_promo")[..8]
const CREATE_PROMO_DISC = new Uint8Array([135, 231, 68, 194, 63, 31, 192, 82]);

/**
 * Encode a Borsh String: 4-byte LE length + UTF-8 bytes.
 */
function encodeBorshString(str) {
  const bytes = new TextEncoder().encode(str);
  const buf = new Uint8Array(4 + bytes.length);
  const view = new DataView(buf.buffer);
  view.setUint32(0, bytes.length, true);
  buf.set(bytes, 4);
  return buf;
}

/**
 * Encode a u64 as 8 bytes LE.
 */
function encodeU64(value) {
  const buf = new Uint8Array(8);
  const view = new DataView(buf.buffer);
  view.setBigUint64(0, BigInt(value), true);
  return buf;
}

/**
 * Encode a u16 as 2 bytes LE.
 */
function encodeU16(value) {
  const buf = new Uint8Array(2);
  const view = new DataView(buf.buffer);
  view.setUint16(0, value, true);
  return buf;
}

/**
 * Encode a u32 as 4 bytes LE.
 */
function encodeU32(value) {
  const buf = new Uint8Array(4);
  const view = new DataView(buf.buffer);
  view.setUint32(0, value, true);
  return buf;
}

export async function buildCreatePromo(
  program,
  wallet,
  nameSuffix,
  permissions,
  borrowCapacity,
  borrowRefillPeriod,
  sellCapacity,
  sellRefillPeriod,
  totalBorrowLimit,
  totalSellLimit,
  minDepositLamports,
  maxClaims,
  initialFillBps,
  imageUri,
  marketName = ''
) {
  const posPda = positionPda.value;
  const adminKeyAsset = myKeyAsset.value;
  const authoritySeed = position.value.authoritySeed;

  const [promoPda] = derivePromoPda(authoritySeed, nameSuffix);

  // Build instruction data:
  // discriminator(8) + name_suffix(String) + permissions(u8) + borrow_capacity(u64) +
  // borrow_refill_period(u64) + sell_capacity(u64) + sell_refill_period(u64) +
  // total_borrow_limit(u64) + total_sell_limit(u64) +
  // min_deposit_lamports(u64) + max_claims(u32) + initial_fill_bps(u16) +
  // image_uri(String) + market_name(String)
  const nameSuffixBytes = encodeBorshString(nameSuffix);
  const imageUriBytes = encodeBorshString(imageUri);
  const marketNameBytes = encodeBorshString(marketName);

  const dataLen = 8 + nameSuffixBytes.length + 1 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 4 + 2 + imageUriBytes.length + marketNameBytes.length;
  const data = new Uint8Array(dataLen);
  let offset = 0;

  data.set(CREATE_PROMO_DISC, offset); offset += 8;
  data.set(nameSuffixBytes, offset); offset += nameSuffixBytes.length;
  data[offset] = permissions; offset += 1;
  data.set(encodeU64(borrowCapacity), offset); offset += 8;
  data.set(encodeU64(borrowRefillPeriod), offset); offset += 8;
  data.set(encodeU64(sellCapacity), offset); offset += 8;
  data.set(encodeU64(sellRefillPeriod), offset); offset += 8;
  data.set(encodeU64(totalBorrowLimit), offset); offset += 8;
  data.set(encodeU64(totalSellLimit), offset); offset += 8;
  data.set(encodeU64(minDepositLamports), offset); offset += 8;
  data.set(encodeU32(maxClaims), offset); offset += 4;
  data.set(encodeU16(initialFillBps), offset); offset += 2;
  data.set(imageUriBytes, offset); offset += imageUriBytes.length;
  data.set(marketNameBytes, offset); offset += marketNameBytes.length;

  const [configPda] = deriveConfigPda();
  const keys = [
    { pubkey: wallet, isSigner: true, isWritable: true },
    { pubkey: adminKeyAsset, isSigner: false, isWritable: false },
    { pubkey: posPda, isSigner: false, isWritable: false },
    { pubkey: promoPda, isSigner: false, isWritable: true },
    { pubkey: configPda, isSigner: false, isWritable: false },
    { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
  ];

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys,
    data: Buffer.from(data),
  });

  return {
    description: [
      'Create Promo',
      `Name: ${nameSuffix}`,
      `Permissions: ${permissionsName(permissions)} (0x${permissions.toString(16).padStart(2, '0')})`,
      `Min Deposit: ${lamportsToSol(minDepositLamports)} SOL`,
      `Max Claims: ${maxClaims === 0 ? 'Unlimited' : maxClaims}`,
      `Promo PDA: ${shortPubkey(promoPda)}`,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
