use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

use crate::errors::HardigError;
use crate::state::{KeyAuthorization, KeyRole};

/// Validates that the signer holds an authorized key NFT for the given position
/// and that the key's role is in the allowed set.
pub fn validate_key(
    signer: &Signer,
    key_nft_token_account: &Account<TokenAccount>,
    key_auth: &Account<KeyAuthorization>,
    position: &Pubkey,
    allowed_roles: &[KeyRole],
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

    // 3. This key has the right role
    require!(
        allowed_roles.contains(&key_auth.role),
        HardigError::InsufficientPermission
    );

    Ok(())
}
