use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Borrow<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT token account.
    pub key_nft_ata: Account<'info, TokenAccount>,

    /// The admin's KeyAuthorization.
    #[account(
        constraint = key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub key_auth: Account<'info, KeyAuthorization>,

    /// The position to borrow against.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Borrow>, amount: u64) -> Result<()> {
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.key_nft_ata,
        &ctx.accounts.key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin],
    )?;

    require!(amount > 0, HardigError::InsufficientFunds);

    ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;

    // CPI into Mayflower if remaining_accounts provided
    if ctx.remaining_accounts.len() >= 10 {
        do_mayflower_borrow(ctx.remaining_accounts, amount)?;
    }

    ctx.accounts.position.user_debt = ctx
        .accounts
        .position
        .user_debt
        .checked_add(amount)
        .ok_or(HardigError::BorrowCapacityExceeded)?;

    Ok(())
}

/// CPI into Mayflower borrow.
///
/// remaining_accounts layout:
///   [0]  program_pda (authority PDA)
///   [1]  personal_position
///   [2]  user_base_token_ata (wSOL ATA of program PDA)
///   [3]  tenant
///   [4]  market_group
///   [5]  market_meta
///   [6]  market_base_vault
///   [7]  market_nav_vault
///   [8]  fee_vault
///   [9]  mayflower_market
///   [10] mayflower_program
///   [11] token_program
///   [12] log_account
fn do_mayflower_borrow(remaining: &[AccountInfo], amount: u64) -> Result<()> {
    let program_pda = &remaining[0];
    let personal_position = &remaining[1];
    let user_base_token_ata = &remaining[2];

    let ix = mayflower::build_borrow_ix(
        program_pda.key(),
        personal_position.key(),
        user_base_token_ata.key(),
        amount,
    );

    let (_, bump) = Pubkey::find_program_address(&[b"authority"], &crate::ID);
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

    invoke_signed(&ix, remaining, signer_seeds)?;

    Ok(())
}
