# Artist Artwork Marketplace

## Problem

All Hardig key NFTs share the same default image. Users want custom artwork for their position keys, and Solana artists want to sell it. Payment must be enforced on-chain -- users can't get custom artwork without paying the artist.

## Solution

A cross-program architecture. A separate artwork program handles artist registration, pricing, and payment. Hardig just reads the proof-of-purchase (a receipt PDA) and uses the image URI when minting key NFTs.

Hardig never CPIs into the artwork program. It only reads account data -- zero reentrancy risk, minimal CU cost (~200 bytes of reads).

## Design Principles

- **Separation of concerns.** Artwork commerce lives in its own program. Hardig stays focused on position management.
- **Multiple trusted programs.** The protocol admin can whitelist any number of artwork programs. Adding/removing doesn't require redeploying Hardig.
- **No server keys.** All verification is on-chain. The config PDA private key never leaves the program.
- **Receipt snapshots.** Image URIs are copied into the receipt at purchase time. If the artist later changes their listing or the artwork program goes away, the buyer keeps what they paid for.
- **Position-bound, not wallet-bound.** The receipt is tied to `authority_seed` (permanent), so artwork survives admin key recovery and works for any wallet that holds the admin key.

## External Artwork Program (`hardig-artwork`)

A separate Solana program in its own repository. Anyone can build a compatible artwork program -- the Hardig protocol admin whitelists trusted programs via on-chain PDAs.

**Recommended stack:** Anchor 0.32.1 (matches Hardig), LiteSVM for tests.

### ArtworkSet

An artist's listing. One per artwork bundle.

#### State Definition

```rust
use anchor_lang::prelude::*;

#[account]
pub struct ArtworkSet {
    /// Artist wallet that receives SOL payments.
    pub artist: Pubkey,
    /// Human-readable name for this artwork set.
    pub set_name: String,
    /// Price per position in lamports.
    pub price_lamports: u64,
    /// Image URL for admin keys (max 128 bytes). Should point to Irys/Arweave.
    pub admin_image_uri: String,
    /// Image URL for delegated keys (max 128 bytes). Should point to Irys/Arweave.
    pub delegate_image_uri: String,
    /// Artist can deactivate to stop new sales.
    pub active: bool,
    /// Total number of purchases.
    pub sales_count: u32,
    /// PDA bump seed.
    pub bump: u8,
}

impl ArtworkSet {
    pub const SEED: &'static [u8] = b"artwork_set";
    pub const MAX_NAME_LEN: usize = 32;
    pub const MAX_IMAGE_URI_LEN: usize = 128;
    // discriminator(8) + artist(32) + set_name(4+32) + price_lamports(8)
    // + admin_image_uri(4+128) + delegate_image_uri(4+128) + active(1)
    // + sales_count(4) + bump(1)
    pub const SIZE: usize = 8 + 32 + (4 + 32) + 8 + (4 + 128) + (4 + 128) + 1 + 4 + 1; // 354
}
```

**PDA seeds:** `[b"artwork_set", artist.key().as_ref(), set_name.as_bytes()]`

#### Byte Layout

All fields are Borsh-serialized, little-endian.

| Offset | Size | Field | Type | Description |
|--------|------|-------|------|-------------|
| 0 | 8 | discriminator | `[u8; 8]` | `sha256("account:ArtworkSet")[..8]` |
| 8 | 32 | `artist` | `Pubkey` | Artist wallet |
| 40 | 4 | `set_name` length | `u32 LE` | Borsh string length prefix |
| 44 | 0..32 | `set_name` data | `[u8]` | UTF-8 bytes (max 32) |
| 44+N | 8 | `price_lamports` | `u64 LE` | Price in lamports |
| 52+N | 4 | `admin_image_uri` length | `u32 LE` | Borsh string length prefix |
| 56+N | 0..128 | `admin_image_uri` data | `[u8]` | UTF-8 bytes (max 128) |
| 56+N+A | 4 | `delegate_image_uri` length | `u32 LE` | Borsh string length prefix |
| 60+N+A | 0..128 | `delegate_image_uri` data | `[u8]` | UTF-8 bytes (max 128) |
| 60+N+A+D | 1 | `active` | `bool` | 0 = inactive, 1 = active |
| 61+N+A+D | 4 | `sales_count` | `u32 LE` | Total purchases |
| 65+N+A+D | 1 | `bump` | `u8` | PDA bump seed |

