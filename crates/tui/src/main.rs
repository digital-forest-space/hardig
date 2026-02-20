mod app;
mod ui;

use std::io::{self, stdout};

use clap::{Parser, Subcommand};
use crossterm::{

    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use serde::Serialize;
use solana_sdk::signature::{read_keypair_file, Signer};

use app::{lamports_to_sol, permissions_name};

#[derive(Parser)]
#[command(name = "hardig-tui")]
#[command(about = "Interactive TUI for the Härdig program")]
struct Cli {
    /// Path to the keypair JSON file
    keypair: String,

    /// Solana cluster (localnet, devnet, mainnet-beta, or a custom RPC URL)
    #[arg(long, default_value = "localnet")]
    cluster: String,

    /// Print progress/debug info to stderr
    #[arg(long, global = true)]
    verbose: bool,

    /// Target a specific position by admin asset pubkey (for multi-position wallets)
    #[arg(long, global = true)]
    position: Option<String>,

    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Read-only state dump
    Status,
    /// Initialize the protocol config
    InitProtocol,
    /// Migrate ProtocolConfig to add collection field (protocol admin only, one-time)
    MigrateConfig,
    /// Create the MPL-Core collection for key NFTs (protocol admin only, one-time)
    CreateCollection {
        /// Metadata URI (upload collection-metadata.json to Irys/Arweave first)
        #[arg(long)]
        uri: String,
    },
    /// Create a new position NFT (also initializes Mayflower PersonalPosition)
    CreatePosition {
        /// Nav token mint to use (defaults to navSOL).
        /// When specified, the MarketConfig for this mint must already exist on-chain.
        #[arg(long)]
        nav_mint: Option<String>,
        /// Optional name for the position NFT (max 32 characters)
        #[arg(long)]
        name: Option<String>,
    },
    /// Buy navSOL with SOL
    Buy {
        /// Amount in SOL
        #[arg(long)]
        amount: f64,
    },
    /// Sell navSOL back to SOL
    Sell {
        /// Amount in SOL
        #[arg(long)]
        amount: f64,
    },
    /// Borrow against deposited navSOL
    Borrow {
        /// Amount in SOL
        #[arg(long)]
        amount: f64,
    },
    /// Repay outstanding debt
    Repay {
        /// Amount in SOL
        #[arg(long)]
        amount: f64,
    },
    /// Reinvest (borrow + buy) to compound position
    Reinvest,
    /// Authorize a new key for the position
    AuthorizeKey {
        /// Target wallet public key
        #[arg(long)]
        wallet: String,
        /// Permissions bitmask: 25=Operator, 9=Depositor, 16=Keeper, or custom
        #[arg(long)]
        permissions: u8,
        /// Sell rate-limit capacity in SOL (for PERM_LIMITED_SELL)
        #[arg(long, default_value = "0")]
        sell_capacity: f64,
        /// Sell rate-limit refill period in slots
        #[arg(long, default_value = "0")]
        sell_refill_slots: u64,
        /// Borrow rate-limit capacity in SOL (for PERM_LIMITED_BORROW)
        #[arg(long, default_value = "0")]
        borrow_capacity: f64,
        /// Borrow rate-limit refill period in slots
        #[arg(long, default_value = "0")]
        borrow_refill_slots: u64,
        /// Optional name for the key NFT (max 32 characters)
        #[arg(long)]
        name: Option<String>,
    },
    /// Revoke a non-admin key by index
    RevokeKey {
        /// Index of the non-admin key to revoke
        #[arg(long)]
        index: usize,
    },
    /// Send heartbeat to prove admin liveness (resets recovery timer)
    Heartbeat,
    /// Configure a recovery key for the position (admin only)
    ConfigureRecovery {
        /// Target wallet to receive the recovery key NFT
        #[arg(long)]
        target_wallet: String,
        /// Grace period days
        #[arg(long, default_value = "0")]
        days: u64,
        /// Grace period hours
        #[arg(long, default_value = "0")]
        hours: u64,
        /// Grace period minutes
        #[arg(long, default_value = "0")]
        minutes: u64,
        /// Lock the recovery config so it can't be changed later
        #[arg(long, default_value = "false")]
        lock_config: bool,
        /// Optional label for the recovery key NFT
        #[arg(long)]
        name: Option<String>,
    },
    /// Execute recovery after lockout has expired (recovery key holder only)
    ExecuteRecovery,
    /// Show compact position balances
    Balances,
    /// Transfer protocol admin to a new pubkey (current admin only)
    TransferAdmin {
        /// New admin public key
        #[arg(long)]
        new_admin: String,
    },
    /// Create a MarketConfig PDA (protocol admin only)
    CreateMarketConfig {
        /// Market name shorthand (e.g. navSOL) — fetches addresses from API
        #[arg(long, requires = "markets_url", conflicts_with_all = ["nav_mint", "base_mint", "market_group", "market_meta", "mayflower_market", "market_base_vault", "market_nav_vault", "fee_vault"])]
        market: Option<String>,
        /// URL of the markets API (required with --market)
        #[arg(long, requires = "market")]
        markets_url: Option<String>,
        /// Nav token mint (e.g. navSOL)
        #[arg(long, required_unless_present = "market")]
        nav_mint: Option<String>,
        /// Base token mint (e.g. wSOL)
        #[arg(long, required_unless_present = "market")]
        base_mint: Option<String>,
        /// Mayflower market group
        #[arg(long, required_unless_present = "market")]
        market_group: Option<String>,
        /// Mayflower market meta
        #[arg(long, required_unless_present = "market")]
        market_meta: Option<String>,
        /// Mayflower market
        #[arg(long, required_unless_present = "market")]
        mayflower_market: Option<String>,
        /// Market base vault
        #[arg(long, required_unless_present = "market")]
        market_base_vault: Option<String>,
        /// Market nav vault
        #[arg(long, required_unless_present = "market")]
        market_nav_vault: Option<String>,
        /// Fee vault
        #[arg(long, required_unless_present = "market")]
        fee_vault: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// JSON output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(tag = "type")]
enum CliOutput {
    #[serde(rename = "success")]
    Success {
        action: String,
        signature: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },
    #[serde(rename = "noop")]
    Noop { action: String, message: String },
    #[serde(rename = "error")]
    Error { action: String, error: String },
    #[serde(rename = "status")]
    Status(PositionStatus),
    #[serde(rename = "balances")]
    Balances(BalancesCompact),
}

#[derive(Serialize)]
struct BalancesCompact {
    deposited: String,
    debt: String,
    borrow_capacity: String,
    wsol: String,
    nav_sol: String,
}

#[derive(Serialize)]
struct DiscoveredPositionInfo {
    admin_asset: String,
    role: String,
    deposited_nav: String,
    debt: String,
}

#[derive(Serialize)]
struct PositionStatus {
    wallet: String,
    protocol_exists: bool,
    positions_found: usize,
    all_positions: Vec<DiscoveredPositionInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<PositionInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mayflower: Option<MayflowerInfo>,
    keyring: Vec<KeyInfo>,
}

#[derive(Serialize)]
struct PositionInfo {
    pda: String,
    admin_asset: String,
    role: String,
    deposited_nav: String,
    debt: String,
    borrow_capacity: String,
    last_admin_activity: i64,
    last_admin_activity_ago: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery: Option<RecoveryInfo>,
}

#[derive(Serialize)]
struct RecoveryInfo {
    recovery_asset: String,
    grace_period_secs: i64,
    grace_period: String,
    config_locked: bool,
    secs_since_activity: i64,
    recoverable: bool,
    /// "3d 5h" until recoverable, or "0s" if already recoverable
    recoverable_in: String,
}

#[derive(Serialize)]
struct MayflowerInfo {
    initialized: bool,
    atas_exist: bool,
    wsol_balance: String,
    nav_sol_balance: String,
}

#[derive(Serialize)]
struct KeyInfo {
    pda: String,
    asset: String,
    role: String,
    held_by_signer: bool,
}

/// Fetch Mayflower market configs from the API and resolve a market by name.
/// Returns the 8 pubkeys needed for create_market_config.
fn resolve_market(
    url: &str,
    name: &str,
) -> Result<
    (
        solana_sdk::pubkey::Pubkey,
        solana_sdk::pubkey::Pubkey,
        solana_sdk::pubkey::Pubkey,
        solana_sdk::pubkey::Pubkey,
        solana_sdk::pubkey::Pubkey,
        solana_sdk::pubkey::Pubkey,
        solana_sdk::pubkey::Pubkey,
        solana_sdk::pubkey::Pubkey,
    ),
    String,
> {
    use std::str::FromStr;

    let body: String = ureq::get(url)
        .call()
        .map_err(|e| format!("Failed to fetch markets: {e}"))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse JSON: {e}"))?;

    let markets = json["markets"]
        .as_array()
        .ok_or("Unexpected API response: missing markets array")?;

    let needle = name.to_lowercase();
    let market = markets
        .iter()
        .find(|m| {
            m["name"]
                .as_str()
                .map(|n| n.to_lowercase() == needle)
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            let available: Vec<&str> = markets
                .iter()
                .filter_map(|m| m["name"].as_str())
                .collect();
            format!(
                "Market \"{}\" not found. Available markets: {}",
                name,
                available.join(", ")
            )
        })?;

    let pk = |field: &str| -> Result<solana_sdk::pubkey::Pubkey, String> {
        let s = market[field]
            .as_str()
            .ok_or_else(|| format!("Missing field '{}' in market data", field))?;
        solana_sdk::pubkey::Pubkey::from_str(s)
            .map_err(|_| format!("Invalid pubkey for '{}': {}", field, s))
    };

    Ok((
        pk("navMint")?,
        pk("baseMint")?,
        pk("marketGroup")?,
        pk("marketMetadata")?,
        pk("mayflowerMarket")?,
        pk("marketSolVault")?,
        pk("marketNavVault")?,
        pk("feeVault")?,
    ))
}

fn cluster_to_url(cluster: &str) -> &str {
    match cluster {
        "localnet" | "localhost" => "http://127.0.0.1:8899",
        "devnet" => "https://api.devnet.solana.com",
        "mainnet-beta" | "mainnet" => "https://api.mainnet-beta.solana.com",
        url => url,
    }
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let rpc_url = cluster_to_url(&cli.cluster);

    let keypair = read_keypair_file(&cli.keypair).unwrap_or_else(|e| {
        eprintln!("Failed to read keypair from {}: {}", cli.keypair, e);
        std::process::exit(1);
    });

    match cli.action {
        Some(action) => {
            run_oneshot(rpc_url, keypair, cli.verbose, cli.position, action);
            Ok(())
        }
        None => run_interactive(rpc_url, keypair),
    }
}

fn run_interactive(rpc_url: &str, keypair: solana_sdk::signature::Keypair) -> io::Result<()> {
    // Panic hook: always restore terminal.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic);
    }));

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = app::App::new(rpc_url, keypair, false);
    let result = app.run(&mut terminal);

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    result
}

