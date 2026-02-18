use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::{Token, TokenAccount};

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyAuthorization, KeyRole, MarketConfig, PositionNFT};

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

    /// The MarketConfig for this position's market.
    #[account(
        constraint = market_config.key() == position.market_config @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_config: Account<'info, MarketConfig>,

    pub system_program: Program<'info, System>,

    // -- Mayflower CPI accounts (union of buy + borrow) --

    /// Mutable because Mayflower CPI marks user_wallet as writable.
    /// CHECK: PDA derived from this program.
    #[account(mut, seeds = [b"authority", position.admin_nft_mint.as_ref()], bump)]
    pub program_pda: UncheckedAccount<'info>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub user_shares: UncheckedAccount<'info>,

    /// CHECK: Validated as correct ATA for program_pda + nav_mint.
    #[account(
        mut,
        constraint = user_nav_sol_ata.key() == get_associated_token_address(&program_pda.key(), &market_config.nav_mint) @ HardigError::InvalidAta,
    )]
    pub user_nav_sol_ata: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA (used for buy CPI input).
    /// CHECK: Validated as correct ATA for program_pda + base_mint.
    #[account(
        mut,
        constraint = user_wsol_ata.key() == get_associated_token_address(&program_pda.key(), &market_config.base_mint) @ HardigError::InvalidAta,
    )]
    pub user_wsol_ata: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA (used for borrow CPI output).
    /// In practice this is the same address as user_wsol_ata.
    /// CHECK: Validated as correct ATA for program_pda + base_mint.
    #[account(
        mut,
        constraint = user_base_token_ata.key() == get_associated_token_address(&program_pda.key(), &market_config.base_mint) @ HardigError::InvalidAta,
    )]
    pub user_base_token_ata: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(constraint = tenant.key() == mayflower::MAYFLOWER_TENANT @ HardigError::InvalidMayflowerAccount)]
    pub tenant: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(constraint = market_group.key() == market_config.market_group @ HardigError::InvalidMayflowerAccount)]
    pub market_group: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(constraint = market_meta.key() == market_config.market_meta @ HardigError::InvalidMayflowerAccount)]
    pub market_meta: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(mut, constraint = mayflower_market.key() == market_config.mayflower_market @ HardigError::InvalidMayflowerAccount)]
    pub mayflower_market: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(mut, constraint = nav_sol_mint.key() == market_config.nav_mint @ HardigError::InvalidMayflowerAccount)]
    pub nav_sol_mint: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(mut, constraint = market_base_vault.key() == market_config.market_base_vault @ HardigError::InvalidMayflowerAccount)]
    pub market_base_vault: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(mut, constraint = market_nav_vault.key() == market_config.market_nav_vault @ HardigError::InvalidMayflowerAccount)]
    pub market_nav_vault: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(mut, constraint = fee_vault.key() == market_config.fee_vault @ HardigError::InvalidMayflowerAccount)]
    pub fee_vault: UncheckedAccount<'info>,

    /// CHECK: Validated against market_config.
    #[account(constraint = wsol_mint.key() == market_config.base_mint @ HardigError::InvalidMayflowerAccount)]
    pub wsol_mint: UncheckedAccount<'info>,

    /// CHECK: Constant address validated by constraint.
    #[account(constraint = mayflower_program.key() == mayflower::MAYFLOWER_PROGRAM_ID @ HardigError::InvalidMayflowerAccount)]
    pub mayflower_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub log_account: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Reinvest>, min_out: u64) -> Result<()> {
    validate_key(
        &ctx.accounts.signer,
        &ctx.accounts.key_nft_ata,
        &ctx.accounts.key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin, KeyRole::Operator, KeyRole::Keeper],
    )?;

    let mc = &ctx.accounts.market_config;

    // Validate PDA-derived accounts
    let program_pda = ctx.accounts.program_pda.key();
    let (expected_pp, _) = mayflower::derive_personal_position(&program_pda, &mc.market_meta);
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

    // Read deposited shares before buy CPI for slippage check
    let shares_before = deposited_shares;

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

    let bump = ctx.bumps.program_pda;
    let mint_key = ctx.accounts.position.admin_nft_mint;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", mint_key.as_ref(), &[bump]]];

    // Step 1: Borrow the available capacity
    let borrow_ix = mayflower::build_borrow_ix(
        program_pda,
        ctx.accounts.personal_position.key(),
        ctx.accounts.user_base_token_ata.key(),
        capacity,
        &market,
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
        0, // Mayflower's own min_output — we enforce slippage ourselves
        &market,
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

    // Slippage check: verify navSOL shares received >= min_out
    let shares_after = {
        let pp_data = ctx.accounts.personal_position.try_borrow_data()?;
        mayflower::read_deposited_shares(&pp_data)?
    };
    let shares_received = shares_after.saturating_sub(shares_before);
    require!(shares_received >= min_out, HardigError::SlippageExceeded);

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
