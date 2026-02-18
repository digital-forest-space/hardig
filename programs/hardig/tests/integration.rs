use litesvm::LiteSVM;
use solana_sdk::{
    account::Account,
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
    MAYFLOWER_PROGRAM_ID, MAYFLOWER_TENANT,
};
use hardig::state::{KeyAuthorization, KeyRole, MarketConfig, PositionNFT, ProtocolConfig};

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

/// Plant the PersonalPosition and user_shares PDAs for a given admin_nft_mint.
/// Must be called AFTER the position is created (admin_nft_mint is known).
fn plant_position_stubs(svm: &mut LiteSVM, admin_nft_mint: &Pubkey) {
    let (program_pda, _) = Pubkey::find_program_address(
        &[b"authority", admin_nft_mint.as_ref()],
        &program_id(),
    );
    let (pp_pda, _) = mayflower::derive_personal_position(&program_pda, &DEFAULT_MARKET_META);
    let (escrow_pda, _) = mayflower::derive_personal_position_escrow(&pp_pda);

    // PersonalPosition — needs to be large enough for floor price / debt reads
    let owner = MAYFLOWER_PROGRAM_ID;
    plant_account(svm, &pp_pda, &owner, 256);
    plant_account(svm, &escrow_pda, &owner, 256);

    // ATAs for program PDA (wSOL and navSOL)
    let wsol_ata = get_ata(&program_pda, &DEFAULT_WSOL_MINT);
    let nav_sol_ata = get_ata(&program_pda, &DEFAULT_NAV_SOL_MINT);
    // Token accounts need 165 bytes (SPL token account size), owned by token program
    plant_account(svm, &wsol_ata, &SPL_TOKEN_ID, 165);
    plant_account(svm, &nav_sol_ata, &SPL_TOKEN_ID, 165);
}

