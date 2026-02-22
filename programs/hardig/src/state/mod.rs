use anchor_lang::prelude::*;

pub mod promo;
pub use promo::*;

/// Global protocol configuration. Singleton PDA (seeds = [b"config"]).
#[account]
pub struct ProtocolConfig {
    /// The admin who initialized the protocol.
    pub admin: Pubkey,
    /// The MPL-Core collection for all Härdig key NFTs (Pubkey::default() = not yet created).
    pub collection: Pubkey,
    /// Pending admin for two-step transfer (Pubkey::default() = no pending transfer).
    pub pending_admin: Pubkey,
    /// Bump seed for the config PDA.
    pub bump: u8,
}

impl ProtocolConfig {
    pub const SEED: &'static [u8] = b"config";
    // discriminator + admin + collection + pending_admin + bump
    pub const SIZE: usize = 8 + 32 + 32 + 32 + 1; // 105
}

/// A navSOL position controlled by an NFT keyring.
/// PDA seeds = [b"position", authority_seed].
#[account]
pub struct PositionNFT {
    /// Permanent PDA seed (first admin asset pubkey). Never changes after creation.
    pub authority_seed: Pubkey,
    /// The Mayflower PersonalPosition PDA owned by this program.
    pub position_pda: Pubkey,
    /// The MarketConfig PDA this position is bound to (set during create_position).
    pub market_config: Pubkey,
    /// navSOL deposited (local tracking, Mayflower is source of truth).
    pub deposited_nav: u64,
    /// Total SOL borrowed (user + reinvest). Mayflower is source of truth.
    pub user_debt: u64,
    /// Max market/floor spread ratio (in bps) allowed for reinvest.
    pub max_reinvest_spread_bps: u16,
    /// Last time the admin signed an instruction (unix timestamp).
    /// Used for future time-locked recovery mechanism.
    pub last_admin_activity: i64,
    /// Bump seed for the position PDA.
    pub bump: u8,
    /// Bump seed for the per-position authority PDA (seeds = [b"authority", authority_seed]).
    pub authority_bump: u8,
    /// The current admin key NFT (MPL-Core asset). Updated on recovery.
    pub current_admin_asset: Pubkey,
    /// The recovery key NFT (MPL-Core asset). Pubkey::default() = no recovery configured.
    pub recovery_asset: Pubkey,
    /// Inactivity threshold in seconds before recovery can execute.
    pub recovery_lockout_secs: i64,
    /// If true, recovery config cannot be changed.
    pub recovery_config_locked: bool,
}

impl PositionNFT {
    pub const SEED: &'static [u8] = b"position";
    // discriminator(8) + authority_seed(32) + position_pda(32) + market_config(32)
    // + deposited_nav(8) + user_debt(8) + max_reinvest_spread_bps(2)
    // + last_admin_activity(8) + bump(1) + authority_bump(1)
    // + current_admin_asset(32) + recovery_asset(32) + recovery_lockout_secs(8)
    // + recovery_config_locked(1)
    pub const SIZE: usize = 8 + 32 + 32 + 32 + 8 + 8 + 2 + 8 + 1 + 1 + 32 + 32 + 8 + 1;
}

/// On-chain configuration for a Mayflower market.
/// PDA seeds = [b"market_config", nav_mint].
#[account]
pub struct MarketConfig {
    /// The nav token mint (e.g. navSOL) — also the PDA seed.
    pub nav_mint: Pubkey,
    /// The base mint (e.g. wSOL) — included for markets with non-SOL base.
    pub base_mint: Pubkey,
    /// Mayflower market group account.
    pub market_group: Pubkey,
    /// Mayflower market metadata account.
    pub market_meta: Pubkey,
    /// Mayflower market account.
    pub mayflower_market: Pubkey,
    /// Mayflower market base vault.
    pub market_base_vault: Pubkey,
    /// Mayflower market nav vault.
    pub market_nav_vault: Pubkey,
    /// Mayflower fee vault.
    pub fee_vault: Pubkey,
    /// Bump seed for the MarketConfig PDA.
    pub bump: u8,
}

impl MarketConfig {
    pub const SEED: &'static [u8] = b"market_config";
    // discriminator(8) + 8 pubkeys(32*8) + bump(1)
    pub const SIZE: usize = 8 + 32 * 8 + 1;
}

