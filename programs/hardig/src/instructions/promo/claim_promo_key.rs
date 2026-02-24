use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::Token;
use mpl_core::{
    instructions::CreateV2CpiBuilder,
    types::{
        Attribute, Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair,
    },
};

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{
    ClaimReceipt, KeyState, MarketConfig, PositionState, PromoConfig, ProtocolConfig, RateBucket,
    PERM_LIMITED_BORROW, PERM_LIMITED_SELL,
};
use super::super::{format_sol_amount, metadata_uri, permission_attributes, slots_to_duration};

#[derive(Accounts)]
#[instruction(amount: u64, min_out: u64)]
pub struct ClaimPromoKey<'info> {
    /// The user claiming the promo key. Pays rent for ClaimReceipt + KeyState + NFT.
    #[account(mut)]
    pub claimer: Signer<'info>,

    /// The PromoConfig PDA — read params, increment claims_count.
    #[account(
        mut,
        seeds = [PromoConfig::SEED, promo.authority_seed.as_ref(), promo.name_suffix.as_bytes()],
        bump = promo.bump,
    )]
    pub promo: Box<Account<'info, PromoConfig>>,

    /// One-per-wallet guard. Init fails if PDA already exists.
    #[account(
        init,
        payer = claimer,
        space = ClaimReceipt::SIZE,
        seeds = [ClaimReceipt::SEED, promo.key().as_ref(), claimer.key().as_ref()],
        bump,
    )]
    pub claim_receipt: Account<'info, ClaimReceipt>,

    /// The position this promo is for. Mutable because deposited_nav updates.
    #[account(mut)]
    pub position: Box<Account<'info, PositionState>>,

    /// The new MPL-Core asset for the key NFT.
    #[account(mut)]
    pub key_asset: Signer<'info>,

    /// KeyState PDA for rate limiting.
    #[account(
        init,
        payer = claimer,
        space = KeyState::SIZE,
        seeds = [KeyState::SEED, key_asset.key().as_ref()],
        bump,
    )]
    pub key_state: Account<'info, KeyState>,

    /// Protocol config PDA — collection address + CPI signer.
    #[account(
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
        constraint = config.collection != Pubkey::default() @ HardigError::CollectionNotCreated,
    )]
    pub config: Account<'info, ProtocolConfig>,

    /// The MPL-Core collection asset.
    /// CHECK: Validated against config.collection.
    #[account(
        mut,
        constraint = collection.key() == config.collection @ HardigError::CollectionNotCreated,
    )]
    pub collection: UncheckedAccount<'info>,

    /// CHECK: MPL-Core program validated by address constraint.
    #[account(address = mpl_core::ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    // -- MarketConfig + Mayflower CPI accounts --

    /// The MarketConfig for this position's market.
    #[account(
        constraint = market_config.key() == position.market_config @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_config: Box<Account<'info, MarketConfig>>,

    /// Program PDA (authority) that owns the Mayflower PersonalPosition.
    /// CHECK: PDA derived from this program.
    #[account(
        mut,
        seeds = [b"authority", position.authority_seed.as_ref()],
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
    /// CHECK: Validated as correct ATA for program_pda + nav_mint.
    #[account(
        mut,
        constraint = user_nav_sol_ata.key() == get_associated_token_address(&program_pda.key(), &market_config.nav_mint) @ HardigError::InvalidAta,
    )]
    pub user_nav_sol_ata: UncheckedAccount<'info>,

    /// Program PDA's wSOL ATA.
    /// CHECK: Validated as correct ATA for program_pda + base_mint.
    #[account(
        mut,
        constraint = user_wsol_ata.key() == get_associated_token_address(&program_pda.key(), &market_config.base_mint) @ HardigError::InvalidAta,
    )]
    pub user_wsol_ata: UncheckedAccount<'info>,

    /// Mayflower tenant.
    /// CHECK: Constant address validated by constraint.
    #[account(
        constraint = tenant.key() == mayflower::MAYFLOWER_TENANT @ HardigError::InvalidMayflowerAccount,
    )]
    pub tenant: UncheckedAccount<'info>,

    /// Mayflower market group.
    /// CHECK: Validated against market_config.
    #[account(
        constraint = market_group.key() == market_config.market_group @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_group: UncheckedAccount<'info>,

    /// Mayflower market metadata.
    /// CHECK: Validated against market_config.
    #[account(
        constraint = market_meta.key() == market_config.market_meta @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_meta: UncheckedAccount<'info>,

    /// Mayflower market.
    /// CHECK: Validated against market_config.
    #[account(
        mut,
        constraint = mayflower_market.key() == market_config.mayflower_market @ HardigError::InvalidMayflowerAccount,
    )]
    pub mayflower_market: UncheckedAccount<'info>,

    /// navSOL mint.
    /// CHECK: Validated against market_config.
    #[account(
        mut,
        constraint = nav_sol_mint.key() == market_config.nav_mint @ HardigError::InvalidMayflowerAccount,
    )]
    pub nav_sol_mint: UncheckedAccount<'info>,

    /// Mayflower market base vault.
    /// CHECK: Validated against market_config.
    #[account(
        mut,
        constraint = market_base_vault.key() == market_config.market_base_vault @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_base_vault: UncheckedAccount<'info>,

    /// Mayflower market nav vault.
    /// CHECK: Validated against market_config.
    #[account(
        mut,
        constraint = market_nav_vault.key() == market_config.market_nav_vault @ HardigError::InvalidMayflowerAccount,
    )]
    pub market_nav_vault: UncheckedAccount<'info>,

    /// Mayflower fee vault.
    /// CHECK: Validated against market_config.
    #[account(
        mut,
        constraint = fee_vault.key() == market_config.fee_vault @ HardigError::InvalidMayflowerAccount,
    )]
    pub fee_vault: UncheckedAccount<'info>,

    /// wSOL mint (baseMint for Mayflower CPI).
    /// CHECK: Validated against market_config.
    #[account(
        constraint = wsol_mint.key() == market_config.base_mint @ HardigError::InvalidMayflowerAccount,
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

