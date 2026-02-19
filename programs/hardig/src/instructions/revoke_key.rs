use anchor_lang::prelude::*;
use mpl_core::{
    ID as MPL_CORE_ID,
    instructions::BurnV1CpiBuilder,
};

use crate::errors::HardigError;
use crate::state::{KeyState, PositionNFT, PERM_MANAGE_KEYS};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct RevokeKey<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, update_authority, permissions).
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position.
    pub position: Account<'info, PositionNFT>,

    /// The target key asset to revoke and burn.
    /// CHECK: Validated in handler (update_authority matches program_pda, not the admin key).
    #[account(mut)]
    pub target_asset: UncheckedAccount<'info>,

    /// The target key's KeyState PDA. Closed, rent refunded to admin.
    #[account(
        mut,
        close = admin,
        constraint = target_key_state.asset == target_asset.key() @ HardigError::InvalidKey,
    )]
    pub target_key_state: Account<'info, KeyState>,

    /// Per-position authority PDA (signs burn as PermanentBurnDelegate).
    /// CHECK: PDA derived from program.
    #[account(
        seeds = [b"authority", position.admin_asset.as_ref()],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    /// CHECK: MPL-Core program validated by address constraint.
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<RevokeKey>) -> Result<()> {
    // Validate the admin holds their key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.program_pda.key(),
        PERM_MANAGE_KEYS,
    )?;

    // Prevent revoking the admin key
    require!(
        ctx.accounts.target_asset.key() != ctx.accounts.position.admin_asset,
        HardigError::CannotRevokeAdminKey
    );

    // Burn the target asset via PermanentBurnDelegate (program PDA can always burn).
    let bump = ctx.bumps.program_pda;
    let admin_asset_key = ctx.accounts.position.admin_asset;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", admin_asset_key.as_ref(), &[bump]]];

    BurnV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.target_asset.to_account_info())
        .authority(Some(&ctx.accounts.program_pda.to_account_info()))
        .payer(&ctx.accounts.admin.to_account_info())
        .system_program(Some(&ctx.accounts.system_program.to_account_info()))
        .invoke_signed(signer_seeds)?;

    // target_key_state is closed by the `close = admin` constraint.

    Ok(())
}
