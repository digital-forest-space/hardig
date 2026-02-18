use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, CloseAccount, Mint, Token, TokenAccount};

use crate::errors::HardigError;
use crate::state::{KeyAuthorization, KeyRole, PositionNFT};

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
        constraint = target_key_auth.role != KeyRole::Admin @ HardigError::CannotRevokeAdminKey,
    )]
    pub target_key_auth: Account<'info, KeyAuthorization>,

    /// The mint of the key NFT being revoked.
    #[account(
        mut,
        constraint = target_nft_mint.key() == target_key_auth.key_nft_mint @ HardigError::InvalidKey,
    )]
    pub target_nft_mint: Account<'info, Mint>,

    /// The ATA holding the key NFT. Optional — only needed when the admin holds
    /// the target NFT and wants it burned during revoke. When provided and the
    /// admin is the token account owner, the NFT is burned and the ATA closed.
    /// When omitted (or when the admin is not the owner), only the
    /// KeyAuthorization PDA is closed.
    #[account(mut)]
    pub target_nft_ata: Option<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<RevokeKey>) -> Result<()> {
    // Validate the admin holds their key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_nft_ata,
        &ctx.accounts.admin_key_auth,
        &ctx.accounts.position.key(),
        &[KeyRole::Admin],
    )?;

    // The target_key_auth is closed by the `close = admin` constraint.

    // If the caller provided the target NFT ATA and the admin owns it with a
    // positive balance, burn the NFT and close the token account. This handles
    // the case where the admin has reclaimed the key (e.g. theft recovery) and
    // wants to eliminate the orphaned token.
    if let Some(target_ata) = &ctx.accounts.target_nft_ata {
        if target_ata.owner == ctx.accounts.admin.key()
            && target_ata.mint == ctx.accounts.target_nft_mint.key()
            && target_ata.amount > 0
        {
            // Burn the NFT token — admin is the token account owner, so they
            // can authorize the burn without needing mint authority.
            token::burn(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Burn {
                        mint: ctx.accounts.target_nft_mint.to_account_info(),
                        from: target_ata.to_account_info(),
                        authority: ctx.accounts.admin.to_account_info(),
                    },
                ),
                1,
            )?;

            // Close the now-empty token account, returning rent to admin.
            token::close_account(CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                CloseAccount {
                    account: target_ata.to_account_info(),
                    destination: ctx.accounts.admin.to_account_info(),
                    authority: ctx.accounts.admin.to_account_info(),
                },
            ))?;
        }
    }

    Ok(())
}
