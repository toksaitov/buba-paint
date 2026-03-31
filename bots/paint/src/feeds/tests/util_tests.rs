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
