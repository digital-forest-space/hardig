import { useState, useEffect, useRef } from 'preact/hooks';
import { useConnection, useWallet } from '@solana/wallet-adapter-react';
import { Transaction } from '@solana/web3.js';
import { getProgram } from '../program.js';
import {
  position,
  keyring,
  pushLog,
  takeSnapshot,
  preTxSnapshot,
  lastTxSignature,
  cluster,
  customUrl,
} from '../state.js';
import {
  buildInitializeProtocol,
  buildCreatePosition,
  buildSetup,
  buildBuy,
  buildWithdraw,
  buildBorrow,
  buildRepay,
  buildReinvest,
  buildAuthorizeKey,
  buildRevokeKey,
} from '../instructions/index.js';
import { parseSolToLamports, lamportsToSol, shortPubkey, formatDelta, roleName, explorerUrl } from '../utils.js';

// Phase: form | building | confirm | result
export function ActionModal({ action, onClose, onRefresh }) {
  const { connection } = useConnection();
  const wallet = useWallet();
  const [phase, setPhase] = useState('form');
  const [pending, setPending] = useState(null);
  const [sending, setSending] = useState(false);
  const [result, setResult] = useState(null);
  const [error, setError] = useState(null);
  const didAutoSubmit = useRef(false);

  // Form fields
  const [amount, setAmount] = useState('');
  const [targetWallet, setTargetWallet] = useState('');
  const [role, setRole] = useState('1');
  const [revokeIdx, setRevokeIdx] = useState('0');

  const walletPk = wallet.publicKey;

  const getClusterName = () => {
    if (cluster.value === 'custom') return customUrl.value || 'http://localhost:8899';
    return cluster.value;
  };

  async function handleSubmit() {
    setError(null);
    setPhase('building');
    try {
      const program = getProgram(connection, wallet);
      let built;
      switch (action) {
        case 'initProtocol':
          built = await buildInitializeProtocol(program, walletPk);
          break;
        case 'createPosition':
          built = await buildCreatePosition(program, walletPk);
          break;
        case 'setup':
          built = await buildSetup(program, walletPk);
          if (!built) {
            pushLog('Setup already complete');
            onClose();
            return;
          }
          break;
        case 'buy': {
          const lam = parseSolToLamports(amount);
          if (!lam) { setError('Invalid SOL amount'); setPhase('form'); return; }
          built = await buildBuy(program, walletPk, lam);
          break;
        }
        case 'sell': {
          const lam = parseSolToLamports(amount);
          if (!lam) { setError('Invalid SOL amount'); setPhase('form'); return; }
          built = await buildWithdraw(program, walletPk, lam);
          break;
        }
        case 'borrow': {
          const lam = parseSolToLamports(amount);
          if (!lam) { setError('Invalid SOL amount'); setPhase('form'); return; }
          built = await buildBorrow(program, walletPk, lam);
          break;
        }
        case 'repay': {
          const lam = parseSolToLamports(amount);
          if (!lam) { setError('Invalid SOL amount'); setPhase('form'); return; }
          built = await buildRepay(program, walletPk, lam);
          break;
        }
        case 'reinvest':
          built = await buildReinvest(program, walletPk);
          break;
        case 'authorize': {
          if (!targetWallet.trim()) { setError('Target wallet required'); setPhase('form'); return; }
          const r = parseInt(role);
          if (r === 0) { setError('Cannot create a second admin key'); setPhase('form'); return; }
          built = await buildAuthorizeKey(program, walletPk, targetWallet.trim(), r);
          break;
        }
        case 'revoke': {
          const revocable = keyring.value.filter((k) => k.role !== 0);
          const idx = parseInt(revokeIdx);
          if (idx < 0 || idx >= revocable.length) { setError('Invalid key index'); setPhase('form'); return; }
          built = await buildRevokeKey(program, walletPk, revocable[idx]);
          break;
        }
        default:
          setError('Unknown action: ' + action);
          return;
      }
      setPending(built);
      setPhase('confirm');
    } catch (e) {
      const msg = e.message || String(e);
      pushLog('Build error: ' + msg, true);
      setError(msg);
      setPhase('form');
    }
  }

  // Auto-submit for no-form actions (useEffect, not during render)
  const noFormActions = ['initProtocol', 'reinvest', 'setup'];
  useEffect(() => {
    if (noFormActions.includes(action) && !didAutoSubmit.current) {
      didAutoSubmit.current = true;
      handleSubmit();
    }
  }, [action]);

  async function handleConfirm() {
    if (!pending) return;
    setSending(true);
    setError(null);

    const snap = takeSnapshot();
    preTxSnapshot.value = snap;

    try {
      const tx = new Transaction();
      for (const ix of pending.instructions) {
        tx.add(ix);
      }

      const { blockhash, lastValidBlockHeight } =
        await connection.getLatestBlockhash('confirmed');
      tx.recentBlockhash = blockhash;
      tx.feePayer = walletPk;

      if (pending.extraSigners.length > 0) {
        tx.partialSign(...pending.extraSigners);
      }

      // Sign with wallet (separate from send for better errors)
      const signed = await wallet.signTransaction(tx);

      // Send raw and get detailed RPC errors
      const sig = await connection.sendRawTransaction(signed.serialize(), {
        skipPreflight: false,
      });

      await connection.confirmTransaction(
        { signature: sig, blockhash, lastValidBlockHeight },
        'confirmed'
      );

      pushLog('TX confirmed: ' + sig);
      lastTxSignature.value = sig;

      await onRefresh();
      setResult({ sig, snapshot: snap });
      setPhase('result');
    } catch (e) {
      let msg = e.message || String(e);
      // Extract RPC simulation logs if available
      if (e.logs) {
        const logStr = e.logs.join('\n');
        pushLog('TX logs:\n' + logStr, true);
        msg += '\n' + logStr;
      }
      pushLog('TX failed: ' + msg, true);
      setError(msg);
      preTxSnapshot.value = null;
      setSending(false);
    }
  }

  const titles = {
    initProtocol: 'Initialize Protocol',
    createPosition: 'Create Position',
    setup: 'Setup Mayflower',
    buy: 'Buy navSOL',
    sell: 'Sell navSOL',
    borrow: 'Borrow SOL',
    repay: 'Repay SOL',
    reinvest: 'Reinvest',
    authorize: 'Authorize Key',
    revoke: 'Revoke Key',
  };

  return (
    <div class="modal-overlay" onClick={onClose}>
      <div class="modal" onClick={(e) => e.stopPropagation()}>
        <h3>{titles[action]}</h3>

        {error && (
          <div style={{
            background: 'rgba(248,113,113,0.1)',
            border: '1px solid var(--red)',
            borderRadius: '4px',
            padding: '8px 10px',
            marginBottom: '12px',
            fontSize: '12px',
            color: 'var(--red)',
            wordBreak: 'break-all',
          }}>
            {error}
          </div>
        )}

        {phase === 'building' && (
          <p><span class="spinner" /> Building transaction...</p>
        )}

        {phase === 'form' && !noFormActions.includes(action) && (
          <div>
            {(action === 'buy' || action === 'sell' || action === 'borrow' || action === 'repay') && (
              <div class="form-group">
                <label>Amount (SOL)</label>
                <input
                  type="text"
                  value={amount}
                  onInput={(e) => setAmount(e.target.value)}
                  placeholder={
                    action === 'sell'
                      ? lamportsToSol(position.value?.depositedNav || 0)
                      : action === 'repay'
                      ? lamportsToSol(position.value?.userDebt || 0)
                      : '0.0'
                  }
                  autoFocus
                />
              </div>
            )}


            {action === 'authorize' && (
              <>
                <div class="form-group">
                  <label>Target Wallet (pubkey)</label>
                  <input
                    type="text"
                    value={targetWallet}
                    onInput={(e) => setTargetWallet(e.target.value)}
                    placeholder="Enter wallet address..."
                    autoFocus
                  />
                </div>
                <div class="form-group">
                  <label>Role</label>
                  <select value={role} onChange={(e) => setRole(e.target.value)}>
                    <option value="1">Operator</option>
                    <option value="2">Depositor</option>
                    <option value="3">Keeper</option>
                  </select>
                </div>
              </>
            )}

            {action === 'revoke' && (
              <div class="form-group">
                <label>Key to Revoke</label>
                <select value={revokeIdx} onChange={(e) => setRevokeIdx(e.target.value)}>
                  {keyring.value
                    .filter((k) => k.role !== 0)
                    .map((k, i) => (
                      <option key={i} value={i}>
                        {shortPubkey(k.mint)} ({roleName(k.role)})
                      </option>
                    ))}
                </select>
              </div>
            )}

            <div class="btn-row">
              <button class="btn" onClick={onClose}>Cancel</button>
              <button class="btn btn-primary" onClick={handleSubmit}>Continue</button>
            </div>
          </div>
        )}

        {/* No-form actions that errored back to 'form' — show retry */}
        {phase === 'form' && noFormActions.includes(action) && error && (
          <div class="btn-row">
            <button class="btn" onClick={onClose}>Cancel</button>
            <button class="btn btn-primary" onClick={() => { didAutoSubmit.current = false; setError(null); handleSubmit(); }}>
              Retry
            </button>
          </div>
        )}

        {phase === 'confirm' && pending && (
          <div>
            <ul class="confirm-list">
              {pending.description.map((line, i) => (
                <li key={i}>{i === 0 ? <strong>{line}</strong> : line}</li>
              ))}
              <li>{pending.instructions.length} instruction(s)</li>
              {pending.extraSigners.length > 0 && (
                <li>{pending.extraSigners.length} extra signer(s)</li>
              )}
            </ul>
            <div class="btn-row">
              <button class="btn" onClick={onClose} disabled={sending}>Cancel</button>
              <button class="btn btn-primary" onClick={handleConfirm} disabled={sending}>
                {sending ? (<><span class="spinner" />Sending...</>) : 'Confirm'}
              </button>
            </div>
          </div>
        )}

        {phase === 'result' && result && (
          <div>
            <p style={{ marginBottom: '10px' }}>Transaction confirmed.</p>
            <p>
              <a
                class="tx-link"
                href={explorerUrl(result.sig, getClusterName())}
                target="_blank"
                rel="noopener noreferrer"
              >
                {result.sig}
              </a>
            </p>

            {result.snapshot && position.value && (
              <table class="result-table">
                <thead>
                  <tr>
                    <th>Field</th>
                    <th>Before</th>
                    <th>After</th>
                    <th>Delta</th>
                  </tr>
                </thead>
                <tbody>
                  {[
                    ['Deposited', result.snapshot.depositedNav, position.value.depositedNav],
                    ['User Debt', result.snapshot.userDebt, position.value.userDebt],
                    ['Protocol Debt', result.snapshot.protocolDebt, position.value.protocolDebt],
                  ].map(([label, before, after]) => {
                    const delta = formatDelta(before, after);
                    const cls = delta.startsWith('+') ? 'positive' : delta.startsWith('-') ? 'negative' : '';
                    return (
                      <tr key={label}>
                        <th>{label}</th>
                        <td>{lamportsToSol(before)}</td>
                        <td>{lamportsToSol(after)}</td>
                        <td class={`delta ${cls}`}>{delta}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}

            <div class="btn-row">
              <button class="btn btn-primary" onClick={onClose}>Done</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
