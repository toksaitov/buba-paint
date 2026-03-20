use super::*;

#[test]
fn can_trade_before_any_losses() {
    let cb = CircuitBreaker::new(3, 10_000);
    assert!(cb.can_trade(0));
    assert!(cb.can_trade(1_000_000));
}

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

#[test]
fn three_consecutive_losses_triggers_pause() {
    let mut cb = CircuitBreaker::new(3, 10_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    assert!(cb.can_trade(200));

    cb.record_result(false, 300); // 3rd loss → trigger
    assert!(cb.is_paused(300));
    assert!(!cb.can_trade(300));
    // Counter resets to 0 after trigger.
    assert_eq!(cb.consecutive_losses(), 0);
}

#[test]
fn pause_expires_after_duration() {
    let mut cb = CircuitBreaker::new(3, 10_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    cb.record_result(false, 300); // triggers, pause_until = 10_300

    // Still paused just before expiry.
    assert!(cb.is_paused(10_299));
    assert!(!cb.can_trade(10_299));

    // Exactly at expiry → can trade again.
    assert!(cb.can_trade(10_300));
    assert!(!cb.is_paused(10_300));

    // Well past expiry.
    assert!(cb.can_trade(20_000));
}

#[test]
fn counter_resets_to_zero_after_trigger() {
    let mut cb = CircuitBreaker::new(2, 5_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200); // triggers
    assert_eq!(cb.consecutive_losses(), 0);

    // After pause expires, a single loss should not trigger again.
    cb.record_result(false, 6_000);
    assert_eq!(cb.consecutive_losses(), 1);
    assert!(cb.can_trade(6_000));
}

#[test]
fn interleaved_wins_prevent_trigger() {
    let mut cb = CircuitBreaker::new(3, 10_000);

    cb.record_result(false, 100);
    cb.record_result(false, 200);
    cb.record_result(true, 300); // resets
    cb.record_result(false, 400);
    cb.record_result(false, 500);
    // Only 2 consecutive losses at this point — no trigger.
    assert!(cb.can_trade(500));
    assert_eq!(cb.consecutive_losses(), 2);
}

#[test]
fn multiple_triggers() {
    let mut cb = CircuitBreaker::new(2, 1_000);

    // First trigger.
    cb.record_result(false, 100);
    cb.record_result(false, 200);
    assert!(cb.is_paused(200));
    assert!(cb.can_trade(1_200));

    // Second trigger after pause expires.
    cb.record_result(false, 2_000);
    cb.record_result(false, 2_100);
    assert!(cb.is_paused(2_100));
    assert!(cb.can_trade(3_100));
}

#[test]
fn max_losses_one() {
    let mut cb = CircuitBreaker::new(1, 500);
    cb.record_result(false, 100);
    assert!(cb.is_paused(100));
    assert!(cb.can_trade(600));
}

// -- Boundary-condition tests ---------------------------------------------

#[test]
fn losses_exactly_at_max_triggers_pause() {
    // With max_losses=3, exactly 3 consecutive losses should trigger the pause.
    let mut cb = CircuitBreaker::new(3, 10_000);

    // Record exactly 3 consecutive losses.
    cb.record_result(false, 100);
    assert!(cb.can_trade(100), "1 loss should not trigger pause");
    cb.record_result(false, 200);
    assert!(cb.can_trade(200), "2 losses should not trigger pause");
    cb.record_result(false, 300); // 3rd loss → triggers (>= max_losses)
    assert!(
        !cb.can_trade(300),
        "3 consecutive losses should trigger pause"
    );
    assert!(cb.is_paused(300));

    // Counter resets to 0 after trigger.
    assert_eq!(cb.consecutive_losses(), 0);

    // Record a 4th loss — this starts a new streak (counter = 1), but
    // we're still within the pause window so can_trade remains false.
    cb.record_result(false, 400);
    assert!(
        !cb.can_trade(400),
        "should still be paused during pause window even after 4th loss"
    );
    assert_eq!(cb.consecutive_losses(), 1, "4th loss starts a new streak");
}

// -- Phase 3: log_if_paused rate-limiting tests ---------------------------

#[test]
fn log_if_paused_updates_timestamp() {
    let mut cb = CircuitBreaker::new(3, 900_000);
    // Trigger the breaker
    cb.record_result(false, 1_000);
    cb.record_result(false, 2_000);
    cb.record_result(false, 3_000);
    cb.log_if_paused(4_000);
    assert_eq!(cb.last_paused_log_ms, 4_000);
}

#[test]
fn log_if_paused_rate_limited() {
    let mut cb = CircuitBreaker::new(3, 900_000);
    cb.record_result(false, 1_000);
    cb.record_result(false, 2_000);
    cb.record_result(false, 3_000);
    cb.log_if_paused(4_000);
    assert_eq!(cb.last_paused_log_ms, 4_000);

    cb.log_if_paused(30_000);
    assert_eq!(cb.last_paused_log_ms, 4_000); // unchanged

    cb.log_if_paused(65_000);
    assert_eq!(cb.last_paused_log_ms, 65_000); // updated
}

#[test]
fn log_if_paused_no_op_when_not_paused() {
    let mut cb = CircuitBreaker::new(3, 900_000);
    // Not paused — should not update.
    cb.log_if_paused(1_000);
    assert_eq!(cb.last_paused_log_ms, 0);
}
