use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Buy<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The signer's key NFT token account.
    pub key_nft_ata: Account<'info, TokenAccount>,

    /// The signer's KeyAuthorization.
    #[account(
        constraint = key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub key_auth: Account<'info, KeyAuthorization>,

    /// The position to buy navSOL for.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Buy>, amount: u64) -> Result<()> {
    validate_key(
        &ctx.accounts.signer,
        &ctx.accounts.key_nft_ata,
        &ctx.accounts.key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin, KeyRole::Operator, KeyRole::Depositor],
    )?;

    require!(amount > 0, HardigError::InsufficientFunds);

    if ctx.accounts.key_auth.role == KeyRole::Admin {
        ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;
    }

    // If Mayflower accounts are passed via remaining_accounts, do CPI.
    // Otherwise, accounting-only mode (for testing).
    if ctx.remaining_accounts.len() >= 14 {
        do_mayflower_buy(ctx.remaining_accounts, amount)?;
    }

    ctx.accounts.position.deposited_nav = ctx
        .accounts
        .position
        .deposited_nav
        .checked_add(amount)
        .ok_or(HardigError::InsufficientFunds)?;

    Ok(())
}

/// CPI into Mayflower BuyWithExactCashInAndDeposit.
///
/// remaining_accounts layout:
///   [0]  program_pda (authority PDA, signer via invoke_signed)
///   [1]  personal_position
///   [2]  user_shares
///   [3]  user_nav_sol_ata
///   [4]  user_wsol_ata
///   [5]  tenant
///   [6]  market_group
///   [7]  market_meta
///   [8]  mayflower_market
///   [9]  nav_sol_mint
///   [10] market_base_vault
///   [11] market_nav_vault
///   [12] fee_vault
///   [13] mayflower_program
///   [14] token_program
///   [15] log_account
fn do_mayflower_buy(remaining: &[AccountInfo], amount: u64) -> Result<()> {
    let program_pda = &remaining[0];
    let personal_position = &remaining[1];
    let user_shares = &remaining[2];
    let user_nav_sol_ata = &remaining[3];
    let user_wsol_ata = &remaining[4];

    let ix = mayflower::build_buy_ix(
        program_pda.key(),
        personal_position.key(),
        user_shares.key(),
        user_nav_sol_ata.key(),
        user_wsol_ata.key(),
        amount,
        0, // min_output = 0 (accept any slippage for now)
    );

    // Derive PDA signer seeds
    let (_, bump) = Pubkey::find_program_address(&[b"authority"], &crate::ID);
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

    invoke_signed(&ix, remaining, signer_seeds)?;

    Ok(())
}
