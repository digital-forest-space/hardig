# Härdig

*Swedish — hardy, resilient; able to endure difficult conditions.*

Solana program implementing an NFT keyring model for managing navSOL positions on the Mayflower protocol.

Each position is controlled by a set of NFT keys with different permission levels (Admin, Operator, Depositor, Keeper). Keys are standard SPL tokens held in wallets — transfer the NFT and you transfer the permission.

**Program ID:** `4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p`

## Permission Matrix

| Action     | Admin | Operator | Depositor | Keeper |
|------------|:-----:|:--------:|:---------:|:------:|
| Buy        | Y     | Y        | Y         |        |
| Sell       | Y     |          |           |        |
| Borrow     | Y     |          |           |        |
| Repay      | Y     | Y        | Y         |        |
| Reinvest   | Y     | Y        |           | Y      |
| Auth/Revoke| Y     |          |           |        |

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

- All 10 program instructions with form / confirm / result flow
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
  src/state.rs                # ProtocolConfig, PositionNFT, KeyAuthorization, KeyRole
  src/instructions/           # One file per instruction + validate_key helper
  src/mayflower/              # Mayflower CPI builders, constants, floor price reader
  tests/integration.rs        # LiteSVM unit tests
  tests/mainnet_fork.rs       # Mainnet fork tests
crates/tui/                   # Terminal UI + CLI
  src/main.rs                 # clap CLI with subcommands
  src/app.rs                  # App state, instruction builders, RPC refresh
  src/ui.rs                   # ratatui rendering
web-lite/                     # Browser app
  src/                        # Preact components, Anchor IDL, instruction builders
Anchor.toml                   # Anchor configuration
```

## Typical Lifecycle

1. **Init Protocol** — one-time, creates the global config PDA
2. **Create Position** — mints an admin key NFT to your wallet, creates the position PDA
3. **Setup Mayflower** — initializes the Mayflower PersonalPosition and creates wSOL/navSOL ATAs
4. **Buy** — deposit SOL to buy navSOL (wraps SOL → wSOL, CPI to Mayflower)
5. **Reinvest** — borrows available capacity and buys more navSOL in one transaction
6. **Authorize Key** — mint a role key NFT to another wallet (Operator, Depositor, or Keeper)
7. **Borrow / Repay / Sell** — manage debt and withdraw as needed