Where N = actual `set_name` byte length, A = actual `admin_image_uri` byte length, D = actual `delegate_image_uri` byte length.

**Rent cost:** ~0.003 SOL

### ArtworkReceipt

Proof of purchase, bound to a specific position. This is the account Hardig reads.

#### State Definition

```rust
#[account]
pub struct ArtworkReceipt {
    /// Which ArtworkSet was purchased.
    pub artwork_set: Pubkey,
    /// Bound to this position's authority_seed.
    pub position_seed: Pubkey,
    /// Who paid for the artwork.
    pub buyer: Pubkey,
    /// Unix timestamp of purchase.
    pub purchased_at: i64,
    /// Admin key image URI, snapshotted at purchase time.
    pub admin_image_uri: String,
    /// Delegate key image URI, snapshotted at purchase time.
    pub delegate_image_uri: String,
    /// PDA bump seed.
    pub bump: u8,
}

impl ArtworkReceipt {
    pub const SEED: &'static [u8] = b"artwork_receipt";
    pub const MAX_IMAGE_URI_LEN: usize = 128;
    // discriminator(8) + artwork_set(32) + position_seed(32) + buyer(32)
    // + purchased_at(8) + admin_image_uri(4+128) + delegate_image_uri(4+128) + bump(1)
    pub const SIZE: usize = 8 + 32 + 32 + 32 + 8 + (4 + 128) + (4 + 128) + 1; // 377
}
```

**PDA seeds:** `[b"artwork_receipt", artwork_set.key().as_ref(), position_authority_seed.as_ref()]`

#### Byte Layout (Hardig reads this)

This is the critical layout that Hardig's artwork reader module must match exactly.

| Offset | Size | Field | Type | Description |
|--------|------|-------|------|-------------|
| 0 | 8 | discriminator | `[u8; 8]` | `sha256("account:ArtworkReceipt")[..8]` |
| 8 | 32 | `artwork_set` | `Pubkey` | The ArtworkSet this receipt is for |
| 40 | 32 | `position_seed` | `Pubkey` | The position's `authority_seed` |
| 72 | 32 | `buyer` | `Pubkey` | Who paid |
| 104 | 8 | `purchased_at` | `i64 LE` | Unix timestamp |
| 112 | 4 | `admin_image_uri` length | `u32 LE` | Borsh string length prefix |
| 116 | 0..128 | `admin_image_uri` data | `[u8]` | UTF-8 bytes (max 128) |
| 116+A | 4 | `delegate_image_uri` length | `u32 LE` | Borsh string length prefix |
| 120+A | 0..128 | `delegate_image_uri` data | `[u8]` | UTF-8 bytes (max 128) |
| 120+A+D | 1 | `bump` | `u8` | PDA bump seed |

Where A = actual `admin_image_uri` byte length, D = actual `delegate_image_uri` byte length.

**Rent cost:** ~0.003 SOL (paid by buyer, reclaimable via `close_receipt`)

#### Computing the Discriminator

Anchor computes discriminators as:

```rust
use anchor_lang::solana_program::hash::hash;

let disc = &hash(b"account:ArtworkReceipt").to_bytes()[..8];
```

In JavaScript:

```js
import { createHash } from 'crypto';

const disc = createHash('sha256')
  .update('account:ArtworkReceipt')
  .digest()
  .slice(0, 8);
```

### Instructions

#### 1. `create_artwork_set`

**Who:** Artist (signer, payer).

**Purpose:** Register a new artwork listing.

**Accounts:**

| # | Account | Signer | Mut | Description |
|---|---------|--------|-----|-------------|
| 0 | `artist` | yes | yes | Artist wallet (payer for PDA rent) |
| 1 | `artwork_set` | no | yes | ArtworkSet PDA (init) |
| 2 | `system_program` | no | no | System program |

**Args:**

