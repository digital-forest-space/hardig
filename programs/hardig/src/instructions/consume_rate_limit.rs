use anchor_lang::prelude::*;

use crate::errors::HardigError;
use crate::state::RateBucket;

/// Consume `amount` from a token-bucket rate limiter.
///
/// Refills the bucket proportionally based on elapsed slots, then drains `amount`.
/// Returns `Err(RateLimitExceeded)` if insufficient tokens remain after refill.
pub fn consume_rate_limit(bucket: &mut RateBucket, amount: u64, current_slot: u64) -> Result<()> {
    let elapsed = current_slot.saturating_sub(bucket.last_update);

    // Refill: capacity * elapsed / refill_period, capped at capacity
    let refill = if elapsed >= bucket.refill_period {
        bucket.capacity
    } else {
        // Use u128 to avoid overflow on large capacity * elapsed products
        ((bucket.capacity as u128) * (elapsed as u128) / (bucket.refill_period as u128)) as u64
    };

    bucket.level = bucket.level.saturating_add(refill).min(bucket.capacity);
    bucket.last_update = current_slot;

    require!(bucket.level >= amount, HardigError::RateLimitExceeded);

    bucket.level -= amount;
    Ok(())
}
