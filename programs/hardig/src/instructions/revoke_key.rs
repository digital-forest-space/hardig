use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct RevokeKey<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT token account.
    pub admin_nft_ata: Account<'info, TokenAccount>,

    /// The admin's KeyAuthorization.
    #[account(
        constraint = admin_key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub admin_key_auth: Account<'info, KeyAuthorization>,

    /// The position.
    pub position: Account<'info, PositionNFT>,

    /// The KeyAuthorization to revoke. Closed, rent refunded to admin.
    #[account(
        mut,
        close = admin,
        constraint = target_key_auth.position == position.key() @ HardigError::WrongPosition,
        constraint = target_key_auth.role != KeyRole::Admin @ HardigError::CannotRevokeAdminKey,
    )]
    pub target_key_auth: Account<'info, KeyAuthorization>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<RevokeKey>) -> Result<()> {
    // Validate the admin holds their key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_nft_ata,
        &ctx.accounts.admin_key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin],
    )?;

    // The target_key_auth is closed by the `close = admin` constraint.
    // The key NFT may or may not still exist — we don't check or require it.

    Ok(())
}
