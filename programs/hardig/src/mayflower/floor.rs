use anchor_lang::prelude::*;

use crate::errors::HardigError;
use super::constants::*;

/// Read the floor price from the Mayflower market account data.
///
/// Floor price is stored as a Rust Decimal (16 bytes) at offset 104.
/// Returns the price as lamports-per-navSOL-lamport (scaled by 1e9).
pub fn read_floor_price(market_data: &[u8]) -> Result<u64> {
    require!(
        market_data.len() > MARKET_FLOOR_PRICE_OFFSET + RUST_DECIMAL_SIZE,
        HardigError::InvalidPositionPda
    );
    // Verify MayflowerMarket discriminator
    require!(
        market_data[..8] == MARKET_DISCRIMINATOR,
        HardigError::InvalidMayflowerAccount
    );

    let decimal_bytes =
        &market_data[MARKET_FLOOR_PRICE_OFFSET..MARKET_FLOOR_PRICE_OFFSET + RUST_DECIMAL_SIZE];

    decode_rust_decimal_to_lamports(decimal_bytes)
}

/// Read deposited shares from a PersonalPosition account.
pub fn read_deposited_shares(position_data: &[u8]) -> Result<u64> {
    require!(
        position_data.len() >= PP_DEPOSITED_SHARES_OFFSET + 8,
        HardigError::InvalidPositionPda
    );
    // Verify PersonalPosition discriminator
    require!(
        position_data[..8] == PP_DISCRIMINATOR,
        HardigError::InvalidPositionPda
    );

    let bytes: [u8; 8] = position_data[PP_DEPOSITED_SHARES_OFFSET..PP_DEPOSITED_SHARES_OFFSET + 8]
        .try_into()
        .map_err(|_| error!(HardigError::InvalidPositionPda))?;

    Ok(u64::from_le_bytes(bytes))
}

/// Read current debt from a PersonalPosition account.
pub fn read_debt(position_data: &[u8]) -> Result<u64> {
    require!(
        position_data.len() >= PP_DEBT_OFFSET + 8,
        HardigError::InvalidPositionPda
    );
    // Verify PersonalPosition discriminator
    require!(
        position_data[..8] == PP_DISCRIMINATOR,
        HardigError::InvalidPositionPda
    );

    let bytes: [u8; 8] = position_data[PP_DEBT_OFFSET..PP_DEBT_OFFSET + 8]
        .try_into()
        .map_err(|_| error!(HardigError::InvalidPositionPda))?;

    Ok(u64::from_le_bytes(bytes))
}

/// Calculate available borrow capacity for a position.
///
/// capacity = (deposited_shares * floor_price / 1e9) - current_debt
pub fn calculate_borrow_capacity(
    deposited_shares: u64,
    floor_price_lamports: u64,
    current_debt: u64,
) -> Result<u64> {
    let floor_value = (deposited_shares as u128)
        .checked_mul(floor_price_lamports as u128)
        .ok_or(error!(HardigError::InsufficientFunds))?
        / 1_000_000_000u128;

    let capacity = floor_value.saturating_sub(current_debt as u128);

    u64::try_from(capacity).map_err(|_| error!(HardigError::InsufficientFunds))
}

