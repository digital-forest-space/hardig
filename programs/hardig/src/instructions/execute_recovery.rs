use anchor_lang::prelude::*;
use borsh::BorshDeserialize;
use mpl_core::{
    ID as MPL_CORE_ID,
    accounts::BaseAssetV1,
    fetch_plugin,
    instructions::{CreateV2CpiBuilder, BurnV1CpiBuilder},
    types::{
        Attribute, Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair, Key as AssetKey, PluginType,
    },
};

use crate::errors::HardigError;
use crate::state::{PositionNFT, ProtocolConfig, PRESET_ADMIN};
use super::{permission_attributes, metadata_uri};

#[derive(Accounts)]
pub struct ExecuteRecovery<'info> {
    #[account(mut)]
    pub recovery_holder: Signer<'info>,

    /// The recovery key NFT (MPL-Core asset). Mutable because it gets burned.
    /// CHECK: Validated in handler (owner check, matches position.recovery_asset).
    #[account(mut)]
    pub recovery_key_asset: UncheckedAccount<'info>,

    /// The position to recover.
    #[account(mut)]
    pub position: Account<'info, PositionNFT>,

    /// The old admin's MPL-Core asset (to burn).
    /// CHECK: Validated in handler against position.current_admin_asset.
    #[account(mut)]
    pub old_admin_asset: UncheckedAccount<'info>,

    /// The new MPL-Core asset for the new admin key. Created by MPL-Core CPI.
    #[account(mut)]
    pub new_admin_asset: Signer<'info>,

    /// Protocol config PDA — needed to sign as collection authority.
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

pub fn handler(ctx: Context<ExecuteRecovery>) -> Result<()> {
    let position = &ctx.accounts.position;

    // 1. Verify recovery is configured
    require!(
        position.recovery_asset != Pubkey::default(),
        HardigError::RecoveryNotConfigured
    );

    // 2. Verify the recovery key asset matches
    require!(
        ctx.accounts.recovery_key_asset.key() == position.recovery_asset,
        HardigError::InvalidKey
    );

    // 3. Verify recovery key is owned by MPL-Core
    require!(
        *ctx.accounts.recovery_key_asset.owner == MPL_CORE_ID,
        HardigError::InvalidKey
    );

    // 4. Verify signer owns the recovery key (parse MPL-Core asset owner from bytes 1..33)
    {
        let data = ctx.accounts.recovery_key_asset.try_borrow_data()?;
        require!(!data.is_empty(), HardigError::InvalidKey);
        let key = AssetKey::try_from_slice(&data[0..1])
            .map_err(|_| error!(HardigError::InvalidKey))?;
        require!(key == AssetKey::AssetV1, HardigError::InvalidKey);
        require!(data.len() >= 33, HardigError::InvalidKey);
        let owner = Pubkey::try_from(&data[1..33])
            .map_err(|_| error!(HardigError::InvalidKey))?;
        require!(
            owner == ctx.accounts.recovery_holder.key(),
            HardigError::KeyNotHeld
        );
    }

    // 5. Verify lockout period has expired
    let now = Clock::get()?.unix_timestamp;
    let elapsed = now.saturating_sub(position.last_admin_activity);
    require!(
        elapsed >= position.recovery_lockout_secs,
        HardigError::RecoveryLockoutNotExpired
    );

    // 6. Verify old admin asset matches
    require!(
        ctx.accounts.old_admin_asset.key() == position.current_admin_asset,
        HardigError::InvalidKey
    );

    // 7. Read old admin key's market attribute and name for propagation to new admin key
    let old_admin_info = &ctx.accounts.old_admin_asset.to_account_info();

    // Read the old admin asset's name
    let old_name = {
        let data = old_admin_info.try_borrow_data()?;
        let base = BaseAssetV1::from_bytes(&data)
            .map_err(|_| error!(HardigError::InvalidKey))?;
        base.name.clone()
    };

    // Read the old admin asset's market attribute
    let old_market = {
        let (_, attrs, _) = fetch_plugin::<BaseAssetV1, Attributes>(
            old_admin_info,
            PluginType::Attributes,
        )
        .map_err(|_| error!(HardigError::InvalidKey))?;
        attrs
            .attribute_list
            .iter()
            .find(|a| a.key == "market")
            .map(|a| a.value.clone())
            .unwrap_or_default()
    };

    let config = &ctx.accounts.config;
    let config_seeds: &[&[u8]] = &[ProtocolConfig::SEED, &[config.bump]];

    // 8. Create new admin key NFT
    let authority_seed = position.authority_seed;

    let mut attrs = permission_attributes(PRESET_ADMIN);
    attrs.push(Attribute {
        key: "position".to_string(),
        value: authority_seed.to_string(),
    });
    if !old_market.is_empty() {
        attrs.push(Attribute {
            key: "market".to_string(),
            value: old_market.clone(),
        });
    }

    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.new_admin_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.recovery_holder.to_account_info())
        .owner(Some(&ctx.accounts.recovery_holder.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name(old_name.clone())
        .uri(metadata_uri(&old_name, PRESET_ADMIN, None, None, if old_market.is_empty() { None } else { Some(&old_market) }, None))
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

    // 9. Burn old admin asset
    BurnV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.old_admin_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.recovery_holder.to_account_info())
        .system_program(Some(&ctx.accounts.system_program.to_account_info()))
        .invoke_signed(&[config_seeds])?;

    // 10. Burn recovery key
    BurnV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.recovery_key_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.recovery_holder.to_account_info())
        .system_program(Some(&ctx.accounts.system_program.to_account_info()))
        .invoke_signed(&[config_seeds])?;

    // 11. Update position state
    let position = &mut ctx.accounts.position;
    position.current_admin_asset = ctx.accounts.new_admin_asset.key();
    position.recovery_asset = Pubkey::default();
    position.recovery_lockout_secs = 0;
    position.recovery_config_locked = false;
    position.last_admin_activity = now;

    Ok(())
}
