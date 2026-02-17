import {
  mayflowerInitialized,
  atasExist,
  wsolBalance,
  navSolBalance,
} from '../state.js';
import { lamportsToSol } from '../utils.js';

export function MayflowerPanel() {
  return (
    <div class="card">
      <h2>Mayflower</h2>
      <div class="data-row">
        <span class="label">Position Init</span>
        <span class={`value ${mayflowerInitialized.value ? 'positive' : 'negative'}`}>
          {mayflowerInitialized.value ? 'Yes' : 'No'}
        </span>
      </div>
      <div class="data-row">
        <span class="label">ATAs Created</span>
        <span class={`value ${atasExist.value ? 'positive' : 'negative'}`}>
          {atasExist.value ? 'Yes' : 'No'}
        </span>
      </div>
      {atasExist.value && (
        <>
          <div class="data-row">
            <span class="label">wSOL Balance</span>
            <span class="value">{lamportsToSol(wsolBalance.value)} SOL</span>
          </div>
          <div class="data-row">
            <span class="label">navSOL Balance</span>
            <span class="value">{lamportsToSol(navSolBalance.value)}</span>
          </div>
        </>
      )}
    </div>
  );
}
