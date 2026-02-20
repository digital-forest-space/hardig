import { useWallet } from '@solana/wallet-adapter-react';
import {
  protocolExists,
  myPermissions,
  discoveredPositions,
} from '../state.js';
import { permissionsName, permissionsClass } from '../utils.js';
import { PositionPanel } from './PositionPanel.jsx';
import { KeyringTable } from './KeyringTable.jsx';
import { ActionBar } from './ActionBar.jsx';
import { LogPanel } from './LogPanel.jsx';
import { PositionSelector } from './PositionSelector.jsx';

export function Dashboard({ onAction, onSwitchPosition }) {
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

  if (discoveredPositions.value.length === 0) {
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
            <span class={`badge ${permissionsClass(myPermissions.value)}`}>
              {permissionsName(myPermissions.value)}
            </span>
          </span>
        </div>
      </div>
      <PositionSelector onSwitch={onSwitchPosition} />
      <ActionBar onAction={onAction} />
      <PositionPanel />
      <KeyringTable />
      <LogPanel />
    </div>
  );
}
