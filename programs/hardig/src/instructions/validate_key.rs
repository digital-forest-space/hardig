use anchor_lang::prelude::*;
use borsh::BorshDeserialize;
use mpl_core::{
    accounts::BaseAssetV1,
    fetch_plugin,
    types::{Attributes, Key as AssetKey, PluginType},
};

use crate::errors::HardigError;

/// Validates that the signer owns the given MPL-Core key asset, that the asset
/// belongs to the expected position (via update_authority), and that the key
/// has at least one of the required permission bits set.
///
/// Returns the permissions bitmask for further checks (e.g., rate limiting).
pub fn validate_key(
    signer: &Signer,
    key_asset_info: &AccountInfo,
    expected_authority_pda: &Pubkey,
    required: u8,
) -> Result<u8> {
    // Deserialize and validate the asset
    let data = key_asset_info.try_borrow_data()?;

    // Verify this is an MPL-Core AssetV1 account (first byte = Key::AssetV1)
    require!(!data.is_empty(), HardigError::InvalidKey);
    let key = AssetKey::try_from_slice(&data[0..1])
        .map_err(|_| error!(HardigError::InvalidKey))?;
    require!(key == AssetKey::AssetV1, HardigError::InvalidKey);

    // Parse owner (bytes 1..33) and update_authority (bytes 33+)
    require!(data.len() >= 33, HardigError::InvalidKey);
    let owner = Pubkey::try_from(&data[1..33])
        .map_err(|_| error!(HardigError::InvalidKey))?;

    // Read update_authority: borsh-encoded enum (1 byte tag + optional 32-byte pubkey)
    require!(data.len() >= 34, HardigError::InvalidKey);
    let ua_tag = data[33];
    // UpdateAuthority::Address = tag 1
    require!(ua_tag == 1 && data.len() >= 66, HardigError::WrongPosition);
    let ua_pubkey = Pubkey::try_from(&data[34..66])
        .map_err(|_| error!(HardigError::WrongPosition))?;

    drop(data);

    // 1. Signer owns this asset
    require!(owner == signer.key(), HardigError::KeyNotHeld);

    // 2. Asset belongs to the expected position (via update_authority)
    require!(
        ua_pubkey == *expected_authority_pda,
        HardigError::WrongPosition
    );

    // 3. Read permissions from Attributes plugin
    let (_, attributes, _) = fetch_plugin::<BaseAssetV1, Attributes>(
        key_asset_info,
        PluginType::Attributes,
    )
    .map_err(|_| error!(HardigError::InvalidKey))?;

    let permissions = attributes
        .attribute_list
        .iter()
        .find(|a| a.key == "permissions")
        .and_then(|a| a.value.parse::<u8>().ok())
        .ok_or(error!(HardigError::InvalidKey))?;

    // 4. Check required permission(s)
    require!(
        permissions & required != 0,
        HardigError::InsufficientPermission
    );

    Ok(permissions)
}