| Name | Type | Description |
|------|------|-------------|
| `set_name` | `String` | Human-readable name (max 32 bytes) |
| `price_lamports` | `u64` | Price per position in lamports |
| `admin_image_uri` | `String` | Admin key image URL (max 128 bytes) |
| `delegate_image_uri` | `String` | Delegate key image URL (max 128 bytes) |

**Behavior:**
- Validates string lengths (`set_name <= 32`, URIs `<= 128`)
- Initializes ArtworkSet PDA with `active = true`, `sales_count = 0`
- PDA seeds: `[b"artwork_set", artist.key(), set_name.as_bytes()]`

#### 2. `update_artwork_set`

**Who:** Artist (must match `artwork_set.artist`).

**Purpose:** Update price, images, or active flag. Does NOT affect existing receipts.

**Accounts:**

| # | Account | Signer | Mut | Description |
|---|---------|--------|-----|-------------|
| 0 | `artist` | yes | no | Artist wallet |
| 1 | `artwork_set` | no | yes | ArtworkSet PDA |

**Args:**

| Name | Type | Description |
|------|------|-------------|
| `price_lamports` | `Option<u64>` | New price (None = keep current) |
| `admin_image_uri` | `Option<String>` | New admin image (None = keep current) |
| `delegate_image_uri` | `Option<String>` | New delegate image (None = keep current) |
| `active` | `Option<bool>` | New active state (None = keep current) |

**Behavior:**
- Validates `artist.key() == artwork_set.artist`
- Updates only the fields that are `Some`

#### 3. `purchase_artwork`

**Who:** Any user (buyer).

**Purpose:** Pay the artist and create a receipt bound to a specific position.

**Accounts:**

| # | Account | Signer | Mut | Description |
|---|---------|--------|-----|-------------|
| 0 | `buyer` | yes | yes | Buyer wallet (payer for SOL transfer + PDA rent) |
| 1 | `artwork_set` | no | yes | ArtworkSet PDA (mut for `sales_count` increment) |
| 2 | `artist` | no | yes | Artist wallet (receives SOL payment) |
| 3 | `receipt` | no | yes | ArtworkReceipt PDA (init) |
| 4 | `system_program` | no | no | System program |

**Args:**

| Name | Type | Description |
|------|------|-------------|
| `position_authority_seed` | `Pubkey` | The position's `authority_seed` to bind the receipt to |

**Behavior:**
1. Validates `artwork_set.active == true`
2. Validates `artist.key() == artwork_set.artist`
3. Transfers `artwork_set.price_lamports` SOL from `buyer` to `artist` via `system_program::transfer`
4. Creates ArtworkReceipt PDA:
   - Seeds: `[b"artwork_receipt", artwork_set.key(), position_authority_seed]`
   - Snapshots `admin_image_uri` and `delegate_image_uri` from ArtworkSet
   - Sets `buyer = buyer.key()`, `purchased_at = Clock::get()?.unix_timestamp`
5. Increments `artwork_set.sales_count`

**Validation:**
- One receipt per (artwork_set, position) pair -- PDA derivation enforces this
- If the buyer wants different artwork for the same position, they must `close_receipt` first, then purchase again

#### 4. `close_receipt`

**Who:** Buyer (must match `receipt.buyer`).

**Purpose:** Reclaim rent from a receipt. The position loses its custom artwork.

**Accounts:**

| # | Account | Signer | Mut | Description |
|---|---------|--------|-----|-------------|
| 0 | `buyer` | yes | yes | Buyer wallet (receives rent refund) |
| 1 | `receipt` | no | yes | ArtworkReceipt PDA (close) |

**Behavior:**
- Validates `buyer.key() == receipt.buyer`
- Closes the ArtworkReceipt account, returns rent to buyer
- After this, Hardig's `authorize_key` will fall back to the default image

## Changes to Hardig

### Trusted Provider Whitelist

Small marker PDAs that the protocol admin creates to whitelist provider programs.

#### State Definition

```rust
/// Marker PDA for a trusted provider program.
/// PDA seeds = [b"trusted_provider", program_id].
#[account]
pub struct TrustedProvider {
    /// The trusted provider program ID.
    pub program_id: Pubkey,
    /// Protocol admin who added it.
    pub added_by: Pubkey,
    /// Can be deactivated without deleting.
    pub active: bool,
    /// PDA bump seed.
    pub bump: u8,
}

impl TrustedProvider {
    pub const SEED: &'static [u8] = b"trusted_provider";
    // discriminator(8) + program_id(32) + added_by(32) + active(1) + bump(1)
    pub const SIZE: usize = 8 + 32 + 32 + 1 + 1; // 74
}
```

