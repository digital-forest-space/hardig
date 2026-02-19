use anchor_lang::prelude::*;
use mpl_core::{
    ID as MPL_CORE_ID,
    instructions::CreateV2CpiBuilder,
    types::{
        Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair,
    },
};

use crate::state::{PositionNFT, PRESET_ADMIN};
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

    /// Per-position authority PDA. Set as update_authority on the key NFT.
    /// CHECK: PDA derived from program, not read.
    #[account(
        seeds = [b"authority", admin_asset.key().as_ref()],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    /// CHECK: MPL-Core program validated by address constraint.
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CreatePosition>, max_reinvest_spread_bps: u16) -> Result<()> {
    // Create the admin key NFT via MPL-Core
    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.admin_asset.to_account_info())
        .payer(&ctx.accounts.admin.to_account_info())
        .owner(Some(&ctx.accounts.admin.to_account_info()))
        .update_authority(Some(&ctx.accounts.program_pda.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name("H\u{00e4}rdig Admin Key".to_string())
        .uri(String::new())
        .plugins(vec![
            PluginAuthorityPair {
                plugin: Plugin::Attributes(Attributes {
                    attribute_list: permission_attributes(PRESET_ADMIN),
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
        .invoke()?;

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
