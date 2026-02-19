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
| 72 | 1 | `bump` | PDA bump seed |

**Total size:** 73 bytes

**Source:** `ProtocolConfig` in `programs/hardig/src/state.rs`

### PositionNFT

Represents a navSOL position controlled by an NFT keyring. One per admin key.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | discriminator | Anchor account discriminator |
| 8 | 32 | `admin_asset` | MPL-Core asset pubkey of the admin key NFT |
| 40 | 32 | `position_pda` | Mayflower PersonalPosition PDA owned by this position |
| 72 | 32 | `market_config` | MarketConfig PDA this position is bound to |
| 104 | 8 | `deposited_nav` | navSOL deposited (local tracking; Mayflower is source of truth) |
| 112 | 8 | `user_debt` | Total SOL borrowed (local tracking; Mayflower is source of truth) |
| 120 | 2 | `max_reinvest_spread_bps` | Max market/floor spread ratio (bps) for reinvest |
| 122 | 8 | `last_admin_activity` | Unix timestamp of last admin-signed instruction |
| 130 | 1 | `bump` | PDA bump seed |
| 131 | 1 | `authority_bump` | Bump for the per-position authority PDA |

**Total size:** 132 bytes

**Source:** `PositionNFT` in `programs/hardig/src/state.rs`

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

**Source:** `MarketConfig` in `programs/hardig/src/state.rs`

### KeyState

Mutable state for a delegated key NFT. Tracks rate-limit token buckets. Created for every delegated key via `authorize_key`.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | discriminator | Anchor account discriminator |
| 8 | 32 | `asset` | MPL-Core asset pubkey this state belongs to |
| 40 | 1 | `bump` | PDA bump seed |
| 41 | 32 | `sell_bucket` | RateBucket for `PERM_LIMITED_SELL` |
| 73 | 32 | `borrow_bucket` | RateBucket for `PERM_LIMITED_BORROW` |

**Total size:** 105 bytes

Each **RateBucket** (32 bytes, all little-endian u64):

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | `capacity` | Max tokens (shares for sell, lamports for borrow) |
| 8 | 8 | `refill_period` | Slots for a full refill from 0 to capacity |
| 16 | 8 | `level` | Tokens remaining at last update |
| 24 | 8 | `last_update` | Slot of last update |

**Source:** `KeyState`, `RateBucket` in `programs/hardig/src/state.rs`

## PDA Derivation

All PDAs use the Hardig program ID (`4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p`) as the program.

### Summary Table

| PDA | Seeds | Account Type |
|-----|-------|--------------|
| Protocol config | `["config"]` | `ProtocolConfig` |
| Position | `["position", admin_asset]` | `PositionNFT` |
| Per-position authority | `["authority", admin_asset]` | (no account data; signer PDA) |
| Key state | `["key_state", asset]` | `KeyState` |
| Market config | `["market_config", nav_mint]` | `MarketConfig` |

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

// PositionNFT (adminAsset is a PublicKey)
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
| `create_position` | Any signer | `max_reinvest_spread_bps: u16` | Mint admin key NFT and create position |
| `authorize_key` | `PERM_MANAGE_KEYS` | `permissions: u8`, rate-limit params | Mint a delegated key NFT to a target wallet |
| `revoke_key` | `PERM_MANAGE_KEYS` | -- | Close key authorization; burn NFT if admin holds it |
| `buy` | `PERM_BUY` | `amount: u64`, `min_out: u64` | Deposit SOL to buy nav tokens via Mayflower CPI |
| `withdraw` | `PERM_SELL` or `PERM_LIMITED_SELL` | `amount: u64`, `min_out: u64` | Sell nav tokens to withdraw SOL |
| `borrow` | `PERM_BORROW` or `PERM_LIMITED_BORROW` | `amount: u64` | Borrow SOL against nav-token floor |
| `repay` | `PERM_REPAY` | `amount: u64` | Repay borrowed SOL |
| `reinvest` | `PERM_REINVEST` | `min_out: u64` | Borrow available capacity and buy more nav tokens |
| `transfer_admin` | Protocol admin | `new_admin: Pubkey` | Transfer protocol admin rights |
| `migrate_config` | Protocol admin | -- | Migrate ProtocolConfig from v0 to v1 |

### Key Validation

Every position-modifying instruction validates the signer's key via `validate_key()`:

1. Deserializes the MPL-Core asset and confirms the signer is the owner.
2. Reads the `position` attribute from the asset's Attributes plugin and verifies it matches the position's `admin_asset`.
3. Reads the `permissions` attribute and checks the required permission bit is set.

The `withdraw` and `borrow` instructions additionally support rate-limited keys. If the key has `PERM_LIMITED_SELL` or `PERM_LIMITED_BORROW` (instead of the unrestricted `PERM_SELL`/`PERM_BORROW`), the instruction consumes from the corresponding `RateBucket` in the key's `KeyState` PDA.

## Reading Position Data

### Fetching a PositionNFT

```js
const positionInfo = await connection.getAccountInfo(positionPda);
const data = positionInfo.data;
const view = new DataView(data.buffer, data.byteOffset);

const adminAsset      = new PublicKey(data.slice(8, 40));
const mfPositionPda   = new PublicKey(data.slice(40, 72));
const marketConfigPda = new PublicKey(data.slice(72, 104));
const depositedNav    = Number(view.getBigUint64(104, true));
const userDebt        = Number(view.getBigUint64(112, true));
const spreadBps       = view.getUint16(120, true);
const lastAdmin       = Number(view.getBigInt64(122, true));
const bump            = data[130];
const authorityBump   = data[131];
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

### Units

- **Sell bucket:** capacity and level are in navSOL shares (9 decimals, same as SPL token amounts).
- **Borrow bucket:** capacity and level are in lamports.
- **Refill period:** measured in Solana slots (~400ms each).

## Discovery

There are two types of keys to discover: admin keys (one per position) and delegated keys (created via `authorize_key`). The discovery approach avoids `getProgramAccounts` on the MPL-Core program (which most RPC providers reject due to the massive account set).

### Step 1: Scan Hardig Program Accounts

Fetch all `PositionNFT` accounts (132 bytes) and `KeyState` accounts (105 bytes) from the Hardig program using size filters:

```js
const PROGRAM_ID = new PublicKey('4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p');
const POSITION_SIZE = 132;
const KEY_STATE_SIZE = 105;

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
```

### Step 2: Check Admin Keys

Each `PositionNFT` stores `admin_asset` at bytes 8..40. Load the MPL-Core asset account for each admin asset and check if the owner matches the target wallet:

```js
// Parse admin_asset from each PositionNFT
const positions = positionAccounts.map(({ pubkey, account }) => ({
  posPda: pubkey,
  adminAsset: new PublicKey(account.data.slice(8, 40)),
}));

// Batch-fetch the MPL-Core asset accounts
const assetInfos = await connection.getMultipleAccountsInfo(
  positions.map(p => p.adminAsset)
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

Each `KeyState` stores `asset` at bytes 8..40. Load those MPL-Core asset accounts and:

1. Check the `owner` field (bytes 1..33) matches the target wallet.
2. Read the `position` attribute from the Attributes plugin to find which `admin_asset` this key is bound to.
3. Look up the corresponding `PositionNFT` via the admin asset.

```js
// Parse asset pubkey from each KeyState
const keyStates = keyStateAccounts.map(({ pubkey, account }) => ({
  keyStatePda: pubkey,
  asset: new PublicKey(account.data.slice(8, 40)),
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
  // Use it to look up the corresponding PositionNFT PDA
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