**PDA seeds:** `[b"trusted_provider", program_id.as_ref()]`

#### Byte Layout

| Offset | Size | Field | Type | Description |
|--------|------|-------|------|-------------|
| 0 | 8 | discriminator | `[u8; 8]` | Anchor discriminator |
| 8 | 32 | `program_id` | `Pubkey` | Trusted provider program |
| 40 | 32 | `added_by` | `Pubkey` | Admin who registered it |
| 72 | 1 | `active` | `bool` | 0 = inactive, 1 = active |
| 73 | 1 | `bump` | `u8` | PDA bump seed |

**Total size:** 74 bytes

#### New Instructions on Hardig

**`add_trusted_provider_program`**

| # | Account | Signer | Mut | Description |
|---|---------|--------|-----|-------------|
| 0 | `admin` | yes | yes | Protocol admin (payer) |
| 1 | `config` | no | no | ProtocolConfig PDA (validates admin) |
| 2 | `trusted_provider` | no | yes | TrustedProvider PDA (init) |
| 3 | `system_program` | no | no | System program |

Args: `program_id: Pubkey`

Requires: `admin.key() == config.admin`

**`remove_trusted_provider_program`**

| # | Account | Signer | Mut | Description |
|---|---------|--------|-----|-------------|
| 0 | `admin` | yes | no | Protocol admin |
| 1 | `config` | no | no | ProtocolConfig PDA (validates admin) |
| 2 | `trusted_provider` | no | yes | TrustedProvider PDA |

Args: none. Sets `active = false`.

### Artwork Reader Module

New module `src/artwork/mod.rs` -- raw byte deserialization of ArtworkReceipt accounts. Same pattern as `mayflower/floor.rs` reads Mayflower PersonalPosition data.

```rust
use anchor_lang::prelude::*;
use crate::errors::HardigError;

/// Expected Anchor discriminator for ArtworkReceipt.
/// sha256("account:ArtworkReceipt")[..8]
pub const ARTWORK_RECEIPT_DISCRIMINATOR: [u8; 8] = [/* computed at build time */];

// Fixed offsets in the ArtworkReceipt account.
const ARTWORK_SET_OFFSET: usize = 8;
const POSITION_SEED_OFFSET: usize = 40;
const BUYER_OFFSET: usize = 72;
const PURCHASED_AT_OFFSET: usize = 104;
const ADMIN_IMAGE_URI_OFFSET: usize = 112;

/// Read and validate the position_seed from an ArtworkReceipt.
pub fn read_receipt_position_seed(data: &[u8]) -> Result<Pubkey> {
    require!(data.len() >= 72, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_RECEIPT_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    Ok(Pubkey::try_from(&data[POSITION_SEED_OFFSET..POSITION_SEED_OFFSET + 32]).unwrap())
}

/// Read the admin_image_uri from an ArtworkReceipt.
pub fn read_admin_image(data: &[u8]) -> Result<String> {
    require!(data.len() > ADMIN_IMAGE_URI_OFFSET + 4, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_RECEIPT_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    read_borsh_string(data, ADMIN_IMAGE_URI_OFFSET)
}

/// Read the delegate_image_uri from an ArtworkReceipt.
/// Located immediately after the admin_image_uri string.
pub fn read_delegate_image(data: &[u8]) -> Result<String> {
    require!(data.len() > ADMIN_IMAGE_URI_OFFSET + 4, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_RECEIPT_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    // Skip past admin_image_uri to find delegate_image_uri
    let admin_len = u32::from_le_bytes(
        data[ADMIN_IMAGE_URI_OFFSET..ADMIN_IMAGE_URI_OFFSET + 4].try_into().unwrap()
    ) as usize;
    let delegate_offset = ADMIN_IMAGE_URI_OFFSET + 4 + admin_len;
    read_borsh_string(data, delegate_offset)
}

/// Read a Borsh-encoded String (4-byte LE length prefix + UTF-8 bytes) at the given offset.
fn read_borsh_string(data: &[u8], offset: usize) -> Result<String> {
    require!(data.len() >= offset + 4, HardigError::InvalidArtworkReceipt);
    let len = u32::from_le_bytes(
        data[offset..offset + 4].try_into().unwrap()
    ) as usize;
    require!(len <= 128, HardigError::InvalidArtworkReceipt);
    require!(data.len() >= offset + 4 + len, HardigError::InvalidArtworkReceipt);
    String::from_utf8(data[offset + 4..offset + 4 + len].to_vec())
        .map_err(|_| error!(HardigError::InvalidArtworkReceipt))
}
```

