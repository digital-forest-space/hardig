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

export function roleName(roleValue) {
  switch (roleValue) {
    case 0: return 'Admin';
    case 1: return 'Operator';
    case 2: return 'Depositor';
    case 3: return 'Keeper';
    default: return 'Unknown';
  }
}

export function roleClass(roleValue) {
  switch (roleValue) {
    case 0: return 'badge-admin';
    case 1: return 'badge-operator';
    case 2: return 'badge-depositor';
    case 3: return 'badge-keeper';
    default: return '';
  }
}

export function explorerUrl(sig, cluster) {
  const base = 'https://explorer.solana.com/tx/' + sig;
  if (cluster === 'mainnet-beta') return base;
  if (cluster === 'devnet') return base + '?cluster=devnet';
  return base + '?cluster=custom&customUrl=' + encodeURIComponent(cluster);
}
