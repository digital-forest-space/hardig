use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::{ProtocolConfig, TrustedProvider};

#[derive(Accounts)]
pub struct RemoveTrustedProvider<'info> {
    #[account(
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
        seeds = [TrustedProvider::SEED, trusted_provider.program_id.as_ref()],
        bump = trusted_provider.bump,
    )]
    pub trusted_provider: Account<'info, TrustedProvider>,
}

pub fn handler(ctx: Context<RemoveTrustedProvider>) -> Result<()> {
    ctx.accounts.trusted_provider.active = false;
    Ok(())
}
