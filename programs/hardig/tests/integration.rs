use litesvm::LiteSVM;
use solana_sdk::{
    account::Account,
    clock::Clock,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};

use anchor_lang::AccountDeserialize;
use hardig::mayflower::{
    self, DEFAULT_FEE_VAULT, DEFAULT_MARKET_BASE_VAULT, DEFAULT_MARKET_GROUP, DEFAULT_MARKET_META,
    DEFAULT_MARKET_NAV_VAULT, DEFAULT_MAYFLOWER_MARKET, DEFAULT_NAV_SOL_MINT, DEFAULT_WSOL_MINT,
    MAYFLOWER_PROGRAM_ID, MAYFLOWER_TENANT, PP_DISCRIMINATOR,
};
use hardig::state::{
    ClaimReceipt, KeyState, MarketConfig, PositionNFT, PromoConfig, ProtocolConfig,
    PERM_BUY, PERM_SELL, PERM_MANAGE_KEYS, PERM_REINVEST,
    PERM_LIMITED_SELL, PERM_LIMITED_BORROW,
    PRESET_ADMIN, PRESET_DEPOSITOR, PRESET_KEEPER, PRESET_OPERATOR,
};

const SPL_TOKEN_ID: Pubkey = solana_sdk::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ATA_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const MPL_CORE_ID: Pubkey =
    solana_sdk::pubkey!("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

fn program_id() -> Pubkey {
    hardig::ID
}

fn sighash(name: &str) -> Vec<u8> {
    let hash = solana_sdk::hash::hash(format!("global:{}", name).as_bytes());
    hash.to_bytes()[..8].to_vec()
}

fn get_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), SPL_TOKEN_ID.as_ref(), mint.as_ref()],
        &ATA_PROGRAM_ID,
    )
    .0
}

fn market_config_pda(nav_mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[MarketConfig::SEED, nav_mint.as_ref()],
        &program_id(),
    )
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();

    // Load hardig program
    let program_bytes = std::fs::read("../../target/deploy/hardig.so")
        .expect("Run `anchor build` first");
    let _ = svm.add_program(program_id(), &program_bytes);

    // Load mock Mayflower program
    let mock_bytes = std::fs::read("../../target/deploy/mock_mayflower.so")
        .expect("Run `anchor build` first (mock-mayflower)");
    let _ = svm.add_program(MAYFLOWER_PROGRAM_ID, &mock_bytes);

    // Load MPL-Core program
    let mpl_core_bytes = std::fs::read("../../test-fixtures/mpl_core.so")
        .expect("Run `solana program dump CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d test-fixtures/mpl_core.so`");
    let _ = svm.add_program(MPL_CORE_ID, &mpl_core_bytes);

    // Plant stub accounts at all constant Mayflower addresses
    plant_mayflower_stubs(&mut svm);

    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    (svm, admin)
}

/// Create minimal accounts at all constant Mayflower addresses and derived PDAs.
fn plant_mayflower_stubs(svm: &mut LiteSVM) {
    let owner = MAYFLOWER_PROGRAM_ID;

    // Constant addresses: tenant, market_group, market_meta
    for addr in [
        MAYFLOWER_TENANT,
        DEFAULT_MARKET_GROUP,
        DEFAULT_MARKET_META,
    ] {
        plant_account(svm, &addr, &owner, 256);
    }

    // Mutable constant addresses
    for addr in [
        DEFAULT_MAYFLOWER_MARKET,
        DEFAULT_MARKET_BASE_VAULT,
        DEFAULT_MARKET_NAV_VAULT,
        DEFAULT_FEE_VAULT,
    ] {
        plant_account(svm, &addr, &owner, 256);
    }

    // Mints (owned by token program)
    for addr in [DEFAULT_NAV_SOL_MINT, DEFAULT_WSOL_MINT] {
        plant_account(svm, &addr, &SPL_TOKEN_ID, 82);
    }

    // Derived PDAs: log account
    let (log_pda, _) = mayflower::derive_log_account();
    plant_account(svm, &log_pda, &owner, 256);
}

/// Plant the PersonalPosition and user_shares PDAs for a given admin_asset.
/// Must be called BEFORE create_position so the Mayflower CPI has accounts to write to.
fn plant_position_stubs(svm: &mut LiteSVM, admin_asset: &Pubkey) {
    let (program_pda, _) = Pubkey::find_program_address(
        &[b"authority", admin_asset.as_ref()],
        &program_id(),
    );
    let (pp_pda, _) = mayflower::derive_personal_position(&program_pda, &DEFAULT_MARKET_META);
    let (escrow_pda, _) = mayflower::derive_personal_position_escrow(&pp_pda);

    // PersonalPosition — needs valid discriminator and large enough for floor price / debt reads
    let owner = MAYFLOWER_PROGRAM_ID;
    plant_pp_account(svm, &pp_pda, &owner, 256);
    plant_account(svm, &escrow_pda, &owner, 256);

    // ATAs for program PDA (wSOL and navSOL)
    let wsol_ata = get_ata(&program_pda, &DEFAULT_WSOL_MINT);
    let nav_sol_ata = get_ata(&program_pda, &DEFAULT_NAV_SOL_MINT);
    // Token accounts need 165 bytes (SPL token account size), owned by token program
    plant_account(svm, &wsol_ata, &SPL_TOKEN_ID, 165);
    plant_account(svm, &nav_sol_ata, &SPL_TOKEN_ID, 165);
}

fn plant_account(svm: &mut LiteSVM, address: &Pubkey, owner: &Pubkey, size: usize) {
    let account = Account {
        lamports: 1_000_000_000,
        data: vec![0u8; size],
        owner: *owner,
        executable: false,
        rent_epoch: 0,
    };
    svm.set_account(*address, account).unwrap();
}

/// Plant a PersonalPosition stub with the correct Mayflower discriminator.
fn plant_pp_account(svm: &mut LiteSVM, address: &Pubkey, owner: &Pubkey, size: usize) {
    let mut data = vec![0u8; size];
    data[..8].copy_from_slice(&PP_DISCRIMINATOR);
    let account = Account {
        lamports: 1_000_000_000,
        data,
        owner: *owner,
        executable: false,
        rent_epoch: 0,
    };
    svm.set_account(*address, account).unwrap();
}

fn send_tx(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    signers: &[&Keypair],
) -> Result<(), String> {
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(
        ixs,
        Some(&signers[0].pubkey()),
        signers,
        blockhash,
    );
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{:?}", e))
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

fn ix_init_protocol(admin: &Pubkey) -> Instruction {
    let (config_pda, _) =
        Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id());
    Instruction::new_with_bytes(
        program_id(),
        &sighash("initialize_protocol"),
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new(config_pda, false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

fn ix_create_market_config(admin: &Pubkey) -> Instruction {
    let (config_pda, _) =
        Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id());
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("create_market_config");
    // 8 Pubkey args
    data.extend_from_slice(DEFAULT_NAV_SOL_MINT.as_ref());
    data.extend_from_slice(DEFAULT_WSOL_MINT.as_ref());
    data.extend_from_slice(DEFAULT_MARKET_GROUP.as_ref());
    data.extend_from_slice(DEFAULT_MARKET_META.as_ref());
    data.extend_from_slice(DEFAULT_MAYFLOWER_MARKET.as_ref());
    data.extend_from_slice(DEFAULT_MARKET_BASE_VAULT.as_ref());
    data.extend_from_slice(DEFAULT_MARKET_NAV_VAULT.as_ref());
    data.extend_from_slice(DEFAULT_FEE_VAULT.as_ref());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(mc_pda, false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id())
}

fn ix_create_collection(admin: &Pubkey) -> (Instruction, Keypair) {
    let (config_pda, _) = config_pda();
    let collection_kp = Keypair::new();

    let mut data = sighash("create_collection");
    // Borsh-encode the uri String: 4-byte little-endian length prefix + UTF-8 bytes
    let uri = "";
    data.extend_from_slice(&(uri.len() as u32).to_le_bytes());
    data.extend_from_slice(uri.as_bytes());

    let ix = Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),                         // admin
            AccountMeta::new(config_pda, false),                    // config
            AccountMeta::new(collection_kp.pubkey(), true),         // collection_asset (signer)
            AccountMeta::new_readonly(MPL_CORE_ID, false),          // mpl_core_program
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    );
    (ix, collection_kp)
}

fn ix_create_position(
    admin: &Pubkey,
    asset: &Pubkey,
    spread_bps: u16,
    collection: &Pubkey,
) -> Instruction {
    ix_create_position_with_market(admin, asset, spread_bps, collection, None, "navSOL")
}

fn ix_create_position_with_market(
    admin: &Pubkey,
    asset: &Pubkey,
    spread_bps: u16,
    collection: &Pubkey,
    name: Option<&str>,
    market_name: &str,
) -> Instruction {
    let (position_pda, _) =
        Pubkey::find_program_address(&[PositionNFT::SEED, asset.as_ref()], &program_id());
    let (program_pda, _) =
        Pubkey::find_program_address(&[b"authority", asset.as_ref()], &program_id());
    let (config_pda, _) = config_pda();
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);
    let (pp_pda, _) = mayflower::derive_personal_position(&program_pda, &DEFAULT_MARKET_META);
    let (escrow_pda, _) = mayflower::derive_personal_position_escrow(&pp_pda);
    let (log_pda, _) = mayflower::derive_log_account();

    let mut data = sighash("create_position");
    data.extend_from_slice(&spread_bps.to_le_bytes());
    // name: Option<String>
    match name {
        Some(n) => {
            data.push(1); // Some
            data.extend_from_slice(&(n.len() as u32).to_le_bytes());
            data.extend_from_slice(n.as_bytes());
        }
        None => {
            data.push(0); // None
        }
    }
    // market_name: String
    data.extend_from_slice(&(market_name.len() as u32).to_le_bytes());
    data.extend_from_slice(market_name.as_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),                                    // admin
            AccountMeta::new(*asset, true),                                    // admin_asset (signer)
            AccountMeta::new(position_pda, false),                             // position
            AccountMeta::new_readonly(program_pda, false),                     // program_pda
            AccountMeta::new_readonly(config_pda, false),                      // config
            AccountMeta::new(*collection, false),                              // collection
            AccountMeta::new_readonly(mc_pda, false),                          // market_config
            AccountMeta::new_readonly(MPL_CORE_ID, false),                     // mpl_core_program
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),  // system_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),                    // token_program
            AccountMeta::new(pp_pda, false),                                   // mayflower_personal_position
            AccountMeta::new(escrow_pda, false),                               // mayflower_user_shares
            AccountMeta::new_readonly(DEFAULT_MARKET_META, false),             // mayflower_market_meta
            AccountMeta::new_readonly(DEFAULT_NAV_SOL_MINT, false),            // nav_sol_mint
            AccountMeta::new(log_pda, false),                                  // mayflower_log
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),            // mayflower_program
        ],
    )
}

/// Like ix_create_position but with a custom name.
fn ix_create_position_named(
    admin: &Pubkey,
    asset: &Pubkey,
    spread_bps: u16,
    collection: &Pubkey,
    name: &str,
) -> Instruction {
    ix_create_position_with_market(admin, asset, spread_bps, collection, Some(name), "navSOL")
}

fn ix_authorize_key(
    admin: &Pubkey,
    admin_asset: &Pubkey,
    _position_pda: &Pubkey,
    new_asset: &Pubkey,
    target_wallet: &Pubkey,
    role: u8,
    sell_bucket_capacity: u64,
    sell_refill_period_slots: u64,
    borrow_bucket_capacity: u64,
    borrow_refill_period_slots: u64,
    collection: &Pubkey,
) -> Instruction {
    let (pos_pda, _) =
        Pubkey::find_program_address(&[PositionNFT::SEED, admin_asset.as_ref()], &program_id());
    let (key_state_pda, _) =
        Pubkey::find_program_address(&[KeyState::SEED, new_asset.as_ref()], &program_id());
    let (cfg_pda, _) = config_pda();

    let mut data = sighash("authorize_key");
    data.push(role);
    data.extend_from_slice(&sell_bucket_capacity.to_le_bytes());
    data.extend_from_slice(&sell_refill_period_slots.to_le_bytes());
    data.extend_from_slice(&borrow_bucket_capacity.to_le_bytes());
    data.extend_from_slice(&borrow_refill_period_slots.to_le_bytes());
    // name: Option<String> = None
    data.push(0);

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),                   // admin
            AccountMeta::new_readonly(*admin_asset, false),    // admin_key_asset
            AccountMeta::new(pos_pda, false),                  // position (mut for last_admin_activity)
            AccountMeta::new(*new_asset, true),                // new_key_asset (signer)
            AccountMeta::new_readonly(*target_wallet, false),  // target_wallet
            AccountMeta::new(key_state_pda, false),            // key_state (init)
            AccountMeta::new_readonly(cfg_pda, false),         // config
            AccountMeta::new(*collection, false),              // collection
            AccountMeta::new_readonly(MPL_CORE_ID, false),     // mpl_core_program
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