/// Decode a 16-byte Rust Decimal into lamports (scaled by 1e9).
///
/// Layout:
///   bytes[0..4] = flags (u32 LE), where byte[2] is the scale
///   bytes[4..16] = 96-bit unsigned mantissa (little-endian)
///
/// value = mantissa / 10^scale
/// Returns value * 1e9 as u64.
fn decode_rust_decimal_to_lamports(bytes: &[u8]) -> Result<u64> {
    // Sign bit is the MSB of byte 3 (flags word). A negative floor price
    // should never occur, but if it does, treat it as zero to avoid
    // inflating borrow capacity via underflow.
    if bytes[3] & 0x80 != 0 {
        return Ok(0);
    }

    let scale = bytes[2] as u32;

    let mut mantissa: u128 = 0;
    for i in 4..16 {
        mantissa |= (bytes[i] as u128) << (8 * (i - 4));
    }

    let scaled = mantissa
        .checked_mul(1_000_000_000u128)
        .ok_or(error!(HardigError::InsufficientFunds))?;

    let divisor = 10u128
        .checked_pow(scale)
        .ok_or(error!(HardigError::InsufficientFunds))?;

    let result = scaled / divisor;

    u64::try_from(result).map_err(|_| error!(HardigError::InsufficientFunds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borrow_capacity_basic() {
        let deposited = 10_000_000_000u64;
        let floor = 1_000_000_000u64;
        let cap = calculate_borrow_capacity(deposited, floor, 0).unwrap();
        assert_eq!(cap, 10_000_000_000);
    }

    #[test]
    fn test_borrow_capacity_with_debt() {
        let deposited = 10_000_000_000u64;
        let floor = 1_500_000_000u64;
        let debt = 5_000_000_000u64;
        let cap = calculate_borrow_capacity(deposited, floor, debt).unwrap();
        assert_eq!(cap, 10_000_000_000);
    }

    #[test]
    fn test_borrow_capacity_fully_borrowed() {
        let deposited = 10_000_000_000u64;
        let floor = 1_000_000_000u64;
        let debt = 10_000_000_000u64;
        let cap = calculate_borrow_capacity(deposited, floor, debt).unwrap();
        assert_eq!(cap, 0);
    }

    #[test]
    fn test_borrow_capacity_over_borrowed() {
        let cap = calculate_borrow_capacity(10_000_000_000, 1_000_000_000, 20_000_000_000).unwrap();
        assert_eq!(cap, 0);
    }

    #[test]
    fn test_decode_rust_decimal_one() {
        let mut bytes = [0u8; 16];
        bytes[2] = 0;
        bytes[4] = 1;
        let result = decode_rust_decimal_to_lamports(&bytes).unwrap();
        assert_eq!(result, 1_000_000_000);
    }

    #[test]
    fn test_decode_rust_decimal_one_point_five() {
        let mut bytes = [0u8; 16];
        bytes[2] = 1;
        bytes[4] = 15;
        let result = decode_rust_decimal_to_lamports(&bytes).unwrap();
        assert_eq!(result, 1_500_000_000);
    }

    #[test]
    fn test_decode_rust_decimal_zero() {
        let bytes = [0u8; 16];
        let result = decode_rust_decimal_to_lamports(&bytes).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_decode_rust_decimal_negative() {
        let mut bytes = [0u8; 16];
        bytes[2] = 0; // scale
        bytes[3] = 0x80; // sign bit set (negative)
        bytes[4] = 1; // mantissa = 1
        let result = decode_rust_decimal_to_lamports(&bytes).unwrap();
        assert_eq!(result, 0); // negative → 0
    }

    #[test]
    fn test_read_floor_price_valid_discriminator() {
        // Build a minimal Market account: discriminator + enough bytes for floor price
        let mut data = vec![0u8; MARKET_FLOOR_PRICE_OFFSET + RUST_DECIMAL_SIZE + 1];
        data[..8].copy_from_slice(&MARKET_DISCRIMINATOR);
        // Write floor price = 1.0 SOL as Rust Decimal at offset 104
        data[MARKET_FLOOR_PRICE_OFFSET + 2] = 0; // scale = 0
        data[MARKET_FLOOR_PRICE_OFFSET + 4] = 1; // mantissa = 1
        let result = read_floor_price(&data).unwrap();
        assert_eq!(result, 1_000_000_000);
    }

    #[test]
    fn test_read_floor_price_wrong_discriminator() {
        let mut data = vec![0u8; MARKET_FLOOR_PRICE_OFFSET + RUST_DECIMAL_SIZE + 1];
        data[..8].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // wrong discriminator
        assert!(read_floor_price(&data).is_err());
    }
}
