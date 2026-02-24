//! Integration tests against a mainnet-forked validator.
//!
//! These tests require a running `solana-test-validator` with cloned Mayflower
//! accounts. Start it with:
//!
//! ```sh
//! ./scripts/start-mainnet-fork.sh --reset
//! ```
//!
//! Then run:
//! ```sh
//! cargo test -p hardig --test mainnet_fork -- --ignored --nocapture
//! ```
//!
//! All tests are `#[ignore]` because they need an external validator process.

use anchor_lang::AccountDeserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program,
    transaction::Transaction,
};

use hardig::mayflower;
use hardig::state::{KeyState, MarketConfig, PositionState, ProtocolConfig, PRESET_OPERATOR};

const RPC_URL: &str = "http://127.0.0.1:8899";

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
// RPC helpers
// ---------------------------------------------------------------------------

fn rpc() -> RpcClient {
    RpcClient::new_with_commitment(RPC_URL.to_string(), CommitmentConfig::confirmed())
}

fn airdrop(client: &RpcClient, pubkey: &Pubkey, lamports: u64) {
    let sig = client.request_airdrop(pubkey, lamports).unwrap();
    loop {
        if client.confirm_transaction(&sig).unwrap() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn send_and_confirm(
    client: &RpcClient,
    ixs: &[Instruction],
    signers: &[&Keypair],
) -> Result<(), String> {
    let blockhash = client.get_latest_blockhash().unwrap();
    let tx = Transaction::new_signed_with_payer(ixs, Some(&signers[0].pubkey()), signers, blockhash);
    client
        .send_and_confirm_transaction(&tx)
        .map(|_| ())
        .map_err(|e| format!("{:?}", e))
}

fn get_position(client: &RpcClient, pda: &Pubkey) -> PositionState {
    let data = client.get_account_data(pda).unwrap();
    PositionState::try_deserialize(&mut data.as_slice()).unwrap()
}

#[allow(dead_code)]
fn get_key_state(client: &RpcClient, pda: &Pubkey) -> KeyState {
    let data = client.get_account_data(pda).unwrap();
    KeyState::try_deserialize(&mut data.as_slice()).unwrap()
}

// ---------------------------------------------------------------------------
// PDA helpers
// ---------------------------------------------------------------------------

fn position_pda(asset: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[PositionState::SEED, asset.as_ref()], &program_id())
}

fn key_state_pda(asset: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[KeyState::SEED, asset.as_ref()],
        &program_id(),
    )
}

fn authority_pda(admin_asset: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"authority", admin_asset.as_ref()], &program_id())
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

