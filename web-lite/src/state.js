import { signal, computed } from '@preact/signals';

// Cluster
export const cluster = signal('localnet');
export const customUrl = signal('');

// Wallet connection state
export const connected = signal(false);

// Protocol
export const protocolExists = signal(false);

// Position
export const positionPda = signal(null);
export const position = signal(null);
export const myRole = signal(null); // numeric: 0=Admin, 1=Operator, 2=Depositor, 3=Keeper
export const myKeyAuthPda = signal(null);
export const myNftMint = signal(null);
export const keyring = signal([]);

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

// Computed permissions
export const cpiReady = computed(() => mayflowerInitialized.value && atasExist.value);

export const canBuy = computed(
  () => cpiReady.value && [0, 1, 2].includes(myRole.value)
);

export const canSell = computed(
  () => cpiReady.value && myRole.value === 0
);

export const canBorrow = computed(
  () => cpiReady.value && myRole.value === 0
);

export const canRepay = computed(() => {
  if (!cpiReady.value) return false;
  if (![0, 1, 2].includes(myRole.value)) return false;
  const pos = position.value;
  return pos && pos.userDebt > 0;
});

export const canReinvest = computed(
  () => cpiReady.value && [0, 1, 3].includes(myRole.value)
);

export const canAuthorize = computed(
  () => myRole.value === 0 && positionPda.value !== null
);

export const canRevoke = computed(
  () => myRole.value === 0 && keyring.value.some((k) => k.role !== 0)
);

export const canSetup = computed(
  () => myRole.value === 0 && positionPda.value !== null && !cpiReady.value
);

export const canInitProtocol = computed(() => !protocolExists.value);

export const canCreatePosition = computed(
  () => protocolExists.value && positionPda.value === null
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
    protocolDebt: pos.protocolDebt,
    borrowCapacity: mfBorrowCapacity.value,
    wsolBalance: wsolBalance.value,
    navSolBalance: navSolBalance.value,
  };
}

// Reset position state
export function resetPositionState() {
  positionPda.value = null;
  position.value = null;
  myRole.value = null;
  myKeyAuthPda.value = null;
  myNftMint.value = null;
  keyring.value = [];
  mayflowerInitialized.value = false;
  atasExist.value = false;
  wsolBalance.value = 0;
  navSolBalance.value = 0;
  mfDepositedShares.value = 0;
  mfDebt.value = 0;
  mfFloorPrice.value = 0;
  mfBorrowCapacity.value = 0;
}
