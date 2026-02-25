use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::TrustedProvider;

/// Expected Anchor discriminator for ArtworkReceipt.
/// sha256("account:ArtworkReceipt")[..8]
pub const ARTWORK_RECEIPT_DISCRIMINATOR: [u8; 8] = [0xe2, 0x3c, 0x32, 0x41, 0xab, 0x2e, 0xd6, 0x47];

/// Expected Anchor discriminator for TrustedProvider.
/// sha256("account:TrustedProvider")[..8]
const TRUSTED_PROVIDER_DISCRIMINATOR: [u8; 8] = [0xa8, 0x60, 0x60, 0xc0, 0xdc, 0x7c, 0x06, 0xcc];

/// Expected Anchor discriminator for ArtworkImage.
/// sha256("account:ArtworkImage")[..8]
const ARTWORK_IMAGE_DISCRIMINATOR: [u8; 8] = [0x61, 0x94, 0x7d, 0xf4, 0xb6, 0xd2, 0x4a, 0x61];

// Fixed offsets in the ArtworkReceipt account data (113 bytes total).
// Layout: disc(8) + artwork_set(32) + position_seed(32) + buyer(32) + purchased_at(8) + bump(1)
const ARTWORK_SET_OFFSET: usize = 8;
const POSITION_SEED_OFFSET: usize = 40;

// Fixed offset to image_uri in ArtworkImage account data.
// Layout: disc(8) + artwork_set(32) + artist(32) + key_type(1) + permissions(1) + image_uri(4+N) + bump(1)
const ARTWORK_IMAGE_URI_OFFSET: usize = 74;

