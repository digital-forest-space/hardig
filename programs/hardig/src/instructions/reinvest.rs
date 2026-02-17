use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Reinvest<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The signer's key NFT token account.
    pub key_nft_ata: Account<'info, TokenAccount>,

    /// The signer's KeyAuthorization.
    #[account(
        constraint = key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub key_auth: Account<'info, KeyAuthorization>,

    /// The position to reinvest for.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Reinvest>) -> Result<()> {
    validate_key(
        &ctx.accounts.signer,
        &ctx.accounts.key_nft_ata,
        &ctx.accounts.key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin, KeyRole::Operator, KeyRole::Keeper],
    )?;

    if ctx.accounts.key_auth.role == KeyRole::Admin {
        ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;
    }

    // Full reinvest flow requires Mayflower accounts in remaining_accounts.
    // Layout:
    //   [0]  program_pda (authority)
    //   [1]  mayflower_market (to read floor price)
    //   [2]  personal_position (Mayflower)
    //   [3]  user_shares
    //   [4]  user_nav_sol_ata
    //   [5]  user_wsol_ata
    //   [6]  user_base_token_ata (wSOL for borrow)
    //   [7]  tenant
    //   [8]  market_group
    //   [9]  market_meta
    //   [10] market_base_vault
    //   [11] market_nav_vault
    //   [12] fee_vault
    //   [13] nav_sol_mint
    //   [14] mayflower_program
    //   [15] token_program
    //   [16] log_account
    if ctx.remaining_accounts.len() >= 15 {
        let market_account = &ctx.remaining_accounts[1];
        let market_data = market_account.try_borrow_data()?;

        let floor_price = mayflower::read_floor_price(&market_data)?;

        let personal_position = &ctx.remaining_accounts[2];
        let pp_data = personal_position.try_borrow_data()?;

        let deposited_shares = mayflower::read_deposited_shares(&pp_data)?;
        let current_debt = mayflower::read_debt(&pp_data)?;

        let capacity =
            mayflower::calculate_borrow_capacity(deposited_shares, floor_price, current_debt)?;

        if capacity == 0 {
            return Ok(());
        }

        // Check spread constraint
        // TODO: Compare market price vs floor price to enforce max_reinvest_spread_bps

        // Drop borrows before CPI
        drop(market_data);
        drop(pp_data);

        // Step 1: Borrow the available capacity
        let borrow_accounts = &ctx.remaining_accounts[..]; // reuse same account slice
        let program_pda = &borrow_accounts[0];
        let borrow_ix = mayflower::build_borrow_ix(
            program_pda.key(),
            personal_position.key(),
            borrow_accounts[6].key(), // user_base_token_ata
            capacity,
        );

        let (_, bump) = Pubkey::find_program_address(&[b"authority"], &crate::ID);
        let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

        invoke_signed(&borrow_ix, ctx.remaining_accounts, signer_seeds)?;

        // Step 2: Read actual wSOL balance after borrow (fees were deducted)
        let wsol_ata = &ctx.remaining_accounts[6]; // user_base_token_ata = wSOL ATA
        let wsol_data = wsol_ata.try_borrow_data()?;
        let actual_amount = if wsol_data.len() >= 72 {
            u64::from_le_bytes(wsol_data[64..72].try_into().unwrap())
        } else {
            0
        };
        drop(wsol_data);

        if actual_amount == 0 {
            return Ok(());
        }

        // Step 3: Buy navSOL with the actual borrowed amount (net of fees)
        let buy_ix = mayflower::build_buy_ix(
            program_pda.key(),
            personal_position.key(),
            borrow_accounts[3].key(), // user_shares
            borrow_accounts[4].key(), // user_nav_sol_ata
            borrow_accounts[5].key(), // user_wsol_ata
            actual_amount,
            0, // min_output
        );

        invoke_signed(&buy_ix, ctx.remaining_accounts, signer_seeds)?;

        // Update accounting with actual amounts
        ctx.accounts.position.protocol_debt = ctx
            .accounts
            .position
            .protocol_debt
            .checked_add(capacity)
            .ok_or(HardigError::BorrowCapacityExceeded)?;

        ctx.accounts.position.deposited_nav = ctx
            .accounts
            .position
            .deposited_nav
            .checked_add(actual_amount)
            .ok_or(HardigError::InsufficientFunds)?;
    }

    Ok(())
}
