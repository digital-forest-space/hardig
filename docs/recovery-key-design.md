# Time-Locked Backup Recovery Key

## Problem

If an admin loses access to their admin NFT key (hardware wallet broken, accident, forgotten), the position's funds are permanently locked. There is no way to regain control.

## Solution

A dead-man's switch recovery mechanism. The admin pre-designates a recovery NFT key. If the admin is inactive for a configurable period, the recovery key holder can claim admin control of the position.

The on-chain program only enforces the inactivity threshold. Grace periods, alerts, and notifications are app-layer concerns -- the app can monitor `last_admin_activity` and alert at configurable thresholds (e.g., "11 months inactive -- do a heartbeat!").

## Design Principles

- Recovery is **repeatable**. Each new admin can configure a fresh recovery key. The position is only irrecoverable if both the current admin NFT and the current recovery NFT are lost simultaneously.
- Recovery **invalidates itself**. After execution, the recovery key's authorization is closed and recovery must be explicitly reconfigured. No lingering recovery authority.
- Recovery fits the **existing keyring model**. The recovery key is an NFT, validated through `KeyAuthorization` with a new `KeyRole::Recovery` role.
- The **authority PDA is stable**. The Mayflower position remains accessible through any number of recovery rotations.

## State Changes

### PositionNFT (modified)

| Field | Change | Type | Notes |
|---|---|---|---|
| `admin_nft_mint` | **Rename** to `authority_seed` | `Pubkey` | Permanent PDA seed, never changes. Initialized to the first admin NFT mint at position creation. |
| `recovery_nft_mint` | **Add** | `Pubkey` | The recovery NFT mint. `Pubkey::default()` = no recovery configured. |
| `recovery_lockout_secs` | **Add** | `i64` | Inactivity threshold in seconds before recovery can execute. |
| `recovery_config_locked` | **Add** | `bool` | If true, recovery config cannot be changed. Prevents an attacker who gains admin from disabling recovery. |

Space impact: 140 -> 181 bytes (+41 bytes, ~0.0005 SOL additional rent per position).

### KeyRole enum (modified)

Add `Recovery = 4`. This role is not included in any existing instruction's allowed roles, so a recovery key cannot perform any financial operation.

### HardigError enum (modified)

Add:
- `RecoveryNotConfigured` -- no recovery key is set for this position
- `RecoveryLockoutNotExpired` -- admin has been active within the lockout period
- `RecoveryConfigLocked` -- recovery config is locked and cannot be changed

## New Instructions

### 1. `configure_recovery`

**Who**: Admin only.

**Purpose**: Sets the recovery NFT mint, lockout duration, and optional config lock.

**Behavior**:
- Validates admin key via `validate_key()` with `KeyRole::Admin`.
- Requires `position.recovery_config_locked == false`.
- Sets `position.recovery_nft_mint`, `position.recovery_lockout_secs`, `position.recovery_config_locked`.
- Creates a `KeyAuthorization` PDA for the recovery NFT mint with `KeyRole::Recovery`.
- If replacing an existing recovery key, closes the old `KeyAuthorization` first.
- Updates `position.last_admin_activity` (admin action).
- For existing positions (pre-recovery feature): uses `realloc` to expand from 140 to 181 bytes.

### 2. `execute_recovery`

**Who**: Recovery key holder.

**Purpose**: Claims admin control after the inactivity threshold has passed.

**Preconditions**:
- Recovery key holder proves NFT ownership via `validate_key()` with `KeyRole::Recovery`.
- `Clock::get()?.unix_timestamp - position.last_admin_activity >= position.recovery_lockout_secs`

**Behavior** (atomic, single instruction):
1. **Mint new admin NFT** to recovery key holder's wallet:
   - Create a new mint account.
   - Mint 1 token to recovery holder's ATA.
   - Set mint authority to `None` (burned, consistent with existing pattern).
   - Set freeze authority to the program PDA.
2. **Authorize new admin**:
   - Create a new `KeyAuthorization` PDA for the new admin NFT mint with `KeyRole::Admin`.
3. **Invalidate old admin**:
   - Close the old admin's `KeyAuthorization` PDA.
   - Freeze the old admin's NFT ATA (program has freeze authority).
4. **Invalidate recovery key**:
   - Close the recovery key's `KeyAuthorization` PDA.
   - Set `position.recovery_nft_mint = Pubkey::default()`.
5. **Update position state**:
   - Set `position.last_admin_activity = Clock::get()?.unix_timestamp`.
   - Do NOT change `position.authority_seed` (preserves Mayflower PDA).

**Post-recovery state**:
- New admin NFT is active with full admin privileges.
- Old admin NFT is frozen and unauthorized (effectively dead).
- Recovery NFT is unauthorized (inert token, no `KeyAuthorization`).
- No active recovery key -- new admin must call `configure_recovery` to set one up.

### 3. `heartbeat`

**Who**: Admin only.

**Purpose**: No-op liveness proof. Updates `last_admin_activity` without performing any financial operation.

**Behavior**:
- Validates admin key via `validate_key()` with `KeyRole::Admin`.
- Sets `position.last_admin_activity = Clock::get()?.unix_timestamp`.
- No other state changes.

