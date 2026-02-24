# Time-Locked Backup Recovery Key

## Problem

If an admin loses access to their admin NFT key (hardware wallet broken, accident, forgotten), the position's funds are permanently locked. There is no way to regain control.

## Solution

A dead-man's switch recovery mechanism. The admin pre-designates a recovery NFT key. If the admin is inactive for a configurable period, the recovery key holder can claim admin control of the position.

The on-chain program only enforces the inactivity threshold. Grace periods, alerts, and notifications are app-layer concerns -- the app can monitor `last_admin_activity` and alert at configurable thresholds (e.g., "11 months inactive -- do a heartbeat!").

## Design Principles

- Recovery is **repeatable**. Each new admin can configure a fresh recovery key. The position is only irrecoverable if both the current admin NFT and the current recovery NFT are lost simultaneously.
- Recovery **invalidates itself**. After execution, the recovery key is burned and recovery must be explicitly reconfigured. No lingering recovery authority.
- Recovery fits the **existing keyring model**. The recovery key is an MPL-Core NFT in the Hardig collection, with `position` attribute and permissions = 0 (no financial operations). It is validated directly via `position.recovery_asset`, not through `validate_key()`.
- The **authority PDA is stable**. The Mayflower position remains accessible through any number of recovery rotations.

## State Changes

### PositionState (modified)

| Field | Change | Type | Notes |
|---|---|---|---|
| `admin_asset` | **Rename** to `authority_seed` | `Pubkey` | Permanent PDA seed, never changes. Initialized to the first admin MPL-Core asset pubkey at position creation. |
| `current_admin_asset` | **Add** | `Pubkey` | The current admin key NFT (MPL-Core asset). Updated on recovery. Initialized to `admin_asset` value at creation. |
| `recovery_asset` | **Add** | `Pubkey` | The recovery key NFT (MPL-Core asset). `Pubkey::default()` = no recovery configured. |
| `recovery_lockout_secs` | **Add** | `i64` | Inactivity threshold in seconds before recovery can execute. |
| `recovery_config_locked` | **Add** | `bool` | If true, recovery config cannot be changed. Prevents an attacker who gains admin from disabling recovery. |

Space impact: 132 -> 205 bytes (+73 bytes, ~0.001 SOL additional rent per position).

### HardigError enum (modified)

Add:
- `RecoveryNotConfigured` -- no recovery key is set for this position
- `RecoveryLockoutNotExpired` -- admin has been active within the lockout period
- `RecoveryConfigLocked` -- recovery config is locked and cannot be changed

### Permission model

No new permission bits are needed. The recovery key has permissions = 0 in its MPL-Core Attributes, which means it fails all `validate_key()` checks (which require `permissions & required != 0`). The `execute_recovery` instruction validates the recovery key directly by checking:
1. `key_asset_info.owner == mpl_core::ID` (valid MPL-Core account)
2. Signer owns the asset (parsed from asset data bytes 1..33)
3. `key_asset.key() == position.recovery_asset`

This is a deliberate subset of `validate_key()` -- permissions are irrelevant for recovery, only ownership matters.

## New Instructions

### 1. `configure_recovery`

**Who**: Admin only.

**Purpose**: Sets the recovery key, lockout duration, and optional config lock.

**Accounts**:
- `admin` (signer, mut) -- the admin wallet
- `admin_key_asset` (unchecked) -- admin's MPL-Core key NFT, validated via `validate_key()`
- `position` (mut) -- the PositionState account (may need realloc)
- `recovery_asset` (signer) -- new MPL-Core asset to create for the recovery key
- `target_wallet` (unchecked) -- wallet that will hold the recovery key
- `config` -- ProtocolConfig PDA (for collection address)
- `collection` (mut, unchecked) -- MPL-Core collection
- `mpl_core_program` -- MPL-Core program
- `system_program` -- System program

**Behavior**:
- Validates admin key via `validate_key()` with `PERM_MANAGE_KEYS`.
- Requires `position.recovery_config_locked == false` (error: `RecoveryConfigLocked`).
- If replacing an existing recovery key (`position.recovery_asset != Pubkey::default()`), burns the old recovery asset via `BurnV1CpiBuilder`.
- Creates a new recovery key NFT via `CreateV2CpiBuilder`:
  - Name: "Hardig Recovery Key" (or with optional suffix)
  - Attributes: `permissions = 0`, `position = authority_seed`, `recovery = true`
  - Plugins: `PermanentBurnDelegate`, `PermanentTransferDelegate` (same as other keys)
  - Owner: `target_wallet` (should be a DIFFERENT wallet from admin)
