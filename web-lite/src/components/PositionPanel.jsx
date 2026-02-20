import { PublicKey } from '@solana/web3.js';
import { position, mfBorrowCapacity, mfDepositedShares, mfDebt, marketConfig } from '../state.js';
import { lamportsToSol, navTokenName, shortPubkey } from '../utils.js';

function formatLockout(secs) {
  if (secs <= 0) return '0';
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  if (days > 0) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (hours > 0) return `${hours}h`;
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
      {hasRecovery && (
        <div class="data-row">
          <span class="label">Recovery</span>
          <span class="value">
            {formatLockout(pos.recoveryLockoutSecs)} lockout
            {pos.recoveryConfigLocked ? ' (locked)' : ''}
            {' \u2014 '}
            {shortPubkey(pos.recoveryAsset.toString())}
          </span>
        </div>
      )}
    </div>
  );
}
