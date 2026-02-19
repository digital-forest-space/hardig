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

use hardig::state::{
    KeyState, MarketConfig, PositionNFT, ProtocolConfig, PERM_BORROW, PERM_BUY,
    PERM_LIMITED_BORROW, PERM_LIMITED_SELL, PERM_MANAGE_KEYS, PERM_REINVEST, PERM_REPAY,
    PERM_SELL, PRESET_ADMIN, PRESET_OPERATOR,
};

// Mayflower constants and helpers
use hardig::mayflower::{
    calculate_borrow_capacity, derive_log_account, derive_personal_position,
    derive_personal_position_escrow, read_debt, read_deposited_shares, read_floor_price,
    DEFAULT_FEE_VAULT, DEFAULT_MARKET_BASE_VAULT, DEFAULT_MARKET_GROUP, DEFAULT_MARKET_META,
    DEFAULT_MARKET_NAV_VAULT, DEFAULT_MAYFLOWER_MARKET, DEFAULT_NAV_SOL_MINT, DEFAULT_WSOL_MINT,
    MAYFLOWER_PROGRAM_ID, MAYFLOWER_TENANT,
};

use crate::ui;

// Well-known program IDs
const SPL_TOKEN_ID: Pubkey = solana_sdk::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ATA_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const SYSTEM_PROGRAM_ID: Pubkey = solana_sdk::pubkey!("11111111111111111111111111111111");
const MPL_CORE_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

fn get_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), SPL_TOKEN_ID.as_ref(), mint.as_ref()],
        &ATA_PROGRAM_ID,
    )
    .0
}

fn create_ata_idempotent_ix(payer: &Pubkey, wallet: &Pubkey, mint: &Pubkey) -> Instruction {
    let ata = get_ata(wallet, mint);
    Instruction::new_with_bytes(
        ATA_PROGRAM_ID,
        &[1], // CreateIdempotent
        vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(*wallet, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
        ],
    )
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
    AuthorizeKey,
    RevokeKey,
    Buy,
    Sell,
    Borrow,
    Repay,
}

pub struct KeyEntry {
    pub pda: Pubkey,
    pub asset: Pubkey,
    pub permissions: u8,
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
    pub my_permissions: Option<u8>,
    pub my_key_state_pda: Option<Pubkey>,
    pub my_asset: Option<Pubkey>,
    pub keyring: Vec<KeyEntry>,

    // Market config (loaded from position's market_config PDA)
    pub market_config_pda: Option<Pubkey>,
    pub market_config: Option<MarketConfig>,

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
    pub form_info: Option<String>,
    pub input_field: usize,
    pub input_buf: String,

    // Permission checkboxes for authorize_key form
    pub perm_bits: u8,
    pub perm_cursor: usize,

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

