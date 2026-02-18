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

    /// Create a new position with an admin key NFT.
    pub fn create_position(
        ctx: Context<CreatePosition>,
        max_reinvest_spread_bps: u16,
    ) -> Result<()> {
        instructions::create_position::handler(ctx, max_reinvest_spread_bps)
    }

    /// Authorize a new key NFT for a position (admin only).
    pub fn authorize_key(ctx: Context<AuthorizeKey>, role: u8) -> Result<()> {
        instructions::authorize_key::handler(ctx, role)
    }

    /// Revoke a key by closing its KeyAuthorization (admin only).
    pub fn revoke_key(ctx: Context<RevokeKey>) -> Result<()> {
        instructions::revoke_key::handler(ctx)
    }

    /// Buy navSOL by depositing SOL (admin, operator, or depositor).
    /// `min_out`: minimum navSOL shares to receive (slippage protection, 0 = no check).
    pub fn buy(ctx: Context<Buy>, amount: u64, min_out: u64) -> Result<()> {
        instructions::buy::handler(ctx, amount, min_out)
    }

    /// Withdraw SOL/navSOL from the position (admin only).
    /// `min_out`: minimum wSOL to receive (slippage protection, 0 = no check).
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64, min_out: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, amount, min_out)
    }

    /// Borrow SOL against the navSOL floor (admin only).
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

    /// Initialize a Mayflower PersonalPosition owned by this program's PDA.
    /// Called once after create_position (admin only).
    pub fn init_mayflower_position(ctx: Context<InitMayflowerPosition>) -> Result<()> {
        instructions::init_mayflower_position::handler(ctx)
    }

    /// Transfer protocol admin rights to a new pubkey (current admin only).
    pub fn transfer_admin(ctx: Context<TransferAdmin>, new_admin: Pubkey) -> Result<()> {
        instructions::transfer_admin::handler(ctx, new_admin)
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