fn run_oneshot(
    rpc_url: &str,
    keypair: solana_sdk::signature::Keypair,
    verbose: bool,
    position_filter: Option<String>,
    action: Action,
) {
    let mut app = app::App::new(rpc_url, keypair, verbose);

    // If --position specified, select that specific position
    if let Some(ref pos_str) = position_filter {
        use std::str::FromStr;
        match solana_sdk::pubkey::Pubkey::from_str(pos_str) {
            Ok(target_asset) => {
                if let Some(idx) = app.discovered_positions.iter().position(|dp| dp.admin_asset == target_asset) {
                    app.reselect_position(idx);
                    app.refresh_mayflower_state();
                } else {
                    let output = CliOutput::Error {
                        action: action_to_name(&action),
                        error: format!("Position with admin asset {} not found", pos_str),
                    };
                    println!("{}", serde_json::to_string(&output).unwrap());
                    std::process::exit(1);
                }
            }
            Err(_) => {
                let output = CliOutput::Error {
                    action: action_to_name(&action),
                    error: format!("Invalid --position pubkey: {}", pos_str),
                };
                println!("{}", serde_json::to_string(&output).unwrap());
                std::process::exit(1);
            }
        }
    }

    let action_name = action_to_name(&action);

    // Handle read-only commands (no transaction)
    if matches!(action, Action::Status) {
        let output = build_status_output(&app);
        println!("{}", serde_json::to_string(&output).unwrap());
        return;
    }
    if matches!(action, Action::Balances) {
        let output = build_balances_output(&app);
        println!("{}", serde_json::to_string(&output).unwrap());
        return;
    }

    // Pre-fill form_fields and call the appropriate build_* method
    if let Some(noop) = populate_and_build(&mut app, &action) {
        println!("{}", serde_json::to_string(&noop).unwrap());
        return;
    }

    // Extract pending_action or check for error in message_log
    match execute_pending(&mut app, &action_name) {
        Ok(output) => {
            println!("{}", serde_json::to_string(&output).unwrap());
        }
        Err(output) => {
            println!("{}", serde_json::to_string(&output).unwrap());
            std::process::exit(1);
        }
    }
}

