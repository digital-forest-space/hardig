import { TransactionInstruction } from '@solana/web3.js';
import { PROGRAM_ID, deriveConfigPda } from '../constants.js';
import { myKeyAsset, positionPda } from '../state.js';
import { shortPubkey } from '../utils.js';

// Anchor discriminator: sha256("global:update_promo")[..8]
const UPDATE_PROMO_DISC = new Uint8Array([27, 234, 19, 191, 30, 2, 27, 161]);

/**
 * Encode an Option<bool>: 0x00 for None, 0x01 + (0x00|0x01) for Some.
 */
function encodeOptionBool(value) {
  if (value === null || value === undefined) {
    return new Uint8Array([0x00]);
  }
  return new Uint8Array([0x01, value ? 0x01 : 0x00]);
}

/**
 * Encode an Option<u32>: 0x00 for None, 0x01 + 4-byte LE for Some.
 */
function encodeOptionU32(value) {
  if (value === null || value === undefined) {
    return new Uint8Array([0x00]);
  }
  const buf = new Uint8Array(5);
  buf[0] = 0x01;
  const view = new DataView(buf.buffer);
  view.setUint32(1, value, true);
  return buf;
}

export async function buildUpdatePromo(program, wallet, promoPda, active, maxClaims) {
  const posPda = positionPda.value;
  const adminKeyAsset = myKeyAsset.value;

  // Build instruction data:
  // discriminator(8) + active(Option<bool>) + max_claims(Option<u32>)
  const activeBytes = encodeOptionBool(active);
  const maxClaimsBytes = encodeOptionU32(maxClaims);

  const dataLen = 8 + activeBytes.length + maxClaimsBytes.length;
  const data = new Uint8Array(dataLen);
  let offset = 0;

  data.set(UPDATE_PROMO_DISC, offset); offset += 8;
  data.set(activeBytes, offset); offset += activeBytes.length;
  data.set(maxClaimsBytes, offset); offset += maxClaimsBytes.length;

  const [configPda] = deriveConfigPda();
  const keys = [
    { pubkey: wallet, isSigner: true, isWritable: true },
    { pubkey: adminKeyAsset, isSigner: false, isWritable: false },
    { pubkey: posPda, isSigner: false, isWritable: false },
    { pubkey: promoPda, isSigner: false, isWritable: true },
    { pubkey: configPda, isSigner: false, isWritable: false },
  ];

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys,
    data: Buffer.from(data),
  });

  const changes = [];
  if (active !== null && active !== undefined) {
    changes.push(`Active: ${active ? 'Yes' : 'No'}`);
  }
  if (maxClaims !== null && maxClaims !== undefined) {
    changes.push(`Max Claims: ${maxClaims === 0 ? 'Unlimited' : maxClaims}`);
  }

  return {
    description: [
      'Update Promo',
      `Promo: ${shortPubkey(promoPda)}`,
      ...changes,
    ],
    instructions: [ix],
    extraSigners: [],
  };
}
