use litesvm::LiteSVM;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};

use anchor_lang::AccountDeserialize;
use hardig::state::{KeyAuthorization, KeyRole, PositionNFT, ProtocolConfig};

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
// Setup
// ---------------------------------------------------------------------------

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let program_bytes = std::fs::read("../../target/deploy/hardig.so")
        .expect("Run `anchor build` first");
    let _ = svm.add_program(program_id(), &program_bytes);

    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    (svm, admin)
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
        Pubkey::find_program_address(&[b"authority"], &program_id());

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
        Pubkey::find_program_address(&[b"authority"], &program_id());

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

fn ix_revoke_key(
    admin: &Pubkey,
    admin_nft_mint: &Pubkey,
    admin_key_auth: &Pubkey,
    position_pda: &Pubkey,
    target_key_auth: &Pubkey,
) -> Instruction {
    let admin_nft_ata = get_ata(admin, admin_nft_mint);
    Instruction::new_with_bytes(
        program_id(),
        &sighash("revoke_key"),
        vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new_readonly(admin_nft_ata, false),
            AccountMeta::new_readonly(*admin_key_auth, false),
            AccountMeta::new_readonly(*position_pda, false),
            AccountMeta::new(*target_key_auth, false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ],
    )
}

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
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
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

/// Full setup: init protocol + create position + authorize operator/depositor/keeper.
/// Returns all the keypairs and PDAs needed for permission testing.
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
// #16: Permission matrix tests
// ===========================================================================

// ---- Buy: Admin, Operator, Depositor allowed; Keeper denied ----

#[test]
fn test_buy_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 1_000_000);
}

#[test]
fn test_buy_operator_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "buy", &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, Some(500_000),
    );
    send_tx(&mut svm, &[ix], &[&h.operator]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 500_000);
}

#[test]
fn test_buy_depositor_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "buy", &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, Some(250_000),
    );
    send_tx(&mut svm, &[ix], &[&h.depositor]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 250_000);
}

#[test]
fn test_buy_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "buy", &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Withdraw: Admin only ----

#[test]
fn test_withdraw_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    // First buy something
    let buy_ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    // Then withdraw
    let ix = ix_role_gated(
        "withdraw", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(500_000),
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 500_000);
}

#[test]
fn test_withdraw_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    // Buy first
    let buy_ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    // Operator tries to withdraw
    let ix = ix_role_gated(
        "withdraw", &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, Some(500_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator]).is_err());
}

#[test]
fn test_withdraw_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_role_gated(
        "withdraw", &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, Some(500_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

#[test]
fn test_withdraw_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let buy_ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    let ix = ix_role_gated(
        "withdraw", &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, Some(500_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Borrow: Admin only ----

#[test]
fn test_borrow_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "borrow", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 1_000_000);
}

#[test]
fn test_borrow_operator_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "borrow", &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.operator]).is_err());
}

#[test]
fn test_borrow_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "borrow", &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}

#[test]
fn test_borrow_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "borrow", &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Repay: Admin, Operator, Depositor allowed; Keeper denied ----

#[test]
fn test_repay_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    // Borrow first
    let borrow_ix = ix_role_gated(
        "borrow", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    // Repay
    let ix = ix_role_gated(
        "repay", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(500_000),
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 500_000);
}

#[test]
fn test_repay_operator_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_role_gated(
        "borrow", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_role_gated(
        "repay", &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, Some(500_000),
    );
    send_tx(&mut svm, &[ix], &[&h.operator]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 500_000);
}

#[test]
fn test_repay_depositor_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_role_gated(
        "borrow", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_role_gated(
        "repay", &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, Some(300_000),
    );
    send_tx(&mut svm, &[ix], &[&h.depositor]).unwrap();
    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.user_debt, 700_000);
}

#[test]
fn test_repay_keeper_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let borrow_ix = ix_role_gated(
        "borrow", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000),
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    let ix = ix_role_gated(
        "repay", &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.keeper]).is_err());
}

// ---- Reinvest: Admin, Operator, Keeper allowed; Depositor denied ----

#[test]
fn test_reinvest_admin_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "reinvest", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, None,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
}

#[test]
fn test_reinvest_operator_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "reinvest", &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, None,
    );
    send_tx(&mut svm, &[ix], &[&h.operator]).unwrap();
}

#[test]
fn test_reinvest_keeper_ok() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "reinvest", &h.keeper.pubkey(), &h.keeper_nft_mint,
        &h.keeper_key_auth, &h.position_pda, None,
    );
    send_tx(&mut svm, &[ix], &[&h.keeper]).unwrap();
}

#[test]
fn test_reinvest_depositor_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    let ix = ix_role_gated(
        "reinvest", &h.depositor.pubkey(), &h.depositor_nft_mint,
        &h.depositor_key_auth, &h.position_pda, None,
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
    let ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &pos2, Some(100_000),
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

    // Revoke the operator key
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth,
        &h.position_pda,
        &h.operator_key_auth,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    // Key auth account should be closed
    assert!(svm.get_account(&h.operator_key_auth).is_none());

    // Operator can no longer buy
    let buy_ix = ix_role_gated(
        "buy", &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[buy_ix], &[&h.operator]).is_err());
}