/// Patch the PositionNFT account to set `market_config` and `position_pda` fields,
/// simulating what `init_mayflower_position` would do via CPI.
fn patch_position_for_cpi(svm: &mut LiteSVM, admin_nft_mint: &Pubkey) {
    let (pos_pda, _) = position_pda(admin_nft_mint);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let (program_pda, _) = Pubkey::find_program_address(
        &[b"authority", admin_nft_mint.as_ref()],
        &program_id(),
    );
    let (pp_pda, _) = mayflower::derive_personal_position(&program_pda, &DEFAULT_MARKET_META);

    let mut account = svm.get_account(&pos_pda).unwrap();
    // position_pda at offset 40..72
    account.data[40..72].copy_from_slice(pp_pda.as_ref());
    // market_config at offset 72..104
    account.data[72..104].copy_from_slice(mc_pda.as_ref());
    svm.set_account(pos_pda, account).unwrap();
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

fn ix_create_position(
    admin: &Pubkey,
    mint: &Pubkey,
    spread_bps: u16,
) -> Instruction {
    let admin_ata = get_ata(admin, mint);
    let (position_pda, _) =
        Pubkey::find_program_address(&[PositionNFT::SEED, mint.as_ref()], &program_id());
    let (key_auth_pda, _) = Pubkey::find_program_address(
        &[KeyAuthorization::SEED, position_pda.as_ref(), mint.as_ref()],
        &program_id(),
    );
    let (program_pda, _) =
        Pubkey::find_program_address(&[b"authority", mint.as_ref()], &program_id());

    let mut data = sighash("create_position");
    data.extend_from_slice(&spread_bps.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new(*mint, true),
            AccountMeta::new(admin_ata, false),
            AccountMeta::new(position_pda, false),
            AccountMeta::new(key_auth_pda, false),
            AccountMeta::new_readonly(program_pda, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

fn ix_authorize_key(
    admin: &Pubkey,
    admin_nft_mint: &Pubkey,
    position_pda: &Pubkey,
    admin_key_auth: &Pubkey,
    new_mint: &Pubkey,
    target_wallet: &Pubkey,
    role: u8,
) -> Instruction {
    let admin_nft_ata = get_ata(admin, admin_nft_mint);
    let target_ata = get_ata(target_wallet, new_mint);
    let (new_key_auth, _) = Pubkey::find_program_address(
        &[KeyAuthorization::SEED, position_pda.as_ref(), new_mint.as_ref()],
        &program_id(),
    );
    let (program_pda, _) =
        Pubkey::find_program_address(&[b"authority", admin_nft_mint.as_ref()], &program_id());

    let mut data = sighash("authorize_key");
    data.push(role);

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(admin_nft_ata, false),
            AccountMeta::new_readonly(*admin_key_auth, false),
            AccountMeta::new_readonly(*position_pda, false),
            AccountMeta::new(*new_mint, true),
            AccountMeta::new(target_ata, false),
            AccountMeta::new_readonly(*target_wallet, false),
            AccountMeta::new(new_key_auth, false),
            AccountMeta::new_readonly(program_pda, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

/// Build a revoke_key instruction.
/// `target_nft_holder`: if `Some`, pass that wallet's ATA for the target NFT
///   (enables burn when the admin is the holder). If `None`, pass the program ID
///   as the Anchor "None" sentinel, skipping the burn entirely.
fn ix_revoke_key(
    admin: &Pubkey,
    admin_nft_mint: &Pubkey,
    admin_key_auth: &Pubkey,
    position_pda: &Pubkey,
    target_key_auth: &Pubkey,
    target_nft_mint: &Pubkey,
    target_nft_holder: Option<&Pubkey>,
) -> Instruction {
    let admin_nft_ata = get_ata(admin, admin_nft_mint);

    // When a holder is specified, derive the real ATA; otherwise use the
    // program ID which Anchor interprets as Option::None.
    let target_nft_ata = match target_nft_holder {
        Some(holder) => get_ata(holder, target_nft_mint),
        None => program_id(),
    };

    Instruction::new_with_bytes(
        program_id(),
        &sighash("revoke_key"),
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(admin_nft_ata, false),
            AccountMeta::new_readonly(*admin_key_auth, false),
            AccountMeta::new_readonly(*position_pda, false),
            AccountMeta::new(*target_key_auth, false),
            AccountMeta::new(*target_nft_mint, false),
            AccountMeta::new(target_nft_ata, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

// ---------------------------------------------------------------------------
// Financial instruction builders (with MarketConfig)
// ---------------------------------------------------------------------------

/// Compute the common Mayflower derived addresses for a given position's admin_nft_mint.
fn mayflower_addrs(admin_nft_mint: &Pubkey) -> (Pubkey, Pubkey, Pubkey, Pubkey, Pubkey, Pubkey) {
    let (program_pda, _) = Pubkey::find_program_address(
        &[b"authority", admin_nft_mint.as_ref()],
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
    nft_mint: &Pubkey,
    key_auth_pda: &Pubkey,
    position_pda: &Pubkey,
    admin_nft_mint: &Pubkey,
    amount: u64,
) -> Instruction {
    let nft_ata = get_ata(signer, nft_mint);
    let (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_nft_mint);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("buy");
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage check)

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),                          // signer
            AccountMeta::new_readonly(nft_ata, false),                // key_nft_ata
            AccountMeta::new_readonly(*key_auth_pda, false),          // key_auth
            AccountMeta::new(*position_pda, false),                   // position
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
    nft_mint: &Pubkey,
    key_auth_pda: &Pubkey,
    position_pda: &Pubkey,
    admin_nft_mint: &Pubkey,
    amount: u64,
) -> Instruction {
    let nft_ata = get_ata(admin, nft_mint);
    let (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_nft_mint);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("withdraw");
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage check)

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(nft_ata, false),
            AccountMeta::new_readonly(*key_auth_pda, false),
            AccountMeta::new(*position_pda, false),
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
    nft_mint: &Pubkey,
    key_auth_pda: &Pubkey,
    position_pda: &Pubkey,
    admin_nft_mint: &Pubkey,
    amount: u64,
) -> Instruction {
    let nft_ata = get_ata(admin, nft_mint);
    let (program_pda, pp_pda, _escrow_pda, log_pda, wsol_ata, _nav_sol_ata) = mayflower_addrs(admin_nft_mint);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("borrow");
    data.extend_from_slice(&amount.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(nft_ata, false),
            AccountMeta::new_readonly(*key_auth_pda, false),
            AccountMeta::new(*position_pda, false),
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
    nft_mint: &Pubkey,
    key_auth_pda: &Pubkey,
    position_pda: &Pubkey,
    admin_nft_mint: &Pubkey,
    amount: u64,
) -> Instruction {
    let nft_ata = get_ata(signer, nft_mint);
    let (program_pda, pp_pda, _escrow_pda, log_pda, wsol_ata, _nav_sol_ata) = mayflower_addrs(admin_nft_mint);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("repay");
    data.extend_from_slice(&amount.to_le_bytes());

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(nft_ata, false),
            AccountMeta::new_readonly(*key_auth_pda, false),
            AccountMeta::new(*position_pda, false),
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

fn ix_reinvest(
    signer: &Pubkey,
    nft_mint: &Pubkey,
    key_auth_pda: &Pubkey,
    position_pda: &Pubkey,
    admin_nft_mint: &Pubkey,
) -> Instruction {
    let nft_ata = get_ata(signer, nft_mint);
    let (program_pda, pp_pda, escrow_pda, log_pda, wsol_ata, nav_sol_ata) = mayflower_addrs(admin_nft_mint);
    let (mc_pda, _) = market_config_pda(&DEFAULT_NAV_SOL_MINT);

    let mut data = sighash("reinvest");
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out = 0 (no slippage check)

    Instruction::new_with_bytes(
        program_id(),
        &data,
        vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(nft_ata, false),
            AccountMeta::new_readonly(*key_auth_pda, false),
            AccountMeta::new(*position_pda, false),
            AccountMeta::new_readonly(mc_pda, false),                  // market_config
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

fn position_pda(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[PositionNFT::SEED, mint.as_ref()], &program_id())
}

fn key_auth_pda(position: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[KeyAuthorization::SEED, position.as_ref(), mint.as_ref()],
        &program_id(),
    )
}

fn read_position(svm: &LiteSVM, pda: &Pubkey) -> PositionNFT {
    let account = svm.get_account(pda).unwrap();
    PositionNFT::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn read_key_auth(svm: &LiteSVM, pda: &Pubkey) -> KeyAuthorization {
    let account = svm.get_account(pda).unwrap();
    KeyAuthorization::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn read_market_config(svm: &LiteSVM, pda: &Pubkey) -> MarketConfig {
    let account = svm.get_account(pda).unwrap();
    MarketConfig::try_deserialize(&mut account.data.as_slice()).unwrap()
}

/// Full setup: init protocol + create market config + create position + authorize operator/depositor/keeper.
/// Also plants Mayflower position stubs for CPI.
struct TestHarness {
    admin: Keypair,
    admin_nft_mint: Keypair,
    position_pda: Pubkey,
    admin_key_auth: Pubkey,

    operator: Keypair,
    operator_nft_mint: Pubkey,
    operator_key_auth: Pubkey,

    depositor: Keypair,
    depositor_nft_mint: Pubkey,
    depositor_key_auth: Pubkey,

    keeper: Keypair,
    keeper_nft_mint: Pubkey,
    keeper_key_auth: Pubkey,

    #[allow(dead_code)]
    outsider: Keypair,
}

fn full_setup(svm: &mut LiteSVM) -> TestHarness {
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();

    // Init protocol
    send_tx(svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    // Create MarketConfig for default navSOL market
    send_tx(svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).unwrap();

    // Create position
    let admin_nft_mint = Keypair::new();
    send_tx(
        svm,
        &[ix_create_position(
            &admin.pubkey(),
            &admin_nft_mint.pubkey(),
            500,
        )],
        &[&admin, &admin_nft_mint],
    )
    .unwrap();

    let (pos_pda, _) = position_pda(&admin_nft_mint.pubkey());
    let (admin_ka, _) = key_auth_pda(&pos_pda, &admin_nft_mint.pubkey());

    // Plant Mayflower position stubs (personal_position, escrow, ATAs)
    plant_position_stubs(svm, &admin_nft_mint.pubkey());

    // Patch PositionNFT to set market_config and position_pda (simulates init_mayflower_position)
    patch_position_for_cpi(svm, &admin_nft_mint.pubkey());

    // Authorize operator
    let operator = Keypair::new();
    svm.airdrop(&operator.pubkey(), 5_000_000_000).unwrap();
    let op_mint = Keypair::new();
    send_tx(
        svm,
        &[ix_authorize_key(
            &admin.pubkey(),
            &admin_nft_mint.pubkey(),
            &pos_pda,
            &admin_ka,
            &op_mint.pubkey(),
            &operator.pubkey(),
            1, // Operator
        )],
        &[&admin, &op_mint],
    )
    .unwrap();
    let (op_ka, _) = key_auth_pda(&pos_pda, &op_mint.pubkey());

    // Authorize depositor
    let depositor = Keypair::new();
    svm.airdrop(&depositor.pubkey(), 5_000_000_000).unwrap();
    let dep_mint = Keypair::new();
    send_tx(
        svm,
        &[ix_authorize_key(
            &admin.pubkey(),
            &admin_nft_mint.pubkey(),
            &pos_pda,
            &admin_ka,
            &dep_mint.pubkey(),
            &depositor.pubkey(),
            2, // Depositor
        )],
        &[&admin, &dep_mint],
    )
    .unwrap();
    let (dep_ka, _) = key_auth_pda(&pos_pda, &dep_mint.pubkey());

    // Authorize keeper
    let keeper = Keypair::new();
    svm.airdrop(&keeper.pubkey(), 5_000_000_000).unwrap();
    let keep_mint = Keypair::new();
    send_tx(
        svm,
        &[ix_authorize_key(
            &admin.pubkey(),
            &admin_nft_mint.pubkey(),
            &pos_pda,
            &admin_ka,
            &keep_mint.pubkey(),
            &keeper.pubkey(),
            3, // Keeper
        )],
        &[&admin, &keep_mint],
    )
    .unwrap();
    let (keep_ka, _) = key_auth_pda(&pos_pda, &keep_mint.pubkey());

    // Outsider with no key
    let outsider = Keypair::new();
    svm.airdrop(&outsider.pubkey(), 5_000_000_000).unwrap();

    TestHarness {
        admin,
        admin_nft_mint,
        position_pda: pos_pda,
        admin_key_auth: admin_ka,
        operator,
        operator_nft_mint: op_mint.pubkey(),
        operator_key_auth: op_ka,
        depositor,
        depositor_nft_mint: dep_mint.pubkey(),
        depositor_key_auth: dep_ka,
        keeper,
        keeper_nft_mint: keep_mint.pubkey(),
        keeper_key_auth: keep_ka,
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

    // Transfer admin to new_admin
    let ix = ix_transfer_admin(&admin.pubkey(), &new_admin.pubkey());
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    // Verify the config now has the new admin
    let config = read_protocol_config(&svm);
    assert_eq!(config.admin, new_admin.pubkey());
}

#[test]
fn test_transfer_admin_non_admin_denied() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let non_admin = Keypair::new();
    svm.airdrop(&non_admin.pubkey(), 5_000_000_000).unwrap();

    // Non-admin tries to transfer — should fail
    let ix = ix_transfer_admin(&non_admin.pubkey(), &non_admin.pubkey());
    assert!(send_tx(&mut svm, &[ix], &[&non_admin]).is_err());

    // Verify admin is unchanged
    let config = read_protocol_config(&svm);
    assert_eq!(config.admin, admin.pubkey());
}

#[test]
fn test_transfer_admin_old_admin_rejected_new_admin_works() {
    let (mut svm, admin) = setup();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let new_admin = Keypair::new();
    svm.airdrop(&new_admin.pubkey(), 5_000_000_000).unwrap();

    // Transfer admin
    let ix = ix_transfer_admin(&admin.pubkey(), &new_admin.pubkey());
    send_tx(&mut svm, &[ix], &[&admin]).unwrap();

    // Old admin tries to create market config — should fail
    assert!(send_tx(&mut svm, &[ix_create_market_config(&admin.pubkey())], &[&admin]).is_err());

    // New admin can create market config
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
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
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
        &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 500_000,
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
        &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 250_000,
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
        &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Withdraw: Admin only ----

#[test]
fn test_withdraw_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    // First buy something
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    // Then withdraw
    let ix = ix_withdraw(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 500_000,
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
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 500_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator]).is_err());
}

#[test]
fn test_withdraw_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 500_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

#[test]
fn test_withdraw_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 500_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Borrow: Admin only ----

#[test]
fn test_borrow_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
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
        &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator]).is_err());
}

#[test]
fn test_borrow_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_borrow(
        &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

#[test]
fn test_borrow_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_borrow(
        &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Repay: Admin, Operator, Depositor allowed; Keeper denied ----

#[test]
fn test_repay_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 500_000,
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
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 500_000,
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
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 300_000,
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
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Reinvest: Admin, Operator, Keeper allowed; Depositor denied ----

#[test]
fn test_reinvest_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_reinvest(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(),
    );
    // Reinvest with zero capacity should succeed (early return)
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
}

#[test]
fn test_reinvest_operator_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_reinvest(
        &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(),
    );
    send_tx(&mut svm, &[ix], &[&h.operator]).unwrap();
}

#[test]
fn test_reinvest_keeper_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_reinvest(
        &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(),
    );
    send_tx(&mut svm, &[ix], &[&h.keeper]).unwrap();
}

#[test]
fn test_reinvest_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_reinvest(
        &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

// ---- Authorize/Revoke: Admin only ----

#[test]
fn test_authorize_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_mint = Keypair::new();
    let random_wallet = Keypair::new();
    let ix = ix_authorize_key(
        &h.operator.pubkey(),
        &h.operator_nft_mint,
        &h.position_pda,
        &h.operator_key_auth,
        &new_mint.pubkey(),
        &random_wallet.pubkey(),
        2, // Depositor
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator, &new_mint]).is_err());
}

#[test]
fn test_revoke_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_revoke_key(
        &h.operator.pubkey(),
        &h.operator_nft_mint,
        &h.operator_key_auth,
        &h.position_pda,
        &h.keeper_key_auth,
        &h.keeper_nft_mint,
        None, // no burn needed for a denied test
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator]).is_err());
}

