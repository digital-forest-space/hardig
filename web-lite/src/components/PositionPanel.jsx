import { position, mfBorrowCapacity, mfDepositedShares, mfDebt } from '../state.js';
import { lamportsToSol } from '../utils.js';

export function PositionPanel() {
  const pos = position.value;
  if (!pos) return null;

  return (
    <div class="card">
      <h2>Position</h2>
      <div class="data-row">
        <span class="label">Deposited (local)</span>
        <span class="value">{lamportsToSol(pos.depositedNav)} SOL</span>
      </div>
      <div class="data-row">
        <span class="label">Deposited (Mayflower)</span>
        <span class="value">{lamportsToSol(mfDepositedShares.value)} shares</span>
      </div>
      <div class="data-row">
        <span class="label">User Debt</span>
        <span class={`value ${pos.userDebt > 0 ? 'negative' : ''}`}>
          {lamportsToSol(pos.userDebt)} SOL
        </span>
      </div>
      <div class="data-row">
        <span class="label">Protocol Debt</span>
        <span class={`value ${pos.protocolDebt > 0 ? 'negative' : ''}`}>
          {lamportsToSol(pos.protocolDebt)} SOL
        </span>
      </div>
      <div class="data-row">
        <span class="label">Mayflower Debt</span>
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