- Sets `position.recovery_asset = recovery_asset.key()`.
- Sets `position.recovery_lockout_secs` from instruction arg.
- If `lock_config` arg is true, requires `recovery_asset != Pubkey::default()` and sets `position.recovery_config_locked = true`.
- Updates `position.last_admin_activity` (admin action).
- For existing positions (pre-recovery): uses Anchor `realloc` to expand from 132 to 205 bytes. Payer covers additional rent.

### 2. `execute_recovery`

**Who**: Recovery key holder.

**Purpose**: Claims admin control after the inactivity threshold has passed.

**Accounts**:
- `recovery_holder` (signer, mut) -- the recovery key holder's wallet
- `recovery_key_asset` (unchecked) -- the recovery MPL-Core asset, validated directly
- `position` (mut) -- the PositionState account
- `old_admin_asset` (mut, unchecked) -- the old admin's MPL-Core asset (to burn)
- `new_admin_asset` (signer) -- new MPL-Core asset to create for the new admin key
- `config` -- ProtocolConfig PDA (for collection address + signing)
- `collection` (mut, unchecked) -- MPL-Core collection
- `mpl_core_program` -- MPL-Core program
- `system_program` -- System program

**Preconditions**:
- Recovery key holder proves NFT ownership: `recovery_key_asset.owner == mpl_core::ID`, signer owns the asset, `recovery_key_asset.key() == position.recovery_asset`.
- `Clock::get()?.unix_timestamp - position.last_admin_activity >= position.recovery_lockout_secs`
- `position.recovery_asset != Pubkey::default()` (error: `RecoveryNotConfigured`)

**Behavior** (atomic, single instruction):
1. **Create new admin NFT** via `CreateV2CpiBuilder`:
   - Owner: `recovery_holder` wallet
   - Attributes: `permissions = PRESET_ADMIN`, `position = position.authority_seed`
   - Same plugin setup as `create_position` (PermanentBurnDelegate, PermanentTransferDelegate)
2. **Burn old admin NFT** via `BurnV1CpiBuilder`:
   - Uses config PDA as authority (PermanentBurnDelegate)
   - Validates `old_admin_asset.key() == position.current_admin_asset`
3. **Burn recovery key** via `BurnV1CpiBuilder`:
   - Uses config PDA as authority (PermanentBurnDelegate)
4. **Update position state**:
   - `position.current_admin_asset = new_admin_asset.key()`
   - `position.recovery_asset = Pubkey::default()` (no active recovery)
   - `position.recovery_lockout_secs = 0`
   - `position.recovery_config_locked = false` (new admin can configure fresh recovery)
   - `position.last_admin_activity = Clock::get()?.unix_timestamp`
   - Do NOT change `position.authority_seed` (preserves Mayflower PDA)

**Post-recovery state**:
- New admin NFT is active with full admin privileges.
- Old admin NFT is burned (account closed by MPL-Core).
- Recovery NFT is burned (account closed by MPL-Core).
- No active recovery key -- new admin must call `configure_recovery` to set one up.
- Existing delegated keys still work (their `position` attribute matches `authority_seed`).

### 3. `heartbeat`

**Who**: Admin only.

**Purpose**: No-op liveness proof. Updates `last_admin_activity` without performing any financial operation.

**Accounts**:
- `admin` (signer) -- the admin wallet
- `admin_key_asset` (unchecked) -- admin's MPL-Core key NFT
- `position` (mut) -- the PositionState account

**Behavior**:
- Validates admin key via `validate_key()` with `PERM_MANAGE_KEYS`.
- Sets `position.last_admin_activity = Clock::get()?.unix_timestamp`.
- No other state changes.

## Rename: admin_asset -> authority_seed

The field `admin_asset` in `PositionState` is renamed to `authority_seed` to reflect its true purpose: a permanent PDA seed, not a pointer to the current admin NFT. A new `current_admin_asset` field is added to track the current admin key.

This rename is a **field rename only**. Borsh serialization is positional, not named. The on-chain byte layout is unchanged for existing fields. No data migration is needed for the rename itself (only realloc for the new fields).

### Files affected by the rename

**On-chain program:**
- `state.rs` -- field definition, SIZE constant, add `current_admin_asset`
- `create_position.rs` -- initialization (set both `authority_seed` and `current_admin_asset`)
- `buy.rs`, `withdraw.rs`, `borrow.rs`, `repay.rs`, `reinvest.rs` -- signer seeds change from `position.admin_asset` to `position.authority_seed`, admin activity check changes from `key_asset == position.admin_asset` to `key_asset == position.current_admin_asset`
- `authorize_key.rs` -- `position.admin_asset` references change to `position.authority_seed` (for `position` attribute binding) and `position.current_admin_asset` (for validate_key)
- `revoke_key.rs` -- `CannotRevokeAdminKey` guard changes from `position.admin_asset` to `position.current_admin_asset`