// ---- Cannot create second admin ----

#[test]
fn test_cannot_create_second_admin() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_mint = Keypair::new();
    let random_wallet = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.position_pda,
        &h.admin_key_auth,
        &new_mint.pubkey(),
        &random_wallet.pubkey(),
        0, // Admin role
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin, &new_mint]).is_err());
}

// ---- Cannot revoke admin key ----

#[test]
fn test_cannot_revoke_admin_key() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth,
        &h.position_pda,
        &h.admin_key_auth, // trying to revoke self
        &h.admin_nft_mint.pubkey(),
        Some(&h.admin.pubkey()),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

// ---- Wrong position key rejected ----

#[test]
fn test_wrong_position_key_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Create a second position
    let admin2 = Keypair::new();
    svm.airdrop(&admin2.pubkey(), 10_000_000_000).unwrap();
    let mint2 = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_create_position(&admin2.pubkey(), &mint2.pubkey(), 300)],
        &[&admin2, &mint2],
    )
    .unwrap();
    let (pos2, _) = position_pda(&mint2.pubkey());

    // Try to use admin1's key on position2
    let ix = ix_buy(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &pos2, &mint2.pubkey(), 100_000,
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
fn test_create_position_and_admin_nft() {
    let (mut svm, _) = setup();
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    send_tx(&mut svm, &[ix_init_protocol(&admin.pubkey())], &[&admin]).unwrap();

    let mint_kp = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_create_position(&admin.pubkey(), &mint_kp.pubkey(), 750)],
        &[&admin, &mint_kp],
    )
    .unwrap();

    // Check position
    let (pos_pda, _) = position_pda(&mint_kp.pubkey());
    let pos = read_position(&svm, &pos_pda);
    assert_eq!(pos.admin_nft_mint, mint_kp.pubkey());
    assert_eq!(pos.max_reinvest_spread_bps, 750);
    assert_eq!(pos.deposited_nav, 0);
    assert_eq!(pos.user_debt, 0);
    assert_eq!(pos.protocol_debt, 0);
    assert_eq!(pos.market_config, Pubkey::default());

    // Check admin key auth
    let (ka_pda, _) = key_auth_pda(&pos_pda, &mint_kp.pubkey());
    let ka = read_key_auth(&svm, &ka_pda);
    assert_eq!(ka.position, pos_pda);
    assert_eq!(ka.key_nft_mint, mint_kp.pubkey());
    assert_eq!(ka.role, KeyRole::Admin);

    // Check NFT minted to admin's ATA
    let ata = get_ata(&admin.pubkey(), &mint_kp.pubkey());
    let ata_account = svm.get_account(&ata).unwrap();
    // Amount at offset 64
    let amount = u64::from_le_bytes(ata_account.data[64..72].try_into().unwrap());
    assert_eq!(amount, 1);
}