/// Read and validate the position_seed from an ArtworkReceipt.
pub fn read_receipt_position_seed(data: &[u8]) -> Result<Pubkey> {
    require!(data.len() >= POSITION_SEED_OFFSET + 32, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_RECEIPT_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    Ok(Pubkey::try_from(&data[POSITION_SEED_OFFSET..POSITION_SEED_OFFSET + 32]).unwrap())
}

/// Read the artwork_set pubkey from an ArtworkReceipt.
pub fn read_receipt_artwork_set(data: &[u8]) -> Result<Pubkey> {
    require!(data.len() >= ARTWORK_SET_OFFSET + 32, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_RECEIPT_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    Ok(Pubkey::try_from(&data[ARTWORK_SET_OFFSET..ARTWORK_SET_OFFSET + 32]).unwrap())
}

/// Read the image_uri from an ArtworkImage account.
pub fn read_artwork_image_uri(data: &[u8]) -> Result<String> {
    require!(data.len() > ARTWORK_IMAGE_URI_OFFSET + 4, HardigError::InvalidArtworkReceipt);
    require!(
        data[..8] == ARTWORK_IMAGE_DISCRIMINATOR,
        HardigError::InvalidArtworkReceipt
    );
    read_borsh_string(data, ARTWORK_IMAGE_URI_OFFSET)
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

/// Validate an artwork receipt from remaining_accounts and optionally read an image URI
/// from a separate ArtworkImage PDA.
///
/// Expects `remaining_accounts[0]` = ArtworkReceipt, `remaining_accounts[1]` = TrustedProvider PDA.
/// When `image_key_type` is `Some((key_type, permissions))`, also expects
/// `remaining_accounts[2]` = ArtworkImage PDA, and returns the image URI from it.
///
/// Returns `Some(image_uri)` if artwork is present and image was requested/found,
/// `None` if no artwork_id or image was not requested.
///
/// When `graceful_fallback` is true and the receipt/trusted-provider accounts are missing
/// or the receipt has been closed, returns `Ok(None)` instead of erroring. This is used
/// by `authorize_key` so that a closed receipt doesn't brick key management.
pub fn validate_artwork_receipt<'info>(
    artwork_id: &Option<Pubkey>,
    remaining_accounts: &[AccountInfo<'info>],
    position_authority_seed: &Pubkey,
    program_id: &Pubkey,
    image_key_type: Option<(u8, u8)>,
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

    // If no image requested, we're done (set_position_artwork just validates the trust chain)
    let (key_type, permissions) = match image_key_type {
        Some(kt) => kt,
        None => return Ok(None),
    };

    // Read artwork_set from receipt to derive ArtworkImage PDA
    let artwork_set = read_receipt_artwork_set(&receipt_data)?;
    drop(receipt_data);

    // Need the ArtworkImage account in remaining_accounts[2]
    if remaining_accounts.len() < 3 {
        if graceful_fallback {
            return Ok(None);
        }
        return Err(error!(HardigError::InvalidArtworkReceipt));
    }

    let image_info = &remaining_accounts[2];

    // Derive ArtworkImage PDA from trusted program
    let (expected_image_pda, _) = Pubkey::find_program_address(
        &[b"artwork_image", artwork_set.as_ref(), &[key_type], &[permissions]],
        &trusted_program_id,
    );
    require!(
        image_info.key() == expected_image_pda,
        HardigError::InvalidArtworkReceipt
    );

    // Verify ArtworkImage is owned by the trusted program
    require!(
        *image_info.owner == trusted_program_id,
        HardigError::InvalidArtworkReceipt
    );

    // If ArtworkImage account is empty/closed, graceful fallback
    if image_info.data_len() == 0 {
        if graceful_fallback {
            return Ok(None);
        }
        return Err(error!(HardigError::InvalidArtworkReceipt));
    }

    // Read the image URI from ArtworkImage
    let image_data = image_info.try_borrow_data()?;
    let image_uri = read_artwork_image_uri(&image_data)?;

    Ok(Some(image_uri))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal fake ArtworkReceipt account data blob (113 bytes).
    /// Layout: disc(8) + artwork_set(32) + position_seed(32) + buyer(32) + purchased_at(8) + bump(1)
    fn build_fake_receipt(position_seed: &Pubkey, artwork_set: &Pubkey) -> Vec<u8> {
        let mut data = Vec::new();
        // discriminator (8)
        data.extend_from_slice(&ARTWORK_RECEIPT_DISCRIMINATOR);
        // artwork_set (32)
        data.extend_from_slice(artwork_set.as_ref());
        // position_seed (32)
        data.extend_from_slice(position_seed.as_ref());
        // buyer (32)
        data.extend_from_slice(&[0u8; 32]);
        // purchased_at (8)
        data.extend_from_slice(&0i64.to_le_bytes());
        // bump (1)
        data.push(0);
        data
    }

    #[test]
    fn test_read_position_seed() {
        let seed = Pubkey::new_unique();
        let artwork_set = Pubkey::new_unique();
        let data = build_fake_receipt(&seed, &artwork_set);
        let result = read_receipt_position_seed(&data).unwrap();
        assert_eq!(result, seed);
    }

    #[test]
    fn test_read_artwork_set() {
        let seed = Pubkey::new_unique();
        let artwork_set = Pubkey::new_unique();
        let data = build_fake_receipt(&seed, &artwork_set);
        let result = read_receipt_artwork_set(&data).unwrap();
        assert_eq!(result, artwork_set);
    }

    #[test]
    fn test_wrong_discriminator() {
        let mut data = build_fake_receipt(&Pubkey::new_unique(), &Pubkey::new_unique());
        data[0] = 0xFF; // corrupt discriminator
        assert!(read_receipt_position_seed(&data).is_err());
    }

    #[test]
    fn test_truncated_data() {
        let data = vec![0u8; 20]; // too short
        assert!(read_receipt_position_seed(&data).is_err());
    }

    #[test]
    fn test_read_artwork_image_uri() {
        let uri = "https://example.com/image.png";
        let mut data = Vec::new();
        // ARTWORK_IMAGE_DISCRIMINATOR (8)
        data.extend_from_slice(&ARTWORK_IMAGE_DISCRIMINATOR);
        // artwork_set (32)
        data.extend_from_slice(&[0u8; 32]);
        // artist (32)
        data.extend_from_slice(&[0u8; 32]);
        // key_type (1)
        data.push(0);
        // permissions (1)
        data.push(0);
        // image_uri borsh string (4 + N)
        data.extend_from_slice(&(uri.len() as u32).to_le_bytes());
        data.extend_from_slice(uri.as_bytes());
        // bump (1)
        data.push(0);

        let result = read_artwork_image_uri(&data).unwrap();
        assert_eq!(result, uri);
    }
}
