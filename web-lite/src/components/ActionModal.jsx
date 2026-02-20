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
  marketConfig,
  discoveredPromos,
} from '../state.js';
import {
  buildInitializeProtocol,
  buildCreatePosition,
  buildCreateMarketConfig,
  buildBuy,
  buildWithdraw,
  buildBorrow,
  buildRepay,
  buildReinvest,
  buildAuthorizeKey,
  buildRevokeKey,
  buildCreatePromo,
  buildUpdatePromo,
} from '../instructions/index.js';
import { availableMarkets, marketEntryToPubkeys } from '../markets.js';
import { parseSolToLamports, lamportsToSol, shortPubkey, formatDelta, permissionsName, navTokenName, explorerUrl, PERM_BUY, PERM_SELL, PERM_BORROW, PERM_REPAY, PERM_REINVEST, PERM_MANAGE_KEYS, PERM_LIMITED_SELL, PERM_LIMITED_BORROW, PRESET_OPERATOR } from '../utils.js';
import { deriveMarketConfigPda } from '../constants.js';

// Phase: form | building | confirm | result
export function ActionModal({ action, actionData, onClose, onRefresh }) {
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
  const [targetWallet, setTargetWallet] = useState(wallet.publicKey?.toBase58() || '');
  const [permissions, setPermissions] = useState(String(PRESET_OPERATOR));
  const [revokeIdx, setRevokeIdx] = useState('0');
  const [sellCapacity, setSellCapacity] = useState('');
  const [sellDays, setSellDays] = useState('');
  const [sellHours, setSellHours] = useState('');
  const [sellMinutes, setSellMinutes] = useState('');
  const [borrowCapacity, setBorrowCapacity] = useState('');
  const [borrowDays, setBorrowDays] = useState('');
  const [borrowHours, setBorrowHours] = useState('');
  const [borrowMinutes, setBorrowMinutes] = useState('');
  const [positionName, setPositionName] = useState('');
  const [keyName, setKeyName] = useState('');
  const [selectedMarketIdx, setSelectedMarketIdx] = useState('0');

  // Promo form fields
  const [promoNameSuffix, setPromoNameSuffix] = useState('');
  const [promoPermissions, setPromoPermissions] = useState(String(PERM_BUY | PERM_LIMITED_BORROW));
  const [promoBorrowCapacity, setPromoBorrowCapacity] = useState('0.05');
  const [promoBorrowDays, setPromoBorrowDays] = useState('90');
  const [promoBorrowHours, setPromoBorrowHours] = useState('');
  const [promoBorrowMinutes, setPromoBorrowMinutes] = useState('');
  const [promoSellCapacity, setPromoSellCapacity] = useState('');
  const [promoSellDays, setPromoSellDays] = useState('');
  const [promoSellHours, setPromoSellHours] = useState('');
  const [promoSellMinutes, setPromoSellMinutes] = useState('');
  const [promoMinDeposit, setPromoMinDeposit] = useState('0.02');
  const [promoMaxClaims, setPromoMaxClaims] = useState('0');
  const [promoImageUri, setPromoImageUri] = useState('');
  const [editMaxClaims, setEditMaxClaims] = useState('');

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
        case 'createPosition': {
          const pName = positionName.trim() || null;
          if (pName && pName.length > 32) { setError('Label must be 32 characters or less'); setPhase('form'); return; }
          const markets = availableMarkets.value;
          const idx = parseInt(selectedMarketIdx) || 0;
          const selectedEntry = markets.length > 0 ? markets[idx] : null;
          const mEntry = selectedEntry ? marketEntryToPubkeys(selectedEntry) : null;
          if (mEntry) {
            // Check if MarketConfig PDA exists; if not, auto-create it
            const [mcPda] = deriveMarketConfigPda(mEntry.navMint);
            const mcAcc = await connection.getAccountInfo(mcPda);
            if (!mcAcc) {
              // Build createMarketConfig IX and prepend it
              const mcBuilt = await buildCreateMarketConfig(
                program, walletPk,
                mEntry.navMint, mEntry.baseMint, mEntry.marketGroup, mEntry.marketMeta,
                mEntry.mayflowerMarket, mEntry.marketBaseVault, mEntry.marketNavVault, mEntry.feeVault
              );
              const posBuilt = await buildCreatePosition(program, walletPk, pName, mEntry);
              built = {
                description: [...mcBuilt.description, ...posBuilt.description],
                instructions: [...mcBuilt.instructions, ...posBuilt.instructions],
                extraSigners: [...mcBuilt.extraSigners, ...posBuilt.extraSigners],
              };
            } else {
              built = await buildCreatePosition(program, walletPk, pName, mEntry);
            }
          } else {
            built = await buildCreatePosition(program, walletPk, pName);
          }
          break;
        }
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
          const p = parseInt(permissions);
          if (p === 0) { setError('Permissions cannot be zero'); setPhase('form'); return; }
          if (p & PERM_MANAGE_KEYS) { setError('Cannot grant PERM_MANAGE_KEYS to delegated keys'); setPhase('form'); return; }
          const toSlots = (d, h, m) => (parseInt(d) || 0) * 216000 + (parseInt(h) || 0) * 9000 + (parseInt(m) || 0) * 150;
          const sc = (p & PERM_LIMITED_SELL) ? (parseSolToLamports(sellCapacity) || 0) : 0;
          const sr = (p & PERM_LIMITED_SELL) ? toSlots(sellDays, sellHours, sellMinutes) : 0;
          const bc = (p & PERM_LIMITED_BORROW) ? (parseSolToLamports(borrowCapacity) || 0) : 0;
          const br = (p & PERM_LIMITED_BORROW) ? toSlots(borrowDays, borrowHours, borrowMinutes) : 0;
          if ((p & PERM_LIMITED_SELL) && (sc === 0 || sr === 0)) { setError('Sell capacity and refill period must be nonzero'); setPhase('form'); return; }
          if ((p & PERM_LIMITED_BORROW) && (bc === 0 || br === 0)) { setError('Borrow capacity and refill period must be nonzero'); setPhase('form'); return; }
          const kName = keyName.trim() || null;
          if (kName && kName.length > 32) { setError('Label must be 32 characters or less'); setPhase('form'); return; }
          built = await buildAuthorizeKey(program, walletPk, targetWallet.trim(), p, sc, sr, bc, br, kName);
          break;
        }
        case 'revoke': {
          const adminMint = position.value?.currentAdminAsset || position.value?.adminAsset;
          const revocable = keyring.value.filter((k) => !adminMint || !k.mint.equals(adminMint));
          const idx = parseInt(revokeIdx);
          if (idx < 0 || idx >= revocable.length) { setError('Invalid key index'); setPhase('form'); return; }
          built = await buildRevokeKey(program, walletPk, revocable[idx]);
          break;
        }
        case 'createPromo': {
          const ns = promoNameSuffix.trim();
          if (!ns) { setError('Name suffix is required'); setPhase('form'); return; }
          if (ns.length > 64) { setError('Name suffix must be 64 characters or less'); setPhase('form'); return; }
          const pp = parseInt(promoPermissions) || 0;
          if (pp === 0) { setError('Permissions cannot be zero'); setPhase('form'); return; }
          const toSlots = (d, h, m) => (parseInt(d) || 0) * 216000 + (parseInt(h) || 0) * 9000 + (parseInt(m) || 0) * 150;
          const pbc = (pp & PERM_LIMITED_BORROW) ? (parseSolToLamports(promoBorrowCapacity) || 0) : 0;
          const pbr = (pp & PERM_LIMITED_BORROW) ? toSlots(promoBorrowDays, promoBorrowHours, promoBorrowMinutes) : 0;
          const psc = (pp & PERM_LIMITED_SELL) ? (parseSolToLamports(promoSellCapacity) || 0) : 0;
          const psr = (pp & PERM_LIMITED_SELL) ? toSlots(promoSellDays, promoSellHours, promoSellMinutes) : 0;
          if ((pp & PERM_LIMITED_SELL) && (psc === 0 || psr === 0)) { setError('Sell capacity and refill period must be nonzero'); setPhase('form'); return; }
          if ((pp & PERM_LIMITED_BORROW) && (pbc === 0 || pbr === 0)) { setError('Borrow capacity and refill period must be nonzero'); setPhase('form'); return; }
          const pmd = parseSolToLamports(promoMinDeposit) || 0;
          const pmc = parseInt(promoMaxClaims) || 0;
          const piu = promoImageUri.trim();
          if (piu.length > 128) { setError('Image URI must be 128 characters or less'); setPhase('form'); return; }
          built = await buildCreatePromo(program, walletPk, ns, pp, pbc, pbr, psc, psr, pmd, pmc, piu);
          break;
        }
        case 'togglePromo': {
          const pi = actionData?.promoIndex;
          const promo = discoveredPromos.value[pi];
          if (!promo) { setError('Promo not found'); setPhase('form'); return; }
          built = await buildUpdatePromo(program, walletPk, promo.pda, !promo.config.active, null);
          break;
        }
        case 'editPromoMaxClaims': {
          const pi2 = actionData?.promoIndex;
          const promo2 = discoveredPromos.value[pi2];
          if (!promo2) { setError('Promo not found'); setPhase('form'); return; }
          const newMax = parseInt(editMaxClaims);
          if (isNaN(newMax) || newMax < 0) { setError('Invalid max claims value'); setPhase('form'); return; }
          built = await buildUpdatePromo(program, walletPk, promo2.pda, null, newMax);
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
  const noFormActions = ['initProtocol', 'reinvest', 'togglePromo'];
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
    buy: 'Buy navSOL',
    sell: 'Sell navSOL',
    borrow: 'Borrow SOL',
    repay: 'Repay SOL',
    reinvest: 'Reinvest',
    authorize: 'Authorize Key',
    revoke: 'Revoke Key',
    createPromo: 'Create Promo',
    togglePromo: 'Toggle Promo',
    editPromoMaxClaims: 'Edit Max Claims',
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

            {action === 'createPosition' && (
              <>
                {availableMarkets.value.length > 0 && (
                  <div class="form-group">
                    <label>Market</label>
                    <select
                      value={selectedMarketIdx}
                      onChange={(e) => setSelectedMarketIdx(e.target.value)}
                    >
                      {availableMarkets.value.map((m, i) => (
                        <option key={i} value={i}>
                          {m.navSymbol}{m.floorPrice ? ` (floor: ${m.floorPrice.toFixed(6)})` : ''}
                        </option>
                      ))}
                    </select>
                  </div>
                )}
                <div class="form-group">
                  <label>Label (optional)</label>
                  <input
                    type="text"
                    value={positionName}
                    onInput={(e) => setPositionName(e.target.value)}
                    placeholder="e.g. Savings"
                    maxLength={32}
                    autoFocus
                  />
                  <div style={{ fontSize: '11px', color: 'var(--text-dim)', marginTop: '4px' }}>
                    Appended to base name: H&auml;rdig Admin Key - <em>{positionName.trim() || '...'}</em>
                  </div>
                </div>
              </>
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
                  <label>Label (optional)</label>
                  <input
                    type="text"
                    value={keyName}
                    onInput={(e) => setKeyName(e.target.value)}
                    placeholder="e.g. Puppy"
                    maxLength={32}
                  />
                  <div style={{ fontSize: '11px', color: 'var(--text-dim)', marginTop: '4px' }}>
                    Appended to base name: H&auml;rdig Key - <em>{keyName.trim() || '...'}</em>
                  </div>
                </div>
                <div class="form-group">
                  <label>Permissions</label>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '6px', marginTop: '6px' }}>
                    {[
                      [PERM_BUY, 'Buy'],
                      [PERM_SELL, 'Sell'],
                      [PERM_BORROW, 'Borrow'],
                      [PERM_REPAY, 'Repay'],
                      [PERM_REINVEST, 'Reinvest'],
                      [PERM_LIMITED_SELL, 'Limited Sell'],
                      [PERM_LIMITED_BORROW, 'Limited Borrow'],
                    ].map(([bit, name]) => {
                      const p = parseInt(permissions) || 0;
                      return (
                        <label key={bit} style={{ display: 'flex', alignItems: 'center', gap: '6px', cursor: 'pointer', fontSize: '13px' }}>
                          <input
                            type="checkbox"
                            checked={(p & bit) !== 0}
                            onChange={() => setPermissions(String(p ^ bit))}
                          />
                          {name}
                        </label>
                      );
                    })}
                  </div>
                  {((parseInt(permissions) || 0) & PERM_LIMITED_SELL) !== 0 && (
                    <div style={{ marginTop: '8px', marginLeft: '26px' }}>
                      <div class="form-group" style={{ marginBottom: '4px' }}>
                        <label style={{ fontSize: '11px' }}>Sell Capacity (navSOL)</label>
                        <input type="text" value={sellCapacity} onInput={(e) => setSellCapacity(e.target.value)} placeholder="e.g. 5.0" />
                      </div>
                      <label style={{ fontSize: '11px', display: 'block', marginBottom: '4px' }}>Sell Refill Period</label>
                      <div style={{ display: 'flex', gap: '8px', alignItems: 'flex-end' }}>
                        <div>
                          <label style={{ fontSize: '10px' }}>Days</label>
                          <input type="number" min="0" value={sellDays} onInput={(e) => setSellDays(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                        <div>
                          <label style={{ fontSize: '10px' }}>Hours</label>
                          <input type="number" min="0" max="23" value={sellHours} onInput={(e) => setSellHours(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                        <div>
                          <label style={{ fontSize: '10px' }}>Min</label>
                          <input type="number" min="0" max="59" value={sellMinutes} onInput={(e) => setSellMinutes(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                      </div>
                    </div>
                  )}
                  {((parseInt(permissions) || 0) & PERM_LIMITED_BORROW) !== 0 && (
                    <div style={{ marginTop: '8px', marginLeft: '26px' }}>
                      <div class="form-group" style={{ marginBottom: '4px' }}>
                        <label style={{ fontSize: '11px' }}>Borrow Capacity (SOL)</label>
                        <input type="text" value={borrowCapacity} onInput={(e) => setBorrowCapacity(e.target.value)} placeholder="e.g. 5.0" />
                      </div>
                      <label style={{ fontSize: '11px', display: 'block', marginBottom: '4px' }}>Borrow Refill Period</label>
                      <div style={{ display: 'flex', gap: '8px', alignItems: 'flex-end' }}>
                        <div>
                          <label style={{ fontSize: '10px' }}>Days</label>
                          <input type="number" min="0" value={borrowDays} onInput={(e) => setBorrowDays(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                        <div>
                          <label style={{ fontSize: '10px' }}>Hours</label>
                          <input type="number" min="0" max="23" value={borrowHours} onInput={(e) => setBorrowHours(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                        <div>
                          <label style={{ fontSize: '10px' }}>Min</label>
                          <input type="number" min="0" max="59" value={borrowMinutes} onInput={(e) => setBorrowMinutes(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                      </div>
                    </div>
                  )}
                  <div style={{ fontSize: '11px', color: 'var(--text-dim)', marginTop: '8px' }}>
                    {permissionsName(parseInt(permissions) || 0)} (0x{((parseInt(permissions) || 0).toString(16)).toUpperCase().padStart(2, '0')})
                  </div>
                </div>
              </>
            )}

            {action === 'revoke' && (
              <div class="form-group">
                <label>Key to Revoke</label>
                <select value={revokeIdx} onChange={(e) => setRevokeIdx(e.target.value)}>
                  {keyring.value
                    .filter((k) => {
                      const adminMint = position.value?.currentAdminAsset || position.value?.adminAsset;
                      return !adminMint || !k.mint.equals(adminMint);
                    })
                    .map((k, i) => (
                      <option key={i} value={i}>
                        {shortPubkey(k.mint)} ({permissionsName(k.permissions)})
                      </option>
                    ))}
                </select>
              </div>
            )}

            {action === 'createPromo' && (
              <>
                <div class="form-group">
                  <label>Name Suffix</label>
                  <input
                    type="text"
                    value={promoNameSuffix}
                    onInput={(e) => setPromoNameSuffix(e.target.value)}
                    placeholder="e.g. Promo Borrow"
                    maxLength={64}
                    autoFocus
                  />
                  <div style={{ fontSize: '11px', color: 'var(--text-dim)', marginTop: '4px' }}>
                    Used in the NFT name and as PDA seed. Max 64 characters.
                  </div>
                </div>
                <div class="form-group">
                  <label>Permissions</label>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '6px', marginTop: '6px' }}>
                    {[
                      [PERM_BUY, 'Buy'],
                      [PERM_SELL, 'Sell'],
                      [PERM_BORROW, 'Borrow'],
                      [PERM_REPAY, 'Repay'],
                      [PERM_REINVEST, 'Reinvest'],
                      [PERM_LIMITED_SELL, 'Limited Sell'],
                      [PERM_LIMITED_BORROW, 'Limited Borrow'],
                    ].map(([bit, name]) => {
                      const p = parseInt(promoPermissions) || 0;
                      return (
                        <label key={bit} style={{ display: 'flex', alignItems: 'center', gap: '6px', cursor: 'pointer', fontSize: '13px' }}>
                          <input
                            type="checkbox"
                            checked={(p & bit) !== 0}
                            onChange={() => setPromoPermissions(String(p ^ bit))}
                          />
                          {name}
                        </label>
                      );
                    })}
                  </div>
                  {((parseInt(promoPermissions) || 0) & PERM_LIMITED_BORROW) !== 0 && (
                    <div style={{ marginTop: '8px', marginLeft: '26px' }}>
                      <div class="form-group" style={{ marginBottom: '4px' }}>
                        <label style={{ fontSize: '11px' }}>Borrow Capacity (SOL)</label>
                        <input type="text" value={promoBorrowCapacity} onInput={(e) => setPromoBorrowCapacity(e.target.value)} placeholder="e.g. 5.0" />
                      </div>
                      <label style={{ fontSize: '11px', display: 'block', marginBottom: '4px' }}>Borrow Refill Period</label>
                      <div style={{ display: 'flex', gap: '8px', alignItems: 'flex-end' }}>
                        <div>
                          <label style={{ fontSize: '10px' }}>Days</label>
                          <input type="number" min="0" value={promoBorrowDays} onInput={(e) => setPromoBorrowDays(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                        <div>
                          <label style={{ fontSize: '10px' }}>Hours</label>
                          <input type="number" min="0" max="23" value={promoBorrowHours} onInput={(e) => setPromoBorrowHours(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                        <div>
                          <label style={{ fontSize: '10px' }}>Min</label>
                          <input type="number" min="0" max="59" value={promoBorrowMinutes} onInput={(e) => setPromoBorrowMinutes(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                      </div>
                    </div>
                  )}
                  {((parseInt(promoPermissions) || 0) & PERM_LIMITED_SELL) !== 0 && (
                    <div style={{ marginTop: '8px', marginLeft: '26px' }}>
                      <div class="form-group" style={{ marginBottom: '4px' }}>
                        <label style={{ fontSize: '11px' }}>Sell Capacity (navSOL)</label>
                        <input type="text" value={promoSellCapacity} onInput={(e) => setPromoSellCapacity(e.target.value)} placeholder="e.g. 5.0" />
                      </div>
                      <label style={{ fontSize: '11px', display: 'block', marginBottom: '4px' }}>Sell Refill Period</label>
                      <div style={{ display: 'flex', gap: '8px', alignItems: 'flex-end' }}>
                        <div>
                          <label style={{ fontSize: '10px' }}>Days</label>
                          <input type="number" min="0" value={promoSellDays} onInput={(e) => setPromoSellDays(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                        <div>
                          <label style={{ fontSize: '10px' }}>Hours</label>
                          <input type="number" min="0" max="23" value={promoSellHours} onInput={(e) => setPromoSellHours(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                        <div>
                          <label style={{ fontSize: '10px' }}>Min</label>
                          <input type="number" min="0" max="59" value={promoSellMinutes} onInput={(e) => setPromoSellMinutes(e.target.value)} placeholder="0" style={{ width: '60px' }} />
                        </div>
                      </div>
                    </div>
                  )}
                  <div style={{ fontSize: '11px', color: 'var(--text-dim)', marginTop: '8px' }}>
                    {permissionsName(parseInt(promoPermissions) || 0)} (0x{((parseInt(promoPermissions) || 0).toString(16)).toUpperCase().padStart(2, '0')})
                  </div>
                </div>
                <div class="form-group">
                  <label>Min Deposit (SOL)</label>
                  <input
                    type="text"
                    value={promoMinDeposit}
                    onInput={(e) => setPromoMinDeposit(e.target.value)}
                    placeholder="e.g. 0.1"
                  />
                </div>
                <div class="form-group">
                  <label>Max Claims (0 for unlimited)</label>
                  <input
                    type="number"
                    min="0"
                    value={promoMaxClaims}
                    onInput={(e) => setPromoMaxClaims(e.target.value)}
                    placeholder="0"
                  />
                </div>
                <div class="form-group">
                  <label>Image URI (optional)</label>
                  <input
                    type="text"
                    value={promoImageUri}
                    onInput={(e) => setPromoImageUri(e.target.value)}
                    placeholder="https://..."
                    maxLength={128}
                  />
                </div>
              </>
            )}

            {action === 'editPromoMaxClaims' && (() => {
              const promoIdx = actionData?.promoIndex;
              const promo = discoveredPromos.value[promoIdx];
              return (
                <>
                  {promo && (
                    <div style={{ fontSize: '12px', color: 'var(--text-dim)', marginBottom: '10px' }}>
                      Promo: <strong>{promo.config.nameSuffix}</strong> (current: {promo.config.maxClaims === 0 ? 'unlimited' : promo.config.maxClaims}, claimed: {promo.config.claimsCount})
                    </div>
                  )}
                  <div class="form-group">
                    <label>New Max Claims (0 for unlimited)</label>
                    <input
                      type="number"
                      min="0"
                      value={editMaxClaims}
                      onInput={(e) => setEditMaxClaims(e.target.value)}
                      placeholder="0"
                      autoFocus
                    />
                  </div>
                </>
              );
            })()}

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
                    ['Deposited', result.snapshot.depositedNav, position.value.depositedNav, navTokenName(marketConfig.value?.navMint)],
                    ['Debt', result.snapshot.userDebt, position.value.userDebt, 'SOL'],
                  ].map(([label, before, after, unit]) => {
                    const delta = formatDelta(before, after);
                    const cls = delta.startsWith('+') ? 'positive' : delta.startsWith('-') ? 'negative' : '';
                    return (
                      <tr key={label}>
                        <th>{label}</th>
                        <td>{lamportsToSol(before)} {unit}</td>
                        <td>{lamportsToSol(after)} {unit}</td>
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
