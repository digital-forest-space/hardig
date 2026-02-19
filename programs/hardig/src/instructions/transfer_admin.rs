use anchor_lang::prelude::*;

use crate::state::ProtocolConfig;

#[derive(Accounts)]
pub struct TransferAdmin<'info> {
    #[account(
        constraint = admin.key() == config.admin,
    )]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
    )]
    pub config: Account<'info, ProtocolConfig>,
}

pub fn handler(ctx: Context<TransferAdmin>, new_admin: Pubkey) -> Result<()> {
    ctx.accounts.config.pending_admin = new_admin;
    Ok(())
}