fn ix_init_protocol(admin: &Pubkey) -> Instruction {
    let (config_pda, _) = Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id());
    Instruction::new_with_bytes(
        program_id(),
        &sighash("initialize_protocol"),
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new(config_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

fn ix_create_market_config(admin: &Pubkey) -> Instruction {
    let (config_pda, _) =
        Pubkey::find_program_address(&[ProtocolConfig::SEED], &program_id());
    let (mc_pda, _) = market_config_pda(&mayflower::DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("create_market_config");
    data.extend_from_slice(mayflower::DEFAULT_NAV_SOL_MINT.as_ref());
    data.extend_from_slice(mayflower::DEFAULT_WSOL_MINT.as_ref());
    data.extend_from_slice(mayflower::DEFAULT_MARKET_GROUP.as_ref());
    data.extend_from_slice(mayflower::DEFAULT_MARKET_META.as_ref());
    data.extend_from_slice(mayflower::DEFAULT_MAYFLOWER_MARKET.as_ref());
    data.extend_from_slice(mayflower::DEFAULT_MARKET_BASE_VAULT.as_ref());
    data.extend_from_slice(mayflower::DEFAULT_MARKET_NAV_VAULT.as_ref());
    data.extend_from_slice(mayflower::DEFAULT_FEE_VAULT.as_ref());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(mc_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
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
    let uri = "";
    data.extend_from_slice(&(uri.len() as u32).to_le_bytes());
    data.extend_from_slice(uri.as_bytes());

    let ix = Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new(config_pda, false),
            AccountMeta::new(collection_kp.pubkey(), true),
            AccountMeta::new_readonly(MPL_CORE_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    );
    (ix, collection_kp)
}

fn ix_create_position(admin: &Pubkey, asset: &Pubkey, spread_bps: u16, collection: &Pubkey) -> Instruction {
    let (pos_pda, _) =
        Pubkey::find_program_address(&[PositionState::SEED, asset.as_ref()], &program_id());
    let (prog_pda, _) = authority_pda(asset);
    let (config_pda, _) = config_pda();
    let (mc_pda, _) = market_config_pda(&mayflower::DEFAULT_NAV_SOL_MINT);
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda, &mayflower::DEFAULT_MARKET_META);
    let (escrow_pda, _) = mayflower::derive_personal_position_escrow(&pp_pda);
    let (log_pda, _) = mayflower::derive_log_account();

    let mut data = sighash("create_position");
    data.extend_from_slice(&spread_bps.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),                                    // admin
            AccountMeta::new(*asset, true),                                    // admin_asset (signer)
            AccountMeta::new(pos_pda, false),                                  // position
            AccountMeta::new_readonly(prog_pda, false),                        // program_pda
            AccountMeta::new_readonly(config_pda, false),                      // config
            AccountMeta::new(*collection, false),                              // collection
            AccountMeta::new_readonly(mc_pda, false),                          // market_config
            AccountMeta::new_readonly(MPL_CORE_ID, false),                     // mpl_core_program
            AccountMeta::new_readonly(system_program::ID, false),              // system_program
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),                    // token_program
            AccountMeta::new(pp_pda, false),                                   // mayflower_personal_position
            AccountMeta::new(escrow_pda, false),                               // mayflower_user_shares
            AccountMeta::new_readonly(mayflower::DEFAULT_MARKET_META, false),  // mayflower_market_meta
            AccountMeta::new_readonly(mayflower::DEFAULT_NAV_SOL_MINT, false), // nav_sol_mint
            AccountMeta::new(log_pda, false),                                  // mayflower_log
            AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false), // mayflower_program
        ],
    )
}

fn ix_authorize_key(
    admin: &Pubkey,
    admin_asset: &Pubkey,
    new_asset: &Pubkey,
    target_wallet: &Pubkey,
    role: u8,
    sell_bucket_capacity: u64,
    sell_refill_period_slots: u64,
    borrow_bucket_capacity: u64,
    borrow_refill_period_slots: u64,
) -> Instruction {
    let (pos_pda, _) =
        Pubkey::find_program_address(&[PositionState::SEED, admin_asset.as_ref()], &program_id());
    let (prog_pda, _) = authority_pda(admin_asset);
    let (ks_pda, _) = key_state_pda(new_asset);

    let mut data = sighash("authorize_key");
    data.push(role);
    data.extend_from_slice(&sell_bucket_capacity.to_le_bytes());
    data.extend_from_slice(&sell_refill_period_slots.to_le_bytes());
    data.extend_from_slice(&borrow_bucket_capacity.to_le_bytes());
    data.extend_from_slice(&borrow_refill_period_slots.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),                   // admin
            AccountMeta::new_readonly(*admin_asset, false),    // admin_key_asset
            AccountMeta::new_readonly(pos_pda, false),         // position
            AccountMeta::new(*new_asset, true),                // new_key_asset (signer)
            AccountMeta::new_readonly(*target_wallet, false),  // target_wallet
            AccountMeta::new(ks_pda, false),                   // key_state (init)
            AccountMeta::new_readonly(prog_pda, false),        // program_pda
            AccountMeta::new_readonly(MPL_CORE_ID, false),     // mpl_core_program
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

/// Compute the common Mayflower derived addresses for a given position's admin_asset.
fn mayflower_addrs(admin_asset: &Pubkey) -> (Pubkey, Pubkey, Pubkey, Pubkey, Pubkey, Pubkey) {
    let (program_pda, _) = authority_pda(admin_asset);
    let (pp_pda, _) = mayflower::derive_personal_position(&program_pda, &mayflower::DEFAULT_MARKET_META);
    let (escrow_pda, _) = mayflower::derive_personal_position_escrow(&pp_pda);
    let (log_pda, _) = mayflower::derive_log_account();
    let wsol_ata = get_ata(&program_pda, &mayflower::DEFAULT_WSOL_MINT);
    let nav_sol_ata = get_ata(&program_pda, &mayflower::DEFAULT_NAV_SOL_MINT);
    (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata)
}

/// Build a buy instruction with all Mayflower CPI accounts.
fn ix_buy_with_cpi(
    signer: &Pubkey,
    key_asset: &Pubkey,
    position: &Pubkey,
    admin_asset: &Pubkey,
    amount: u64,
) -> Instruction {
    let (program_pda, pp_pda, escrow, log_account, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&mayflower::DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("buy");
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),                          // signer
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(*position, false),                       // position
            AccountMeta::new_readonly(mc_pda, false),                 // market_config
            AccountMeta::new_readonly(system_program::ID, false),     // system_program
            AccountMeta::new(program_pda, false),                     // program_pda
            AccountMeta::new(pp_pda, false),                          // personal_position
            AccountMeta::new(escrow, false),                          // user_shares
            AccountMeta::new(nav_sol_ata, false),                     // user_nav_sol_ata
            AccountMeta::new(wsol_ata, false),                        // user_wsol_ata
            AccountMeta::new_readonly(mayflower::MAYFLOWER_TENANT, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_MARKET_GROUP, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_MARKET_META, false),
            AccountMeta::new(mayflower::DEFAULT_MAYFLOWER_MARKET, false),
            AccountMeta::new(mayflower::DEFAULT_NAV_SOL_MINT, false),
            AccountMeta::new(mayflower::DEFAULT_MARKET_BASE_VAULT, false),
            AccountMeta::new(mayflower::DEFAULT_MARKET_NAV_VAULT, false),
            AccountMeta::new(mayflower::DEFAULT_FEE_VAULT, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_WSOL_MINT, false),
            AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new(log_account, false),
        ],
    )
}