### Receipt Validation in Hardig Instructions

When `create_position` or `authorize_key` receives an artwork receipt:

```rust
// 1. The receipt account is passed as a remaining_account
let receipt_info = &ctx.remaining_accounts[0];

// 2. The TrustedProvider PDA is passed as a remaining_account
let trusted_info = &ctx.remaining_accounts[1];

// 3. Deserialize the TrustedProvider and validate
let trusted_data = trusted_info.try_borrow_data()?;
// ... check discriminator, check active == true ...
let trusted_provider_id = Pubkey::try_from(&trusted_data[8..40]).unwrap();

// 4. Verify the receipt is owned by the trusted program
require!(
    *receipt_info.owner == trusted_provider_id,
    HardigError::UntrustedProviderProgram
);

// 5. Verify the TrustedProvider PDA is valid
let (expected_trusted_pda, _) = Pubkey::find_program_address(
    &[TrustedProvider::SEED, receipt_info.owner.as_ref()],
    &crate::ID,
);
require!(
    trusted_info.key() == expected_trusted_pda,
    HardigError::UntrustedProviderProgram
);

// 6. Read and validate receipt data
let receipt_data = receipt_info.try_borrow_data()?;
let position_seed = artwork::read_receipt_position_seed(&receipt_data)?;
require!(
    position_seed == ctx.accounts.admin_asset.key(),  // or position.authority_seed
    HardigError::ArtworkReceiptPositionMismatch
);

// 7. Read the image URI
let image_uri = artwork::read_admin_image(&receipt_data)?;
// Pass to metadata_uri() as image_override
```

### Integration into Existing Instructions

**`create_position`** -- when `artwork_id` is `Some(receipt_pubkey)`:
1. Receipt account passed as `remaining_accounts[0]`
2. TrustedProvider PDA passed as `remaining_accounts[1]`
3. Validate: `receipt.owner == trusted_provider.program_id`, `trusted_provider.active`, receipt PDA is valid, `receipt.position_seed == admin_asset.key()`
4. Read `admin_image_uri` from receipt
5. Pass as `image_override` to `metadata_uri()`
6. Store `artwork_id = Some(receipt_pubkey)` on position

**`authorize_key`** -- reads `position.artwork_id`:
1. If `Some(receipt_pubkey)`, expect receipt in `remaining_accounts[0]`
2. Verify `remaining_accounts[0].key() == receipt_pubkey`
3. Read `delegate_image_uri` from receipt
4. Pass as `image_override` to `metadata_uri()`
5. If receipt account is missing or closed, fall back to default `KEY_IMAGE`

**`set_position_artwork`** (new instruction) -- admin can change or clear artwork:
- Validates admin key via `validate_key(PERM_MANAGE_KEYS)`
- Sets `position.artwork_id` to new receipt or `None`
- Does NOT re-mint existing NFTs -- only affects future `authorize_key` calls

### What `artwork_id` Points To

`PositionState.artwork_id: Option<Pubkey>` = the **ArtworkReceipt** pubkey. The receipt contains both image URIs (admin and delegate) and is position-bound via PDA seeds.

### New Error Variants

Add to `HardigError` enum in `errors.rs`:

```rust
#[msg("Artwork receipt is invalid or malformed")]
InvalidArtworkReceipt,
#[msg("Receipt is not from a trusted provider program")]
UntrustedProviderProgram,
#[msg("Artwork receipt does not match this position")]
ArtworkReceiptPositionMismatch,
```

## How the Pieces Connect

