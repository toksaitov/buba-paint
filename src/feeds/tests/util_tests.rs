use super::*;

#[test]
fn attempt_0_returns_base_plus_jitter() {
    let d = backoff_delay(0, 1000, 60_000);
    // base_ms * 2^0 = 1000, jitter = 0..250
    assert!(d.as_millis() >= 1000);
    assert!(d.as_millis() < 1000 + 250);
}

#[test]
fn attempt_1_returns_double_base_plus_jitter() {
    let d = backoff_delay(1, 1000, 60_000);
    // base_ms * 2^1 = 2000, jitter = 0..500
    assert!(d.as_millis() >= 2000);
    assert!(d.as_millis() < 2000 + 500);
}

#[test]
fn large_attempt_capped_at_max() {
    let d = backoff_delay(20, 1000, 30_000);
    // exp would be huge, but capped at 30_000. jitter = 0..7500
    assert!(d.as_millis() >= 30_000);
    assert!(d.as_millis() < 30_000 + 7500);
}

#[test]
fn very_large_attempt_does_not_overflow() {
    let d = backoff_delay(100, 1000, 60_000);
    // attempt.min(10) means 2^10 = 1024 * 1000 = 1_024_000, capped at 60_000
    assert!(d.as_millis() >= 60_000);
    assert!(d.as_millis() < 60_000 + 15_000);
}

#[test]
fn zero_base_returns_zero() {
    let d = backoff_delay(5, 0, 60_000);
    // 0 * anything = 0, capped at 0 (since 0 < 60_000), jitter = 0 (0/4 = 0)
    assert_eq!(d.as_millis(), 0);
}

#[test]
fn zero_max_returns_zero() {
    let d = backoff_delay(5, 1000, 0);
    // exp = large, capped at 0, jitter = 0 (0/4 = 0)
    assert_eq!(d.as_millis(), 0);
}

#[test]
fn attempt_0_base_equals_max() {
    let d = backoff_delay(0, 5000, 5000);
    // exp = 5000, capped at 5000, jitter = 0..1250
    assert!(d.as_millis() >= 5000);
    assert!(d.as_millis() < 5000 + 1250);
}
