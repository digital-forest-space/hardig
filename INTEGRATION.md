# Hardig Integration Guide

Technical reference for third-party developers integrating with the Hardig on-chain program.

## Program ID and IDL

**Program ID:** `4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p`

**IDL location:**

- After running `anchor build`, the IDL is at `target/idl/hardig.json`.
- On-chain (if published): `anchor idl fetch 4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p --provider.cluster mainnet`
- The IDL file can be used with `@coral-xyz/anchor` to construct instructions and decode accounts.

## Account Structures

All accounts use the standard 8-byte Anchor discriminator prefix. Sizes listed below include the discriminator.

### ProtocolConfig

Singleton global configuration. One per deployment.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | discriminator | Anchor account discriminator |
| 8 | 32 | `admin` | Protocol admin pubkey |
| 40 | 32 | `collection` | MPL-Core collection for key NFTs (`Pubkey::default()` if not yet created) |
| 72 | 32 | `pending_admin` | Pending admin for two-step transfer (`Pubkey::default()` = no pending transfer) |
| 104 | 1 | `bump` | PDA bump seed |

**Total size:** 105 bytes

**Source:** `ProtocolConfig` in `programs/hardig/src/state/mod.rs`

### PositionState

Represents a navSOL position controlled by an NFT keyring. One per admin key.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | discriminator | Anchor account discriminator |
| 8 | 32 | `authority_seed` | Permanent PDA seed (first admin asset pubkey). Never changes after creation |
| 40 | 32 | `position_pda` | Mayflower PersonalPosition PDA owned by this position |
| 72 | 32 | `market_config` | MarketConfig PDA this position is bound to |
| 104 | 8 | `deposited_nav` | navSOL deposited (local tracking; Mayflower is source of truth) |
| 112 | 8 | `user_debt` | Total SOL borrowed (local tracking; Mayflower is source of truth) |
| 120 | 2 | `max_reinvest_spread_bps` | Max market/floor spread ratio (bps) for reinvest |
| 122 | 8 | `last_admin_activity` | Unix timestamp of last admin-signed instruction |
| 130 | 1 | `bump` | PDA bump seed |
| 131 | 1 | `authority_bump` | Bump for the per-position authority PDA |
| 132 | 32 | `current_admin_asset` | Current admin key NFT (MPL-Core asset). Updated on recovery |
| 164 | 32 | `recovery_asset` | Recovery key NFT (`Pubkey::default()` = no recovery configured) |
| 196 | 8 | `recovery_lockout_secs` | Inactivity threshold in seconds before recovery can execute |
| 204 | 1 | `recovery_config_locked` | If true, recovery config cannot be changed |
| 205 | 33 | `artwork_id` | Optional artwork set ID for custom key visuals (`Option<Pubkey>`: 1 byte tag + 32 byte pubkey) |

**Total size:** 238 bytes

**Source:** `PositionState` in `programs/hardig/src/state/mod.rs`

### MarketConfig

On-chain configuration for a Mayflower market (e.g., navSOL, navJUP). Created by the protocol admin.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | discriminator | Anchor account discriminator |
| 8 | 32 | `nav_mint` | Nav token mint (e.g., navSOL) |
| 40 | 32 | `base_mint` | Base mint (e.g., wSOL) |
| 72 | 32 | `market_group` | Mayflower market group account |
| 104 | 32 | `market_meta` | Mayflower market metadata account |
| 136 | 32 | `mayflower_market` | Mayflower market account |
| 168 | 32 | `market_base_vault` | Mayflower market base vault |
| 200 | 32 | `market_nav_vault` | Mayflower market nav vault |
| 232 | 32 | `fee_vault` | Mayflower fee vault |
| 264 | 1 | `bump` | PDA bump seed |

**Total size:** 265 bytes

**Source:** `MarketConfig` in `programs/hardig/src/state/mod.rs`

### KeyState

