use anchor_lang::prelude::*;

use crate::state::{PositionState, ProtocolConfig, PERM_MANAGE_KEYS};
use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct SetPositionArtwork<'info> {
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, update_authority, permissions).
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position to update artwork for. Mutable to update artwork_id + last_admin_activity.
    #[account(mut)]
    pub position: Account<'info, PositionState>,

    /// Protocol config PDA — needed to read the collection address for validate_key.
    #[account(
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
    )]
    pub config: Account<'info, ProtocolConfig>,
}

pub fn handler(ctx: Context<SetPositionArtwork>, artwork_id: Option<Pubkey>) -> Result<()> {
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.position.authority_seed,
        PERM_MANAGE_KEYS,
        &ctx.accounts.config.collection,
    )?;

    // If setting artwork, validate the receipt via remaining_accounts
    if artwork_id.is_some() {
        crate::artwork::validate_artwork_receipt(
            &artwork_id,
            ctx.remaining_accounts,
            &ctx.accounts.position.authority_seed,
            ctx.program_id,
            true, // read admin image (just to validate the receipt is well-formed)
        )?;
    }

    ctx.accounts.position.artwork_id = artwork_id;
    ctx.accounts.position.last_admin_activity = Clock::get()?.unix_timestamp;

    Ok(())
}