#[test]
fn test_multiple_keys_per_position() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // We already have 4 keys (admin + operator + depositor + keeper).
    // Verify all key auths exist and have correct roles.
    let admin_ka = read_key_auth(&svm, &h.admin_key_auth);
    assert_eq!(admin_ka.role, KeyRole::Admin);

    let op_ka = read_key_auth(&svm, &h.operator_key_auth);
    assert_eq!(op_ka.role, KeyRole::Operator);

    let dep_ka = read_key_auth(&svm, &h.depositor_key_auth);
    assert_eq!(dep_ka.role, KeyRole::Depositor);

    let keep_ka = read_key_auth(&svm, &h.keeper_key_auth);
    assert_eq!(keep_ka.role, KeyRole::Keeper);

    // All point to the same position
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
    let ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(0),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

#[test]
fn test_withdraw_more_than_deposited_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    // Buy 1 SOL
    let buy_ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000_000),
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();
    // Try to withdraw 2 SOL
    let ix = ix_role_gated(
        "withdraw", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(2_000_000_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

#[test]
fn test_repay_more_than_debt_rejected() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);
    // Borrow 1 SOL
    let borrow_ix = ix_role_gated(
        "borrow", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(1_000_000_000),
    );
    send_tx(&mut svm, &[borrow_ix], &[&h.admin]).unwrap();
    // Try to repay 2 SOL
    let ix = ix_role_gated(
        "repay", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(2_000_000_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.admin]).is_err());
}

// ===========================================================================
// #18: Theft recovery scenario tests
// ===========================================================================

/// Scenario: Operator key NFT is stolen (transferred to attacker).
/// Admin revokes the stolen key, re-issues a new one to the legitimate operator.
#[test]
fn test_theft_recovery_operator_key_stolen() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Step 1: Admin revokes the compromised operator key
    let ix = ix_revoke_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth,
        &h.position_pda,
        &h.operator_key_auth,
    );
    send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();

    // The key auth is now closed
    assert!(svm.get_account(&h.operator_key_auth).is_none());

    // Step 2: The old operator key can no longer be used
    let buy_ix = ix_role_gated(
        "buy", &h.operator.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[buy_ix], &[&h.operator]).is_err());

    // Step 3: Admin re-issues a new key to the same operator wallet
    let new_op_mint = Keypair::new();
    let ix = ix_authorize_key(
        &h.admin.pubkey(),
        &h.admin_nft_mint.pubkey(),
        &h.position_pda,
        &h.admin_key_auth,
        &new_op_mint.pubkey(),
        &h.operator.pubkey(),
        1, // Operator
    );
    send_tx(&mut svm, &[ix], &[&h.admin, &new_op_mint]).unwrap();

    // Step 4: The new key works
    let (new_op_ka, _) = key_auth_pda(&h.position_pda, &new_op_mint.pubkey());
    let buy_ix = ix_role_gated(
        "buy", &h.operator.pubkey(), &new_op_mint.pubkey(),
        &new_op_ka, &h.position_pda, Some(100_000),
    );
    send_tx(&mut svm, &[buy_ix], &[&h.operator]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 100_000);
}

/// Scenario: Multiple keys stolen at once — admin revokes all, re-issues fresh set.
#[test]
fn test_theft_recovery_mass_revoke_and_reissue() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Revoke all non-admin keys
    for ka_pda in [
        h.operator_key_auth,
        h.depositor_key_auth,
        h.keeper_key_auth,
    ] {
        let ix = ix_revoke_key(
            &h.admin.pubkey(),
            &h.admin_nft_mint.pubkey(),
            &h.admin_key_auth,
            &h.position_pda,
            &ka_pda,
        );
        send_tx(&mut svm, &[ix], &[&h.admin]).unwrap();
    }

    // All key auths closed
    assert!(svm.get_account(&h.operator_key_auth).is_none());
    assert!(svm.get_account(&h.depositor_key_auth).is_none());
    assert!(svm.get_account(&h.keeper_key_auth).is_none());

    // Admin key still works
    let buy_ix = ix_role_gated(
        "buy", &h.admin.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(500_000),
    );
    send_tx(&mut svm, &[buy_ix], &[&h.admin]).unwrap();

    // Re-issue fresh key to new operator wallet
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

    // New operator can act
    let (new_op_ka, _) = key_auth_pda(&h.position_pda, &new_op_mint.pubkey());
    let buy_ix = ix_role_gated(
        "buy", &new_operator.pubkey(), &new_op_mint.pubkey(),
        &new_op_ka, &h.position_pda, Some(200_000),
    );
    send_tx(&mut svm, &[buy_ix], &[&new_operator]).unwrap();

    let pos = read_position(&svm, &h.position_pda);
    assert_eq!(pos.deposited_nav, 700_000); // 500k + 200k
}

/// Scenario: Attacker has a random keypair and tries to use someone else's key auth.
/// The validate_key check rejects because NFT ATA owner != signer.
#[test]
fn test_attacker_cannot_use_others_key_auth() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 5_000_000_000).unwrap();

    // Attacker signs, passes operator's key auth but their own derived ATA has no token.
    let ix = ix_role_gated(
        "buy", &attacker.pubkey(), &h.operator_nft_mint,
        &h.operator_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&attacker]).is_err());
}

/// Scenario: Depositor tries to escalate privileges by passing admin's key auth.
#[test]
fn test_privilege_escalation_denied() {
    let (mut svm, _) = setup();
    let h = full_setup(&mut svm);

    // Depositor signs, but passes the admin's NFT mint / key auth.
    // Depositor's ATA for admin NFT mint won't have a token.
    let ix = ix_role_gated(
        "withdraw", &h.depositor.pubkey(), &h.admin_nft_mint.pubkey(),
        &h.admin_key_auth, &h.position_pda, Some(100_000),
    );
    assert!(send_tx(&mut svm, &[ix], &[&h.depositor]).is_err());
}