#[test]
fn test_authorize_and_revoke_key() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Verify operator key auth exists
    let ka = read_key_auth(&svm, &h.operator_key_auth);
    assert_eq!(ka.role, KeyRole::Operator);
    assert_eq!(ka.position, h.position_pda);

    // Verify operator holds NFT
    let ata = get_ata(&h.operator.pubkey(), &h.operator_nft_mint);
    let ata_account = svm.get_account(&ata).unwrap();
    let amount = u64::from_le_bytes(ata_account.data[64..72].try_into().unwrap());
    assert_eq!(amount, 1);

    // Revoke the operator key (NFT held by operator, not admin — no burn)
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth,
        &h.position_pda,
        &h.operator_key_auth,
        &h.operator_nft_mint,
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    // Key auth account should be closed
    assert!(svm.get_account(&h.operator_key_auth).is_none());

    // Operator can no longer buy
    let buy_ix = ix_buy(
        &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[buy_ix], &[&h.operator]).is_err());
}

#[test]
fn test_revoke_burns_nft_when_admin_holds_it() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Authorize a new key NFT to the admin's own wallet (simulates admin
    // reclaiming a key, or issuing one to themselves for testing).
    let extra_mint = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_nft_mint.pubkey(),
            &h.position_pda,
            &h.admin_key_auth,
            &extra_mint.pubkey(),
            &h.admin.pubkey(), // target wallet = admin
            1, // Operator
        )],
        &[&h.admin, &extra_mint],
    )
    .unwrap();
    let (extra_ka, _) = key_auth_pda(&h.position_pda, &extra_mint.pubkey());

    // Verify admin holds the new key NFT
    let ata = get_ata(&h.admin.pubkey(), &extra_mint.pubkey());
    let ata_account = svm.get_account(&ata).unwrap();
    let amount = u64::from_le_bytes(ata_account.data[64..72].try_into().unwrap());
    assert_eq!(amount, 1);

    // Revoke — admin holds the NFT, so it should be burned and ATA closed
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth,
        &h.position_pda,
        &extra_ka,
        &extra_mint.pubkey(),
        Some(&h.admin.pubkey()),
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    // Key auth should be closed
    assert!(svm.get_account(&extra_ka).is_none());

    // ATA should be closed (account gone)
    assert!(svm.get_account(&ata).is_none());

    // Mint supply should be 0 (token was burned)
    let mint_account = svm.get_account(&extra_mint.pubkey()).unwrap();
    let supply = u64::from_le_bytes(mint_account.data[36..44].try_into().unwrap());
    assert_eq!(supply, 0);
}

