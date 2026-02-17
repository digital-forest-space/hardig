import {
  PP_DEPOSITED_SHARES_OFFSET,
  PP_DEBT_OFFSET,
  MARKET_FLOOR_PRICE_OFFSET,
  getAta,
  DEFAULT_NAV_SOL_MINT,
  DEFAULT_WSOL_MINT,
  DEFAULT_MAYFLOWER_MARKET,
  deriveProgramPda,
  derivePersonalPosition,
} from './constants.js';
import {
  mayflowerInitialized,
  atasExist,
  wsolBalance,
  navSolBalance,
  mfDepositedShares,
  mfDebt,
  mfFloorPrice,
  mfBorrowCapacity,
  pushLog,
  position,
  marketConfig,
} from './state.js';

// Read u64 LE from buffer at offset
function readU64(data, offset) {
  const view = new DataView(data.buffer, data.byteOffset);
  return Number(view.getBigUint64(offset, true));
}

// Read token balance from ATA (u64 LE at offset 64)
function parseTokenBalance(data) {
  if (!data || data.length < 72) return null;
  const view = new DataView(data.buffer, data.byteOffset);
  return Number(view.getBigUint64(64, true));
}

// Decode Rust Decimal (16 bytes) to lamports (scaled by 1e9)
function decodeRustDecimalToLamports(bytes) {
  const scale = bytes[2];

  let mantissa = BigInt(0);
  for (let i = 4; i < 16; i++) {
    mantissa |= BigInt(bytes[i]) << BigInt(8 * (i - 4));
  }

  const scaled = mantissa * BigInt(1_000_000_000);
  const divisor = BigInt(10) ** BigInt(scale);
  return Number(scaled / divisor);
}

// Calculate borrow capacity using BigInt for overflow safety
function calculateBorrowCapacity(shares, floorPrice, debt) {
  const floorValue =
    (BigInt(shares) * BigInt(floorPrice)) / BigInt(1_000_000_000);
  const capacity = floorValue - BigInt(debt);
  return capacity > BigInt(0) ? Number(capacity) : 0;
}

export async function refreshMayflowerState(connection) {
  wsolBalance.value = 0;
  navSolBalance.value = 0;
  atasExist.value = false;
  mfDepositedShares.value = 0;
  mfDebt.value = 0;
  mfFloorPrice.value = 0;
  mfBorrowCapacity.value = 0;

  if (!mayflowerInitialized.value) return;

  const mc = marketConfig.value;
  const baseMint = mc ? mc.baseMint : DEFAULT_WSOL_MINT;
  const navMint = mc ? mc.navMint : DEFAULT_NAV_SOL_MINT;
  const marketMeta = mc ? mc.marketMeta : undefined;
  const mfMarket = mc ? mc.mayflowerMarket : DEFAULT_MAYFLOWER_MARKET;

  const [programPda] = deriveProgramPda(position.value.adminNftMint);
  const [ppPda] = derivePersonalPosition(programPda, marketMeta);
  const wsolAta = getAta(programPda, baseMint);
  const navAta = getAta(programPda, navMint);

  // Batch fetch all accounts
  const infos = await connection.getMultipleAccountsInfo([
    wsolAta,
    navAta,
    ppPda,
    mfMarket,
  ]);

  const [wsolInfo, navInfo, ppInfo, marketInfo] = infos;

  // Check ATAs
  const wsol = wsolInfo ? parseTokenBalance(wsolInfo.data) : null;
  const nav = navInfo ? parseTokenBalance(navInfo.data) : null;

  if (wsol !== null && nav !== null) {
    wsolBalance.value = wsol;
    navSolBalance.value = nav;
    atasExist.value = true;
  }

  // Read PersonalPosition
  if (ppInfo && ppInfo.data.length >= PP_DEBT_OFFSET + 8) {
    const shares = readU64(ppInfo.data, PP_DEPOSITED_SHARES_OFFSET);
    const debt = readU64(ppInfo.data, PP_DEBT_OFFSET);
    mfDepositedShares.value = shares;
    mfDebt.value = debt;
  }

  // Read market floor price
  if (marketInfo && marketInfo.data.length > MARKET_FLOOR_PRICE_OFFSET + 16) {
    const decimalBytes = marketInfo.data.slice(
      MARKET_FLOOR_PRICE_OFFSET,
      MARKET_FLOOR_PRICE_OFFSET + 16
    );
    const floor = decodeRustDecimalToLamports(decimalBytes);
    mfFloorPrice.value = floor;
  }

  // Calculate borrow capacity
  mfBorrowCapacity.value = calculateBorrowCapacity(
    mfDepositedShares.value,
    mfFloorPrice.value,
    mfDebt.value
  );
}
