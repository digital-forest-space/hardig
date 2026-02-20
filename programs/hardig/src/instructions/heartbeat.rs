use anchor_lang::prelude::*;

use crate::state::{PositionNFT, PERM_MANAGE_KEYS};
use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct Heartbeat<'info> {
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key.
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position to update activity for.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,
}

pub fn handler(ctx: Context<Heartbeat>) -> Result<()> {
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.position.authority_seed,
        PERM_MANAGE_KEYS,
    )?;

    ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;

    Ok(())
}
