use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{MarketConfig, PositionNFT, PERM_MANAGE_KEYS};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct InitMayflowerPosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, update_authority, permissions).
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position (mutated to store Mayflower PersonalPosition PDA and MarketConfig).
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    /// The MarketConfig for the target market.
    pub market_config: Account<'info, MarketConfig>,

    /// Program PDA that will own the Mayflower PersonalPosition.
    /// CHECK: PDA derived from this program.
    #[account(
        seeds = [b"authority", position.admin_asset.as_ref()],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    /// The Mayflower PersonalPosition PDA.
    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub mayflower_personal_position: UncheckedAccount<'info>,

    /// The Mayflower PersonalPosition escrow (user shares).
    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub mayflower_user_shares: UncheckedAccount<'info>,

    /// The Mayflower market metadata.
    /// CHECK: Validated against market_config in handler.
    pub mayflower_market_meta: UncheckedAccount<'info>,

    /// The navSOL mint.
    /// CHECK: Validated against market_config in handler.
    pub nav_sol_mint: UncheckedAccount<'info>,

    /// The Mayflower log account.
    /// CHECK: PDA of Mayflower program.
    #[account(mut)]
    pub mayflower_log: UncheckedAccount<'info>,

    /// The Mayflower program.
    /// CHECK: Constant address validated in handler.
    pub mayflower_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, anchor_spl::token::Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitMayflowerPosition>) -> Result<()> {
    // Validate admin holds admin key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.program_pda.key(),
        PERM_MANAGE_KEYS,
    )?;

    let mc = &ctx.accounts.market_config;

    // Validate Mayflower account addresses against MarketConfig
    require!(
        ctx.accounts.mayflower_market_meta.key() == mc.market_meta,
        HardigError::InvalidMayflowerAccount
    );
    require!(
        ctx.accounts.nav_sol_mint.key() == mc.nav_mint,
        HardigError::InvalidMayflowerAccount
    );
    require!(
        ctx.accounts.mayflower_program.key() == mayflower::MAYFLOWER_PROGRAM_ID,
        HardigError::InvalidMayflowerAccount
    );

    // Validate PersonalPosition PDA derivation
    let program_pda = ctx.accounts.program_pda.key();
    let (expected_pp, _) = mayflower::derive_personal_position(&program_pda, &mc.market_meta);
    require!(
        ctx.accounts.mayflower_personal_position.key() == expected_pp,
        HardigError::InvalidPositionPda
    );

    let (expected_escrow, _) =
        mayflower::derive_personal_position_escrow(&expected_pp);
    require!(
        ctx.accounts.mayflower_user_shares.key() == expected_escrow,
        HardigError::InvalidPositionPda
    );

    // Build MarketAddresses from MarketConfig
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

    // Build and invoke CPI
    let ix = mayflower::build_init_personal_position_ix(
        ctx.accounts.admin.key(),
        program_pda,
        expected_pp,
        expected_escrow,
        &market,
    );

    let bump = ctx.bumps.program_pda;
    let admin_asset_key = ctx.accounts.position.admin_asset;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", admin_asset_key.as_ref(), &[bump]]];

    invoke_signed(
        &ix,
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.program_pda.to_account_info(),
            ctx.accounts.mayflower_market_meta.to_account_info(),
            ctx.accounts.nav_sol_mint.to_account_info(),
            ctx.accounts.mayflower_personal_position.to_account_info(),
            ctx.accounts.mayflower_user_shares.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.mayflower_log.to_account_info(),
            ctx.accounts.mayflower_program.to_account_info(),
        ],
        signer_seeds,
    )?;

    // Store the Mayflower PersonalPosition PDA and MarketConfig in our position
    ctx.accounts.position.position_pda = expected_pp;
    ctx.accounts.position.market_config = ctx.accounts.market_config.key();
    ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;

    Ok(())
}
