use anchor_lang::prelude::*;
use anchor_spl::metadata::{
    create_master_edition_v3, create_metadata_accounts_v3,
    mpl_token_metadata::types::DataV2,
    CreateMasterEditionV3, CreateMetadataAccountsV3, Metadata as MetaplexProgram,
};
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

use crate::errors::HardigError;
use crate::state::{
    KeyAuthorization, PositionNFT, RateBucket, PERM_LIMITED_BORROW, PERM_LIMITED_SELL,
    PERM_MANAGE_KEYS,
};

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
        mint::freeze_authority = program_pda,
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

    /// Per-position authority PDA used as mint authority.
    /// CHECK: PDA derived from program, not read.
    #[account(
        seeds = [b"authority", position.admin_nft_mint.as_ref()],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    /// Metaplex Token Metadata PDA for the new key NFT.
    /// CHECK: Created by Metaplex CPI; derived as ["metadata", metaplex_program, mint].
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,

    /// Master Edition PDA for the new key NFT.
    /// CHECK: Created by Metaplex CPI; derived as ["metadata", metaplex_program, mint, "edition"].
    #[account(mut)]
    pub master_edition: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub token_metadata_program: Program<'info, MetaplexProgram>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<AuthorizeKey>,
    permissions: u8,
    sell_bucket_capacity: u64,
    sell_refill_period_slots: u64,
    borrow_bucket_capacity: u64,
    borrow_refill_period_slots: u64,
) -> Result<()> {
    // Validate the admin holds their key
    validate_key(
        &ctx.accounts.admin,
        &ctx.accounts.admin_nft_ata,
        &ctx.accounts.admin_key_auth,
        &ctx.accounts.position.key(),
        PERM_MANAGE_KEYS,
    )?;

    // Validate the permissions bitmask
    require!(permissions != 0, HardigError::InvalidKeyRole);
    require!(permissions & PERM_MANAGE_KEYS == 0, HardigError::CannotCreateSecondAdmin);

    // Validate rate-limit params match permission bits
    if permissions & PERM_LIMITED_SELL != 0 {
        require!(
            sell_bucket_capacity > 0 && sell_refill_period_slots > 0,
            HardigError::InvalidKeyRole
        );
    } else {
        require!(
            sell_bucket_capacity == 0 && sell_refill_period_slots == 0,
            HardigError::InvalidKeyRole
        );
    }
    if permissions & PERM_LIMITED_BORROW != 0 {
        require!(
            borrow_bucket_capacity > 0 && borrow_refill_period_slots > 0,
            HardigError::InvalidKeyRole
        );
    } else {
        require!(
            borrow_bucket_capacity == 0 && borrow_refill_period_slots == 0,
            HardigError::InvalidKeyRole
        );
    }

    // Mint 1 key NFT to the target wallet
    let bump = ctx.bumps.program_pda;
    let mint_key = ctx.accounts.position.admin_nft_mint;
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", mint_key.as_ref(), &[bump]]];

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

    // Create Metaplex metadata
    create_metadata_accounts_v3(
        CpiContext::new_with_signer(
            ctx.accounts.token_metadata_program.to_account_info(),
            CreateMetadataAccountsV3 {
                metadata: ctx.accounts.metadata.to_account_info(),
                mint: ctx.accounts.new_key_mint.to_account_info(),
                mint_authority: ctx.accounts.program_pda.to_account_info(),
                payer: ctx.accounts.admin.to_account_info(),
                update_authority: ctx.accounts.program_pda.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
            signer_seeds,
        ),
        DataV2 {
            name: "H\u{00e4}rdig Key".to_string(),
            symbol: "HKEY".to_string(),
            uri: String::new(),
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        },
        true, // is_mutable
        true, // update_authority_is_signer
        None, // collection_details
    )?;

    // Create Master Edition (max_supply=0 freezes supply, replaces set_authority(None))
    create_master_edition_v3(
        CpiContext::new_with_signer(
            ctx.accounts.token_metadata_program.to_account_info(),
            CreateMasterEditionV3 {
                edition: ctx.accounts.master_edition.to_account_info(),
                mint: ctx.accounts.new_key_mint.to_account_info(),
                update_authority: ctx.accounts.program_pda.to_account_info(),
                mint_authority: ctx.accounts.program_pda.to_account_info(),
                payer: ctx.accounts.admin.to_account_info(),
                metadata: ctx.accounts.metadata.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
            signer_seeds,
        ),
        Some(0),
    )?;

    let current_slot = Clock::get()?.slot;

    // Create the KeyAuthorization
    let key_auth = &mut ctx.accounts.new_key_auth;
    key_auth.position = ctx.accounts.position.key();
    key_auth.key_nft_mint = ctx.accounts.new_key_mint.key();
    key_auth.permissions = permissions;
    key_auth.bump = ctx.bumps.new_key_auth;

    // Initialize rate-limit buckets
    if permissions & PERM_LIMITED_SELL != 0 {
        key_auth.sell_bucket = RateBucket {
            capacity: sell_bucket_capacity,
            refill_period: sell_refill_period_slots,
            level: sell_bucket_capacity, // starts full
            last_update: current_slot,
        };
    }
    if permissions & PERM_LIMITED_BORROW != 0 {
        key_auth.borrow_bucket = RateBucket {
            capacity: borrow_bucket_capacity,
            refill_period: borrow_refill_period_slots,
            level: borrow_bucket_capacity, // starts full
            last_update: current_slot,
        };
    }

    Ok(())
}