#[test]
fn test_revoke_skips_burn_when_admin_not_holder() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Operator holds the NFT, not admin. Burn should be skipped.
    let ata_before = get_ata(&h.operator.pubkey(), &h.operator_nft_mint);
    let ata_account_before = svm.get_account(&ata_before).unwrap();
    let amount_before = u64::from_le_bytes(ata_account_before.data[64..72].try_into().unwrap());
    assert_eq!(amount_before, 1);

    // Pass None for the ATA — admin doesn't hold the NFT, so we skip burn.
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth,
        &h.position_pda,
        &h.operator_key_auth,
        &h.operator_nft_mint,
        None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    // Key auth should be closed
    assert!(svm.get_account(&h.operator_key_auth).is_none());

    // But the NFT ATA should still exist with amount=1 (burn was skipped)
    let ata_account_after = svm.get_account(&ata_before).unwrap();
    let amount_after = u64::from_le_bytes(ata_account_after.data[64..72].try_into().unwrap());
    assert_eq!(amount_after, 1);
}

#[test]
fn test_multiple_keys_per_position() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let admin_ka = read_key_auth(&svm, &h.admin_key_auth);
    assert_eq!(admin_ka.role, KeyRole::Admin);

    let op_ka = read_key_auth(&svm, &h.operator_key_auth);
    assert_eq!(op_ka.role, KeyRole::Operator);

    let dep_ka = read_key_auth(&svm, &h.depositor_key_auth);
    assert_eq!(dep_ka.role, KeyRole::Depositor);

    let keep_ka = read_key_auth(&svm, &h.keeper_key_auth);
    assert_eq!(keep_ka.role, KeyRole::Keeper);

    assert_eq!(op_ka.position, h.position_pda);
    assert_eq!(dep_ka.position, h.position_pda);
    assert_eq!(keep_ka.position, h.position_pda);
}

