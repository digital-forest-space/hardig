use anchor_lang::prelude::*;

#[error_code]
pub enum HardigError {
    // Permission errors
    #[msg("Signer is not authorized for this action")]
    Unauthorized,
    #[msg("This action requires admin key")]
    AdminOnly,
    #[msg("Key does not have the required permission for this instruction")]
    InsufficientPermission,
    #[msg("Signer does not hold the key NFT")]
    KeyNotHeld,
    #[msg("Key asset has invalid or missing attributes")]
    InvalidKey,
    #[msg("Key asset does not belong to this position")]
    WrongPosition,

    // Key management errors
    #[msg("Cannot grant PERM_MANAGE_KEYS to delegated keys")]
    CannotCreateSecondAdmin,
    #[msg("Cannot revoke the admin key")]
    CannotRevokeAdminKey,
    #[msg("Key is already authorized for this position")]
    KeyAlreadyAuthorized,
    #[msg("Invalid permissions value (zero or reserved bits set)")]
    InvalidKeyRole,

    // Fund errors
    #[msg("Insufficient funds for this operation")]
    InsufficientFunds,
    #[msg("Borrow amount exceeds available capacity")]
    BorrowCapacityExceeded,
    #[msg("Market/floor spread exceeds max_reinvest_spread_bps")]
    ReinvestSpreadTooHigh,

    // Nirvana CPI errors
    #[msg("Invalid Nirvana account address or derivation")]
    InvalidMayflowerAccount,

    // Slippage errors
    #[msg("Output amount is less than minimum specified (slippage exceeded)")]
    SlippageExceeded,

    // ATA validation errors
    #[msg("Token account is not the correct ATA for the program PDA")]
    InvalidAta,

    // Rate-limit errors
    #[msg("Rate limit exceeded")]
    RateLimitExceeded,

    // Collection errors
    #[msg("Collection has not been created yet")]
    CollectionNotCreated,
    #[msg("Collection has already been created")]
    CollectionAlreadyCreated,

    // Migration errors
    #[msg("Config account already has the expected size")]
    AlreadyMigrated,

    // State errors
    #[msg("Invalid NFT mint")]
    InvalidNftMint,
    #[msg("Invalid position PDA")]
    InvalidPositionPda,
    #[msg("Position already exists for this admin key")]
    PositionAlreadyExists,

    // Name errors
    #[msg("Custom name suffix exceeds 32 characters")]
    NameTooLong,
    #[msg("Image URI exceeds maximum allowed length")]
    ImageUriTooLong,

    // Recovery errors
    #[msg("No recovery key is configured for this position")]
    RecoveryNotConfigured,
    #[msg("Admin has been active within the lockout period")]
    RecoveryLockoutNotExpired,
    #[msg("Recovery configuration is locked and cannot be changed")]
    RecoveryConfigLocked,
    #[msg("Lockout period must be greater than zero")]
    InvalidLockout,
    #[msg("Must provide old_recovery_asset to replace an existing recovery key")]
    OldRecoveryAssetRequired,

    // Promo errors
    #[msg("Max claims cannot be less than current claims count")]
    MaxClaimsBelowCurrent,
    #[msg("Promo is not active")]
    PromoInactive,
    #[msg("Promo has reached maximum claims")]
    PromoMaxClaimsReached,
}
