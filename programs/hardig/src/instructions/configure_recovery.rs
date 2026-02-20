use anchor_lang::prelude::*;
use mpl_core::{
    ID as MPL_CORE_ID,
    instructions::{CreateV2CpiBuilder, BurnV1CpiBuilder},
    types::{
        Attribute, Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair,
    },
};

use crate::errors::HardigError;
use crate::state::{PositionNFT, ProtocolConfig, PERM_MANAGE_KEYS};
use super::validate_key::validate_key;
use super::metadata_uri;

#[derive(Accounts)]
pub struct ConfigureRecovery<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key.
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position to configure recovery for.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    /// The new MPL-Core asset for the recovery key. Created by MPL-Core CPI.
    #[account(mut)]
    pub recovery_asset: Signer<'info>,

    /// The wallet that will hold the recovery key (should differ from admin).
    /// CHECK: Any wallet can receive the recovery key.
    pub target_wallet: UncheckedAccount<'info>,

    /// Existing recovery key asset to burn (if replacing). Optional.
    /// CHECK: Validated in handler against position.recovery_asset.
    #[account(mut)]
    pub old_recovery_asset: Option<UncheckedAccount<'info>>,

    /// Protocol config PDA — needed to read the collection address and sign.
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

pub fn handler(
    ctx: Context<ConfigureRecovery>,
    lockout_secs: i64,
    lock_config: bool,
    name: Option<String>,
) -> Result<()> {
    // Validate admin holds their key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.position.authority_seed,
        PERM_MANAGE_KEYS,
    )?;

    // Check config is not locked
    require!(
        !ctx.accounts.position.recovery_config_locked,
        HardigError::RecoveryConfigLocked
    );

    // Lockout must be between 1 second and 10 years (prevents permanent lockout footgun)
    const MAX_LOCKOUT_SECS: i64 = 315_360_000; // 10 years
    require!(lockout_secs > 0 && lockout_secs <= MAX_LOCKOUT_SECS, HardigError::InvalidLockout);

    // If replacing an existing recovery key, require and burn the old one
    let position = &ctx.accounts.position;
    if position.recovery_asset != Pubkey::default() {
        let old_asset = ctx.accounts.old_recovery_asset.as_ref()
            .ok_or(error!(HardigError::OldRecoveryAssetRequired))?;
        require!(
            old_asset.key() == position.recovery_asset,
            HardigError::InvalidKey
        );
        let config = &ctx.accounts.config;
        let config_seeds: &[&[u8]] = &[ProtocolConfig::SEED, &[config.bump]];

        BurnV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
            .asset(&old_asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .authority(Some(&ctx.accounts.config.to_account_info()))
            .payer(&ctx.accounts.admin.to_account_info())
            .system_program(Some(&ctx.accounts.system_program.to_account_info()))
            .invoke_signed(&[config_seeds])?;
    }

    // Build NFT name
    let base_name = "H\u{00e4}rdig Recovery Key";
    let nft_name = match &name {
        Some(suffix) => {
            require!(suffix.len() <= 32, HardigError::NameTooLong);
            format!("{} - {}", base_name, suffix)
        }
        None => base_name.to_string(),
    };

    // Build attributes: permissions=0, position binding, recovery marker
    let authority_seed = ctx.accounts.position.authority_seed;
    let attrs = vec![
        Attribute {
            key: "permissions".to_string(),
            value: "0".to_string(),
        },
        Attribute {
            key: "position".to_string(),
            value: authority_seed.to_string(),
        },
        Attribute {
            key: "recovery".to_string(),
            value: "true".to_string(),
        },
    ];

    // Update position state BEFORE CPIs (checks-effects-interactions)
    let position = &mut ctx.accounts.position;
    position.recovery_asset = ctx.accounts.recovery_asset.key();
    position.recovery_lockout_secs = lockout_secs;
    position.last_admin_activity = Clock::get()?.unix_timestamp;

    if lock_config {
        position.recovery_config_locked = true;
    }

    // Create recovery key NFT via MPL-Core
    let config = &ctx.accounts.config;
    let config_seeds: &[&[u8]] = &[ProtocolConfig::SEED, &[config.bump]];

    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.recovery_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.admin.to_account_info())
        .owner(Some(&ctx.accounts.target_wallet.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name(nft_name.clone())
        .uri(metadata_uri(&nft_name, 0, None, None, None, None, None))
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

    Ok(())
}