Mutable state for a delegated key NFT. Tracks rate-limit token buckets. Created for every delegated key via `authorize_key`.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | discriminator | Anchor account discriminator |
| 8 | 32 | `authority_seed` | Position's authority_seed this key belongs to (for memcmp filtering) |
| 40 | 32 | `asset` | MPL-Core asset pubkey this state belongs to |
| 72 | 1 | `bump` | PDA bump seed |
| 73 | 32 | `sell_bucket` | RateBucket for `PERM_LIMITED_SELL` |
| 105 | 32 | `borrow_bucket` | RateBucket for `PERM_LIMITED_BORROW` |
| 137 | 8 | `total_sell_limit` | Optional lifetime sell cap in navSOL shares (0 = no cap) |
| 145 | 8 | `total_sold` | Accumulator of total navSOL shares sold via this key |
| 153 | 8 | `total_borrow_limit` | Optional lifetime borrow cap in lamports (0 = no cap) |
| 161 | 8 | `total_borrowed` | Accumulator of total lamports borrowed via this key |

**Total size:** 169 bytes

Each **RateBucket** (32 bytes, all little-endian u64):

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | `capacity` | Max tokens (shares for sell, lamports for borrow) |
| 8 | 8 | `refill_period` | Slots for a full refill from 0 to capacity |
| 16 | 8 | `level` | Tokens remaining at last update |
| 24 | 8 | `last_update` | Slot of last update |

**Source:** `KeyState`, `RateBucket` in `programs/hardig/src/state/mod.rs`

### PromoConfig

Per-position promotional campaign configuration. Allows permissionless key claiming with a required deposit.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | discriminator | Anchor account discriminator |
| 8 | 32 | `authority_seed` | Position's authority_seed this promo belongs to |
| 40 | 1 | `permissions` | Key permissions bitmask granted to claimed keys |
| 41 | 8 | `borrow_capacity` | LimitedBorrow bucket capacity (lamports) |
| 49 | 8 | `borrow_refill_period` | LimitedBorrow refill period (slots) |
| 57 | 8 | `sell_capacity` | LimitedSell bucket capacity (0 if N/A) |
| 65 | 8 | `sell_refill_period` | LimitedSell refill period (0 if N/A) |
| 73 | 8 | `min_deposit_lamports` | Required deposit amount in lamports |
| 81 | 4 | `max_claims` | Max total keys claimable (0 = unlimited) |
| 85 | 4 | `claims_count` | Number of keys claimed so far |
| 89 | 1 | `active` | Whether claiming is enabled |
| 90 | 8 | `total_borrow_limit` | Lifetime borrow cap for claimed keys in lamports (0 = no cap) |
| 98 | 8 | `total_sell_limit` | Lifetime sell cap for claimed keys in navSOL shares (0 = no cap) |
| 106 | 2 | `initial_fill_bps` | Initial bucket fill level in basis points (0 = empty, 10000 = full) |
| 108 | 4+N | `name_suffix` | NFT name suffix (Borsh string: 4-byte LE length + UTF-8, max 64 bytes content) |
| ... | 4+N | `image_uri` | Custom NFT image URL (Borsh string, max 128 bytes content) |
| ... | 4+N | `market_name` | Market name for NFT metadata (Borsh string, max 32 bytes content) |
| ... | 1 | `bump` | PDA bump seed |

**Max size:** 347 bytes (with max-length strings)

**PDA seeds:** `["promo", authority_seed, name_suffix_bytes]`

**Source:** `PromoConfig` in `programs/hardig/src/state/promo.rs`

### TrustedProvider

Marker PDA for a trusted artwork provider program. Created by the protocol admin.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | discriminator | Anchor account discriminator |
| 8 | 32 | `program_id` | The trusted provider program ID |
| 40 | 32 | `added_by` | Protocol admin who registered it |
| 72 | 1 | `active` | Whether the provider is active |
| 73 | 1 | `bump` | PDA bump seed |

**Total size:** 74 bytes

**PDA seeds:** `["trusted_provider", program_id]`

