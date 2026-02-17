use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};

use super::constants::*;

/// Derive the PersonalPosition PDA for a given owner.
/// Seeds: ["personal_position", MARKET_META, owner]
pub fn derive_personal_position(owner: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PERSONAL_POSITION_SEED,
            MARKET_META.as_ref(),
            owner.as_ref(),
        ],
        &MAYFLOWER_PROGRAM_ID,
    )
}

/// Derive the PersonalPosition escrow (userShares token account) PDA.
/// Seeds: ["personal_position_escrow", personal_position]
pub fn derive_personal_position_escrow(personal_position: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PERSONAL_POSITION_ESCROW_SEED,
            personal_position.as_ref(),
        ],
        &MAYFLOWER_PROGRAM_ID,
    )
}

/// Derive the Mayflower log account PDA.
pub fn derive_log_account() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[LOG_SEED], &MAYFLOWER_PROGRAM_ID)
}

/// Build the `init_personal_position` instruction for Mayflower.
///
/// `payer` signs and pays rent. `owner` is stored as the position owner
/// (in our case, a PDA owned by this program).
pub fn build_init_personal_position_ix(
    payer: Pubkey,
    owner: Pubkey,
    personal_position: Pubkey,
    user_shares: Pubkey,
) -> Instruction {
    let (log_account, _) = derive_log_account();

    Instruction {
        program_id: MAYFLOWER_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(payer, true),                          // 0: payer (signer)
            AccountMeta::new_readonly(owner, false),                // 1: owner
            AccountMeta::new_readonly(MARKET_META, false),          // 2: marketMetadata
            AccountMeta::new_readonly(NAV_SOL_MINT, false),         // 3: navToken mint
            AccountMeta::new(personal_position, false),             // 4: personalPosition PDA
            AccountMeta::new(user_shares, false),                   // 5: userShares PDA
            AccountMeta::new_readonly(anchor_spl::token::ID, false), // 6: Token Program
            AccountMeta::new_readonly(anchor_lang::system_program::ID, false), // 7: System Program
            AccountMeta::new(log_account, false),                   // 8: log account
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // 9: Mayflower program
        ],
        data: IX_INIT_PERSONAL_POSITION.to_vec(),
    }
}

/// Build the `buy` (BuyWithExactCashInAndDeposit) instruction for Mayflower.
///
/// `user_wallet` is the signer — for CPI this will be our program PDA.
pub fn build_buy_ix(
    user_wallet: Pubkey,
    personal_position: Pubkey,
    user_shares: Pubkey,
    user_nav_sol_ata: Pubkey,
    user_wsol_ata: Pubkey,
    input_amount: u64,
    min_output: u64,
) -> Instruction {
    let (log_account, _) = derive_log_account();

    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&IX_BUY);
    data.extend_from_slice(&input_amount.to_le_bytes());
    data.extend_from_slice(&min_output.to_le_bytes());

    Instruction {
        program_id: MAYFLOWER_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(user_wallet, true),                    // 0: userWallet (signer)
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),     // 1: tenant
            AccountMeta::new_readonly(MARKET_GROUP, false),         // 2: marketGroup
            AccountMeta::new_readonly(MARKET_META, false),          // 3: marketMetadata
            AccountMeta::new(MAYFLOWER_MARKET, false),              // 4: mayflowerMarket
            AccountMeta::new(personal_position, false),             // 5: personalPosition
            AccountMeta::new(user_shares, false),                   // 6: userShares
            AccountMeta::new(NAV_SOL_MINT, false),                  // 7: navToken mint
            AccountMeta::new_readonly(WSOL_MINT, false),            // 8: baseMint
            AccountMeta::new(user_nav_sol_ata, false),              // 9: userNavSolATA
            AccountMeta::new(user_wsol_ata, false),                 // 10: userWsolATA
            AccountMeta::new(MARKET_BASE_VAULT, false),             // 11: marketBaseVault
            AccountMeta::new(MARKET_NAV_VAULT, false),              // 12: marketNavVault
            AccountMeta::new(FEE_VAULT, false),                     // 13: feeVault
            AccountMeta::new_readonly(anchor_spl::token::ID, false), // 14: Token Program
            AccountMeta::new_readonly(anchor_spl::token::ID, false), // 15: Token Program (dup)
            AccountMeta::new(log_account, false),                   // 16: logAccount
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // 17: Mayflower program
        ],
        data,
    }
}