/// Build borrow instruction with all Mayflower CPI accounts.
fn ix_borrow_with_cpi(
    signer: &Pubkey,
    key_asset: &Pubkey,
    key_state: Option<&Pubkey>,
    position: &Pubkey,
    admin_asset: &Pubkey,
    amount: u64,
) -> Instruction {
    let (program_pda, pp_pda, _escrow, log_account, wsol_ata, _nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&mayflower::DEFAULT_NAV_SOL_MINT);
    let key_state_key = key_state.copied().unwrap_or(program_id());

    let mut data = sighash("borrow");
    data.extend_from_slice(&amount.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),                          // admin
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(key_state_key, false),                   // key_state (Option)
            AccountMeta::new(*position, false),                       // position
            AccountMeta::new_readonly(mc_pda, false),                 // market_config
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new(program_pda, false),                     // program_pda
            AccountMeta::new(pp_pda, false),                          // personal_position
            AccountMeta::new(wsol_ata, false),                        // user_base_token_ata
            AccountMeta::new_readonly(mayflower::MAYFLOWER_TENANT, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_MARKET_GROUP, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_MARKET_META, false),
            AccountMeta::new(mayflower::DEFAULT_MARKET_BASE_VAULT, false),
            AccountMeta::new(mayflower::DEFAULT_MARKET_NAV_VAULT, false),
            AccountMeta::new(mayflower::DEFAULT_FEE_VAULT, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_WSOL_MINT, false),
            AccountMeta::new(mayflower::DEFAULT_MAYFLOWER_MARKET, false),
            AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new(log_account, false),
        ],
    )
}

/// Build repay instruction with all Mayflower CPI accounts.
fn ix_repay_with_cpi(
    signer: &Pubkey,
    key_asset: &Pubkey,
    position: &Pubkey,
    admin_asset: &Pubkey,
    amount: u64,
) -> Instruction {
    let (program_pda, pp_pda, _escrow, log_account, wsol_ata, _nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&mayflower::DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("repay");
    data.extend_from_slice(&amount.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),                          // signer
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(*position, false),                       // position
            AccountMeta::new_readonly(mc_pda, false),                 // market_config
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new(program_pda, false),                     // program_pda
            AccountMeta::new(pp_pda, false),                          // personal_position
            AccountMeta::new(wsol_ata, false),                        // user_base_token_ata
            AccountMeta::new_readonly(mayflower::DEFAULT_MARKET_META, false),
            AccountMeta::new(mayflower::DEFAULT_MARKET_BASE_VAULT, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_WSOL_MINT, false),
            AccountMeta::new(mayflower::DEFAULT_MAYFLOWER_MARKET, false),
            AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new(log_account, false),
        ],
    )
}

