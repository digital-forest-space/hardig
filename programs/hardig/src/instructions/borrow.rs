use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::Token;

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{KeyState, MarketConfig, PositionNFT, PERM_BORROW, PERM_LIMITED_BORROW};

use super::consume_rate_limit::consume_rate_limit;
use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Borrow<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The signer's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, update_authority, permissions).
    pub key_asset: UncheckedAccount<'info>,

    /// Optional KeyState for rate-limited keys (validated in handler).
    #[account(mut)]
    pub key_state: Option<Account<'info, KeyState>>,

    /// The position to borrow against.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    /// The MarketConfig for this position's market.
    #[account(
        constraint = market_config.key() == position.market_config @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_config: Account<'info, MarketConfig>,

    pub system_program: Program<'info, System>,

    // -- Mayflower CPI accounts --

    /// Mutable because Mayflower CPI marks user_wallet as writable.
    /// CHECK: PDA derived from this program.
    #[account(mut, seeds = [b"authority", position.admin_asset.as_ref()], bump)]
    pub program_pda: UncheckedAccount<'info>,

    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA (receives borrowed funds).
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

    /// CHECK: Validated against market_config.
    #[account(mut, constraint = mayflower_market.key() == market_config.mayflower_market @ HardigError::InvalidMayflowerAccount)]
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
    let permissions = validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.key_asset.to_account_info(),
        &ctx.accounts.program_pda.key(),
        PERM_BORROW | PERM_LIMITED_BORROW,
    )?;

    // Validate KeyState matches key_asset if provided
    if let Some(ref ks) = ctx.accounts.key_state {
        require!(ks.asset == ctx.accounts.key_asset.key(), HardigError::InvalidKey);
    }

    // Enforce rate limit for PERM_LIMITED_BORROW (skipped if unlimited PERM_BORROW is set)
    if permissions & PERM_BORROW == 0 && permissions & PERM_LIMITED_BORROW != 0 {
        let key_state = ctx.accounts.key_state.as_deref_mut()
            .ok_or(error!(HardigError::RateLimitExceeded))?;
        consume_rate_limit(
            &mut key_state.borrow_bucket,
            amount,
            Clock::get()?.slot,
        )?;
    }

    require!(amount > 0, HardigError::InsufficientFunds);

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

    // Build and invoke Mayflower borrow CPI
    let ix = mayflower::build_borrow_ix(
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

    // Close PDA's wSOL ATA — returns borrowed wSOL + rent as native SOL to admin
    // Only attempt if the account is an initialized SPL token account (state byte at offset 108)
    let ata_initialized = {
        let data = ctx.accounts.user_base_token_ata.try_borrow_data()?;
        data.len() >= 109 && data[108] != 0
    };
    if ata_initialized {
        let close_ix = Instruction {
            program_id: anchor_spl::token::ID,
            accounts: vec![
                AccountMeta::new(ctx.accounts.user_base_token_ata.key(), false),
                AccountMeta::new(ctx.accounts.admin.key(), false),
                AccountMeta::new_readonly(ctx.accounts.program_pda.key(), true),
            ],
            data: vec![9], // SPL Token CloseAccount
        };
        invoke_signed(
            &close_ix,
            &[
                ctx.accounts.user_base_token_ata.to_account_info(),
                ctx.accounts.admin.to_account_info(),
                ctx.accounts.program_pda.to_account_info(),
            ],
            signer_seeds,
        )?;
    }

    ctx.accounts.position.user_debt = ctx
        .accounts
        .position
        .user_debt
        .checked_add(amount)
        .ok_or(HardigError::BorrowCapacityExceeded)?;

    Ok(())
}
