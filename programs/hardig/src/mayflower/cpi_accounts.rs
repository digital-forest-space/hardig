use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};

use super::constants::*;

/// Market-specific addresses extracted from a MarketConfig account.
/// Passed to CPI builders so they don't embed constants.
pub struct MarketAddresses {
    pub nav_mint: Pubkey,
    pub base_mint: Pubkey,
    pub market_group: Pubkey,
    pub market_meta: Pubkey,
    pub mayflower_market: Pubkey,
    pub market_base_vault: Pubkey,
    pub market_nav_vault: Pubkey,
    pub fee_vault: Pubkey,
}

/// Derive the PersonalPosition PDA for a given owner and market_meta.
/// Seeds: ["personal_position", market_meta, owner]
pub fn derive_personal_position(owner: &Pubkey, market_meta: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PERSONAL_POSITION_SEED,
            market_meta.as_ref(),
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

/// Derive the Mayflower liq_vault_main authority PDA.
/// Seeds: ["liq_vault_main", market_meta]
pub fn derive_liq_vault_main(market_meta: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[LIQ_VAULT_MAIN_SEED, market_meta.as_ref()],
        &MAYFLOWER_PROGRAM_ID,
    )
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
    market: &MarketAddresses,
) -> Instruction {
    let (log_account, _) = derive_log_account();

    Instruction {
        program_id: MAYFLOWER_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(payer, true),                          // 0: payer (signer)
            AccountMeta::new_readonly(owner, false),                // 1: owner
            AccountMeta::new_readonly(market.market_meta, false),   // 2: marketMetadata
            AccountMeta::new_readonly(market.nav_mint, false),      // 3: navToken mint
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
    market: &MarketAddresses,
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
            AccountMeta::new_readonly(market.market_group, false),  // 2: marketGroup
            AccountMeta::new_readonly(market.market_meta, false),   // 3: marketMetadata
            AccountMeta::new(market.mayflower_market, false),       // 4: mayflowerMarket
            AccountMeta::new(personal_position, false),             // 5: personalPosition
            AccountMeta::new(user_shares, false),                   // 6: userShares
            AccountMeta::new(market.nav_mint, false),               // 7: navToken mint
            AccountMeta::new_readonly(market.base_mint, false),     // 8: baseMint
            AccountMeta::new(user_nav_sol_ata, false),              // 9: userNavSolATA
            AccountMeta::new(user_wsol_ata, false),                 // 10: userWsolATA
            AccountMeta::new(market.market_base_vault, false),      // 11: marketBaseVault
            AccountMeta::new(market.market_nav_vault, false),       // 12: marketNavVault
            AccountMeta::new(market.fee_vault, false),              // 13: feeVault
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
/// Sells navSOL for SOL. NOTE: sell layout differs from buy!
/// Positions 6-13 are rearranged: vaults first, then mints, then user accounts.
pub fn build_sell_ix(
    user_wallet: Pubkey,
    personal_position: Pubkey,
    user_shares: Pubkey,
    user_nav_sol_ata: Pubkey,
    user_wsol_ata: Pubkey,
    input_amount: u64,
    min_output: u64,
    market: &MarketAddresses,
) -> Instruction {
    let (log_account, _) = derive_log_account();

    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&IX_SELL);
    data.extend_from_slice(&input_amount.to_le_bytes());
    data.extend_from_slice(&min_output.to_le_bytes());

    Instruction {
        program_id: MAYFLOWER_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(user_wallet, true),                     // 0: userWallet (signer)
            AccountMeta::new_readonly(MAYFLOWER_TENANT, false),      // 1: tenant
            AccountMeta::new_readonly(market.market_group, false),   // 2: marketGroup
            AccountMeta::new_readonly(market.market_meta, false),    // 3: marketMetadata
            AccountMeta::new(market.mayflower_market, false),        // 4: mayflowerMarket
            AccountMeta::new(personal_position, false),              // 5: personalPosition
            AccountMeta::new(market.market_base_vault, false),       // 6: marketBaseVault
            AccountMeta::new(market.market_nav_vault, false),        // 7: marketNavVault
            AccountMeta::new(market.fee_vault, false),               // 8: feeVault
            AccountMeta::new(market.nav_mint, false),                // 9: navMint
            AccountMeta::new_readonly(market.base_mint, false),      // 10: baseMint
            AccountMeta::new(user_wsol_ata, false),                  // 11: userWsolATA
            AccountMeta::new(user_nav_sol_ata, false),               // 12: userNavSolATA
            AccountMeta::new(user_shares, false),                    // 13: userShares
            AccountMeta::new_readonly(anchor_spl::token::ID, false), // 14: Token Program
            AccountMeta::new_readonly(anchor_spl::token::ID, false), // 15: Token Program (dup)
            AccountMeta::new(log_account, false),                    // 16: logAccount
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),  // 17: Mayflower program
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
    market: &MarketAddresses,
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
            AccountMeta::new_readonly(market.market_group, false),  // 2: marketGroup
            AccountMeta::new_readonly(market.market_meta, false),   // 3: marketMetadata
            AccountMeta::new(market.market_base_vault, false),      // 4: marketBaseVault
            AccountMeta::new(market.market_nav_vault, false),       // 5: marketNavVault
            AccountMeta::new(market.fee_vault, false),              // 6: feeVault
            AccountMeta::new_readonly(market.base_mint, false),     // 7: baseMint
            AccountMeta::new(user_base_token_ata, false),           // 8: userBaseTokenATA
            AccountMeta::new(market.mayflower_market, false),       // 9: mayflowerMarket
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
/// Account layout (10 accounts):
///   0: userWallet (signer, writable)
///   1: marketMetadata (readonly)
///   2: mayflowerMarket (writable)
///   3: personalPosition (writable)
///   4: baseMint (readonly)
///   5: userBaseTokenATA (writable)
///   6: marketBaseVault (writable)
///   7: tokenProgram (readonly)
///   8: logAccount (writable)
///   9: mayflowerProgram (readonly)
pub fn build_repay_ix(
    user_wallet: Pubkey,
    personal_position: Pubkey,
    user_base_token_ata: Pubkey,
    repay_amount: u64,
    market: &MarketAddresses,
) -> Instruction {
    let (log_account, _) = derive_log_account();

    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&IX_REPAY);
    data.extend_from_slice(&repay_amount.to_le_bytes());

    Instruction {
        program_id: MAYFLOWER_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(user_wallet, true),                     // 0: userWallet
            AccountMeta::new_readonly(market.market_meta, false),    // 1: marketMetadata
            AccountMeta::new(market.mayflower_market, false),        // 2: mayflowerMarket
            AccountMeta::new(personal_position, false),              // 3: personalPosition
            AccountMeta::new_readonly(market.base_mint, false),      // 4: baseMint
            AccountMeta::new(user_base_token_ata, false),            // 5: userBaseTokenATA
            AccountMeta::new(market.market_base_vault, false),       // 6: marketBaseVault
            AccountMeta::new_readonly(anchor_spl::token::ID, false), // 7: tokenProgram
            AccountMeta::new(log_account, false),                    // 8: logAccount
            AccountMeta::new_readonly(MAYFLOWER_PROGRAM_ID, false),  // 9: mayflowerProgram
        ],
        data,
    }
}