        let (log_pda, _) = derive_log_account();

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
            my_permissions: None,
            my_key_state_pda: None,
            my_asset: None,
            keyring: Vec::new(),
            market_config_pda: None,
            market_config: None,
            program_pda: Pubkey::default(),
            pp_pda: Pubkey::default(),
            escrow_pda: Pubkey::default(),
            log_pda,
            wsol_ata: Pubkey::default(),
            nav_sol_ata: Pubkey::default(),
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
            form_info: None,
            input_field: 0,
            input_buf: String::new(),
            perm_bits: PRESET_OPERATOR,
            perm_cursor: 0,
            key_cursor: 0,
            pending_action: None,
            pre_tx_snapshot: None,
            last_tx_signature: None,
        };
        app.push_log("Welcome to Härdig TUI");
        app.push_log(format!("Wallet: {}", app.keypair.pubkey()));
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
                self.build_create_position()
            }

            // One-time Mayflower setup (admin only)
            KeyCode::Char('S')
                if self.has_perm(PERM_MANAGE_KEYS) && !self.cpi_ready() =>
            {
                self.build_setup(None)
            }
            // Admin actions
            KeyCode::Char('a') if self.has_perm(PERM_MANAGE_KEYS) => {
                self.enter_authorize_key()
            }
            KeyCode::Char('x') if self.has_perm(PERM_MANAGE_KEYS) => self.enter_revoke_key(),
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

    pub fn has_perm(&self, perm: u8) -> bool {
        self.my_permissions.map_or(false, |p| p & perm != 0)
    }
    pub fn cpi_ready(&self) -> bool {
        // ATAs are created on-demand via create_ata_idempotent in each instruction,
        // so we only need the Mayflower PersonalPosition to be initialized.
        self.mayflower_initialized
    }
    pub fn can_buy(&self) -> bool {
        self.cpi_ready() && self.has_perm(PERM_BUY)
    }
    pub fn can_sell(&self) -> bool {
        self.cpi_ready() && (self.has_perm(PERM_SELL) || self.has_perm(PERM_LIMITED_SELL))
    }
    pub fn can_borrow(&self) -> bool {
        self.cpi_ready() && (self.has_perm(PERM_BORROW) || self.has_perm(PERM_LIMITED_BORROW))
    }
    pub fn can_repay(&self) -> bool {
        self.cpi_ready()
            && self.position.as_ref().map(|p| p.user_debt > 0).unwrap_or(false)
            && self.has_perm(PERM_REPAY)
    }
    pub fn can_reinvest(&self) -> bool {
        self.cpi_ready() && self.has_perm(PERM_REINVEST)
    }

    // -----------------------------------------------------------------------
    // Form handler
    // -----------------------------------------------------------------------

    fn handle_form(&mut self, key: KeyCode) {
        let is_perm_field =
            matches!(self.form_kind, Some(FormKind::AuthorizeKey)) && self.input_field == 1;

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
            KeyCode::Left | KeyCode::Up if is_perm_field => {
                self.perm_cursor = self.perm_cursor.saturating_sub(1);
            }
            KeyCode::Right | KeyCode::Down if is_perm_field => {
                if self.perm_cursor < 6 {
                    self.perm_cursor += 1;
                }
            }
            KeyCode::Backspace if !is_perm_field => {
                self.input_buf.pop();
            }
            KeyCode::Char(c) if is_perm_field => {
                const PERM_ORDER: [u8; 7] =
                    [PERM_BUY, PERM_SELL, PERM_BORROW, PERM_REPAY, PERM_REINVEST, PERM_LIMITED_SELL, PERM_LIMITED_BORROW];
                match c {
                    ' ' => { self.perm_bits ^= PERM_ORDER[self.perm_cursor]; self.sync_perm_field(); }
                    '1' => { self.perm_bits ^= PERM_BUY; self.sync_perm_field(); }
                    '2' => { self.perm_bits ^= PERM_SELL; self.sync_perm_field(); }
                    '3' => { self.perm_bits ^= PERM_BORROW; self.sync_perm_field(); }
                    '4' => { self.perm_bits ^= PERM_REPAY; self.sync_perm_field(); }
                    '5' => { self.perm_bits ^= PERM_REINVEST; self.sync_perm_field(); }
                    '6' => { self.perm_bits ^= PERM_LIMITED_SELL; self.sync_perm_field(); }
                    '7' => { self.perm_bits ^= PERM_LIMITED_BORROW; self.sync_perm_field(); }
                    _ => {}
                }
            }
            KeyCode::Char(c) => {
                self.input_buf.push(c);
            }
            _ => {}
        }
    }

    fn sync_perm_field(&mut self) {
        let val = self.perm_bits.to_string();
        self.form_fields[1].1 = val.clone();
        if self.input_field == 1 {
            self.input_buf = val;
        }

        // Preserve existing rate-limit field values by label prefix
        let sell_cap = self.find_field_value("Sell Capacity");
        let sell_refill = self.find_field_value("Sell Refill");
        let borrow_cap = self.find_field_value("Borrow Capacity");
        let borrow_refill = self.find_field_value("Borrow Refill");

        // Rebuild fields after index 1
        self.form_fields.truncate(2);
        if self.perm_bits & PERM_LIMITED_SELL != 0 {
            self.form_fields.push(("Sell Capacity (SOL)".into(), sell_cap.unwrap_or("0".into())));
            self.form_fields.push(("Sell Refill Period (slots)".into(), sell_refill.unwrap_or("0".into())));
        }
        if self.perm_bits & PERM_LIMITED_BORROW != 0 {
            self.form_fields.push(("Borrow Capacity (SOL)".into(), borrow_cap.unwrap_or("0".into())));
            self.form_fields.push(("Borrow Refill Period (slots)".into(), borrow_refill.unwrap_or("0".into())));
        }

        // Clamp cursor if fields were removed
        if self.input_field >= self.form_fields.len() {
            self.input_field = self.form_fields.len() - 1;
            self.input_buf = self.form_fields[self.input_field].1.clone();
        }
    }

    fn find_field_value(&self, prefix: &str) -> Option<String> {
        self.form_fields.iter()
            .find(|(label, _)| label.starts_with(prefix))
            .map(|(_, val)| val.clone())
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

    fn enter_authorize_key(&mut self) {
        self.perm_bits = PRESET_OPERATOR;
        self.perm_cursor = 0;
        self.screen = Screen::Form;
        self.form_info = None;
        self.form_kind = Some(FormKind::AuthorizeKey);
        let my_wallet = self.keypair.pubkey().to_string();
        self.form_fields = vec![
            ("Target Wallet (pubkey)".into(), my_wallet.clone()),
            ("Permissions".into(), PRESET_OPERATOR.to_string()),
        ];
        self.input_field = 0;
        self.input_buf = my_wallet;
    }

    fn enter_revoke_key(&mut self) {
        self.form_info = None;
        let admin_asset_key = self.position.as_ref().map(|p| p.admin_asset);
        let revocable: Vec<(usize, &KeyEntry)> = self
            .keyring
            .iter()
            .enumerate()
            .filter(|(_, k)| Some(k.asset) != admin_asset_key)
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
                short_pubkey(&k.asset),
                permissions_name(k.permissions)
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
        self.form_info = None;
        self.form_kind = Some(FormKind::Buy);
        self.form_fields = vec![("Amount (SOL)".into(), "1".into())];
        self.input_field = 0;
        self.input_buf = self.form_fields[0].1.clone();
    }

    fn enter_sell(&mut self) {
        let max = self.position.as_ref().map(|p| p.deposited_nav).unwrap_or(0);
        self.screen = Screen::Form;
        self.form_kind = Some(FormKind::Sell);
        self.form_info = Some(format!("Available: {} navSOL", lamports_to_sol(max)));
        self.form_fields = vec![("Amount (navSOL)".into(), lamports_to_sol(max))];
        self.input_field = 0;
        self.input_buf = self.form_fields[0].1.clone();
    }

    fn enter_borrow(&mut self) {
        self.screen = Screen::Form;
        self.form_info = None;
        self.form_kind = Some(FormKind::Borrow);
        self.form_fields = vec![("Amount (SOL)".into(), String::new())];
        self.input_field = 0;
        self.input_buf.clear();
    }

    fn enter_repay(&mut self) {
        let max = self.position.as_ref().map(|p| p.user_debt).unwrap_or(0);
        self.screen = Screen::Form;
        self.form_info = None;
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
        let asset_kp = Keypair::new();
        let asset = asset_kp.pubkey();
        let admin = self.keypair.pubkey();
        let (position_pda, _) =
            Pubkey::find_program_address(&[PositionNFT::SEED, asset.as_ref()], &hardig::ID);
        let (prog_pda, _) =
            Pubkey::find_program_address(&[b"authority", asset.as_ref()], &hardig::ID);

        let mut data = sighash("create_position");
        data.extend_from_slice(&0u16.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(admin, true),
            AccountMeta::new(asset, true),
            AccountMeta::new(position_pda, false),
            AccountMeta::new_readonly(prog_pda, false),
            AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        self.goto_confirm(PendingAction {
            description: vec![
                "Create Position".into(),
                format!("Admin Asset: {}", asset),
                format!("Position PDA: {}", position_pda),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![asset_kp],
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
        let permissions_u8: u8 = match self.form_fields[1].1.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                self.push_log("Invalid permissions");
                return;
            }
        };
        if permissions_u8 == 0 {
            self.push_log("Permissions cannot be zero");
            return;
        }
        if permissions_u8 & PERM_MANAGE_KEYS != 0 {
            self.push_log("Cannot grant PERM_MANAGE_KEYS to delegated keys");
            return;
        }

        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };
        let admin_asset = self.my_asset.unwrap();

        let asset_kp = Keypair::new();
        let new_asset = asset_kp.pubkey();
        let (new_key_state, _) = Pubkey::find_program_address(
            &[
                KeyState::SEED,
                new_asset.as_ref(),
            ],
            &hardig::ID,
        );

        // Parse rate-limit params (only present when corresponding limited bit is set)
        let sell_cap = self.find_field_value("Sell Capacity")
            .and_then(|v| parse_sol_to_lamports(&v))
            .unwrap_or(0);
        let sell_refill: u64 = self.find_field_value("Sell Refill")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        let borrow_cap = self.find_field_value("Borrow Capacity")
            .and_then(|v| parse_sol_to_lamports(&v))
            .unwrap_or(0);
        let borrow_refill: u64 = self.find_field_value("Borrow Refill")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);

        let mut data = sighash("authorize_key");
        data.push(permissions_u8);
        data.extend_from_slice(&sell_cap.to_le_bytes());
        data.extend_from_slice(&sell_refill.to_le_bytes());
        data.extend_from_slice(&borrow_cap.to_le_bytes());
        data.extend_from_slice(&borrow_refill.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new_readonly(admin_asset, false),
            AccountMeta::new_readonly(position_pda, false),
            AccountMeta::new(new_asset, true),
            AccountMeta::new_readonly(target_wallet, false),
            AccountMeta::new(new_key_state, false),
            AccountMeta::new_readonly(self.program_pda, false),
            AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        self.goto_confirm(PendingAction {
            description: vec![
                "Authorize Key".into(),
                format!("Target: {}", target_wallet),
                format!("Permissions: {} (0x{:02X})", permissions_name(permissions_u8), permissions_u8),
                format!("Key Asset: {}", new_asset),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![asset_kp],
        });
    }

    pub fn build_revoke_key(&mut self) {
        let admin_asset_key = self.position.as_ref().map(|p| p.admin_asset);
        let revocable: Vec<&KeyEntry> = self
            .keyring
            .iter()
            .filter(|k| Some(k.asset) != admin_asset_key)
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
        let admin_asset = self.my_asset.unwrap();

        // Derive the target key's KeyState PDA
        let (target_key_state, _) = Pubkey::find_program_address(
            &[KeyState::SEED, target.asset.as_ref()],
            &hardig::ID,
        );

        let data = sighash("revoke_key");
        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new_readonly(admin_asset, false),
            AccountMeta::new_readonly(position_pda, false),
            AccountMeta::new(target.asset, false),
            AccountMeta::new(target_key_state, false),
            AccountMeta::new_readonly(self.program_pda, false),
            AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        self.goto_confirm(PendingAction {
            description: vec![
                "Revoke Key".into(),
                format!("Key Asset: {}", target.asset),
                format!("Permissions: {}", permissions_name(target.permissions)),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    /// Combined one-time setup: init Mayflower position + create ATAs.
    /// Only includes instructions for steps not yet completed.
    ///
    /// When `nav_mint` is `Some(mint)`, use that mint for deriving the
    /// MarketConfig PDA.  The MarketConfig must already exist on-chain
    /// (auto-creation only happens for the default navSOL mint).
    ///
    /// When `nav_mint` is `None`, keep existing behaviour: use defaults and
    /// auto-create if needed.
    pub fn build_setup(&mut self, nav_mint: Option<Pubkey>) {
        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };

        // If a custom nav-mint was supplied, try to load its MarketConfig now
        // (discover_position only loaded the default or position-stored one).
        if let Some(custom_mint) = nav_mint {
            let (custom_mc_pda, _) = Pubkey::find_program_address(
                &[MarketConfig::SEED, custom_mint.as_ref()],
                &hardig::ID,
            );
            // Only reload if we don't already have this exact MarketConfig
            if self.market_config_pda != Some(custom_mc_pda) {
                match self.rpc.get_account(&custom_mc_pda) {
                    Ok(mc_acc) => {
                        match MarketConfig::try_deserialize(&mut mc_acc.data.as_slice()) {
                            Ok(mc) => {
                                self.market_config_pda = Some(custom_mc_pda);
                                self.market_config = Some(mc);
                                // Re-derive Mayflower addresses with the new market config
                                if self.position.is_some() {
                                    let mc = self.market_config.as_ref().unwrap();
                                    let (pp_pda, _) = derive_personal_position(&self.program_pda, &mc.market_meta);
                                    let (escrow_pda, _) = derive_personal_position_escrow(&pp_pda);
                                    self.pp_pda = pp_pda;
                                    self.escrow_pda = escrow_pda;
                                    self.wsol_ata = get_ata(&self.program_pda, &mc.base_mint);
                                    self.nav_sol_ata = get_ata(&self.program_pda, &mc.nav_mint);
                                    // Re-check whether the Mayflower position is initialized
                                    self.mayflower_initialized = self.rpc.get_account(&pp_pda).is_ok();
                                    self.refresh_mayflower_state();
                                    self.push_log(format!(
                                        "Using custom nav-mint MarketConfig: {}",
                                        short_pubkey(&custom_mc_pda),
                                    ));
                                }
                            }
                            Err(_) => {
                                self.push_log(format!(
                                    "MarketConfig at {} exists but failed to deserialize",
                                    custom_mc_pda,
                                ));
                                return;
                            }
                        }
                    }
                    Err(_) => {
                        self.push_log(format!(
                            "No MarketConfig found for nav-mint {}. Create it first with create-market-config.",
                            custom_mint,
                        ));
                        return;
                    }
                }
            }
        }

        let mut instructions = Vec::new();
        let mut description = vec!["Setup Mayflower Accounts".into()];

        // Step 0: Create MarketConfig if it doesn't exist on-chain yet.
        // Auto-creation only happens when using the default nav-mint (no custom
        // --nav-mint was provided, or it matches the default).
        let mc_pda = self.market_config_pda.unwrap_or_else(|| {
            Pubkey::find_program_address(
                &[MarketConfig::SEED, DEFAULT_NAV_SOL_MINT.as_ref()],
                &hardig::ID,
            )
            .0
        });
        if self.market_config.is_none() {
            // When a custom nav-mint is specified the MarketConfig must already
            // exist — we returned early above if it didn't.  So reaching here
            // means we are using the default and can auto-create.
            let (config_pda, _) =
                Pubkey::find_program_address(&[ProtocolConfig::SEED], &hardig::ID);

            let mut data = sighash("create_market_config");
            data.extend_from_slice(DEFAULT_NAV_SOL_MINT.as_ref());
            data.extend_from_slice(DEFAULT_WSOL_MINT.as_ref());
            data.extend_from_slice(DEFAULT_MARKET_GROUP.as_ref());
            data.extend_from_slice(DEFAULT_MARKET_META.as_ref());
            data.extend_from_slice(DEFAULT_MAYFLOWER_MARKET.as_ref());
            data.extend_from_slice(DEFAULT_MARKET_BASE_VAULT.as_ref());
            data.extend_from_slice(DEFAULT_MARKET_NAV_VAULT.as_ref());
            data.extend_from_slice(DEFAULT_FEE_VAULT.as_ref());

            let accounts = vec![
                AccountMeta::new(self.keypair.pubkey(), true),
                AccountMeta::new_readonly(config_pda, false),
                AccountMeta::new(mc_pda, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            ];
            instructions.push(Instruction::new_with_bytes(hardig::ID, &data, accounts));
            description.push(format!(
                "Create MarketConfig: {}",
                short_pubkey(&mc_pda)
            ));
        }

        // Step 1: Init Mayflower position if needed
        if !self.mayflower_initialized {
            let admin_asset = self.my_asset.unwrap();

            let market_meta = self
                .market_config
                .as_ref()
                .map(|mc| mc.market_meta)
                .unwrap_or(DEFAULT_MARKET_META);
            let nav_mint = self
                .market_config
                .as_ref()
                .map(|mc| mc.nav_mint)
                .unwrap_or(DEFAULT_NAV_SOL_MINT);

            let data = sighash("init_mayflower_position");
            let accounts = vec![
                AccountMeta::new(self.keypair.pubkey(), true),
                AccountMeta::new_readonly(admin_asset, false),
                AccountMeta::new(position_pda, false),
                AccountMeta::new_readonly(mc_pda, false),
                AccountMeta::new_readonly(self.program_pda, false),
                AccountMeta::new(self.pp_pda, false),
                AccountMeta::new(self.escrow_pda, false),
                AccountMeta::new_readonly(market_meta, false),
                AccountMeta::new_readonly(nav_mint, false),
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
            let base_mint = self
                .market_config
                .as_ref()
                .map(|mc| mc.base_mint)
                .unwrap_or(DEFAULT_WSOL_MINT);
            let nav_mint = self
                .market_config
                .as_ref()
                .map(|mc| mc.nav_mint)
                .unwrap_or(DEFAULT_NAV_SOL_MINT);
            instructions.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &payer,
                    &self.program_pda,
                    &base_mint,
                    &SPL_TOKEN_ID,
                ),
            );
            instructions.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &payer,
                    &self.program_pda,
                    &nav_mint,
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

    pub fn build_buy(&mut self) {
        let amount = match parse_sol_to_lamports(&self.form_fields[0].1) {
            Some(v) => v,
            None => {
                self.push_log("Invalid SOL amount");
                return;
            }
        };
        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };

        let key_asset = self.my_asset.unwrap();
        let mc_pda = self.market_config_pda.unwrap();
        let mc = self.market_config.as_ref().unwrap();

        let mut data = sighash("buy");
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage protection)

        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),          // signer
            AccountMeta::new_readonly(key_asset, false),            // key_asset
            AccountMeta::new(position_pda, false),                  // position
            AccountMeta::new_readonly(mc_pda, false),               // market_config
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),    // system_program
            AccountMeta::new(self.program_pda, false),              // program_pda
            AccountMeta::new(self.pp_pda, false),                   // personal_position
            AccountMeta::new(self.escrow_pda, false),               // user_shares
            AccountMeta::new(self.nav_sol_ata, false),              // user_nav_sol_ata
            AccountMeta::new(self.wsol_ata, false),                 // user_wsol_ata
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),     // tenant
            AccountMeta::new_readonly(mc.market_group, false),      // market_group
            AccountMeta::new_readonly(mc.market_meta, false),       // market_meta
            AccountMeta::new(mc.mayflower_market, false),           // mayflower_market
            AccountMeta::new(mc.nav_mint, false),                   // nav_sol_mint
            AccountMeta::new(mc.market_base_vault, false),          // market_base_vault
            AccountMeta::new(mc.market_nav_vault, false),           // market_nav_vault
            AccountMeta::new(mc.fee_vault, false),                  // fee_vault
            AccountMeta::new_readonly(mc.base_mint, false),         // wsol_mint
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // mayflower_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),         // token_program
            AccountMeta::new(self.log_pda, false),                  // log_account
        ];

        // Ensure PDA's wSOL ATA exists (may have been closed by a previous sell)
        let create_ata_ix = create_ata_idempotent_ix(
            &self.keypair.pubkey(),
            &self.program_pda,
            &mc.base_mint,
        );

        // Wrap SOL → wSOL ATA, then sync_native
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
                format!("Position: {}", short_pubkey(&position_pda)),
                format!(
                    "Permissions: {}",
                    permissions_name(self.my_permissions.unwrap_or(0))
                ),
            ],
            instructions: vec![create_ata_ix, transfer_ix, sync_ix, buy_ix],
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
        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };

        let key_asset = self.my_asset.unwrap();
        let mc_pda = self.market_config_pda.unwrap();
        let mc = self.market_config.as_ref().unwrap();

        let mut data = sighash("withdraw");
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage protection)

        // For sell/withdraw, key_state is optional (only needed for rate-limited keys).
        // Pass it if we have a KeyState PDA for this key.
        let key_state_pda = self.my_key_state_pda;

        let mut accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),          // admin
            AccountMeta::new_readonly(key_asset, false),            // key_asset
        ];
        // Optional key_state account (Anchor Option<Account>)
        if let Some(ks_pda) = key_state_pda {
            accounts.push(AccountMeta::new(ks_pda, false));         // key_state (Some)
        } else {
            accounts.push(AccountMeta::new_readonly(hardig::ID, false)); // key_state (None sentinel)
        }
        accounts.extend_from_slice(&[
            AccountMeta::new(position_pda, false),                  // position
            AccountMeta::new_readonly(mc_pda, false),               // market_config
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),    // system_program
            AccountMeta::new(self.program_pda, false),              // program_pda
            AccountMeta::new(self.pp_pda, false),                   // personal_position
            AccountMeta::new(self.escrow_pda, false),               // user_shares
            AccountMeta::new(self.nav_sol_ata, false),              // user_nav_sol_ata
            AccountMeta::new(self.wsol_ata, false),                 // user_wsol_ata
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),     // tenant
            AccountMeta::new_readonly(mc.market_group, false),      // market_group
            AccountMeta::new_readonly(mc.market_meta, false),       // market_meta
            AccountMeta::new(mc.mayflower_market, false),           // mayflower_market
            AccountMeta::new(mc.nav_mint, false),                   // nav_sol_mint
            AccountMeta::new(mc.market_base_vault, false),          // market_base_vault
            AccountMeta::new(mc.market_nav_vault, false),           // market_nav_vault
            AccountMeta::new(mc.fee_vault, false),                  // fee_vault
            AccountMeta::new_readonly(mc.base_mint, false),         // wsol_mint
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // mayflower_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),         // token_program
            AccountMeta::new(self.log_pda, false),                  // log_account
        ]);

        // Sell CPI uses ~170K CUs inside Mayflower + close_account — needs extra compute
        let compute_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(400_000);

        // Ensure PDA's wSOL ATA exists (may have been closed by a previous sell)
        let create_ata_ix = create_ata_idempotent_ix(
            &self.keypair.pubkey(),
            &self.program_pda,
            &mc.base_mint,
        );

        self.goto_confirm(PendingAction {
            description: vec![
                "Sell navSOL".into(),
                format!("Amount: {} navSOL", lamports_to_sol(amount)),
                format!("Position: {}", short_pubkey(&position_pda)),
            ],
            instructions: vec![compute_ix, create_ata_ix, Instruction::new_with_bytes(hardig::ID, &data, accounts)],
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
        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };

        let key_asset = self.my_asset.unwrap();
        let mc_pda = self.market_config_pda.unwrap();
        let mc = self.market_config.as_ref().unwrap();

        let mut data = sighash("borrow");
        data.extend_from_slice(&amount.to_le_bytes());

        // For borrow, key_state is optional (only needed for rate-limited keys).
        let key_state_pda = self.my_key_state_pda;

        let mut accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),          // admin
            AccountMeta::new_readonly(key_asset, false),            // key_asset
        ];
        // Optional key_state account (Anchor Option<Account>)
        if let Some(ks_pda) = key_state_pda {
            accounts.push(AccountMeta::new(ks_pda, false));         // key_state (Some)
        } else {
            accounts.push(AccountMeta::new_readonly(hardig::ID, false)); // key_state (None sentinel)
        }
        accounts.extend_from_slice(&[
            AccountMeta::new(position_pda, false),                  // position
            AccountMeta::new_readonly(mc_pda, false),               // market_config
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),    // system_program
            AccountMeta::new(self.program_pda, false),              // program_pda
            AccountMeta::new(self.pp_pda, false),                   // personal_position
            AccountMeta::new(self.wsol_ata, false),                 // user_base_token_ata
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),     // tenant
            AccountMeta::new_readonly(mc.market_group, false),      // market_group
            AccountMeta::new_readonly(mc.market_meta, false),       // market_meta
            AccountMeta::new(mc.market_base_vault, false),          // market_base_vault
            AccountMeta::new(mc.market_nav_vault, false),           // market_nav_vault
            AccountMeta::new(mc.fee_vault, false),                  // fee_vault
            AccountMeta::new_readonly(mc.base_mint, false),         // wsol_mint
            AccountMeta::new(mc.mayflower_market, false),           // mayflower_market
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // mayflower_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),         // token_program
            AccountMeta::new(self.log_pda, false),                  // log_account
        ]);

        // Ensure PDA's wSOL ATA exists (may have been closed by a previous sell/borrow)
        let create_ata_ix = create_ata_idempotent_ix(
            &self.keypair.pubkey(),
            &self.program_pda,
            &mc.base_mint,
        );

        self.goto_confirm(PendingAction {
            description: vec![
                "Borrow".into(),
                format!("Amount: {} SOL", lamports_to_sol(amount)),
                format!("Position: {}", short_pubkey(&position_pda)),
            ],
            instructions: vec![create_ata_ix, Instruction::new_with_bytes(hardig::ID, &data, accounts)],
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
        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };

        let key_asset = self.my_asset.unwrap();
        let mc_pda = self.market_config_pda.unwrap();
        let mc = self.market_config.as_ref().unwrap();

        let mut data = sighash("repay");
        data.extend_from_slice(&amount.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),          // signer
            AccountMeta::new_readonly(key_asset, false),            // key_asset
            AccountMeta::new(position_pda, false),                  // position
            AccountMeta::new_readonly(mc_pda, false),               // market_config
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),    // system_program
            AccountMeta::new(self.program_pda, false),              // program_pda
            AccountMeta::new(self.pp_pda, false),                   // personal_position
            AccountMeta::new(self.wsol_ata, false),                 // user_base_token_ata
            AccountMeta::new_readonly(mc.market_meta, false),       // market_meta
            AccountMeta::new(mc.market_base_vault, false),          // market_base_vault
            AccountMeta::new_readonly(mc.base_mint, false),         // wsol_mint
            AccountMeta::new(mc.mayflower_market, false),           // mayflower_market
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // mayflower_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),         // token_program
            AccountMeta::new(self.log_pda, false),                  // log_account
        ];

        // Ensure PDA's wSOL ATA exists (may have been closed by a previous sell/borrow)
        let create_ata_ix = create_ata_idempotent_ix(
            &self.keypair.pubkey(),
            &self.program_pda,
            &mc.base_mint,
        );

        // Wrap SOL → PDA's wSOL ATA, then sync_native
        let transfer_ix = solana_sdk::system_instruction::transfer(
            &self.keypair.pubkey(),
            &self.wsol_ata,
            amount,
        );
        let sync_ix =
            spl_token::instruction::sync_native(&SPL_TOKEN_ID, &self.wsol_ata).unwrap();

        let repay_ix = Instruction::new_with_bytes(hardig::ID, &data, accounts);

        self.goto_confirm(PendingAction {
            description: vec![
                "Repay".into(),
                format!("Amount: {} SOL", lamports_to_sol(amount)),
                format!("Position: {}", short_pubkey(&position_pda)),
            ],
            instructions: vec![create_ata_ix, transfer_ix, sync_ix, repay_ix],
            extra_signers: vec![],
        });
    }

    pub fn build_reinvest(&mut self) {
        let position_pda = match self.position_pda {
            Some(p) => p,
            None => {
                self.push_log("No position loaded");
                return;
            }
        };

        let key_asset = self.my_asset.unwrap();
        let mc_pda = self.market_config_pda.unwrap();
        let mc = self.market_config.as_ref().unwrap();

        let mut data = sighash("reinvest");
        data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage protection)

        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),          // signer
            AccountMeta::new_readonly(key_asset, false),            // key_asset
            AccountMeta::new(position_pda, false),                  // position
            AccountMeta::new_readonly(mc_pda, false),               // market_config
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),    // system_program
            AccountMeta::new(self.program_pda, false),              // program_pda
            AccountMeta::new(self.pp_pda, false),                   // personal_position
            AccountMeta::new(self.escrow_pda, false),               // user_shares
            AccountMeta::new(self.nav_sol_ata, false),              // user_nav_sol_ata
            AccountMeta::new(self.wsol_ata, false),                 // user_wsol_ata
            AccountMeta::new(self.wsol_ata, false),                 // user_base_token_ata (same)
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),     // tenant
            AccountMeta::new_readonly(mc.market_group, false),      // market_group
            AccountMeta::new_readonly(mc.market_meta, false),       // market_meta
            AccountMeta::new(mc.mayflower_market, false),           // mayflower_market
            AccountMeta::new(mc.nav_mint, false),                   // nav_sol_mint
            AccountMeta::new(mc.market_base_vault, false),          // market_base_vault
            AccountMeta::new(mc.market_nav_vault, false),           // market_nav_vault
            AccountMeta::new(mc.fee_vault, false),                  // fee_vault
            AccountMeta::new_readonly(mc.base_mint, false),         // wsol_mint
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // mayflower_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),         // token_program
            AccountMeta::new(self.log_pda, false),                  // log_account
        ];

        // Reinvest does borrow + buy CPIs in one tx — needs extra compute
        let compute_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(400_000);

        // Ensure PDA's wSOL ATA exists (may have been closed by a previous sell/borrow)
        let create_ata_ix = create_ata_idempotent_ix(
            &self.keypair.pubkey(),
            &self.program_pda,
            &mc.base_mint,
        );

        self.goto_confirm(PendingAction {
            description: vec![
                "Reinvest (CPI)".into(),
                format!("Position: {}", short_pubkey(&position_pda)),
                format!(
                    "Permissions: {}",
                    permissions_name(self.my_permissions.unwrap_or(0))
                ),
                "Borrows available capacity and buys more navSOL".into(),
            ],
            instructions: vec![compute_ix, create_ata_ix, Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    pub fn build_transfer_admin(&mut self, new_admin: Pubkey) {
        let (config_pda, _) =
            Pubkey::find_program_address(&[ProtocolConfig::SEED], &hardig::ID);

        let mut data = sighash("transfer_admin");
        data.extend_from_slice(new_admin.as_ref());

        let accounts = vec![
            AccountMeta::new_readonly(self.keypair.pubkey(), true),
            AccountMeta::new(config_pda, false),
        ];

        self.goto_confirm(PendingAction {
            description: vec![
                "Transfer Protocol Admin".into(),
                format!("New Admin: {}", new_admin),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
            extra_signers: vec![],
        });
    }

    pub fn build_create_market_config(
        &mut self,
        nav_mint: Pubkey,
        base_mint: Pubkey,
        market_group: Pubkey,
        market_meta: Pubkey,
        mayflower_market: Pubkey,
        market_base_vault: Pubkey,
        market_nav_vault: Pubkey,
        fee_vault: Pubkey,
    ) {
        let (config_pda, _) =
            Pubkey::find_program_address(&[ProtocolConfig::SEED], &hardig::ID);
        let (mc_pda, _) = Pubkey::find_program_address(
            &[MarketConfig::SEED, nav_mint.as_ref()],
            &hardig::ID,
        );

        let mut data = sighash("create_market_config");
        data.extend_from_slice(nav_mint.as_ref());
        data.extend_from_slice(base_mint.as_ref());
        data.extend_from_slice(market_group.as_ref());
        data.extend_from_slice(market_meta.as_ref());
        data.extend_from_slice(mayflower_market.as_ref());
        data.extend_from_slice(market_base_vault.as_ref());
        data.extend_from_slice(market_nav_vault.as_ref());
        data.extend_from_slice(fee_vault.as_ref());

        let accounts = vec![
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(mc_pda, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        self.goto_confirm(PendingAction {
            description: vec![
                "Create Market Config".into(),
                format!("Nav Mint: {}", nav_mint),
                format!("MarketConfig PDA: {}", mc_pda),
            ],
            instructions: vec![Instruction::new_with_bytes(hardig::ID, &data, accounts)],
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
        self.my_permissions = None;
        self.my_key_state_pda = None;
        self.my_asset = None;
        self.keyring.clear();
        self.market_config_pda = None;
        self.market_config = None;

        // Step 1: Get all PositionNFT accounts from the program
        let pos_config = RpcProgramAccountsConfig {
            filters: Some(vec![RpcFilterType::DataSize(
                PositionNFT::SIZE as u64,
            )]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                commitment: Some(CommitmentConfig::confirmed()),
                ..Default::default()
            },
            ..Default::default()
        };

        let positions = match self.rpc.get_program_accounts_with_config(&hardig::ID, pos_config) {
            Ok(a) => a,
            Err(e) => {
                self.push_log(format!("Scan failed: {}", e));
                return;
            }
        };

        // Step 2: For each position, check if signer owns the admin asset
        let signer = self.keypair.pubkey();
        let mut found_position: Option<Pubkey> = None;
        let mut found_pos_data: Option<PositionNFT> = None;

        for (pos_pda, pos_acc) in &positions {
            let pos = match PositionNFT::try_deserialize(&mut pos_acc.data.as_slice()) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if self.check_asset_owner(&pos.admin_asset, &signer) {
                found_position = Some(*pos_pda);
                found_pos_data = Some(pos);
                break;
            }
        }

        // Step 3: If not found via admin key, check delegated keys via KeyState accounts
        if found_position.is_none() {
            let ks_config = RpcProgramAccountsConfig {
                filters: Some(vec![RpcFilterType::DataSize(
                    KeyState::SIZE as u64,
                )]),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..Default::default()
                },
                ..Default::default()
            };

            if let Ok(key_states) = self.rpc.get_program_accounts_with_config(&hardig::ID, ks_config) {
                for (_ks_pda, ks_acc) in &key_states {
                    let ks = match KeyState::try_deserialize(&mut ks_acc.data.as_slice()) {
                        Ok(k) => k,
                        Err(_) => continue,
                    };

                    if !self.check_asset_owner(&ks.asset, &signer) {
                        continue;
                    }

                    // Read the asset's update_authority to find which position it belongs to
                    let ua = match self.read_asset_update_authority(&ks.asset) {
                        Some(ua) => ua,
                        None => continue,
                    };

                    // Find the position whose program_pda matches this update_authority
                    for (pos_pda, pos_acc) in &positions {
                        let pos = match PositionNFT::try_deserialize(&mut pos_acc.data.as_slice()) {
                            Ok(p) => p,
                            Err(_) => continue,
                        };
                        let (expected_pda, _) = Pubkey::find_program_address(
                            &[b"authority", pos.admin_asset.as_ref()],
                            &hardig::ID,
                        );
                        if expected_pda == ua {
                            found_position = Some(*pos_pda);
                            found_pos_data = Some(pos);
                            break;
                        }
                    }
                    if found_position.is_some() {
                        break;
                    }
                }
            }
        }

        if let (Some(pos_pda), Some(pos)) = (found_position, found_pos_data) {
            // Derive the program_pda for this position
            let admin_asset = pos.admin_asset;
            let (program_pda, _) = Pubkey::find_program_address(
                &[b"authority", admin_asset.as_ref()],
                &hardig::ID,
            );

            // Check if signer holds the admin asset
            let signer_is_admin = self.check_asset_owner(&admin_asset, &signer);
            if signer_is_admin {
                self.my_permissions = Some(PRESET_ADMIN);
                self.my_key_state_pda = None; // Admin has no KeyState PDA
                self.my_asset = Some(admin_asset);
            }

            // If not admin, find the signer's delegated key
            if !signer_is_admin {
                // Scan KeyState accounts to find signer's key
                let ks_config = RpcProgramAccountsConfig {
                    filters: Some(vec![RpcFilterType::DataSize(
                        KeyState::SIZE as u64,
                    )]),
                    account_config: RpcAccountInfoConfig {
                        encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                        commitment: Some(CommitmentConfig::confirmed()),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                if let Ok(key_states) = self.rpc.get_program_accounts_with_config(&hardig::ID, ks_config) {
                    for (ks_pda, ks_acc) in &key_states {
                        let ks = match KeyState::try_deserialize(&mut ks_acc.data.as_slice()) {
                            Ok(k) => k,
                            Err(_) => continue,
                        };
                        if !self.check_asset_owner(&ks.asset, &signer) {
                            continue;
                        }
                        // Verify this key belongs to this position
                        if let Some(ua) = self.read_asset_update_authority(&ks.asset) {
                            if ua == program_pda {
                                if let Some(perms) = self.read_asset_permissions(&ks.asset) {
                                    self.my_permissions = Some(perms);
                                    self.my_key_state_pda = Some(*ks_pda);
                                    self.my_asset = Some(ks.asset);
                                }
                                break;
                            }
                        }
                    }
                }
            }

            self.position_pda = Some(pos_pda);
            self.mayflower_initialized = pos.position_pda != Pubkey::default();

            // Fetch market config: from position if set, otherwise try default
            let mc_key = if pos.market_config != Pubkey::default() {
                pos.market_config
            } else {
                Pubkey::find_program_address(
                    &[MarketConfig::SEED, DEFAULT_NAV_SOL_MINT.as_ref()],
                    &hardig::ID,
                )
                .0
            };
            if let Ok(mc_acc) = self.rpc.get_account(&mc_key) {
                if let Ok(mc) =
                    MarketConfig::try_deserialize(&mut mc_acc.data.as_slice())
                {
                    self.market_config_pda = Some(mc_key);
                    self.market_config = Some(mc);
                }
            }

            // Derive per-position Mayflower addresses from admin_asset
            self.program_pda = program_pda;
            let market_meta = self
                .market_config
                .as_ref()
                .map(|mc| mc.market_meta)
                .unwrap_or(DEFAULT_MARKET_META);
            let (pp_pda, _) = derive_personal_position(&program_pda, &market_meta);
            let (escrow_pda, _) = derive_personal_position_escrow(&pp_pda);
            self.pp_pda = pp_pda;
            self.escrow_pda = escrow_pda;
            let base_mint = self
                .market_config
                .as_ref()
                .map(|mc| mc.base_mint)
                .unwrap_or(DEFAULT_WSOL_MINT);
            let nav_mint = self
                .market_config
                .as_ref()
                .map(|mc| mc.nav_mint)
                .unwrap_or(DEFAULT_NAV_SOL_MINT);
            self.wsol_ata = get_ata(&program_pda, &base_mint);
            self.nav_sol_ata = get_ata(&program_pda, &nav_mint);

            self.position = Some(pos);

            // Build keyring: admin key + all delegated keys (via KeyState scan)
            self.keyring.push(KeyEntry {
                pda: pos_pda,
                asset: admin_asset,
                permissions: PRESET_ADMIN,
                held_by_signer: signer_is_admin,
            });

            // Scan KeyState accounts for delegated keys belonging to this position
            let ks_config = RpcProgramAccountsConfig {
                filters: Some(vec![RpcFilterType::DataSize(
                    KeyState::SIZE as u64,
                )]),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..Default::default()
                },
                ..Default::default()
            };
            if let Ok(key_states) = self.rpc.get_program_accounts_with_config(&hardig::ID, ks_config) {
                for (ks_pda, ks_acc) in &key_states {
                    let ks = match KeyState::try_deserialize(&mut ks_acc.data.as_slice()) {
                        Ok(k) => k,
                        Err(_) => continue,
                    };
                    // Check if this key belongs to this position (update_authority == program_pda)
                    let ua = match self.read_asset_update_authority(&ks.asset) {
                        Some(ua) => ua,
                        None => continue,
                    };
                    if ua != program_pda {
                        continue;
                    }
                    let perms = self.read_asset_permissions(&ks.asset).unwrap_or(0);
                    let held = self.check_asset_owner(&ks.asset, &signer);
                    self.keyring.push(KeyEntry {
                        pda: *ks_pda,
                        asset: ks.asset,
                        permissions: perms,
                        held_by_signer: held,
                    });
                }
            }

            let perms = self.my_permissions.unwrap_or(0);
            self.push_log(format!(
                "Found position {} (permissions: {}{})",
                short_pubkey(&pos_pda),
                permissions_name(perms),
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

        // Check ATAs — each may or may not exist (sell/borrow close the wSOL ATA)
        let wsol_exists = self.read_token_balance(&self.wsol_ata);
        let nav_exists = self.read_token_balance(&self.nav_sol_ata);
        if let Some(wsol) = wsol_exists {
            self.wsol_balance = wsol;
        }
        if let Some(nav) = nav_exists {
            self.nav_sol_balance = nav;
        }
        self.atas_exist = wsol_exists.is_some() || nav_exists.is_some();

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
        let mf_market = self
            .market_config
            .as_ref()
            .map(|mc| mc.mayflower_market)
            .unwrap_or(DEFAULT_MAYFLOWER_MARKET);
        if let Ok(market_acc) = self.rpc.get_account(&mf_market) {
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

    /// Check if a given wallet owns an MPL-Core asset by reading the asset account's
    /// owner field (bytes 1..33 of the account data).
    fn check_asset_owner(&self, asset: &Pubkey, wallet: &Pubkey) -> bool {
        if let Ok(acc) = self.rpc.get_account(asset) {
            if acc.data.len() >= 33 {
                if let Ok(owner) = Pubkey::try_from(&acc.data[1..33]) {
                    return owner == *wallet;
                }
            }
        }
        false
    }

    /// Read the update_authority pubkey from an MPL-Core asset account.
    /// The update_authority is a borsh-encoded enum at byte 33+.
    /// Tag 1 = UpdateAuthority::Address, followed by 32-byte pubkey.
    fn read_asset_update_authority(&self, asset: &Pubkey) -> Option<Pubkey> {
        let acc = self.rpc.get_account(asset).ok()?;
        if acc.data.len() < 66 {
            return None;
        }
        let ua_tag = acc.data[33];
        if ua_tag != 1 {
            return None;
        }
        Pubkey::try_from(&acc.data[34..66]).ok()
    }

    /// Read the permissions value from an MPL-Core asset's Attributes plugin.
    fn read_asset_permissions(&self, asset: &Pubkey) -> Option<u8> {
        use mpl_core::{
            accounts::BaseAssetV1,
            fetch_plugin,
            types::{Attributes, PluginType},
        };
        let acc = self.rpc.get_account(asset).ok()?;
        // Create a temporary AccountInfo for fetch_plugin
        let asset_key = *asset;
        let mut lamports = acc.lamports;
        let mut data = acc.data.clone();
        let account_info = solana_sdk::account_info::AccountInfo::new(
            &asset_key,
            false,
            false,
            &mut lamports,
            &mut data,
            &acc.owner,
            false,
            0,
        );
        let (_, attributes, _) = fetch_plugin::<BaseAssetV1, Attributes>(
            &account_info,
            PluginType::Attributes,
        ).ok()?;
        attributes
            .attribute_list
            .iter()
            .find(|a| a.key == "permissions")
            .and_then(|a| a.value.parse::<u8>().ok())
    }

    pub fn take_snapshot(&self) -> Option<PositionSnapshot> {
        let pos = self.position.as_ref()?;
        Some(PositionSnapshot {
            deposited_nav: pos.deposited_nav,
            user_debt: pos.user_debt,
            borrow_capacity: self.mf_borrow_capacity,
            wsol_balance: self.wsol_balance,
            nav_sol_balance: self.nav_sol_balance,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn permissions_name(permissions: u8) -> String {
    if permissions == PRESET_ADMIN {
        return "Admin".into();
    }
    if permissions == 0 {
        return "None".into();
    }
    let bits: &[(u8, &str)] = &[
        (PERM_BUY, "Buy"),
        (PERM_SELL, "Sell"),
        (PERM_BORROW, "Borrow"),
        (PERM_REPAY, "Repay"),
        (PERM_REINVEST, "Reinvest"),
        (PERM_MANAGE_KEYS, "ManageKeys"),
        (PERM_LIMITED_SELL, "LimSell"),
        (PERM_LIMITED_BORROW, "LimBorrow"),
    ];
    let names: Vec<&str> = bits
        .iter()
        .filter(|(bit, _)| permissions & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if names.is_empty() {
        "None".into()
    } else {
        names.join(", ")
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
    if lamports == 0 {
        "0".to_string()
    } else {
        let whole = lamports / 1_000_000_000;
        let frac = lamports % 1_000_000_000;
        // Full precision, trim trailing zeros — lossless round-trip with parse_sol_to_lamports
        let s = format!("{}.{:09}", whole, frac);
        s.trim_end_matches('0').to_string()
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
