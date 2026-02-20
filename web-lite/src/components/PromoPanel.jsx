import { discoveredPromos, canManagePromos } from '../state.js';
import { shortPubkey, permissionsName, lamportsToSol, formatSolAmount, slotsToDuration } from '../utils.js';

export function PromoPanel({ onAction }) {
  if (!canManagePromos.value) return null;

  const promos = discoveredPromos.value;

  return (
    <div class="card">
      <h2>Promos</h2>

      {promos.length === 0 ? (
        <p style={{ color: 'var(--text-dim)', fontSize: '12px', marginBottom: '10px' }}>
          No promos configured for this position.
        </p>
      ) : (
        <table class="keyring-table" style={{ marginBottom: '10px' }}>
          <thead>
            <tr>
              <th>Name</th>
              <th>Status</th>
              <th>Claims</th>
              <th>Permissions</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {promos.map((promo, i) => {
              const c = promo.config;
              const claimsStr = c.maxClaims === 0
                ? `${c.claimsCount} / unlimited`
                : `${c.claimsCount} / ${c.maxClaims}`;

              return [
                <tr key={i}>
                  <td style={{ fontSize: '13px' }}>
                    {c.nameSuffix || <span style={{ color: 'var(--text-dim)' }}>--</span>}
                  </td>
                  <td>
                    <span class={`badge ${c.active ? 'badge-active' : 'badge-paused'}`}>
                      {c.active ? 'Active' : 'Paused'}
                    </span>
                  </td>
                  <td style={{ fontSize: '12px' }}>{claimsStr}</td>
                  <td style={{ fontSize: '12px' }}>{permissionsName(c.permissions)}</td>
                  <td>
                    <button
                      class="btn"
                      style={{ padding: '2px 8px', fontSize: '11px', marginRight: '4px' }}
                      onClick={() => onAction('togglePromo', { promoIndex: i })}
                    >
                      {c.active ? 'Pause' : 'Resume'}
                    </button>
                    <button
                      class="btn"
                      style={{ padding: '2px 8px', fontSize: '11px' }}
                      onClick={() => onAction('editPromoMaxClaims', { promoIndex: i })}
                    >
                      Max Claims
                    </button>
                  </td>
                </tr>,
                <tr key={`${i}-detail`} class="sub-row">
                  <td colspan="5" style={{ color: 'var(--text-dim)', fontSize: '12px', paddingLeft: '24px' }}>
                    <div>Min Deposit: {lamportsToSol(c.minDepositLamports)} SOL</div>
                    {c.borrowCapacity > 0 && (
                      <div>Borrow: {formatSolAmount(c.borrowCapacity)} SOL / {slotsToDuration(c.borrowRefillPeriod)}</div>
                    )}
                    {c.sellCapacity > 0 && (
                      <div>Sell: {formatSolAmount(c.sellCapacity)} navSOL / {slotsToDuration(c.sellRefillPeriod)}</div>
                    )}
                    {c.imageUri && (
                      <div>Image: {c.imageUri.length > 40 ? c.imageUri.slice(0, 40) + '...' : c.imageUri}</div>
                    )}
                    <div>PDA: {shortPubkey(promo.pda)}</div>
                  </td>
                </tr>,
              ];
            })}
          </tbody>
        </table>
      )}

      <button
        class="btn btn-primary"
        style={{ fontSize: '12px', padding: '6px 14px' }}
        onClick={() => onAction('createPromo')}
      >
        Create Promo
      </button>
    </div>
  );
}
