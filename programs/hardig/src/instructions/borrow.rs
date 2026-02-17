use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::{Token, TokenAccount};

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

    // -- Mayflower CPI accounts --

    /// Mutable because Mayflower CPI marks user_wallet as writable.
    /// CHECK: PDA derived from this program.
    #[account(mut, seeds = [b"authority"], bump)]
    pub program_pda: UncheckedAccount<'info>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA (receives borrowed funds).
    /// CHECK: Validated in handler as ATA derivation.
    #[account(mut)]
    pub user_base_token_ata: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(constraint = tenant.key() == mayflower::MAYFLOWER_TENANT @ HardigError::InvalidMayflowerAccount)]
    pub tenant: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(constraint = market_group.key() == mayflower::MARKET_GROUP @ HardigError::InvalidMayflowerAccount)]
    pub market_group: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(constraint = market_meta.key() == mayflower::MARKET_META @ HardigError::InvalidMayflowerAccount)]
    pub market_meta: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(mut, constraint = market_base_vault.key() == mayflower::MARKET_BASE_VAULT @ HardigError::InvalidMayflowerAccount)]
    pub market_base_vault: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(mut, constraint = market_nav_vault.key() == mayflower::MARKET_NAV_VAULT @ HardigError::InvalidMayflowerAccount)]
    pub market_nav_vault: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(mut, constraint = fee_vault.key() == mayflower::FEE_VAULT @ HardigError::InvalidMayflowerAccount)]
    pub fee_vault: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(constraint = wsol_mint.key() == mayflower::WSOL_MINT @ HardigError::InvalidMayflowerAccount)]
    pub wsol_mint: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(mut, constraint = mayflower_market.key() == mayflower::MAYFLOWER_MARKET @ HardigError::InvalidMayflowerAccount)]
    pub mayflower_market: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(constraint = mayflower_program.key() == mayflower::MAYFLOWER_PROGRAM_ID @ HardigError::InvalidMayflowerAccount)]
    pub mayflower_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub log_account: UncheckedAccount<'info>,
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

    // Validate PDA-derived accounts
    let program_pda = ctx.accounts.program_pda.key();
    let (expected_pp, _) = mayflower::derive_personal_position(&program_pda);
    require!(
        ctx.accounts.personal_position.key() == expected_pp,
        HardigError::InvalidMayflowerAccount
    );
    let (expected_log, _) = mayflower::derive_log_account();
    require!(
        ctx.accounts.log_account.key() == expected_log,
        HardigError::InvalidMayflowerAccount
    );

    ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;

    // Build and invoke Mayflower borrow CPI
    let ix = mayflower::build_borrow_ix(
        program_pda,
        ctx.accounts.personal_position.key(),
        ctx.accounts.user_base_token_ata.key(),
        amount,
    );

    let bump = ctx.bumps.program_pda;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

    invoke_signed(
        &ix,
        &[
            ctx.accounts.program_pda.to_account_info(),
            ctx.accounts.tenant.to_account_info(),
            ctx.accounts.market_group.to_account_info(),
            ctx.accounts.market_meta.to_account_info(),
            ctx.accounts.market_base_vault.to_account_info(),
            ctx.accounts.market_nav_vault.to_account_info(),
            ctx.accounts.fee_vault.to_account_info(),
            ctx.accounts.wsol_mint.to_account_info(),
            ctx.accounts.user_base_token_ata.to_account_info(),
            ctx.accounts.mayflower_market.to_account_info(),
            ctx.accounts.personal_position.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.log_account.to_account_info(),
            ctx.accounts.mayflower_program.to_account_info(),
        ],
        signer_seeds,
    )?;

    ctx.accounts.position.user_debt = ctx
        .accounts
        .position
        .user_debt
        .checked_add(amount)
        .ok_or(HardigError::BorrowCapacityExceeded)?;

    Ok(())
}