fn ix_revoke_key(
    admin: &Pubkey,
    admin_asset: &Pubkey,
    target_asset: &Pubkey,
    target_key_state: &Pubkey,
    collection: &Pubkey,
) -> Instruction {
    let (pos_pda, _) =
        Pubkey::find_program_address(&[PositionNFT::SEED, admin_asset.as_ref()], &program_id());
    let (cfg_pda, _) = config_pda();

    Instruction::new_with_bytes(
        program_id(),
        &sighash("revoke_key"),
        vec![
            AccountMeta::new(*admin, true),                   // admin
            AccountMeta::new_readonly(*admin_asset, false),    // admin_key_asset
            AccountMeta::new(pos_pda, false),                  // position (mut for last_admin_activity)
            AccountMeta::new(*target_asset, false),            // target_asset
            AccountMeta::new(*target_key_state, false),        // target_key_state
            AccountMeta::new_readonly(cfg_pda, false),         // config
            AccountMeta::new(*collection, false),              // collection
            AccountMeta::new_readonly(MPL_CORE_ID, false),     // mpl_core_program
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

// ---------------------------------------------------------------------------
// Financial instruction builders (with MarketConfig)
// ---------------------------------------------------------------------------

/// Compute the common Mayflower derived addresses for a given position's admin_asset.
fn mayflower_addrs(admin_asset: &Pubkey) -> (Pubkey, Pubkey, Pubkey, Pubkey, Pubkey, Pubkey) {
    let (program_pda, _) = Pubkey::find_program_address(
        &[b"authority", admin_asset.as_ref()],
        &program_id(),
    );
    let (pp_pda, _) = mayflower::derive_personal_position(&program_pda, &DEFAULT_MARKET_META);
    let (escrow_pda, _) = mayflower::derive_personal_position_escrow(&pp_pda);
    let (log_pda, _) = mayflower::derive_log_account();
    let wsol_ata = get_ata(&program_pda, &DEFAULT_WSOL_MINT);
    let nav_sol_ata = get_ata(&program_pda, &DEFAULT_NAV_SOL_MINT);
    (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata)
}

fn ix_buy(
    signer: &Pubkey,
    key_asset: &Pubkey,
    position_pda: &Pubkey,
    admin_asset: &Pubkey,
    amount: u64,
) -> Instruction {
    let (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("buy");
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage check)

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),                          // signer
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(*position_pda, false),                   // position
            AccountMeta::new_readonly(config_pda().0, false),         // config
            AccountMeta::new_readonly(mc_pda, false),                 // market_config
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false), // system_program
            AccountMeta::new(program_pda, false),                      // program_pda (mut for CPI)
            AccountMeta::new(pp_pda, false),                          // personal_position
            AccountMeta::new(escrow_pda, false),                      // user_shares
            AccountMeta::new(nav_sol_ata, false),                     // user_nav_sol_ata
            AccountMeta::new(wsol_ata, false),                        // user_wsol_ata
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),       // tenant
            AccountMeta::new_readonly(DEFAULT_MARKET_GROUP, false),   // market_group
            AccountMeta::new_readonly(DEFAULT_MARKET_META, false),    // market_meta
            AccountMeta::new(DEFAULT_MAYFLOWER_MARKET, false),        // mayflower_market
            AccountMeta::new(DEFAULT_NAV_SOL_MINT, false),            // nav_sol_mint
            AccountMeta::new(DEFAULT_MARKET_BASE_VAULT, false),       // market_base_vault
            AccountMeta::new(DEFAULT_MARKET_NAV_VAULT, false),        // market_nav_vault
            AccountMeta::new(DEFAULT_FEE_VAULT, false),               // fee_vault
            AccountMeta::new_readonly(DEFAULT_WSOL_MINT, false),      // wsol_mint
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),   // mayflower_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),           // token_program
            AccountMeta::new(log_pda, false),                         // log_account
        ],
    )
}

fn ix_withdraw(
    admin: &Pubkey,
    key_asset: &Pubkey,
    key_state: Option<&Pubkey>,
    position_pda: &Pubkey,
    admin_asset: &Pubkey,
    amount: u64,
) -> Instruction {
    let (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);
    // For Option<Account>, pass program ID as "None" sentinel
    let key_state_key = key_state.copied().unwrap_or(program_id());

    let mut data = sighash("withdraw");
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage check)

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(key_state_key, false),                   // key_state (Option)
            AccountMeta::new(*position_pda, false),
            AccountMeta::new_readonly(config_pda().0, false),          // config
            AccountMeta::new_readonly(mc_pda, false),                  // market_config
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            AccountMeta::new(program_pda, false),
            AccountMeta::new(pp_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(nav_sol_ata, false),
            AccountMeta::new(wsol_ata, false),
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),
            AccountMeta::new_readonly(DEFAULT_MARKET_GROUP, false),
            AccountMeta::new_readonly(DEFAULT_MARKET_META, false),
            AccountMeta::new(DEFAULT_MAYFLOWER_MARKET, false),
            AccountMeta::new(DEFAULT_NAV_SOL_MINT, false),
            AccountMeta::new(DEFAULT_MARKET_BASE_VAULT, false),
            AccountMeta::new(DEFAULT_MARKET_NAV_VAULT, false),
            AccountMeta::new(DEFAULT_FEE_VAULT, false),
            AccountMeta::new_readonly(DEFAULT_WSOL_MINT, false),
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new(log_pda, false),
        ],
    )
}

fn ix_borrow(
    admin: &Pubkey,
    key_asset: &Pubkey,
    key_state: Option<&Pubkey>,
    position_pda: &Pubkey,
    admin_asset: &Pubkey,
    amount: u64,
) -> Instruction {
    let (program_pda, pp_pda, _escrow_pda, log_pda, wsol_ata, _nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);
    let key_state_key = key_state.copied().unwrap_or(program_id());

    let mut data = sighash("borrow");
    data.extend_from_slice(&amount.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(key_state_key, false),                   // key_state (Option)
            AccountMeta::new(*position_pda, false),
            AccountMeta::new_readonly(config_pda().0, false),          // config
            AccountMeta::new_readonly(mc_pda, false),                  // market_config
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            AccountMeta::new(program_pda, false),
            AccountMeta::new(pp_pda, false),
            AccountMeta::new(wsol_ata, false),              // user_base_token_ata
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),
            AccountMeta::new_readonly(DEFAULT_MARKET_GROUP, false),
            AccountMeta::new_readonly(DEFAULT_MARKET_META, false),
            AccountMeta::new(DEFAULT_MARKET_BASE_VAULT, false),
            AccountMeta::new(DEFAULT_MARKET_NAV_VAULT, false),
            AccountMeta::new(DEFAULT_FEE_VAULT, false),
            AccountMeta::new_readonly(DEFAULT_WSOL_MINT, false),
            AccountMeta::new(DEFAULT_MAYFLOWER_MARKET, false),
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new(log_pda, false),
        ],
    )
}

fn ix_repay(
    signer: &Pubkey,
    key_asset: &Pubkey,
    position_pda: &Pubkey,
    admin_asset: &Pubkey,
    amount: u64,
) -> Instruction {
    let (program_pda, pp_pda, _escrow_pda, log_pda, wsol_ata, _nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("repay");
    data.extend_from_slice(&amount.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(*position_pda, false),
            AccountMeta::new_readonly(config_pda().0, false),          // config
            AccountMeta::new_readonly(mc_pda, false),                  // market_config
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            AccountMeta::new(program_pda, false),
            AccountMeta::new(pp_pda, false),
            AccountMeta::new(wsol_ata, false),              // user_base_token_ata
            AccountMeta::new_readonly(DEFAULT_MARKET_META, false),
            AccountMeta::new(DEFAULT_MARKET_BASE_VAULT, false),
            AccountMeta::new_readonly(DEFAULT_WSOL_MINT, false),
            AccountMeta::new(DEFAULT_MAYFLOWER_MARKET, false),
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new(log_pda, false),
        ],
    )
}

fn ix_reinvest(
    signer: &Pubkey,
    key_asset: &Pubkey,
    position_pda: &Pubkey,
    admin_asset: &Pubkey,
) -> Instruction {
    let (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("reinvest");
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage check)

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(*position_pda, false),
            AccountMeta::new_readonly(mc_pda, false),                  // market_config
            AccountMeta::new_readonly(config_pda().0, false),          // config
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            AccountMeta::new(program_pda, false),
            AccountMeta::new(pp_pda, false),                          // personal_position
            AccountMeta::new(escrow_pda, false),                      // user_shares
            AccountMeta::new(nav_sol_ata, false),                     // user_nav_sol_ata
            AccountMeta::new(wsol_ata, false),                        // user_wsol_ata
            AccountMeta::new(wsol_ata, false),                        // user_base_token_ata (same)
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),
            AccountMeta::new_readonly(DEFAULT_MARKET_GROUP, false),
            AccountMeta::new_readonly(DEFAULT_MARKET_META, false),
            AccountMeta::new(DEFAULT_MAYFLOWER_MARKET, false),
            AccountMeta::new(DEFAULT_NAV_SOL_MINT, false),
            AccountMeta::new(DEFAULT_MARKET_BASE_VAULT, false),
            AccountMeta::new(DEFAULT_MARKET_NAV_VAULT, false),
            AccountMeta::new(DEFAULT_FEE_VAULT, false),
            AccountMeta::new_readonly(DEFAULT_WSOL_MINT, false),
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new(log_pda, false),
        ],
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn position_pda(asset: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[PositionNFT::SEED, asset.as_ref()], &program_id())
}

fn key_state_pda(asset: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[KeyState::SEED, asset.as_ref()],
        &program_id(),
    )
}

fn read_position(svm: &LiteSVM, pda: &Pubkey) -> PositionNFT {
    let account = svm.get_account(pda).unwrap();
    PositionNFT::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn read_key_state(svm: &LiteSVM, pda: &Pubkey) -> KeyState {
    let account = svm.get_account(pda).unwrap();
    KeyState::try_deserialize(&mut account.data.as_slice()).unwrap()
}

/// Advance LiteSVM clock unix_timestamp by the given number of seconds.
fn advance_clock(svm: &mut LiteSVM, secs: i64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += secs;
    clock.slot += (secs as u64) * 2; // approximate slot advancement
    svm.set_sysvar(&clock);
}

fn read_market_config(svm: &LiteSVM, pda: &Pubkey) -> MarketConfig {
    let account = svm.get_account(pda).unwrap();
    MarketConfig::try_deserialize(&mut account.data.as_slice()).unwrap()
}

/// Full setup: init protocol + create market config + create position + authorize operator/depositor/keeper.
/// Also plants Mayflower position stubs for CPI.
struct TestHarness {
    admin: Keypair,
    admin_asset: Keypair,
    position_pda: Pubkey,
    collection: Pubkey,

    operator: Keypair,
    operator_asset: Pubkey,
    operator_key_state: Pubkey,

    depositor: Keypair,
    depositor_asset: Pubkey,
    depositor_key_state: Pubkey,

    keeper: Keypair,
    keeper_asset: Pubkey,
    keeper_key_state: Pubkey,

    #[allow(dead_code)]
    outsider: Keypair,
}

fn full_setup(svm: &mut LiteSVM) -> TestHarness {
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();

    // Init protocol
    send_tx(svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    // Create collection
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();

    // Create MarketConfig for default navSOL market
    send_tx(svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    // Create position (plant Mayflower stubs first so the CPI has accounts to write to)
    let admin_asset = Keypair::new();
    plant_position_stubs(svm, &admin_asset.pubkey());
    send_tx(
        svm,
        &[ix_create_position(
            &admin.pubkey(),
            &admin_asset.pubkey(),
            500,
            &collection,
        )],
        &[&admin, &admin_asset],
    )
    .unwrap();

    let (pos_pda, _) = position_pda(&admin_asset.pubkey());

    // Authorize operator
    let operator = Keypair::new();
    svm.airdrop(&operator.pubkey(), 5_000_000_000).unwrap();
    let op_asset = Keypair::new();
    send_tx(
        svm,
        &[ix_authorize_key(
            &admin.pubkey(),
            &admin_asset.pubkey(),
            &pos_pda,
            &op_asset.pubkey(),
            &operator.pubkey(),
            PRESET_OPERATOR,
            0, 0, 0, 0,
            &collection,
        )],
        &[&admin, &op_asset],
    )
    .unwrap();
    let (op_ks, _) = key_state_pda(&op_asset.pubkey());

    // Authorize depositor
    let depositor = Keypair::new();
    svm.airdrop(&depositor.pubkey(), 5_000_000_000).unwrap();
    let dep_asset = Keypair::new();
    send_tx(
        svm,
        &[ix_authorize_key(
            &admin.pubkey(),
            &admin_asset.pubkey(),
            &pos_pda,
            &dep_asset.pubkey(),
            &depositor.pubkey(),
            PRESET_DEPOSITOR,
            0, 0, 0, 0,
            &collection,
        )],
        &[&admin, &dep_asset],
    )
    .unwrap();
    let (dep_ks, _) = key_state_pda(&dep_asset.pubkey());

    // Authorize keeper
    let keeper = Keypair::new();
    svm.airdrop(&keeper.pubkey(), 5_000_000_000).unwrap();
    let keep_asset = Keypair::new();
    send_tx(
        svm,
        &[ix_authorize_key(
            &admin.pubkey(),
            &admin_asset.pubkey(),
            &pos_pda,
            &keep_asset.pubkey(),
            &keeper.pubkey(),
            PRESET_KEEPER,
            0, 0, 0, 0,
            &collection,
        )],
        &[&admin, &keep_asset],
    )
    .unwrap();
    let (keep_ks, _) = key_state_pda(&keep_asset.pubkey());

    // Outsider with no key
    let outsider = Keypair::new();
    svm.airdrop(&outsider.pubkey(), 5_000_000_000).unwrap();

    TestHarness {
        admin,
        admin_asset,
        position_pda: pos_pda,
        collection,
        operator,
        operator_asset: op_asset.pubkey(),
        operator_key_state: op_ks,
        depositor,
        depositor_asset: dep_asset.pubkey(),
        depositor_key_state: dep_ks,
        keeper,
        keeper_asset: keep_asset.pubkey(),
        keeper_key_state: keep_ks,
        outsider,
    }
}

// ===========================================================================
// MarketConfig tests
// ===========================================================================

#[test]
fn test_create_market_config() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();
    send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);
    let mc = read_market_config(&svm, &mc_pda);
    assert_eq!(mc.nav_mint, DEFAULT_NAV_SOL_MINT);
    assert_eq!(mc.base_mint, DEFAULT_WSOL_MINT);
    assert_eq!(mc.market_group, DEFAULT_MARKET_GROUP);
    assert_eq!(mc.market_meta, DEFAULT_MARKET_META);
    assert_eq!(mc.mayflower_market, DEFAULT_MAYFLOWER_MARKET);
    assert_eq!(mc.market_base_vault, DEFAULT_MARKET_BASE_VAULT);
    assert_eq!(mc.market_nav_vault, DEFAULT_MARKET_NAV_VAULT);
    assert_eq!(mc.fee_vault, DEFAULT_FEE_VAULT);
}

#[test]
fn test_create_market_config_non_admin_denied() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let non_admin = Keypair::new();
    svm.airdrop(&non_admin.pubkey(), 5_000_000_000).unwrap();
    assert!(send_tx(&mut svm, &[ix_create_market_config(&non_admin.pubkey())], &[&non_admin]).is_err());
}

