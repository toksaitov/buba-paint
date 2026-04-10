use super::*;

/// Verifies that attempt 0 returns base plus jitter.
#[test]
fn attempt_0_returns_base_plus_jitter() {
    let d = backoff_delay(0, 1000, 60_000);

    assert!(d.as_millis() >= 1000);
    assert!(d.as_millis() < 1000 + 250);
}

/// Verifies that attempt 1 returns double base plus jitter.
#[test]
fn attempt_1_returns_double_base_plus_jitter() {
    let d = backoff_delay(1, 1000, 60_000);

    assert!(d.as_millis() >= 2000);
    assert!(d.as_millis() < 2000 + 500);
}

/// Verifies that large attempt capped at max.
#[test]
fn large_attempt_capped_at_max() {
    let d = backoff_delay(20, 1000, 30_000);

    assert!(d.as_millis() >= 30_000);
    assert!(d.as_millis() < 30_000 + 7500);
}

/// Verifies that very large attempt does not overflow.
#[test]
fn very_large_attempt_does_not_overflow() {
    let d = backoff_delay(100, 1000, 60_000);

    assert!(d.as_millis() >= 60_000);
    assert!(d.as_millis() < 60_000 + 15_000);
}

/// Verifies that zero base returns zero.
#[test]
fn zero_base_returns_zero() {
    let d = backoff_delay(5, 0, 60_000);

    assert_eq!(d.as_millis(), 0);
}

/// Verifies that zero max returns zero.
#[test]
fn zero_max_returns_zero() {
    let d = backoff_delay(5, 1000, 0);

    assert_eq!(d.as_millis(), 0);
}

/// Verifies that attempt 0 base equals max.
#[test]
fn attempt_0_base_equals_max() {
    let d = backoff_delay(0, 5000, 5000);

    assert!(d.as_millis() >= 5000);
    assert!(d.as_millis() < 5000 + 1250);
}

/// Verifies that should reset backoff stable connection.
#[test]
fn should_reset_backoff_stable_connection() {
    assert!(should_reset_backoff(1000, 10000, 5000));
}

/// Verifies that should reset backoff unstable connection.
#[test]
fn should_reset_backoff_unstable_connection() {
    assert!(!should_reset_backoff(1000, 2000, 5000));
}

/// Verifies that should reset backoff exact boundary.
#[test]
fn should_reset_backoff_exact_boundary() {
    assert!(should_reset_backoff(1000, 6000, 5000));
}

/// Verifies that should reset backoff zero duration.
#[test]
fn should_reset_backoff_zero_duration() {
    assert!(!should_reset_backoff(1000, 1000, 5000));
}

/// Verifies that should reset backoff zero threshold.
#[test]
fn should_reset_backoff_zero_threshold() {
    assert!(should_reset_backoff(1000, 1001, 0));
}

/// Verifies that should reset backoff overflow protection.
#[test]
fn should_reset_backoff_overflow_protection() {
    assert!(!should_reset_backoff(u64::MAX, 0, 5000));
}

/// Verifies that disconnect causes expose stable operator labels.
#[test]
fn feed_disconnect_cause_labels_are_stable() {
    assert_eq!(
        FeedDisconnectCause::WebsocketError.as_str(),
        "websocket_error"
    );
    assert_eq!(
        FeedDisconnectCause::ConnectTimeout.as_str(),
        "connect_timeout"
    );
    assert_eq!(FeedDisconnectCause::IdleTimeout.as_str(), "idle_timeout");
    assert_eq!(FeedDisconnectCause::StaleTimeout.as_str(), "stale_timeout");
    assert_eq!(FeedDisconnectCause::PingFailure.as_str(), "ping_failure");
    assert_eq!(
        FeedDisconnectCause::ConnectionFailed.as_str(),
        "connection_failed"
    );
}

/// Verifies that feed-health detail payloads serialize expected reconnect metadata.
#[test]
fn feed_health_details_serialize_reconnect_metadata() {
    let json = FeedHealthDetails::new(
        FeedDisconnectCause::IdleTimeout,
        2,
        Some(1200),
        Some(4500),
        true,
        None,
        Some(20_000),
    )
    .to_json()
    .expect("details JSON should serialize");

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["causeClass"], "idle_timeout");
    assert_eq!(value["attempt"], 2);
    assert_eq!(value["reconnectDelayMs"], 1200);
    assert_eq!(value["connectionLifetimeMs"], 4500);
    assert_eq!(value["afterResubscribe"], true);
    assert_eq!(value["timeoutMs"], 20000);
}

/// Verifies that feed-disconnect reports derive their JSON from the chosen retry delay.
#[test]
fn feed_disconnect_report_builds_details_json() {
    let report = FeedDisconnectReport {
        connection_id: Some("feed-1".to_string()),
        cause: FeedDisconnectCause::ConnectTimeout,
        connection_lifetime_ms: None,
        after_resubscribe: false,
        error: Some("connect timed out".to_string()),
        timeout_ms: Some(10_000),
    };

    let json = report
        .details_json(3, Some(2200))
        .expect("report JSON should serialize");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["causeClass"], "connect_timeout");
    assert_eq!(value["attempt"], 3);
    assert_eq!(value["reconnectDelayMs"], 2200);
    assert_eq!(value["error"], "connect timed out");
}
