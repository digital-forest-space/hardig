use anchor_lang::prelude::*;

#[error_code]
pub enum HardigError {
    // Permission errors
    #[msg("Signer is not authorized for this action")]
    Unauthorized,
    #[msg("This action requires admin key")]
    AdminOnly,
    #[msg("Key role does not have permission for this instruction")]
    InsufficientPermission,
    #[msg("Signer does not hold the key NFT")]
    KeyNotHeld,
    #[msg("Key NFT mint does not match KeyAuthorization")]
    InvalidKey,
    #[msg("KeyAuthorization is for a different position")]
    WrongPosition,

    // Key management errors
    #[msg("Cannot create a second admin key")]
    CannotCreateSecondAdmin,
    #[msg("Cannot revoke the admin key")]
    CannotRevokeAdminKey,
    #[msg("Key is already authorized for this position")]
    KeyAlreadyAuthorized,
    #[msg("Invalid key role value")]
    InvalidKeyRole,

    // Fund errors
    #[msg("Insufficient funds for this operation")]
    InsufficientFunds,
    #[msg("Borrow amount exceeds available capacity")]
    BorrowCapacityExceeded,
    #[msg("Market/floor spread exceeds max_reinvest_spread_bps")]
    ReinvestSpreadTooHigh,

    // State errors
    #[msg("Invalid NFT mint")]
    InvalidNftMint,
    #[msg("Invalid position PDA")]
    InvalidPositionPda,
    #[msg("Position already exists for this admin key")]
    PositionAlreadyExists,
}