// ===========================================================================
// #36: Transfer admin tests
// ===========================================================================

fn ix_transfer_admin(admin: &Pubkey, new_admin: &Pubkey) -> Instruction {
    let (config_pda, _) =
        Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id());

    let mut data = sighash("transfer_admin");
    data.extend_from_slice(new_admin.as_ref());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new_readonly(*admin, true),
            AccountMeta::new(config_pda, false),
        ],
    )
}

fn ix_accept_admin(new_admin: &Pubkey) -> Instruction {
    let (config_pda, _) =
        Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id());

    Instruction::new_with_bytes(
        program_id(),
        &sighash("accept_admin"),
        vec![
            AccountMeta::new_readonly(*new_admin, true),
            AccountMeta::new(config_pda, false),
        ],
    )
}

fn read_protocol_config(svm: &LiteSVM) -> ProtocolConfig {
    let (config_pda, _) =
        Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id());
    let account = svm.get_account(&config_pda).unwrap();
    ProtocolConfig::try_deserialize(&mut account.data.as_slice()).unwrap()
}

#[test]
fn test_transfer_admin_ok() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let new_admin = Keypair::new();
    svm.airdrop(&new_admin.pubkey(), 5_000_000_000).unwrap();

    // Step 1: Nominate
    let ix = ix_transfer_admin(&admin.pubkey(), &new_admin.pubkey());
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    let config = read_protocol_config(&svm);
    assert_eq!(config.admin, admin.pubkey()); // still old admin
    assert_eq!(config.pending_admin, new_admin.pubkey());

    // Step 2: Accept
    send_tx(&mut svm, &[ix_accept_admin(&new_admin.pubkey())], &[&new_admin]).unwrap();

    let config = read_protocol_config(&svm);
    assert_eq!(config.admin, new_admin.pubkey());
    assert_eq!(config.pending_admin, Pubkey::default());
}

#[test]
fn test_transfer_admin_non_admin_denied() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let non_admin = Keypair::new();
    svm.airdrop(&non_admin.pubkey(), 5_000_000_000).unwrap();

    // Non-admin cannot nominate
    let ix = ix_transfer_admin(&non_admin.pubkey(), &non_admin.pubkey());
    assert!(send_tx(&mut svm, &[ix], &[&non_admin]).is_err());

    let config = read_protocol_config(&svm);
    assert_eq!(config.admin, admin.pubkey());
    assert_eq!(config.pending_admin, Pubkey::default());
}

#[test]
fn test_transfer_admin_old_admin_rejected_new_admin_works() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let new_admin = Keypair::new();
    svm.airdrop(&new_admin.pubkey(), 5_000_000_000).unwrap();

    // Nominate new admin
    let ix = ix_transfer_admin(&admin.pubkey(), &new_admin.pubkey());
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    // New admin accepts
    send_tx(&mut svm, &[ix_accept_admin(&new_admin.pubkey())], &[&new_admin]).unwrap();

    // Old admin can no longer create market config
    assert!(send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).is_err());

    // New admin can
    send_tx(&mut svm, &[ix_create_market_config(&new_admin.pubkey())], &[&new_admin]).unwrap();

    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);
    let mc = read_market_config(&svm, &mc_pda);
    assert_eq!(mc.nav_mint, DEFAULT_NAV_SOL_MINT);
}

// ===========================================================================
// #16: Permission matrix tests
// ===========================================================================

// ---- Buy: Admin, Operator, Depositor allowed; Keeper denied ----

#[test]
fn test_buy_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 1_000_000);
}

#[test]
fn test_buy_operator_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_buy(
        &h.operator.pubkey(), &h.operator_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 500_000,
    );
    send_tx(&mut svm, &[ix], &[&h.operator]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 500_000);
}

#[test]
fn test_buy_depositor_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_buy(
        &h.depositor.pubkey(), &h.depositor_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 250_000,
    );
    send_tx(&mut svm, &[ix], &[&h.depositor]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 250_000);
}

#[test]
fn test_buy_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_buy(
        &h.keeper.pubkey(), &h.keeper_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Withdraw: Admin only ----

#[test]
fn test_withdraw_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 500_000,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 500_000);
}

#[test]
fn test_withdraw_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.operator.pubkey(), &h.operator_asset,
        Some(&h.operator_key_state), &h.position_pda, &h.admin_asset.pubkey(), 500_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator]).is_err());
}

#[test]
fn test_withdraw_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.depositor.pubkey(), &h.depositor_asset,
        Some(&h.depositor_key_state), &h.position_pda, &h.admin_asset.pubkey(), 500_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

#[test]
fn test_withdraw_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.keeper.pubkey(), &h.keeper_asset,
        Some(&h.keeper_key_state), &h.position_pda, &h.admin_asset.pubkey(), 500_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Borrow: Admin only ----

#[test]
fn test_borrow_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 1_000_000);
}

#[test]
fn test_borrow_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_borrow(
        &h.operator.pubkey(), &h.operator_asset,
        Some(&h.operator_key_state), &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator]).is_err());
}

#[test]
fn test_borrow_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_borrow(
        &h.depositor.pubkey(), &h.depositor_asset,
        Some(&h.depositor_key_state), &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

#[test]
fn test_borrow_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_borrow(
        &h.keeper.pubkey(), &h.keeper_asset,
        Some(&h.keeper_key_state), &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Repay: Admin, Operator, Depositor allowed; Keeper denied ----

#[test]
fn test_repay_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 500_000,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 500_000);
}

#[test]
fn test_repay_operator_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.operator.pubkey(), &h.operator_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 500_000,
    );
    send_tx(&mut svm, &[ix], &[&h.operator]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 500_000);
}

#[test]
fn test_repay_depositor_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.depositor.pubkey(), &h.depositor_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 300_000,
    );
    send_tx(&mut svm, &[ix], &[&h.depositor]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 700_000);
}

#[test]
fn test_repay_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.keeper.pubkey(), &h.keeper_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Reinvest: Admin, Operator, Keeper allowed; Depositor denied ----

#[test]
fn test_reinvest_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_reinvest(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(),
    );
    // Reinvest with zero capacity should succeed (early return)
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
}

#[test]
fn test_reinvest_operator_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_reinvest(
        &h.operator.pubkey(), &h.operator_asset,
        &h.position_pda, &h.admin_asset.pubkey(),
    );
    send_tx(&mut svm, &[ix], &[&h.operator]).unwrap();
}

#[test]
fn test_reinvest_keeper_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_reinvest(
        &h.keeper.pubkey(), &h.keeper_asset,
        &h.position_pda, &h.admin_asset.pubkey(),
    );
    send_tx(&mut svm, &[ix], &[&h.keeper]).unwrap();
}

#[test]
fn test_reinvest_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_reinvest(
        &h.depositor.pubkey(), &h.depositor_asset,
        &h.position_pda, &h.admin_asset.pubkey(),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

// ---- Authorize/Revoke: Admin only ----

#[test]
fn test_authorize_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_asset = Keypair::new();
    let random_wallet = Keypair::new();
    let ix = ix_authorize_key(
        &h.operator.pubkey(),
        &h.operator_asset,
        &h.position_pda,
        &new_asset.pubkey(),
        &random_wallet.pubkey(),
        PRESET_DEPOSITOR,
        0, 0, 0, 0,
        &h.collection,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator, &new_asset]).is_err());
}

#[test]
fn test_revoke_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_revoke_key(
        &h.operator.pubkey(),
        &h.operator_asset,
        &h.keeper_asset,
        &h.keeper_key_state,
        &h.collection,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator]).is_err());
}

// ---- Cannot create second admin ----

#[test]
fn test_cannot_create_second_admin() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_asset = Keypair::new();
    let random_wallet = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &new_asset.pubkey(),
        &random_wallet.pubkey(),
        PRESET_ADMIN, // Has PERM_MANAGE_KEYS -> rejected
        0, 0, 0, 0,
        &h.collection,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin, &new_asset]).is_err());
}

// ---- Cannot revoke admin key ----

#[test]
fn test_cannot_revoke_admin_key() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    // Admin key has no KeyState, so we can't pass a valid target_key_state.
    // This test verifies that the program rejects revoking the admin asset.
    // We use the operator's key_state as a dummy (will fail for other reasons too).
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.admin_asset.pubkey(),
        &h.operator_key_state, // dummy - doesn't matter, should fail first on admin check
        &h.collection,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

// ---- Wrong position key rejected ----

#[test]
fn test_wrong_position_key_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Create a second position (plant stubs before create_position for Mayflower CPI)
    let admin2 = Keypair::new();
    svm.airdrop(&admin2.pubkey(), 10_000_000_000).unwrap();
    let asset2 = Keypair::new();
    plant_position_stubs(&mut svm, &asset2.pubkey());
    send_tx(
        &mut svm,
        &[ix_create_position(&admin2.pubkey(), &asset2.pubkey(), 300, &h.collection)],
        &[&admin2, &asset2],
    )
    .unwrap();
    let (pos2, _) = position_pda(&asset2.pubkey());

    // Try to use admin1's key on position2 — should fail (update_authority mismatch)
    let ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &pos2, &asset2.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

// ===========================================================================
// #17: NFT lifecycle tests
// ===========================================================================

#[test]
fn test_init_protocol() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let (config_pda, _) =
        Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id());
    let account = svm.get_account(&config_pda).unwrap();
    let config = ProtocolConfig::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(config.admin, admin.pubkey());
}

#[test]
fn test_create_position_and_admin_asset() {
    let (mut svm, _) = setup();
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    // Create collection
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(&mut svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();

    // Create MarketConfig for default navSOL market
    send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    let asset_kp = Keypair::new();
    plant_position_stubs(&mut svm, &asset_kp.pubkey());
    send_tx(
        &mut svm,
        &[ix_create_position(&admin.pubkey(), &asset_kp.pubkey(), 750, &collection)],
        &[&admin, &asset_kp],
    )
    .unwrap();

    // Check position
    let (pos_pda, _) = position_pda(&asset_kp.pubkey());
    let pos = read_position(&svm, &pos_pda);
    assert_eq!(pos.authority_seed, asset_kp.pubkey());
    assert_eq!(pos.max_reinvest_spread_bps, 750);
    assert_eq!(pos.deposited_nav, 0);
    assert_eq!(pos.user_debt, 0);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);
    assert_eq!(pos.market_config, mc_pda);

    // Check MPL-Core asset exists (owned by MPL-Core program)
    let asset_account = svm.get_account(&asset_kp.pubkey());
    assert!(asset_account.is_some(), "MPL-Core asset should exist");
    let asset_data = asset_account.unwrap();
    assert_eq!(asset_data.owner, MPL_CORE_ID, "asset should be owned by MPL-Core program");
}

#[test]
fn test_authorize_and_revoke_key() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Verify operator KeyState exists
    let ks = read_key_state(&svm, &h.operator_key_state);
    assert_eq!(ks.asset, h.operator_asset);

    // Verify operator holds MPL-Core asset
    let asset_account = svm.get_account(&h.operator_asset);
    assert!(asset_account.is_some(), "operator asset should exist");

    // Revoke the operator key (burns asset via PermanentBurnDelegate)
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.operator_asset,
        &h.operator_key_state,
        &h.collection,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    // KeyState account should be closed
    assert!(svm.get_account(&h.operator_key_state).is_none());

    // After MPL-Core burn, account is resized to 1 byte with Key::Uninitialized (0)
    let asset_after = svm.get_account(&h.operator_asset);
    assert!(
        asset_after.is_none()
            || asset_after.as_ref().unwrap().data.is_empty()
            || asset_after.as_ref().unwrap().data.len() == 1,
        "asset should be burned"
    );

    // Operator can no longer buy
    let buy_ix = ix_buy(
        &h.operator.pubkey(), &h.operator_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[buy_ix], &[&h.operator]).is_err());
}

#[test]
fn test_multiple_keys_per_position() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // All delegated keys should have KeyState PDAs
    let op_ks = read_key_state(&svm, &h.operator_key_state);
    assert_eq!(op_ks.asset, h.operator_asset);

    let dep_ks = read_key_state(&svm, &h.depositor_key_state);
    assert_eq!(dep_ks.asset, h.depositor_asset);

    let keep_ks = read_key_state(&svm, &h.keeper_key_state);
    assert_eq!(keep_ks.asset, h.keeper_asset);
}

