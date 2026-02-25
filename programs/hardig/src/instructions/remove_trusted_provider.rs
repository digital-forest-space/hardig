use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::{ProtocolConfig, TrustedProvider};

#[derive(Accounts)]
pub struct RemoveTrustedProvider<'info> {
    #[account(
        mut,
        constraint = admin.key() == config.admin @ HardigError::Unauthorized,
    )]
    pub admin: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
    )]
    pub config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        close = admin,
        seeds = [TrustedProvider::SEED, trusted_provider.program_id.as_ref()],
        bump = trusted_provider.bump,
    )]
    pub trusted_provider: Account<'info, TrustedProvider>,
}

pub fn handler(_ctx: Context<RemoveTrustedProvider>) -> Result<()> {
    // Account is closed by Anchor's `close = admin` constraint.
    // Rent is returned to the admin. The provider can be re-added later
    // via `add_trusted_provider`.
    Ok(())
}
