use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::{Token, TokenAccount};

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

    // -- Mayflower CPI accounts (union of buy + borrow) --

    /// Mutable because Mayflower CPI marks user_wallet as writable.
    /// CHECK: PDA derived from this program.
    #[account(mut, seeds = [b"authority"], bump)]
    pub program_pda: UncheckedAccount<'info>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub user_shares: UncheckedAccount<'info>,

    /// CHECK: Validated in handler as ATA derivation.
    #[account(mut)]
    pub user_nav_sol_ata: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA (used for buy CPI input).
    /// CHECK: Validated in handler as ATA derivation.
    #[account(mut)]
    pub user_wsol_ata: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA (used for borrow CPI output).
    /// In practice this is the same address as user_wsol_ata.
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
    #[account(mut, constraint = mayflower_market.key() == mayflower::MAYFLOWER_MARKET @ HardigError::InvalidMayflowerAccount)]
    pub mayflower_market: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(mut, constraint = nav_sol_mint.key() == mayflower::NAV_SOL_MINT @ HardigError::InvalidMayflowerAccount)]
    pub nav_sol_mint: UncheckedAccount<'info>,

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
    #[account(constraint = mayflower_program.key() == mayflower::MAYFLOWER_PROGRAM_ID @ HardigError::InvalidMayflowerAccount)]
    pub mayflower_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub log_account: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Reinvest>) -> Result<()> {
    validate_key(
        &ctx.accounts.signer,
        &ctx.accounts.key_nft_ata,
        &ctx.accounts.key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin, KeyRole::Operator, KeyRole::Keeper],
    )?;

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

    // Read borrow capacity from Mayflower accounts
    let market_data = ctx.accounts.mayflower_market.try_borrow_data()?;
    let floor_price = mayflower::read_floor_price(&market_data)?;

    let pp_data = ctx.accounts.personal_position.try_borrow_data()?;
    let deposited_shares = mayflower::read_deposited_shares(&pp_data)?;
    let current_debt = mayflower::read_debt(&pp_data)?;

    let capacity =
        mayflower::calculate_borrow_capacity(deposited_shares, floor_price, current_debt)?;

    if capacity == 0 {
        return Ok(());
    }

    // Drop borrows before CPI
    drop(market_data);
    drop(pp_data);

    let bump = ctx.bumps.program_pda;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

    // Step 1: Borrow the available capacity
    let borrow_ix = mayflower::build_borrow_ix(
        program_pda,
        ctx.accounts.personal_position.key(),
        ctx.accounts.user_base_token_ata.key(),
        capacity,
    );

    invoke_signed(
        &borrow_ix,
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

    // Step 2: Read actual wSOL balance after borrow (fees may have been deducted)
    let wsol_data = ctx.accounts.user_base_token_ata.try_borrow_data()?;
    let actual_amount = if wsol_data.len() >= 72 {
        u64::from_le_bytes(wsol_data[64..72].try_into().unwrap())
    } else {
        0
    };
    drop(wsol_data);

    if actual_amount == 0 {
        return Ok(());
    }

    // Step 3: Buy navSOL with the actual borrowed amount
    let buy_ix = mayflower::build_buy_ix(
        program_pda,
        ctx.accounts.personal_position.key(),
        ctx.accounts.user_shares.key(),
        ctx.accounts.user_nav_sol_ata.key(),
        ctx.accounts.user_wsol_ata.key(),
        actual_amount,
        0, // min_output
    );

    invoke_signed(
        &buy_ix,
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
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.log_account.to_account_info(),
            ctx.accounts.mayflower_program.to_account_info(),
        ],
        signer_seeds,
    )?;

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

    Ok(())
}