**Source:** `TrustedProvider` in `programs/hardig/src/state/mod.rs`

## PDA Derivation

All PDAs use the Hardig program ID (`4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p`) as the program.

### Summary Table

| PDA | Seeds | Account Type |
|-----|-------|--------------|
| Protocol config | `["config"]` | `ProtocolConfig` |
| Position | `["position", authority_seed]` | `PositionState` |
| Per-position authority | `["authority", authority_seed]` | (no account data; signer PDA) |
| Key state | `["key_state", asset]` | `KeyState` |
| Market config | `["market_config", nav_mint]` | `MarketConfig` |
| Promo config | `["promo", authority_seed, name_suffix]` | `PromoConfig` |
| Claim receipt | `["claim_receipt", promo, claimer]` | `ClaimReceipt` |
| Trusted provider | `["trusted_provider", program_id]` | `TrustedProvider` |

### JavaScript (using `@solana/web3.js`)

```js
import { PublicKey } from '@solana/web3.js';
import { Buffer } from 'buffer';

const PROGRAM_ID = new PublicKey('4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p');

// ProtocolConfig
const [configPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('config')],
  PROGRAM_ID
);

// PositionState (adminAsset is a PublicKey)
const [positionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('position'), adminAsset.toBuffer()],
  PROGRAM_ID
);

// Per-position authority (signer PDA, no on-chain account)
const [authorityPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('authority'), adminAsset.toBuffer()],
  PROGRAM_ID
);

// KeyState (asset is the MPL-Core asset PublicKey)
const [keyStatePda] = PublicKey.findProgramAddressSync(
  [Buffer.from('key_state'), asset.toBuffer()],
  PROGRAM_ID
);

// MarketConfig (navMint is the nav token mint PublicKey)
const [marketConfigPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('market_config'), navMint.toBuffer()],
  PROGRAM_ID
);
```

### Rust (using `anchor-lang`)

```rust
use anchor_lang::prelude::Pubkey;

let program_id = hardig::ID;

let (config_pda, _bump) = Pubkey::find_program_address(&[b"config"], &program_id);

let (position_pda, _bump) = Pubkey::find_program_address(
    &[b"position", admin_asset.as_ref()],
    &program_id,
);

let (authority_pda, _bump) = Pubkey::find_program_address(
    &[b"authority", admin_asset.as_ref()],
    &program_id,
);

let (key_state_pda, _bump) = Pubkey::find_program_address(
    &[b"key_state", asset.as_ref()],
    &program_id,
);

let (market_config_pda, _bump) = Pubkey::find_program_address(
    &[b"market_config", nav_mint.as_ref()],
    &program_id,
);
```

## Instruction Overview

### Permission Bitmask

Permissions are stored as a single `u8` bitmask on each key NFT's MPL-Core `Attributes` plugin:

| Bit | Hex | Constant | Permission |
|-----|-----|----------|------------|
| 0 | `0x01` | `PERM_BUY` | Buy nav tokens |
| 1 | `0x02` | `PERM_SELL` | Sell (withdraw) nav tokens |
| 2 | `0x04` | `PERM_BORROW` | Borrow SOL against position |
| 3 | `0x08` | `PERM_REPAY` | Repay borrowed SOL |
| 4 | `0x10` | `PERM_REINVEST` | Reinvest borrow capacity |
| 5 | `0x20` | `PERM_MANAGE_KEYS` | Authorize/revoke keys |
| 6 | `0x40` | `PERM_LIMITED_SELL` | Rate-limited sell (uses KeyState bucket) |
| 7 | `0x80` | `PERM_LIMITED_BORROW` | Rate-limited borrow (uses KeyState bucket) |

**Role presets:**

| Role | Hex | Bits |
|------|-----|------|
| Admin | `0x3F` | buy + sell + borrow + repay + reinvest + manage_keys |
| Operator | `0x19` | buy + repay + reinvest |
| Depositor | `0x09` | buy + repay |
| Keeper | `0x10` | reinvest only |

### Instruction Table

