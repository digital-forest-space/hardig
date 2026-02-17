use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct InitMayflowerPosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT token account.
    pub admin_nft_ata: Account<'info, TokenAccount>,

    /// The admin's KeyAuthorization.
    #[account(
        constraint = admin_key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub admin_key_auth: Account<'info, KeyAuthorization>,

    /// The position (mutated to store Mayflower PersonalPosition PDA).
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    /// Program PDA that will own the Mayflower PersonalPosition.
    /// CHECK: PDA derived from this program.
    #[account(
        seeds = [b"authority"],
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
    /// CHECK: Constant address validated in handler.
    pub mayflower_market_meta: UncheckedAccount<'info>,

    /// The navSOL mint.
    /// CHECK: Constant address validated in handler.
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
        &ctx.accounts.admin_nft_ata,
        &ctx.accounts.admin_key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin],
    )?;

    // Validate Mayflower account addresses
    require!(
        ctx.accounts.mayflower_market_meta.key() == mayflower::MARKET_META,
        HardigError::InvalidPositionPda
    );
    require!(
        ctx.accounts.nav_sol_mint.key() == mayflower::NAV_SOL_MINT,
        HardigError::InvalidPositionPda
    );
    require!(
        ctx.accounts.mayflower_program.key() == mayflower::MAYFLOWER_PROGRAM_ID,
        HardigError::InvalidPositionPda
    );

    // Validate PersonalPosition PDA derivation
    let program_pda = ctx.accounts.program_pda.key();
    let (expected_pp, _) = mayflower::derive_personal_position(&program_pda);
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

    // Build and invoke CPI
    let ix = mayflower::build_init_personal_position_ix(
        ctx.accounts.admin.key(),
        program_pda,
        expected_pp,
        expected_escrow,
    );

    let bump = ctx.bumps.program_pda;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

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

    // Store the Mayflower PersonalPosition PDA in our position
    ctx.accounts.position.position_pda = expected_pp;
    ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;

    Ok(())
}
