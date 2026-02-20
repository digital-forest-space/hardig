use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// Promo feature – isolated state for future extraction
// ---------------------------------------------------------------------------

/// Per-position promo configuration.
/// PDA seeds = [b"promo", authority_seed, name_suffix].
#[account]
pub struct PromoConfig {
    /// Which position this promo is for (the position's authority_seed).
    pub authority_seed: Pubkey,
    /// Key permissions bitmask granted to claimed promo keys.
    pub permissions: u8,
    /// LimitedBorrow bucket capacity (lamports).
    pub borrow_capacity: u64,
    /// LimitedBorrow refill period (slots).
    pub borrow_refill_period: u64,
    /// LimitedSell bucket capacity (0 if N/A).
    pub sell_capacity: u64,
    /// LimitedSell refill period (0 if N/A).
    pub sell_refill_period: u64,
    /// Suggested deposit amount in lamports (frontend reads).
    pub min_deposit_lamports: u64,
    /// Max total keys that can be claimed (0 = unlimited).
    pub max_claims: u32,
    /// Current number of claimed keys.
    pub claims_count: u32,
    /// Admin can pause/resume claiming.
    pub active: bool,
    /// NFT name suffix (e.g. "Promo Borrow").
    pub name_suffix: String,
    /// Custom NFT image URL (max 128 bytes).
    pub image_uri: String,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl PromoConfig {
    pub const SEED: &'static [u8] = b"promo";
    pub const MAX_IMAGE_URI_LEN: usize = 128;
}

/// One-time claim receipt preventing double-claims.
/// PDA seeds = [b"claim", promo_pda, claimer_pubkey].
#[account]
pub struct ClaimReceipt {
    /// The wallet that claimed.
    pub claimer: Pubkey,
    /// The PromoConfig PDA this receipt belongs to.
    pub promo: Pubkey,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl ClaimReceipt {
    pub const SEED: &'static [u8] = b"claim";
}
