use anchor_lang::prelude::*;
use mpl_core::{
    ID as MPL_CORE_ID,
    instructions::BurnV1CpiBuilder,
};

use crate::errors::HardigError;
use crate::state::{KeyState, PositionNFT, ProtocolConfig, PERM_MANAGE_KEYS};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct RevokeKey<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, position attribute, permissions).
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position.
    pub position: Account<'info, PositionNFT>,

    /// The target key asset to revoke and burn.
    /// CHECK: Validated in handler (position attribute check via KeyState).
    #[account(mut)]
    pub target_asset: UncheckedAccount<'info>,

    /// The target key's KeyState PDA. Closed, rent refunded to admin.
    #[account(
        mut,
        close = admin,
        constraint = target_key_state.asset == target_asset.key() @ HardigError::InvalidKey,
    )]
    pub target_key_state: Account<'info, KeyState>,

    /// Protocol config PDA — signs burn as the collection's update authority
    /// (PermanentBurnDelegate authority).
    #[account(
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
        constraint = config.collection != Pubkey::default() @ HardigError::CollectionNotCreated,
    )]
    pub config: Account<'info, ProtocolConfig>,

    /// The MPL-Core collection asset for Härdig key NFTs.
    /// CHECK: Validated against config.collection.
    #[account(
        mut,
        constraint = collection.key() == config.collection @ HardigError::CollectionNotCreated,
    )]
    pub collection: UncheckedAccount<'info>,

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
        &ctx.accounts.position.admin_asset,
        PERM_MANAGE_KEYS,
    )?;

    // Prevent revoking the admin key
    require!(
        ctx.accounts.target_asset.key() != ctx.accounts.position.admin_asset,
        HardigError::CannotRevokeAdminKey
    );

    // Burn the target asset via PermanentBurnDelegate.
    // The collection's update_authority (config PDA) is the PermanentBurnDelegate authority.
    let config = &ctx.accounts.config;
    let signer_seeds: &[&[&[u8]]] = &[&[ProtocolConfig::SEED, &[config.bump]]];

    BurnV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.target_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.admin.to_account_info())
        .system_program(Some(&ctx.accounts.system_program.to_account_info()))
        .invoke_signed(signer_seeds)?;

    // target_key_state is closed by the `close = admin` constraint.

    Ok(())
}
