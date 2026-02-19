use anchor_lang::prelude::*;
use borsh::BorshDeserialize;
use mpl_core::{
    accounts::BaseAssetV1,
    fetch_plugin,
    types::{Attributes, Key as AssetKey, PluginType},
    ID,
};

use crate::errors::HardigError;

/// Validates that the signer owns the given MPL-Core key asset, that the asset
/// belongs to the expected position (via the `position` attribute), and that the
/// key has at least one of the required permission bits set.
///
/// Returns the permissions bitmask for further checks (e.g., rate limiting).
pub fn validate_key(
    signer: &Signer,
    key_asset_info: &AccountInfo,
    expected_admin_asset: &Pubkey,
    required: u8,
) -> Result<u8> {
    // Verify the account is owned by the MPL-Core program
    require!(
        *key_asset_info.owner == ID,
        HardigError::InvalidKey
    );

    // Deserialize and validate the asset
    let data = key_asset_info.try_borrow_data()?;

    // Verify this is an MPL-Core AssetV1 account (first byte = Key::AssetV1)
    require!(!data.is_empty(), HardigError::InvalidKey);
    let key = AssetKey::try_from_slice(&data[0..1])
        .map_err(|_| error!(HardigError::InvalidKey))?;
    require!(key == AssetKey::AssetV1, HardigError::InvalidKey);

    // Parse owner (bytes 1..33)
    require!(data.len() >= 33, HardigError::InvalidKey);
    let owner = Pubkey::try_from(&data[1..33])
        .map_err(|_| error!(HardigError::InvalidKey))?;

    drop(data);

    // 1. Signer owns this asset
    require!(owner == signer.key(), HardigError::KeyNotHeld);

    // 2. Read attributes from Attributes plugin
    let (_, attributes, _) = fetch_plugin::<BaseAssetV1, Attributes>(
        key_asset_info,
        PluginType::Attributes,
    )
    .map_err(|_| error!(HardigError::InvalidKey))?;

    // 3. Asset belongs to the expected position (via "position" attribute)
    let position_attr = attributes
        .attribute_list
        .iter()
        .find(|a| a.key == "position")
        .ok_or(error!(HardigError::WrongPosition))?;
    require!(
        position_attr.value == expected_admin_asset.to_string(),
        HardigError::WrongPosition
    );

    // 4. Read permissions
    let permissions = attributes
        .attribute_list
        .iter()
        .find(|a| a.key == "permissions")
        .and_then(|a| a.value.parse::<u8>().ok())
        .ok_or(error!(HardigError::InvalidKey))?;

    // 5. Check required permission(s)
    require!(
        permissions & required != 0,
        HardigError::InsufficientPermission
    );

    Ok(permissions)
}
