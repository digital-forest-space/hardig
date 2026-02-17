use std::io;
use std::time::Instant;

use anchor_lang::AccountDeserialize;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::prelude::*;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::RpcFilterType;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

pub use hardig::state::KeyRole;
use hardig::state::{KeyAuthorization, PositionNFT, ProtocolConfig};

// Mayflower constants and helpers
use hardig::mayflower::{
    calculate_borrow_capacity, derive_log_account, derive_personal_position,
    derive_personal_position_escrow, read_debt, read_deposited_shares, read_floor_price,
    FEE_VAULT, MARKET_BASE_VAULT, MARKET_GROUP, MARKET_META, MARKET_NAV_VAULT,
    MAYFLOWER_MARKET, MAYFLOWER_PROGRAM_ID, MAYFLOWER_TENANT, NAV_SOL_MINT, WSOL_MINT,
};

use crate::ui;

// Well-known program IDs
const SPL_TOKEN_ID: Pubkey = solana_sdk::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ATA_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const SYSTEM_PROGRAM_ID: Pubkey = solana_sdk::pubkey!("11111111111111111111111111111111");

fn get_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), SPL_TOKEN_ID.as_ref(), mint.as_ref()],
        &ATA_PROGRAM_ID,
    )
    .0
}

/// Anchor instruction discriminator: first 8 bytes of SHA-256("global:<name>")
fn sighash(name: &str) -> Vec<u8> {
    let hash = solana_sdk::hash::hash(format!("global:{}", name).as_bytes());
    hash.to_bytes()[..8].to_vec()
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Form,
    Confirm,
    Result,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    CreatePosition,
    AuthorizeKey,
    RevokeKey,
    Buy,
    Sell,
    Borrow,
    Repay,
}

pub struct KeyEntry {
    pub pda: Pubkey,
    pub mint: Pubkey,
    pub role: KeyRole,
    pub held_by_signer: bool,
}

pub struct PendingAction {
    pub description: Vec<String>,
    pub instructions: Vec<Instruction>,
    pub extra_signers: Vec<Keypair>,
}

#[derive(Clone)]
pub struct PositionSnapshot {
    pub deposited_nav: u64,
    pub user_debt: u64,
    pub protocol_debt: u64,
    pub borrow_capacity: u64,
    pub wsol_balance: u64,
    pub nav_sol_balance: u64,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    pub rpc: RpcClient,
    pub keypair: Keypair,
    pub should_quit: bool,
    pub screen: Screen,
    pub message_log: Vec<String>,
    pub verbose: bool,

    // Protocol state
    pub protocol_exists: bool,

    // Position state (single position mode)
    pub position_pda: Option<Pubkey>,
    pub position: Option<PositionNFT>,
    pub my_role: Option<KeyRole>,
    pub my_key_auth_pda: Option<Pubkey>,
    pub my_nft_mint: Option<Pubkey>,
    pub keyring: Vec<KeyEntry>,

    // Mayflower state
    pub program_pda: Pubkey,
    pub pp_pda: Pubkey,
    pub escrow_pda: Pubkey,
    pub log_pda: Pubkey,
    pub wsol_ata: Pubkey,
    pub nav_sol_ata: Pubkey,
    pub mayflower_initialized: bool,
    pub wsol_balance: u64,
    pub nav_sol_balance: u64,
    pub atas_exist: bool,
    // Real Mayflower values (from on-chain PersonalPosition + Market)
    pub mf_deposited_shares: u64,
    pub mf_debt: u64,
    pub mf_floor_price: u64,
    pub mf_borrow_capacity: u64,

    // Refresh tracking
    pub last_refresh: Option<Instant>,

    // Form state
    pub form_kind: Option<FormKind>,
    pub form_fields: Vec<(String, String)>,
    pub input_field: usize,
    pub input_buf: String,

    // Key cursor for keyring navigation
    pub key_cursor: usize,

    // Confirm state
    pub pending_action: Option<PendingAction>,

    // Result screen state
    pub pre_tx_snapshot: Option<PositionSnapshot>,
    pub last_tx_signature: Option<String>,
}

impl App {
    pub fn new(rpc_url: &str, keypair: Keypair, verbose: bool) -> Self {
        let rpc =
            RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

        // Compute Mayflower derived addresses (constant per program)
        let (program_pda, _) = Pubkey::find_program_address(&[b"authority"], &hardig::ID);
        let (pp_pda, _) = derive_personal_position(&program_pda);
        let (escrow_pda, _) = derive_personal_position_escrow(&pp_pda);
        let (log_pda, _) = derive_log_account();
        let wsol_ata = get_ata(&program_pda, &WSOL_MINT);
        let nav_sol_ata = get_ata(&program_pda, &NAV_SOL_MINT);

        let mut app = Self {
            rpc,
            keypair,
            should_quit: false,
            screen: Screen::Dashboard,
            message_log: Vec::new(),
            verbose,
            protocol_exists: false,
            position_pda: None,
            position: None,
            my_role: None,
            my_key_auth_pda: None,
            my_nft_mint: None,
            keyring: Vec::new(),
            program_pda,
            pp_pda,
            escrow_pda,
            log_pda,
            wsol_ata,
            nav_sol_ata,
            mayflower_initialized: false,
            wsol_balance: 0,
            nav_sol_balance: 0,
            atas_exist: false,
            mf_deposited_shares: 0,
            mf_debt: 0,
            mf_floor_price: 0,
            mf_borrow_capacity: 0,
            last_refresh: None,
            form_kind: None,
            form_fields: Vec::new(),
            input_field: 0,
            input_buf: String::new(),
            key_cursor: 0,
            pending_action: None,
            pre_tx_snapshot: None,
            last_tx_signature: None,
        };
        app.push_log("Welcome to Härdig TUI");
        app.push_log(format!("Wallet: {}", app.keypair.pubkey()));
        app.push_log(format!("Program PDA: {}", short_pubkey(&program_pda)));
        app.refresh();
        app
    }

