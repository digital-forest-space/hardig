use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::{KeyCreatorOrigin, PositionNFT, PromoConfig, ProtocolConfig, PERM_MANAGE_KEYS};
use super::super::validate_key::validate_key;
use super::super::validate_delegated_permissions;

#[derive(Accounts)]
#[instruction(name_suffix: String)]
pub struct CreatePromo<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key.
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position this promo is for.
    pub position: Account<'info, PositionNFT>,

    /// The PromoConfig PDA to create. Seeds include name_suffix to allow multiple promos per position.
    #[account(
        init,
        payer = admin,
        space = PromoConfig::SIZE,
        seeds = [PromoConfig::SEED, position.authority_seed.as_ref(), name_suffix.as_bytes()],
        bump,
    )]
    pub promo: Account<'info, PromoConfig>,

    /// Protocol config PDA — provides collection pubkey for key validation.
    #[account(
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
    )]
    pub config: Account<'info, ProtocolConfig>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<CreatePromo>,
    name_suffix: String,
    permissions: u8,
    borrow_capacity: u64,
    borrow_refill_period: u64,
    sell_capacity: u64,
    sell_refill_period: u64,
    min_deposit_lamports: u64,
    max_claims: u32,
    image_uri: String,
) -> Result<()> {
    // Validate admin holds their key with MANAGE_KEYS permission
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.position.authority_seed,
        PERM_MANAGE_KEYS,
        &ctx.accounts.config.collection,
    )?;

    // Validate name_suffix length
    require!(name_suffix.len() <= 64, HardigError::NameTooLong);

    // Validate image_uri length
    require!(
        image_uri.len() <= PromoConfig::MAX_IMAGE_URI_LEN,
        HardigError::ImageUriTooLong
    );

    // Validate permissions + rate-limit params for promo-created keys
    validate_delegated_permissions(
        KeyCreatorOrigin::Promo,
        permissions,
        sell_capacity,
        sell_refill_period,
        borrow_capacity,
        borrow_refill_period,
    )?;

    // Populate the PromoConfig
    let promo = &mut ctx.accounts.promo;
    promo.authority_seed = ctx.accounts.position.authority_seed;
    promo.permissions = permissions;
    promo.borrow_capacity = borrow_capacity;
    promo.borrow_refill_period = borrow_refill_period;
    promo.sell_capacity = sell_capacity;
    promo.sell_refill_period = sell_refill_period;
    promo.min_deposit_lamports = min_deposit_lamports;
    promo.max_claims = max_claims;
    promo.claims_count = 0;
    promo.active = true;
    promo.name_suffix = name_suffix;
    promo.image_uri = image_uri;
    promo.bump = ctx.bumps.promo;

    Ok(())
}
