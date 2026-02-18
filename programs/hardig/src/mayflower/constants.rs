use anchor_lang::prelude::*;

/// Mayflower program ID.
pub const MAYFLOWER_PROGRAM_ID: Pubkey =
    pubkey!("AVMmmRzwc2kETQNhPiFVnyu62HrgsQXTD6D7SnSfEz7v");

/// Mayflower tenant account.
pub const MAYFLOWER_TENANT: Pubkey =
    pubkey!("81JEJdJSZbaXixpD8WQSBWBfkDa6m6KpXpSErzYUHq6z");

// Default navSOL market accounts — used for tests and initial MarketConfig seeding.
pub const DEFAULT_MARKET_GROUP: Pubkey =
    pubkey!("Lmdgb4NE4T3ubmQZQZQZ7t4UP6A98NdVbmZPcoEdkdC");

pub const DEFAULT_MARKET_META: Pubkey =
    pubkey!("DotD4dZAyr4Kb6AD3RHid8VgmsHUzWF6LRd4WvAMezRj");

pub const DEFAULT_MAYFLOWER_MARKET: Pubkey =
    pubkey!("A5M1nWfi6ATSamEJ1ASr2FC87BMwijthTbNRYG7BhYSc");

pub const DEFAULT_MARKET_BASE_VAULT: Pubkey =
    pubkey!("43vPhZeow3pgYa6zrPXASVQhdXTMfowyfNK87BYizhnL");

pub const DEFAULT_MARKET_NAV_VAULT: Pubkey =
    pubkey!("BCYzijbWwmqRnsTWjGhHbneST2emQY36WcRAkbkhsQMt");

pub const DEFAULT_FEE_VAULT: Pubkey =
    pubkey!("B8jccpiKZjapgfw1ay6EH3pPnxqTmimsm2KsTZ9LSmjf");

// Default mints
pub const DEFAULT_NAV_SOL_MINT: Pubkey =
    pubkey!("navSnrYJkCxMiyhM3F7K889X1u8JFLVHHLxiyo6Jjqo");

pub const DEFAULT_WSOL_MINT: Pubkey =
    pubkey!("So11111111111111111111111111111111111111112");

// Instruction discriminators (derived from Mayflower IDL).
// NOTE: init_personal_position does not follow standard Anchor sighash pattern.
pub const IX_INIT_PERSONAL_POSITION: [u8; 8] = [146, 163, 167, 48, 30, 216, 179, 88];
pub const IX_BUY: [u8; 8] = [30, 205, 124, 67, 20, 142, 236, 136];
pub const IX_BORROW: [u8; 8] = [228, 253, 131, 202, 207, 116, 89, 18];
pub const IX_REPAY: [u8; 8] = [234, 103, 67, 82, 208, 234, 219, 166];
pub const IX_SELL: [u8; 8] = [223, 239, 212, 254, 255, 120, 53, 1];

// PDA seeds
pub const PERSONAL_POSITION_SEED: &[u8] = b"personal_position";
pub const PERSONAL_POSITION_ESCROW_SEED: &[u8] = b"personal_position_escrow";
pub const LOG_SEED: &[u8] = b"log";
pub const LIQ_VAULT_MAIN_SEED: &[u8] = b"liq_vault_main";

// PersonalPosition account layout offsets
pub const PP_DISCRIMINATOR: [u8; 8] = [40, 172, 123, 89, 170, 15, 56, 141];
pub const PP_SIZE: usize = 121;
pub const PP_DEPOSITED_SHARES_OFFSET: usize = 104; // u64 LE
pub const PP_DEBT_OFFSET: usize = 112; // u64 LE

// MayflowerMarket account layout
pub const MARKET_FLOOR_PRICE_OFFSET: usize = 104; // Rust Decimal, 16 bytes
pub const RUST_DECIMAL_SIZE: usize = 16;
