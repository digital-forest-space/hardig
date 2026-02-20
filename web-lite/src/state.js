import { signal, computed } from '@preact/signals';
import { PERM_BUY, PERM_SELL, PERM_BORROW, PERM_REPAY, PERM_REINVEST, PERM_MANAGE_KEYS, PERM_LIMITED_SELL, PERM_LIMITED_BORROW } from './utils.js';

// Cluster — persisted in localStorage so custom RPC URLs survive page reloads
export const cluster = signal(localStorage.getItem('hardig_cluster') || 'localnet');
export const customUrl = signal(localStorage.getItem('hardig_customUrl') || '');

// Auto-persist changes
cluster.subscribe((v) => localStorage.setItem('hardig_cluster', v));
customUrl.subscribe((v) => localStorage.setItem('hardig_customUrl', v));

// Wallet connection state
export const connected = signal(false);

// Protocol
export const protocolExists = signal(false);
export const collection = signal(null);

// Position
export const positionPda = signal(null);
export const position = signal(null);
export const myPermissions = signal(null); // u8 bitmask
export const myKeyAsset = signal(null);
export const myNftMint = signal(null);
export const keyring = signal([]);

// Multi-position discovery
export const discoveredPositions = signal([]);
export const activePositionIndex = signal(0);

// Market config (loaded from position's market_config PDA)
export const marketConfigPda = signal(null);
export const marketConfig = signal(null);

// Mayflower state
export const mayflowerInitialized = signal(false);
export const atasExist = signal(false);
export const wsolBalance = signal(0);
export const navSolBalance = signal(0);
export const mfDepositedShares = signal(0);
export const mfDebt = signal(0);
export const mfFloorPrice = signal(0);
export const mfBorrowCapacity = signal(0);

// UI state
export const refreshing = signal(false);
export const logs = signal([]);

// Pre-TX snapshot for result screen
export const preTxSnapshot = signal(null);
export const lastTxSignature = signal(null);

// Helper: check if current permissions include a specific bit
function hasPerm(perm) {
  const p = myPermissions.value;
  return p !== null && (p & perm) !== 0;
}

// Computed permissions
export const cpiReady = computed(() => mayflowerInitialized.value);

export const canBuy = computed(
  () => cpiReady.value && hasPerm(PERM_BUY)
);

export const canSell = computed(
  () => cpiReady.value && (hasPerm(PERM_SELL) || hasPerm(PERM_LIMITED_SELL))
);

export const canBorrow = computed(
  () => cpiReady.value && (hasPerm(PERM_BORROW) || hasPerm(PERM_LIMITED_BORROW))
);

export const canRepay = computed(() => {
  if (!cpiReady.value) return false;
  if (!hasPerm(PERM_REPAY)) return false;
  const pos = position.value;
  return pos && pos.userDebt > 0;
});

export const canReinvest = computed(
  () => cpiReady.value && hasPerm(PERM_REINVEST)
);

export const canAuthorize = computed(
  () => hasPerm(PERM_MANAGE_KEYS) && positionPda.value !== null
);

export const canRevoke = computed(
  () => hasPerm(PERM_MANAGE_KEYS) && keyring.value.length > 1
);

export const canInitProtocol = computed(() => !protocolExists.value);

export const canCreatePosition = computed(
  () => protocolExists.value
);

// Logging helper
export function pushLog(msg, isError = false) {
  const entry = { text: String(msg), isError, ts: Date.now() };
  const next = [...logs.value, entry];
  if (next.length > 100) next.shift();
  logs.value = next;
}

// Take a snapshot of current state for result screen
export function takeSnapshot() {
  const pos = position.value;
  if (!pos) return null;
  return {
    depositedNav: pos.depositedNav,
    userDebt: pos.userDebt,
    borrowCapacity: mfBorrowCapacity.value,
    wsolBalance: wsolBalance.value,
    navSolBalance: navSolBalance.value,
  };
}

// Reset position state
export function resetPositionState() {
  positionPda.value = null;
  position.value = null;
  myPermissions.value = null;
  myKeyAsset.value = null;
  myNftMint.value = null;
  keyring.value = [];
  marketConfigPda.value = null;
  marketConfig.value = null;
  mayflowerInitialized.value = false;
  atasExist.value = false;
  wsolBalance.value = 0;
  navSolBalance.value = 0;
  mfDepositedShares.value = 0;
  mfDebt.value = 0;
  mfFloorPrice.value = 0;
  mfBorrowCapacity.value = 0;
  discoveredPositions.value = [];
  activePositionIndex.value = 0;
}
