use std::time::Duration;

use serde::Serialize;

/// Exponential backoff with jitter for `WebSocket` reconnection.
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

/// One classified feed disconnect or reconnect-failure cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FeedDisconnectCause {
    WebsocketError,
    ConnectTimeout,
    IdleTimeout,
    StaleTimeout,
    PingFailure,
    ConnectionFailed,
}

impl FeedDisconnectCause {
    /// Return the stable operator/log label for this cause.
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::WebsocketError => "websocket_error",
            Self::ConnectTimeout => "connect_timeout",
            Self::IdleTimeout => "idle_timeout",
            Self::StaleTimeout => "stale_timeout",
            Self::PingFailure => "ping_failure",
            Self::ConnectionFailed => "connection_failed",
        }
    }
}

/// Structured feed-health metadata persisted alongside disconnect/stale events.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct FeedHealthDetails {
    #[serde(rename = "causeClass")]
    pub cause_class: &'static str,
    #[serde(rename = "attempt")]
    pub attempt: u32,
    #[serde(rename = "reconnectDelayMs", skip_serializing_if = "Option::is_none")]
    pub reconnect_delay_ms: Option<u64>,
    #[serde(
        rename = "connectionLifetimeMs",
        skip_serializing_if = "Option::is_none"
    )]
    pub connection_lifetime_ms: Option<u64>,
    #[serde(rename = "afterResubscribe")]
    pub after_resubscribe: bool,
    #[serde(rename = "error", skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "timeoutMs", skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl FeedHealthDetails {
    /// Build one health-details payload for feed lifecycle logging.
    #[must_use]
    pub(crate) fn new(
        cause: FeedDisconnectCause,
        attempt: u32,
        reconnect_delay_ms: Option<u64>,
        connection_lifetime_ms: Option<u64>,
        after_resubscribe: bool,
        error: Option<String>,
        timeout_ms: Option<u64>,
    ) -> Self {
        Self {
            cause_class: cause.as_str(),
            attempt,
            reconnect_delay_ms,
            connection_lifetime_ms,
            after_resubscribe,
            error,
            timeout_ms,
        }
    }

    /// Serialize one health-details payload into the DB/log JSON form.
    #[must_use]
    pub(crate) fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

/// One disconnected or failed connection attempt, ready for logging/retry.
#[derive(Debug, Clone)]
pub(crate) struct FeedDisconnectReport {
    pub connection_id: Option<String>,
    pub cause: FeedDisconnectCause,
    pub connection_lifetime_ms: Option<u64>,
    pub after_resubscribe: bool,
    pub error: Option<String>,
    pub timeout_ms: Option<u64>,
}

impl FeedDisconnectReport {
    /// Build the persisted health-details JSON for the chosen reconnect attempt.
    #[must_use]
    pub(crate) fn details_json(
        &self,
        attempt: u32,
        reconnect_delay_ms: Option<u64>,
    ) -> Option<String> {
        FeedHealthDetails::new(
            self.cause,
            attempt,
            reconnect_delay_ms,
            self.connection_lifetime_ms,
            self.after_resubscribe,
            self.error.clone(),
            self.timeout_ms,
        )
        .to_json()
    }
}

/// Get current time in milliseconds since epoch.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Get current time in microseconds since epoch.
pub(crate) fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

#[cfg(test)]
#[path = "tests/util_tests.rs"]
mod tests;
