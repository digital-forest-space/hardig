use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::Token;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyState, MarketConfig, PositionState, ProtocolConfig, PERM_LIMITED_SELL, PERM_SELL};

use super::consume_rate_limit::consume_rate_limit;
use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The signer's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, update_authority, permissions).
    pub key_asset: UncheckedAccount<'info>,

    /// Optional KeyState for rate-limited keys (validated in handler).
    #[account(mut)]
    pub key_state: Option<Account<'info, KeyState>>,

    /// The position to withdraw from.
    #[account(mut)]
    pub position: Account<'info, PositionState>,

    /// Protocol config PDA — provides collection pubkey for key validation.
    #[account(
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
    )]
    pub config: Account<'info, ProtocolConfig>,

    /// The MarketConfig for this position's market.
    #[account(
        constraint = market_config.key() == position.market_config @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_config: Account<'info, MarketConfig>,

    pub system_program: Program<'info, System>,

    // -- Mayflower CPI accounts --

    /// Mutable because Mayflower CPI marks user_wallet as writable.
    /// CHECK: PDA derived from this program.
    #[account(mut, seeds = [b"authority", position.authority_seed.as_ref()], bump)]
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

    /// CHECK: Validated as correct ATA for program_pda + base_mint.
    #[account(
        mut,
        constraint = user_wsol_ata.key() == get_associated_token_address(&program_pda.key(), &market_config.base_mint) @ HardigError::InvalidAta,
    )]
    pub user_wsol_ata: UncheckedAccount<'info>,

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

