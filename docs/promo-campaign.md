# Promo Campaign: "Borrow Key Giveaway"

A forever-running promotional campaign that demonstrates Hardig's keyring model
by letting participants borrow small amounts of SOL from a shared position.

## Concept

Operator sets up a navSOL position as a permanent promotional sink. Participants
deposit a tiny amount (spam filter), then receive a rate-limited borrow key that
lets them periodically claim SOL — more than they deposited.

No liquidation risk in Mayflower means the position is safe regardless of how
much debt accumulates. navSOL yield gradually restores borrow capacity over time.

## User Flow

1. User visits landing page with Nirvana + Hardig info
2. User receives a Depositor key NFT
3. User deposits 0.01 SOL into the position (spam filter, proves intent)
4. User receives a LimitedBorrow key NFT (e.g. 0.02 SOL / 90 days)
5. User borrows periodically — each cycle they claim more than they deposited

## Economics

### Key Variables

- **D** = operator deposit (the promo budget, in SOL)
- **N** = number of participants
- **C** = per-user borrow capacity
- **R** = refill period
- **Y** = navSOL APY (~5-10%)

### Constraint

Total borrowable at any moment: floor_value(deposited_navSOL) - total_debt.
When borrow capacity hits zero, borrows fail until yield restores it.

### Example Configurations

| Operator Deposit | User Deposit | Borrow Limit | Refill  | Max Users | User Return |
|------------------|-------------|-------------|---------|-----------|-------------|
| 5 SOL            | 0.01 SOL    | 0.02 SOL    | 90 days | ~250      | ~8x/year    |
| 10 SOL           | 0.01 SOL    | 0.02 SOL    | 90 days | ~500      | ~8x/year    |
| 10 SOL           | 0.01 SOL    | 0.05 SOL    | 180 days| ~200      | ~10x/year   |

### Self-Sustainability

For the campaign to run indefinitely without topping up, yield must exceed drain:

    yield_per_year = D * Y
    drain_per_year = N_active * (C * 365 / R)

    Sustainable when: D * Y >= N_active * C * 365 / R

Example: 0.02 SOL / 90 days, 7% APY, 100 active users:
- Drain: 100 * 0.02 * (365/90) = 8.1 SOL/year
- Required deposit: 8.1 / 0.07 = ~116 SOL

In practice, not all users claim every cycle, so real numbers are more forgiving.

### Reinvest Leverage

Reinvesting (borrow SOL -> buy more navSOL) amplifies yield with no liquidation
risk. A 2x reinvest on 30 SOL gives 60 SOL earning yield, doubling capacity
regeneration. This is the main lever for making the campaign self-sustaining
with a smaller initial deposit.

## Operator Setup

1. Transfer shielded SOL to a fresh wallet
2. Create a position with an admin key
3. Buy navSOL (the promotional deposit)
4. Optionally reinvest to leverage yield
5. Set up recovery key on a separate cold wallet
6. Issue Depositor + LimitedBorrow keys to participants

## Required Technical Work

### 1. Batch Key Issuance (CLI)

Currently keys are issued one at a time in the TUI. Need a CLI command:

    hardig-tui batch-authorize \
      --permissions LimitedBorrow \
      --capacity 0.02 \
      --refill-days 90 \
      --wallets wallets.csv

Reads wallet addresses from CSV, issues a Depositor + LimitedBorrow key pair
to each in batch transactions.

### 2. Landing Page

Separate project from web-lite (which is an admin tool). This is a public-facing
marketing site with proper design, targeting end users who may have never heard
of Härdig or Mayflower.

Should include:
- Nirvana / Härdig explainer (what is navSOL, what are keys, why no liquidation risk)
- Live borrow capacity and active keys count (read from RPC)
- "Capacity available" / "Come back later" status
- One-click "Claim Key" flow with wallet connect
- Instructions for borrowing after claiming
- "Run your own campaign" section: how anyone can set up their own position,
  configure a promo, and fund a giveaway — with economics calculator and
  step-by-step guide

