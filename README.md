# Härdig

*Swedish — hardy, resilient; able to endure difficult conditions.* — [hardig.app](https://www.hardig.app/)

Solana program implementing an NFT keyring model for delegated management of positions on the Nirvana protocol. Supports multiple nav-token markets (navSOL, navJUP, etc.) via on-chain MarketConfig PDAs.

Each position is controlled by a set of NFT keys with different permission levels (Admin, Operator, Depositor, Keeper). Keys are standard SPL tokens held in wallets — transfer the NFT and you transfer the permission. Mint authority is permanently disabled after minting each key NFT, guaranteeing a supply of exactly 1. Freeze authority is held by the program PDA, enabling account freezing for theft recovery.

**Website:** [hardig.app](https://hardig.app) | **X:** [@hardig_app](https://x.com/hardig_app)

**Program ID:** `4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p`

## Permission Matrix

| Action          | Admin | Operator | Depositor | Keeper |
|-----------------|:-----:|:--------:|:---------:|:------:|
| Buy             | Y     | Y        | Y         |        |
| Sell            | Y     |          |           |        |
| Borrow          | Y     |          |           |        |
| Repay           | Y     | Y        | Y         |        |
| Reinvest        | Y     | Y        |           | Y      |
| Auth/Revoke     | Y     |          |           |        |
| Transfer Admin  | Protocol admin only                   |||

## Architecture

**Per-position fund isolation:** Each position has its own authority PDA (`seeds = [b"authority", admin_nft_mint]`) that owns a separate Nirvana PersonalPosition. Funds cannot be commingled between positions.

**MarketConfig:** On-chain PDA (`seeds = [b"market_config", nav_mint]`) that stores the 8 Nirvana market addresses for a given nav token. Each position is bound to a specific MarketConfig, enabling support for multiple markets (navSOL, navJUP, etc.) without program changes.

**CPI safety:** All token accounts are validated as canonical ATAs derived from the program PDA and the relevant mints. Buy, sell (withdraw), and reinvest instructions enforce slippage protection via `min_out` parameters. Accounting tracks actual Nirvana deltas (before/after CPI reads) rather than input amounts.

**Revoke with burn:** When revoking a key, if the admin holds the target NFT (e.g. after theft recovery), the NFT is burned and the ATA closed, returning rent. When the admin does not hold it, only the KeyAuthorization PDA is closed.

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) and Cargo
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools) (v1.18+)
- [Anchor CLI](https://www.anchor-lang.com/docs/installation) (v0.32.1)
- Node.js 18+ (for web-lite and Anchor TypeScript tests)

### Build the Program

```sh
anchor build
```

### Run Tests

Unit tests use LiteSVM and need no running validator:

```sh
cargo test -p hardig --test integration
```

Mainnet fork tests require a running validator with cloned Nirvana accounts:

```sh
./scripts/start-mainnet-fork.sh --reset
cargo test -p hardig --test mainnet_fork -- --ignored --nocapture
```

## Clients

There are two ways to interact with the program: a terminal UI/CLI and a browser app.

### TUI (`crates/tui/`)

A ratatui-based terminal interface with both interactive and one-shot CLI modes.

**Build:**

```sh
cargo build -p hardig-tui --release
```

**Interactive mode** (full dashboard with keyring, balances, action keys):

```sh
hardig-tui <KEYPAIR_PATH> --cluster localnet
```

| Key | Action |
|-----|--------|
| `r` | Refresh |
| `I` | Init Protocol |
| `n` | New Position |
| `b` | Buy |
| `s` | Sell |
| `d` | Borrow |
| `p` | Repay |
| `i` | Reinvest |
| `a` | Authorize Key |
| `x` | Revoke Key |
| `q` | Quit |

**One-shot CLI mode** (JSON output, scriptable):

```sh
# Check position status
hardig-tui <KEYPAIR_PATH> --cluster localnet status

# Initialize protocol
hardig-tui <KEYPAIR_PATH> --cluster localnet init-protocol

# Create a position (500 bps max reinvest spread, also initializes Nirvana position)
hardig-tui <KEYPAIR_PATH> --cluster localnet create-position --spread-bps 500

# Create a position with a non-default nav token (MarketConfig must exist)
hardig-tui <KEYPAIR_PATH> --cluster localnet create-position --spread-bps 500 --nav-mint <NAV_MINT_PUBKEY>

# Buy 1 SOL worth of navSOL
hardig-tui <KEYPAIR_PATH> --cluster localnet buy --amount 1.0

# Sell navSOL
hardig-tui <KEYPAIR_PATH> --cluster localnet sell --amount 0.5

# Borrow against position
hardig-tui <KEYPAIR_PATH> --cluster localnet borrow --amount 0.1

# Repay debt
hardig-tui <KEYPAIR_PATH> --cluster localnet repay --amount 0.1

# Reinvest (borrow capacity -> more navSOL)
hardig-tui <KEYPAIR_PATH> --cluster localnet reinvest

# Authorize a new key
hardig-tui <KEYPAIR_PATH> --cluster localnet authorize-key --wallet <PUBKEY> --role 1

# Revoke a key by index
hardig-tui <KEYPAIR_PATH> --cluster localnet revoke-key --index 0

# Compact balances
hardig-tui <KEYPAIR_PATH> --cluster localnet balances

# Transfer protocol admin
hardig-tui <KEYPAIR_PATH> --cluster localnet transfer-admin --new-admin <PUBKEY>

# Create a MarketConfig for a new nav token (protocol admin only)
hardig-tui <KEYPAIR_PATH> --cluster localnet create-market-config \
  --nav-mint <PUBKEY> --base-mint <PUBKEY> --market-group <PUBKEY> \
  --market-meta <PUBKEY> --nirvana-market <PUBKEY> \
  --market-base-vault <PUBKEY> --market-nav-vault <PUBKEY> --fee-vault <PUBKEY>
```

All CLI commands output JSON to stdout. Use `--verbose` for progress info on stderr.

### Web Lite (`web-lite/`)

A lightweight browser app built with Preact + Vite. Connects via Wallet Standard (Phantom, Backpack, etc.) and constructs transactions from the Anchor IDL.

**Install and run:**

```sh
cd web-lite
npm install
npm run dev
```

Opens at `http://localhost:5173`. Connect your wallet, select the cluster (localnet / devnet / mainnet / custom URL), and the dashboard will auto-discover any positions where your wallet holds a key NFT.

**Features:**

- All 12 program instructions with form / confirm / result flow
- Position dashboard with deposited shares, debt, and borrow capacity
- Nirvana state (ATAs, wSOL/navSOL balances)
- Keyring table showing all keys, roles, and held status
- Permission-gated action buttons based on your key's role
- Transaction explorer links per cluster
- Scrollable message log

**Production build:**

```sh
cd web-lite
npm run build     # outputs to web-lite/dist/
npm run preview   # preview the production build
```

## Project Layout

```
programs/hardig/              # On-chain program
  src/lib.rs                  # Program entrypoint
  src/errors.rs               # HardigError enum
  src/state.rs                # ProtocolConfig, PositionState, KeyAuthorization, KeyRole, MarketConfig
  src/instructions/           # One file per instruction + validate_key helper
  src/mayflower/              # Nirvana CPI builders, constants, floor price reader
  tests/integration.rs        # LiteSVM unit tests (60 tests)
  tests/mainnet_fork.rs       # Mainnet fork tests
test-programs/mock-mayflower/ # Mock Nirvana program for LiteSVM tests
crates/tui/                   # Terminal UI + CLI
  src/main.rs                 # clap CLI with subcommands
  src/app.rs                  # App state, instruction builders, RPC refresh
  src/ui.rs                   # ratatui rendering
web-lite/                     # Browser app
  src/                        # Preact components, Anchor IDL, instruction builders
Anchor.toml                   # Anchor configuration
```

## Typical Lifecycle

1. **Init Protocol** — one-time, creates the global ProtocolConfig PDA
2. **Create Market Config** — protocol admin registers a Nirvana market (navSOL, navJUP, etc.)
3. **Create Position** — mints an admin key NFT, creates the position PDA, and initializes the Nirvana PersonalPosition in a single transaction
4. **Buy** — deposit SOL to buy nav tokens (wraps SOL → wSOL, CPI to Nirvana)
5. **Reinvest** — borrows available capacity and buys more nav tokens in one transaction
6. **Authorize Key** — mint a role key NFT to another wallet (Operator, Depositor, or Keeper)
7. **Borrow / Repay / Sell** — manage debt and withdraw as needed
8. **Revoke Key** — close authorization; burns the NFT if admin holds it
9. **Transfer Admin** — hand off protocol admin rights to a new wallet

## Reading Rate-Limited Key Allowances

Keys with `PERM_LIMITED_SELL` (0x40) or `PERM_LIMITED_BORROW` (0x80) are governed by a token-bucket rate limiter stored in the key's `KeyState` PDA. The bucket refills linearly over a configurable number of slots, capping at a maximum capacity.

To compute the currently available spending allowance for a limited key:

1. **Derive the `KeyState` PDA** from the key's MPL-Core asset pubkey:
   - Seeds: `[b"key_state", asset_pubkey]`
   - Program: `4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p`

2. **Fetch and deserialize the account.** The `KeyState` layout is:

   | Offset | Size | Field |
   |--------|------|-------|
   | 0 | 8 | Anchor discriminator |
   | 8 | 32 | `asset` (Pubkey) |
   | 40 | 1 | `bump` (u8) |
   | 41 | 32 | `sell_bucket` (RateBucket) |
   | 73 | 32 | `borrow_bucket` (RateBucket) |

   Each `RateBucket` (32 bytes, all little-endian u64):

   | Offset | Size | Field |
   |--------|------|-------|
   | 0 | 8 | `capacity` — max tokens (lamports for borrow, shares for sell) |
   | 8 | 8 | `refill_period` — slots for a full refill from 0 to capacity |
   | 16 | 8 | `level` — tokens remaining at last update |
   | 24 | 8 | `last_update` — slot of last update |

3. **Get the current slot** via `getSlot` (or `Clock::get()` on-chain).

4. **Apply the refill formula:**

   ```
   elapsed   = current_slot - last_update
   refill    = min(capacity, capacity * elapsed / refill_period)   // use u128/BigInt to avoid overflow
   available = min(capacity, level + refill)
   ```

### Rust

The `RateBucket` struct exposes a read-only helper:

```rust
use hardig::state::{KeyState, RateBucket};

// After deserializing a KeyState account:
let available_sell = key_state.sell_bucket.available_now(current_slot);
let available_borrow = key_state.borrow_bucket.available_now(current_slot);
```

### JavaScript

The `web-lite/src/rateLimits.js` module provides equivalent helpers:

```js
import { getKeyAllowance, bucketAvailableNow, parseKeyState } from './rateLimits.js';

// High-level: fetch + compute in one call
const allowance = await getKeyAllowance(connection, assetPubkey);
console.log(allowance.sellAvailable, allowance.borrowAvailable);

// Low-level: parse raw account data + compute
const ks = parseKeyState(accountData);
const available = bucketAvailableNow(ks.sellBucket, currentSlot);
```

## Disclaimer

This program was built with [Claude Code](https://claude.ai/claude-code) and has **not been audited**. Use at your own risk. Do not deposit funds you cannot afford to lose.

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.