/// Build reinvest instruction with all Mayflower CPI accounts.
#[allow(dead_code)]
fn ix_reinvest_with_cpi(
    signer: &Pubkey,
    key_asset: &Pubkey,
    position: &Pubkey,
    admin_asset: &Pubkey,
) -> Instruction {
    let (program_pda, pp_pda, escrow, log_account, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_asset);
    let (mc_pda, _) = market_config_pda(&mayflower::DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("reinvest");
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),                          // signer
            AccountMeta::new_readonly(*key_asset, false),             // key_asset
            AccountMeta::new(*position, false),                       // position
            AccountMeta::new_readonly(mc_pda, false),                 // market_config
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new(program_pda, false),                     // program_pda
            AccountMeta::new(pp_pda, false),                          // personal_position
            AccountMeta::new(escrow, false),                          // user_shares
            AccountMeta::new(nav_sol_ata, false),                     // user_nav_sol_ata
            AccountMeta::new(wsol_ata, false),                        // user_wsol_ata
            AccountMeta::new(wsol_ata, false),                        // user_base_token_ata (same)
            AccountMeta::new_readonly(mayflower::MAYFLOWER_TENANT, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_MARKET_GROUP, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_MARKET_META, false),
            AccountMeta::new(mayflower::DEFAULT_MAYFLOWER_MARKET, false),
            AccountMeta::new(mayflower::DEFAULT_NAV_SOL_MINT, false),
            AccountMeta::new(mayflower::DEFAULT_MARKET_BASE_VAULT, false),
            AccountMeta::new(mayflower::DEFAULT_MARKET_NAV_VAULT, false),
            AccountMeta::new(mayflower::DEFAULT_FEE_VAULT, false),
            AccountMeta::new_readonly(mayflower::DEFAULT_WSOL_MINT, false),
            AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new(log_account, false),
        ],
    )
}

// ---------------------------------------------------------------------------
// Full setup helper
// ---------------------------------------------------------------------------

struct ForkHarness {
    admin: Keypair,
    admin_asset: Keypair,
    position_pda: Pubkey,
    #[allow(dead_code)]
    collection: Pubkey,
}

