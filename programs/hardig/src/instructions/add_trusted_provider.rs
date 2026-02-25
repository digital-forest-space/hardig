use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::{ProtocolConfig, TrustedProvider};

#[derive(Accounts)]
#[instruction(program_id: Pubkey)]
pub struct AddTrustedProvider<'info> {
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
        init,
        payer = admin,
        space = TrustedProvider::SIZE,
        seeds = [TrustedProvider::SEED, program_id.as_ref()],
        bump,
    )]
    pub trusted_provider: Account<'info, TrustedProvider>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<AddTrustedProvider>, program_id: Pubkey) -> Result<()> {
    let tp = &mut ctx.accounts.trusted_provider;
    tp.program_id = program_id;
    tp.added_by = ctx.accounts.admin.key();
    tp.active = true;
    tp.bump = ctx.bumps.trusted_provider;
    Ok(())
}
