import { position, mfBorrowCapacity, mfDepositedShares, mfDebt, marketConfig } from '../state.js';
import { lamportsToSol, navTokenName } from '../utils.js';

export function PositionPanel() {
  const pos = position.value;
  if (!pos) return null;

  const mc = marketConfig.value;
  const tokenName = navTokenName(mc?.navMint);

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
    </div>
  );
}
