use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT token account.
    pub key_nft_ata: Account<'info, TokenAccount>,

    /// The admin's KeyAuthorization.
    #[account(
        constraint = key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub key_auth: Account<'info, KeyAuthorization>,

    /// The position to withdraw from.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.key_nft_ata,
        &ctx.accounts.key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin],
    )?;

    require!(amount > 0, HardigError::InsufficientFunds);
    require!(
        amount <= ctx.accounts.position.deposited_nav,
        HardigError::InsufficientFunds
    );

    ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;

    // CPI into Mayflower sell if remaining_accounts provided.
    // NOTE: IX_SELL discriminator must be derived before this works on mainnet.
    if ctx.remaining_accounts.len() >= 14 {
        do_mayflower_sell(ctx.remaining_accounts, amount)?;
    }

    ctx.accounts.position.deposited_nav = ctx
        .accounts
        .position
        .deposited_nav
        .checked_sub(amount)
        .ok_or(HardigError::InsufficientFunds)?;

    Ok(())
}

/// CPI into Mayflower SellWithExactTokenIn. Same account layout as buy.
fn do_mayflower_sell(remaining: &[AccountInfo], amount: u64) -> Result<()> {
    let program_pda = &remaining[0];
    let personal_position = &remaining[1];
    let user_shares = &remaining[2];
    let user_nav_sol_ata = &remaining[3];
    let user_wsol_ata = &remaining[4];

    let ix = mayflower::build_sell_ix(
        program_pda.key(),
        personal_position.key(),
        user_shares.key(),
        user_nav_sol_ata.key(),
        user_wsol_ata.key(),
        amount,
        0, // min_output
    );

    let (_, bump) = Pubkey::find_program_address(&[b"authority"], &crate::ID);
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

    invoke_signed(&ix, remaining, signer_seeds)?;

    Ok(())
}
