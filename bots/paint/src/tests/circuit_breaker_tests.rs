use super::*;

/// Verifies that can trade before any losses.
#[test]
fn can_trade_before_any_losses() {
    let cb = CircuitBreaker::new(3, 10_000);
    assert!(cb.can_trade(0));
    assert!(cb.can_trade(1_000_000));
}

/// Verifies that win resets counter.
#[test]
fn win_resets_counter() {
    let mut cb = CircuitBreaker::new(3, 10_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    assert_eq!(cb.consecutive_losses(), 2);

    cb.record_result(true, 300);
    assert_eq!(cb.consecutive_losses(), 0);
    assert!(cb.can_trade(300));
}

/// Verifies that three consecutive losses triggers pause.
#[test]
fn three_consecutive_losses_triggers_pause() {
    let mut cb = CircuitBreaker::new(3, 10_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    assert!(cb.can_trade(200));

    cb.record_result(false, 300);
    assert!(cb.is_paused(300));
    assert!(!cb.can_trade(300));

    assert_eq!(cb.consecutive_losses(), 0);
}

/// Verifies that pause expires after duration.
#[test]
fn pause_expires_after_duration() {
    let mut cb = CircuitBreaker::new(3, 10_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    cb.record_result(false, 300);

    assert!(cb.is_paused(10_299));
    assert!(!cb.can_trade(10_299));

    assert!(cb.can_trade(10_300));
    assert!(!cb.is_paused(10_300));

    assert!(cb.can_trade(20_000));
}

/// Verifies that counter resets to zero after trigger.
#[test]
fn counter_resets_to_zero_after_trigger() {
    let mut cb = CircuitBreaker::new(2, 5_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    assert_eq!(cb.consecutive_losses(), 0);

    cb.record_result(false, 6_000);
    assert_eq!(cb.consecutive_losses(), 1);
    assert!(cb.can_trade(6_000));
}

/// Verifies that interleaved wins prevent trigger.
#[test]
fn interleaved_wins_prevent_trigger() {
    let mut cb = CircuitBreaker::new(3, 10_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    cb.record_result(true, 300);
    cb.record_result(false, 400);
    cb.record_result(false, 500);

    assert!(cb.can_trade(500));
    assert_eq!(cb.consecutive_losses(), 2);
}

/// Verifies that multiple triggers.
#[test]
fn multiple_triggers() {
    let mut cb = CircuitBreaker::new(2, 1_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    assert!(cb.is_paused(200));
    assert!(cb.can_trade(1_200));

    cb.record_result(false, 2_000);
    cb.record_result(false, 2_100);
    assert!(cb.is_paused(2_100));
    assert!(cb.can_trade(3_100));
}

/// Verifies that max losses one.
#[test]
fn max_losses_one() {
    let mut cb = CircuitBreaker::new(1, 500);
    cb.record_result(false, 100);
    assert!(cb.is_paused(100));
    assert!(cb.can_trade(600));
}

/// Verifies that losses exactly at max triggers pause.
#[test]
fn losses_exactly_at_max_triggers_pause() {
    let mut cb = CircuitBreaker::new(3, 10_000);

    cb.record_result(false, 100);
    assert!(cb.can_trade(100), "1 loss should not trigger pause");
    cb.record_result(false, 200);
    assert!(cb.can_trade(200), "2 losses should not trigger pause");
    cb.record_result(false, 300);
    assert!(
        !cb.can_trade(300),
        "3 consecutive losses should trigger pause"
    );
    assert!(cb.is_paused(300));

    assert_eq!(cb.consecutive_losses(), 0);

    cb.record_result(false, 400);
    assert!(
        !cb.can_trade(400),
        "should still be paused during pause window even after 4th loss"
    );
    assert_eq!(cb.consecutive_losses(), 1, "4th loss starts a new streak");
}

/// Verifies that log if paused updates timestamp.
#[test]
fn log_if_paused_updates_timestamp() {
    let mut cb = CircuitBreaker::new(3, 900_000);

    cb.record_result(false, 1_000);
    cb.record_result(false, 2_000);
    cb.record_result(false, 3_000);
    cb.log_if_paused(4_000);
    assert_eq!(cb.last_paused_log_ms, 4_000);
}

/// Verifies that log if paused rate limited.
#[test]
fn log_if_paused_rate_limited() {
    let mut cb = CircuitBreaker::new(3, 900_000);
    cb.record_result(false, 1_000);
    cb.record_result(false, 2_000);
    cb.record_result(false, 3_000);
    cb.log_if_paused(4_000);
    assert_eq!(cb.last_paused_log_ms, 4_000);

    cb.log_if_paused(30_000);
    assert_eq!(cb.last_paused_log_ms, 4_000);

    cb.log_if_paused(65_000);
    assert_eq!(cb.last_paused_log_ms, 65_000);
}

/// Verifies that log if paused no op when not paused.
#[test]
fn log_if_paused_no_op_when_not_paused() {
    let mut cb = CircuitBreaker::new(3, 900_000);

    cb.log_if_paused(1_000);
    assert_eq!(cb.last_paused_log_ms, 0);
}