#[test]
fn test_invalid_role_value_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let new_mint = Keypair::new();
    let target = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.position_pda,
        &h.admin_key_auth,
        &new_mint.pubkey(),
        &target.pubkey(),
        99, // Invalid role
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin, &new_mint]).is_err());
}

// ---- Accounting edge cases ----

#[test]
fn test_buy_zero_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_buy(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 0,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

#[test]
fn test_withdraw_more_than_deposited_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_withdraw(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 2_000_000_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

#[test]
fn test_repay_more_than_debt_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_borrow(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 1_000_000_000,
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_repay(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 2_000_000_000,
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

    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth,
        &h.position_pda,
        &h.operator_key_auth,
        &h.operator_nft_mint,
        None, // operator holds it, no burn
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    assert!(svm.get_account(&h.operator_key_auth).is_none());

    let buy_ix = ix_buy(
        &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[buy_ix], &[&h.operator]).is_err());

    let new_op_mint = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.position_pda,
        &h.admin_key_auth,
        &new_op_mint.pubkey(),
        &h.operator.pubkey(),
        1,
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &new_op_mint]).unwrap();

    let (new_op_ka, _) = key_auth_pda(&h.position_pda, &new_op_mint.pubkey());
    let buy_ix = ix_buy(
        &h.operator.pubkey(), &new_op_mint.pubkey(),
        &new_op_ka, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.operator]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 100_000);
}