    pub fn push_log(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if self.verbose {
            eprintln!("[INFO] {}", msg);
        }
        self.message_log.push(msg);
        if self.message_log.len() > 100 {
            self.message_log.remove(0);
        }
    }

    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| ui::draw(frame, self))?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c')
                {
                    self.should_quit = true;
                    continue;
                }
                match self.screen {
                    Screen::Dashboard => self.handle_dashboard(key.code),
                    Screen::Form => self.handle_form(key.code),
                    Screen::Confirm => self.handle_confirm(key.code),
                    Screen::Result => self.handle_result(key.code),
                }
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Dashboard handler
    // -----------------------------------------------------------------------

    fn handle_dashboard(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('r') => self.refresh(),

            KeyCode::Char('I') if !self.protocol_exists => self.build_init_protocol(),

            KeyCode::Char('n') if self.position_pda.is_none() && self.protocol_exists => {
                self.enter_create_position()
            }

            // One-time Mayflower setup (admin only)
            KeyCode::Char('S')
                if self.my_role == Some(KeyRole::Admin) && !self.cpi_ready() =>
            {
                self.build_setup()
            }
            // Admin actions
            KeyCode::Char('a') if self.my_role == Some(KeyRole::Admin) => {
                self.enter_authorize_key()
            }
            KeyCode::Char('x') if self.my_role == Some(KeyRole::Admin) => self.enter_revoke_key(),
            KeyCode::Char('s') if self.can_sell() => self.enter_sell(),
            KeyCode::Char('d') if self.can_borrow() => self.enter_borrow(),

            // Role-gated actions
            KeyCode::Char('b') if self.can_buy() => self.enter_buy(),
            KeyCode::Char('p') if self.can_repay() => self.enter_repay(),
            KeyCode::Char('i') if self.can_reinvest() => self.build_reinvest(),

            // Navigate keyring
            KeyCode::Up | KeyCode::Char('k') => {
                if self.key_cursor > 0 {
                    self.key_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.keyring.is_empty() && self.key_cursor < self.keyring.len() - 1 {
                    self.key_cursor += 1;
                }
            }
            _ => {}
        }
    }

    pub fn cpi_ready(&self) -> bool {
        self.mayflower_initialized && self.atas_exist
    }
    pub fn can_buy(&self) -> bool {
        self.cpi_ready()
            && matches!(
                self.my_role,
                Some(KeyRole::Admin) | Some(KeyRole::Operator) | Some(KeyRole::Depositor)
            )
    }
    pub fn can_sell(&self) -> bool {
        self.cpi_ready() && self.my_role == Some(KeyRole::Admin)
    }
    pub fn can_borrow(&self) -> bool {
        self.cpi_ready() && self.my_role == Some(KeyRole::Admin)
    }
    pub fn can_repay(&self) -> bool {
        self.cpi_ready()
            && self.position.as_ref().map(|p| p.user_debt > 0).unwrap_or(false)
            && matches!(
                self.my_role,
                Some(KeyRole::Admin) | Some(KeyRole::Operator) | Some(KeyRole::Depositor)
            )
    }
    pub fn can_reinvest(&self) -> bool {
        self.cpi_ready()
            && matches!(
                self.my_role,
                Some(KeyRole::Admin) | Some(KeyRole::Operator) | Some(KeyRole::Keeper)
            )
    }

    // -----------------------------------------------------------------------
    // Form handler
    // -----------------------------------------------------------------------

    fn handle_form(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                self.screen = Screen::Dashboard;
                self.form_fields.clear();
            }
            KeyCode::Tab => {
                if !self.form_fields.is_empty() {
                    self.form_fields[self.input_field].1 = self.input_buf.clone();
                    self.input_field = (self.input_field + 1) % self.form_fields.len();
                    self.input_buf = self.form_fields[self.input_field].1.clone();
                }
            }
            KeyCode::Enter => {
                if !self.form_fields.is_empty() {
                    self.form_fields[self.input_field].1 = self.input_buf.clone();
                }
                self.submit_form();
            }
            KeyCode::Backspace => {
                self.input_buf.pop();
            }
            KeyCode::Char(c) => {
                self.input_buf.push(c);
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Confirm handler
    // -----------------------------------------------------------------------

    fn handle_confirm(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(action) = self.pending_action.take() {
                    self.pre_tx_snapshot = self.take_snapshot();
                    match self.send_action_result(action) {
                        Ok(sig) => {
                            self.push_log(format!("TX confirmed: {}", sig));
                            self.last_tx_signature = Some(sig);
                            self.refresh();
                            self.screen = Screen::Result;
                        }
                        Err(e) => {
                            self.push_log(e);
                            self.pre_tx_snapshot = None;
                            self.screen = Screen::Dashboard;
                        }
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.pending_action = None;
                self.screen = Screen::Dashboard;
                self.push_log("Action cancelled.");
            }
            _ => {}
        }
    }

    fn handle_result(&mut self, _key: KeyCode) {
        self.pre_tx_snapshot = None;
        self.last_tx_signature = None;
        self.screen = Screen::Dashboard;
    }

    // -----------------------------------------------------------------------
    // Form entry points
    // -----------------------------------------------------------------------

    fn enter_create_position(&mut self) {
        self.screen = Screen::Form;
        self.form_kind = Some(FormKind::CreatePosition);
        self.form_fields = vec![("Max Reinvest Spread (bps)".into(), "500".into())];
        self.input_field = 0;
        self.input_buf = self.form_fields[0].1.clone();
    }

    fn enter_authorize_key(&mut self) {
        self.screen = Screen::Form;
        self.form_kind = Some(FormKind::AuthorizeKey);
        self.form_fields = vec![
            ("Target Wallet (pubkey)".into(), String::new()),
            (
                "Role (1=Operator, 2=Depositor, 3=Keeper)".into(),
                "1".into(),
            ),
        ];
        self.input_field = 0;
        self.input_buf = self.form_fields[0].1.clone();
    }

    fn enter_revoke_key(&mut self) {
        let revocable: Vec<(usize, &KeyEntry)> = self
            .keyring
            .iter()
            .enumerate()
            .filter(|(_, k)| k.role != KeyRole::Admin)
            .collect();
        if revocable.is_empty() {
            self.push_log("No non-admin keys to revoke.");
            return;
        }
        self.screen = Screen::Form;
        self.form_kind = Some(FormKind::RevokeKey);
        let mut desc = String::new();
        for (idx, (_, k)) in revocable.iter().enumerate() {
            desc.push_str(&format!(
                "{}: {} ({})\n",
                idx,
                short_pubkey(&k.mint),
                role_name(k.role)
            ));
        }
        self.form_fields = vec![
            ("Available keys".into(), desc),
            ("Key index to revoke".into(), "0".into()),
        ];
        self.input_field = 1;
        self.input_buf = self.form_fields[1].1.clone();
    }

    fn enter_buy(&mut self) {
        self.screen = Screen::Form;
        self.form_kind = Some(FormKind::Buy);
        self.form_fields = vec![("Amount (SOL)".into(), "1".into())];
        self.input_field = 0;
        self.input_buf = self.form_fields[0].1.clone();
    }

    fn enter_sell(&mut self) {
        let max = self.position.as_ref().map(|p| p.deposited_nav).unwrap_or(0);
        self.screen = Screen::Form;
        self.form_kind = Some(FormKind::Sell);
        self.form_fields = vec![("Amount (SOL)".into(), lamports_to_sol(max))];
        self.input_field = 0;
        self.input_buf = self.form_fields[0].1.clone();
    }

    fn enter_borrow(&mut self) {
        self.screen = Screen::Form;
        self.form_kind = Some(FormKind::Borrow);
        self.form_fields = vec![("Amount (SOL)".into(), String::new())];
        self.input_field = 0;
        self.input_buf.clear();
    }

    fn enter_repay(&mut self) {
        let max = self.position.as_ref().map(|p| p.user_debt).unwrap_or(0);
        self.screen = Screen::Form;
        self.form_kind = Some(FormKind::Repay);
        self.form_fields = vec![("Amount (SOL)".into(), lamports_to_sol(max))];
        self.input_field = 0;
        self.input_buf = self.form_fields[0].1.clone();
    }

    // -----------------------------------------------------------------------
    // Form submission
    // -----------------------------------------------------------------------

    fn submit_form(&mut self) {
        match self.form_kind {
            Some(FormKind::CreatePosition) => self.build_create_position(),
            Some(FormKind::AuthorizeKey) => self.build_authorize_key(),
            Some(FormKind::RevokeKey) => self.build_revoke_key(),
            Some(FormKind::Buy) => self.build_buy(),
            Some(FormKind::Sell) => self.build_sell(),
            Some(FormKind::Borrow) => self.build_borrow(),
            Some(FormKind::Repay) => self.build_repay(),
            None => {}
        }
    }

    fn goto_confirm(&mut self, action: PendingAction) {
        self.pending_action = Some(action);
        self.screen = Screen::Confirm;
    }

    // -----------------------------------------------------------------------
    // Mayflower remaining_accounts builders
    // -----------------------------------------------------------------------

    /// Build the remaining_accounts for a buy instruction (17 entries).
    fn buy_remaining_accounts(&self) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(self.program_pda, false),            // [0]
            AccountMeta::new(self.pp_pda, false),                 // [1]
            AccountMeta::new(self.escrow_pda, false),             // [2]
            AccountMeta::new(self.nav_sol_ata, false),            // [3]
            AccountMeta::new(self.wsol_ata, false),               // [4]
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),   // [5]
            AccountMeta::new_readonly(MARKET_GROUP, false),       // [6]
            AccountMeta::new_readonly(MARKET_META, false),        // [7]
            AccountMeta::new(MAYFLOWER_MARKET, false),            // [8]
            AccountMeta::new(NAV_SOL_MINT, false),                // [9]
            AccountMeta::new(MARKET_BASE_VAULT, false),           // [10]
            AccountMeta::new(MARKET_NAV_VAULT, false),            // [11]
            AccountMeta::new(FEE_VAULT, false),                   // [12]
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // [13]
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),       // [14]
            AccountMeta::new(self.log_pda, false),                // [15]
            AccountMeta::new_readonly(WSOL_MINT, false),          // [16] needed by CPI
        ]
    }

    /// Build the remaining_accounts for a borrow instruction (14 entries).
    fn borrow_remaining_accounts(&self) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(self.program_pda, false),            // [0]
            AccountMeta::new(self.pp_pda, false),                 // [1]
            AccountMeta::new(self.wsol_ata, false),               // [2]
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),   // [3]
            AccountMeta::new_readonly(MARKET_GROUP, false),       // [4]
            AccountMeta::new_readonly(MARKET_META, false),        // [5]
            AccountMeta::new(MARKET_BASE_VAULT, false),           // [6]
            AccountMeta::new(MARKET_NAV_VAULT, false),            // [7]
            AccountMeta::new(FEE_VAULT, false),                   // [8]
            AccountMeta::new(MAYFLOWER_MARKET, false),            // [9]
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // [10]
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),       // [11]
            AccountMeta::new(self.log_pda, false),                // [12]
            AccountMeta::new_readonly(WSOL_MINT, false),          // [13] needed by CPI
        ]
    }

    /// Build the remaining_accounts for a repay instruction (13 entries).
    /// Same layout as borrow (repay is the reverse operation on the same accounts).
    fn repay_remaining_accounts(&self) -> Vec<AccountMeta> {
        self.borrow_remaining_accounts()
    }

    /// Build the remaining_accounts for a withdraw/sell instruction (16 entries).
    /// Same layout as buy (sell is the reverse of buy).
    fn sell_remaining_accounts(&self) -> Vec<AccountMeta> {
        self.buy_remaining_accounts()
    }

    /// Build the remaining_accounts for reinvest (18 entries).
    fn reinvest_remaining_accounts(&self) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(self.program_pda, false),            // [0]
            AccountMeta::new(MAYFLOWER_MARKET, false),            // [1] for floor price read
            AccountMeta::new(self.pp_pda, false),                 // [2]
            AccountMeta::new(self.escrow_pda, false),             // [3]
            AccountMeta::new(self.nav_sol_ata, false),            // [4]
            AccountMeta::new(self.wsol_ata, false),               // [5]
            AccountMeta::new(self.wsol_ata, false),               // [6] same as [5] for borrow
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),   // [7]
            AccountMeta::new_readonly(MARKET_GROUP, false),       // [8]
            AccountMeta::new_readonly(MARKET_META, false),        // [9]
            AccountMeta::new(MARKET_BASE_VAULT, false),           // [10]
            AccountMeta::new(MARKET_NAV_VAULT, false),            // [11]
            AccountMeta::new(FEE_VAULT, false),                   // [12]
            AccountMeta::new(NAV_SOL_MINT, false),                // [13]
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // [14]
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),       // [15]
            AccountMeta::new(self.log_pda, false),                // [16]
            AccountMeta::new_readonly(WSOL_MINT, false),          // [17] needed by CPI
        ]
    }

    // -----------------------------------------------------------------------
    // Instruction builders
    // -----------------------------------------------------------------------

    pub fn build_init_protocol(&mut self) {
        let (config_pda, _) =
            Pubkey::find_program_address(&[ProtocolConfig::SEED], &hardig::ID);

        let data = sighash("initialize_protocol");
        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new(config_pda, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        self.goto_confirm(PendingAction {
            description: vec![
                "Initialize Protocol".into(),
                format!("Config PDA: {}", config_pda),
                format!("Admin: {}", self.keypair.pubkey()),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    pub fn build_create_position(&mut self) {
        let spread: u16 = match self.form_fields[0].1.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                self.push_log("Invalid spread value");
                return;
            }
        };

        let mint_kp = Keypair::new();
        let mint = mint_kp.pubkey();
        let admin = self.keypair.pubkey();
        let admin_ata = get_ata(&admin, &mint);
        let (position_pda, _) =
            Pubkey::find_program_address(&[PositionNFT::SEED, mint.as_ref()], &hardig::ID);
        let (key_auth_pda, _) = Pubkey::find_program_address(
            &[KeyAuthorization::SEED, position_pda.as_ref(), mint.as_ref()],
            &hardig::ID,
        );

        let mut data = sighash("create_position");
        data.extend_from_slice(&spread.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(admin, true),
            AccountMeta::new(mint, true),
            AccountMeta::new(admin_ata, false),
            AccountMeta::new(position_pda, false),
            AccountMeta::new(key_auth_pda, false),
            AccountMeta::new_readonly(self.program_pda, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        self.goto_confirm(PendingAction {
            description: vec![
                "Create Position".into(),
                format!("Admin NFT Mint: {}", mint),
                format!("Position PDA: {}", position_pda),
                format!("Max Reinvest Spread: {} bps", spread),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![mint_kp],
        });
    }

    pub fn build_authorize_key(&mut self) {
        let target_wallet: Pubkey = match self.form_fields[0].1.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                self.push_log("Invalid pubkey");
                return;
            }
        };
        let role_u8: u8 = match self.form_fields[1].1.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                self.push_log("Invalid role");
                return;
            }
        };
        if role_u8 == 0 {
            self.push_log("Cannot create a second admin key");
            return;
        }

        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };
        let admin_nft_mint = self.my_nft_mint.unwrap();
        let admin_nft_ata = get_ata(&self.keypair.pubkey(), &admin_nft_mint);
        let admin_key_auth = self.my_key_auth_pda.unwrap();

        let mint_kp = Keypair::new();
        let new_mint = mint_kp.pubkey();
        let target_ata = get_ata(&target_wallet, &new_mint);
        let (new_key_auth, _) = Pubkey::find_program_address(
            &[
                KeyAuthorization::SEED,
                position_pda.as_ref(),
                new_mint.as_ref(),
            ],
            &hardig::ID,
        );

        let mut data = sighash("authorize_key");
        data.push(role_u8);

        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new_readonly(admin_nft_ata, false),
            AccountMeta::new_readonly(admin_key_auth, false),
            AccountMeta::new_readonly(position_pda, false),
            AccountMeta::new(new_mint, true),
            AccountMeta::new(target_ata, false),
            AccountMeta::new_readonly(target_wallet, false),
            AccountMeta::new(new_key_auth, false),
            AccountMeta::new_readonly(self.program_pda, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        let rn = match role_u8 {
            1 => "Operator",
            2 => "Depositor",
            3 => "Keeper",
            _ => "Unknown",
        };

        self.goto_confirm(PendingAction {
            description: vec![
                "Authorize Key".into(),
                format!("Target: {}", target_wallet),
                format!("Role: {} ({})", rn, role_u8),
                format!("Key NFT Mint: {}", new_mint),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![mint_kp],
        });
    }

    pub fn build_revoke_key(&mut self) {
        let revocable: Vec<&KeyEntry> = self
            .keyring
            .iter()
            .filter(|k| k.role != KeyRole::Admin)
            .collect();
        let idx: usize = match self.form_fields[1].1.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                self.push_log("Invalid index");
                return;
            }
        };
        if idx >= revocable.len() {
            self.push_log("Index out of range");
            return;
        }

        let target = &revocable[idx];
        let position_pda = self.position_pda.unwrap();
        let admin_nft_mint = self.my_nft_mint.unwrap();
        let admin_nft_ata = get_ata(&self.keypair.pubkey(), &admin_nft_mint);
        let admin_key_auth = self.my_key_auth_pda.unwrap();

        let data = sighash("revoke_key");
        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new_readonly(admin_nft_ata, false),
            AccountMeta::new_readonly(admin_key_auth, false),
            AccountMeta::new_readonly(position_pda, false),
            AccountMeta::new(target.pda, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        self.goto_confirm(PendingAction {
            description: vec![
                "Revoke Key".into(),
                format!("Key Mint: {}", target.mint),
                format!("Role: {}", role_name(target.role)),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    /// Combined one-time setup: init Mayflower position + create ATAs.
    /// Only includes instructions for steps not yet completed.
    pub fn build_setup(&mut self) {
        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };

        let mut instructions = Vec::new();
        let mut description = vec!["Setup Mayflower Accounts".into()];

        // Step 1: Init Mayflower position if needed
        if !self.mayflower_initialized {
            let admin_nft_mint = self.my_nft_mint.unwrap();
            let admin_nft_ata = get_ata(&self.keypair.pubkey(), &admin_nft_mint);
            let admin_key_auth = self.my_key_auth_pda.unwrap();

            let data = sighash("init_mayflower_position");
            let accounts = vec![
                AccountMeta::new(self.keypair.pubkey(), true),
                AccountMeta::new_readonly(admin_nft_ata, false),
                AccountMeta::new_readonly(admin_key_auth, false),
                AccountMeta::new(position_pda, false),
                AccountMeta::new_readonly(self.program_pda, false),
                AccountMeta::new(self.pp_pda, false),
                AccountMeta::new(self.escrow_pda, false),
                AccountMeta::new_readonly(MARKET_META, false),
                AccountMeta::new_readonly(NAV_SOL_MINT, false),
                AccountMeta::new(self.log_pda, false),
                AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),
                AccountMeta::new_readonly(SPL_TOKEN_ID, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            ];
            instructions.push(Instruction::new_with_bytes(hardig::ID, &data, accounts));
            description.push(format!(
                "Init PersonalPosition: {}",
                short_pubkey(&self.pp_pda)
            ));
        }

        // Step 2: Create ATAs if needed
        if !self.atas_exist {
            let payer = self.keypair.pubkey();
            instructions.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &payer,
                    &self.program_pda,
                    &WSOL_MINT,
                    &SPL_TOKEN_ID,
                ),
            );
            instructions.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &payer,
                    &self.program_pda,
                    &NAV_SOL_MINT,
                    &SPL_TOKEN_ID,
                ),
            );
            description.push(format!("Create wSOL ATA: {}", short_pubkey(&self.wsol_ata)));
            description.push(format!(
                "Create navSOL ATA: {}",
                short_pubkey(&self.nav_sol_ata)
            ));
        }

        if instructions.is_empty() {
            self.push_log("Setup already complete");
            return;
        }

        self.goto_confirm(PendingAction {
            description,
            instructions,
            extra_signers: vec![],
        });
    }

    // -----------------------------------------------------------------------
    // CPI-aware role-gated instruction builders
    // -----------------------------------------------------------------------

    /// Build base accounts for a role-gated instruction.
    fn role_gated_base_accounts(&self) -> Vec<AccountMeta> {
        let nft_mint = self.my_nft_mint.unwrap();
        let nft_ata = get_ata(&self.keypair.pubkey(), &nft_mint);
        let key_auth = self.my_key_auth_pda.unwrap();
        let position_pda = self.position_pda.unwrap();

        vec![
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new_readonly(nft_ata, false),
            AccountMeta::new_readonly(key_auth, false),
            AccountMeta::new(position_pda, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ]
    }

    pub fn build_buy(&mut self) {
        let amount = match parse_sol_to_lamports(&self.form_fields[0].1) {
            Some(v) => v,
            None => {
                self.push_log("Invalid SOL amount");
                return;
            }
        };
        if self.position_pda.is_none() {
            self.push_log("No position loaded");
            return;
        }

        let mut data = sighash("buy");
        data.extend_from_slice(&amount.to_le_bytes());

        let mut accounts = self.role_gated_base_accounts();
        accounts.extend(self.buy_remaining_accounts());

        // Prepend wrap: transfer SOL → wSOL ATA, then sync_native
        let transfer_ix = solana_sdk::system_instruction::transfer(
            &self.keypair.pubkey(),
            &self.wsol_ata,
            amount,
        );
        let sync_ix =
            spl_token::instruction::sync_native(&SPL_TOKEN_ID, &self.wsol_ata).unwrap();

        let buy_ix = Instruction::new_with_bytes(hardig::ID, &data, accounts);

        self.goto_confirm(PendingAction {
            description: vec![
                "Buy navSOL".into(),
                format!("Amount: {} SOL", lamports_to_sol(amount)),
                format!("Position: {}", short_pubkey(&self.position_pda.unwrap())),
                format!(
                    "Role: {}",
                    role_name(self.my_role.unwrap_or(KeyRole::Keeper))
                ),
            ],
            instructions: vec![transfer_ix, sync_ix, buy_ix],
            extra_signers: vec![],
        });
    }

    pub fn build_sell(&mut self) {
        let amount = match parse_sol_to_lamports(&self.form_fields[0].1) {
            Some(v) => v,
            None => {
                self.push_log("Invalid SOL amount");
                return;
            }
        };
        if self.position_pda.is_none() {
            self.push_log("No position loaded");
            return;
        }

        let mut data = sighash("withdraw");
        data.extend_from_slice(&amount.to_le_bytes());

        let mut accounts = self.role_gated_base_accounts();
        accounts.extend(self.sell_remaining_accounts());

        self.goto_confirm(PendingAction {
            description: vec![
                "Sell navSOL (IX_SELL TODO)".into(),
                format!("Amount: {} SOL", lamports_to_sol(amount)),
                format!("Position: {}", short_pubkey(&self.position_pda.unwrap())),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    pub fn build_borrow(&mut self) {
        let amount = match parse_sol_to_lamports(&self.form_fields[0].1) {
            Some(v) => v,
            None => {
                self.push_log("Invalid SOL amount");
                return;
            }
        };
        if self.position_pda.is_none() {
            self.push_log("No position loaded");
            return;
        }

        let mut data = sighash("borrow");
        data.extend_from_slice(&amount.to_le_bytes());

        let mut accounts = self.role_gated_base_accounts();
        accounts.extend(self.borrow_remaining_accounts());

        self.goto_confirm(PendingAction {
            description: vec![
                "Borrow".into(),
                format!("Amount: {} SOL", lamports_to_sol(amount)),
                format!("Position: {}", short_pubkey(&self.position_pda.unwrap())),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    pub fn build_repay(&mut self) {
        let amount = match parse_sol_to_lamports(&self.form_fields[0].1) {
            Some(v) => v,
            None => {
                self.push_log("Invalid SOL amount");
                return;
            }
        };
        if self.position_pda.is_none() {
            self.push_log("No position loaded");
            return;
        }

        let mut data = sighash("repay");
        data.extend_from_slice(&amount.to_le_bytes());

        let mut accounts = self.role_gated_base_accounts();
        accounts.extend(self.repay_remaining_accounts());

        self.goto_confirm(PendingAction {
            description: vec![
                "Repay".into(),
                format!("Amount: {} SOL", lamports_to_sol(amount)),
                format!("Position: {}", short_pubkey(&self.position_pda.unwrap())),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    pub fn build_reinvest(&mut self) {
        if self.position_pda.is_none() {
            self.push_log("No position loaded");
            return;
        }

        let data = sighash("reinvest");

        let mut accounts = self.role_gated_base_accounts();
        accounts.extend(self.reinvest_remaining_accounts());

        // Reinvest does borrow + buy CPIs in one tx — needs extra compute
        let compute_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(400_000);

        self.goto_confirm(PendingAction {
            description: vec![
                "Reinvest (CPI)".into(),
                format!("Position: {}", short_pubkey(&self.position_pda.unwrap())),
                format!(
                    "Role: {}",
                    role_name(self.my_role.unwrap_or(KeyRole::Keeper))
                ),
                "Borrows available capacity and buys more navSOL".into(),
            ],
            instructions: vec![compute_ix, Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    // -----------------------------------------------------------------------
    // Send transaction
    // -----------------------------------------------------------------------

    /// Send a transaction and return the signature or error message.
    pub fn send_action_result(&mut self, action: PendingAction) -> Result<String, String> {
        let blockhash = self
            .rpc
            .get_latest_blockhash()
            .map_err(|e| format!("RPC error: {}", e))?;

        let mut signers: Vec<&dyn Signer> = vec![&self.keypair];
        for ks in &action.extra_signers {
            signers.push(ks);
        }

        let tx = Transaction::new_signed_with_payer(
            &action.instructions,
            Some(&self.keypair.pubkey()),
            &signers,
            blockhash,
        );

        self.rpc
            .send_and_confirm_transaction(&tx)
            .map(|sig| sig.to_string())
            .map_err(|e| format!("TX failed: {}", e))
    }

    // -----------------------------------------------------------------------
    // RPC: Refresh state
    // -----------------------------------------------------------------------

    pub fn refresh(&mut self) {
        self.push_log("Refreshing...");
        self.check_protocol();
        self.discover_position();
        self.refresh_mayflower_state();
        self.last_refresh = Some(Instant::now());
        self.push_log("Refresh complete.");
    }

    fn check_protocol(&mut self) {
        let (config_pda, _) =
            Pubkey::find_program_address(&[ProtocolConfig::SEED], &hardig::ID);
        self.protocol_exists = self.rpc.get_account(&config_pda).is_ok();
    }

    fn discover_position(&mut self) {
        self.position_pda = None;
        self.position = None;
        self.my_role = None;
        self.my_key_auth_pda = None;
        self.my_nft_mint = None;
        self.keyring.clear();

        // Get all KeyAuthorization accounts from the program
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![RpcFilterType::DataSize(
                KeyAuthorization::SIZE as u64,
            )]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                commitment: Some(CommitmentConfig::confirmed()),
                ..Default::default()
            },
            ..Default::default()
        };

        let accounts = match self.rpc.get_program_accounts_with_config(&hardig::ID, config) {
            Ok(a) => a,
            Err(e) => {
                self.push_log(format!("Scan failed: {}", e));
                return;
            }
        };

        // Find KeyAuthorizations where the signer holds the NFT
        let mut found_position: Option<Pubkey> = None;
        let mut best: Option<(KeyRole, Pubkey, Pubkey)> = None;

        for (pubkey, account) in &accounts {
            let ka = match KeyAuthorization::try_deserialize(&mut account.data.as_slice()) {
                Ok(k) => k,
                Err(_) => continue,
            };

            if self.check_holds_nft(&ka.key_nft_mint) {
                let is_better = match &best {
                    None => true,
                    Some((r, _, _)) => (ka.role as u8) < (*r as u8),
                };
                if is_better {
                    found_position = Some(ka.position);
                    best = Some((ka.role, *pubkey, ka.key_nft_mint));
                }
            }
        }

        if let (Some(pos_pda), Some((role, auth_pda, mint))) = (found_position, best) {
            self.position_pda = Some(pos_pda);
            self.my_role = Some(role);
            self.my_key_auth_pda = Some(auth_pda);
            self.my_nft_mint = Some(mint);

            if let Ok(acc) = self.rpc.get_account(&pos_pda) {
                if let Ok(pos) = PositionNFT::try_deserialize(&mut acc.data.as_slice()) {
                    self.mayflower_initialized = pos.position_pda != Pubkey::default();
                    self.position = Some(pos);
                }
            }

            // Load all keys for this position
            for (pubkey, account) in &accounts {
                if let Ok(ka) = KeyAuthorization::try_deserialize(&mut account.data.as_slice()) {
                    if ka.position == pos_pda {
                        self.keyring.push(KeyEntry {
                            pda: *pubkey,
                            mint: ka.key_nft_mint,
                            role: ka.role,
                            held_by_signer: self.check_holds_nft(&ka.key_nft_mint),
                        });
                    }
                }
            }

            self.push_log(format!(
                "Found position {} (role: {}{})",
                short_pubkey(&pos_pda),
                role_name(role),
                if self.mayflower_initialized {
                    ", Mayflower OK"
                } else {
                    ""
                },
            ));
        } else {
            self.push_log("No position found for this keypair.");
        }
    }

    fn refresh_mayflower_state(&mut self) {
        self.wsol_balance = 0;
        self.nav_sol_balance = 0;
        self.atas_exist = false;
        self.mf_deposited_shares = 0;
        self.mf_debt = 0;
        self.mf_floor_price = 0;
        self.mf_borrow_capacity = 0;

        if !self.mayflower_initialized {
            return;
        }

        // Check wSOL ATA
        let wsol_exists = self.read_token_balance(&self.wsol_ata);
        let nav_exists = self.read_token_balance(&self.nav_sol_ata);

        match (wsol_exists, nav_exists) {
            (Some(wsol), Some(nav)) => {
                self.wsol_balance = wsol;
                self.nav_sol_balance = nav;
                self.atas_exist = true;
            }
            _ => {
                self.atas_exist = false;
            }
        }

        // Read real borrow capacity from Mayflower accounts
        if let Ok(pp_acc) = self.rpc.get_account(&self.pp_pda) {
            if let (Ok(shares), Ok(debt)) = (
                read_deposited_shares(&pp_acc.data),
                read_debt(&pp_acc.data),
            ) {
                self.mf_deposited_shares = shares;
                self.mf_debt = debt;
            }
        }
        if let Ok(market_acc) = self.rpc.get_account(&MAYFLOWER_MARKET) {
            if let Ok(floor) = read_floor_price(&market_acc.data) {
                self.mf_floor_price = floor;
            }
        }
        if let Ok(cap) = calculate_borrow_capacity(
            self.mf_deposited_shares,
            self.mf_floor_price,
            self.mf_debt,
        ) {
            self.mf_borrow_capacity = cap;
        }
    }

    /// Read the token balance from an ATA. Returns None if the account doesn't exist.
    fn read_token_balance(&self, ata: &Pubkey) -> Option<u64> {
        let account = self.rpc.get_account(ata).ok()?;
        if account.data.len() >= 72 {
            let bytes: [u8; 8] = account.data[64..72].try_into().ok()?;
            Some(u64::from_le_bytes(bytes))
        } else {
            None
        }
    }

    fn check_holds_nft(&self, mint: &Pubkey) -> bool {
        let ata = get_ata(&self.keypair.pubkey(), mint);
        self.read_token_balance(&ata) == Some(1)
    }

    pub fn take_snapshot(&self) -> Option<PositionSnapshot> {
        let pos = self.position.as_ref()?;
        Some(PositionSnapshot {
            deposited_nav: pos.deposited_nav,
            user_debt: pos.user_debt,
            protocol_debt: pos.protocol_debt,
            borrow_capacity: self.mf_borrow_capacity,
            wsol_balance: self.wsol_balance,
            nav_sol_balance: self.nav_sol_balance,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn role_name(role: KeyRole) -> &'static str {
    match role {
        KeyRole::Admin => "Admin",
        KeyRole::Operator => "Operator",
        KeyRole::Depositor => "Depositor",
        KeyRole::Keeper => "Keeper",
    }
}

pub fn short_pubkey(pubkey: &Pubkey) -> String {
    let s = pubkey.to_string();
    if s.len() > 12 {
        format!("{}..{}", &s[..4], &s[s.len() - 4..])
    } else {
        s
    }
}

/// Parse a SOL amount string (e.g. "0.01") into lamports.
pub fn parse_sol_to_lamports(s: &str) -> Option<u64> {
    let sol: f64 = s.trim().parse().ok()?;
    if sol < 0.0 {
        return None;
    }
    let lamports = (sol * 1_000_000_000.0).round() as u64;
    if lamports == 0 && sol > 0.0 {
        return None; // too small to represent
    }
    Some(lamports)
}

pub fn lamports_to_sol(lamports: u64) -> String {
    let sol = lamports as f64 / 1_000_000_000.0;
    if sol == 0.0 {
        "0".to_string()
    } else if sol < 0.001 {
        format!("{:.9}", sol)
    } else {
        format!("{:.4}", sol)
    }
}

pub fn format_delta(before: u64, after: u64) -> String {
    if after > before {
        format!("+{}", lamports_to_sol(after - before))
    } else if before > after {
        format!("-{}", lamports_to_sol(before - after))
    } else {
        "0".to_string()
    }
}