```
                    +---------------------------+
                    |  Artwork Program           |
                    |  (hardig-artwork)          |
                    |                            |
  Artist ---------> |  create_artwork_set        |
                    |  update_artwork_set        |
                    |                            |
  User ----SOL----> |  purchase_artwork -------> | Artist wallet
                    |  (creates Receipt PDA)     |
                    +-------------+--------------+
                                  |
                    Receipt PDA   |  (owned by artwork program)
                    persists      |
                    on-chain      |
                                  |
                    +-------------+---------------+
                    |  Hardig     |  Program       |
                    |             v                |
  User -----------> |  create_position            |
                    |    reads receipt bytes       |
                    |    validates trusted provider |
                    |    passes image_override     |
                    |    stores artwork_id         |
                    |                              |
  Admin ----------> |  authorize_key              |
                    |    reads position.artwork_id |
                    |    reads receipt bytes       |
                    |    passes image_override     |
                    +-+----------------------------+
```

## End-to-End User Flow

### Artist Setup

1. Artist uploads admin and delegate images to Irys/Arweave, gets permanent URLs
2. Artist calls `create_artwork_set` with name, price, and both image URLs
3. ArtworkSet PDA is created -- listing is now visible to users

### User Purchase + Position Creation

1. User browses available ArtworkSets (client reads ArtworkSet accounts from the artwork program)
2. User calls `purchase_artwork` with the desired ArtworkSet and their position's `authority_seed`
3. SOL is transferred from user to artist atomically; ArtworkReceipt is created
4. User calls Hardig `create_position` with `artwork_id = Some(receipt_pubkey)` and the receipt + trusted-program accounts in remaining_accounts
5. Admin key NFT is minted with the artist's `admin_image_uri`

### Delegated Key Creation

1. Admin calls Hardig `authorize_key` for a position that has `artwork_id = Some(receipt_pubkey)`
2. Client includes the receipt account in remaining_accounts
3. Delegated key NFT is minted with the artist's `delegate_image_uri`

### Changing Artwork

1. Admin calls `set_position_artwork` with a new receipt (or `None` to clear)
2. Future delegated keys use the new artwork; existing NFTs are unchanged

## PDA Derivation Reference

### Artwork Program PDAs

```js
const ARTWORK_PROGRAM_ID = new PublicKey('...');

// ArtworkSet
const [artworkSetPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('artwork_set'), artistWallet.toBuffer(), Buffer.from(setName)],
  ARTWORK_PROGRAM_ID
);

// ArtworkReceipt
const [receiptPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('artwork_receipt'), artworkSet.toBuffer(), positionAuthoritySeed.toBuffer()],
  ARTWORK_PROGRAM_ID
);
```

### Hardig PDAs (new)

```js
const HARDIG_PROGRAM_ID = new PublicKey('4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p');

// TrustedProvider
const [trustedPda] = PublicKey.findProgramAddressSync(
  [Buffer.from('trusted_provider'), artworkProgramId.toBuffer()],
  HARDIG_PROGRAM_ID
);
```

## Security Properties

| Property | How |
|----------|-----|
| Can't get artwork without paying | Receipt PDA only created by `purchase_artwork` (which transfers SOL) |
| Receipt can't be forged | PDA with deterministic seeds, owned by artwork program |
| Receipt is position-bound | PDA seeds include `position_authority_seed` |
| No CPI reentrancy risk | Hardig reads bytes, never invokes artwork program |
| Bad provider program can be deactivated | Protocol admin sets `active = false` on whitelist PDA |
| Artist can't rug buyer's images | Receipt snapshots image URIs at purchase time |
| Config PDA private key stays off servers | No server signing needed |
| Artwork survives admin recovery | Receipt bound to `authority_seed` (permanent), not wallet |

## Security Considerations

### Receipt lifetime

If a buyer calls `close_receipt` to reclaim rent, the position's `artwork_id` becomes a dangling pointer. Hardig's `authorize_key` should gracefully fall back to the default image if the receipt account no longer exists, rather than failing.

### Image URI content

Image URIs from receipts are passed through `json_escape()` in `metadata_uri()`, preventing JSON injection in NFT metadata. URIs should point to permanent storage (Irys/Arweave) rather than mutable URLs.