## Authority PDA

The authority PDA derivation is renamed but unchanged in behavior:

```
seeds = [b"authority", position.authority_seed.as_ref()]
```

`authority_seed` is set once at position creation (to the first admin NFT mint) and never changes. This ensures the Mayflower `PersonalPosition` remains accessible through any number of admin rotations.

The rename from `admin_nft_mint` to `authority_seed` propagates across all CPI instructions that reconstruct signer seeds.

## Rename: admin_nft_mint -> authority_seed

The field `admin_nft_mint` in `PositionNFT` is renamed to `authority_seed` to reflect its true purpose: a permanent PDA seed, not a pointer to the current admin NFT. This rename affects:

- `state.rs` -- field definition and SIZE constant
- `create_position.rs` -- initialization
- All CPI instructions (`buy.rs`, `withdraw.rs`, `borrow.rs`, `repay.rs`, `reinvest.rs`) -- signer seed reconstruction
- `init_mayflower_position.rs` -- signer seed reconstruction
- Tests and clients (TUI, web)

After recovery, `authority_seed` still holds the original admin NFT mint pubkey. The current admin is determined by who holds a `KeyAuthorization` with `KeyRole::Admin` for this position, not by the `authority_seed` field.

## Migration Path

**Existing positions**: `configure_recovery` uses Anchor's `realloc` to expand PositionNFT from 140 to 181 bytes. The payer (admin) covers the additional rent (~0.0005 SOL).

**New positions**: Initialized with the larger size. `recovery_nft_mint = Pubkey::default()`, `recovery_lockout_secs = 0`, `recovery_config_locked = false`.

**The `authority_seed` rename**: This is a field rename only. The on-chain serialized bytes are identical -- Borsh serialization is positional, not named. No data migration is needed. Only the Rust code and clients need updating.

## Security Properties

| Property | How |
|---|---|
| Recovery key can only execute recovery | `KeyRole::Recovery` is not in any other instruction's allowed roles |
| Inactivity timer resets on every admin action | Already implemented across all admin instructions |
| Old admin NFT is dead after recovery | Frozen (program has freeze authority) + KeyAuthorization closed |
| Recovery key is dead after recovery | KeyAuthorization closed, recovery_nft_mint reset to default |
| One recovery key per position | Stored directly in PositionNFT (not a separate PDA) |
| Config lock prevents attacker disabling recovery | Optional `recovery_config_locked` flag |

## Security Considerations

### Clock sysvar reliability

`Clock::get()?.unix_timestamp` is a stake-weighted median of validator timestamps. Drift bounds are 25% fast / 150% slow relative to expected slot timing. For multi-day lockouts, this drift is negligible (hours, not days).

### Front-running

Solana has no public mempool like Ethereum. Recovery initiation is by the recovery key holder; only the admin can prevent it (by staying active). These are different keys, so front-running is not a concern.

### Recovery key compromise

If an attacker steals the recovery NFT, they must wait for the full inactivity period. The admin resets the timer with any transaction (or a heartbeat). The attacker gains nothing unless the admin is genuinely inactive for the full lockout duration.

### Both keys lost

If both the current admin NFT and current recovery NFT are lost simultaneously, funds are irrecoverable. This is an accepted property of self-custody and must be clearly communicated to users.

## App-Layer Responsibilities

The on-chain program is passive. The app layer should:

- **Monitor inactivity**: Alert the admin at configurable thresholds (e.g., 80% of lockout period elapsed).
- **Prompt heartbeat**: Remind the admin to prove liveness if approaching the threshold.
- **Post-recovery flow**: After recovery executes, immediately guide the new admin to configure a fresh recovery key.
- **Setup guidance**: Explain that the recovery NFT should be stored separately from the admin NFT (different wallet, hardware wallet, safe deposit box, trusted party).
- **Clear communication**: Users must understand that losing both keys means permanent fund loss.

## Research Sources

- [vovacodes/wallet-program](https://github.com/vovacodes/wallet-program) -- Anchor social recovery wallet with grace period
- [Squads Protocol v4](https://github.com/Squads-Protocol/v4) -- Production timelocks on Solana ($10B+ secured)
- [Streamflow Finance timelock](https://github.com/streamflow-finance/timelock) -- Canonical Solana timelock implementation
- [Vitalik: Why we need social recovery wallets](https://vitalik.ca/general/2021/01/11/recovery.html)
- [Argent recovery documentation](https://support.argent.xyz/hc/en-us/articles/360007338877)
- [Safe RecoveryHub](https://safe.mirror.xyz/WxKSxD9J1bRI-SDOuDvAAIezwVrvWWkpuwuzcLDPSmk)
- [OpenZeppelin: Argent vulnerability report (CVE-2020-15302)](https://blog.openzeppelin.com/argent-vulnerability-report)
- [RareSkills: The Solana Clock](https://rareskills.io/post/solana-clock)
- [Anza: Bank Timestamp Correction](https://docs.anza.xyz/implemented-proposals/bank-timestamp-correction)
