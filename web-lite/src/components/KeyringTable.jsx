import { keyring, myNftMint } from '../state.js';
import { shortPubkey, permissionsName, permissionsClass } from '../utils.js';

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
            <th>Mint</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          {keys.map((k, i) => (
            <tr key={i}>
              <td>
                <span class={`badge ${permissionsClass(k.permissions)}`}>
                  {permissionsName(k.permissions)}
                </span>
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
          ))}
        </tbody>
      </table>
    </div>
  );
}