fn action_to_name(action: &Action) -> String {
    match action {
        Action::Status => "status".into(),
        Action::InitProtocol => "init-protocol".into(),
        Action::MigrateConfig => "migrate-config".into(),
        Action::CreateCollection { .. } => "create-collection".into(),
        Action::CreatePosition { .. } => "create-position".into(),
        Action::Buy { .. } => "buy".into(),
        Action::Sell { .. } => "sell".into(),
        Action::Borrow { .. } => "borrow".into(),
        Action::Repay { .. } => "repay".into(),
        Action::Reinvest => "reinvest".into(),
        Action::AuthorizeKey { .. } => "authorize-key".into(),
        Action::RevokeKey { .. } => "revoke-key".into(),
        Action::Heartbeat => "heartbeat".into(),
        Action::ConfigureRecovery { .. } => "configure-recovery".into(),
        Action::ExecuteRecovery => "execute-recovery".into(),
        Action::Balances => "balances".into(),
        Action::TransferAdmin { .. } => "transfer-admin".into(),
        Action::CreateMarketConfig { .. } => "create-market-config".into(),
    }
}

fn sol_amount_to_field(amount: f64) -> String {
    // Preserve the user's input precision
    format!("{}", amount)
}

fn populate_and_build(app: &mut app::App, action: &Action) -> Option<CliOutput> {
    match action {
        Action::Status | Action::Balances => unreachable!(),
        Action::InitProtocol => {
            if app.protocol_exists {
                return Some(CliOutput::Noop {
                    action: "init-protocol".into(),
                    message: "Protocol already initialized".into(),
                });
            }
            app.build_init_protocol();
        }
        Action::MigrateConfig => {
            app.build_migrate_config();
        }
        Action::CreateCollection { ref uri } => {
            if app.collection.is_some() {
                return Some(CliOutput::Noop {
                    action: "create-collection".into(),
                    message: "Collection already exists".into(),
                });
            }
            app.build_create_collection(uri.clone());
        }
        Action::CreatePosition { ref nav_mint, ref name } => {
            let parsed_mint = match nav_mint {
                Some(s) => {
                    use std::str::FromStr;
                    match solana_sdk::pubkey::Pubkey::from_str(s) {
                        Ok(pk) => Some(pk),
                        Err(_) => {
                            return Some(CliOutput::Error {
                                action: "create-position".into(),
                                error: format!("Invalid --nav-mint pubkey: {}", s),
                            });
                        }
                    }
                }
                None => None,
            };
            app.form_fields = vec![
                ("Label (optional)".into(), name.clone().unwrap_or_default()),
            ];
            app.build_create_position(parsed_mint);
        }
        Action::Buy { amount } => {
            app.form_fields = vec![("Amount (SOL)".into(), sol_amount_to_field(*amount))];
            app.build_buy();
        }
        Action::Sell { amount } => {
            app.form_fields = vec![("Amount (SOL)".into(), sol_amount_to_field(*amount))];
            app.build_sell();
        }
        Action::Borrow { amount } => {
            app.form_fields = vec![("Amount (SOL)".into(), sol_amount_to_field(*amount))];
            app.build_borrow();
        }
        Action::Repay { amount } => {
            app.form_fields = vec![("Amount (SOL)".into(), sol_amount_to_field(*amount))];
            app.build_repay();
        }
        Action::Reinvest => {
            app.build_reinvest();
        }
        Action::Heartbeat => {
            app.build_heartbeat();
        }
        Action::ConfigureRecovery { target_wallet, days, hours, minutes, lock_config, name } => {
            app.form_fields = vec![
                ("Target Wallet (pubkey)".into(), target_wallet.clone()),
                ("Grace Period Days".into(), days.to_string()),
                ("Grace Period Hours".into(), hours.to_string()),
                ("Grace Period Minutes".into(), minutes.to_string()),
                ("Lock config? (true/false)".into(), lock_config.to_string()),
                ("Label (optional)".into(), name.clone().unwrap_or_default()),
            ];
            app.build_configure_recovery();
        }
        Action::ExecuteRecovery => {
            app.build_execute_recovery();
        }
        Action::AuthorizeKey { wallet, permissions, sell_capacity, sell_refill_slots, borrow_capacity, borrow_refill_slots, name } => {
            app.form_fields = vec![
                ("Target Wallet (pubkey)".into(), wallet.clone()),
                ("Permissions".into(), permissions.to_string()),
                ("Sell Capacity (SOL, 0=none)".into(), sol_amount_to_field(*sell_capacity)),
                ("Sell Refill Period (slots)".into(), sell_refill_slots.to_string()),
                ("Borrow Capacity (SOL, 0=none)".into(), sol_amount_to_field(*borrow_capacity)),
                ("Borrow Refill Period (slots)".into(), borrow_refill_slots.to_string()),
                ("Label (optional)".into(), name.clone().unwrap_or_default()),
            ];
            app.build_authorize_key();
        }
        Action::RevokeKey { index } => {
            // build_revoke_key reads from form_fields[1].1 for the index
            app.form_fields = vec![
                ("Available keys".into(), String::new()),
                ("Key index to revoke".into(), index.to_string()),
            ];
            app.build_revoke_key();
        }
        Action::TransferAdmin { new_admin } => {
            use std::str::FromStr;
            match solana_sdk::pubkey::Pubkey::from_str(new_admin) {
                Ok(pubkey) => {
                    app.build_transfer_admin(pubkey);
                }
                Err(_) => {
                    return Some(CliOutput::Error {
                        action: "transfer-admin".into(),
                        error: format!("Invalid pubkey: {}", new_admin),
                    });
                }
            }
        }
        Action::CreateMarketConfig {
            market,
            markets_url,
            nav_mint,
            base_mint,
            market_group,
            market_meta,
            mayflower_market,
            market_base_vault,
            market_nav_vault,
            fee_vault,
        } => {
            let result = if let Some(name) = market {
                let url = markets_url.as_deref().unwrap();
                resolve_market(url, name)
            } else {
                use std::str::FromStr;
                let parse = |s: &Option<String>| {
                    let s = s.as_ref().unwrap();
                    solana_sdk::pubkey::Pubkey::from_str(s)
                        .map_err(|_| format!("Invalid pubkey: {}", s))
                };
                (|| -> Result<_, String> {
                    Ok((
                        parse(nav_mint)?,
                        parse(base_mint)?,
                        parse(market_group)?,
                        parse(market_meta)?,
                        parse(mayflower_market)?,
                        parse(market_base_vault)?,
                        parse(market_nav_vault)?,
                        parse(fee_vault)?,
                    ))
                })()
            };
            match result {
                Ok((nm, bm, mg, mm, mfm, mbv, mnv, fv)) => {
                    app.build_create_market_config(nm, bm, mg, mm, mfm, mbv, mnv, fv);
                }
                Err(e) => {
                    return Some(CliOutput::Error {
                        action: "create-market-config".into(),
                        error: e,
                    });
                }
            }
        }
    }
    None
}

