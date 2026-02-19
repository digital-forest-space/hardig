use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::Token;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{MarketConfig, PositionNFT, PERM_REPAY};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Repay<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The signer's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, update_authority, permissions).
    pub key_asset: UncheckedAccount<'info>,

    /// The position to repay debt for.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    /// The MarketConfig for this position's market.
    #[account(
        constraint = market_config.key() == position.market_config @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_config: Account<'info, MarketConfig>,

    pub system_program: Program<'info, System>,

    // -- Mayflower CPI accounts (10-account repay layout) --

    /// Mutable because Mayflower CPI marks user_wallet as writable.
    /// CHECK: PDA derived from this program.
    #[account(mut, seeds = [b"authority", position.admin_asset.as_ref()], bump)]
    pub program_pda: UncheckedAccount<'info>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA (repays from this account).
    /// CHECK: Validated as correct ATA for program_pda + base_mint.
    #[account(
        mut,
        constraint = user_base_token_ata.key() == get_associated_token_address(&program_pda.key(), &market_config.base_mint) @ HardigError::InvalidAta,
    )]
    pub user_base_token_ata: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(constraint = market_meta.key() == market_config.market_meta @ HardigError::InvalidMayflowerAccount)]
    pub market_meta: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(mut, constraint = market_base_vault.key() == market_config.market_base_vault @ HardigError::InvalidMayflowerAccount)]
    pub market_base_vault: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(constraint = wsol_mint.key() == market_config.base_mint @ HardigError::InvalidMayflowerAccount)]
    pub wsol_mint: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(mut, constraint = mayflower_market.key() == market_config.mayflower_market @ HardigError::InvalidMayflowerAccount)]
    pub mayflower_market: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(constraint = mayflower_program.key() == mayflower::MAYFLOWER_PROGRAM_ID @ HardigError::InvalidMayflowerAccount)]
    pub mayflower_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,

    /// Mayflower log account.
    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub log_account: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Repay>, amount: u64) -> Result<()> {
    validate_key(
        &ctx.accounts.signer,
        &ctx.accounts.key_asset.to_account_info(),
        &ctx.accounts.program_pda.key(),
        PERM_REPAY,
    )?;

    require!(amount > 0, HardigError::InsufficientFunds);
    require!(
        amount <= ctx.accounts.position.user_debt,
        HardigError::InsufficientFunds
    );

    let mc = &ctx.accounts.market_config;

    // Validate PDA-derived accounts
    let program_pda = ctx.accounts.program_pda.key();
    let (expected_pp, _) = mayflower::derive_personal_position(&program_pda, &mc.market_meta);
    require!(
        ctx.accounts.personal_position.key() == expected_pp,
        HardigError::InvalidMayflowerAccount
    );
    let (expected_log, _) = mayflower::derive_log_account();
    require!(
        ctx.accounts.log_account.key() == expected_log,
        HardigError::InvalidMayflowerAccount
    );

    if ctx.accounts.key_asset.key() == ctx.accounts.position.admin_asset {
        ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;
    }

    let market = mayflower::MarketAddresses {
        nav_mint: mc.nav_mint,
        base_mint: mc.base_mint,
        market_group: mc.market_group,
        market_meta: mc.market_meta,
        mayflower_market: mc.mayflower_market,
        market_base_vault: mc.market_base_vault,
        market_nav_vault: mc.market_nav_vault,
        fee_vault: mc.fee_vault,
    };

    // Build and invoke Mayflower repay CPI
    let ix = mayflower::build_repay_ix(
        program_pda,
        ctx.accounts.personal_position.key(),
        ctx.accounts.user_base_token_ata.key(),
        amount,
        &market,
    );

    let bump = ctx.bumps.program_pda;
    let admin_asset_key = ctx.accounts.position.admin_asset;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", admin_asset_key.as_ref(), &[bump]]];

    invoke_signed(
        &ix,
        &[
            ctx.accounts.program_pda.to_account_info(),
            ctx.accounts.market_meta.to_account_info(),
            ctx.accounts.mayflower_market.to_account_info(),
            ctx.accounts.personal_position.to_account_info(),
            ctx.accounts.wsol_mint.to_account_info(),
            ctx.accounts.user_base_token_ata.to_account_info(),
            ctx.accounts.market_base_vault.to_account_info(),
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
        .checked_sub(amount)
        .ok_or(HardigError::InsufficientFunds)?;

    Ok(())
}