pub fn handler(ctx: Context<ClaimPromoKey>, amount: u64, min_out: u64) -> Result<()> {
    let promo = &ctx.accounts.promo;

    // 1. Check promo is active
    require!(promo.active, HardigError::PromoInactive);

    // 2. Check claims limit (0 = unlimited)
    if promo.max_claims > 0 {
        require!(
            promo.claims_count < promo.max_claims,
            HardigError::PromoMaxClaimsReached
        );
    }

    // 3. Check position matches promo
    require!(
        ctx.accounts.position.authority_seed == promo.authority_seed,
        HardigError::InvalidKey
    );

    // 4. Enforce minimum deposit
    require!(
        amount >= promo.min_deposit_lamports,
        HardigError::InsufficientFunds
    );

    // 5. Populate ClaimReceipt
    let claim_receipt = &mut ctx.accounts.claim_receipt;
    claim_receipt.claimer = ctx.accounts.claimer.key();
    claim_receipt.promo = ctx.accounts.promo.key();
    claim_receipt.bump = ctx.bumps.claim_receipt;

    // 6. Build NFT name
    let nft_name = format!("H\u{00e4}rdig Key - {}", promo.name_suffix);

    // 7. Build attributes
    let permissions = promo.permissions;
    let mut attrs = permission_attributes(permissions);
    attrs.push(Attribute {
        key: "position".to_string(),
        value: ctx.accounts.position.authority_seed.to_string(),
    });
    attrs.push(Attribute {
        key: "promo".to_string(),
        value: ctx.accounts.promo.key().to_string(),
    });
    if !promo.market_name.is_empty() {
        attrs.push(Attribute {
            key: "market".to_string(),
            value: promo.market_name.clone(),
        });
    }

    // 8. Build limited sell/borrow strings if applicable
    let sell_limit_str = if permissions & PERM_LIMITED_SELL != 0 {
        let v = format!(
            "{} navSOL / {}",
            format_sol_amount(promo.sell_capacity),
            slots_to_duration(promo.sell_refill_period)
        );
        attrs.push(Attribute {
            key: "limited_sell".to_string(),
            value: v.clone(),
        });
        Some(v)
    } else {
        None
    };

    let borrow_limit_str = if permissions & PERM_LIMITED_BORROW != 0 {
        let v = format!(
            "{} SOL / {}",
            format_sol_amount(promo.borrow_capacity),
            slots_to_duration(promo.borrow_refill_period)
        );
        attrs.push(Attribute {
            key: "limited_borrow".to_string(),
            value: v.clone(),
        });
        Some(v)
    } else {
        None
    };

    // 9. Determine image override
    let image = if promo.image_uri.is_empty() {
        None
    } else {
        Some(promo.image_uri.as_str())
    };

    // 10. Build metadata URI
    let uri = metadata_uri(
        &nft_name,
        permissions,
        sell_limit_str.as_deref(),
        borrow_limit_str.as_deref(),
        if promo.market_name.is_empty() { None } else { Some(promo.market_name.as_str()) },
        None,
        image,
    );

    // 11. Mint key NFT via MPL-Core CreateV2CpiBuilder
    let config = &ctx.accounts.config;
    let config_seeds: &[&[u8]] = &[ProtocolConfig::SEED, &[config.bump]];

    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.key_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.claimer.to_account_info())
        .owner(Some(&ctx.accounts.claimer.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name(nft_name)
        .uri(uri)
        .plugins(vec![
            PluginAuthorityPair {
                plugin: Plugin::Attributes(Attributes {
                    attribute_list: attrs,
                }),
                authority: Some(PluginAuthority::UpdateAuthority),
            },
            PluginAuthorityPair {
                plugin: Plugin::PermanentBurnDelegate(PermanentBurnDelegate {}),
                authority: Some(PluginAuthority::UpdateAuthority),
            },
            PluginAuthorityPair {
                plugin: Plugin::PermanentTransferDelegate(PermanentTransferDelegate {}),
                authority: Some(PluginAuthority::UpdateAuthority),
            },
        ])
        .invoke_signed(&[config_seeds])?;

    // 12. Initialize KeyState
    let clock = Clock::get()?;
    let current_slot = clock.slot;

    let key_state = &mut ctx.accounts.key_state;
    key_state.asset = ctx.accounts.key_asset.key();
    key_state.bump = ctx.bumps.key_state;
    key_state.authority_seed = ctx.accounts.position.authority_seed;

    if permissions & PERM_LIMITED_SELL != 0 {
        key_state.sell_bucket = RateBucket {
            capacity: promo.sell_capacity,
            refill_period: promo.sell_refill_period,
            level: promo.sell_capacity, // starts full
            last_update: current_slot,
        };
    }
    if permissions & PERM_LIMITED_BORROW != 0 {
        key_state.borrow_bucket = RateBucket {
            capacity: promo.borrow_capacity,
            refill_period: promo.borrow_refill_period,
            level: promo.borrow_capacity, // starts full
            last_update: current_slot,
        };
    }

    // 13. Increment claims_count (checked to prevent overflow)
    ctx.accounts.promo.claims_count = ctx.accounts.promo.claims_count
        .checked_add(1)
        .ok_or(error!(HardigError::PromoMaxClaimsReached))?;

    // 14. Mayflower buy CPI — convert deposit SOL to navSOL
    if amount > 0 {
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

        let ix = mayflower::build_buy_ix(
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
        let authority_seed = ctx.accounts.position.authority_seed;
        let signer_seeds: &[&[&[u8]]] = &[&[b"authority", authority_seed.as_ref(), &[bump]]];

        // Read deposited shares BEFORE the buy CPI
        let pp_info = ctx.accounts.personal_position.to_account_info();
        let shares_before = {
            let data = pp_info.try_borrow_data()?;
            mayflower::read_deposited_shares(&data)?
        };

        invoke_signed(
            &ix,
            &[
                ctx.accounts.program_pda.to_account_info(),
                ctx.accounts.tenant.to_account_info(),
                ctx.accounts.market_group.to_account_info(),
                ctx.accounts.market_meta.to_account_info(),
                ctx.accounts.mayflower_market.to_account_info(),
                pp_info.clone(),
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

        // Read deposited shares AFTER the buy CPI and compute the actual navSOL received
        let shares_after = {
            let data = pp_info.try_borrow_data()?;
            mayflower::read_deposited_shares(&data)?
        };
        let shares_received = shares_after
            .checked_sub(shares_before)
            .ok_or(HardigError::InsufficientFunds)?;

        // Slippage check: verify navSOL shares received >= min_out
        require!(shares_received >= min_out, HardigError::SlippageExceeded);

        ctx.accounts.position.deposited_nav = ctx
            .accounts
            .position
            .deposited_nav
            .checked_add(shares_received)
            .ok_or(HardigError::InsufficientFunds)?;
    }

    Ok(())
}
