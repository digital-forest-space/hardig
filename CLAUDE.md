# Härdig

Solana program (Anchor 0.32.1) implementing an NFT keyring model for managing navSOL positions on the Mayflower protocol.

## Project layout

```
programs/hardig/          # On-chain program (crate: hardig)
  src/lib.rs              # Program entrypoint, declare_id!, #[program] mod hardig
  src/errors.rs           # HardigError enum
  src/state.rs            # ProtocolConfig, PositionNFT, KeyAuthorization, KeyRole
  src/instructions/       # One file per instruction + validate_key helper + mod.rs with glob re-exports
  src/mayflower/          # Mayflower CPI builders, constants, floor price reader
  tests/integration.rs    # LiteSVM unit tests (permission matrix, lifecycle, theft recovery)
  tests/mainnet_fork.rs   # Tests against solana-test-validator with cloned Mayflower accounts (#[ignore])
crates/tui/               # TUI + CLI binary (crate: hardig-tui)
  src/main.rs             # clap CLI with subcommands (status, buy, sell, borrow, repay, reinvest, etc.)
  src/app.rs              # App state, instruction builders, RPC refresh
  src/ui.rs               # ratatui rendering
tests/hardig.ts           # Anchor TypeScript test (placeholder)
scripts/start-mainnet-fork.sh  # Launches solana-test-validator with cloned Mayflower accounts
Anchor.toml               # Program ID: 4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p
```

## Key identifiers

- Program ID: `4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p`
- Crate name: `hardig` (not `auth_nft` — the project was renamed)
- TUI binary: `hardig-tui`
- Error enum: `HardigError`
- Display name: **Härdig** (Swedish, meaning hardy/resilient; ASCII fallback: Hardig)

## Architecture

**NFT Keyring**: Each position has one admin key NFT and zero or more role keys (Operator, Depositor, Keeper). Keys are NFTs held in wallets; `KeyAuthorization` PDAs link them to positions with roles.

**Permission matrix**:
| Action     | Admin | Operator | Depositor | Keeper |
|------------|-------|----------|-----------|--------|
| Buy        | Y     | Y        | Y         |        |
| Sell       | Y     |          |           |        |
| Borrow     | Y     |          |           |        |
| Repay      | Y     | Y        | Y         |        |
| Reinvest   | Y     | Y        |           | Y      |
| Auth/Revoke| Y     |          |           |        |

**Mayflower CPI**: Buy, sell, borrow, repay, and reinvest instructions forward to the Mayflower protocol via `invoke_signed` using remaining_accounts. The program PDA (`seeds = [b"authority"]`) owns the Mayflower PersonalPosition.

**Borrow capacity**: Read from on-chain Mayflower `PersonalPosition` (deposited shares, debt) and `Market` (floor price). Not tracked in hardig accounting — Mayflower is source of truth.

## Build commands

```sh
# Check program
cargo check -p hardig

# Check TUI
cargo check -p hardig-tui

# Run unit tests (LiteSVM, no validator needed)
cargo test -p hardig --test integration

# Run mainnet fork tests (requires running validator)
./scripts/start-mainnet-fork.sh --reset
cargo test -p hardig --test mainnet_fork -- --ignored --nocapture

# Anchor build (for .so and IDL)
anchor build
```

## Build gotchas

- `anchor-spl/idl-build` must be in `[features] idl-build = [...]`, NOT in dependency features
- `blake3 = "=1.8.2"` is pinned to avoid edition2024 incompatibility with Solana BPF toolchain
- `#[allow(ambiguous_glob_reexports)]` in `instructions/mod.rs` is required — Anchor's `#[program]` macro needs the glob re-exports for `__client_accounts_*` types
- The `#[program] pub mod hardig` name determines Anchor instruction discriminators — changing it would break any deployed state (there is none currently)

## Conventions

- Filenames and crate names: ASCII `hardig`
- User-facing strings (TUI title bar, CLI about, etc.): `Härdig`
- All instructions validate keys via `validate_key()` in `instructions/validate_key.rs`
- CPI account layouts are documented in comments above each `do_mayflower_*` function
