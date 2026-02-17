use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::{Token, TokenAccount};

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

    // -- Mayflower CPI accounts --

    /// Program PDA (authority) that owns the Mayflower PersonalPosition.
    /// Mutable because Mayflower CPI marks user_wallet as writable.
    /// CHECK: PDA derived from this program.
    #[account(
        mut,
        seeds = [b"authority"],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    /// Mayflower PersonalPosition PDA.
    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// Mayflower user shares escrow.
    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub user_shares: UncheckedAccount<'info>,

    /// Program PDA's navSOL ATA.
    /// CHECK: Validated in handler as ATA derivation.
    #[account(mut)]
    pub user_nav_sol_ata: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA.
    /// CHECK: Validated in handler as ATA derivation.
    #[account(mut)]
    pub user_wsol_ata: UncheckedAccount<'info>,

    /// Mayflower tenant.
    /// CHECK: Constant address validated by constraint.
    #[account(
        constraint = tenant.key() == mayflower::MAYFLOWER_TENANT @ HardigError::InvalidMayflowerAccount,
    )]
    pub tenant: UncheckedAccount<'info>,

    /// Mayflower market group.
    /// CHECK: Constant address validated by constraint.
    #[account(
        constraint = market_group.key() == mayflower::MARKET_GROUP @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_group: UncheckedAccount<'info>,

    /// Mayflower market metadata.
    /// CHECK: Constant address validated by constraint.
    #[account(
        constraint = market_meta.key() == mayflower::MARKET_META @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_meta: UncheckedAccount<'info>,

    /// Mayflower market.
    /// CHECK: Constant address validated by constraint.
    #[account(
        mut,
        constraint = mayflower_market.key() == mayflower::MAYFLOWER_MARKET @ HardigError::InvalidMayflowerAccount,
    )]
    pub mayflower_market: UncheckedAccount<'info>,

    /// navSOL mint.
    /// CHECK: Constant address validated by constraint.
    #[account(
        mut,
        constraint = nav_sol_mint.key() == mayflower::NAV_SOL_MINT @ HardigError::InvalidMayflowerAccount,
    )]
    pub nav_sol_mint: UncheckedAccount<'info>,

    /// Mayflower market base vault.
    /// CHECK: Constant address validated by constraint.
    #[account(
        mut,
        constraint = market_base_vault.key() == mayflower::MARKET_BASE_VAULT @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_base_vault: UncheckedAccount<'info>,

    /// Mayflower market nav vault.
    /// CHECK: Constant address validated by constraint.
    #[account(
        mut,
        constraint = market_nav_vault.key() == mayflower::MARKET_NAV_VAULT @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_nav_vault: UncheckedAccount<'info>,

    /// Mayflower fee vault.
    /// CHECK: Constant address validated by constraint.
    #[account(
        mut,
        constraint = fee_vault.key() == mayflower::FEE_VAULT @ HardigError::InvalidMayflowerAccount,
    )]
    pub fee_vault: UncheckedAccount<'info>,

    /// wSOL mint (baseMint for Mayflower CPI).
    /// CHECK: Constant address validated by constraint.
    #[account(
        constraint = wsol_mint.key() == mayflower::WSOL_MINT @ HardigError::InvalidMayflowerAccount,
    )]
    pub wsol_mint: UncheckedAccount<'info>,

    /// Mayflower program.
    /// CHECK: Constant address validated by constraint.
    #[account(
        constraint = mayflower_program.key() == mayflower::MAYFLOWER_PROGRAM_ID @ HardigError::InvalidMayflowerAccount,
    )]
    pub mayflower_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,

    /// Mayflower log account.
    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub log_account: UncheckedAccount<'info>,
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

    // Validate PDA-derived accounts
    let program_pda = ctx.accounts.program_pda.key();
    let (expected_pp, _) = mayflower::derive_personal_position(&program_pda);
    require!(
        ctx.accounts.personal_position.key() == expected_pp,
        HardigError::InvalidMayflowerAccount
    );
    let (expected_escrow, _) = mayflower::derive_personal_position_escrow(&expected_pp);
    require!(
        ctx.accounts.user_shares.key() == expected_escrow,
        HardigError::InvalidMayflowerAccount
    );
    let (expected_log, _) = mayflower::derive_log_account();
    require!(
        ctx.accounts.log_account.key() == expected_log,
        HardigError::InvalidMayflowerAccount
    );

    if ctx.accounts.key_auth.role == KeyRole::Admin {
        ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;
    }

    // Build and invoke Mayflower buy CPI
    let ix = mayflower::build_buy_ix(
        program_pda,
        ctx.accounts.personal_position.key(),
        ctx.accounts.user_shares.key(),
        ctx.accounts.user_nav_sol_ata.key(),
        ctx.accounts.user_wsol_ata.key(),
        amount,
        0, // min_output = 0 (accept any slippage for now)
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
            ctx.accounts.mayflower_market.to_account_info(),
            ctx.accounts.personal_position.to_account_info(),
            ctx.accounts.user_shares.to_account_info(),
            ctx.accounts.nav_sol_mint.to_account_info(),
            ctx.accounts.wsol_mint.to_account_info(),
            ctx.accounts.user_nav_sol_ata.to_account_info(),
            ctx.accounts.user_wsol_ata.to_account_info(),
            ctx.accounts.market_base_vault.to_account_info(),
            ctx.accounts.market_nav_vault.to_account_info(),
            ctx.accounts.fee_vault.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.token_program.to_account_info(), // Token Program appears twice in CPI
            ctx.accounts.log_account.to_account_info(),
            ctx.accounts.mayflower_program.to_account_info(),
        ],
        signer_seeds,
    )?;

    ctx.accounts.position.deposited_nav = ctx
        .accounts
        .position
        .deposited_nav
        .checked_add(amount)
        .ok_or(HardigError::InsufficientFunds)?;

    Ok(())
}