### Artwork program upgrades

Hardig reads raw bytes at fixed offsets, not via CPI. If the artwork program upgrades and changes its account layout, old receipts keep their old layout on-chain. A new program version with a different discriminator would need a new TrustedProvider whitelist entry.

### Both images in one receipt

Storing both `admin_image_uri` and `delegate_image_uri` in the receipt means one purchase covers all key types. The artist designs a cohesive set, not individual images.

### Price manipulation

The artwork program should validate that `artist.key() == artwork_set.artist` in `purchase_artwork` to prevent SOL from being sent to the wrong wallet. Price is read from the ArtworkSet at purchase time.

## Costs

| Account | Size | Rent | Paid by | Reclaimable |
|---------|------|------|---------|-------------|
| ArtworkSet | 354 bytes | ~0.003 SOL | Artist | Yes (via closing) |
| ArtworkReceipt | 377 bytes | ~0.003 SOL | Buyer | Yes (via `close_receipt`) |
| TrustedProvider | 74 bytes | ~0.001 SOL | Protocol admin | Yes (via closing) |

## Testing Strategy

### Artwork Program (standalone)

Test with LiteSVM in the artwork program's own repo:

1. **Happy path:** create set, purchase, verify receipt data matches
2. **Price enforcement:** purchase with insufficient SOL fails
3. **Inactive set:** purchase from deactivated set fails
4. **One receipt per position:** second purchase for same (set, position) pair fails
5. **Artist validation:** only artist can update their set
6. **Close receipt:** buyer reclaims rent, account is closed
7. **Update set:** changes don't affect existing receipts

### Hardig Integration

Test in Hardig's integration test suite:

1. **With artwork:** create position with valid receipt, verify NFT image matches
2. **Without artwork:** create position with `artwork_id = None`, verify default image
3. **Untrusted program:** receipt from non-whitelisted program is rejected
4. **Wrong position:** receipt bound to different position is rejected
5. **Deactivated provider:** receipt from deactivated trusted provider is rejected
6. **Delegate key:** authorize_key reads delegate image from receipt
7. **Closed receipt:** authorize_key falls back to default image
8. **Set position artwork:** admin can change/clear artwork_id

## Implementation Phases

1. **Design & build artwork program** (separate repo)
2. **Add TrustedProvider state + admin instructions to Hardig**
3. **Add artwork reader module to Hardig** (`src/artwork/mod.rs`, pattern from `mayflower/floor.rs`)
4. **Wire into create_position** (remaining_accounts, validation, image_override)
5. **Wire into authorize_key** (read position.artwork_id, pass delegate image)
6. **Add set_position_artwork instruction**
7. **Client integration** (TUI, web-lite, API)

## Files to Create/Modify

### New Artwork Program (separate repo)

| File | Description |
|------|-------------|
| `programs/hardig-artwork/src/lib.rs` | Program entrypoint with all instructions |
| `programs/hardig-artwork/src/state.rs` | ArtworkSet, ArtworkReceipt definitions |
| `programs/hardig-artwork/src/errors.rs` | Error enum |
| `programs/hardig-artwork/src/instructions/` | One file per instruction |
| `programs/hardig-artwork/tests/integration.rs` | LiteSVM tests |

### Hardig Modifications

| File | Change |
|------|--------|
| `programs/hardig/src/state/mod.rs` | Add `TrustedProvider` account type |
| `programs/hardig/src/instructions/add_trusted_provider_program.rs` | New instruction |
| `programs/hardig/src/instructions/remove_trusted_provider_program.rs` | New instruction |
| `programs/hardig/src/errors.rs` | Add artwork error variants |
| `programs/hardig/src/artwork/mod.rs` | New module: raw byte reader for receipt accounts |
| `programs/hardig/src/lib.rs` | Add `pub mod artwork;`, new instruction dispatches |
| `programs/hardig/src/instructions/create_position.rs` | Accept optional receipt in remaining_accounts |
| `programs/hardig/src/instructions/authorize_key.rs` | Read `artwork_id`, pass delegate image |
| `programs/hardig/src/instructions/set_position_artwork.rs` | New instruction |
| `programs/hardig/tests/integration.rs` | Artwork integration tests |
