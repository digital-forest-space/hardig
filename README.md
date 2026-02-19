# Härdig

*Swedish — hardy, resilient; able to endure difficult conditions.*

Solana program implementing an NFT keyring model for delegated management of positions on the Nirvana protocol. Supports multiple nav-token markets (navSOL, navJUP, etc.) via on-chain MarketConfig PDAs.

Each position is controlled by a set of NFT keys with different permission levels (Admin, Operator, Depositor, Keeper). Keys are standard SPL tokens held in wallets — transfer the NFT and you transfer the permission. Mint authority is permanently disabled after minting each key NFT, guaranteeing a supply of exactly 1. Freeze authority is held by the program PDA, enabling account freezing for theft recovery.

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

**Per-position fund isolation:** Each position has its own authority PDA (`seeds = [b"authority", admin_nft_mint]`) that owns a separate Mayflower PersonalPosition. Funds cannot be commingled between positions.

**MarketConfig:** On-chain PDA (`seeds = [b"market_config", nav_mint]`) that stores the 8 Mayflower market addresses for a given nav token. Each position is bound to a specific MarketConfig, enabling support for multiple markets (navSOL, navJUP, etc.) without program changes.

**CPI safety:** All token accounts are validated as canonical ATAs derived from the program PDA and the relevant mints. Buy, sell (withdraw), and reinvest instructions enforce slippage protection via `min_out` parameters. Accounting tracks actual Mayflower deltas (before/after CPI reads) rather than input amounts.

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

Mainnet fork tests require a running validator with cloned Mayflower accounts:

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
| `S` | Setup Mayflower |
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

# Create a position (500 bps max reinvest spread)
hardig-tui <KEYPAIR_PATH> --cluster localnet create-position --spread-bps 500

# One-time Mayflower setup (init position + create ATAs)
hardig-tui <KEYPAIR_PATH> --cluster localnet setup

# Setup with a non-default nav token (MarketConfig must exist)
hardig-tui <KEYPAIR_PATH> --cluster localnet setup --nav-mint <NAV_MINT_PUBKEY>

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
  --market-meta <PUBKEY> --mayflower-market <PUBKEY> \
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
- Mayflower state (ATAs, wSOL/navSOL balances)
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
  src/state.rs                # ProtocolConfig, PositionNFT, KeyAuthorization, KeyRole, MarketConfig
  src/instructions/           # One file per instruction + validate_key helper
  src/mayflower/              # Mayflower CPI builders, constants, floor price reader
  tests/integration.rs        # LiteSVM unit tests (44 tests)
  tests/mainnet_fork.rs       # Mainnet fork tests
test-programs/mock-mayflower/ # Mock Mayflower program for LiteSVM tests
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
2. **Create Market Config** — protocol admin registers a Mayflower market (navSOL, navJUP, etc.)
3. **Create Position** — mints an admin key NFT to your wallet, creates the position PDA
4. **Setup Mayflower** — initializes the Mayflower PersonalPosition and creates wSOL/nav-token ATAs (use `--nav-mint` for non-default markets)
5. **Buy** — deposit SOL to buy nav tokens (wraps SOL → wSOL, CPI to Mayflower)
6. **Reinvest** — borrows available capacity and buys more nav tokens in one transaction
7. **Authorize Key** — mint a role key NFT to another wallet (Operator, Depositor, or Keeper)
8. **Borrow / Repay / Sell** — manage debt and withdraw as needed
9. **Revoke Key** — close authorization; burns the NFT if admin holds it
10. **Transfer Admin** — hand off protocol admin rights to a new wallet

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.
