use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::{PositionNFT, PromoConfig, PERM_MANAGE_KEYS};
use super::super::validate_key::validate_key;

#[derive(Accounts)]
pub struct UpdatePromo<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key.
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position this promo belongs to.
    pub position: Account<'info, PositionNFT>,

    /// The PromoConfig PDA to update.
    #[account(
        mut,
        seeds = [PromoConfig::SEED, position.authority_seed.as_ref(), promo.name_suffix.as_bytes()],
        bump = promo.bump,
        constraint = promo.authority_seed == position.authority_seed @ HardigError::InvalidKey,
    )]
    pub promo: Account<'info, PromoConfig>,
}

pub fn handler(
    ctx: Context<UpdatePromo>,
    active: Option<bool>,
    max_claims: Option<u32>,
) -> Result<()> {
    // Validate admin holds their key with MANAGE_KEYS permission
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.position.authority_seed,
        PERM_MANAGE_KEYS,
    )?;

    let promo = &mut ctx.accounts.promo;

    if let Some(active) = active {
        promo.active = active;
    }

    if let Some(max_claims) = max_claims {
        require!(
            max_claims >= promo.claims_count,
            HardigError::MaxClaimsBelowCurrent
        );
        promo.max_claims = max_claims;
    }

    Ok(())
}
