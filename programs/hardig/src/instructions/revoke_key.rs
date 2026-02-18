use anchor_lang::prelude::*;
use anchor_spl::metadata::{BurnNft, burn_nft, Metadata as MetaplexProgram};
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::errors::HardigError;
use crate::state::{KeyAuthorization, PositionNFT, PERM_MANAGE_KEYS};

use super::validate_key::validate_key;

#[derive(Accounts)]
pub struct RevokeKey<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT token account.
    pub admin_nft_ata: Account<'info, TokenAccount>,

    /// The admin's KeyAuthorization.
    #[account(
        constraint = admin_key_auth.position == position.key() @ HardigError::WrongPosition,
    )]
    pub admin_key_auth: Account<'info, KeyAuthorization>,

    /// The position.
    pub position: Account<'info, PositionNFT>,

    /// The KeyAuthorization to revoke. Closed, rent refunded to admin.
    #[account(
        mut,
        close = admin,
        constraint = target_key_auth.position == position.key() @ HardigError::WrongPosition,
        constraint = target_key_auth.key_nft_mint != position.admin_nft_mint @ HardigError::CannotRevokeAdminKey,
    )]
    pub target_key_auth: Account<'info, KeyAuthorization>,

    /// The mint of the key NFT being revoked.
    #[account(
        mut,
        constraint = target_nft_mint.key() == target_key_auth.key_nft_mint @ HardigError::InvalidKey,
    )]
    pub target_nft_mint: Account<'info, Mint>,

    /// The ATA holding the key NFT. Optional — only needed when the admin holds
    /// the target NFT and wants it burned during revoke.
    #[account(mut)]
    pub target_nft_ata: Option<Account<'info, TokenAccount>>,

    /// Metaplex Metadata PDA for the target NFT. Optional — required for burn.
    /// CHECK: Validated by Metaplex CPI.
    #[account(mut)]
    pub metadata: Option<UncheckedAccount<'info>>,

    /// Master Edition PDA for the target NFT. Optional — required for burn.
    /// CHECK: Validated by Metaplex CPI.
    #[account(mut)]
    pub master_edition: Option<UncheckedAccount<'info>>,

    pub token_program: Program<'info, Token>,
    pub token_metadata_program: Option<Program<'info, MetaplexProgram>>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<RevokeKey>) -> Result<()> {
    // Validate the admin holds their key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_nft_ata,
        &ctx.accounts.admin_key_auth,
        &ctx.accounts.position.key(),
        PERM_MANAGE_KEYS,
    )?;

    // The target_key_auth is closed by the `close = admin` constraint.

    // If the admin holds the target NFT AND metadata accounts are provided,
    // use Metaplex burn_nft which closes metadata + master_edition + token + mint
    // in one CPI call.
    if let (
        Some(target_ata),
        Some(metadata),
        Some(master_edition),
        Some(token_metadata_program),
    ) = (
        &ctx.accounts.target_nft_ata,
        &ctx.accounts.metadata,
        &ctx.accounts.master_edition,
        &ctx.accounts.token_metadata_program,
    ) {
        if target_ata.owner == ctx.accounts.admin.key()
            && target_ata.mint == ctx.accounts.target_nft_mint.key()
            && target_ata.amount > 0
        {
            burn_nft(
                CpiContext::new(
                    token_metadata_program.to_account_info(),
                    BurnNft {
                        metadata: metadata.to_account_info(),
                        owner: ctx.accounts.admin.to_account_info(),
                        mint: ctx.accounts.target_nft_mint.to_account_info(),
                        token: target_ata.to_account_info(),
                        edition: master_edition.to_account_info(),
                        spl_token: ctx.accounts.token_program.to_account_info(),
                    },
                ),
                None, // no collection metadata
            )?;
        }
    }

    Ok(())
}
