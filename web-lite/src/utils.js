export function lamportsToSol(lamports) {
  if (typeof lamports === 'bigint') {
    lamports = Number(lamports);
  }
  const sol = lamports / 1_000_000_000;
  if (sol === 0) return '0';
  if (sol < 0.001) return sol.toFixed(9);
  return sol.toFixed(4);
}

export function parseSolToLamports(s) {
  const sol = parseFloat(s.trim());
  if (isNaN(sol) || sol < 0) return null;
  const lamports = Math.round(sol * 1_000_000_000);
  if (lamports === 0 && sol > 0) return null;
  return lamports;
}

export function shortPubkey(pubkey) {
  const s = pubkey.toString();
  if (s.length > 12) {
    return s.slice(0, 4) + '..' + s.slice(-4);
  }
  return s;
}

export function formatDelta(before, after) {
  if (after > before) {
    return '+' + lamportsToSol(after - before);
  } else if (before > after) {
    return '-' + lamportsToSol(before - after);
  }
  return '0';
}

// Permission bitmask constants
export const PERM_BUY = 0x01;
export const PERM_SELL = 0x02;
export const PERM_BORROW = 0x04;
export const PERM_REPAY = 0x08;
export const PERM_REINVEST = 0x10;
export const PERM_MANAGE_KEYS = 0x20;
export const PERM_LIMITED_SELL = 0x40;
export const PERM_LIMITED_BORROW = 0x80;

export const PRESET_ADMIN = 0x3F;
export const PRESET_OPERATOR = 0x19;
export const PRESET_DEPOSITOR = 0x09;
export const PRESET_KEEPER = 0x10;

export function permissionsName(permissions) {
  switch (permissions) {
    case PRESET_ADMIN: return 'Admin';
    case PRESET_OPERATOR: return 'Operator';
    case PRESET_DEPOSITOR: return 'Depositor';
    case PRESET_KEEPER: return 'Keeper';
    case PERM_LIMITED_SELL: return 'LimitedSell';
    case PERM_LIMITED_BORROW: return 'LimitedBorrow';
    case PERM_LIMITED_SELL | PERM_LIMITED_BORROW: return 'LimitedSellBorrow';
    case 0: case null: case undefined: return 'None';
    default: return 'Custom';
  }
}

export function permissionsClass(permissions) {
  switch (permissions) {
    case PRESET_ADMIN: return 'badge-admin';
    case PRESET_OPERATOR: return 'badge-operator';
    case PRESET_DEPOSITOR: return 'badge-depositor';
    case PRESET_KEEPER: return 'badge-keeper';
    default: return '';
  }
}

/**
 * Convert a slot count to a human-readable time estimate using Solana's ~400ms slot time.
 *
 * - < 150 slots (~1 min): show seconds, e.g. "~40s"
 * - 150-9,000 slots (~1 min - ~1 hr): show minutes, e.g. "~20m"
 * - 9,000-216,000 slots (~1 hr - ~1 day): show hours, e.g. "~6h"
 * - 216,000+ slots (~1+ day): show days, e.g. "~3d"
 */
export function slotsToHuman(slots) {
  if (!slots || slots <= 0) return '~0s';
  const totalSecs = Math.floor((slots * 400) / 1000);
  if (slots < 150) {
    return `~${totalSecs}s`;
  } else if (slots < 9000) {
    return `~${Math.max(1, Math.floor(totalSecs / 60))}m`;
  } else if (slots < 216000) {
    return `~${Math.max(1, Math.floor(totalSecs / 3600))}h`;
  } else {
    return `~${Math.max(1, Math.floor(totalSecs / 86400))}d`;
  }
}

export function explorerUrl(sig, cluster) {
  const base = 'https://explorer.solana.com/tx/' + sig;
  if (cluster === 'mainnet-beta') return base;
  if (cluster === 'devnet') return base + '?cluster=devnet';
  return base + '?cluster=custom&customUrl=' + encodeURIComponent(cluster);
}
