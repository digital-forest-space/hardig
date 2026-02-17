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
    pub fn buy(ctx: Context<Buy>, amount: u64) -> Result<()> {
        instructions::buy::handler(ctx, amount)
    }

    /// Withdraw SOL/navSOL from the position (admin only).
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, amount)
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
    pub fn reinvest(ctx: Context<Reinvest>) -> Result<()> {
        instructions::reinvest::handler(ctx)
    }

    /// Initialize a Mayflower PersonalPosition owned by this program's PDA.
    /// Called once after create_position (admin only).
    pub fn init_mayflower_position(ctx: Context<InitMayflowerPosition>) -> Result<()> {
        instructions::init_mayflower_position::handler(ctx)
    }
}
