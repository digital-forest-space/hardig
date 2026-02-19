use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::ProtocolConfig;

/// Migration for ProtocolConfig. Handles two transitions:
///
/// v0 -> v2 (41 -> 105 bytes):
///   Old: [discriminator(8)][admin(32)][bump(1)]
///   New: [discriminator(8)][admin(32)][collection(32)][pending_admin(32)][bump(1)]
///
/// v1 -> v2 (73 -> 105 bytes):
///   Old: [discriminator(8)][admin(32)][collection(32)][bump(1)]
///   New: [discriminator(8)][admin(32)][collection(32)][pending_admin(32)][bump(1)]
#[derive(Accounts)]
pub struct MigrateConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The config PDA. We use UncheckedAccount because the old layout
    /// can't be deserialized as the new ProtocolConfig struct.
    /// CHECK: Validated via PDA seeds and manual data inspection.
    #[account(
        mut,
        seeds = [ProtocolConfig::SEED],
        bump,
    )]
    pub config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

const V0_SIZE: usize = 8 + 32 + 1;      // 41 bytes
const V1_SIZE: usize = 8 + 32 + 32 + 1;  // 73 bytes

pub fn handler(ctx: Context<MigrateConfig>) -> Result<()> {
    let config_info = &ctx.accounts.config.to_account_info();
    let data = config_info.try_borrow_data()?;
    let current_size = data.len();

    // Guard: already at latest size
    require!(
        current_size == V0_SIZE || current_size == V1_SIZE,
        HardigError::AlreadyMigrated
    );

    // Read admin (always at offset 8..40 in all versions)
    let admin = Pubkey::try_from(&data[8..40]).unwrap();

    // Verify caller is admin
    require!(admin == ctx.accounts.admin.key(), HardigError::Unauthorized);

    // Read version-specific fields
    let (collection, bump) = if current_size == V0_SIZE {
        // v0: no collection field, bump at offset 40
        (Pubkey::default(), data[40])
    } else {
        // v1: collection at 40..72, bump at 72
        let collection = Pubkey::try_from(&data[40..72]).unwrap();
        (collection, data[72])
    };

    drop(data);

    // Realloc to new size (105 bytes)
    let new_size = ProtocolConfig::SIZE;
    let rent = Rent::get()?;
    let new_min_balance = rent.minimum_balance(new_size);
    let current_balance = config_info.lamports();
    let diff = new_min_balance.saturating_sub(current_balance);

    if diff > 0 {
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.admin.to_account_info(),
                    to: config_info.clone(),
                },
            ),
            diff,
        )?;
    }

    #[allow(deprecated)]
    config_info.realloc(new_size, false)?;

    // Write new layout
    let mut data = config_info.try_borrow_mut_data()?;
    // data[0..8]    — discriminator (unchanged)
    // data[8..40]   — admin (unchanged)
    // data[40..72]  — collection
    data[40..72].copy_from_slice(&collection.to_bytes());
    // data[72..104] — pending_admin = Pubkey::default()
    data[72..104].copy_from_slice(&Pubkey::default().to_bytes());
    // data[104]     — bump
    data[104] = bump;

    Ok(())
}
