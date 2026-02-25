use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::TrustedProvider;

/// Expected Anchor discriminator for ArtworkReceipt.
/// sha256("account:ArtworkReceipt")[..8]
pub const ARTWORK_RECEIPT_DISCRIMINATOR: [u8; 8] = [0xe2, 0x3c, 0x32, 0x41, 0xab, 0x2e, 0xd6, 0x47];

/// Expected Anchor discriminator for TrustedProvider.
/// sha256("account:TrustedProvider")[..8]
const TRUSTED_PROVIDER_DISCRIMINATOR: [u8; 8] = [0xa8, 0x60, 0x60, 0xc0, 0xdc, 0x7c, 0x06, 0xcc];

// Fixed offsets in the ArtworkReceipt account data.
const POSITION_SEED_OFFSET: usize = 40;
const ADMIN_IMAGE_URI_OFFSET: usize = 112;

/// Read and validate the position_seed from an ArtworkReceipt.
pub fn read_receipt_position_seed(data: &[u8]) -> Result<Pubkey> {
    require!(data.len() >= POSITION_SEED_OFFSET + 32, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_RECEIPT_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    Ok(Pubkey::try_from(&data[POSITION_SEED_OFFSET..POSITION_SEED_OFFSET + 32]).unwrap())
}

/// Read the admin_image_uri from an ArtworkReceipt.
pub fn read_admin_image(data: &[u8]) -> Result<String> {
    require!(data.len() > ADMIN_IMAGE_URI_OFFSET + 4, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_RECEIPT_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    read_borsh_string(data, ADMIN_IMAGE_URI_OFFSET)
}

/// Read the delegate_image_uri from an ArtworkReceipt.
/// Located immediately after the admin_image_uri string.
pub fn read_delegate_image(data: &[u8]) -> Result<String> {
    require!(data.len() > ADMIN_IMAGE_URI_OFFSET + 4, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_RECEIPT_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    // Skip past admin_image_uri to find delegate_image_uri
    let admin_len = u32::from_le_bytes(
        data[ADMIN_IMAGE_URI_OFFSET..ADMIN_IMAGE_URI_OFFSET + 4].try_into().unwrap()
    ) as usize;
    require!(admin_len <= 128, HardigError::InvalidArtworkReceipt);
    let delegate_offset = ADMIN_IMAGE_URI_OFFSET + 4 + admin_len;
    read_borsh_string(data, delegate_offset)
}

/// Read a Borsh-encoded String (4-byte LE length prefix + UTF-8 bytes) at the given offset.
fn read_borsh_string(data: &[u8], offset: usize) -> Result<String> {
    require!(data.len() >= offset + 4, HardigError::InvalidArtworkReceipt);
    let len = u32::from_le_bytes(
        data[offset..offset + 4].try_into().unwrap()
    ) as usize;
    require!(len <= 128, HardigError::InvalidArtworkReceipt);
    require!(data.len() >= offset + 4 + len, HardigError::InvalidArtworkReceipt);
    String::from_utf8(data[offset + 4..offset + 4 + len].to_vec())
        .map_err(|_| error!(HardigError::InvalidArtworkReceipt))
}

/// Validate an artwork receipt from remaining_accounts and return the image URI.
///
/// Expects `remaining_accounts[0]` = ArtworkReceipt, `remaining_accounts[1]` = TrustedProvider PDA.
/// Returns `Some(image_uri)` if artwork is present, `None` if no artwork_id.
///
/// When `graceful_fallback` is true and the receipt/trusted-provider accounts are missing
/// or the receipt has been closed, returns `Ok(None)` instead of erroring. This is used
/// by `authorize_key` so that a closed receipt doesn't brick key management.
pub fn validate_artwork_receipt<'info>(
    artwork_id: &Option<Pubkey>,
    remaining_accounts: &[AccountInfo<'info>],
    position_authority_seed: &Pubkey,
    program_id: &Pubkey,
    read_admin_image_uri: bool,
    graceful_fallback: bool,
) -> Result<Option<String>> {
    let receipt_pubkey = match artwork_id {
        Some(pk) => pk,
        None => return Ok(None),
    };

    // If remaining_accounts are missing, either error or fall back gracefully
    if remaining_accounts.len() < 2 {
        if graceful_fallback {
            return Ok(None);
        }
        return Err(error!(HardigError::InvalidArtworkReceipt));
    }

    let receipt_info = &remaining_accounts[0];
    let trusted_info = &remaining_accounts[1];

    // Verify the receipt account matches the artwork_id
    require!(
        receipt_info.key() == *receipt_pubkey,
        HardigError::InvalidArtworkReceipt
    );

    // If the receipt account has been closed (zero data/lamports), fall back gracefully
    if graceful_fallback && receipt_info.data_len() == 0 {
        return Ok(None);
    }

    // Deserialize and validate the TrustedProvider PDA
    let trusted_data = trusted_info.try_borrow_data()?;
    require!(trusted_data.len() >= TrustedProvider::SIZE, HardigError::UntrustedProvider);

    // Verify TrustedProvider discriminator
    require!(
        trusted_data[..8] == TRUSTED_PROVIDER_DISCRIMINATOR,
        HardigError::UntrustedProvider
    );

    // Read program_id and active flag from TrustedProvider
    let trusted_program_id = Pubkey::try_from(&trusted_data[8..40])
        .map_err(|_| error!(HardigError::UntrustedProvider))?;
    let active = trusted_data[72] != 0;
    drop(trusted_data);

    require!(active, HardigError::UntrustedProvider);

    // Verify the TrustedProvider PDA is owned by this program (defense-in-depth)
    require!(
        *trusted_info.owner == *program_id,
        HardigError::UntrustedProvider
    );

    // Verify the TrustedProvider PDA is correctly derived
    let (expected_trusted_pda, _) = Pubkey::find_program_address(
        &[TrustedProvider::SEED, trusted_program_id.as_ref()],
        program_id,
    );
    require!(
        trusted_info.key() == expected_trusted_pda,
        HardigError::UntrustedProvider
    );

    // Verify the receipt is owned by the trusted program
    require!(
        *receipt_info.owner == trusted_program_id,
        HardigError::UntrustedProvider
    );

    // Read and validate receipt data
    let receipt_data = receipt_info.try_borrow_data()?;
    let position_seed = read_receipt_position_seed(&receipt_data)?;
    require!(
        position_seed == *position_authority_seed,
        HardigError::ArtworkReceiptPositionMismatch
    );

    // Read the appropriate image URI
    let image_uri = if read_admin_image_uri {
        read_admin_image(&receipt_data)?
    } else {
        read_delegate_image(&receipt_data)?
    };

    Ok(Some(image_uri))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal fake ArtworkReceipt account data blob.
    fn build_fake_receipt(position_seed: &Pubkey, admin_image: &str, delegate_image: &str) -> Vec<u8> {
        let mut data = Vec::new();
        // discriminator (8)
        data.extend_from_slice(&ARTWORK_RECEIPT_DISCRIMINATOR);
        // artwork_set (32)
        data.extend_from_slice(&[0u8; 32]);
        // position_seed (32)
        data.extend_from_slice(position_seed.as_ref());
        // buyer (32)
        data.extend_from_slice(&[0u8; 32]);
        // purchased_at (8)
        data.extend_from_slice(&0i64.to_le_bytes());
        // admin_image_uri (4 + len)
        data.extend_from_slice(&(admin_image.len() as u32).to_le_bytes());
        data.extend_from_slice(admin_image.as_bytes());
        // delegate_image_uri (4 + len)
        data.extend_from_slice(&(delegate_image.len() as u32).to_le_bytes());
        data.extend_from_slice(delegate_image.as_bytes());
        data
    }

    #[test]
    fn test_read_position_seed() {
        let seed = Pubkey::new_unique();
        let data = build_fake_receipt(&seed, "https://example.com/admin.png", "https://example.com/delegate.png");
        let result = read_receipt_position_seed(&data).unwrap();
        assert_eq!(result, seed);
    }

    #[test]
    fn test_read_admin_image() {
        let seed = Pubkey::new_unique();
        let data = build_fake_receipt(&seed, "https://example.com/admin.png", "https://example.com/delegate.png");
        let result = read_admin_image(&data).unwrap();
        assert_eq!(result, "https://example.com/admin.png");
    }

    #[test]
    fn test_read_delegate_image() {
        let seed = Pubkey::new_unique();
        let data = build_fake_receipt(&seed, "https://example.com/admin.png", "https://example.com/delegate.png");
        let result = read_delegate_image(&data).unwrap();
        assert_eq!(result, "https://example.com/delegate.png");
    }

    #[test]
    fn test_wrong_discriminator() {
        let mut data = build_fake_receipt(&Pubkey::new_unique(), "img", "img");
        data[0] = 0xFF; // corrupt discriminator
        assert!(read_receipt_position_seed(&data).is_err());
    }

    #[test]
    fn test_image_uri_too_long() {
        let long_uri = "x".repeat(129);
        let data = build_fake_receipt(&Pubkey::new_unique(), &long_uri, "ok");
        assert!(read_admin_image(&data).is_err());
    }

    #[test]
    fn test_truncated_data() {
        let data = vec![0u8; 20]; // too short
        assert!(read_receipt_position_seed(&data).is_err());
    }
}
