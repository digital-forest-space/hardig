use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::state::KeyAuthorization;

/// Validates that the signer holds an authorized key NFT for the given position
/// and that the key has at least one of the required permission bits set.
pub fn validate_key(
    signer: &Signer,
    key_nft_token_account: &Account<TokenAccount>,
    key_auth: &Account<KeyAuthorization>,
    position: &Pubkey,
    required: u8,
) -> Result<()> {
    // 1. Signer holds this NFT
    require!(
        key_nft_token_account.owner == signer.key(),
        HardigError::Unauthorized
    );
    require!(
        key_nft_token_account.mint == key_auth.key_nft_mint,
        HardigError::InvalidKey
    );
    require!(
        key_nft_token_account.amount == 1,
        HardigError::KeyNotHeld
    );

    // 2. This key is authorized for this position
    require!(
        key_auth.position == *position,
        HardigError::WrongPosition
    );

    // 3. This key has the required permission(s)
    require!(
        key_auth.permissions & required != 0,
        HardigError::InsufficientPermission
    );

    Ok(())
}