#[test]
fn test_theft_recovery_mass_revoke_and_reissue() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    for (ka_pda, nft_mint) in [
        (h.operator_key_auth, h.operator_nft_mint),
        (h.depositor_key_auth, h.depositor_nft_mint),
        (h.keeper_key_auth, h.keeper_nft_mint),
    ] {
        let ix = ix_revoke_key(
            &h.admin.pubkey(),
            &h.admin_nft_mint.pubkey(),
            &h.admin_key_auth,
            &h.position_pda,
            &ka_pda,
            &nft_mint,
            None, // holders are not the admin, skip burn
        );
        send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    }

    assert!(svm.get_account(&h.operator_key_auth).is_none());
    assert!(svm.get_account(&h.depositor_key_auth).is_none());
    assert!(svm.get_account(&h.keeper_key_auth).is_none());

    let buy_ix = ix_buy(
        &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 500_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();

    let new_operator = Keypair::new();
    svm.airdrop(&new_operator.pubkey(), 5_000_000_000).unwrap();
    let new_op_mint = Keypair::new();
    send_tx(
        &mut svm,
        &[ix_authorize_key(
            &h.admin.pubkey(),
            &h.admin_nft_mint.pubkey(),
            &h.position_pda,
            &h.admin_key_auth,
            &new_op_mint.pubkey(),
            &new_operator.pubkey(),
            1,
        )],
        &[&h.admin, &new_op_mint],
    )
    .unwrap();

    let (new_op_ka, _) = key_auth_pda(&h.position_pda, &new_op_mint.pubkey());
    let buy_ix = ix_buy(
        &new_operator.pubkey(), &new_op_mint.pubkey(),
        &new_op_ka, &h.position_pda, &h.admin_nft_mint.pubkey(), 200_000,
    );
    send_tx(&mut svm, &[buy_ix], &[&new_operator]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 700_000);
}

#[test]
fn test_attacker_cannot_use_others_key_auth() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 5_000_000_000).unwrap();

    let ix = ix_buy(
        &attacker.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&attacker]).is_err());
}

#[test]
fn test_privilege_escalation_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let ix = ix_withdraw(
        &h.depositor.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, &h.admin_nft_mint.pubkey(), 100_000,
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}
