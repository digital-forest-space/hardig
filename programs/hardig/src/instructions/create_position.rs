use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use mpl_core::{
    ID as MPL_CORE_ID,
    instructions::CreateV2CpiBuilder,
    types::{
        Attribute, Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair,
    },
};

use crate::errors::HardigError;
use crate::mayflower;
use crate::state::{MarketConfig, PositionNFT, ProtocolConfig, PRESET_ADMIN};
use super::{metadata_uri, permission_attributes};

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

    /// The MarketConfig for the target market.
    pub market_config: Account<'info, MarketConfig>,

    /// CHECK: MPL-Core program validated by address constraint.
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    pub token_program: Program<'info, anchor_spl::token::Token>,

    /// The Mayflower PersonalPosition PDA.
    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub mayflower_personal_position: UncheckedAccount<'info>,

    /// The Mayflower PersonalPosition escrow (user shares).
    /// CHECK: Validated in handler via seed derivation.
    #[account(mut)]
    pub mayflower_user_shares: UncheckedAccount<'info>,

    /// The Mayflower market metadata.
    /// CHECK: Validated against market_config in handler.
    pub mayflower_market_meta: UncheckedAccount<'info>,

    /// The navSOL mint.
    /// CHECK: Validated against market_config in handler.
    pub nav_sol_mint: UncheckedAccount<'info>,

    /// The Mayflower log account.
    /// CHECK: PDA of Mayflower program.
    #[account(mut)]
    pub mayflower_log: UncheckedAccount<'info>,

    /// The Mayflower program.
    /// CHECK: Constant address validated in handler.
    pub mayflower_program: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<CreatePosition>, max_reinvest_spread_bps: u16, name: Option<String>, market_name: String) -> Result<()> {
    let mc = &ctx.accounts.market_config;

    // --- Validate Mayflower account addresses against MarketConfig ---
    require!(
        ctx.accounts.mayflower_market_meta.key() == mc.market_meta,
        HardigError::InvalidMayflowerAccount
    );
    require!(
        ctx.accounts.nav_sol_mint.key() == mc.nav_mint,
        HardigError::InvalidMayflowerAccount
    );
    require!(
        ctx.accounts.mayflower_program.key() == mayflower::MAYFLOWER_PROGRAM_ID,
        HardigError::InvalidMayflowerAccount
    );

    let program_pda = ctx.accounts.program_pda.key();
    let (expected_pp, _) = mayflower::derive_personal_position(&program_pda, &mc.market_meta);
    require!(
        ctx.accounts.mayflower_personal_position.key() == expected_pp,
        HardigError::InvalidPositionPda
    );

    let (expected_escrow, _) = mayflower::derive_personal_position_escrow(&expected_pp);
    require!(
        ctx.accounts.mayflower_user_shares.key() == expected_escrow,
        HardigError::InvalidPositionPda
    );

    let (expected_log, _) = mayflower::derive_log_account();
    require!(
        ctx.accounts.mayflower_log.key() == expected_log,
        HardigError::InvalidMayflowerAccount
    );

    // --- Build NFT name from base + optional suffix ---
    let base_name = "H\u{00e4}rdig Admin Key";
    let nft_name = match &name {
        Some(suffix) => {
            require!(suffix.len() <= 32, HardigError::NameTooLong);
            format!("{} - {}", base_name, suffix)
        }
        None => base_name.to_string(),
    };

    // --- Create admin NFT via MPL-Core CPI ---
    let config = &ctx.accounts.config;
    let config_seeds: &[&[u8]] = &[ProtocolConfig::SEED, &[config.bump]];

    // Validate market_name length
    require!(market_name.len() <= 32, HardigError::NameTooLong);

    let mut attrs = permission_attributes(PRESET_ADMIN);
    attrs.push(Attribute {
        key: "position".to_string(),
        value: ctx.accounts.admin_asset.key().to_string(),
    });
    attrs.push(Attribute {
        key: "market".to_string(),
        value: market_name.clone(),
    });

    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.admin_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.admin.to_account_info())
        .owner(Some(&ctx.accounts.admin.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name(nft_name.clone())
        .uri(metadata_uri(&nft_name, PRESET_ADMIN, None, None, Some(&market_name), None, None))
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

    // --- Init Mayflower PersonalPosition via CPI ---
    let market = mayflower::MarketAddresses {
        nav_mint: mc.nav_mint,
        base_mint: mc.base_mint,
        market_group: mc.market_group,
        market_meta: mc.market_meta,
        mayflower_market: mc.mayflower_market,
        market_base_vault: mc.market_base_vault,
        market_nav_vault: mc.market_nav_vault,
        fee_vault: mc.fee_vault,
    };

    let ix = mayflower::build_init_personal_position_ix(
        ctx.accounts.admin.key(),
        program_pda,
        expected_pp,
        expected_escrow,
        &market,
    );

    let bump = ctx.bumps.program_pda;
    let admin_asset_key = ctx.accounts.admin_asset.key();
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", admin_asset_key.as_ref(), &[bump]]];

    invoke_signed(
        &ix,
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.program_pda.to_account_info(),
            ctx.accounts.mayflower_market_meta.to_account_info(),
            ctx.accounts.nav_sol_mint.to_account_info(),
            ctx.accounts.mayflower_personal_position.to_account_info(),
            ctx.accounts.mayflower_user_shares.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.mayflower_log.to_account_info(),
            ctx.accounts.mayflower_program.to_account_info(),
        ],
        signer_seeds,
    )?;

    // --- Initialize position with all fields set ---
    let position = &mut ctx.accounts.position;
    position.authority_seed = ctx.accounts.admin_asset.key();
    position.position_pda = expected_pp;
    position.market_config = ctx.accounts.market_config.key();
    position.deposited_nav = 0;
    position.user_debt = 0;
    position.max_reinvest_spread_bps = max_reinvest_spread_bps;
    position.last_admin_activity = Clock::get()?.unix_timestamp;
    position.bump = ctx.bumps.position;
    position.authority_bump = ctx.bumps.program_pda;
    position.current_admin_asset = ctx.accounts.admin_asset.key();
    position.recovery_asset = Pubkey::default();
    position.recovery_lockout_secs = 0;
    position.recovery_config_locked = false;

    Ok(())
}
