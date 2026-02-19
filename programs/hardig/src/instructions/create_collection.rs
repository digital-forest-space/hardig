use anchor_lang::prelude::*;
use mpl_core::{
    ID as MPL_CORE_ID,
    instructions::CreateCollectionV2CpiBuilder,
};

use crate::errors::HardigError;
use crate::state::ProtocolConfig;

#[derive(Accounts)]
pub struct CreateCollection<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
        constraint = config.admin == admin.key() @ HardigError::Unauthorized,
        constraint = config.collection == Pubkey::default() @ HardigError::CollectionAlreadyCreated,
    )]
    pub config: Account<'info, ProtocolConfig>,

    /// The MPL-Core collection asset. Fresh keypair, signed by the caller.
    #[account(mut)]
    pub collection_asset: Signer<'info>,

    /// CHECK: MPL-Core program validated by address constraint.
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CreateCollection>, uri: String) -> Result<()> {
    let config = &ctx.accounts.config;
    let signer_seeds: &[&[u8]] = &[ProtocolConfig::SEED, &[config.bump]];

    CreateCollectionV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .collection(&ctx.accounts.collection_asset.to_account_info())
        .payer(&ctx.accounts.admin.to_account_info())
        .update_authority(Some(&ctx.accounts.config.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name("H\u{00e4}rdig Keys".to_string())
        .uri(uri)
        .invoke_signed(&[signer_seeds])?;

    let config = &mut ctx.accounts.config;
    config.collection = ctx.accounts.collection_asset.key();

    Ok(())
}
