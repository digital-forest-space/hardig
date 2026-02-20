import { PublicKey } from '@solana/web3.js';
import { position, mfBorrowCapacity, mfDepositedShares, mfDebt, marketConfig } from '../state.js';
import { lamportsToSol, navTokenName, shortPubkey } from '../utils.js';

function formatDuration(secs) {
  if (secs <= 0) return '0s';
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (days > 0) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (hours > 0) return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
  if (mins > 0) return `${mins}m`;
  return `${secs}s`;
}

export function PositionPanel() {
  const pos = position.value;
  if (!pos) return null;

  const mc = marketConfig.value;
  const tokenName = navTokenName(mc?.navMint);
  const hasRecovery = pos.recoveryAsset && !pos.recoveryAsset.equals(PublicKey.default);

  return (
    <div class="card">
      <h2>Position</h2>
      <div class="data-row">
        <span class="label">Deposited</span>
        <span class="value">{lamportsToSol(mfDepositedShares.value)} {tokenName}</span>
      </div>
      <div class="data-row">
        <span class="label">Debt</span>
        <span class={`value ${mfDebt.value > 0 ? 'negative' : ''}`}>
          {lamportsToSol(mfDebt.value)} SOL
        </span>
      </div>
      <div class="data-row">
        <span class="label">Borrow Capacity</span>
        <span class="value positive">
          {lamportsToSol(mfBorrowCapacity.value)} SOL
        </span>
      </div>
      {pos.lastAdminActivity > 0 && (() => {
        const now = Math.floor(Date.now() / 1000);
        const ago = now - pos.lastAdminActivity;
        const remaining = hasRecovery ? pos.recoveryLockoutSecs - ago : null;
        return (
          <div class="data-row">
            <span class="label">Last Activity</span>
            <span class="value">
              {formatDuration(ago)} ago
              {hasRecovery && remaining > 0 && (
                <span> &mdash; recoverable in {formatDuration(remaining)}</span>
              )}
              {hasRecovery && remaining <= 0 && (
                <span class="negative"> &mdash; RECOVERABLE NOW</span>
              )}
            </span>
          </div>
        );
      })()}
      {hasRecovery && (
        <div class="data-row">
          <span class="label">Recovery</span>
          <span class="value">
            {formatDuration(pos.recoveryLockoutSecs)} grace
            {pos.recoveryConfigLocked ? ' (locked)' : ''}
            {' \u2014 '}
            {shortPubkey(pos.recoveryAsset.toString())}
          </span>
        </div>
      )}
    </div>
  );
}