| Instruction | Required Permission | Parameters | Description |
|-------------|-------------------|------------|-------------|
| `initialize_protocol` | Protocol deployer (first call) | -- | Create global ProtocolConfig PDA |
| `create_collection` | Protocol admin | `uri: String` | Create MPL-Core collection for key NFTs |
| `create_market_config` | Protocol admin | 8 Mayflower market pubkeys | Register a Mayflower market |
| `create_position` | Any signer | `max_reinvest_spread_bps: u16`, `name: Option<String>`, `market_name: String`, `artwork_id: Option<Pubkey>` | Mint admin key NFT and create position |
| `authorize_key` | `PERM_MANAGE_KEYS` | `permissions: u8`, rate-limit params, `total_sell_limit: u64`, `total_borrow_limit: u64`, `name: Option<String>` | Mint a delegated key NFT to a target wallet |
| `revoke_key` | `PERM_MANAGE_KEYS` | -- | Close key authorization; burn NFT if admin holds it |
| `buy` | `PERM_BUY` | `amount: u64`, `min_out: u64` | Deposit SOL to buy nav tokens via Mayflower CPI |
| `withdraw` | `PERM_SELL` or `PERM_LIMITED_SELL` | `amount: u64`, `min_out: u64` | Sell nav tokens to withdraw SOL |
| `borrow` | `PERM_BORROW` or `PERM_LIMITED_BORROW` | `amount: u64` | Borrow SOL against nav-token floor |
| `repay` | `PERM_REPAY` | `amount: u64` | Repay borrowed SOL |
| `reinvest` | `PERM_REINVEST` | `min_out: u64` | Borrow available capacity and buy more nav tokens |
| `heartbeat` | `PERM_MANAGE_KEYS` | -- | No-op liveness proof; resets recovery lockout |
| `configure_recovery` | `PERM_MANAGE_KEYS` | `lockout_secs: i64`, `lock_config: bool`, `name: Option<String>` | Set or replace the dead-man's switch recovery key |
| `execute_recovery` | Recovery key holder | -- | Claim admin control after lockout expires |
| `transfer_admin` | Protocol admin | `new_admin: Pubkey` | Transfer protocol admin rights |
| `accept_admin` | Pending admin | -- | Accept a pending protocol admin transfer |
| `create_promo` | `PERM_MANAGE_KEYS` | `name_suffix`, `permissions`, rate-limit params, `total_borrow_limit`, `total_sell_limit`, `min_deposit_lamports`, `max_claims`, `initial_fill_bps`, `image_uri`, `market_name` | Create a promotional campaign for a position |
| `update_promo` | `PERM_MANAGE_KEYS` | `active: Option<bool>`, `max_claims: Option<u32>` | Toggle promo active state or update max claims |
| `claim_promo_key` | Any signer | `amount: u64`, `min_out: u64` | Claim a promo key NFT (deposits SOL via Mayflower buy) |
| `add_trusted_provider` | Protocol admin | `program_id: Pubkey` | Register a trusted artwork provider program |
| `remove_trusted_provider` | Protocol admin | -- | Deactivate a trusted artwork provider (closes PDA) |
| `set_position_artwork` | `PERM_MANAGE_KEYS` | `artwork_id: Option<Pubkey>` | Set or clear custom artwork on a position (affects future keys) |
| `migrate_config` | Protocol admin | -- | Migrate ProtocolConfig from v0 to v1 |

### Key Validation

Every position-modifying instruction validates the signer's key via `validate_key()`:

1. Deserializes the MPL-Core asset and confirms the signer is the owner.
2. Reads the `position` attribute from the asset's Attributes plugin and verifies it matches the position's `admin_asset`.
3. Reads the `permissions` attribute and checks the required permission bit is set.