fn full_fork_setup(client: &RpcClient) -> ForkHarness {
    let admin = Keypair::new();
    airdrop(client, &admin.pubkey(), 50_000_000_000); // 50 SOL

    // Init protocol
    send_and_confirm(client, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    // Create collection
    let (coll_ix, coll_kp) = ix_create_collection(&admin.pubkey());
    send_and_confirm(client, &[coll_ix], &[&admin, &coll_kp]).unwrap();
    let collection = coll_kp.pubkey();

    // Create MarketConfig for default navSOL market
    send_and_confirm(client, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    // Create position (admin_asset is a new keypair that becomes the MPL-Core asset)
    let admin_asset = Keypair::new();
    send_and_confirm(
        client,
        &[ix_create_position(
            &admin.pubkey(),
            &admin_asset.pubkey(),
            500, // max_reinvest_spread_bps
            &collection,
        )],
        &[&admin, &admin_asset],
    )
    .unwrap();

    let (pos_pda, _) = position_pda(&admin_asset.pubkey());

    ForkHarness {
        admin,
        admin_asset,
        position_pda: pos_pda,
        collection,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_mainnet_fork_read_floor_price() {
    let client = rpc();
    let market_data = client.get_account_data(&mayflower::DEFAULT_MAYFLOWER_MARKET).unwrap();
    let floor = mayflower::read_floor_price(&market_data).unwrap();

    assert!(floor > 0, "Floor price should be positive, got {}", floor);
    assert!(
        floor < 100_000_000_000,
        "Floor price suspiciously high: {}",
        floor
    );

    println!(
        "Current floor price: {} lamports (per navSOL-lamport, scaled 1e9)",
        floor
    );
}

#[test]
#[ignore]
fn test_mainnet_fork_init_protocol_and_position() {
    let client = rpc();
    let harness = full_fork_setup(&client);

    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.authority_seed, harness.admin_asset.pubkey());
    assert_eq!(pos.max_reinvest_spread_bps, 500);
    assert_eq!(pos.deposited_nav, 0);
    assert_eq!(pos.user_debt, 0);
    assert!(pos.last_admin_activity > 0);

    println!("Position created: {}", harness.position_pda);
}

#[test]
#[ignore]
fn test_mainnet_fork_buy_with_cpi() {
    let client = rpc();
    let harness = full_fork_setup(&client);

    // Fund the program PDA's wSOL ATA for the buy.
    let (prog_pda, _) = authority_pda(&harness.admin_asset.pubkey());
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::DEFAULT_WSOL_MINT);

    // Create wSOL ATA for program PDA and fund it
    let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::DEFAULT_WSOL_MINT,
        &SPL_TOKEN_ID,
    );
    send_and_confirm(&client, &[create_ata_ix], &[&harness.admin]).unwrap();

    // Transfer SOL to wSOL ATA and sync native
    let buy_amount = 1_000_000_000u64; // 1 SOL
    let transfer_ix = solana_sdk::system_instruction::transfer(
        &harness.admin.pubkey(),
        &user_wsol_ata,
        buy_amount,
    );
    let sync_ix = spl_token::instruction::sync_native(&SPL_TOKEN_ID, &user_wsol_ata).unwrap();
    send_and_confirm(
        &client,
        &[transfer_ix, sync_ix],
        &[&harness.admin],
    )
    .unwrap();

    // Also create navSOL ATA for program PDA
    let create_nav_ata_ix =
        spl_associated_token_account::instruction::create_associated_token_account(
            &harness.admin.pubkey(),
            &prog_pda,
            &mayflower::DEFAULT_NAV_SOL_MINT,
            &SPL_TOKEN_ID,
        );
    send_and_confirm(&client, &[create_nav_ata_ix], &[&harness.admin]).unwrap();

    // Now do the buy via CPI
    let result = send_and_confirm(
        &client,
        &[ix_buy_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            buy_amount,
        )],
        &[&harness.admin],
    );
    assert!(result.is_ok(), "buy with CPI failed: {:?}", result);

    // Verify position accounting
    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.deposited_nav, buy_amount);
    assert_eq!(pos.user_debt, 0);

    // Verify Mayflower PersonalPosition has deposited shares
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda, &mayflower::DEFAULT_MARKET_META);
    let pp_data = client.get_account_data(&pp_pda).unwrap();
    let shares = mayflower::read_deposited_shares(&pp_data).unwrap();
    assert!(shares > 0, "Should have deposited shares, got 0");

    println!(
        "Buy succeeded: {} lamports deposited, {} shares",
        buy_amount, shares
    );
}

#[test]
#[ignore]
fn test_mainnet_fork_borrow_against_floor() {
    let client = rpc();
    let harness = full_fork_setup(&client);

    // Setup wSOL and navSOL ATAs + fund
    let (prog_pda, _) = authority_pda(&harness.admin_asset.pubkey());
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::DEFAULT_WSOL_MINT);

    let create_wsol = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::DEFAULT_WSOL_MINT,
        &SPL_TOKEN_ID,
    );
    let create_nav = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::DEFAULT_NAV_SOL_MINT,
        &SPL_TOKEN_ID,
    );
    send_and_confirm(&client, &[create_wsol, create_nav], &[&harness.admin]).unwrap();

    let buy_amount = 5_000_000_000u64; // 5 SOL
    let transfer_ix = solana_sdk::system_instruction::transfer(
        &harness.admin.pubkey(),
        &user_wsol_ata,
        buy_amount,
    );
    let sync_ix = spl_token::instruction::sync_native(&SPL_TOKEN_ID, &user_wsol_ata).unwrap();
    send_and_confirm(&client, &[transfer_ix, sync_ix], &[&harness.admin]).unwrap();

    // Buy navSOL
    send_and_confirm(
        &client,
        &[ix_buy_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            buy_amount,
        )],
        &[&harness.admin],
    )
    .unwrap();

    // Read floor and calculate borrow capacity
    let market_data = client.get_account_data(&mayflower::DEFAULT_MAYFLOWER_MARKET).unwrap();
    let floor = mayflower::read_floor_price(&market_data).unwrap();

    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda, &mayflower::DEFAULT_MARKET_META);
    let pp_data = client.get_account_data(&pp_pda).unwrap();
    let shares = mayflower::read_deposited_shares(&pp_data).unwrap();
    let debt = mayflower::read_debt(&pp_data).unwrap();
    let capacity = mayflower::calculate_borrow_capacity(shares, floor, debt).unwrap();

    println!(
        "Floor: {}, Shares: {}, Debt: {}, Capacity: {}",
        floor, shares, debt, capacity
    );
    assert!(capacity > 0, "Should have borrow capacity");

    // Borrow half the capacity (admin key — no key_state needed)
    let borrow_amount = capacity / 2;
    let result = send_and_confirm(
        &client,
        &[ix_borrow_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            None, // admin has no key_state
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            borrow_amount,
        )],
        &[&harness.admin],
    );
    assert!(result.is_ok(), "borrow failed: {:?}", result);

    // Verify position accounting
    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.user_debt, borrow_amount);

    // Verify Mayflower debt
    let pp_data = client.get_account_data(&pp_pda).unwrap();
    let mf_debt = mayflower::read_debt(&pp_data).unwrap();
    assert!(mf_debt > 0, "Mayflower debt should be > 0 after borrow");

    println!("Borrowed {} lamports, Mayflower debt: {}", borrow_amount, mf_debt);
}