// ---------------------------------------------------------------------------
// Permission bitmask constants
// ---------------------------------------------------------------------------

/// Each bit grants a specific permission.
pub const PERM_BUY: u8 = 0x01;
pub const PERM_SELL: u8 = 0x02;
pub const PERM_BORROW: u8 = 0x04;
pub const PERM_REPAY: u8 = 0x08;
pub const PERM_REINVEST: u8 = 0x10;
pub const PERM_MANAGE_KEYS: u8 = 0x20;

/// Rate-limited sell permission (bit 6). Enforced by token-bucket in KeyState.
pub const PERM_LIMITED_SELL: u8 = 0x40;
/// Rate-limited borrow permission (bit 7). Enforced by token-bucket in KeyState.
pub const PERM_LIMITED_BORROW: u8 = 0x80;
/// Mask for rate-limited permission bits.
pub const PERM_LIMITED_MASK: u8 = 0xC0;

/// All defined permissions (bits 0-7).
pub const PERM_ALL: u8 = 0xFF;

/// Who is creating this delegated key — determines which permissions are allowed.
#[derive(Clone, Copy)]
pub enum KeyCreatorOrigin {
    /// Admin directly authorizes a key to a known recipient.
    Admin,
    /// Permissionless promo claim — anyone can mint.
    Promo,
}

impl KeyCreatorOrigin {
    /// Permission bits allowed for this origin.
    pub fn allowed_permissions(&self) -> u8 {
        match self {
            Self::Admin => PERM_BUY | PERM_SELL | PERM_BORROW | PERM_REPAY
                         | PERM_REINVEST | PERM_LIMITED_SELL | PERM_LIMITED_BORROW,
            Self::Promo => PERM_BUY | PERM_LIMITED_BORROW,
        }
    }
}

// Backwards-compatible presets
pub const PRESET_ADMIN: u8 = 0x3F; // all 6 bits
pub const PRESET_OPERATOR: u8 = 0x19; // buy + repay + reinvest
pub const PRESET_DEPOSITOR: u8 = 0x09; // buy + repay
pub const PRESET_KEEPER: u8 = 0x10; // reinvest only

/// Token-bucket rate limiter. Embedded in KeyState.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct RateBucket {
    /// Maximum tokens (shares for sell, lamports for borrow).
    pub capacity: u64,
    /// Number of slots for a full refill from 0 to capacity.
    pub refill_period: u64,
    /// Current tokens available.
    pub level: u64,
    /// Slot of last update.
    pub last_update: u64,
}

impl RateBucket {
    /// Compute the currently available tokens without mutating state.
    ///
    /// Replicates the refill logic from `consume_rate_limit` in read-only form:
    ///
    /// ```text
    /// elapsed = current_slot - last_update
    /// refill  = min(capacity, capacity * elapsed / refill_period)   // u128 intermediate
    /// available = min(capacity, level + refill)
    /// ```
    ///
    /// Returns 0 for an unconfigured bucket (capacity == 0).
    pub fn available_now(&self, current_slot: u64) -> u64 {
        if self.capacity == 0 {
            return 0;
        }

        let elapsed = current_slot.saturating_sub(self.last_update);

        let refill = if elapsed >= self.refill_period {
            self.capacity
        } else {
            ((self.capacity as u128) * (elapsed as u128) / (self.refill_period as u128)) as u64
        };

        self.level.saturating_add(refill).min(self.capacity)
    }
}

/// Mutable state for a key NFT. Created for all delegated keys (via authorize_key).
/// PDA seeds = [b"key_state", asset].
#[account]
pub struct KeyState {
    /// The MPL-Core asset this state belongs to.
    pub asset: Pubkey,
    /// Bump seed for this PDA.
    pub bump: u8,
    /// Rate-limit bucket for PERM_LIMITED_SELL.
    pub sell_bucket: RateBucket,
    /// Rate-limit bucket for PERM_LIMITED_BORROW.
    pub borrow_bucket: RateBucket,
}

impl KeyState {
    pub const SEED: &'static [u8] = b"key_state";
    // discriminator(8) + asset(32) + bump(1) + sell_bucket(32) + borrow_bucket(32)
    pub const SIZE: usize = 8 + 32 + 1 + 32 + 32;
}
