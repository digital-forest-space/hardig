use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Repay<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The signer's key NFT token account.
    pub key_nft_ata: Account<'info, TokenAccount>,

    /// The signer's KeyAuthorization.
    #[account(
        constraint = key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub key_auth: Account<'info, KeyAuthorization>,

    /// The position to repay debt for.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Repay>, amount: u64) -> Result<()> {
    validate_key(
        &ctx.accounts.signer,
        &ctx.accounts.key_nft_ata,
        &ctx.accounts.key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin, KeyRole::Operator, KeyRole::Depositor],
    )?;

    require!(amount > 0, HardigError::InsufficientFunds);
    require!(
        amount <= ctx.accounts.position.user_debt,
        HardigError::InsufficientFunds
    );

    if ctx.accounts.key_auth.role == KeyRole::Admin {
        ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;
    }

    // CPI into Mayflower if remaining_accounts provided
    if ctx.remaining_accounts.len() >= 10 {
        do_mayflower_repay(ctx.remaining_accounts, amount)?;
    }

    ctx.accounts.position.user_debt = ctx
        .accounts
        .position
        .user_debt
        .checked_sub(amount)
        .ok_or(HardigError::InsufficientFunds)?;

    Ok(())
}

/// CPI into Mayflower repay. Same account layout as borrow.
fn do_mayflower_repay(remaining: &[AccountInfo], amount: u64) -> Result<()> {
    let program_pda = &remaining[0];
    let personal_position = &remaining[1];
    let user_base_token_ata = &remaining[2];

    let ix = mayflower::build_repay_ix(
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
