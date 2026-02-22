use anchor_lang::prelude::*;
use mpl_core::{
    instructions::CreateV2CpiBuilder,
    types::{
        Attribute, Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair,
    },
};

use crate::errors::HardigError;
use crate::state::{
    ClaimReceipt, KeyState, PositionNFT, PromoConfig, ProtocolConfig, RateBucket,
    PERM_LIMITED_BORROW, PERM_LIMITED_SELL,
};
use super::super::{format_sol_amount, metadata_uri, permission_attributes, slots_to_duration};

#[derive(Accounts)]
#[instruction()]
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
    pub promo: Account<'info, PromoConfig>,

    /// One-per-wallet guard. Init fails if PDA already exists.
    #[account(
        init,
        payer = claimer,
        space = 8 + std::mem::size_of::<ClaimReceipt>(),
        seeds = [ClaimReceipt::SEED, promo.key().as_ref(), claimer.key().as_ref()],
        bump,
    )]
    pub claim_receipt: Account<'info, ClaimReceipt>,

    /// The position this promo is for. Mutable because min_deposit_lamports
    /// transfers SOL into this account as a deposit bond.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

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
}

pub fn handler(ctx: Context<ClaimPromoKey>) -> Result<()> {
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

    // 4. Enforce minimum deposit bond
    if promo.min_deposit_lamports > 0 {
        let transfer_ix = anchor_lang::system_program::Transfer {
            from: ctx.accounts.claimer.to_account_info(),
            to: ctx.accounts.position.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            transfer_ix,
        );
        anchor_lang::system_program::transfer(cpi_ctx, promo.min_deposit_lamports)?;
    }

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

    // 8. Build limited sell/borrow strings if applicable
    let sell_limit_str = if promo.sell_capacity > 0 {
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

    let borrow_limit_str = if promo.borrow_capacity > 0 {
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
        None,
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

    if permissions & PERM_LIMITED_SELL != 0 && promo.sell_capacity > 0 {
        key_state.sell_bucket = RateBucket {
            capacity: promo.sell_capacity,
            refill_period: promo.sell_refill_period,
            level: promo.sell_capacity, // starts full
            last_update: current_slot,
        };
    }
    if permissions & PERM_LIMITED_BORROW != 0 && promo.borrow_capacity > 0 {
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

    Ok(())
}
