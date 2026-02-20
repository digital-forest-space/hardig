import { discoveredPositions, activePositionIndex } from '../state.js';
import { shortPubkey, lamportsToSol, permissionsName, PRESET_ADMIN } from '../utils.js';

export function PositionSelector({ onSwitch }) {
  const positions = discoveredPositions.value;
  if (positions.length <= 1) return null;

  return (
    <div class="card" style={{ marginBottom: '12px' }}>
      <div class="data-row">
        <span class="label">Position</span>
        <span class="value">
          <select
            value={activePositionIndex.value}
            onChange={(e) => {
              const idx = parseInt(e.target.value, 10);
              if (!isNaN(idx) && idx !== activePositionIndex.value) {
                onSwitch(idx);
              }
            }}
            style={{
              padding: '4px 8px',
              borderRadius: '4px',
              border: '1px solid #555',
              background: '#1a1a2e',
              color: '#e0e0e0',
              fontSize: '14px',
            }}
          >
            {positions.map((dp, i) => {
              const role = dp.isAdmin ? 'Admin' : permissionsName(dp.permissions);
              const label = `${shortPubkey(dp.adminAsset)} — ${role} — ${lamportsToSol(dp.depositedNav)} deposited`;
              return (
                <option key={i} value={i}>
                  {label}
                </option>
              );
            })}
          </select>
        </span>
      </div>
    </div>
  );
}
