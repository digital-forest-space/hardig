pub mod authorize_key;
pub mod borrow;
pub mod buy;
pub mod consume_rate_limit;
pub mod create_collection;
pub mod create_market_config;
pub mod create_position;
pub mod init_mayflower_position;
pub mod initialize_protocol;
pub mod migrate_config;
pub mod reinvest;
pub mod repay;
pub mod revoke_key;
pub mod transfer_admin;
pub mod validate_key;
pub mod withdraw;

#[allow(ambiguous_glob_reexports)]
pub use authorize_key::*;
pub use borrow::*;
pub use buy::*;
pub use create_collection::*;
pub use create_market_config::*;
pub use create_position::*;
pub use init_mayflower_position::*;
pub use initialize_protocol::*;
pub use migrate_config::*;
pub use reinvest::*;
pub use repay::*;
pub use revoke_key::*;
pub use transfer_admin::*;
pub use withdraw::*;

use mpl_core::types::Attribute;
use crate::state::*;

/// Build human-readable on-chain attributes from a permission bitmask.
pub fn permission_attributes(permissions: u8) -> Vec<Attribute> {
    let base = permissions & !PERM_LIMITED_MASK;
    let role = match base {
        PRESET_ADMIN => "Admin",
        PRESET_OPERATOR => "Operator",
        PRESET_DEPOSITOR => "Depositor",
        PRESET_KEEPER => "Keeper",
        _ => "Custom",
    };

    let flag = |bit: u8| -> &'static str {
        if permissions & bit != 0 { "true" } else { "false" }
    };

    vec![
        Attribute { key: "permissions".to_string(), value: permissions.to_string() },
        Attribute { key: "role".to_string(), value: role.to_string() },
        Attribute { key: "buy".to_string(), value: flag(PERM_BUY).to_string() },
        Attribute { key: "sell".to_string(), value: flag(PERM_SELL).to_string() },
        Attribute { key: "borrow".to_string(), value: flag(PERM_BORROW).to_string() },
        Attribute { key: "repay".to_string(), value: flag(PERM_REPAY).to_string() },
        Attribute { key: "reinvest".to_string(), value: flag(PERM_REINVEST).to_string() },
        Attribute { key: "manage_keys".to_string(), value: flag(PERM_MANAGE_KEYS).to_string() },
        Attribute { key: "limited_sell".to_string(), value: flag(PERM_LIMITED_SELL).to_string() },
        Attribute { key: "limited_borrow".to_string(), value: flag(PERM_LIMITED_BORROW).to_string() },
    ]
}

/// Format a raw u64 amount (lamports or shares, 9 decimals) as a human-readable string.
/// Trailing zeros after the decimal point are trimmed. Integer amounts have no decimal point.
pub fn format_sol_amount(raw: u64) -> String {
    let whole = raw / 1_000_000_000;
    let frac = raw % 1_000_000_000;
    if frac == 0 {
        return whole.to_string();
    }
    let frac_str = format!("{:09}", frac);
    let trimmed = frac_str.trim_end_matches('0');
    format!("{}.{}", whole, trimmed)
}
