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

use app::{lamports_to_sol, role_name};

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

    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Read-only state dump
    Status,
    /// Initialize the protocol config
    InitProtocol,
    /// Create a new position NFT
    CreatePosition,
    /// One-time setup: init Mayflower position + create ATAs
    Setup {
        /// Nav token mint to use (defaults to navSOL).
        /// When specified, the MarketConfig for this mint must already exist on-chain.
        #[arg(long)]
        nav_mint: Option<String>,
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
        /// Role: 1=Operator, 2=Depositor, 3=Keeper
        #[arg(long)]
        role: u8,
    },
    /// Revoke a non-admin key by index
    RevokeKey {
        /// Index of the non-admin key to revoke
        #[arg(long)]
        index: usize,
    },
    /// Show compact position balances
    Balances,
    /// Create a MarketConfig PDA (protocol admin only)
    CreateMarketConfig {
        /// Nav token mint (e.g. navSOL)
        #[arg(long)]
        nav_mint: String,
        /// Base token mint (e.g. wSOL)
        #[arg(long)]
        base_mint: String,
        /// Mayflower market group
        #[arg(long)]
        market_group: String,
        /// Mayflower market meta
        #[arg(long)]
        market_meta: String,
        /// Mayflower market
        #[arg(long)]
        mayflower_market: String,
        /// Market base vault
        #[arg(long)]
        market_base_vault: String,
        /// Market nav vault
        #[arg(long)]
        market_nav_vault: String,
        /// Fee vault
        #[arg(long)]
        fee_vault: String,
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
    user_debt: String,
    protocol_debt: String,
    borrow_capacity: String,
    wsol: String,
    nav_sol: String,
}

#[derive(Serialize)]
struct PositionStatus {
    wallet: String,
    protocol_exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<PositionInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mayflower: Option<MayflowerInfo>,
    keyring: Vec<KeyInfo>,
}

#[derive(Serialize)]
struct PositionInfo {
    pda: String,
    admin_mint: String,
    role: String,
    deposited_nav: String,
    user_debt: String,
    protocol_debt: String,
    borrow_capacity: String,
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
    mint: String,
    role: String,
    held_by_signer: bool,
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
            run_oneshot(rpc_url, keypair, cli.verbose, action);
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
    action: Action,
) {
    let mut app = app::App::new(rpc_url, keypair, verbose);

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
        Action::CreatePosition { .. } => "create-position".into(),
        Action::Setup { .. } => "setup".into(),
        Action::Buy { .. } => "buy".into(),
        Action::Sell { .. } => "sell".into(),
        Action::Borrow { .. } => "borrow".into(),
        Action::Repay { .. } => "repay".into(),
        Action::Reinvest => "reinvest".into(),
        Action::AuthorizeKey { .. } => "authorize-key".into(),
        Action::RevokeKey { .. } => "revoke-key".into(),
        Action::Balances => "balances".into(),
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
        Action::CreatePosition => {
            if app.position_pda.is_some() {
                return Some(CliOutput::Noop {
                    action: "create-position".into(),
                    message: "Position already exists for this keypair".into(),
                });
            }
            app.build_create_position();
        }
        Action::Setup { ref nav_mint } => {
            if app.cpi_ready() {
                return Some(CliOutput::Noop {
                    action: "setup".into(),
                    message: "Mayflower position and ATAs already initialized".into(),
                });
            }
            let parsed_mint = match nav_mint {
                Some(s) => {
                    use std::str::FromStr;
                    match solana_sdk::pubkey::Pubkey::from_str(s) {
                        Ok(pk) => Some(pk),
                        Err(_) => {
                            return Some(CliOutput::Error {
                                action: "setup".into(),
                                error: format!("Invalid --nav-mint pubkey: {}", s),
                            });
                        }
                    }
                }
                None => None,
            };
            app.build_setup(parsed_mint);
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
        Action::AuthorizeKey { wallet, role } => {
            app.form_fields = vec![
                ("Target Wallet (pubkey)".into(), wallet.clone()),
                (
                    "Role (1=Operator, 2=Depositor, 3=Keeper)".into(),
                    role.to_string(),
                ),
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
        Action::CreateMarketConfig {
            nav_mint,
            base_mint,
            market_group,
            market_meta,
            mayflower_market,
            market_base_vault,
            market_nav_vault,
            fee_vault,
        } => {
            use std::str::FromStr;
            let parse = |s: &str| {
                solana_sdk::pubkey::Pubkey::from_str(s).map_err(|_| format!("Invalid pubkey: {}", s))
            };
            match (|| -> Result<_, String> {
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
            })() {
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
    let rows: Vec<(&str, u64, u64)> = if let Some(pos) = &app.position {
        vec![
            ("Deposited", before.deposited_nav, pos.deposited_nav),
            ("User Debt", before.user_debt, pos.user_debt),
            ("Protocol Debt", before.protocol_debt, pos.protocol_debt),
            ("Borrow Cap", before.borrow_capacity, app.mf_borrow_capacity),
            ("wSOL", before.wsol_balance, app.wsol_balance),
            ("navSOL", before.nav_sol_balance, app.nav_sol_balance),
        ]
    } else {
        return;
    };
    eprintln!(
        "  {:<14} {:>14} {:>14} {:>14}",
        "", "Before", "After", "Delta"
    );
    for (label, bv, av) in &rows {
        eprintln!(
            "  {:<14} {:>14} {:>14} {:>14}",
            label,
            format!("{} SOL", lamports_to_sol(*bv)),
            format!("{} SOL", lamports_to_sol(*av)),
            app::format_delta(*bv, *av),
        );
    }
}

fn build_balances_output(app: &app::App) -> CliOutput {
    match &app.position {
        Some(pos) => {
            CliOutput::Balances(BalancesCompact {
                deposited: lamports_to_sol(pos.deposited_nav),
                user_debt: lamports_to_sol(pos.user_debt),
                protocol_debt: lamports_to_sol(pos.protocol_debt),
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
    let position = app.position.as_ref().map(|pos| {
        PositionInfo {
            pda: app
                .position_pda
                .map(|p| p.to_string())
                .unwrap_or_default(),
            admin_mint: pos.admin_nft_mint.to_string(),
            role: app.my_role.map(role_name).unwrap_or("None").to_string(),
            deposited_nav: lamports_to_sol(pos.deposited_nav),
            user_debt: lamports_to_sol(pos.user_debt),
            protocol_debt: lamports_to_sol(pos.protocol_debt),
            borrow_capacity: lamports_to_sol(app.mf_borrow_capacity),
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
            mint: k.mint.to_string(),
            role: role_name(k.role).to_string(),
            held_by_signer: k.held_by_signer,
        })
        .collect();

    CliOutput::Status(PositionStatus {
        wallet: app.keypair.pubkey().to_string(),
        protocol_exists: app.protocol_exists,
        position,
        mayflower,
        keyring,
    })
}
