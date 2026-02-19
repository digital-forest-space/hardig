import { mayflowerInitialized } from '../state.js';

export function MayflowerPanel() {
  return (
    <div class="card">
      <h2>Nirvana</h2>
      <div class="data-row">
        <span class="label">Position Init</span>
        <span class={`value ${mayflowerInitialized.value ? 'positive' : 'negative'}`}>
          {mayflowerInitialized.value ? 'Yes' : 'No'}
        </span>
      </div>
    </div>
  );
}