#[test]
fn test_zero_permissions_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_asset = Keypair::new();
    let target = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &new_asset.pubkey(),
        &target.pubkey(),
        0x00, // Zero permissions
        0, 0, 0, 0,
        &h.collection,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin, &new_asset]).is_err());
}

#[test]
fn test_limited_sell_requires_rate_params() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_asset = Keypair::new();
    let target = Keypair::new();
    // PERM_LIMITED_SELL with zero capacity should fail
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &new_asset.pubkey(),
        &target.pubkey(),
        PERM_LIMITED_SELL,
        0, 0, 0, 0, // missing capacity/refill
        &h.collection,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin, &new_asset]).is_err());
}

#[test]
fn test_manage_keys_permission_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_asset = Keypair::new();
    let target = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &new_asset.pubkey(),
        &target.pubkey(),
        PERM_MANAGE_KEYS, // PERM_MANAGE_KEYS alone -> rejected
        0, 0, 0, 0,
        &h.collection,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin, &new_asset]).is_err());
}

#[test]
fn test_custom_bitmask_buy_reinvest() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Create a custom key with only buy + reinvest permissions
    let custom_user = Keypair::new();
    svm.airdrop(&custom_user.pubkey(), 5_000_000_000).unwrap();
    let custom_asset = Keypair::new();
    let custom_perms = PERM_BUY | PERM_REINVEST; // 0x11
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &custom_asset.pubkey(),
            &custom_user.pubkey(),
            custom_perms,
            0, 0, 0, 0,
            &h.collection,
        )],
        &[&h.admin, &custom_asset],
    )
    .unwrap();

    // Custom key can buy
    let buy_ix = ix_buy(
        &custom_user.pubkey(), &custom_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&custom_user]).unwrap();

    // Custom key can reinvest
    let reinvest_ix = ix_reinvest(
        &custom_user.pubkey(), &custom_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(),
    );
    send_tx(&mut svm, &[reinvest_ix], &[&custom_user]).unwrap();

    // Custom key cannot sell (withdraw)
    let pos = read_position(&svm, &h.position_pda);
    if pos.deposited_nav > 0 {
        let (custom_ks, _) = key_state_pda(&custom_asset.pubkey());
        let sell_ix = ix_withdraw(
            &custom_user.pubkey(), &custom_asset.pubkey(),
            Some(&custom_ks), &h.position_pda, &h.admin_asset.pubkey(), 50_000,
        );
        assert!(send_tx(&mut svm, &[sell_ix], &[&custom_user]).is_err());
    }

    // Custom key cannot borrow
    let (custom_ks, _) = key_state_pda(&custom_asset.pubkey());
    let borrow_ix = ix_borrow(
        &custom_user.pubkey(), &custom_asset.pubkey(),
        Some(&custom_ks), &h.position_pda, &h.admin_asset.pubkey(), 50_000,
    );
    assert!(send_tx(&mut svm, &[borrow_ix], &[&custom_user]).is_err());
}

// ---- Accounting edge cases ----

#[test]
fn test_buy_zero_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 0,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

#[test]
fn test_withdraw_more_than_deposited_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 1_000_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 2_000_000_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

#[test]
fn test_repay_more_than_debt_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 1_000_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 2_000_000_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

// ===========================================================================
// #18: Theft recovery scenario tests
// ===========================================================================

#[test]
fn test_theft_recovery_operator_key_stolen() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Revoke compromised operator key
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.operator_asset,
        &h.operator_key_state,
        &h.collection,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    assert!(svm.get_account(&h.operator_key_state).is_none());

    // Old operator key can no longer buy
    let buy_ix = ix_buy(
        &h.operator.pubkey(), &h.operator_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[buy_ix], &[&h.operator]).is_err());

    // Issue new key to operator
    let new_op_asset = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &new_op_asset.pubkey(),
        &h.operator.pubkey(),
        PRESET_OPERATOR,
        0, 0, 0, 0,
        &h.collection,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &new_op_asset]).unwrap();

    // New key works
    let buy_ix = ix_buy(
        &h.operator.pubkey(), &new_op_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.operator]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 100_000);
}

#[test]
fn test_theft_recovery_mass_revoke_and_reissue() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    for (asset, ks) in [
        (h.operator_asset, h.operator_key_state),
        (h.depositor_asset, h.depositor_key_state),
        (h.keeper_asset, h.keeper_key_state),
    ] {
        let ix = ix_revoke_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &asset,
            &ks,
            &h.collection,
        );
        send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    }

    assert!(svm.get_account(&h.operator_key_state).is_none());
    assert!(svm.get_account(&h.depositor_key_state).is_none());
    assert!(svm.get_account(&h.keeper_key_state).is_none());

    // Admin can still buy
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 500_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();

    // Reissue new operator
    let new_operator = Keypair::new();
    svm.airdrop(&new_operator.pubkey(), 5_000_000_000).unwrap();
    let new_op_asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &new_op_asset.pubkey(),
            &new_operator.pubkey(),
            1,
            0, 0, 0, 0,
            &h.collection,
        )],
        &[&h.admin, &new_op_asset],
    )
    .unwrap();

    let buy_ix = ix_buy(
        &new_operator.pubkey(), &new_op_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 200_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&new_operator]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 700_000);
}

#[test]
fn test_attacker_cannot_use_others_key_asset() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 5_000_000_000).unwrap();

    // Attacker tries to use operator's key asset — fails (owner check)
    let ix = ix_buy(
        &attacker.pubkey(), &h.operator_asset,
        &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&attacker]).is_err());
}

#[test]
fn test_privilege_escalation_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Depositor tries to use admin's key for withdraw — fails (owner check)
    let ix = ix_withdraw(
        &h.depositor.pubkey(), &h.admin_asset.pubkey(),
        None, &h.position_pda, &h.admin_asset.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

// ===========================================================================
// Rate-limited permissions tests
// ===========================================================================

#[test]
fn test_authorize_limited_sell_key() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();
    let asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &asset.pubkey(),
            &user.pubkey(),
            PERM_BUY | PERM_LIMITED_SELL,
            1_000_000_000, // 1 SOL capacity
            500_000,       // ~500k slots refill period
            0, 0,
            &h.collection,
        )],
        &[&h.admin, &asset],
    )
    .unwrap();
    let (ks_pda, _) = key_state_pda(&asset.pubkey());
    let ks = read_key_state(&svm, &ks_pda);
    assert_eq!(ks.sell_bucket.capacity, 1_000_000_000);
    assert_eq!(ks.sell_bucket.refill_period, 500_000);
    assert_eq!(ks.sell_bucket.level, 1_000_000_000); // starts full
}

#[test]
fn test_limited_sell_within_capacity() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 5_000_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();

    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();
    let asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &asset.pubkey(),
            &user.pubkey(),
            PERM_LIMITED_SELL,
            2_000_000_000, // 2 SOL capacity
            1_000_000,
            0, 0,
            &h.collection,
        )],
        &[&h.admin, &asset],
    )
    .unwrap();
    let (ks_pda, _) = key_state_pda(&asset.pubkey());

    let sell_ix = ix_withdraw(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 1_000_000_000,
    );
    send_tx(&mut svm, &[sell_ix], &[&user]).unwrap();

    let ks = read_key_state(&svm, &ks_pda);
    assert_eq!(ks.sell_bucket.level, 1_000_000_000); // 2B - 1B = 1B remaining
}

#[test]
fn test_limited_sell_exceeds_capacity() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 5_000_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();

    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();
    let asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &asset.pubkey(),
            &user.pubkey(),
            PERM_LIMITED_SELL,
            500_000_000, // 0.5 SOL capacity
            1_000_000,
            0, 0,
            &h.collection,
        )],
        &[&h.admin, &asset],
    )
    .unwrap();
    let (ks_pda, _) = key_state_pda(&asset.pubkey());

    let sell_ix = ix_withdraw(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 1_000_000_000,
    );
    assert!(send_tx(&mut svm, &[sell_ix], &[&user]).is_err());
}

#[test]
fn test_limited_sell_drains_then_rejects() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 5_000_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();

    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();
    let asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &asset.pubkey(),
            &user.pubkey(),
            PERM_LIMITED_SELL,
            1_000_000_000, // 1 SOL capacity
            1_000_000,
            0, 0,
            &h.collection,
        )],
        &[&h.admin, &asset],
    )
    .unwrap();
    let (ks_pda, _) = key_state_pda(&asset.pubkey());

    // First sell: 600M (succeeds)
    let sell_ix = ix_withdraw(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 600_000_000,
    );
    send_tx(&mut svm, &[sell_ix], &[&user]).unwrap();

    // Second sell: 600M (exceeds remaining 400M)
    let sell_ix2 = ix_withdraw(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 600_000_000,
    );
    assert!(send_tx(&mut svm, &[sell_ix2], &[&user]).is_err());
}

#[test]
fn test_limited_sell_refills_over_slots() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 5_000_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();

    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();
    let asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &asset.pubkey(),
            &user.pubkey(),
            PERM_LIMITED_SELL,
            1_000_000_000, // 1 SOL capacity
            1_000_000,     // 1M slots for full refill
            0, 0,
            &h.collection,
        )],
        &[&h.admin, &asset],
    )
    .unwrap();
    let (ks_pda, _) = key_state_pda(&asset.pubkey());

    // Drain the bucket completely
    let sell_ix = ix_withdraw(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 1_000_000_000,
    );
    send_tx(&mut svm, &[sell_ix], &[&user]).unwrap();

    // Verify bucket is empty
    let ks = read_key_state(&svm, &ks_pda);
    assert_eq!(ks.sell_bucket.level, 0);

    // Advance clock by half the refill period (500k slots)
    let mut clock: solana_sdk::clock::Clock = svm.get_sysvar();
    clock.slot += 500_000;
    svm.set_sysvar(&clock);

    // Should be able to sell ~0.5 SOL now (half refilled)
    let sell_ix2 = ix_withdraw(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 400_000_000,
    );
    send_tx(&mut svm, &[sell_ix2], &[&user]).unwrap();
}

#[test]
fn test_unlimited_sell_overrides_limited() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_asset.pubkey(),
        &h.position_pda, &h.admin_asset.pubkey(), 5_000_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();

    // Key with both PERM_SELL and PERM_LIMITED_SELL — rate limit is skipped
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();
    let asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &asset.pubkey(),
            &user.pubkey(),
            PERM_SELL | PERM_LIMITED_SELL,
            100, // tiny capacity — would block if enforced
            1,
            0, 0,
            &h.collection,
        )],
        &[&h.admin, &asset],
    )
    .unwrap();
    let (ks_pda, _) = key_state_pda(&asset.pubkey());

    // Sell way more than the tiny bucket — should succeed because PERM_SELL overrides
    let sell_ix = ix_withdraw(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 2_000_000_000,
    );
    send_tx(&mut svm, &[sell_ix], &[&user]).unwrap();
}

#[test]
fn test_non_limited_rejects_rate_params() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_asset = Keypair::new();
    let target = Keypair::new();
    // PERM_BUY with non-zero capacity should fail
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &new_asset.pubkey(),
        &target.pubkey(),
        PERM_BUY,
        1_000_000, 500_000, 0, 0,
        &h.collection,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin, &new_asset]).is_err());
}

#[test]
fn test_limited_borrow_within_capacity() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();
    let asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &asset.pubkey(),
            &user.pubkey(),
            PERM_LIMITED_BORROW,
            0, 0,              // sell: zero (no limited sell)
            2_000_000_000,     // borrow capacity
            1_000_000,         // borrow refill
            &h.collection,
        )],
        &[&h.admin, &asset],
    )
    .unwrap();
    let (ks_pda, _) = key_state_pda(&asset.pubkey());

    let borrow_ix = ix_borrow(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 1_000_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&user]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 1_000_000_000);
}

#[test]
fn test_limited_borrow_exceeds_capacity() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();
    let asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &asset.pubkey(),
            &user.pubkey(),
            PERM_LIMITED_BORROW,
            0, 0,
            500_000_000, // 0.5 SOL borrow capacity
            1_000_000,
            &h.collection,
        )],
        &[&h.admin, &asset],
    )
    .unwrap();
    let (ks_pda, _) = key_state_pda(&asset.pubkey());

    let borrow_ix = ix_borrow(
        &user.pubkey(), &asset.pubkey(),
        Some(&ks_pda), &h.position_pda, &h.admin_asset.pubkey(), 1_000_000_000,
    );
    assert!(send_tx(&mut svm, &[borrow_ix], &[&user]).is_err());
}

// ===========================================================================
// MPL-Core asset tests
// ===========================================================================

#[test]
fn test_create_position_creates_mpl_core_asset() {
    let (mut svm, _) = setup();
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    // Create collection
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(&mut svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();

    // Create MarketConfig for default navSOL market
    send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    let asset_kp = Keypair::new();
    plant_position_stubs(&mut svm, &asset_kp.pubkey());
    send_tx(
        &mut svm,
        &[ix_create_position(&admin.pubkey(), &asset_kp.pubkey(), 500, &collection)],
        &[&admin, &asset_kp],
    )
    .unwrap();

    // MPL-Core asset should exist
    let asset_account = svm.get_account(&asset_kp.pubkey());
    assert!(asset_account.is_some(), "MPL-Core asset should exist");
    let asset_data = asset_account.unwrap();
    assert_eq!(asset_data.owner, MPL_CORE_ID, "asset should be owned by MPL-Core");
    // First byte should be Key::AssetV1 = 1
    assert_eq!(asset_data.data[0], 1, "first byte should be AssetV1 discriminator");
}

#[test]
fn test_authorize_key_creates_mpl_core_asset() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Operator's MPL-Core asset should exist
    let asset_account = svm.get_account(&h.operator_asset);
    assert!(asset_account.is_some(), "operator MPL-Core asset should exist");
    let asset_data = asset_account.unwrap();
    assert_eq!(asset_data.owner, MPL_CORE_ID, "asset should be owned by MPL-Core");
    assert_eq!(asset_data.data[0], 1, "first byte should be AssetV1 discriminator");
}

