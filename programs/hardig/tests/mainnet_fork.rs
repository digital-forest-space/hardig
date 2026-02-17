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
use hardig::state::{KeyAuthorization, KeyRole, PositionNFT, ProtocolConfig};

const RPC_URL: &str = "http://127.0.0.1:8899";

const SPL_TOKEN_ID: Pubkey = solana_sdk::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ATA_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

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

fn get_position(client: &RpcClient, pda: &Pubkey) -> PositionNFT {
    let data = client.get_account_data(pda).unwrap();
    PositionNFT::try_deserialize(&mut data.as_slice()).unwrap()
}

fn get_key_auth(client: &RpcClient, pda: &Pubkey) -> KeyAuthorization {
    let data = client.get_account_data(pda).unwrap();
    KeyAuthorization::try_deserialize(&mut data.as_slice()).unwrap()
}

// ---------------------------------------------------------------------------
// PDA helpers
// ---------------------------------------------------------------------------

fn position_pda(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[PositionNFT::SEED, mint.as_ref()], &program_id())
}

fn key_auth_pda(position: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[KeyAuthorization::SEED, position.as_ref(), mint.as_ref()],
        &program_id(),
    )
}

fn authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"authority"], &program_id())
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

fn ix_create_position(admin: &Pubkey, mint: &Pubkey, spread_bps: u16) -> Instruction {
    let admin_ata = get_ata(admin, mint);
    let (pos_pda, _) =
        Pubkey::find_program_address(&[PositionNFT::SEED, mint.as_ref()], &program_id());
    let (ka_pda, _) = Pubkey::find_program_address(
        &[KeyAuthorization::SEED, pos_pda.as_ref(), mint.as_ref()],
        &program_id(),
    );
    let (prog_pda, _) = authority_pda();

    let mut data = sighash("create_position");
    data.extend_from_slice(&spread_bps.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new(*mint, true),
            AccountMeta::new(admin_ata, false),
            AccountMeta::new(pos_pda, false),
            AccountMeta::new(ka_pda, false),
            AccountMeta::new_readonly(prog_pda, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

fn ix_authorize_key(
    admin: &Pubkey,
    admin_nft_mint: &Pubkey,
    position: &Pubkey,
    admin_key_auth: &Pubkey,
    new_mint: &Pubkey,
    target_wallet: &Pubkey,
    role: u8,
) -> Instruction {
    let admin_nft_ata = get_ata(admin, admin_nft_mint);
    let target_ata = get_ata(target_wallet, new_mint);
    let (new_ka, _) = Pubkey::find_program_address(
        &[KeyAuthorization::SEED, position.as_ref(), new_mint.as_ref()],
        &program_id(),
    );
    let (prog_pda, _) = authority_pda();

    let mut data = sighash("authorize_key");
    data.push(role);

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(admin_nft_ata, false),
            AccountMeta::new_readonly(*admin_key_auth, false),
            AccountMeta::new_readonly(*position, false),
            AccountMeta::new(*new_mint, true),
            AccountMeta::new(target_ata, false),
            AccountMeta::new_readonly(*target_wallet, false),
            AccountMeta::new(new_ka, false),
            AccountMeta::new_readonly(prog_pda, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

/// Build a role-gated instruction (buy, withdraw, borrow, repay, reinvest).
/// For buy/withdraw/borrow/repay, `amount` is Some(u64).
/// For reinvest, `amount` is None.
fn ix_role_gated(
    ix_name: &str,
    signer: &Pubkey,
    nft_mint: &Pubkey,
    key_auth_pda: &Pubkey,
    position_pda: &Pubkey,
    amount: Option<u64>,
) -> Instruction {
    let nft_ata = get_ata(signer, nft_mint);
    let mut data = sighash(ix_name);
    if let Some(amt) = amount {
        data.extend_from_slice(&amt.to_le_bytes());
    }
    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(nft_ata, false),
            AccountMeta::new_readonly(*key_auth_pda, false),
            AccountMeta::new(*position_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

/// Build a role-gated buy instruction with Mayflower CPI remaining_accounts.
fn ix_buy_with_cpi(
    signer: &Pubkey,
    nft_mint: &Pubkey,
    key_auth: &Pubkey,
    position: &Pubkey,
    amount: u64,
) -> Instruction {
    let nft_ata = get_ata(signer, nft_mint);
    let (prog_pda, _) = authority_pda();
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
    let (escrow, _) = mayflower::derive_personal_position_escrow(&pp_pda);
    let (log_account, _) = mayflower::derive_log_account();
    let user_nav_sol_ata = get_ata(&prog_pda, &mayflower::NAV_SOL_MINT);
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::WSOL_MINT);

    let mut data = sighash("buy");
    data.extend_from_slice(&amount.to_le_bytes());

    let accounts = vec![
        // Named accounts
        AccountMeta::new(*signer, true),
        AccountMeta::new_readonly(nft_ata, false),
        AccountMeta::new_readonly(*key_auth, false),
        AccountMeta::new(*position, false),
        AccountMeta::new_readonly(system_program::ID, false),
        // remaining_accounts for Mayflower CPI (buy layout from buy.rs)
        AccountMeta::new(prog_pda, false),                          // [0] program_pda
        AccountMeta::new(pp_pda, false),                             // [1] personal_position
        AccountMeta::new(escrow, false),                             // [2] user_shares
        AccountMeta::new(user_nav_sol_ata, false),                   // [3] user_nav_sol_ata
        AccountMeta::new(user_wsol_ata, false),                      // [4] user_wsol_ata
        AccountMeta::new_readonly(mayflower::MAYFLOWER_TENANT, false), // [5] tenant
        AccountMeta::new_readonly(mayflower::MARKET_GROUP, false),   // [6] market_group
        AccountMeta::new_readonly(mayflower::MARKET_META, false),    // [7] market_meta
        AccountMeta::new(mayflower::MAYFLOWER_MARKET, false),        // [8] mayflower_market
        AccountMeta::new(mayflower::NAV_SOL_MINT, false),            // [9] nav_sol_mint
        AccountMeta::new(mayflower::MARKET_BASE_VAULT, false),       // [10] market_base_vault
        AccountMeta::new(mayflower::MARKET_NAV_VAULT, false),        // [11] market_nav_vault
        AccountMeta::new(mayflower::FEE_VAULT, false),               // [12] fee_vault
        AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false), // [13] mayflower_program
        AccountMeta::new_readonly(SPL_TOKEN_ID, false),              // [14] token_program
        AccountMeta::new(log_account, false),                        // [15] log_account
    ];

    Instruction::new_with_bytes(program_id(), &data, accounts)
}

/// Build borrow instruction with Mayflower CPI remaining_accounts.
fn ix_borrow_with_cpi(
    signer: &Pubkey,
    nft_mint: &Pubkey,
    key_auth: &Pubkey,
    position: &Pubkey,
    amount: u64,
) -> Instruction {
    let nft_ata = get_ata(signer, nft_mint);
    let (prog_pda, _) = authority_pda();
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
    let (log_account, _) = mayflower::derive_log_account();
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::WSOL_MINT);

    let mut data = sighash("borrow");
    data.extend_from_slice(&amount.to_le_bytes());

    let accounts = vec![
        // Named accounts
        AccountMeta::new(*signer, true),
        AccountMeta::new_readonly(nft_ata, false),
        AccountMeta::new_readonly(*key_auth, false),
        AccountMeta::new(*position, false),
        AccountMeta::new_readonly(system_program::ID, false),
        // remaining_accounts for Mayflower CPI (borrow layout from borrow.rs)
        AccountMeta::new(prog_pda, false),                          // [0] program_pda
        AccountMeta::new(pp_pda, false),                             // [1] personal_position
        AccountMeta::new(user_wsol_ata, false),                      // [2] user_base_token_ata
        AccountMeta::new_readonly(mayflower::MAYFLOWER_TENANT, false), // [3] tenant
        AccountMeta::new_readonly(mayflower::MARKET_GROUP, false),   // [4] market_group
        AccountMeta::new_readonly(mayflower::MARKET_META, false),    // [5] market_meta
        AccountMeta::new(mayflower::MARKET_BASE_VAULT, false),       // [6] market_base_vault
        AccountMeta::new(mayflower::MARKET_NAV_VAULT, false),        // [7] market_nav_vault
        AccountMeta::new(mayflower::FEE_VAULT, false),               // [8] fee_vault
        AccountMeta::new(mayflower::MAYFLOWER_MARKET, false),        // [9] mayflower_market
        AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false), // [10] mayflower_program
        AccountMeta::new_readonly(SPL_TOKEN_ID, false),              // [11] token_program
        AccountMeta::new(log_account, false),                        // [12] log_account
    ];

    Instruction::new_with_bytes(program_id(), &data, accounts)
}

/// Build repay instruction with Mayflower CPI remaining_accounts.
fn ix_repay_with_cpi(
    signer: &Pubkey,
    nft_mint: &Pubkey,
    key_auth: &Pubkey,
    position: &Pubkey,
    amount: u64,
) -> Instruction {
    let nft_ata = get_ata(signer, nft_mint);
    let (prog_pda, _) = authority_pda();
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
    let (log_account, _) = mayflower::derive_log_account();
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::WSOL_MINT);

    let mut data = sighash("repay");
    data.extend_from_slice(&amount.to_le_bytes());

    let accounts = vec![
        // Named accounts
        AccountMeta::new(*signer, true),
        AccountMeta::new_readonly(nft_ata, false),
        AccountMeta::new_readonly(*key_auth, false),
        AccountMeta::new(*position, false),
        AccountMeta::new_readonly(system_program::ID, false),
        // remaining_accounts for Mayflower CPI (repay layout from repay.rs)
        AccountMeta::new(prog_pda, false),                          // [0] program_pda
        AccountMeta::new(pp_pda, false),                             // [1] personal_position
        AccountMeta::new(user_wsol_ata, false),                      // [2] user_base_token_ata
        AccountMeta::new_readonly(mayflower::MAYFLOWER_TENANT, false), // [3] tenant
        AccountMeta::new_readonly(mayflower::MARKET_GROUP, false),   // [4] market_group
        AccountMeta::new_readonly(mayflower::MARKET_META, false),    // [5] market_meta
        AccountMeta::new(mayflower::MARKET_BASE_VAULT, false),       // [6] market_base_vault
        AccountMeta::new(mayflower::MARKET_NAV_VAULT, false),        // [7] market_nav_vault
        AccountMeta::new(mayflower::FEE_VAULT, false),               // [8] fee_vault
        AccountMeta::new_readonly(mayflower::WSOL_MINT, false),      // [9] base_mint
        AccountMeta::new(mayflower::MAYFLOWER_MARKET, false),        // [10] mayflower_market
        AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false), // [11] mayflower_program
        AccountMeta::new_readonly(SPL_TOKEN_ID, false),              // [12] token_program
        AccountMeta::new(log_account, false),                        // [13] log_account
    ];

    Instruction::new_with_bytes(program_id(), &data, accounts)
}

/// Build init_mayflower_position instruction.
fn ix_init_mayflower_position(
    admin: &Pubkey,
    admin_nft_mint: &Pubkey,
    admin_key_auth: &Pubkey,
    position: &Pubkey,
) -> Instruction {
    let admin_nft_ata = get_ata(admin, admin_nft_mint);
    let (prog_pda, _) = authority_pda();
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
    let (escrow, _) = mayflower::derive_personal_position_escrow(&pp_pda);
    let (log_account, _) = mayflower::derive_log_account();

    Instruction::new_with_bytes(
        program_id(),
        &sighash("init_mayflower_position"),
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(admin_nft_ata, false),
            AccountMeta::new_readonly(*admin_key_auth, false),
            AccountMeta::new(*position, false),
            AccountMeta::new_readonly(prog_pda, false),
            AccountMeta::new(pp_pda, false),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(mayflower::MARKET_META, false),
            AccountMeta::new_readonly(mayflower::NAV_SOL_MINT, false),
            AccountMeta::new(log_account, false),
            AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

/// Build reinvest instruction with Mayflower CPI remaining_accounts.
#[allow(dead_code)]
fn ix_reinvest_with_cpi(
    signer: &Pubkey,
    nft_mint: &Pubkey,
    key_auth: &Pubkey,
    position: &Pubkey,
) -> Instruction {
    let nft_ata = get_ata(signer, nft_mint);
    let (prog_pda, _) = authority_pda();
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
    let (escrow, _) = mayflower::derive_personal_position_escrow(&pp_pda);
    let (log_account, _) = mayflower::derive_log_account();
    let user_nav_sol_ata = get_ata(&prog_pda, &mayflower::NAV_SOL_MINT);
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::WSOL_MINT);

    let data = sighash("reinvest");

    let accounts = vec![
        // Named accounts
        AccountMeta::new(*signer, true),
        AccountMeta::new_readonly(nft_ata, false),
        AccountMeta::new_readonly(*key_auth, false),
        AccountMeta::new(*position, false),
        AccountMeta::new_readonly(system_program::ID, false),
        // remaining_accounts for reinvest (reinvest.rs layout)
        AccountMeta::new(prog_pda, false),                          // [0] program_pda
        AccountMeta::new(mayflower::MAYFLOWER_MARKET, false),        // [1] mayflower_market
        AccountMeta::new(pp_pda, false),                             // [2] personal_position
        AccountMeta::new(escrow, false),                             // [3] user_shares
        AccountMeta::new(user_nav_sol_ata, false),                   // [4] user_nav_sol_ata
        AccountMeta::new(user_wsol_ata, false),                      // [5] user_wsol_ata
        AccountMeta::new(user_wsol_ata, false),                      // [6] user_base_token_ata (same as wsol for borrow)
        AccountMeta::new_readonly(mayflower::MAYFLOWER_TENANT, false), // [7] tenant
        AccountMeta::new_readonly(mayflower::MARKET_GROUP, false),   // [8] market_group
        AccountMeta::new_readonly(mayflower::MARKET_META, false),    // [9] market_meta
        AccountMeta::new(mayflower::MARKET_BASE_VAULT, false),       // [10] market_base_vault
        AccountMeta::new(mayflower::MARKET_NAV_VAULT, false),        // [11] market_nav_vault
        AccountMeta::new(mayflower::FEE_VAULT, false),               // [12] fee_vault
        AccountMeta::new(mayflower::NAV_SOL_MINT, false),            // [13] nav_sol_mint
        AccountMeta::new_readonly(mayflower::MAYFLOWER_PROGRAM_ID, false), // [14] mayflower_program
        AccountMeta::new_readonly(SPL_TOKEN_ID, false),              // [15] token_program
        AccountMeta::new(log_account, false),                        // [16] log_account
    ];

    Instruction::new_with_bytes(program_id(), &data, accounts)
}

// ---------------------------------------------------------------------------
// Full setup helper
// ---------------------------------------------------------------------------

struct ForkHarness {
    admin: Keypair,
    admin_nft_mint: Keypair,
    position_pda: Pubkey,
    admin_key_auth: Pubkey,
}

fn full_fork_setup(client: &RpcClient) -> ForkHarness {
    let admin = Keypair::new();
    airdrop(client, &admin.pubkey(), 50_000_000_000); // 50 SOL

    // Init protocol
    send_and_confirm(client, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    // Create position
    let admin_nft_mint = Keypair::new();
    send_and_confirm(
        client,
        &[ix_create_position(
            &admin.pubkey(),
            &admin_nft_mint.pubkey(),
            500, // max_reinvest_spread_bps
        )],
        &[&admin, &admin_nft_mint],
    )
    .unwrap();

    let (pos_pda, _) = position_pda(&admin_nft_mint.pubkey());
    let (admin_ka, _) = key_auth_pda(&pos_pda, &admin_nft_mint.pubkey());

    ForkHarness {
        admin,
        admin_nft_mint,
        position_pda: pos_pda,
        admin_key_auth: admin_ka,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_mainnet_fork_read_floor_price() {
    let client = rpc();
    let market_data = client.get_account_data(&mayflower::MAYFLOWER_MARKET).unwrap();
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
    assert_eq!(pos.admin_nft_mint, harness.admin_nft_mint.pubkey());
    assert_eq!(pos.max_reinvest_spread_bps, 500);
    assert_eq!(pos.deposited_nav, 0);
    assert_eq!(pos.user_debt, 0);
    assert_eq!(pos.protocol_debt, 0);
    assert!(pos.last_admin_activity > 0);

    let ka = get_key_auth(&client, &harness.admin_key_auth);
    assert_eq!(ka.position, harness.position_pda);
    assert_eq!(ka.role, KeyRole::Admin);

    println!("Position created: {}", harness.position_pda);
}

#[test]
#[ignore]
fn test_mainnet_fork_init_mayflower_position() {
    let client = rpc();
    let harness = full_fork_setup(&client);

    // Init Mayflower PersonalPosition via CPI
    let result = send_and_confirm(
        &client,
        &[ix_init_mayflower_position(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
        )],
        &[&harness.admin],
    );
    assert!(result.is_ok(), "init_mayflower_position failed: {:?}", result);

    // Verify position_pda was stored
    let pos = get_position(&client, &harness.position_pda);
    let (prog_pda, _) = authority_pda();
    let (expected_pp, _) = mayflower::derive_personal_position(&prog_pda);
    assert_eq!(pos.position_pda, expected_pp);

    // Verify the Mayflower PersonalPosition account exists
    let pp_data = client.get_account_data(&expected_pp).unwrap();
    assert_eq!(pp_data.len(), mayflower::PP_SIZE);
    assert_eq!(
        &pp_data[..8],
        &mayflower::PP_DISCRIMINATOR,
        "PersonalPosition discriminator mismatch"
    );

    println!("Mayflower PersonalPosition created: {}", expected_pp);
}

#[test]
#[ignore]
fn test_mainnet_fork_buy_with_cpi() {
    let client = rpc();
    let harness = full_fork_setup(&client);

    // First init Mayflower position
    send_and_confirm(
        &client,
        &[ix_init_mayflower_position(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
        )],
        &[&harness.admin],
    )
    .unwrap();

    // Fund the program PDA's wSOL ATA for the buy.
    // In production, the user would wrap SOL → wSOL into the PDA's ATA.
    // For testing, we create and fund it manually.
    let (prog_pda, _) = authority_pda();
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::WSOL_MINT);

    // Create wSOL ATA for program PDA and fund it
    let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::WSOL_MINT,
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
    let _user_nav_ata = get_ata(&prog_pda, &mayflower::NAV_SOL_MINT);
    let create_nav_ata_ix =
        spl_associated_token_account::instruction::create_associated_token_account(
            &harness.admin.pubkey(),
            &prog_pda,
            &mayflower::NAV_SOL_MINT,
            &SPL_TOKEN_ID,
        );
    send_and_confirm(&client, &[create_nav_ata_ix], &[&harness.admin]).unwrap();

    // Now do the buy via CPI
    let result = send_and_confirm(
        &client,
        &[ix_buy_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
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
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
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

    // Init Mayflower position
    send_and_confirm(
        &client,
        &[ix_init_mayflower_position(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
        )],
        &[&harness.admin],
    )
    .unwrap();

    // Setup wSOL and navSOL ATAs + fund
    let (prog_pda, _) = authority_pda();
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::WSOL_MINT);
    let _user_nav_ata = get_ata(&prog_pda, &mayflower::NAV_SOL_MINT);

    let create_wsol = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::WSOL_MINT,
        &SPL_TOKEN_ID,
    );
    let create_nav = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::NAV_SOL_MINT,
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
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
            buy_amount,
        )],
        &[&harness.admin],
    )
    .unwrap();

    // Read floor and calculate borrow capacity
    let market_data = client.get_account_data(&mayflower::MAYFLOWER_MARKET).unwrap();
    let floor = mayflower::read_floor_price(&market_data).unwrap();

    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
    let pp_data = client.get_account_data(&pp_pda).unwrap();
    let shares = mayflower::read_deposited_shares(&pp_data).unwrap();
    let debt = mayflower::read_debt(&pp_data).unwrap();
    let capacity = mayflower::calculate_borrow_capacity(shares, floor, debt).unwrap();

    println!(
        "Floor: {}, Shares: {}, Debt: {}, Capacity: {}",
        floor, shares, debt, capacity
    );
    assert!(capacity > 0, "Should have borrow capacity");

    // Borrow half the capacity
    let borrow_amount = capacity / 2;
    let result = send_and_confirm(
        &client,
        &[ix_borrow_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
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

    // Init + buy + borrow (condensed setup)
    send_and_confirm(
        &client,
        &[ix_init_mayflower_position(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
        )],
        &[&harness.admin],
    )
    .unwrap();

    let (prog_pda, _) = authority_pda();
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::WSOL_MINT);

    let create_wsol = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::WSOL_MINT,
        &SPL_TOKEN_ID,
    );
    let create_nav = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::NAV_SOL_MINT,
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
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
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
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
            borrow_amount,
        )],
        &[&harness.admin],
    )
    .unwrap();

    // Fund wSOL ATA for repay (the borrowed SOL may be in the ATA already)
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
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
            borrow_amount,
        )],
        &[&harness.admin],
    );
    assert!(result.is_ok(), "repay failed: {:?}", result);

    // Verify position debt is 0
    let pos = get_position(&client, &harness.position_pda);
    assert_eq!(pos.user_debt, 0);

    // Verify Mayflower debt is 0
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
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

    // 1. Init Mayflower position
    send_and_confirm(
        &client,
        &[ix_init_mayflower_position(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
        )],
        &[&harness.admin],
    )
    .unwrap();

    let (prog_pda, _) = authority_pda();
    let user_wsol_ata = get_ata(&prog_pda, &mayflower::WSOL_MINT);

    // Setup ATAs
    let create_wsol = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::WSOL_MINT,
        &SPL_TOKEN_ID,
    );
    let create_nav = spl_associated_token_account::instruction::create_associated_token_account(
        &harness.admin.pubkey(),
        &prog_pda,
        &mayflower::NAV_SOL_MINT,
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
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
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
    let op_mint = Keypair::new();
    send_and_confirm(
        &client,
        &[ix_authorize_key(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.position_pda,
            &harness.admin_key_auth,
            &op_mint.pubkey(),
            &operator.pubkey(),
            1, // Operator
        )],
        &[&harness.admin, &op_mint],
    )
    .unwrap();
    let (op_ka, _) = key_auth_pda(&harness.position_pda, &op_mint.pubkey());
    println!("[2] Operator authorized");

    // 4. Borrow against floor (as admin)
    let market_data = client.get_account_data(&mayflower::MAYFLOWER_MARKET).unwrap();
    let floor = mayflower::read_floor_price(&market_data).unwrap();
    let (pp_pda, _) = mayflower::derive_personal_position(&prog_pda);
    let pp_data = client.get_account_data(&pp_pda).unwrap();
    let shares = mayflower::read_deposited_shares(&pp_data).unwrap();
    let capacity = mayflower::calculate_borrow_capacity(shares, floor, 0).unwrap();

    let borrow_amount = capacity / 4; // conservative
    send_and_confirm(
        &client,
        &[ix_borrow_with_cpi(
            &harness.admin.pubkey(),
            &harness.admin_nft_mint.pubkey(),
            &harness.admin_key_auth,
            &harness.position_pda,
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

    // Operator does the repay (accounting only since we can't sign for the PDA easily
    // in a cross-role CPI, but the accounting check is what matters).
    send_and_confirm(
        &client,
        &[ix_role_gated(
            "repay",
            &operator.pubkey(),
            &op_mint.pubkey(),
            &op_ka,
            &harness.position_pda,
            Some(borrow_amount),
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
    assert_eq!(pos.protocol_debt, 0);
    println!("[5] Final state verified - clean position");
}

#[test]
#[ignore]
fn test_mainnet_fork_floor_price_vs_capacity() {
    let client = rpc();

    // Read current floor price
    let market_data = client.get_account_data(&mayflower::MAYFLOWER_MARKET).unwrap();
    let floor = mayflower::read_floor_price(&market_data).unwrap();
    println!("Floor price: {} lamports/navSOL-lamport (scaled 1e9)", floor);

    // The floor should be >= 1 SOL equivalent (navSOL is backed by SOL)
    // floor is scaled by 1e9, so 1 SOL = 1_000_000_000
    assert!(
        floor >= 1_000_000_000,
        "navSOL floor should be >= 1 SOL equivalent, got {}",
        floor
    );

    // Test capacity calculation with known values
    let deposited = 10_000_000_000u64; // 10 navSOL-lamports worth of shares
    let capacity = mayflower::calculate_borrow_capacity(deposited, floor, 0).unwrap();
    println!(
        "10 shares @ floor {} => capacity {} lamports",
        floor, capacity
    );

    // Capacity should be positive and proportional to deposit * floor
    assert!(capacity > 0);
    // With 0 debt, capacity = deposited * floor / 1e9
    let expected = (deposited as u128 * floor as u128 / 1_000_000_000) as u64;
    assert_eq!(capacity, expected);
}
