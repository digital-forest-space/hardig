use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::ProtocolConfig;

#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub new_admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
        constraint = config.pending_admin == new_admin.key() @ HardigError::Unauthorized,
        constraint = config.pending_admin != Pubkey::default() @ HardigError::Unauthorized,
    )]
    pub config: Account<'info, ProtocolConfig>,
}

pub fn handler(ctx: Context<AcceptAdmin>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.admin = config.pending_admin;
    config.pending_admin = Pubkey::default();
    Ok(())
}
