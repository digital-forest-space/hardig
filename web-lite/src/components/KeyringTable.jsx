import { keyring, myNftMint } from '../state.js';
import { shortPubkey, permissionsName, formatSolAmount, slotsToDuration, PRESET_ADMIN } from '../utils.js';

export function KeyringTable() {
  const keys = keyring.value;
  if (keys.length === 0) return null;

  return (
    <div class="card">
      <h2>Keyring</h2>
      <table class="keyring-table">
        <thead>
          <tr>
            <th>Role</th>
            <th>Name</th>
            <th>Asset</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          {keys.map((k, i) => {
            const isAdmin = k.permissions === PRESET_ADMIN;
            const rows = [
              <tr key={i}>
                <td>
                  <span class={`badge ${isAdmin ? 'badge-admin' : ''}`}>
                    {isAdmin ? 'Admin' : 'Delegated'}
                  </span>
                </td>
                <td style={{ fontSize: '13px' }}>
                  {k.name || <span style={{ color: 'var(--text-dim)' }}>--</span>}
                </td>
                <td>{shortPubkey(k.mint)}</td>
                <td>
                  {k.heldBySigner ? (
                    <span class="badge badge-held">HELD</span>
                  ) : (
                    <span style={{ color: 'var(--text-dim)' }}>--</span>
                  )}
                </td>
              </tr>
            ];
            if (!isAdmin) {
              const details = [permissionsName(k.permissions)];
              if (k.sellBucket) {
                details.push(`Sell: ${formatSolAmount(k.sellBucket.capacity)} navSOL / ${slotsToDuration(k.sellBucket.refillPeriod)}`);
              }
              if (k.borrowBucket) {
                details.push(`Borrow: ${formatSolAmount(k.borrowBucket.capacity)} SOL / ${slotsToDuration(k.borrowBucket.refillPeriod)}`);
              }
              rows.push(
                <tr key={`${i}-detail`} class="sub-row">
                  <td colspan="4" style={{ color: 'var(--text-dim)', fontSize: '12px', paddingLeft: '24px' }}>
                    {details.map((d, j) => <div key={j}>{d}</div>)}
                  </td>
                </tr>
              );
            }
            return rows;
          })}
        </tbody>
      </table>
    </div>
  );
}