#[test]
fn test_revoke_burns_mpl_core_asset() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Authorize a key to the admin's own wallet
    let extra_asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_asset.pubkey(),
            &h.position_pda,
            &extra_asset.pubkey(),
            &h.admin.pubkey(),
            PRESET_OPERATOR,
            0, 0, 0, 0,
            &h.collection,
        )],
        &[&h.admin, &extra_asset],
    )
    .unwrap();
    let (extra_ks, _) = key_state_pda(&extra_asset.pubkey());

    // Verify asset exists before revoke
    assert!(svm.get_account(&extra_asset.pubkey()).is_some());
    assert!(svm.get_account(&extra_ks).is_some());

    // Revoke and burn via PermanentBurnDelegate
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &extra_asset.pubkey(),
        &extra_ks,
        &h.collection,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    // KeyState should be closed
    assert!(svm.get_account(&extra_ks).is_none());

    // After MPL-Core burn, account is resized to 1 byte with Key::Uninitialized (0)
    let asset_after = svm.get_account(&extra_asset.pubkey());
    assert!(
        asset_after.is_none()
            || asset_after.as_ref().unwrap().data.is_empty()
            || asset_after.as_ref().unwrap().data.len() == 1,
        "MPL-Core asset should be burned"
    );
}

// ===========================================================================
// Collection tests
// ===========================================================================

#[test]
fn test_create_collection() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(&mut svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();

    let config = read_protocol_config(&svm);
    assert_eq!(config.collection, coll_kp.pubkey());

    // Collection asset should exist (owned by MPL-Core program)
    let coll_account = svm.get_account(&coll_kp.pubkey());
    assert!(coll_account.is_some(), "collection asset should exist");
    assert_eq!(coll_account.unwrap().owner, MPL_CORE_ID);
}

#[test]
fn test_create_collection_twice_rejected() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(&mut svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();

    // Second attempt should fail
    let (coll_ix2, coll_kp2) = ix_create_collection(&admin.pubkey());
    assert!(send_tx(&mut svm, &[coll_ix2], &[&admin, &coll_kp2]).is_err());
}

#[test]
fn test_create_position_without_collection_rejected() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    // Try to create position without creating collection first
    let asset_kp = Keypair::new();
    // Use a dummy pubkey for collection since none exists
    let dummy_collection = Pubkey::new_unique();
    let result = send_tx(
        &mut svm,
        &[ix_create_position(&admin.pubkey(), &asset_kp.pubkey(), 500, &dummy_collection)],
        &[&admin, &asset_kp],
    );
    assert!(result.is_err());
}

/// Extract the name string from an MPL-Core AssetV1 account's raw data.
/// Layout: 1 (key) + 32 (owner) + 1 (update_authority tag) + 32 (authority pubkey) + 4 (name len) + name bytes
fn extract_asset_name(data: &[u8]) -> String {
    let name_len_offset = 1 + 32 + 1 + 32; // = 66
    let name_len = u32::from_le_bytes(data[name_len_offset..name_len_offset + 4].try_into().unwrap()) as usize;
    let name_start = name_len_offset + 4;
    String::from_utf8(data[name_start..name_start + name_len].to_vec()).unwrap()
}

#[test]
fn test_create_position_custom_name() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(&mut svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();
    send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    let asset_kp = Keypair::new();
    plant_position_stubs(&mut svm, &asset_kp.pubkey());
    send_tx(
        &mut svm,
        &[ix_create_position_named(
            &admin.pubkey(),
            &asset_kp.pubkey(),
            500,
            &collection,
            "My Vault",
        )],
        &[&admin, &asset_kp],
    )
    .unwrap();

    // Verify the MPL-Core asset has the base name + suffix
    let asset_account = svm.get_account(&asset_kp.pubkey()).expect("asset should exist");
    assert_eq!(asset_account.owner, MPL_CORE_ID);
    let name = extract_asset_name(&asset_account.data);
    assert_eq!(name, "H\u{00e4}rdig Admin Key - My Vault");
}

/// Extract attribute values from an MPL-Core asset account using fetch_plugin.
/// Returns the list of (key, value) pairs.
fn extract_asset_attributes(account: &Account) -> Vec<(String, String)> {
    use mpl_core::{accounts::BaseAssetV1, fetch_plugin, types::{Attributes, PluginType}};
    let key = Pubkey::new_unique(); // dummy key, not used in fetch_plugin
    let mut lamports = account.lamports;
    let mut data = account.data.clone();
    let account_info = solana_sdk::account_info::AccountInfo::new(
        &key,
        false,
        false,
        &mut lamports,
        &mut data,
        &account.owner,
        false,
        0,
    );
    let (_, attributes, _) = fetch_plugin::<BaseAssetV1, Attributes>(
        &account_info,
        PluginType::Attributes,
    )
    .expect("failed to fetch Attributes plugin");
    attributes
        .attribute_list
        .into_iter()
        .map(|a| (a.key, a.value))
        .collect()
}

/// Find a specific attribute value by key from the attribute list.
fn find_attribute<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

#[test]
fn test_admin_key_has_market_attribute() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(&mut svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();
    send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    let asset_kp = Keypair::new();
    plant_position_stubs(&mut svm, &asset_kp.pubkey());
    send_tx(
        &mut svm,
        &[ix_create_position_with_market(
            &admin.pubkey(),
            &asset_kp.pubkey(),
            500,
            &collection,
            Some("My Vault"),
            "navSOL",
        )],
        &[&admin, &asset_kp],
    )
    .unwrap();

    // Read admin asset's attributes
    let asset_account = svm.get_account(&asset_kp.pubkey()).expect("asset should exist");
    let attrs = extract_asset_attributes(&asset_account);

    // Verify "market" attribute is set
    assert_eq!(
        find_attribute(&attrs, "market"),
        Some("navSOL"),
        "admin key should have market=navSOL attribute"
    );

    // Verify "position" attribute points to itself (admin asset)
    assert_eq!(
        find_attribute(&attrs, "position"),
        Some(asset_kp.pubkey().to_string().as_str()),
        "admin key position attribute should be its own pubkey"
    );
}

#[test]
fn test_admin_key_market_attribute_custom_value() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(&mut svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();
    send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    let asset_kp = Keypair::new();
    plant_position_stubs(&mut svm, &asset_kp.pubkey());
    send_tx(
        &mut svm,
        &[ix_create_position_with_market(
            &admin.pubkey(),
            &asset_kp.pubkey(),
            500,
            &collection,
            None,
            "navETH",
        )],
        &[&admin, &asset_kp],
    )
    .unwrap();

    let asset_account = svm.get_account(&asset_kp.pubkey()).expect("asset should exist");
    let attrs = extract_asset_attributes(&asset_account);
    assert_eq!(
        find_attribute(&attrs, "market"),
        Some("navETH"),
        "admin key should have market=navETH attribute"
    );
}

#[test]
fn test_delegated_key_has_market_and_position_attributes() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(&mut svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();
    send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    // Create position with market_name = "navSOL" and custom name
    let admin_asset = Keypair::new();
    plant_position_stubs(&mut svm, &admin_asset.pubkey());
    send_tx(
        &mut svm,
        &[ix_create_position_with_market(
            &admin.pubkey(),
            &admin_asset.pubkey(),
            500,
            &collection,
            Some("My Vault"),
            "navSOL",
        )],
        &[&admin, &admin_asset],
    )
    .unwrap();

    let (pos_pda, _) = position_pda(&admin_asset.pubkey());

    // Authorize an operator key
    let operator = Keypair::new();
    svm.airdrop(&operator.pubkey(), 5_000_000_000).unwrap();
    let op_asset = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &admin.pubkey(),
            &admin_asset.pubkey(),
            &pos_pda,
            &op_asset.pubkey(),
            &operator.pubkey(),
            PRESET_OPERATOR,
            0, 0, 0, 0,
            &collection,
        )],
        &[&admin, &op_asset],
    )
    .unwrap();

    // Read the delegated key's attributes
    let op_account = svm.get_account(&op_asset.pubkey()).expect("operator asset should exist");
    let attrs = extract_asset_attributes(&op_account);

    // Verify "market" attribute is inherited from admin key
    assert_eq!(
        find_attribute(&attrs, "market"),
        Some("navSOL"),
        "delegated key should have market=navSOL attribute from admin"
    );

    // Verify "position_name" attribute matches admin asset's name
    assert_eq!(
        find_attribute(&attrs, "position_name"),
        Some("H\u{00e4}rdig Admin Key - My Vault"),
        "delegated key should have position_name matching admin asset name"
    );

    // Verify "position" attribute still points to admin asset pubkey
    assert_eq!(
        find_attribute(&attrs, "position"),
        Some(admin_asset.pubkey().to_string().as_str()),
        "delegated key position attribute should point to admin asset"
    );
}

// ===========================================================================
// Recovery instruction builders
// ===========================================================================

fn ix_heartbeat(
    admin: &Pubkey,
    admin_key_asset: &Pubkey,
    position_pda: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        program_id(),
        &sighash("heartbeat"),
        vec![
            AccountMeta::new_readonly(*admin, true),
            AccountMeta::new_readonly(*admin_key_asset, false),
            AccountMeta::new(*position_pda, false),
            AccountMeta::new_readonly(config_pda().0, false),          // config
        ],
    )
}

fn ix_configure_recovery(
    admin: &Pubkey,
    admin_key_asset: &Pubkey,
    position_pda: &Pubkey,
    recovery_asset: &Pubkey,
    target_wallet: &Pubkey,
    old_recovery_asset: Option<&Pubkey>,
    collection: &Pubkey,
    lockout_secs: i64,
    lock_config: bool,
    name: Option<&str>,
) -> Instruction {
    let (cfg_pda, _) = config_pda();

    let mut data = sighash("configure_recovery");
    data.extend_from_slice(&lockout_secs.to_le_bytes());
    data.push(if lock_config { 1 } else { 0 });
    // name: Option<String>
    match name {
        Some(n) => {
            data.push(1); // Some
            data.extend_from_slice(&(n.len() as u32).to_le_bytes());
            data.extend_from_slice(n.as_bytes());
        }
        None => {
            data.push(0); // None
        }
    }

    // For Option<UncheckedAccount>, Anchor expects the account always present.
    // Pass program_id() as the "None" sentinel.
    let old_recovery = old_recovery_asset.copied().unwrap_or(program_id());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),                    // admin
            AccountMeta::new_readonly(*admin_key_asset, false), // admin_key_asset
            AccountMeta::new(*position_pda, false),             // position
            AccountMeta::new(*recovery_asset, true),            // recovery_asset (signer)
            AccountMeta::new_readonly(*target_wallet, false),   // target_wallet
            AccountMeta::new(old_recovery, false),              // old_recovery_asset (Option)
            AccountMeta::new_readonly(cfg_pda, false),          // config
            AccountMeta::new(*collection, false),               // collection
            AccountMeta::new_readonly(MPL_CORE_ID, false),      // mpl_core_program
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

fn ix_execute_recovery(
    recovery_holder: &Pubkey,
    recovery_key_asset: &Pubkey,
    position_pda: &Pubkey,
    old_admin_asset: &Pubkey,
    new_admin_asset: &Pubkey,
    collection: &Pubkey,
) -> Instruction {
    let (cfg_pda, _) = config_pda();

    Instruction::new_with_bytes(
        program_id(),
        &sighash("execute_recovery"),
        vec![
            AccountMeta::new(*recovery_holder, true),           // recovery_holder
            AccountMeta::new(*recovery_key_asset, false),          // recovery_key_asset (mut for burn)
            AccountMeta::new(*position_pda, false),              // position
            AccountMeta::new(*old_admin_asset, false),           // old_admin_asset
            AccountMeta::new(*new_admin_asset, true),            // new_admin_asset (signer)
            AccountMeta::new_readonly(cfg_pda, false),           // config
            AccountMeta::new(*collection, false),                // collection
            AccountMeta::new_readonly(MPL_CORE_ID, false),       // mpl_core_program
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

// ===========================================================================
// Recovery tests
// ===========================================================================

#[test]
fn test_heartbeat_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let pos_before = read_position(&svm, &h.position_pda);

    // Advance clock so we can detect the timestamp change
    advance_clock(&mut svm, 10);

    let ix = ix_heartbeat(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    let pos_after = read_position(&svm, &h.position_pda);
    assert!(
        pos_after.last_admin_activity >= pos_before.last_admin_activity,
        "heartbeat should update last_admin_activity"
    );
}

#[test]
fn test_heartbeat_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Operator does NOT have PERM_MANAGE_KEYS, so heartbeat should fail
    let ix = ix_heartbeat(
        &h.operator.pubkey(),
        &h.operator_asset,
        &h.position_pda,
    );
    assert!(
        send_tx(&mut svm, &[ix], &[&h.operator]).is_err(),
        "operator should not be able to heartbeat"
    );
}

#[test]
fn test_heartbeat_outsider_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Outsider has no key at all — use a random pubkey as the asset
    let fake_asset = Pubkey::new_unique();
    let ix = ix_heartbeat(
        &h.outsider.pubkey(),
        &fake_asset,
        &h.position_pda,
    );
    assert!(
        send_tx(&mut svm, &[ix], &[&h.outsider]).is_err(),
        "outsider should not be able to heartbeat"
    );
}

#[test]
fn test_configure_recovery_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_wallet = Keypair::new();
    svm.airdrop(&recovery_wallet.pubkey(), 5_000_000_000).unwrap();
    let recovery_asset = Keypair::new();

    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset.pubkey(),
        &recovery_wallet.pubkey(),
        None,
        &h.collection,
        86400, // 1 day lockout
        false,
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.recovery_asset, recovery_asset.pubkey());
    assert_eq!(pos.recovery_lockout_secs, 86400);
    assert!(!pos.recovery_config_locked);

    // Verify recovery key NFT was created with correct attributes
    let rec_account = svm.get_account(&recovery_asset.pubkey()).expect("recovery asset should exist");
    let attrs = extract_asset_attributes(&rec_account);
    assert_eq!(find_attribute(&attrs, "permissions"), Some("0"));
    assert_eq!(find_attribute(&attrs, "recovery"), Some("true"));
    assert_eq!(
        find_attribute(&attrs, "position"),
        Some(h.admin_asset.pubkey().to_string().as_str()),
        "recovery key should be bound to admin asset (authority_seed)"
    );
}

