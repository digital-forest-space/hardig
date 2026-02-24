use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod mayflower;
pub mod state;

use instructions::*;

declare_id!("4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p");

#[program]
pub mod hardig {
    use super::*;

    /// Initialize the global protocol config. Called once by the deployer.
    pub fn initialize_protocol(ctx: Context<InitializeProtocol>) -> Result<()> {
        instructions::initialize_protocol::handler(ctx)
    }

    /// Migrate ProtocolConfig from v0 (41 bytes) to v1 (73 bytes, adds collection field).
    pub fn migrate_config(ctx: Context<MigrateConfig>) -> Result<()> {
        instructions::migrate_config::handler(ctx)
    }

    /// Create the MPL-Core collection for all Härdig key NFTs (protocol admin only, once).
    pub fn create_collection(ctx: Context<CreateCollection>, uri: String) -> Result<()> {
        instructions::create_collection::handler(ctx, uri)
    }

    /// Create a new position with an admin key NFT.
    pub fn create_position(
        ctx: Context<CreatePosition>,
        max_reinvest_spread_bps: u16,
        name: Option<String>,
        market_name: String,
        artwork_id: Option<Pubkey>,
    ) -> Result<()> {
        instructions::create_position::handler(ctx, max_reinvest_spread_bps, name, market_name, artwork_id)
    }

    /// Authorize a new key NFT for a position (admin only).
    pub fn authorize_key(
        ctx: Context<AuthorizeKey>,
        permissions: u8,
        sell_bucket_capacity: u64,
        sell_refill_period_slots: u64,
        borrow_bucket_capacity: u64,
        borrow_refill_period_slots: u64,
        name: Option<String>,
    ) -> Result<()> {
        instructions::authorize_key::handler(
            ctx,
            permissions,
            sell_bucket_capacity,
            sell_refill_period_slots,
            borrow_bucket_capacity,
            borrow_refill_period_slots,
            name,
        )
    }

    /// Revoke a key by burning its MPL-Core asset (admin only).
    pub fn revoke_key(ctx: Context<RevokeKey>) -> Result<()> {
        instructions::revoke_key::handler(ctx)
    }

    /// Buy navSOL by depositing SOL (admin, operator, or depositor).
    /// `min_out`: minimum navSOL shares to receive (slippage protection, 0 = no check).
    pub fn buy(ctx: Context<Buy>, amount: u64, min_out: u64) -> Result<()> {
        instructions::buy::handler(ctx, amount, min_out)
    }

    /// Withdraw SOL/navSOL from the position (sell or limited-sell key).
    /// `min_out`: minimum wSOL to receive (slippage protection, 0 = no check).
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64, min_out: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, amount, min_out)
    }

    /// Borrow SOL against the navSOL floor (borrow or limited-borrow key).
    pub fn borrow(ctx: Context<Borrow>, amount: u64) -> Result<()> {
        instructions::borrow::handler(ctx, amount)
    }

    /// Repay borrowed SOL (admin, operator, or depositor).
    pub fn repay(ctx: Context<Repay>, amount: u64) -> Result<()> {
        instructions::repay::handler(ctx, amount)
    }

    /// Reinvest new borrow capacity into more navSOL (admin, operator, or keeper).
    /// `min_out`: minimum navSOL shares to receive from the buy (slippage protection, 0 = no check).
    pub fn reinvest(ctx: Context<Reinvest>, min_out: u64) -> Result<()> {
        instructions::reinvest::handler(ctx, min_out)
    }

    /// Nominate a new protocol admin (current admin only). The nominated key
    /// must call `accept_admin` to complete the transfer.
    pub fn transfer_admin(ctx: Context<TransferAdmin>, new_admin: Pubkey) -> Result<()> {
        instructions::transfer_admin::handler(ctx, new_admin)
    }

    /// Accept a pending admin transfer (must be called by the nominated key).
    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        instructions::accept_admin::handler(ctx)
    }

    /// No-op liveness proof. Updates last_admin_activity to prove admin is active.
    pub fn heartbeat(ctx: Context<Heartbeat>) -> Result<()> {
        instructions::heartbeat::handler(ctx)
    }

    /// Configure a recovery key for dead-man's switch protection (admin only).
    pub fn configure_recovery(
        ctx: Context<ConfigureRecovery>,
        lockout_secs: i64,
        lock_config: bool,
        name: Option<String>,
    ) -> Result<()> {
        instructions::configure_recovery::handler(ctx, lockout_secs, lock_config, name)
    }

    /// Execute recovery after lockout period has expired (recovery key holder only).
    pub fn execute_recovery(ctx: Context<ExecuteRecovery>) -> Result<()> {
        instructions::execute_recovery::handler(ctx)
    }

    /// Create a PromoConfig PDA for a position (admin only).
    pub fn create_promo(
        ctx: Context<CreatePromo>,
        name_suffix: String,
        permissions: u8,
        borrow_capacity: u64,
        borrow_refill_period: u64,
        sell_capacity: u64,
        sell_refill_period: u64,
        min_deposit_lamports: u64,
        max_claims: u32,
        image_uri: String,
        market_name: String,
    ) -> Result<()> {
        instructions::create_promo::handler(ctx, name_suffix, permissions, borrow_capacity, borrow_refill_period, sell_capacity, sell_refill_period, min_deposit_lamports, max_claims, image_uri, market_name)
    }

    pub fn update_promo(
        ctx: Context<UpdatePromo>,
        active: Option<bool>,
        max_claims: Option<u32>,
    ) -> Result<()> {
        instructions::update_promo::handler(ctx, active, max_claims)
    }

    /// Claim a promo key NFT from a PromoConfig (permissionless — anyone can call).
    /// `amount`: lamports to deposit via Mayflower buy CPI (must be >= promo.min_deposit_lamports).
    /// `min_out`: minimum navSOL shares to receive (slippage protection, 0 = no check).
    pub fn claim_promo_key(ctx: Context<ClaimPromoKey>, amount: u64, min_out: u64) -> Result<()> {
        instructions::claim_promo_key::handler(ctx, amount, min_out)
    }

    /// Create a MarketConfig PDA for a Mayflower market (protocol admin only).
    pub fn create_market_config(
        ctx: Context<CreateMarketConfig>,
        nav_mint: Pubkey,
        base_mint: Pubkey,
        market_group: Pubkey,
        market_meta: Pubkey,
        mayflower_market: Pubkey,
        market_base_vault: Pubkey,
        market_nav_vault: Pubkey,
        fee_vault: Pubkey,
    ) -> Result<()> {
        instructions::create_market_config::handler(
            ctx,
            nav_mint,
            base_mint,
            market_group,
            market_meta,
            mayflower_market,
            market_base_vault,
            market_nav_vault,
            fee_vault,
        )
    }
}