#[test]
#[ignore]
fn test_mainnet_fork_repay_debt() {
    let client = rpc();
    let harness = full_fork_setup(&client);

    let (prog_pda, _) = authority_pda(&harness.admin_asset.pubkey());
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::DEFAULT_WSOL_MINT);

    let create_wsol = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::DEFAULT_WSOL_MINT,
        &SPL_TOKEN_ID,
    );
    let create_nav = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::DEFAULT_NAV_SOL_MINT,
        &SPL_TOKEN_ID,
    );
    send_and_confirm(&client, &[create_wsol, create_nav], &[&harness.admin]).unwrap();

    let buy_amount = 5_000_000_000u64;
    let transfer_ix = solana_sdk::system_instruction::transfer(
        &harness.admin.pubkey(),
        &user_wsol_ata,
        buy_amount,
    );
    let sync_ix = spl_token::instruction::sync_native(&SPL_TOKEN_ID, &user_wsol_ata).unwrap();
    send_and_confirm(&client, &[transfer_ix, sync_ix], &[&harness.admin]).unwrap();

    // Buy
    send_and_confirm(
        &client,
        &[ix_buy_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            buy_amount,
        )],
        &[&harness.admin],
    )
    .unwrap();

    // Borrow 1 SOL
    let borrow_amount = 1_000_000_000u64;
    send_and_confirm(
        &client,
        &[ix_borrow_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            None,
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            borrow_amount,
        )],
        &[&harness.admin],
    )
    .unwrap();

    // Fund wSOL ATA for repay
    let transfer_repay = solana_sdk::system_instruction::transfer(
        &harness.admin.pubkey(),
        &user_wsol_ata,
        borrow_amount,
    );
    let sync_repay = spl_token::instruction::sync_native(&SPL_TOKEN_ID, &user_wsol_ata).unwrap();
    send_and_confirm(&client, &[transfer_repay, sync_repay], &[&harness.admin]).unwrap();

    // Repay
    let result = send_and_confirm(
        &client,
        &[ix_repay_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            borrow_amount,
        )],
        &[&harness.admin],
    );
    assert!(result.is_ok(), "repay failed: {:?}", result);

    // Verify position debt is 0
    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.user_debt, 0);

    // Verify Mayflower debt is 0
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda, &mayflower::DEFAULT_MARKET_META);
    let pp_data = client.get_account_data(&pp_pda).unwrap();
    let mf_debt = mayflower::read_debt(&pp_data).unwrap();
    assert_eq!(mf_debt, 0, "Mayflower debt should be 0 after full repay");

    println!("Repaid {} lamports, debt cleared", borrow_amount);
}