#[test]
fn test_configure_recovery_with_lock() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_wallet = Keypair::new();
    svm.airdrop(&recovery_wallet.pubkey(), 5_000_000_000).unwrap();
    let recovery_asset = Keypair::new();

    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset.pubkey(),
        &recovery_wallet.pubkey(),
        None,
        &h.collection,
        604800, // 7 day lockout
        true,   // lock config
        Some("backup"),
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.recovery_asset, recovery_asset.pubkey());
    assert_eq!(pos.recovery_lockout_secs, 604800);
    assert!(pos.recovery_config_locked);
}

#[test]
fn test_configure_recovery_locked_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_wallet = Keypair::new();
    svm.airdrop(&recovery_wallet.pubkey(), 5_000_000_000).unwrap();

    // First: configure with lock_config = true
    let recovery_asset1 = Keypair::new();
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset1.pubkey(),
        &recovery_wallet.pubkey(),
        None,
        &h.collection,
        86400,
        true, // lock it
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset1]).unwrap();

    // Second: try to reconfigure — should fail because config is locked
    let recovery_asset2 = Keypair::new();
    let ix2 = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset2.pubkey(),
        &recovery_wallet.pubkey(),
        Some(&recovery_asset1.pubkey()),
        &h.collection,
        172800,
        false,
        None,
    );
    assert!(
        send_tx(&mut svm, &[ix2], &[&h.admin, &recovery_asset2]).is_err(),
        "should not be able to reconfigure recovery when locked"
    );

    // Verify original config unchanged
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.recovery_asset, recovery_asset1.pubkey());
    assert_eq!(pos.recovery_lockout_secs, 86400);
}

#[test]
fn test_configure_recovery_replace_requires_old_asset() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_wallet = Keypair::new();
    svm.airdrop(&recovery_wallet.pubkey(), 5_000_000_000).unwrap();

    // First: configure a recovery key
    let recovery_asset1 = Keypair::new();
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset1.pubkey(),
        &recovery_wallet.pubkey(),
        None,
        &h.collection,
        86400,
        false,
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset1]).unwrap();

    // Second: try to replace WITHOUT providing old_recovery_asset — should fail
    let recovery_asset2 = Keypair::new();
    let ix2 = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset2.pubkey(),
        &recovery_wallet.pubkey(),
        None, // omitting old_recovery_asset
        &h.collection,
        172800,
        false,
        None,
    );
    assert!(
        send_tx(&mut svm, &[ix2], &[&h.admin, &recovery_asset2]).is_err(),
        "should not be able to replace recovery key without providing old_recovery_asset"
    );

    // Verify original config unchanged
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.recovery_asset, recovery_asset1.pubkey());
    assert_eq!(pos.recovery_lockout_secs, 86400);
}

#[test]
fn test_configure_recovery_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_wallet = Keypair::new();
    svm.airdrop(&recovery_wallet.pubkey(), 5_000_000_000).unwrap();
    let recovery_asset = Keypair::new();

    // Operator does not have PERM_MANAGE_KEYS
    let ix = ix_configure_recovery(
        &h.operator.pubkey(),
        &h.operator_asset,
        &h.position_pda,
        &recovery_asset.pubkey(),
        &recovery_wallet.pubkey(),
        None,
        &h.collection,
        86400,
        false,
        None,
    );
    assert!(
        send_tx(&mut svm, &[ix], &[&h.operator, &recovery_asset]).is_err(),
        "operator should not be able to configure recovery"
    );
}

#[test]
fn test_execute_recovery_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_holder = Keypair::new();
    svm.airdrop(&recovery_holder.pubkey(), 5_000_000_000).unwrap();
    let recovery_asset = Keypair::new();

    // Configure recovery with 1-second lockout (minimum for testing)
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset.pubkey(),
        &recovery_holder.pubkey(),
        None,
        &h.collection,
        1, // 1 second lockout
        false,
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset]).unwrap();

    // Advance time past the lockout period
    advance_clock(&mut svm, 100);

    // Execute recovery
    let new_admin_asset = Keypair::new();
    let ix = ix_execute_recovery(
        &recovery_holder.pubkey(),
        &recovery_asset.pubkey(),
        &h.position_pda,
        &h.admin_asset.pubkey(),
        &new_admin_asset.pubkey(),
        &h.collection,
    );
    send_tx(&mut svm, &[ix], &[&recovery_holder, &new_admin_asset]).unwrap();

    // Verify position state
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.current_admin_asset, new_admin_asset.pubkey(),
        "current_admin_asset should be updated to new admin");
    assert_eq!(pos.authority_seed, h.admin_asset.pubkey(),
        "authority_seed should remain unchanged");
    assert_eq!(pos.recovery_asset, Pubkey::default(),
        "recovery_asset should be cleared after recovery");
    assert_eq!(pos.recovery_lockout_secs, 0,
        "recovery_lockout_secs should be cleared");
    assert!(!pos.recovery_config_locked,
        "recovery_config_locked should be cleared");

    // Verify new admin key NFT has correct attributes
    let new_admin_account = svm.get_account(&new_admin_asset.pubkey())
        .expect("new admin asset should exist");
    let attrs = extract_asset_attributes(&new_admin_account);
    assert_eq!(find_attribute(&attrs, "permissions"), Some("63"),
        "new admin key should have full admin permissions");
    assert_eq!(
        find_attribute(&attrs, "position"),
        Some(h.admin_asset.pubkey().to_string().as_str()),
        "new admin key should be bound to original authority_seed"
    );

    // Verify old admin key was burned (MPL-Core may leave a 1-byte stub)
    let old_admin_after = svm.get_account(&h.admin_asset.pubkey());
    assert!(
        old_admin_after.is_none()
            || old_admin_after.as_ref().unwrap().data.is_empty()
            || old_admin_after.as_ref().unwrap().data.len() == 1,
        "old admin asset should be burned"
    );

    // Verify recovery key was burned
    let recovery_after = svm.get_account(&recovery_asset.pubkey());
    assert!(
        recovery_after.is_none()
            || recovery_after.as_ref().unwrap().data.is_empty()
            || recovery_after.as_ref().unwrap().data.len() == 1,
        "recovery asset should be burned"
    );
}

#[test]
fn test_execute_recovery_too_early() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_holder = Keypair::new();
    svm.airdrop(&recovery_holder.pubkey(), 5_000_000_000).unwrap();
    let recovery_asset = Keypair::new();

    // Configure recovery with a long lockout
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset.pubkey(),
        &recovery_holder.pubkey(),
        None,
        &h.collection,
        999999, // very long lockout
        false,
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset]).unwrap();

    // Try to execute recovery immediately — should fail
    let new_admin_asset = Keypair::new();
    let ix = ix_execute_recovery(
        &recovery_holder.pubkey(),
        &recovery_asset.pubkey(),
        &h.position_pda,
        &h.admin_asset.pubkey(),
        &new_admin_asset.pubkey(),
        &h.collection,
    );
    assert!(
        send_tx(&mut svm, &[ix], &[&recovery_holder, &new_admin_asset]).is_err(),
        "execute_recovery should fail before lockout expires"
    );

    // Verify position unchanged
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.current_admin_asset, h.admin_asset.pubkey());
    assert_eq!(pos.recovery_asset, recovery_asset.pubkey());
}

#[test]
fn test_execute_recovery_heartbeat_resets_lockout() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_holder = Keypair::new();
    svm.airdrop(&recovery_holder.pubkey(), 5_000_000_000).unwrap();
    let recovery_asset = Keypair::new();

    // Configure recovery with 1-second lockout
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset.pubkey(),
        &recovery_holder.pubkey(),
        None,
        &h.collection,
        1,
        false,
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset]).unwrap();

    // Advance time past lockout
    advance_clock(&mut svm, 100);

    // Admin sends heartbeat — resets last_admin_activity
    let hb = ix_heartbeat(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
    );
    send_tx(&mut svm, &[hb], &[&h.admin]).unwrap();

    // Now try to execute recovery — should fail because heartbeat reset the timer
    let new_admin_asset = Keypair::new();
    let ix = ix_execute_recovery(
        &recovery_holder.pubkey(),
        &recovery_asset.pubkey(),
        &h.position_pda,
        &h.admin_asset.pubkey(),
        &new_admin_asset.pubkey(),
        &h.collection,
    );
    assert!(
        send_tx(&mut svm, &[ix], &[&recovery_holder, &new_admin_asset]).is_err(),
        "execute_recovery should fail after heartbeat resets lockout"
    );
}

#[test]
fn test_execute_recovery_no_recovery_configured() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Try to execute recovery without configuring one
    let recovery_holder = Keypair::new();
    svm.airdrop(&recovery_holder.pubkey(), 5_000_000_000).unwrap();
    let fake_recovery = Pubkey::new_unique();
    let new_admin_asset = Keypair::new();

    let ix = ix_execute_recovery(
        &recovery_holder.pubkey(),
        &fake_recovery,
        &h.position_pda,
        &h.admin_asset.pubkey(),
        &new_admin_asset.pubkey(),
        &h.collection,
    );
    assert!(
        send_tx(&mut svm, &[ix], &[&recovery_holder, &new_admin_asset]).is_err(),
        "execute_recovery should fail when no recovery is configured"
    );
}

#[test]
fn test_delegated_keys_survive_recovery() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_holder = Keypair::new();
    svm.airdrop(&recovery_holder.pubkey(), 5_000_000_000).unwrap();
    let recovery_asset = Keypair::new();

    // Configure recovery
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset.pubkey(),
        &recovery_holder.pubkey(),
        None,
        &h.collection,
        1,
        false,
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset]).unwrap();

    // Advance time past lockout
    advance_clock(&mut svm, 100);

    // Execute recovery
    let new_admin_asset = Keypair::new();
    let ix = ix_execute_recovery(
        &recovery_holder.pubkey(),
        &recovery_asset.pubkey(),
        &h.position_pda,
        &h.admin_asset.pubkey(),
        &new_admin_asset.pubkey(),
        &h.collection,
    );
    send_tx(&mut svm, &[ix], &[&recovery_holder, &new_admin_asset]).unwrap();

    // Verify operator key state still exists
    let op_ks = read_key_state(&svm, &h.operator_key_state);
    assert_eq!(op_ks.asset, h.operator_asset,
        "operator key_state should still exist after recovery");

    // Verify operator can still buy (authority_seed unchanged, key binding intact)
    let ix = ix_buy(
        &h.operator.pubkey(),
        &h.operator_asset,
        &h.position_pda,
        &h.admin_asset.pubkey(), // authority_seed is still the original admin asset
        500_000,
    );
    send_tx(&mut svm, &[ix], &[&h.operator]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 500_000, "operator buy should still work after recovery");
}

#[test]
fn test_replace_recovery_key() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let recovery_wallet = Keypair::new();
    svm.airdrop(&recovery_wallet.pubkey(), 5_000_000_000).unwrap();

    // First recovery config
    let recovery_asset1 = Keypair::new();
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset1.pubkey(),
        &recovery_wallet.pubkey(),
        None,
        &h.collection,
        86400,
        false, // don't lock
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset1]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.recovery_asset, recovery_asset1.pubkey());

    // Replace with new recovery key (passing old one to burn)
    let recovery_asset2 = Keypair::new();
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset2.pubkey(),
        &recovery_wallet.pubkey(),
        Some(&recovery_asset1.pubkey()),
        &h.collection,
        172800, // new lockout
        false,
        Some("backup-v2"),
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset2]).unwrap();

    // Verify replacement
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.recovery_asset, recovery_asset2.pubkey());
    assert_eq!(pos.recovery_lockout_secs, 172800);

    // Verify old recovery key was burned (MPL-Core may leave a 1-byte stub)
    let old_recovery = svm.get_account(&recovery_asset1.pubkey());
    assert!(
        old_recovery.is_none()
            || old_recovery.as_ref().unwrap().data.is_empty()
            || old_recovery.as_ref().unwrap().data.len() == 1,
        "old recovery asset should be burned when replaced"
    );
}

// ── authorize_key and revoke_key update last_admin_activity ──────────

