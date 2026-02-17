import { useWallet } from '@solana/wallet-adapter-react';
import {
  protocolExists,
  positionPda,
  myRole,
  connected,
} from '../state.js';
import { roleName } from '../utils.js';
import { PositionPanel } from './PositionPanel.jsx';
import { MayflowerPanel } from './MayflowerPanel.jsx';
import { KeyringTable } from './KeyringTable.jsx';
import { ActionBar } from './ActionBar.jsx';
import { LogPanel } from './LogPanel.jsx';

export function Dashboard({ onAction }) {
  const wallet = useWallet();

  if (!wallet.connected) {
    return (
      <div class="status-msg">
        <h2>Connect Wallet</h2>
        <p>Connect a wallet to view your positions.</p>
      </div>
    );
  }

  if (!protocolExists.value) {
    return (
      <div>
        <div class="status-msg">
          <h2>Protocol Not Initialized</h2>
          <p>The Hardig protocol config hasn't been created on this cluster yet.</p>
        </div>
        <ActionBar onAction={onAction} />
        <LogPanel />
      </div>
    );
  }

  if (positionPda.value === null) {
    return (
      <div>
        <div class="status-msg">
          <h2>No Position Found</h2>
          <p>No position NFT found in your wallet. Create a new position to get started.</p>
        </div>
        <ActionBar onAction={onAction} />
        <LogPanel />
      </div>
    );
  }

  return (
    <div>
      <div class="card" style={{ marginBottom: '12px' }}>
        <div class="data-row">
          <span class="label">Your Role</span>
          <span class="value">
            <span class={`badge badge-${roleName(myRole.value).toLowerCase()}`}>
              {roleName(myRole.value)}
            </span>
          </span>
        </div>
      </div>
      <ActionBar onAction={onAction} />
      <PositionPanel />
      <MayflowerPanel />
      <KeyringTable />
      <LogPanel />
    </div>
  );
}