#[test]
#[ignore]
fn test_mainnet_fork_full_lifecycle() {
    let client = rpc();
    let harness = full_fork_setup(&client);

    let (prog_pda, _) = authority_pda(&harness.admin_asset.pubkey());
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::DEFAULT_WSOL_MINT);

    // Setup ATAs
    let create_wsol = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::DEFAULT_WSOL_MINT,
        &SPL_TOKEN_ID,
    );
    let create_nav = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::DEFAULT_NAV_SOL_MINT,
        &SPL_TOKEN_ID,
    );
    send_and_confirm(&client, &[create_wsol, create_nav], &[&harness.admin]).unwrap();

    // 2. Fund and buy 10 SOL worth of navSOL
    let buy_amount = 10_000_000_000u64;
    let transfer_ix = solana_sdk::system_instruction::transfer(
        &harness.admin.pubkey(),
        &user_wsol_ata,
        buy_amount,
    );
    let sync_ix = spl_token::instruction::sync_native(&SPL_TOKEN_ID, &user_wsol_ata).unwrap();
    send_and_confirm(&client, &[transfer_ix, sync_ix], &[&harness.admin]).unwrap();

    send_and_confirm(
        &client,
        &[ix_buy_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            buy_amount,
        )],
        &[&harness.admin],
    )
    .unwrap();

    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.deposited_nav, buy_amount);
    println!("[1] Bought {} lamports of navSOL", buy_amount);

    // 3. Authorize an operator
    let operator = Keypair::new();
    airdrop(&client, &operator.pubkey(), 5_000_000_000);
    let op_asset = Keypair::new();
    send_and_confirm(
        &client,
        &[ix_authorize_key(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            &op_asset.pubkey(),
            &operator.pubkey(),
            PRESET_OPERATOR,
            0, 0, 0, 0, // no rate limits
        )],
        &[&harness.admin, &op_asset],
    )
    .unwrap();
    println!("[2] Operator authorized");

    // 4. Borrow against floor (as admin)
    let market_data = client.get_account_data(&mayflower::DEFAULT_MAYFLOWER_MARKET).unwrap();
    let floor = mayflower::read_floor_price(&market_data).unwrap();
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda, &mayflower::DEFAULT_MARKET_META);
    let pp_data = client.get_account_data(&pp_pda).unwrap();
    let shares = mayflower::read_deposited_shares(&pp_data).unwrap();
    let capacity = mayflower::calculate_borrow_capacity(shares, floor, 0).unwrap();

    let borrow_amount = capacity / 4; // conservative
    send_and_confirm(
        &client,
        &[ix_borrow_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_asset.pubkey(),
            None,
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            borrow_amount,
        )],
        &[&harness.admin],
    )
    .unwrap();

    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.user_debt, borrow_amount);
    println!("[3] Borrowed {} lamports (capacity was {})", borrow_amount, capacity);

    // 5. Operator repays (fund the ATA first)
    let transfer_repay = solana_sdk::system_instruction::transfer(
        &harness.admin.pubkey(),
        &user_wsol_ata,
        borrow_amount,
    );
    let sync_repay = spl_token::instruction::sync_native(&SPL_TOKEN_ID, &user_wsol_ata).unwrap();
    send_and_confirm(&client, &[transfer_repay, sync_repay], &[&harness.admin]).unwrap();

    // Operator does the repay via CPI
    send_and_confirm(
        &client,
        &[ix_repay_with_cpi(
            &operator.pubkey(),
            &op_asset.pubkey(),
            &harness.position_pda,
            &harness.admin_asset.pubkey(),
            borrow_amount,
        )],
        &[&operator],
    )
    .unwrap();

    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.user_debt, 0);
    println!("[4] Operator repaid {} lamports", borrow_amount);

    // 6. Verify final state
    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.deposited_nav, buy_amount);
    assert_eq!(pos.user_debt, 0);
    println!("[5] Final state verified - clean position");
}

#[test]
#[ignore]
fn test_mainnet_fork_floor_price_vs_capacity() {
    let client = rpc();

    // Read current floor price
    let market_data = client.get_account_data(&mayflower::DEFAULT_MAYFLOWER_MARKET).unwrap();
    let floor = mayflower::read_floor_price(&market_data).unwrap();
    println!("Floor price: {} lamports/navSOL-lamport (scaled 1e9)", floor);

    // The floor should be >= 1 SOL equivalent (navSOL is backed by SOL)
    assert!(
        floor >= 1_000_000_000,
        "navSOL floor should be >= 1 SOL equivalent, got {}",
        floor
    );

    // Test capacity calculation with known values
    let deposited = 10_000_000_000u64;
    let capacity = mayflower::calculate_borrow_capacity(deposited, floor, 0).unwrap();
    println!(
        "10 shares @ floor {} => capacity {} lamports",
        floor, capacity
    );

    // Capacity should be positive and proportional to deposit * floor
    assert!(capacity > 0);
    let expected = (deposited as u128 * floor as u128 / 1_000_000_000) as u64;
    assert_eq!(capacity, expected);
}
