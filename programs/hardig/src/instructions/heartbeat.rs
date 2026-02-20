use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::{PositionNFT, PERM_MANAGE_KEYS};
use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Heartbeat<'info> {
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset). Must be the current admin key.
    /// CHECK: Validated in handler via validate_key + admin asset identity check.
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position to update activity for.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,
}

pub fn handler(ctx: Context<Heartbeat>) -> Result<()> {
    // Heartbeat must be admin-only: it resets the recovery lockout timer.
    // Enforce this structurally by checking the key is the current admin asset,
    // not just that it has PERM_MANAGE_KEYS (which is only admin-exclusive by
    // convention in authorize_key).
    require!(
        ctx.accounts.admin_key_asset.key() == ctx.accounts.position.current_admin_asset,
        HardigError::AdminOnly
    );

    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.position.authority_seed,
        PERM_MANAGE_KEYS,
    )?;

    ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;

    Ok(())
}
