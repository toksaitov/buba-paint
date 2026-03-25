use std::time::Duration;

/// Exponential backoff with jitter for WebSocket reconnection.
///
/// Computes `base_ms * 2^attempt` (capped at `max_ms`), then adds 0-25% jitter
/// derived from the current system time.
pub(crate) fn backoff_delay(attempt: u32, base_ms: u64, max_ms: u64) -> Duration {
    let exp = base_ms.saturating_mul(1u64 << attempt.min(10));
    let capped = exp.min(max_ms);
    let jitter = capped / 4;
    let jitter_val = if jitter > 0 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        u64::from(nanos) % jitter
    } else {
        0
    };
    Duration::from_millis(capped + jitter_val)
}

/// Determine whether the backoff attempt counter should reset.
///
/// Returns `true` if the connection was "stable" -- i.e. it lasted at least
/// `min_stable_ms`.  Short-lived connections (connect then immediately
/// disconnect) should NOT reset the counter, so backoff escalates naturally.
pub(crate) fn should_reset_backoff(
    connected_at_ms: u64,
    disconnected_at_ms: u64,
    min_stable_ms: u64,
) -> bool {
    disconnected_at_ms.saturating_sub(connected_at_ms) >= min_stable_ms
}

/// Get current time in milliseconds since epoch.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
#[path = "tests/util_tests.rs"]
mod tests;
