use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

use crate::errors::HardigError;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct AuthorizeKey<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT token account (proves admin holds the key).
    pub admin_nft_ata: Account<'info, TokenAccount>,

    /// The admin's KeyAuthorization.
    #[account(
        constraint = admin_key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub admin_key_auth: Account<'info, KeyAuthorization>,

    /// The position to authorize a key for.
    pub position: Account<'info, PositionNFT>,

    /// The mint for the new key NFT. Created fresh by the client, passed in as a signer.
    #[account(
        init,
        payer = admin,
        mint::decimals = 0,
        mint::authority = program_pda,
    )]
    pub new_key_mint: Account<'info, Mint>,

    /// The target wallet's ATA for the new key NFT.
    #[account(
        init,
        payer = admin,
        associated_token::mint = new_key_mint,
        associated_token::authority = target_wallet,
    )]
    pub target_nft_ata: Account<'info, TokenAccount>,

    /// The wallet that will receive the new key NFT.
    /// CHECK: Any wallet can receive a key.
    pub target_wallet: UncheckedAccount<'info>,

    /// The KeyAuthorization for the new key.
    #[account(
        init,
        payer = admin,
        space = KeyAuthorization::SIZE,
        seeds = [
            KeyAuthorization::SEED,
            position.key().as_ref(),
            new_key_mint.key().as_ref(),
        ],
        bump,
    )]
    pub new_key_auth: Account<'info, KeyAuthorization>,

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

pub fn handler(ctx: Context<AuthorizeKey>, role: u8) -> Result<()> {
    // Validate the admin holds their key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_nft_ata,
        &ctx.accounts.admin_key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin],
    )?;

    // Parse and validate the role
    let key_role = KeyRole::from_u8(role).ok_or(HardigError::InvalidKeyRole)?;

    // Cannot create a second admin key
    require!(key_role != KeyRole::Admin, HardigError::CannotCreateSecondAdmin);

    // Mint 1 key NFT to the target wallet
    let bump = ctx.bumps.program_pda;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.new_key_mint.to_account_info(),
                to: ctx.accounts.target_nft_ata.to_account_info(),
                authority: ctx.accounts.program_pda.to_account_info(),
            },
            signer_seeds,
        ),
        1,
    )?;

    // Create the KeyAuthorization
    let key_auth = &mut ctx.accounts.new_key_auth;
    key_auth.position = ctx.accounts.position.key();
    key_auth.key_nft_mint = ctx.accounts.new_key_mint.key();
    key_auth.role = key_role;
    key_auth.bump = ctx.bumps.new_key_auth;

    Ok(())
}
