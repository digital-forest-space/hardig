use anchor_lang::prelude::*;

/// Global protocol configuration. Singleton PDA (seeds = [b"config"]).
#[account]
pub struct ProtocolConfig {
    /// The admin who initialized the protocol.
    pub admin: Pubkey,
    /// Bump seed for the config PDA.
    pub bump: u8,
}

impl ProtocolConfig {
    pub const SEED: &'static [u8] = b"config";
    pub const SIZE: usize = 8 + 32 + 1; // discriminator + admin + bump
}

/// A navSOL position controlled by an NFT keyring.
/// PDA seeds = [b"position", admin_nft_mint].
#[account]
pub struct PositionNFT {
    /// The admin key NFT mint (master key, only one per position).
    pub admin_nft_mint: Pubkey,
    /// The Mayflower PersonalPosition PDA owned by this program.
    pub position_pda: Pubkey,
    /// navSOL deposited (local tracking, Mayflower is source of truth).
    pub deposited_nav: u64,
    /// SOL borrowed by the user (local tracking).
    pub user_debt: u64,
    /// SOL borrowed by the protocol for reinvestment.
    pub protocol_debt: u64,
    /// Max market/floor spread ratio (in bps) allowed for reinvest.
    pub max_reinvest_spread_bps: u16,
    /// Last time the admin signed an instruction (unix timestamp).
    /// Used for future time-locked recovery mechanism.
    pub last_admin_activity: i64,
    /// Bump seed for the position PDA.
    pub bump: u8,
    /// Bump seed for the per-position authority PDA (seeds = [b"authority", admin_nft_mint]).
    pub authority_bump: u8,
}

impl PositionNFT {
    pub const SEED: &'static [u8] = b"position";
    // discriminator(8) + admin_nft_mint(32) + position_pda(32) + deposited_nav(8)
    // + user_debt(8) + protocol_debt(8) + max_reinvest_spread_bps(2)
    // + last_admin_activity(8) + bump(1) + authority_bump(1)
    pub const SIZE: usize = 8 + 32 + 32 + 8 + 8 + 8 + 2 + 8 + 1 + 1;
}

/// Role assigned to a key NFT.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyRole {
    /// Full control: withdraw, borrow, authorize/revoke keys, update settings.
    Admin = 0,
    /// Deposit + reinvest + repay.
    Operator = 1,
    /// Deposit + repay only.
    Depositor = 2,
    /// Reinvest/compound only.
    Keeper = 3,
}

impl KeyRole {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(KeyRole::Admin),
            1 => Some(KeyRole::Operator),
            2 => Some(KeyRole::Depositor),
            3 => Some(KeyRole::Keeper),
            _ => None,
        }
    }
}

/// Links a key NFT to a position with a specific role.
/// PDA seeds = [b"key_auth", position, key_nft_mint].
#[account]
pub struct KeyAuthorization {
    /// The position this key unlocks.
    pub position: Pubkey,
    /// The NFT mint that serves as the key.
    pub key_nft_mint: Pubkey,
    /// The role/permissions this key grants.
    pub role: KeyRole,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl KeyAuthorization {
    pub const SEED: &'static [u8] = b"key_auth";
    // discriminator(8) + position(32) + key_nft_mint(32) + role(1) + bump(1)
    pub const SIZE: usize = 8 + 32 + 32 + 1 + 1;
}