#[test]
fn test_authorize_key_updates_last_admin_activity() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let pos_before = read_position(&svm, &h.position_pda);

    // Advance clock so we can detect the timestamp change
    advance_clock(&mut svm, 100);

    // Authorize a new key
    let new_asset = Keypair::new();
    let target = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &new_asset.pubkey(),
        &target.pubkey(),
        PRESET_DEPOSITOR, // Depositor
        0, 0, 0, 0,
        &h.collection,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &new_asset]).unwrap();

    let pos_after = read_position(&svm, &h.position_pda);
    assert!(
        pos_after.last_admin_activity > pos_before.last_admin_activity,
        "authorize_key should update last_admin_activity"
    );
}

#[test]
fn test_revoke_key_updates_last_admin_activity() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let pos_before = read_position(&svm, &h.position_pda);

    // Advance clock so we can detect the timestamp change
    advance_clock(&mut svm, 100);

    // Revoke the operator key
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.operator_asset,
        &h.operator_key_state,
        &h.collection,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    let pos_after = read_position(&svm, &h.position_pda);
    assert!(
        pos_after.last_admin_activity > pos_before.last_admin_activity,
        "revoke_key should update last_admin_activity"
    );
}

// ── configure_recovery lockout upper bound ──────────────────────────

#[test]
fn test_configure_recovery_lockout_too_large() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let recovery_wallet = Keypair::new();
    let recovery_asset = Keypair::new();

    // Attempt to configure with i64::MAX lockout (exceeds 10-year cap)
    let ix = ix_configure_recovery(
        &h.admin.pubkey(),
        &h.admin_asset.pubkey(),
        &h.position_pda,
        &recovery_asset.pubkey(),
        &recovery_wallet.pubkey(),
        None,
        &h.collection,
        i64::MAX, // exceeds 10-year max
        false,
        Some("test"),
    );
    let result = send_tx(&mut svm, &[ix], &[&h.admin, &recovery_asset]);
    assert!(result.is_err(), "lockout exceeding 10 years should be rejected");
}

// ===========================================================================
// Promo tests
// ===========================================================================

// ---------------------------------------------------------------------------
// Promo instruction builders
// ---------------------------------------------------------------------------

fn promo_pda(authority_seed: &Pubkey, name_suffix: &str) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[PromoConfig::SEED, authority_seed.as_ref(), name_suffix.as_bytes()],
        &program_id(),
    )
}

fn claim_receipt_pda(promo: &Pubkey, claimer: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[ClaimReceipt::SEED, promo.as_ref(), claimer.as_ref()],
        &program_id(),
    )
}

fn read_promo_config(svm: &LiteSVM, pda: &Pubkey) -> PromoConfig {
    let account = svm.get_account(pda).unwrap();
    PromoConfig::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn read_claim_receipt(svm: &LiteSVM, pda: &Pubkey) -> ClaimReceipt {
    let account = svm.get_account(pda).unwrap();
    ClaimReceipt::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn ix_create_promo(
    admin: &Pubkey,
    admin_asset: &Pubkey,
    name_suffix: &str,
    permissions: u8,
    borrow_capacity: u64,
    borrow_refill_period: u64,
    sell_capacity: u64,
    sell_refill_period: u64,
    min_deposit_lamports: u64,
    max_claims: u32,
    image_uri: &str,
    market_name: &str,
) -> Instruction {
    let (pos_pda, _) = position_pda(admin_asset);
    let position = read_position_seed(admin_asset);
    let (promo, _) = promo_pda(&position, name_suffix);

    // Anchor discriminator + Borsh-serialized args
    // name_suffix is the first arg (#[instruction(name_suffix: String)])
    let mut data = sighash("create_promo");
    // String: 4-byte LE length + bytes
    data.extend_from_slice(&(name_suffix.len() as u32).to_le_bytes());
    data.extend_from_slice(name_suffix.as_bytes());
    data.push(permissions);
    data.extend_from_slice(&borrow_capacity.to_le_bytes());
    data.extend_from_slice(&borrow_refill_period.to_le_bytes());
    data.extend_from_slice(&sell_capacity.to_le_bytes());
    data.extend_from_slice(&sell_refill_period.to_le_bytes());
    data.extend_from_slice(&min_deposit_lamports.to_le_bytes());
    data.extend_from_slice(&max_claims.to_le_bytes());
    // image_uri: String
    data.extend_from_slice(&(image_uri.len() as u32).to_le_bytes());
    data.extend_from_slice(image_uri.as_bytes());
    // market_name: String
    data.extend_from_slice(&(market_name.len() as u32).to_le_bytes());
    data.extend_from_slice(market_name.as_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),                                    // admin
            AccountMeta::new_readonly(*admin_asset, false),                    // admin_key_asset
            AccountMeta::new_readonly(pos_pda, false),                         // position
            AccountMeta::new(promo, false),                                    // promo (init)
            AccountMeta::new_readonly(config_pda().0, false),                  // config
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),  // system_program
        ],
    )
}

/// Helper: get the authority_seed from a position created with the given admin_asset.
/// This is just the admin_asset pubkey (authority_seed = first admin asset pubkey).
fn read_position_seed(admin_asset: &Pubkey) -> Pubkey {
    // authority_seed is always the admin_asset pubkey from create_position
    *admin_asset
}

fn ix_update_promo(
    admin: &Pubkey,
    admin_asset: &Pubkey,
    promo_pda_key: &Pubkey,
    active: Option<bool>,
    max_claims: Option<u32>,
) -> Instruction {
    let (pos_pda, _) = position_pda(admin_asset);

    let mut data = sighash("update_promo");
    // Option<bool>
    match active {
        Some(v) => {
            data.push(1); // Some
            data.push(if v { 1 } else { 0 });
        }
        None => data.push(0), // None
    }
    // Option<u32>
    match max_claims {
        Some(v) => {
            data.push(1); // Some
            data.extend_from_slice(&v.to_le_bytes());
        }
        None => data.push(0), // None
    }

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),                   // admin
            AccountMeta::new_readonly(*admin_asset, false),    // admin_key_asset
            AccountMeta::new_readonly(pos_pda, false),         // position
            AccountMeta::new(*promo_pda_key, false),           // promo (mut)
            AccountMeta::new_readonly(config_pda().0, false),  // config
        ],
    )
}

fn ix_claim_promo_key(
    claimer: &Pubkey,
    promo_pda_key: &Pubkey,
    admin_asset: &Pubkey,
    key_asset: &Pubkey,
    collection: &Pubkey,
    amount: u64,
) -> Instruction {
    let (pos_pda, _) = position_pda(admin_asset);
    let (claim_receipt, _) = claim_receipt_pda(promo_pda_key, claimer);
    let (ks_pda, _) = key_state_pda(key_asset);
    let (cfg_pda, _) = config_pda();
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);
    let (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_asset);

    let mut data = sighash("claim_promo_key");
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage check)

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*claimer, true),                                  // claimer
            AccountMeta::new(*promo_pda_key, false),                           // promo (mut)
            AccountMeta::new(claim_receipt, false),                            // claim_receipt (init)
            AccountMeta::new(pos_pda, false),                                  // position (mut)
            AccountMeta::new(*key_asset, true),                                // key_asset (signer)
            AccountMeta::new(ks_pda, false),                                   // key_state (init)
            AccountMeta::new_readonly(cfg_pda, false),                         // config
            AccountMeta::new(*collection, false),                              // collection
            AccountMeta::new_readonly(MPL_CORE_ID, false),                     // mpl_core_program
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),  // system_program
            AccountMeta::new_readonly(mc_pda, false),                          // market_config
            AccountMeta::new(program_pda, false),                              // program_pda
            AccountMeta::new(pp_pda, false),                                   // personal_position
            AccountMeta::new(escrow_pda, false),                               // user_shares
            AccountMeta::new(nav_sol_ata, false),                              // user_nav_sol_ata
            AccountMeta::new(wsol_ata, false),                                 // user_wsol_ata
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),                // tenant
            AccountMeta::new_readonly(DEFAULT_MARKET_GROUP, false),            // market_group
            AccountMeta::new_readonly(DEFAULT_MARKET_META, false),             // market_meta
            AccountMeta::new(DEFAULT_MAYFLOWER_MARKET, false),                 // mayflower_market
            AccountMeta::new(DEFAULT_NAV_SOL_MINT, false),                     // nav_sol_mint
            AccountMeta::new(DEFAULT_MARKET_BASE_VAULT, false),                // market_base_vault
            AccountMeta::new(DEFAULT_MARKET_NAV_VAULT, false),                 // market_nav_vault
            AccountMeta::new(DEFAULT_FEE_VAULT, false),                        // fee_vault
            AccountMeta::new_readonly(DEFAULT_WSOL_MINT, false),               // wsol_mint
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),            // mayflower_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),                    // token_program
            AccountMeta::new(log_pda, false),                                  // log_account
        ],
    )
}

// ---------------------------------------------------------------------------
// Promo test helpers
// ---------------------------------------------------------------------------

/// Minimal setup: init protocol + collection + market config + position.
/// Returns (admin, admin_asset keypair, position_pda, collection pubkey).
fn promo_setup(svm: &mut LiteSVM) -> (Keypair, Keypair, Pubkey, Pubkey) {
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();

    // Init protocol
    send_tx(svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    // Create collection
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_tx(svm, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();

    // Create MarketConfig
    send_tx(svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    // Create position
    let admin_asset = Keypair::new();
    plant_position_stubs(svm, &admin_asset.pubkey());
    send_tx(
        svm,
        &[ix_create_position(
            &admin.pubkey(),
            &admin_asset.pubkey(),
            500,
            &collection,
        )],
        &[&admin, &admin_asset],
    )
    .unwrap();

    let (pos_pda, _) = position_pda(&admin_asset.pubkey());

    (admin, admin_asset, pos_pda, collection)
}

// ---------------------------------------------------------------------------
// 1. test_create_promo — happy path
// ---------------------------------------------------------------------------

#[test]
fn test_create_promo() {
    let (mut svm, _) = setup();
    let (admin, admin_asset, _pos_pda, _collection) = promo_setup(&mut svm);

    let name_suffix = "Test Promo";
    let permissions: u8 = PERM_BUY | PERM_LIMITED_BORROW;
    let borrow_capacity: u64 = 20_000_000;
    let borrow_refill_period: u64 = 1000;
    let sell_capacity: u64 = 0;
    let sell_refill_period: u64 = 0;
    let min_deposit: u64 = 10_000_000;
    let max_claims: u32 = 100;
    let image_uri = "https://example.com/img.png";

    let ix = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        name_suffix,
        permissions,
        borrow_capacity,
        borrow_refill_period,
        sell_capacity,
        sell_refill_period,
        min_deposit,
        max_claims,
        image_uri,
        "navSOL",
    );
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    // Verify the PromoConfig PDA
    let authority_seed = admin_asset.pubkey();
    let (pda, _) = promo_pda(&authority_seed, name_suffix);
    let promo = read_promo_config(&svm, &pda);

    assert_eq!(promo.authority_seed, authority_seed);
    assert_eq!(promo.permissions, permissions);
    assert_eq!(promo.borrow_capacity, borrow_capacity);
    assert_eq!(promo.borrow_refill_period, borrow_refill_period);
    assert_eq!(promo.sell_capacity, sell_capacity);
    assert_eq!(promo.sell_refill_period, sell_refill_period);
    assert_eq!(promo.min_deposit_lamports, min_deposit);
    assert_eq!(promo.max_claims, max_claims);
    assert_eq!(promo.claims_count, 0);
    assert!(promo.active);
    assert_eq!(promo.name_suffix, name_suffix);
    assert_eq!(promo.image_uri, image_uri);
    assert_eq!(promo.market_name, "navSOL");
}

// ---------------------------------------------------------------------------
// 2. test_create_promo_non_admin_rejected
// ---------------------------------------------------------------------------

#[test]
fn test_create_promo_non_admin_rejected() {
    let (mut svm, _) = setup();
    let (_admin, admin_asset, _pos_pda, _collection) = promo_setup(&mut svm);

    // A non-admin wallet tries to create a promo
    let non_admin = Keypair::new();
    svm.airdrop(&non_admin.pubkey(), 5_000_000_000).unwrap();

    // Build the ix with non_admin as signer but still referencing the admin's position
    let name_suffix = "Evil Promo";
    let (pos_pda, _) = position_pda(&admin_asset.pubkey());
    let authority_seed = admin_asset.pubkey();
    let (promo, _) = promo_pda(&authority_seed, name_suffix);

    let mut data = sighash("create_promo");
    data.extend_from_slice(&(name_suffix.len() as u32).to_le_bytes());
    data.extend_from_slice(name_suffix.as_bytes());
    data.push(PERM_BUY);
    data.extend_from_slice(&0u64.to_le_bytes()); // borrow_capacity
    data.extend_from_slice(&0u64.to_le_bytes()); // borrow_refill_period
    data.extend_from_slice(&0u64.to_le_bytes()); // sell_capacity
    data.extend_from_slice(&0u64.to_le_bytes()); // sell_refill_period
    data.extend_from_slice(&0u64.to_le_bytes()); // min_deposit_lamports
    data.extend_from_slice(&10u32.to_le_bytes()); // max_claims
    let uri = "";
    data.extend_from_slice(&(uri.len() as u32).to_le_bytes());
    let mn = "";
    data.extend_from_slice(&(mn.len() as u32).to_le_bytes());

    let ix = Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(non_admin.pubkey(), true),                        // admin (non-admin signer)
            AccountMeta::new_readonly(admin_asset.pubkey(), false),             // admin_key_asset
            AccountMeta::new_readonly(pos_pda, false),                         // position
            AccountMeta::new(promo, false),                                    // promo (init)
            AccountMeta::new_readonly(config_pda().0, false),                  // config
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    );

    let result = send_tx(&mut svm, &[ix], &[&non_admin]);
    assert!(result.is_err(), "non-admin should not be able to create promo");
}

// ---------------------------------------------------------------------------
// 3. test_create_multiple_promos_per_position
// ---------------------------------------------------------------------------

#[test]
fn test_create_multiple_promos_per_position() {
    let (mut svm, _) = setup();
    let (admin, admin_asset, _pos_pda, _collection) = promo_setup(&mut svm);

    let authority_seed = admin_asset.pubkey();

    // Create promo A
    let ix_a = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        "Promo A",
        PERM_BUY,
        0, 0, 0, 0, 5_000_000, 50, "", "navSOL",
    );
    send_tx(&mut svm, &[ix_a], &[&admin]).unwrap();

    // Create promo B
    let ix_b = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        "Promo B",
        PERM_BUY | PERM_LIMITED_BORROW,
        10_000_000, 500, 0, 0, 1_000_000, 200, "https://example.com/b.png", "navSOL",
    );
    send_tx(&mut svm, &[ix_b], &[&admin]).unwrap();

    // Verify promo A
    let (pda_a, _) = promo_pda(&authority_seed, "Promo A");
    let promo_a = read_promo_config(&svm, &pda_a);
    assert_eq!(promo_a.name_suffix, "Promo A");
    assert_eq!(promo_a.permissions, PERM_BUY);
    assert_eq!(promo_a.max_claims, 50);
    assert!(promo_a.active);

    // Verify promo B
    let (pda_b, _) = promo_pda(&authority_seed, "Promo B");
    let promo_b = read_promo_config(&svm, &pda_b);
    assert_eq!(promo_b.name_suffix, "Promo B");
    assert_eq!(promo_b.permissions, PERM_BUY | PERM_LIMITED_BORROW);
    assert_eq!(promo_b.borrow_capacity, 10_000_000);
    assert_eq!(promo_b.max_claims, 200);
    assert_eq!(promo_b.image_uri, "https://example.com/b.png");
}

