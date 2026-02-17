use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

#[derive(Accounts)]
pub struct CreatePosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The mint for the admin key NFT. Created fresh by the client, passed in as a signer.
    #[account(
        init,
        payer = admin,
        mint::decimals = 0,
        mint::authority = program_pda,
    )]
    pub admin_nft_mint: Account<'info, Mint>,

    /// The admin's ATA for the admin key NFT.
    #[account(
        init,
        payer = admin,
        associated_token::mint = admin_nft_mint,
        associated_token::authority = admin,
    )]
    pub admin_nft_ata: Account<'info, TokenAccount>,

    /// The position account.
    #[account(
        init,
        payer = admin,
        space = PositionNFT::SIZE,
        seeds = [PositionNFT::SEED, admin_nft_mint.key().as_ref()],
        bump,
    )]
    pub position: Account<'info, PositionNFT>,

    /// The KeyAuthorization for the admin key.
    #[account(
        init,
        payer = admin,
        space = KeyAuthorization::SIZE,
        seeds = [
            KeyAuthorization::SEED,
            position.key().as_ref(),
            admin_nft_mint.key().as_ref(),
        ],
        bump,
    )]
    pub admin_key_auth: Account<'info, KeyAuthorization>,

    /// Program PDA used as mint authority.
    /// CHECK: PDA derived from program, not read.
    #[account(
        seeds = [b"authority"],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CreatePosition>, max_reinvest_spread_bps: u16) -> Result<()> {
    // Mint 1 admin key NFT to the admin's ATA
    let bump = ctx.bumps.program_pda;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.admin_nft_mint.to_account_info(),
                to: ctx.accounts.admin_nft_ata.to_account_info(),
                authority: ctx.accounts.program_pda.to_account_info(),
            },
            signer_seeds,
        ),
        1,
    )?;

    // Initialize the position
    let position = &mut ctx.accounts.position;
    position.admin_nft_mint = ctx.accounts.admin_nft_mint.key();
    position.position_pda = Pubkey::default(); // Set during Mayflower CPI integration
    position.deposited_nav = 0;
    position.user_debt = 0;
    position.protocol_debt = 0;
    position.max_reinvest_spread_bps = max_reinvest_spread_bps;
    position.last_admin_activity = Clock::get()?.unix_timestamp;
    position.bump = ctx.bumps.position;

    // Initialize the admin KeyAuthorization
    let key_auth = &mut ctx.accounts.admin_key_auth;
    key_auth.position = ctx.accounts.position.key();
    key_auth.key_nft_mint = ctx.accounts.admin_nft_mint.key();
    key_auth.role = KeyRole::Admin;
    key_auth.bump = ctx.bumps.admin_key_auth;

    Ok(())
}