fn execute_pending(app: &mut app::App, action_name: &str) -> Result<CliOutput, CliOutput> {
    let pending = match app.pending_action.take() {
        Some(p) => p,
        None => {
            // The build_* method logged an error instead of setting pending_action
            let error = app
                .message_log
                .iter()
                .rev()
                .find(|m| {
                    !m.starts_with("Welcome")
                        && !m.starts_with("Wallet:")
                        && !m.starts_with("Program PDA:")
                        && !m.starts_with("Refresh")
                        && !m.starts_with("Found position")
                        && !m.starts_with("No position found")
                        && !m.starts_with("Scan failed")
                })
                .cloned()
                .unwrap_or_else(|| "Unknown error".into());
            return Err(CliOutput::Error {
                action: action_name.to_string(),
                error,
            });
        }
    };

    let before = app.take_snapshot();

    match app.send_action_result(pending) {
        Ok(sig) => {
            app.refresh();
            if app.verbose {
                if let Some(ref snap) = before {
                    print_state_diff(snap, app);
                }
            }
            Ok(CliOutput::Success {
                action: action_name.to_string(),
                signature: sig,
                details: None,
            })
        }
        Err(e) => Err(CliOutput::Error {
            action: action_name.to_string(),
            error: e,
        }),
    }
}

