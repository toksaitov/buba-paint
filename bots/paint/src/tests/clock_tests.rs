use super::*;

#[test]
fn system_clock_returns_plausible_timestamp() {
    let clock = SystemClock;
    let ts = clock.now();
    // 1_700_000_000_000 ms ≈ 2023-11-14
    assert!(
        ts > 1_700_000_000_000,
        "SystemClock returned {ts}, which is before Nov 2023"
    );
}

#[test]
fn backtest_clock_starts_at_zero() {
    let clock = BacktestClock::new();
    assert_eq!(clock.now(), 0);
}

#[test]
fn backtest_clock_returns_what_was_set() {
    let clock = BacktestClock::new();

    clock.set(1_700_000_000_000);
    assert_eq!(clock.now(), 1_700_000_000_000);

    clock.set(1_700_000_060_000);
    assert_eq!(clock.now(), 1_700_000_060_000);
}

#[test]
fn backtest_clock_default_starts_at_zero() {
    let clock = BacktestClock::default();
    assert_eq!(clock.now(), 0);
}

#[test]
fn backtest_clock_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BacktestClock>();
}

#[test]
fn system_clock_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SystemClock>();
}