**Clients (TUI + web-lite):**
- References to `admin_asset` in PositionState deserialization change to `authority_seed`
- New `current_admin_asset` field for admin identity checks
- Discovery logic: match both `authority_seed` and `current_admin_asset` where needed

## Authority PDA

The authority PDA derivation uses the permanent seed:

```
seeds = [b"authority", position.authority_seed.as_ref()]
```

`authority_seed` is set once at position creation (to the first admin MPL-Core asset pubkey) and never changes. This ensures the Mayflower `PersonalPosition` remains accessible through any number of admin rotations.

Similarly, the PositionState PDA remains stable:

```
seeds = [b"position", authority_seed.as_ref()]
```

## Migration Path

**Existing positions**: A dedicated `migrate_position` instruction uses Anchor's `realloc` to expand PositionState from 132 to 205 bytes. It copies `authority_seed` (formerly `admin_asset`) into `current_admin_asset`, and zero-initializes recovery fields. The payer (admin) covers the additional rent (~0.001 SOL).

Alternatively, `configure_recovery` itself can perform the realloc if the account is undersized. This avoids a separate migration instruction but adds complexity to one handler.

**New positions**: Initialized with the larger size. `current_admin_asset = authority_seed`, `recovery_asset = Pubkey::default()`, `recovery_lockout_secs = 0`, `recovery_config_locked = false`.

## Security Properties

| Property | How |
|---|---|
| Recovery key can only execute recovery | Permissions = 0, fails all `validate_key()` checks for financial ops |
| Inactivity timer resets on every admin action | Already implemented: `last_admin_activity` updated in buy, sell, borrow, repay, reinvest |
| Old admin NFT is dead after recovery | Burned via `BurnV1CpiBuilder` (MPL-Core PermanentBurnDelegate) |
| Recovery key is dead after recovery | Burned via `BurnV1CpiBuilder`, `recovery_asset` reset to default |
| One recovery key per position | Stored directly in PositionState (not a separate PDA) |
| Config lock prevents attacker disabling recovery | `recovery_config_locked` flag, only settable when recovery IS configured |
| Delegated keys survive recovery | Their `position` attribute matches `authority_seed` (permanent) |
| No dual-admin window | Single atomic Solana instruction |

## Security Considerations

### Recovery protects against lost keys, not stolen keys

If an attacker steals the admin key, they can drain funds immediately (sell + borrow). The recovery mechanism only fires after the lockout period -- by then the funds may be gone. The `recovery_config_locked` flag helps only in the narrower scenario where an attacker steals the admin key and tries to *disable* recovery rather than drain immediately.

Users must understand: recovery is insurance against *lost access*, not against *compromised keys*.

### Clock sysvar reliability

`Clock::get()?.unix_timestamp` is a stake-weighted median of validator timestamps. Drift bounds are 25% fast / 150% slow relative to expected slot timing. For multi-day lockouts, this drift is negligible (hours, not days).

### Front-running

Solana has no public mempool like Ethereum. Recovery initiation is by the recovery key holder; only the admin can prevent it (by staying active). These are different keys, so front-running is not a concern.

### Recovery key compromise

If an attacker steals the recovery NFT, they must wait for the full inactivity period. The admin resets the timer with any transaction (or a heartbeat). The attacker gains nothing unless the admin is genuinely inactive for the full lockout duration.

### Both keys lost

If both the current admin NFT and current recovery NFT are lost simultaneously, funds are irrecoverable. This is an accepted property of self-custody and must be clearly communicated to users.

### Compute budget

`execute_recovery` performs two MPL-Core creates (new admin asset) and two burns (old admin + recovery key) in a single instruction. Estimated ~250k compute units. Clients must include a `SetComputeUnitLimit` instruction requesting 300k CU.

### Config lock safety

`recovery_config_locked` can only be set to `true` when `recovery_asset != Pubkey::default()`. Setting it without a recovery key configured would permanently lock out recovery configuration.

## App-Layer Responsibilities

The on-chain program is passive. The app layer should:

- **Monitor inactivity**: Alert the admin at configurable thresholds (e.g., 80% of lockout period elapsed).
- **Prompt heartbeat**: Remind the admin to prove liveness if approaching the threshold.
- **Post-recovery flow**: After recovery executes, immediately guide the new admin to configure a fresh recovery key.
- **Setup guidance**: Explain that the recovery NFT should be stored separately from the admin NFT (different wallet, hardware wallet, safe deposit box, trusted party).
- **Clear communication**: Users must understand that losing both keys means permanent fund loss. Recovery does NOT protect against stolen keys.
- **Compute budget**: Always include `SetComputeUnitLimit(300_000)` when submitting `execute_recovery`.

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