pub fn handler(ctx: Context<Withdraw>, amount: u64, min_out: u64) -> Result<()> {
    let permissions = validate_key(
        &ctx.accounts.signer,
        &ctx.accounts.key_asset.to_account_info(),
        &ctx.accounts.position.authority_seed,
        PERM_SELL | PERM_LIMITED_SELL,
        &ctx.accounts.config.collection,
    )?;

    // Validate KeyState matches key_asset if provided
    if let Some(ref ks) = ctx.accounts.key_state {
        require!(ks.asset == ctx.accounts.key_asset.key(), HardigError::InvalidKey);
    }

    require!(amount > 0, HardigError::InsufficientFunds);

    let mc = &ctx.accounts.market_config;

    // Validate PDA-derived accounts BEFORE reading from them
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

    // Use Mayflower's actual deposited shares as the ceiling (source of truth)
    let mayflower_shares = {
        let data = ctx.accounts.personal_position.try_borrow_data()?;
        mayflower::read_deposited_shares(&data)?
    };
    require!(
        amount <= mayflower_shares,
        HardigError::InsufficientFunds
    );

    // Enforce rate limit for PERM_LIMITED_SELL (skipped if unlimited PERM_SELL is set)
    if permissions & PERM_SELL == 0 && permissions & PERM_LIMITED_SELL != 0 {
        let key_state = ctx.accounts.key_state.as_deref_mut()
            .ok_or(error!(HardigError::RateLimitExceeded))?;
        consume_rate_limit(
            &mut key_state.sell_bucket,
            amount,
            Clock::get()?.slot,
        )?;
    }

    if ctx.accounts.key_asset.key() == ctx.accounts.position.current_admin_asset {
        ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;
    }

    // Read wSOL balance before CPI for slippage check
    let wsol_before = {
        let wsol_data = ctx.accounts.user_wsol_ata.try_borrow_data()?;
        if wsol_data.len() >= 72 {
            u64::from_le_bytes(wsol_data[64..72].try_into().unwrap())
        } else {
            0
        }
    };

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

    // Build and invoke Mayflower sell CPI
    let ix = mayflower::build_sell_ix(
        program_pda,
        ctx.accounts.personal_position.key(),
        ctx.accounts.user_shares.key(),
        ctx.accounts.user_nav_sol_ata.key(),
        ctx.accounts.user_wsol_ata.key(),
        amount,
        0, // Mayflower's own min_output — we enforce slippage ourselves
        &market,
    );

    let bump = ctx.bumps.program_pda;
    let admin_asset_key = ctx.accounts.position.authority_seed;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", admin_asset_key.as_ref(), &[bump]]];

    // Read deposited shares BEFORE the sell CPI
    let pp_info = ctx.accounts.personal_position.to_account_info();
    let shares_before = {
        let data = pp_info.try_borrow_data()?;
        mayflower::read_deposited_shares(&data)?
    };

    invoke_signed(
        &ix,
        &[
            ctx.accounts.program_pda.to_account_info(),       // 0: userWallet
            ctx.accounts.tenant.to_account_info(),            // 1: tenant
            ctx.accounts.market_group.to_account_info(),      // 2: marketGroup
            ctx.accounts.market_meta.to_account_info(),       // 3: marketMetadata
            ctx.accounts.mayflower_market.to_account_info(),  // 4: mayflowerMarket
            pp_info.clone(),                                  // 5: personalPosition
            ctx.accounts.market_base_vault.to_account_info(), // 6: marketBaseVault
            ctx.accounts.market_nav_vault.to_account_info(),  // 7: marketNavVault
            ctx.accounts.fee_vault.to_account_info(),         // 8: feeVault
            ctx.accounts.nav_sol_mint.to_account_info(),      // 9: navMint
            ctx.accounts.wsol_mint.to_account_info(),         // 10: baseMint
            ctx.accounts.user_wsol_ata.to_account_info(),     // 11: userWsolATA
            ctx.accounts.user_nav_sol_ata.to_account_info(),  // 12: userNavSolATA
            ctx.accounts.user_shares.to_account_info(),       // 13: userShares
            ctx.accounts.token_program.to_account_info(),     // 14: Token Program
            ctx.accounts.token_program.to_account_info(),     // 15: Token Program (dup)
            ctx.accounts.log_account.to_account_info(),       // 16: logAccount
            ctx.accounts.mayflower_program.to_account_info(), // 17: Mayflower program
        ],
        signer_seeds,
    )?;

    // Read deposited shares AFTER the sell CPI and compute the actual navSOL sold
    let shares_after = {
        let data = pp_info.try_borrow_data()?;
        mayflower::read_deposited_shares(&data)?
    };
    let shares_sold = shares_before
        .checked_sub(shares_after)
        .ok_or(HardigError::InsufficientFunds)?;

    // Slippage check: verify SOL received >= min_out
    let wsol_after = {
        let wsol_data = ctx.accounts.user_wsol_ata.try_borrow_data()?;
        if wsol_data.len() >= 72 {
            u64::from_le_bytes(wsol_data[64..72].try_into().unwrap())
        } else {
            0
        }
    };
    let sol_received = wsol_after.saturating_sub(wsol_before);
    require!(sol_received >= min_out, HardigError::SlippageExceeded);

    // Close PDA's wSOL ATA — returns all wSOL + rent as native SOL to signer
    // Only attempt if the account is an initialized SPL token account (state byte at offset 108)
    let wsol_initialized = {
        let data = ctx.accounts.user_wsol_ata.try_borrow_data()?;
        data.len() >= 109 && data[108] != 0
    };
    if wsol_initialized {
        let close_ix = Instruction {
            program_id: anchor_spl::token::ID,
            accounts: vec![
                AccountMeta::new(ctx.accounts.user_wsol_ata.key(), false),
                AccountMeta::new(ctx.accounts.signer.key(), false),
                AccountMeta::new_readonly(ctx.accounts.program_pda.key(), true),
            ],
            data: vec![9], // SPL Token CloseAccount
        };
        invoke_signed(
            &close_ix,
            &[
                ctx.accounts.user_wsol_ata.to_account_info(),
                ctx.accounts.signer.to_account_info(),
                ctx.accounts.program_pda.to_account_info(),
            ],
            signer_seeds,
        )?;
    }

    ctx.accounts.position.deposited_nav = ctx
        .accounts
        .position
        .deposited_nav
        .saturating_sub(shares_sold);

    Ok(())
}
