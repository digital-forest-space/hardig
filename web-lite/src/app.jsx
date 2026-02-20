import { useState, useEffect, useCallback, useMemo } from 'preact/hooks';
import {
  ConnectionProvider,
  WalletProvider,
  useConnection,
  useWallet,
} from '@solana/wallet-adapter-react';
import { WalletModalProvider } from '@solana/wallet-adapter-react-ui';
import { getClusterUrl } from './rpc.js';
import {
  cluster,
  customUrl,
  connected,
  refreshing,
  pushLog,
  resetPositionState,
  protocolExists,
} from './state.js';
import { checkProtocol, discoverPosition, selectActivePosition } from './discovery.js';
import { refreshMayflowerState } from './mayflower.js';
import { loadMarkets } from './markets.js';
import { WalletButton } from './components/WalletButton.jsx';
import { ClusterSelector } from './components/ClusterSelector.jsx';
import { Dashboard } from './components/Dashboard.jsx';
import { ActionModal } from './components/ActionModal.jsx';

function AppInner() {
  const { connection } = useConnection();
  const wallet = useWallet();
  const [activeAction, setActiveAction] = useState(null);

  const doRefresh = useCallback(async () => {
    if (!wallet.publicKey) return;
    refreshing.value = true;
    pushLog('Refreshing...');
    try {
      await loadMarkets(connection);
      await checkProtocol(connection);
      await discoverPosition(connection, wallet.publicKey);
      await refreshMayflowerState(connection);
      pushLog('Refresh complete.');
    } catch (e) {
      pushLog('Refresh error: ' + e.message, true);
    }
    refreshing.value = false;
  }, [connection, wallet.publicKey]);

  // Refresh on wallet connect/disconnect
  useEffect(() => {
    if (wallet.connected && wallet.publicKey) {
      connected.value = true;
      doRefresh();
    } else {
      connected.value = false;
      resetPositionState();
      protocolExists.value = false;
    }
  }, [wallet.connected, wallet.publicKey, doRefresh]);

  // Refresh on cluster change
  useEffect(() => {
    if (wallet.connected && wallet.publicKey) {
      resetPositionState();
      doRefresh();
    }
  }, [cluster.value, customUrl.value]);

  const handleAction = (action, data) => {
    if (action === 'refresh') {
      doRefresh();
    } else {
      setActiveAction(data ? { type: action, ...data } : action);
    }
  };

  const handleSwitchPosition = useCallback(async (index) => {
    refreshing.value = true;
    try {
      await selectActivePosition(index, connection);
      await refreshMayflowerState(connection);
      pushLog('Switched to position ' + (index + 1));
    } catch (e) {
      pushLog('Switch error: ' + e.message, true);
    }
    refreshing.value = false;
  }, [connection]);

  return (
    <div>
      <div class="header">
        <h1>Härdig</h1>
        <div class="header-controls">
          <ClusterSelector />
          <WalletButton />
        </div>
      </div>

      <Dashboard onAction={handleAction} onSwitchPosition={handleSwitchPosition} />

      {activeAction && (
        <ActionModal
          action={typeof activeAction === 'string' ? activeAction : activeAction.type}
          actionData={typeof activeAction === 'object' ? activeAction : null}
          onClose={() => setActiveAction(null)}
          onRefresh={doRefresh}
        />
      )}
    </div>
  );
}

export function App() {
  const endpoint = useMemo(() => {
    if (cluster.value === 'custom') {
      return customUrl.value || 'http://localhost:8899';
    }
    return getClusterUrl(cluster.value);
  }, [cluster.value, customUrl.value]);

  const wallets = useMemo(() => [], []);

  return (
    <ConnectionProvider endpoint={endpoint}>
      <WalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>
          <AppInner />
        </WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}