Can reuse web-lite's RPC/state utilities as a shared package, but the UI and
hosting are independent.

### 3. Self-Service Key Claim (on-chain)

The big unlock for "open forever" without manual admin involvement.

#### Code Isolation

Promo code lives inside the Härdig program but is isolated for future extraction:

```
programs/hardig/src/
  state.rs                    # core state (PositionState, ProtocolConfig, etc.)
  state/promo.rs              # PromoConfig, ClaimReceipt (promo-only state)
  instructions/
    mod.rs                    # core instruction re-exports
    promo/
      mod.rs                  # promo instruction re-exports
      create_promo.rs
      update_promo.rs
      claim_promo_key.rs
```

Promo instructions import from core (validate_key, metadata_uri, ProtocolConfig)
but core never imports from promo. This one-way dependency makes it straightforward
to extract into a separate program later if the keyring becomes shared across
multiple consumers.

#### Overview

Two new PDA types and three new instructions let anyone claim a promo key
without the admin being online. The admin configures the promo once, then
goes offline. Users self-serve.

#### New State: PromoConfig

PDA seeds: `[b"promo", authority_seed, name_suffix]` (multiple per position, keyed by name).

```
PromoConfig {
    authority_seed: Pubkey       // which position this promo is for
    permissions: u8              // key permissions (e.g. BUY | REPAY | LIMITED_BORROW)
    borrow_capacity: u64         // LimitedBorrow bucket capacity (lamports)
    borrow_refill_period: u64    // LimitedBorrow refill period (slots)
    sell_capacity: u64           // LimitedSell bucket capacity (0 if N/A)
    sell_refill_period: u64      // LimitedSell refill period (0 if N/A)
    min_deposit_lamports: u64    // suggested deposit amount (frontend reads this)
    max_claims: u32              // max total keys (0 = unlimited)
    claims_count: u32            // current count
    active: bool                 // admin can pause/resume
    name_suffix: String          // NFT name suffix (e.g. "Promo Borrow")
    image_uri: String            // custom NFT image URL (max 128 bytes, e.g. Irys/Arweave link)
    bump: u8
}
```

#### New State: ClaimReceipt

PDA seeds: `[b"claim", promo_pda, claimer_pubkey]`.

```
ClaimReceipt {
    claimer: Pubkey
    promo: Pubkey
    bump: u8
}
```

The `init` constraint fails if this PDA already exists — that's the
one-per-wallet guard. Rent (~0.001 SOL) is paid by the claimer.

#### New Instructions

**`create_promo`** (admin only)
Creates the PromoConfig PDA with all parameters. Admin specifies permissions,
rate limits, max claims, name suffix, custom image URI, and the suggested
deposit amount. The image URI (max 128 bytes) points to an Irys or Arweave
hosted image that will be used for all keys minted from this promo. If empty,
the default Härdig key image is used.

**`update_promo`** (admin only)
Toggle `active` flag, adjust `max_claims`. Cannot change permissions or rate
limits after creation (existing keys would be inconsistent).

**`claim_promo_key`** (anyone)
Self-mints a key from the promo template.

Accounts:
```
claimer (signer, mut)       — pays rent for ClaimReceipt + KeyState
promo (PromoConfig, mut)    — read params, increment claims_count
claim_receipt (init PDA)    — one-per-wallet guard
position (PositionState)      — for key binding attributes
key_asset (signer, mut)     — new MPL-Core asset
key_state (init PDA)        — rate limit state
config (ProtocolConfig)     — collection address + CPI signer
collection (mut)            — MPL-Core collection
mpl_core_program
system_program
```

Handler:
1. Check `promo.active == true`
2. Check `promo.claims_count < promo.max_claims` (or 0 = unlimited)
3. `claim_receipt` init handles one-per-wallet (PDA collision = error)
4. Mint key NFT to claimer via CreateV2CpiBuilder (uses `promo.image_uri` if non-empty, otherwise default key image)
5. Init KeyState with rate limits from promo config
6. Increment `promo.claims_count`

