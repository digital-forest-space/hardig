import {
  canBuy,
  canSell,
  canBorrow,
  canRepay,
  canReinvest,
  canAuthorize,
  canRevoke,
  canInitProtocol,
  canCreatePosition,
  positionPda,
  refreshing,
} from '../state.js';

export function ActionBar({ onAction }) {
  const hasPosition = positionPda.value !== null;

  return (
    <div class="action-bar">
      {canInitProtocol.value && (
        <button class="primary" onClick={() => onAction('initProtocol')}>
          Init Protocol
        </button>
      )}
      {canCreatePosition.value && (
        <button class="primary" onClick={() => onAction('createPosition')}>
          New Position
        </button>
      )}
      {hasPosition && (
        <>
          <button disabled={!canBuy.value} onClick={() => onAction('buy')}>
            Buy
          </button>
          <button disabled={!canSell.value} onClick={() => onAction('sell')}>
            Sell
          </button>
          <button disabled={!canBorrow.value} onClick={() => onAction('borrow')}>
            Borrow
          </button>
          <button disabled={!canRepay.value} onClick={() => onAction('repay')}>
            Repay
          </button>
          <button disabled={!canReinvest.value} onClick={() => onAction('reinvest')}>
            Reinvest
          </button>
          <button disabled={!canAuthorize.value} onClick={() => onAction('authorize')}>
            Auth Key
          </button>
          <button disabled={!canRevoke.value} onClick={() => onAction('revoke')}>
            Revoke Key
          </button>
        </>
      )}
      <button
        disabled={refreshing.value}
        onClick={() => onAction('refresh')}
        style={{ marginLeft: 'auto' }}
      >
        {refreshing.value ? 'Refreshing...' : 'Refresh'}
      </button>
    </div>
  );
}
