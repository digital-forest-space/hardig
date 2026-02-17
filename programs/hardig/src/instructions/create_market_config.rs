use anchor_lang::prelude::*;

use crate::state::{MarketConfig, ProtocolConfig};

#[derive(Accounts)]
#[instruction(nav_mint: Pubkey)]
pub struct CreateMarketConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The global protocol config — used to verify the signer is the protocol admin.
    #[account(
        seeds = [ProtocolConfig::SEED],
        bump = config.bump,
        constraint = config.admin == admin.key(),
    )]
    pub config: Account<'info, ProtocolConfig>,

    #[account(
        init,
        payer = admin,
        space = MarketConfig::SIZE,
        seeds = [MarketConfig::SEED, nav_mint.as_ref()],
        bump,
    )]
    pub market_config: Account<'info, MarketConfig>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
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
    let mc = &mut ctx.accounts.market_config;
    mc.nav_mint = nav_mint;
    mc.base_mint = base_mint;
    mc.market_group = market_group;
    mc.market_meta = market_meta;
    mc.mayflower_market = mayflower_market;
    mc.market_base_vault = market_base_vault;
    mc.market_nav_vault = market_nav_vault;
    mc.fee_vault = fee_vault;
    mc.bump = ctx.bumps.market_config;
    Ok(())
}
