use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, msg, pubkey::Pubkey,
};

entrypoint!(process_instruction);

/// Instruction discriminators (must match hardig::mayflower::constants).
const IX_BUY: [u8; 8] = [30, 205, 124, 67, 20, 142, 236, 136];
const IX_SELL: [u8; 8] = [223, 239, 212, 254, 255, 120, 53, 1];
const IX_BORROW: [u8; 8] = [228, 253, 131, 202, 207, 116, 89, 18];
const IX_REPAY: [u8; 8] = [234, 103, 67, 82, 208, 234, 219, 166];

/// PersonalPosition account layout offsets (must match hardig::mayflower::constants).
const PP_DEPOSITED_SHARES_OFFSET: usize = 104;
const PP_DEBT_OFFSET: usize = 112;

/// Mock Mayflower program that simulates account mutations for testing.
///
/// - Buy: increments deposited_shares in PersonalPosition by input_amount (1:1 ratio).
/// - Sell: decrements deposited_shares by input_amount.
/// - Borrow: increments debt in PersonalPosition by borrow_amount.
/// - Repay: decrements debt by repay_amount.
fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.len() < 8 {
        // init_personal_position or other instructions — just succeed
        return Ok(());
    }

    let disc: [u8; 8] = instruction_data[..8].try_into().unwrap();

    match disc {
        IX_BUY => {
            // Buy: accounts[5] = personalPosition, instruction_data[8..16] = input_amount
            if instruction_data.len() >= 16 && accounts.len() > 5 {
                let amount = u64::from_le_bytes(instruction_data[8..16].try_into().unwrap());
                let pp = &accounts[5];
                let mut data = pp.try_borrow_mut_data()?;
                if data.len() >= PP_DEPOSITED_SHARES_OFFSET + 8 {
                    let current = u64::from_le_bytes(
                        data[PP_DEPOSITED_SHARES_OFFSET..PP_DEPOSITED_SHARES_OFFSET + 8]
                            .try_into()
                            .unwrap(),
                    );
                    let new_val = current.saturating_add(amount);
                    data[PP_DEPOSITED_SHARES_OFFSET..PP_DEPOSITED_SHARES_OFFSET + 8]
                        .copy_from_slice(&new_val.to_le_bytes());
                    msg!("mock-mayflower: buy {} shares (total {})", amount, new_val);
                }
            }
        }
        IX_SELL => {
            // Sell: accounts[5] = personalPosition, instruction_data[8..16] = input_amount
            if instruction_data.len() >= 16 && accounts.len() > 5 {
                let amount = u64::from_le_bytes(instruction_data[8..16].try_into().unwrap());
                let pp = &accounts[5];
                let mut data = pp.try_borrow_mut_data()?;
                if data.len() >= PP_DEPOSITED_SHARES_OFFSET + 8 {
                    let current = u64::from_le_bytes(
                        data[PP_DEPOSITED_SHARES_OFFSET..PP_DEPOSITED_SHARES_OFFSET + 8]
                            .try_into()
                            .unwrap(),
                    );
                    let new_val = current.saturating_sub(amount);
                    data[PP_DEPOSITED_SHARES_OFFSET..PP_DEPOSITED_SHARES_OFFSET + 8]
                        .copy_from_slice(&new_val.to_le_bytes());
                    msg!("mock-mayflower: sell {} shares (total {})", amount, new_val);
                }
            }
        }
        IX_BORROW => {
            // Borrow: accounts[10] = personalPosition, accounts[8] = userBaseTokenATA
            // instruction_data[8..16] = borrow_amount
            if instruction_data.len() >= 16 && accounts.len() > 10 {
                let amount = u64::from_le_bytes(instruction_data[8..16].try_into().unwrap());

                // Update debt in PersonalPosition
                let pp = &accounts[10];
                let mut pp_data = pp.try_borrow_mut_data()?;
                if pp_data.len() >= PP_DEBT_OFFSET + 8 {
                    let current = u64::from_le_bytes(
                        pp_data[PP_DEBT_OFFSET..PP_DEBT_OFFSET + 8]
                            .try_into()
                            .unwrap(),
                    );
                    let new_val = current.saturating_add(amount);
                    pp_data[PP_DEBT_OFFSET..PP_DEBT_OFFSET + 8]
                        .copy_from_slice(&new_val.to_le_bytes());
                    msg!("mock-mayflower: borrow {} (debt {})", amount, new_val);
                }
            }
        }
        IX_REPAY => {
            // Repay: accounts[3] = personalPosition, instruction_data[8..16] = repay_amount
            if instruction_data.len() >= 16 && accounts.len() > 3 {
                let amount = u64::from_le_bytes(instruction_data[8..16].try_into().unwrap());
                let pp = &accounts[3];
                let mut data = pp.try_borrow_mut_data()?;
                if data.len() >= PP_DEBT_OFFSET + 8 {
                    let current = u64::from_le_bytes(
                        data[PP_DEBT_OFFSET..PP_DEBT_OFFSET + 8]
                            .try_into()
                            .unwrap(),
                    );
                    let new_val = current.saturating_sub(amount);
                    data[PP_DEBT_OFFSET..PP_DEBT_OFFSET + 8]
                        .copy_from_slice(&new_val.to_le_bytes());
                    msg!("mock-mayflower: repay {} (debt {})", amount, new_val);
                }
            }
        }
        _ => {
            // Other instructions (e.g., init_personal_position) — just succeed
        }
    }

    Ok(())
}
