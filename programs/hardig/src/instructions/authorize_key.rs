use anchor_lang::prelude::*;
use mpl_core::{
    ID as MPL_CORE_ID,
    accounts::BaseAssetV1,
    fetch_plugin,
    instructions::CreateV2CpiBuilder,
    types::{
        Attribute, Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair, PluginType,
    },
};

use crate::errors::HardigError;
use crate::state::{
    KeyCreatorOrigin, KeyState, PositionState, ProtocolConfig, RateBucket,
    PERM_LIMITED_BORROW, PERM_LIMITED_SELL, PERM_MANAGE_KEYS,
};

use super::validate_key::validate_key;
use super::{permission_attributes, metadata_uri, format_sol_amount, slots_to_duration, validate_delegated_permissions};

#[derive(Accounts)]
pub struct AuthorizeKey<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, update_authority, permissions).
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position to authorize a key for. Mutable to update last_admin_activity.
    #[account(mut)]
    pub position: Account<'info, PositionState>,

    /// The MPL-Core asset for the new key NFT. Created by MPL-Core CPI.
    #[account(mut)]
    pub new_key_asset: Signer<'info>,

    /// The wallet that will receive the new key NFT.
    /// CHECK: Any wallet can receive a key.
    pub target_wallet: UncheckedAccount<'info>,

    /// KeyState PDA for the new key (tracks mutable state like rate-limit buckets).
    #[account(
        init,
        payer = admin,
        space = KeyState::SIZE,
        seeds = [KeyState::SEED, new_key_asset.key().as_ref()],
        bump,
    )]
    pub key_state: Account<'info, KeyState>,

    /// Protocol config PDA — needed to read the collection address.
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
    ctx: Context<AuthorizeKey>,
    permissions: u8,
    sell_bucket_capacity: u64,
    sell_refill_period_slots: u64,
    borrow_bucket_capacity: u64,
    borrow_refill_period_slots: u64,
    name: Option<String>,
) -> Result<()> {
    // Validate the admin holds their key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.position.authority_seed,
        PERM_MANAGE_KEYS,
        &ctx.accounts.config.collection,
    )?;

    // Validate permissions + rate-limit params for admin-created delegated keys
    validate_delegated_permissions(
        KeyCreatorOrigin::Admin,
        permissions,
        sell_bucket_capacity,
        sell_refill_period_slots,
        borrow_bucket_capacity,
        borrow_refill_period_slots,
    )?;

    // --- Read admin asset's name and market attribute ---
    // Deserialize the admin key's MPL-Core BaseAssetV1 to get the position name
    let admin_asset_info = &ctx.accounts.admin_key_asset.to_account_info();
    let admin_data = admin_asset_info.try_borrow_data()?;
    let admin_base = BaseAssetV1::from_bytes(&admin_data)
        .map_err(|_| error!(HardigError::InvalidKey))?;
    let admin_asset_name = admin_base.name.clone();
    drop(admin_data);

    // Read the admin asset's "market" attribute from its Attributes plugin
    let (_, admin_attributes, _) = fetch_plugin::<BaseAssetV1, Attributes>(
        admin_asset_info,
        PluginType::Attributes,
    )
    .map_err(|_| error!(HardigError::InvalidKey))?;
    let admin_market = admin_attributes
        .attribute_list
        .iter()
        .find(|a| a.key == "market")
        .map(|a| a.value.clone())
        .unwrap_or_default();

    // --- Validate artwork receipt if present on the position ---
    let image_override = crate::artwork::validate_artwork_receipt(
        &ctx.accounts.position.artwork_id,
        ctx.remaining_accounts,
        &ctx.accounts.position.authority_seed,
        ctx.program_id,
        false, // read delegate_image_uri
    )?;

    // Build attribute list with human-readable permissions + position binding
    let mut attrs = permission_attributes(permissions);
    attrs.push(Attribute {
        key: "position".to_string(),
        value: ctx.accounts.position.authority_seed.to_string(),
    });
    attrs.push(Attribute {
        key: "position_name".to_string(),
        value: admin_asset_name.clone(),
    });
    attrs.push(Attribute {
        key: "market".to_string(),
        value: admin_market.clone(),
    });
    let sell_limit_str = if permissions & PERM_LIMITED_SELL != 0 {
        let v = format!("{} navSOL / {}", format_sol_amount(sell_bucket_capacity), slots_to_duration(sell_refill_period_slots));
        attrs.push(Attribute { key: "limited_sell".to_string(), value: v.clone() });
        Some(v)
    } else {
        None
    };
    let borrow_limit_str = if permissions & PERM_LIMITED_BORROW != 0 {
        let v = format!("{} SOL / {}", format_sol_amount(borrow_bucket_capacity), slots_to_duration(borrow_refill_period_slots));
        attrs.push(Attribute { key: "limited_borrow".to_string(), value: v.clone() });
        Some(v)
    } else {
        None
    };

    // Build NFT name from base + optional suffix
    let base_name = "H\u{00e4}rdig Key";
    let nft_name = match &name {
        Some(suffix) => {
            require!(suffix.len() <= 32, HardigError::NameTooLong);
            format!("{} - {}", base_name, suffix)
        }
        None => base_name.to_string(),
    };

    // Create the new key NFT via MPL-Core, adding it to the collection
    let config = &ctx.accounts.config;
    let config_seeds: &[&[u8]] = &[ProtocolConfig::SEED, &[config.bump]];

    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.new_key_asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .payer(&ctx.accounts.admin.to_account_info())
        .owner(Some(&ctx.accounts.target_wallet.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name(nft_name.clone())
        .uri(metadata_uri(
            &nft_name,
            permissions,
            sell_limit_str.as_deref(),
            borrow_limit_str.as_deref(),
            Some(&admin_market),
            Some(&admin_asset_name),
            image_override.as_deref(),
        ))
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

    let clock = Clock::get()?;
    let current_slot = clock.slot;

    // Update last_admin_activity so key management resets the recovery lockout
    ctx.accounts.position.last_admin_activity = clock.unix_timestamp;

    // Initialize the KeyState
    let key_state = &mut ctx.accounts.key_state;
    key_state.asset = ctx.accounts.new_key_asset.key();
    key_state.bump = ctx.bumps.key_state;
    key_state.authority_seed = ctx.accounts.position.authority_seed;

    if permissions & PERM_LIMITED_SELL != 0 {
        key_state.sell_bucket = RateBucket {
            capacity: sell_bucket_capacity,
            refill_period: sell_refill_period_slots,
            level: sell_bucket_capacity, // starts full
            last_update: current_slot,
        };
    }
    if permissions & PERM_LIMITED_BORROW != 0 {
        key_state.borrow_bucket = RateBucket {
            capacity: borrow_bucket_capacity,
            refill_period: borrow_refill_period_slots,
            level: borrow_bucket_capacity, // starts full
            last_update: current_slot,
        };
    }

    Ok(())
}
