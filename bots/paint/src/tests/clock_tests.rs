use super::*;

/// Verifies that system clock returns plausible timestamp.
#[test]
fn system_clock_returns_plausible_timestamp() {
    let clock = SystemClock;
    let ts = clock.now();

    assert!(
        ts > 1_700_000_000_000,
        "SystemClock returned {ts}, which is before Nov 2023"
    );
}

/// Verifies that backtest clock starts at zero.
#[test]
fn backtest_clock_starts_at_zero() {
    let clock = BacktestClock::new();
    assert_eq!(clock.now(), 0);
}

/// Verifies that backtest clock returns what was set.
#[test]
fn backtest_clock_returns_what_was_set() {
    let clock = BacktestClock::new();

    clock.set(1_700_000_000_000);
    assert_eq!(clock.now(), 1_700_000_000_000);

    clock.set(1_700_000_060_000);
    assert_eq!(clock.now(), 1_700_000_060_000);
}

/// Verifies that backtest clock default starts at zero.
#[test]
fn backtest_clock_default_starts_at_zero() {
    let clock = BacktestClock::default();
    assert_eq!(clock.now(), 0);
}

/// Verifies that backtest clock is send and sync.
#[test]
fn backtest_clock_is_send_and_sync() {
    /// Assert send sync.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BacktestClock>();
}

/// Verifies that system clock is send and sync.
#[test]
fn system_clock_is_send_and_sync() {
    /// Assert send sync.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SystemClock>();
}
