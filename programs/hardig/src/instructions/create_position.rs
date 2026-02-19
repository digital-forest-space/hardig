use anchor_lang::prelude::*;
use mpl_core::{
    ID as MPL_CORE_ID,
    instructions::CreateV2CpiBuilder,
    types::{
        Attribute, Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair,
    },
};

use crate::errors::HardigError;
use crate::state::{PositionNFT, ProtocolConfig, PRESET_ADMIN};
use super::permission_attributes;

#[derive(Accounts)]
pub struct CreatePosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The MPL-Core asset for the admin key NFT. Created by MPL-Core CPI.
    #[account(mut)]
    pub admin_asset: Signer<'info>,

    /// The position account.
    #[account(
        init,
        payer = admin,
        space = PositionNFT::SIZE,
        seeds = [PositionNFT::SEED, admin_asset.key().as_ref()],
        bump,
    )]
    pub position: Account<'info, PositionNFT>,

    /// Per-position authority PDA. Used for Mayflower CPI signing;
    /// bump stored in PositionNFT.authority_bump.
    /// CHECK: PDA derived from program, not read.
    #[account(
        seeds = [b"authority", admin_asset.key().as_ref()],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    /// Protocol config PDA — needed to read the collection address and sign as
    /// collection authority when adding the asset to the collection.
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

pub fn handler(ctx: Context<CreatePosition>, max_reinvest_spread_bps: u16) -> Result<()> {
    let config = &ctx.accounts.config;
    let config_seeds: &[&[u8]] = &[ProtocolConfig::SEED, &[config.bump]];

    // Build attributes including position binding
    let mut attrs = permission_attributes(PRESET_ADMIN);
    attrs.push(Attribute {
        key: "position".to_string(),
        value: ctx.accounts.admin_asset.key().to_string(),
    });

    // Create the admin key NFT via MPL-Core, adding it to the collection.
    // update_authority is inherited from the collection (config PDA).
    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.admin_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.admin.to_account_info())
        .owner(Some(&ctx.accounts.admin.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name("H\u{00e4}rdig Admin Key".to_string())
        .uri(String::new())
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

    // Initialize the position
    let position = &mut ctx.accounts.position;
    position.admin_asset = ctx.accounts.admin_asset.key();
    position.position_pda = Pubkey::default(); // Set during init_mayflower_position
    position.market_config = Pubkey::default(); // Set during init_mayflower_position
    position.deposited_nav = 0;
    position.user_debt = 0;
    position.max_reinvest_spread_bps = max_reinvest_spread_bps;
    position.last_admin_activity = Clock::get()?.unix_timestamp;
    position.bump = ctx.bumps.position;
    position.authority_bump = ctx.bumps.program_pda;

    Ok(())
}
