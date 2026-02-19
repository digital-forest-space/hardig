use anchor_lang::prelude::*;
use mpl_core::{
    ID as MPL_CORE_ID,
    instructions::CreateV2CpiBuilder,
    types::{
        Attribute, Attributes, PermanentBurnDelegate, PermanentTransferDelegate,
        Plugin, PluginAuthority, PluginAuthorityPair,
    },
};

use crate::errors::HardigError;
use crate::state::{
    KeyState, PositionNFT, RateBucket,
    PERM_LIMITED_BORROW, PERM_LIMITED_SELL, PERM_MANAGE_KEYS,
};

use super::validate_key::validate_key;
use super::{permission_attributes, format_sol_amount};

#[derive(Accounts)]
pub struct AuthorizeKey<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The admin's key NFT (MPL-Core asset).
    /// CHECK: Validated in handler via validate_key (owner, update_authority, permissions).
    pub admin_key_asset: UncheckedAccount<'info>,

    /// The position to authorize a key for.
    pub position: Account<'info, PositionNFT>,

    /// The MPL-Core asset for the new key NFT. Created by MPL-Core CPI.
    #[account(mut)]
    pub new_key_asset: Signer<'info>,

    /// The wallet that will receive the new key NFT.
    /// CHECK: Any wallet can receive a key.
    pub target_wallet: UncheckedAccount<'info>,

    /// KeyState PDA for the new key (tracks mutable state like rate-limit buckets).
    #[account(
        init,
        payer = admin,
        space = KeyState::SIZE,
        seeds = [KeyState::SEED, new_key_asset.key().as_ref()],
        bump,
    )]
    pub key_state: Account<'info, KeyState>,

    /// Per-position authority PDA.
    /// CHECK: PDA derived from program.
    #[account(
        seeds = [b"authority", position.admin_asset.as_ref()],
        bump,
    )]
    pub program_pda: UncheckedAccount<'info>,

    /// CHECK: MPL-Core program validated by address constraint.
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
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
        &ctx.accounts.admin_key_asset.to_account_info(),
        &ctx.accounts.program_pda.key(),
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

    // Build attribute list with human-readable permissions
    let mut attrs = permission_attributes(permissions);
    if permissions & PERM_LIMITED_SELL != 0 {
        attrs.push(Attribute {
            key: "sell_limit".to_string(),
            value: format!("{} navSOL / {} slots", format_sol_amount(sell_bucket_capacity), sell_refill_period_slots),
        });
    }
    if permissions & PERM_LIMITED_BORROW != 0 {
        attrs.push(Attribute {
            key: "borrow_limit".to_string(),
            value: format!("{} SOL / {} slots", format_sol_amount(borrow_bucket_capacity), borrow_refill_period_slots),
        });
    }

    // Create the new key NFT via MPL-Core
    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.new_key_asset.to_account_info())
        .payer(&ctx.accounts.admin.to_account_info())
        .owner(Some(&ctx.accounts.target_wallet.to_account_info()))
        .update_authority(Some(&ctx.accounts.program_pda.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name("H\u{00e4}rdig Key".to_string())
        .uri(String::new())
        .plugins(vec![
            PluginAuthorityPair {
                plugin: Plugin::Attributes(Attributes {
                    attribute_list: attrs,
                }),
                authority: Some(PluginAuthority::UpdateAuthority),
            },
            PluginAuthorityPair {
                plugin: Plugin::PermanentBurnDelegate(PermanentBurnDelegate {}),
                authority: Some(PluginAuthority::UpdateAuthority),
            },
            PluginAuthorityPair {
                plugin: Plugin::PermanentTransferDelegate(PermanentTransferDelegate {}),
                authority: Some(PluginAuthority::UpdateAuthority),
            },
        ])
        .invoke()?;

    let current_slot = Clock::get()?.slot;

    // Initialize the KeyState
    let key_state = &mut ctx.accounts.key_state;
    key_state.asset = ctx.accounts.new_key_asset.key();
    key_state.bump = ctx.bumps.key_state;

    if permissions & PERM_LIMITED_SELL != 0 {
        key_state.sell_bucket = RateBucket {
            capacity: sell_bucket_capacity,
            refill_period: sell_refill_period_slots,
            level: sell_bucket_capacity, // starts full
            last_update: current_slot,
        };
    }
    if permissions & PERM_LIMITED_BORROW != 0 {
        key_state.borrow_bucket = RateBucket {
            capacity: borrow_bucket_capacity,
            refill_period: borrow_refill_period_slots,
            level: borrow_bucket_capacity, // starts full
            last_update: current_slot,
        };
    }

    Ok(())
}