The `withdraw` and `borrow` instructions additionally support rate-limited keys. If the key has `PERM_LIMITED_SELL` or `PERM_LIMITED_BORROW` (instead of the unrestricted `PERM_SELL`/`PERM_BORROW`), the instruction consumes from the corresponding `RateBucket` in the key's `KeyState` PDA. Rate-limited keys may also have optional lifetime caps (`total_sell_limit`, `total_borrow_limit`). When nonzero, the accumulator fields (`total_sold`, `total_borrowed`) are checked after each operation and the transaction fails with `TotalLimitExceeded` if the lifetime cap would be exceeded.

## Reading Position Data

### Fetching a PositionState

```js
const positionInfo = await connection.getAccountInfo(positionPda);
const data = positionInfo.data;
const view = new DataView(data.buffer, data.byteOffset);

const authoritySeed       = new PublicKey(data.slice(8, 40));
const mfPositionPda       = new PublicKey(data.slice(40, 72));
const marketConfigPda     = new PublicKey(data.slice(72, 104));
const depositedNav        = Number(view.getBigUint64(104, true));
const userDebt            = Number(view.getBigUint64(112, true));
const spreadBps           = view.getUint16(120, true);
const lastAdmin           = Number(view.getBigInt64(122, true));
const bump                = data[130];
const authorityBump       = data[131];
const currentAdminAsset   = new PublicKey(data.slice(132, 164));
const recoveryAsset       = new PublicKey(data.slice(164, 196));
const recoveryLockoutSecs = Number(view.getBigInt64(196, true));
const recoveryConfigLocked = data[204] !== 0;
const hasArtwork          = data[205] !== 0;
const artworkId           = hasArtwork ? new PublicKey(data.slice(206, 238)) : null;
```

### Computing Borrow Capacity

Borrow capacity is determined from on-chain Mayflower state, not Hardig accounting. You need two accounts:

1. **Mayflower PersonalPosition** -- stores `deposited_shares` and `debt`.
2. **Mayflower Market** -- stores the `floor_price`.

**Deriving the Mayflower PersonalPosition PDA:**

```js
const MAYFLOWER_PROGRAM_ID = new PublicKey('AVMmmRzwc2kETQNhPiFVnyu62HrgsQXTD6D7SnSfEz7v');

const [authorityPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('authority'), adminAsset.toBuffer()],
  PROGRAM_ID  // Hardig program
);

const [personalPositionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('personal_position'), marketMeta.toBuffer(), authorityPda.toBuffer()],
  MAYFLOWER_PROGRAM_ID
);
```

**Reading PersonalPosition data:**

```js
const PP_DEPOSITED_SHARES_OFFSET = 104; // u64 LE
const PP_DEBT_OFFSET = 112;             // u64 LE

const ppInfo = await connection.getAccountInfo(personalPositionPda);
const ppView = new DataView(ppInfo.data.buffer, ppInfo.data.byteOffset);

const depositedShares = Number(ppView.getBigUint64(PP_DEPOSITED_SHARES_OFFSET, true));
const debt            = Number(ppView.getBigUint64(PP_DEBT_OFFSET, true));
```

**Reading floor price from the Mayflower Market account:**

The floor price is stored as a Rust Decimal (16 bytes) at offset 104 in the Mayflower Market account. To decode it:

```js
const MARKET_FLOOR_PRICE_OFFSET = 104;

const marketInfo = await connection.getAccountInfo(mayflowerMarketPubkey);
const mdata = marketInfo.data;

// Rust Decimal layout: bytes[0..4] = flags (byte[2] is scale), bytes[4..16] = 96-bit mantissa (LE)
const scale = mdata[MARKET_FLOOR_PRICE_OFFSET + 2];

let mantissa = BigInt(0);
for (let i = 4; i < 16; i++) {
  mantissa |= BigInt(mdata[MARKET_FLOOR_PRICE_OFFSET + i]) << BigInt(8 * (i - 4));
}

// Convert to lamports-per-navSOL-lamport (scaled by 1e9)
const floorPriceLamports = Number(mantissa * BigInt(1_000_000_000) / BigInt(10) ** BigInt(scale));
```

**Calculating available borrow capacity:**

