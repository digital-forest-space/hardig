use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::ProtocolConfig;

/// One-time migration: realloc ProtocolConfig from 41 bytes (v0: admin + bump)
/// to 73 bytes (v1: admin + collection + bump), inserting collection = Pubkey::default().
///
/// Old layout (41 bytes): [discriminator(8)][admin(32)][bump(1)]
/// New layout (73 bytes): [discriminator(8)][admin(32)][collection(32)][bump(1)]
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

const OLD_SIZE: usize = 8 + 32 + 1; // 41 bytes

pub fn handler(ctx: Context<MigrateConfig>) -> Result<()> {
    let config_info = &ctx.accounts.config.to_account_info();
    let data = config_info.try_borrow_data()?;

    // Guard: already migrated
    require!(data.len() == OLD_SIZE, HardigError::AlreadyMigrated);

    // Read old fields
    let admin = Pubkey::try_from(&data[8..40]).unwrap();
    let bump = data[40];

    // Verify caller is admin
    require!(admin == ctx.accounts.admin.key(), HardigError::Unauthorized);

    drop(data);

    // Realloc to new size
    let new_size = ProtocolConfig::SIZE; // 73
    let rent = Rent::get()?;
    let new_min_balance = rent.minimum_balance(new_size);
    let current_balance = config_info.lamports();
    let diff = new_min_balance.saturating_sub(current_balance);

    if diff > 0 {
        // Transfer additional rent from admin
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

    // Write new layout: discriminator stays, admin stays, insert collection, then bump
    let mut data = config_info.try_borrow_mut_data()?;
    // data[0..8]   — discriminator (unchanged)
    // data[8..40]  — admin (unchanged)
    // data[40..72] — collection = Pubkey::default()
    data[40..72].copy_from_slice(&Pubkey::default().to_bytes());
    // data[72]     — bump
    data[72] = bump;

    Ok(())
}