fn print_state_diff(before: &app::PositionSnapshot, app: &app::App) {
    eprintln!("[RESULT] State changes:");
    let nav_name = app.market_config.as_ref()
        .map(|mc| app::nav_token_name(&mc.nav_mint))
        .unwrap_or("shares");
    let rows: Vec<(&str, u64, u64, &str)> = if let Some(pos) = &app.position {
        vec![
            ("Deposited", before.deposited_nav, pos.deposited_nav, nav_name),
            ("Debt", before.user_debt, pos.user_debt, "SOL"),
            ("Borrow Cap", before.borrow_capacity, app.mf_borrow_capacity, "SOL"),
        ]
    } else {
        return;
    };
    eprintln!(
        "  {:<14} {:>14} {:>14} {:>14}",
        "", "Before", "After", "Delta"
    );
    for (label, bv, av, unit) in &rows {
        eprintln!(
            "  {:<14} {:>14} {:>14} {:>14}",
            label,
            format!("{} {}", lamports_to_sol(*bv), unit),
            format!("{} {}", lamports_to_sol(*av), unit),
            app::format_delta(*bv, *av),
        );
    }
}

fn format_duration(secs: i64) -> String {
    if secs <= 0 { return "0s".into(); }
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        if hours > 0 { format!("{}d {}h", days, hours) } else { format!("{}d", days) }
    } else if hours > 0 {
        if mins > 0 { format!("{}h {}m", hours, mins) } else { format!("{}h", hours) }
    } else if mins > 0 {
        format!("{}m", mins)
    } else {
        format!("{}s", secs)
    }
}