```
capacity = (deposited_shares * floor_price_lamports / 1e9) - current_debt
```

```js
const floorValue = BigInt(depositedShares) * BigInt(floorPriceLamports) / BigInt(1_000_000_000);
const borrowCapacity = Number(floorValue - BigInt(debt));
// Clamp to 0 if negative (over-borrowed)
const available = Math.max(0, borrowCapacity);
```

## Rate-Limited Keys

Keys with `PERM_LIMITED_SELL` (0x40) or `PERM_LIMITED_BORROW` (0x80) are governed by a token-bucket rate limiter stored in the key's `KeyState` PDA. The bucket refills linearly over a configurable slot period, capping at a maximum capacity.

### Computing Available Allowance

1. **Derive the KeyState PDA** from the key's MPL-Core asset pubkey:
   ```js
   const [keyStatePda] = PublicKey.findProgramAddressSync(
     [Buffer.from('key_state'), assetPubkey.toBuffer()],
     PROGRAM_ID
   );
   ```

2. **Fetch and deserialize** the KeyState account (see layout in Account Structures above).

3. **Get the current slot** via `connection.getSlot('confirmed')`.

4. **Apply the refill formula** for each bucket:

   ```
   elapsed   = current_slot - last_update
   refill    = min(capacity, capacity * elapsed / refill_period)   // use BigInt to avoid overflow
   available = min(capacity, level + refill)
   ```

### JavaScript Helper

The `web-lite/src/rateLimits.js` module provides ready-to-use helpers:

```js
import { getKeyAllowance, bucketAvailableNow, parseKeyState } from './rateLimits.js';

// High-level: fetch + compute in one call
const allowance = await getKeyAllowance(connection, assetPubkey);
console.log(allowance.sellAvailable, allowance.borrowAvailable);

// Low-level: parse raw account data + compute
const ks = parseKeyState(accountData);
const available = bucketAvailableNow(ks.sellBucket, currentSlot);
```

### Rust Helper

The `RateBucket` struct has a read-only method:

```rust
use hardig::state::{KeyState, RateBucket};

let available_sell = key_state.sell_bucket.available_now(current_slot);
let available_borrow = key_state.borrow_bucket.available_now(current_slot);
```

### Total Lifetime Limits

In addition to rate buckets, keys may have optional lifetime caps:

- **`total_sell_limit`** (offset 137 in KeyState): Max navSOL shares this key can ever sell. 0 = no cap.
- **`total_sold`** (offset 145): Accumulator of shares sold so far.
- **`total_borrow_limit`** (offset 153): Max lamports this key can ever borrow. 0 = no cap.
- **`total_borrowed`** (offset 161): Accumulator of lamports borrowed so far.

When a lifetime limit is nonzero and the accumulator would exceed it, the transaction fails with `TotalLimitExceeded`. Both rate-bucket and total-limit checks are enforced post-CPI using the actual delta (not the requested amount).

### Units

- **Sell bucket:** capacity and level are in navSOL shares (9 decimals, same as SPL token amounts).
- **Borrow bucket:** capacity and level are in lamports.
- **Total sell limit/sold:** navSOL shares (same as sell bucket).
- **Total borrow limit/borrowed:** lamports (same as borrow bucket).
- **Refill period:** measured in Solana slots (~400ms each).

## Artwork (Custom Key Visuals)

Positions can optionally bind to an artwork set from a trusted third-party provider. When artwork is configured (`artwork_id` on PositionState), newly minted key NFTs receive a custom image URI from an **ArtworkImage** PDA owned by the trusted provider program.

### Trust Chain

Three accounts are validated via `remaining_accounts` when minting a key with artwork:

1. **ArtworkReceipt** (index 0) — Proves the position purchased the artwork set. Owned by the trusted provider program. Contains `artwork_set`, `position_seed`, and `buyer`.
2. **TrustedProvider PDA** (index 1) — Härdig-owned PDA at `["trusted_provider", provider_program_id]`. Must be active. Registered by the protocol admin via `add_trusted_provider`.
3. **ArtworkImage PDA** (index 2) — Owned by the trusted provider program. Contains the `image_uri` for a specific key type and permissions combination.