// ---------------------------------------------------------------------------
// 4. test_update_promo
// ---------------------------------------------------------------------------

#[test]
fn test_update_promo() {
    let (mut svm, _) = setup();
    let (admin, admin_asset, _pos_pda, _collection) = promo_setup(&mut svm);

    let authority_seed = admin_asset.pubkey();
    let name_suffix = "Update Me";

    // Create promo
    let ix = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        name_suffix,
        PERM_BUY,
        0, 0, 0, 0, 0, 100, "", "",
    );
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    let (pda, _) = promo_pda(&authority_seed, name_suffix);

    // Update: set active=false
    let ix_deactivate = ix_update_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        &pda,
        Some(false),
        None,
    );
    send_tx(&mut svm, &[ix_deactivate], &[&admin]).unwrap();
    let promo = read_promo_config(&svm, &pda);
    assert!(!promo.active, "promo should be inactive after update");

    // Update: set max_claims=50
    let ix_max = ix_update_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        &pda,
        None,
        Some(50),
    );
    send_tx(&mut svm, &[ix_max], &[&admin]).unwrap();
    let promo = read_promo_config(&svm, &pda);
    assert_eq!(promo.max_claims, 50);

    // Re-activate
    let ix_reactivate = ix_update_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        &pda,
        Some(true),
        None,
    );
    send_tx(&mut svm, &[ix_reactivate], &[&admin]).unwrap();
    let promo = read_promo_config(&svm, &pda);
    assert!(promo.active, "promo should be active after re-activation");
}

// ---------------------------------------------------------------------------
// 5. test_update_promo_max_below_current_rejected
// ---------------------------------------------------------------------------

#[test]
fn test_update_promo_max_below_current_rejected() {
    let (mut svm, _) = setup();
    let (admin, admin_asset, _pos_pda, collection) = promo_setup(&mut svm);

    let authority_seed = admin_asset.pubkey();
    let name_suffix = "Max Test";

    // Create promo with max_claims=0 (unlimited) to allow claiming
    let ix = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        name_suffix,
        PERM_BUY,
        0, 0, 0, 0, 0, 0, "", "",
    );
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    let (pda, _) = promo_pda(&authority_seed, name_suffix);

    // Claim once to increment claims_count to 1
    let claimer = Keypair::new();
    svm.airdrop(&claimer.pubkey(), 5_000_000_000).unwrap();
    let key_asset = Keypair::new();
    let ix_claim = ix_claim_promo_key(
        &claimer.pubkey(),
        &pda,
        &admin_asset.pubkey(),
        &key_asset.pubkey(),
        &collection,
        0, // min_deposit_lamports = 0 for this promo
    );
    send_tx(&mut svm, &[ix_claim], &[&claimer, &key_asset]).unwrap();

    // Verify claims_count is now 1
    let promo = read_promo_config(&svm, &pda);
    assert_eq!(promo.claims_count, 1);

    // Try to set max_claims=0 — this is unlimited, so it should succeed
    // (0 means unlimited, which is >= any claims_count)
    // Actually, looking at the code: require!(max_claims >= promo.claims_count)
    // 0 < 1, so this should fail. 0 means "unlimited" semantically, but the check is numeric.
    let ix_set_zero = ix_update_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        &pda,
        None,
        Some(0),
    );
    let result = send_tx(&mut svm, &[ix_set_zero], &[&admin]);
    assert!(result.is_err(), "setting max_claims below claims_count should fail");
}

// ---------------------------------------------------------------------------
// 6. test_claim_promo_key — happy path
// ---------------------------------------------------------------------------

#[test]
fn test_claim_promo_key() {
    let (mut svm, _) = setup();
    let (admin, admin_asset, _pos_pda, collection) = promo_setup(&mut svm);

    let authority_seed = admin_asset.pubkey();
    let name_suffix = "Claim Me";
    let permissions: u8 = PERM_BUY | PERM_LIMITED_BORROW;
    let borrow_capacity: u64 = 20_000_000;
    let borrow_refill_period: u64 = 1000;

    // Create promo
    let ix = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        name_suffix,
        permissions,
        borrow_capacity,
        borrow_refill_period,
        0, 0,
        10_000_000,
        100,
        "https://example.com/img.png",
        "navSOL",
    );
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    let (pda, _) = promo_pda(&authority_seed, name_suffix);

    // Claimer claims a key
    let claimer = Keypair::new();
    svm.airdrop(&claimer.pubkey(), 5_000_000_000).unwrap();
    let key_asset = Keypair::new();

    let ix_claim = ix_claim_promo_key(
        &claimer.pubkey(),
        &pda,
        &admin_asset.pubkey(),
        &key_asset.pubkey(),
        &collection,
        10_000_000, // amount = min_deposit_lamports
    );
    send_tx(&mut svm, &[ix_claim], &[&claimer, &key_asset]).unwrap();

    // Verify ClaimReceipt PDA was created
    let (receipt_pda, _) = claim_receipt_pda(&pda, &claimer.pubkey());
    let receipt = read_claim_receipt(&svm, &receipt_pda);
    assert_eq!(receipt.claimer, claimer.pubkey());
    assert_eq!(receipt.promo, pda);

    // Verify KeyState PDA was created with correct rate limits
    let (ks_pda, _) = key_state_pda(&key_asset.pubkey());
    let ks = read_key_state(&svm, &ks_pda);
    assert_eq!(ks.asset, key_asset.pubkey());
    assert_eq!(ks.borrow_bucket.capacity, borrow_capacity);
    assert_eq!(ks.borrow_bucket.refill_period, borrow_refill_period);
    assert_eq!(ks.borrow_bucket.level, borrow_capacity); // starts full
    // No sell bucket configured
    assert_eq!(ks.sell_bucket.capacity, 0);

    // Verify claims_count incremented
    let promo = read_promo_config(&svm, &pda);
    assert_eq!(promo.claims_count, 1);

    // Verify the key NFT exists (account should be owned by MPL-Core)
    let key_account = svm.get_account(&key_asset.pubkey()).unwrap();
    assert_eq!(key_account.owner, MPL_CORE_ID);

    // Verify the key NFT has a promo attribute matching the promo PDA
    let attrs = extract_asset_attributes(&key_account);
    let promo_attr = attrs.iter().find(|(k, _)| k == "promo");
    assert!(promo_attr.is_some(), "key NFT should have a promo attribute");
    assert_eq!(promo_attr.unwrap().1, pda.to_string());
}

// ---------------------------------------------------------------------------
// 7. test_claim_promo_key_duplicate_rejected
// ---------------------------------------------------------------------------

#[test]
fn test_claim_promo_key_duplicate_rejected() {
    let (mut svm, _) = setup();
    let (admin, admin_asset, _pos_pda, collection) = promo_setup(&mut svm);

    let authority_seed = admin_asset.pubkey();
    let name_suffix = "No Dupes";

    let ix = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        name_suffix,
        PERM_BUY,
        0, 0, 0, 0, 0, 0, "", "",
    );
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    let (pda, _) = promo_pda(&authority_seed, name_suffix);

    // First claim succeeds
    let claimer = Keypair::new();
    svm.airdrop(&claimer.pubkey(), 5_000_000_000).unwrap();
    let key_asset_1 = Keypair::new();
    let ix_claim_1 = ix_claim_promo_key(
        &claimer.pubkey(),
        &pda,
        &admin_asset.pubkey(),
        &key_asset_1.pubkey(),
        &collection,
        0, // min_deposit_lamports = 0 for this promo
    );
    send_tx(&mut svm, &[ix_claim_1], &[&claimer, &key_asset_1]).unwrap();

    // Second claim from same wallet should fail (ClaimReceipt PDA collision)
    let key_asset_2 = Keypair::new();
    let ix_claim_2 = ix_claim_promo_key(
        &claimer.pubkey(),
        &pda,
        &admin_asset.pubkey(),
        &key_asset_2.pubkey(),
        &collection,
        0,
    );
    let result = send_tx(&mut svm, &[ix_claim_2], &[&claimer, &key_asset_2]);
    assert!(result.is_err(), "duplicate claim from same wallet should fail");
}

// ---------------------------------------------------------------------------
// 8. test_claim_promo_key_inactive_rejected
// ---------------------------------------------------------------------------

#[test]
fn test_claim_promo_key_inactive_rejected() {
    let (mut svm, _) = setup();
    let (admin, admin_asset, _pos_pda, collection) = promo_setup(&mut svm);

    let authority_seed = admin_asset.pubkey();
    let name_suffix = "Paused";

    // Create promo
    let ix = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        name_suffix,
        PERM_BUY,
        0, 0, 0, 0, 0, 0, "", "",
    );
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    let (pda, _) = promo_pda(&authority_seed, name_suffix);

    // Deactivate promo
    let ix_deactivate = ix_update_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        &pda,
        Some(false),
        None,
    );
    send_tx(&mut svm, &[ix_deactivate], &[&admin]).unwrap();

    // Try to claim — should fail with PromoInactive
    let claimer = Keypair::new();
    svm.airdrop(&claimer.pubkey(), 5_000_000_000).unwrap();
    let key_asset = Keypair::new();
    let ix_claim = ix_claim_promo_key(
        &claimer.pubkey(),
        &pda,
        &admin_asset.pubkey(),
        &key_asset.pubkey(),
        &collection,
        0, // min_deposit_lamports = 0 for this promo
    );
    let result = send_tx(&mut svm, &[ix_claim], &[&claimer, &key_asset]);
    assert!(result.is_err(), "claiming from inactive promo should fail");
}

// ---------------------------------------------------------------------------
// 9. test_claim_promo_key_max_claims_reached
// ---------------------------------------------------------------------------

#[test]
fn test_claim_promo_key_max_claims_reached() {
    let (mut svm, _) = setup();
    let (admin, admin_asset, _pos_pda, collection) = promo_setup(&mut svm);

    let authority_seed = admin_asset.pubkey();
    let name_suffix = "Max One";

    // Create promo with max_claims=1
    let ix = ix_create_promo(
        &admin.pubkey(),
        &admin_asset.pubkey(),
        name_suffix,
        PERM_BUY,
        0, 0, 0, 0, 0, 1, "", "",
    );
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    let (pda, _) = promo_pda(&authority_seed, name_suffix);

    // Wallet A claims successfully
    let claimer_a = Keypair::new();
    svm.airdrop(&claimer_a.pubkey(), 5_000_000_000).unwrap();
    let key_a = Keypair::new();
    let ix_a = ix_claim_promo_key(
        &claimer_a.pubkey(),
        &pda,
        &admin_asset.pubkey(),
        &key_a.pubkey(),
        &collection,
        0, // min_deposit_lamports = 0 for this promo
    );
    send_tx(&mut svm, &[ix_a], &[&claimer_a, &key_a]).unwrap();

    // Verify claims_count is 1
    let promo = read_promo_config(&svm, &pda);
    assert_eq!(promo.claims_count, 1);

    // Wallet B tries to claim — should fail with PromoMaxClaimsReached
    let claimer_b = Keypair::new();
    svm.airdrop(&claimer_b.pubkey(), 5_000_000_000).unwrap();
    let key_b = Keypair::new();
    let ix_b = ix_claim_promo_key(
        &claimer_b.pubkey(),
        &pda,
        &admin_asset.pubkey(),
        &key_b.pubkey(),
        &collection,
        0,
    );
    let result = send_tx(&mut svm, &[ix_b], &[&claimer_b, &key_b]);
    assert!(result.is_err(), "claim should fail when max_claims reached");
}
