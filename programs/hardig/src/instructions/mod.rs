pub mod accept_admin;
pub mod authorize_key;
pub mod borrow;
pub mod buy;
pub mod configure_recovery;
pub mod consume_rate_limit;
pub mod create_collection;
pub mod create_market_config;
pub mod create_position;
pub mod execute_recovery;
pub mod heartbeat;
pub mod initialize_protocol;
pub mod migrate_config;
pub mod reinvest;
pub mod repay;
pub mod revoke_key;
pub mod transfer_admin;
pub mod validate_key;
pub mod withdraw;

#[allow(ambiguous_glob_reexports)]
pub use accept_admin::*;
pub use authorize_key::*;
pub use borrow::*;
pub use buy::*;
pub use configure_recovery::*;
pub use create_collection::*;
pub use create_market_config::*;
pub use create_position::*;
pub use execute_recovery::*;
pub use heartbeat::*;
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
/// Does NOT include limited_sell/limited_borrow — those are added by authorize_key
/// with the actual capacity/period values merged in.
pub fn permission_attributes(permissions: u8) -> Vec<Attribute> {
    let flag = |bit: u8| -> &'static str {
        if permissions & bit != 0 { "true" } else { "false" }
    };

    vec![
        Attribute { key: "permissions".to_string(), value: permissions.to_string() },
        Attribute { key: "buy".to_string(), value: flag(PERM_BUY).to_string() },
        Attribute { key: "sell".to_string(), value: flag(PERM_SELL).to_string() },
        Attribute { key: "borrow".to_string(), value: flag(PERM_BORROW).to_string() },
        Attribute { key: "repay".to_string(), value: flag(PERM_REPAY).to_string() },
        Attribute { key: "reinvest".to_string(), value: flag(PERM_REINVEST).to_string() },
        Attribute { key: "manage_keys".to_string(), value: flag(PERM_MANAGE_KEYS).to_string() },
    ]
}

/// Convert a slot count to a human-readable duration string using ~400ms per slot.
/// Examples: "15 days", "30 days, 12 hours", "6 hours", "45 minutes".
/// NOTE: Used for both on-chain NFT attributes and TUI display — changes affect both.
pub fn slots_to_duration(slots: u64) -> String {
    let total_secs = slots * 400 / 1000;
    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let minutes = (total_secs % 3600) / 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(if days == 1 { "1 day".to_string() } else { format!("{} days", days) });
    }
    if hours > 0 {
        parts.push(if hours == 1 { "1 hour".to_string() } else { format!("{} hours", hours) });
    }
    if parts.is_empty() {
        let m = minutes.max(1);
        parts.push(if m == 1 { "1 minute".to_string() } else { format!("{} minutes", m) });
    }
    parts.join(", ")
}

/// Image hosted on Irys, shared by all Härdig key NFTs.
const KEY_IMAGE: &str = "https://gateway.irys.xyz/GKa2AyPSRe2VnsPXBepTzhzohEBsLNdxFctR1MFoYojK";

/// Build an inline `data:application/json,...` metadata URI so wallets that don't
/// read MPL-Core Attributes (e.g. Phantom) can still display name/image/description.
/// Only includes permission attributes that are actually set, to keep the URI compact.
/// `limited_sell` / `limited_borrow` are passed as pre-formatted strings (e.g. "5 SOL / 15 days").
/// `market` is the market name (e.g. "navSOL", "navETH").
/// `position_name` is the admin asset's on-chain name (for delegated keys).
pub fn metadata_uri(
    name: &str,
    permissions: u8,
    limited_sell: Option<&str>,
    limited_borrow: Option<&str>,
    market: Option<&str>,
    position_name: Option<&str>,
) -> String {
    let mut attrs = Vec::new();
    let bits: &[(u8, &str)] = &[
        (PERM_BUY, "buy"),
        (PERM_SELL, "sell"),
        (PERM_BORROW, "borrow"),
        (PERM_REPAY, "repay"),
        (PERM_REINVEST, "reinvest"),
        (PERM_MANAGE_KEYS, "manage_keys"),
    ];
    for &(bit, label) in bits {
        if permissions & bit != 0 {
            attrs.push(format!("{{\"trait_type\":\"{}\",\"value\":\"true\"}}", label));
        }
    }
    if let Some(v) = limited_sell {
        attrs.push(format!("{{\"trait_type\":\"limited_sell\",\"value\":\"{}\"}}", v));
    }
    if let Some(v) = limited_borrow {
        attrs.push(format!("{{\"trait_type\":\"limited_borrow\",\"value\":\"{}\"}}", v));
    }
    if let Some(v) = market {
        attrs.push(format!("{{\"trait_type\":\"market\",\"value\":\"{}\"}}", v));
    }
    if let Some(v) = position_name {
        attrs.push(format!("{{\"trait_type\":\"position_name\",\"value\":\"{}\"}}", v));
    }

    format!(
        "data:application/json,{{\"name\":\"{}\",\"symbol\":\"HKEY\",\"description\":\"Permission key for managing a H\\u00e4rdig position.\",\"image\":\"{}\",\"attributes\":[{}]}}",
        name, KEY_IMAGE, attrs.join(","),
    )
}

/// Format a raw u64 amount (lamports or shares, 9 decimals) as a human-readable string.
/// Trailing zeros after the decimal point are trimmed. Integer amounts have no decimal point.
/// NOTE: Used for both on-chain NFT attributes and TUI display — changes affect both.
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