### ArtworkImage PDA Derivation (with Fallback)

The ArtworkImage PDA is derived from the trusted provider program:

```js
// Seeds: ["artwork_image", artwork_set, key_type, permissions]
const [artworkImagePda] = PublicKey.findProgramAddressSync(
  [Buffer.from('artwork_image'), artworkSet.toBuffer(), Buffer.from([keyType]), Buffer.from([permissions])],
  trustedProviderProgramId
);
```

**Key types:** `0` = admin, `1` = delegate, `2` = recovery.

Since delegates can have many permission bitmask combinations, the on-chain validation supports a **fallback**: if no exact-match ArtworkImage exists for `(key_type, permissions)`, it accepts the catch-all PDA derived with `permissions = 0`.

**Client-side resolution:**

```js
// 1. Try exact match
const [exactPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('artwork_image'), artworkSet.toBuffer(), Buffer.from([1]), Buffer.from([permissions])],
  trustedProviderProgramId
);
const exactInfo = await connection.getAccountInfo(exactPda);

// 2. If exact match doesn't exist, use catch-all (permissions=0)
let artworkImagePda = exactPda;
if (!exactInfo || exactInfo.data.length === 0) {
  const [fallbackPda] = PublicKey.findProgramAddressSync(
    [Buffer.from('artwork_image'), artworkSet.toBuffer(), Buffer.from([1]), Buffer.from([0])],
    trustedProviderProgramId
  );
  artworkImagePda = fallbackPda;
}

// 3. Pass as remaining_accounts[2] in authorize_key / configure_recovery
```

Artwork is optional — all key-minting instructions use graceful fallback. If the remaining accounts are omitted or the receipt/image accounts are closed, the key is minted with the default metadata (no custom image).

## Discovery

There are two types of keys to discover: admin keys (one per position) and delegated keys (created via `authorize_key`). The discovery approach avoids `getProgramAccounts` on the MPL-Core program (which most RPC providers reject due to the massive account set).

### Step 1: Scan Hardig Program Accounts

Fetch all `PositionState` accounts (238 bytes) and `KeyState` accounts (169 bytes) from the Hardig program using size filters. When discovering keys for a specific position, add a `memcmp` filter on `authority_seed` (offset 8) to avoid fetching all keys protocol-wide:

```js
const PROGRAM_ID = new PublicKey('4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p');
const POSITION_SIZE = 238;
const KEY_STATE_SIZE = 169;

// Discover all positions and keys (initial wallet scan)
const [positionAccounts, keyStateAccounts] = await Promise.all([
  connection.getProgramAccounts(PROGRAM_ID, {
    filters: [{ dataSize: POSITION_SIZE }],
    commitment: 'confirmed',
  }),
  connection.getProgramAccounts(PROGRAM_ID, {
    filters: [{ dataSize: KEY_STATE_SIZE }],
    commitment: 'confirmed',
  }),
]);

// Or: discover keys for a specific position only (much faster at scale)
// authority_seed is the first field after the discriminator (offset 8)
const positionKeyStates = await connection.getProgramAccounts(PROGRAM_ID, {
  filters: [
    { dataSize: KEY_STATE_SIZE },
    { memcmp: { offset: 8, bytes: authoritySeed.toBase58() } },
  ],
  commitment: 'confirmed',
});
```

### Step 2: Check Admin Keys

Each `PositionState` stores `authority_seed` at bytes 8..40 (the original admin asset pubkey). The current admin key is at `current_admin_asset` (bytes 132..164). Load the MPL-Core asset account for each current admin asset and check if the owner matches the target wallet:

```js
// Parse current_admin_asset from each PositionState
const positions = positionAccounts.map(({ pubkey, account }) => ({
  posPda: pubkey,
  authoritySeed: new PublicKey(account.data.slice(8, 40)),
  currentAdminAsset: new PublicKey(account.data.slice(132, 164)),
}));

// Batch-fetch the MPL-Core asset accounts
const assetInfos = await connection.getMultipleAccountsInfo(
  positions.map(p => p.currentAdminAsset)
);

// MPL-Core AssetV1: byte 0 = Key enum (1 = AssetV1), bytes 1..33 = owner
for (let i = 0; i < positions.length; i++) {
  const info = assetInfos[i];
  if (!info || info.data[0] !== 1) continue;
  const owner = new PublicKey(info.data.slice(1, 33));
  if (owner.equals(walletPubkey)) {
    // This wallet is the admin of positions[i]
  }
}
```

### Step 3: Check Delegated Keys

Each `KeyState` stores `authority_seed` at bytes 8..40 and `asset` at bytes 40..72. Load those MPL-Core asset accounts and:

1. Check the `owner` field (bytes 1..33) matches the target wallet.
2. Read the `position` attribute from the Attributes plugin to find which `admin_asset` this key is bound to.
3. Look up the corresponding `PositionState` via the admin asset.

```js
// Parse asset pubkey from each KeyState (authority_seed at 8..40, asset at 40..72)
const keyStates = keyStateAccounts.map(({ pubkey, account }) => ({
  keyStatePda: pubkey,
  authoritySeed: new PublicKey(account.data.slice(8, 40)),
  asset: new PublicKey(account.data.slice(40, 72)),
}));

// Batch-fetch the MPL-Core asset accounts
const delegatedInfos = await connection.getMultipleAccountsInfo(
  keyStates.map(ks => ks.asset)
);

for (let i = 0; i < keyStates.length; i++) {
  const info = delegatedInfos[i];
  if (!info || info.data[0] !== 1) continue;
  const owner = new PublicKey(info.data.slice(1, 33));
  if (!owner.equals(walletPubkey)) continue;

  // Read "position" attribute from the Attributes plugin to find the bound admin_asset
  const positionBinding = readAttributeFromAssetData(info.data, 'position');
  // positionBinding is the admin_asset pubkey as a string
  // Use it to look up the corresponding PositionState PDA
}
```

### Choosing the Best Key

When a wallet holds multiple keys for the same or different positions, prefer the key with the most permission bits set. Break ties by preferring keys that include `PERM_MANAGE_KEYS` (0x20). The `permissions` attribute is stored as a decimal string in the MPL-Core asset's Attributes plugin.

### Reference Implementation

See `web-lite/src/discovery.js` for a complete working implementation of this discovery pattern.

## Error Codes

The `HardigError` enum defines all program errors. The Anchor IDL maps these to numeric codes. Key errors integrators should handle:

| Error | Description |
|-------|-------------|
| `Unauthorized` | Signer is not authorized for this action |
| `InsufficientPermission` | Key lacks the required permission bit |
| `KeyNotHeld` | Signer does not own the key NFT |
| `WrongPosition` | Key's `position` attribute does not match the target position |
| `RateLimitExceeded` | Rate-limited key has insufficient bucket tokens |
| `TotalLimitExceeded` | Lifetime cap on sell or borrow exceeded |
| `InvalidInitialFill` | Initial fill basis points must be 0-10000 |
| `BorrowCapacityExceeded` | Borrow amount exceeds available capacity |
| `SlippageExceeded` | Output amount below `min_out` parameter |
| `InsufficientFunds` | Not enough funds for the operation |

Full error enum: `programs/hardig/src/errors.rs`

## External Dependencies

| Dependency | Program ID | Purpose |
|------------|-----------|---------|
| MPL-Core | `CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d` | NFT minting, ownership, attributes |
| Mayflower | `AVMmmRzwc2kETQNhPiFVnyu62HrgsQXTD6D7SnSfEz7v` | navSOL buy/sell/borrow/repay CPI |
| SPL Token | `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA` | Token account operations |
| Associated Token | `ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL` | ATA derivation and validation |