#### Deposit via Embedded Buy CPI

The claim instruction includes a Mayflower buy CPI directly. The `amount`
and `min_out` parameters control the deposit and slippage protection:

    claim_promo_key(amount: u64, min_out: u64)

When `amount > 0`, the handler performs a Mayflower buy CPI using the
position's program PDA as signer, converting the deposited SOL into navSOL.
The on-chain program enforces `amount >= promo.min_deposit_lamports`.

The client transaction must prepend SOL wrapping instructions before the
claim instruction:

    Transaction {
        ix[0]: createAssociatedTokenAccountIdempotent  (PDA's wSOL ATA)
        ix[1]: SystemProgram.transfer                  (claimer → PDA wSOL ATA)
        ix[2]: syncNative                              (sync wSOL balance)
        ix[3]: claim_promo_key                         (mints key + buys navSOL)
    }

This is important for tax treatment: the SOL goes into a navSOL position as
a deposit, not as a fee payment to a third party.

#### Security: Zero Deposit + Borrow Permission

**Warning:** Creating a promo with `min_deposit_lamports = 0` and
`PERM_LIMITED_BORROW` enabled allows anyone to claim a free key and
immediately borrow SOL from the position without depositing anything.
Multiply by many wallets and the position drains up to the aggregate of
all per-key rate limits.

The on-chain program allows this combination (it's the admin's choice), but
both TUI and web-lite should warn when this configuration is selected. If you
want a free giveaway with borrow access, consider setting a non-zero
`min_deposit_lamports` as a spam filter, or use `PERM_BUY` only (no borrow).

#### What This Enables

- Admin configures promo once, goes offline
- Users visit landing page, connect wallet, click "Claim Key"
- Frontend generates key_asset keypair, bundles claim + deposit
- User signs one transaction, gets key NFT + deposits in one click
- Admin can pause via update_promo or revoke keys via existing revoke_key
- No backend server or hot wallet required
- Claimed keys work identically to admin-issued keys (same validate_key path)

### 4. Notification System

Simple off-chain script that monitors borrow capacity and posts to
Telegram/Discord when capacity is available. Users subscribe to get
"time to claim" alerts.

No program changes needed — just reads on-chain state.

## Revocation

Admin retains the ability to revoke any key via `revoke_key` (burns the NFT
on-chain using PermanentBurnDelegate). This can be used to:
- Remove inactive users to free conceptual slots
- Shut down the campaign if needed
- Run in waves: issue batch, let it drain, revoke, re-issue

## Future Features

### Collection-Gated Claims

Gate promo claims behind ownership of an NFT from a specific collection. Adds
`gate_collection: Option<Pubkey>` to PromoConfig. When set, `claim_promo_key`
requires the claimer to pass a `gate_asset` account — handler verifies the
claimer owns it and it belongs to the required collection.

MPL-Core collections are straightforward (collection key is in the asset data).
Legacy Token Metadata NFTs need more accounts (mint, token account, metadata
PDA) and a check on `collection.key` + `collection.verified`. Start with
MPL-Core only, add legacy support if needed.

### Batch Key Issuance (CLI)

CLI command for admin-curated key distribution without self-service:

    hardig-tui batch-authorize \
      --permissions LimitedBorrow \
      --capacity 0.02 \
      --refill-days 90 \
      --wallets wallets.csv

Reads wallet addresses from CSV, issues keys in batch transactions.

### Notification System

Off-chain script monitoring borrow capacity, posting "time to claim" alerts
to Telegram/Discord. No program changes needed — just reads on-chain state.

## Open Questions

- Exact deposit amount and borrow parameters (needs real yield data)
- Whether to require repayment or treat all borrows as permanent promo spend
- Landing page hosting and domain
- Communication channel for claim notifications (Telegram, Discord, X)
- Whether self-service claim instruction is worth the program complexity