/// Build the `sell` (SellWithExactTokenIn) instruction for Mayflower.
///
/// Mirror of buy — sells navSOL for SOL.
/// TODO: IX_SELL discriminator must be derived from Mayflower IDL.
pub fn build_sell_ix(
    user_wallet: Pubkey,
    personal_position: Pubkey,
    user_shares: Pubkey,
    user_nav_sol_ata: Pubkey,
    user_wsol_ata: Pubkey,
    input_amount: u64,
    min_output: u64,
) -> Instruction {
    let (log_account, _) = derive_log_account();

    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&IX_SELL);
    data.extend_from_slice(&input_amount.to_le_bytes());
    data.extend_from_slice(&min_output.to_le_bytes());

    Instruction {
        program_id: MAYFLOWER_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(user_wallet, true),
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),
            AccountMeta::new_readonly(MARKET_GROUP, false),
            AccountMeta::new_readonly(MARKET_META, false),
            AccountMeta::new(MAYFLOWER_MARKET, false),
            AccountMeta::new(personal_position, false),
            AccountMeta::new(user_shares, false),
            AccountMeta::new(NAV_SOL_MINT, false),
            AccountMeta::new_readonly(WSOL_MINT, false),
            AccountMeta::new(user_nav_sol_ata, false),
            AccountMeta::new(user_wsol_ata, false),
            AccountMeta::new(MARKET_BASE_VAULT, false),
            AccountMeta::new(MARKET_NAV_VAULT, false),
            AccountMeta::new(FEE_VAULT, false),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new(log_account, false),
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),
        ],
        data,
    }
}

/// Build the `borrow` instruction for Mayflower.
pub fn build_borrow_ix(
    user_wallet: Pubkey,
    personal_position: Pubkey,
    user_base_token_ata: Pubkey,
    borrow_amount: u64,
) -> Instruction {
    let (log_account, _) = derive_log_account();

    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&IX_BORROW);
    data.extend_from_slice(&borrow_amount.to_le_bytes());

    Instruction {
        program_id: MAYFLOWER_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(user_wallet, true),                    // 0: userWallet (signer)
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),     // 1: tenant
            AccountMeta::new_readonly(MARKET_GROUP, false),         // 2: marketGroup
            AccountMeta::new_readonly(MARKET_META, false),          // 3: marketMetadata
            AccountMeta::new(MARKET_BASE_VAULT, false),             // 4: marketBaseVault
            AccountMeta::new(MARKET_NAV_VAULT, false),              // 5: marketNavVault
            AccountMeta::new(FEE_VAULT, false),                     // 6: feeVault
            AccountMeta::new_readonly(WSOL_MINT, false),            // 7: baseMint
            AccountMeta::new(user_base_token_ata, false),           // 8: userBaseTokenATA
            AccountMeta::new(MAYFLOWER_MARKET, false),              // 9: mayflowerMarket
            AccountMeta::new(personal_position, false),             // 10: personalPosition
            AccountMeta::new_readonly(anchor_spl::token::ID, false), // 11: Token Program
            AccountMeta::new(log_account, false),                   // 12: logAccount
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false), // 13: Mayflower program
        ],
        data,
    }
}

/// Build the `repay` instruction for Mayflower.
///
/// Mirror of borrow — transfers SOL back to repay debt.
pub fn build_repay_ix(
    user_wallet: Pubkey,
    personal_position: Pubkey,
    user_base_token_ata: Pubkey,
    repay_amount: u64,
) -> Instruction {
    let (log_account, _) = derive_log_account();

    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&IX_REPAY);
    data.extend_from_slice(&repay_amount.to_le_bytes());

    Instruction {
        program_id: MAYFLOWER_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(user_wallet, true),
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),
            AccountMeta::new_readonly(MARKET_GROUP, false),
            AccountMeta::new_readonly(MARKET_META, false),
            AccountMeta::new(MARKET_BASE_VAULT, false),
            AccountMeta::new(MARKET_NAV_VAULT, false),
            AccountMeta::new(FEE_VAULT, false),
            AccountMeta::new_readonly(WSOL_MINT, false),
            AccountMeta::new(user_base_token_ata, false),
            AccountMeta::new(MAYFLOWER_MARKET, false),
            AccountMeta::new(personal_position, false),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new(log_account, false),
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),
        ],
        data,
    }
}