fn build_balances_output(app: &app::App) -> CliOutput {
    match &app.position {
        Some(pos) => {
            CliOutput::Balances(BalancesCompact {
                deposited: lamports_to_sol(pos.deposited_nav),
                debt: lamports_to_sol(pos.user_debt),
                borrow_capacity: lamports_to_sol(app.mf_borrow_capacity),
                wsol: lamports_to_sol(app.wsol_balance),
                nav_sol: lamports_to_sol(app.nav_sol_balance),
            })
        }
        None => CliOutput::Error {
            action: "balances".into(),
            error: "No position found for this keypair".into(),
        },
    }
}

fn build_status_output(app: &app::App) -> CliOutput {
    let all_positions: Vec<DiscoveredPositionInfo> = app
        .discovered_positions
        .iter()
        .map(|dp| DiscoveredPositionInfo {
            admin_asset: dp.admin_asset.to_string(),
            role: if dp.is_admin { "Admin".into() } else { permissions_name(dp.permissions) },
            deposited_nav: lamports_to_sol(dp.deposited_nav),
            debt: lamports_to_sol(dp.user_debt),
        })
        .collect();

    let position = app.position.as_ref().map(|pos| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let secs_since = now - pos.last_admin_activity;
        let has_recovery = pos.recovery_asset != solana_sdk::pubkey::Pubkey::default();
        let recovery = if has_recovery {
            Some(RecoveryInfo {
                recovery_asset: pos.recovery_asset.to_string(),
                grace_period_secs: pos.recovery_lockout_secs,
                grace_period: format_duration(pos.recovery_lockout_secs),
                config_locked: pos.recovery_config_locked,
                secs_since_activity: secs_since,
                recoverable: secs_since >= pos.recovery_lockout_secs,
                recoverable_in: format_duration((pos.recovery_lockout_secs - secs_since).max(0)),
            })
        } else {
            None
        };
        PositionInfo {
            pda: app
                .position_pda
                .map(|p| p.to_string())
                .unwrap_or_default(),
            admin_asset: pos.authority_seed.to_string(),
            role: app.my_permissions.map(permissions_name).unwrap_or_else(|| "None".into()),
            deposited_nav: lamports_to_sol(pos.deposited_nav),
            debt: lamports_to_sol(pos.user_debt),
            borrow_capacity: lamports_to_sol(app.mf_borrow_capacity),
            last_admin_activity: pos.last_admin_activity,
            last_admin_activity_ago: format!("{} ago", format_duration(secs_since)),
            recovery,
        }
    });

    let mayflower = if app.position.is_some() {
        Some(MayflowerInfo {
            initialized: app.mayflower_initialized,
            atas_exist: app.atas_exist,
            wsol_balance: lamports_to_sol(app.wsol_balance),
            nav_sol_balance: lamports_to_sol(app.nav_sol_balance),
        })
    } else {
        None
    };

    let keyring = app
        .keyring
        .iter()
        .map(|k| KeyInfo {
            pda: k.pda.to_string(),
            asset: k.asset.to_string(),
            role: permissions_name(k.permissions),
            held_by_signer: k.held_by_signer,
        })
        .collect();

    CliOutput::Status(PositionStatus {
        wallet: app.keypair.pubkey().to_string(),
        protocol_exists: app.protocol_exists,
        positions_found: app.discovered_positions.len(),
        all_positions,
        position,
        mayflower,
        keyring,
    })
}
