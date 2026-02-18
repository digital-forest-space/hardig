use anchor_lang::prelude::*;
use anchor_spl::metadata::{
    create_master_edition_v3, create_metadata_accounts_v3,
    mpl_token_metadata::types::DataV2,
    CreateMasterEditionV3, CreateMetadataAccountsV3, Metadata as MetaplexProgram,
};
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

use crate::state::{KeyAuthorization, PositionNFT, PRESET_ADMIN};

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
        mint::freeze_authority = program_pda,
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

    /// Per-position authority PDA used as mint authority.
    /// CHECK: PDA derived from program, not read.
    #[account(
        seeds = [b"authority", admin_nft_mint.key().as_ref()],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    /// Metaplex Token Metadata PDA for the admin NFT.
    /// CHECK: Created by Metaplex CPI; derived as ["metadata", metaplex_program, mint].
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,

    /// Master Edition PDA for the admin NFT.
    /// CHECK: Created by Metaplex CPI; derived as ["metadata", metaplex_program, mint, "edition"].
    #[account(mut)]
    pub master_edition: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub token_metadata_program: Program<'info, MetaplexProgram>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<CreatePosition>, max_reinvest_spread_bps: u16) -> Result<()> {
    // Mint 1 admin key NFT to the admin's ATA
    let bump = ctx.bumps.program_pda;
    let mint_key = ctx.accounts.admin_nft_mint.key();
    let signer_seeds: &[&[&[u8]]] = &[&[b"authority", mint_key.as_ref(), &[bump]]];

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

    // Create Metaplex metadata
    create_metadata_accounts_v3(
        CpiContext::new_with_signer(
            ctx.accounts.token_metadata_program.to_account_info(),
            CreateMetadataAccountsV3 {
                metadata: ctx.accounts.metadata.to_account_info(),
                mint: ctx.accounts.admin_nft_mint.to_account_info(),
                mint_authority: ctx.accounts.program_pda.to_account_info(),
                payer: ctx.accounts.admin.to_account_info(),
                update_authority: ctx.accounts.program_pda.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
            signer_seeds,
        ),
        DataV2 {
            name: "H\u{00e4}rdig Admin Key".to_string(),
            symbol: "HKEY".to_string(),
            uri: "https://gateway.irys.xyz/8o9S13VAezVYyU7TCzYxLkt9Uw25Z1bNb1jLTcdM2NBA".to_string(),
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
                mint: ctx.accounts.admin_nft_mint.to_account_info(),
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

    // Initialize the position
    let position = &mut ctx.accounts.position;
    position.admin_nft_mint = ctx.accounts.admin_nft_mint.key();
    position.position_pda = Pubkey::default(); // Set during init_mayflower_position
    position.market_config = Pubkey::default(); // Set during init_mayflower_position
    position.deposited_nav = 0;
    position.user_debt = 0;
    position.max_reinvest_spread_bps = max_reinvest_spread_bps;
    position.last_admin_activity = Clock::get()?.unix_timestamp;
    position.bump = ctx.bumps.position;
    position.authority_bump = ctx.bumps.program_pda;

    // Initialize the admin KeyAuthorization
    let key_auth = &mut ctx.accounts.admin_key_auth;
    key_auth.position = ctx.accounts.position.key();
    key_auth.key_nft_mint = ctx.accounts.admin_nft_mint.key();
    key_auth.permissions = PRESET_ADMIN;
    key_auth.bump = ctx.bumps.admin_key_auth;

    Ok(())
}
