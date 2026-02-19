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
  if (permissions === null || permissions === undefined || permissions === 0) return 'None';
  if (permissions === PRESET_ADMIN) return 'Admin';
  const bits = [
    [PERM_BUY, 'Buy'],
    [PERM_SELL, 'Sell'],
    [PERM_BORROW, 'Borrow'],
    [PERM_REPAY, 'Repay'],
    [PERM_REINVEST, 'Reinvest'],
    [PERM_MANAGE_KEYS, 'ManageKeys'],
    [PERM_LIMITED_SELL, 'LimSell'],
    [PERM_LIMITED_BORROW, 'LimBorrow'],
  ];
  const names = bits.filter(([bit]) => (permissions & bit) !== 0).map(([, name]) => name);
  return names.length > 0 ? names.join(', ') : 'None';
}

export function permissionsClass(permissions) {
  if (permissions === PRESET_ADMIN) return 'badge-admin';
  if (permissions === PRESET_OPERATOR) return 'badge-operator';
  if (permissions === PRESET_DEPOSITOR) return 'badge-depositor';
  if (permissions === PRESET_KEEPER) return 'badge-keeper';
  return '';
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

/**
 * Convert a slot count to a human-readable duration matching on-chain format.
 * NOTE: Must match programs/hardig/src/instructions/mod.rs slots_to_duration().
 * Examples: "30 days", "1 day, 12 hours", "6 hours", "45 minutes".
 */
export function slotsToDuration(slots) {
  if (!slots || slots <= 0) return '1 minute';
  const totalSecs = Math.floor((slots * 400) / 1000);
  const days = Math.floor(totalSecs / 86400);
  const hours = Math.floor((totalSecs % 86400) / 3600);
  const minutes = Math.floor((totalSecs % 3600) / 60);
  const parts = [];
  if (days > 0) parts.push(days === 1 ? '1 day' : `${days} days`);
  if (hours > 0) parts.push(hours === 1 ? '1 hour' : `${hours} hours`);
  if (parts.length === 0) {
    const m = Math.max(1, minutes);
    parts.push(m === 1 ? '1 minute' : `${m} minutes`);
  }
  return parts.join(', ');
}

/**
 * Format a raw u64 amount (lamports/shares, 9 decimals) as a clean string.
 * NOTE: Must match programs/hardig/src/instructions/mod.rs format_sol_amount().
 */
export function formatSolAmount(raw) {
  if (typeof raw === 'bigint') raw = Number(raw);
  const whole = Math.floor(raw / 1_000_000_000);
  const frac = raw % 1_000_000_000;
  if (frac === 0) return String(whole);
  const fracStr = String(frac).padStart(9, '0').replace(/0+$/, '');
  return `${whole}.${fracStr}`;
}

export function explorerUrl(sig, cluster) {
  const base = 'https://explorer.solana.com/tx/' + sig;
  if (cluster === 'mainnet-beta') return base;
  if (cluster === 'devnet') return base + '?cluster=devnet';
  return base + '?cluster=custom&customUrl=' + encodeURIComponent(cluster);
}
