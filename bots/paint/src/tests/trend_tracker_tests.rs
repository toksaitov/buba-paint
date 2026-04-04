use super::*;
use crate::portfolio::StrategyFamily;

/// Verifies that bias zero with fewer than three outcomes.
#[test]
fn bias_zero_with_fewer_than_three_outcomes() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    assert!((tracker.get_trend_bias()).abs() < f64::EPSILON);

    tracker.record_outcome(SignalDirection::Up, true, 100);
    assert!((tracker.get_trend_bias()).abs() < f64::EPSILON);

    tracker.record_outcome(SignalDirection::Up, true, 200);
    assert!((tracker.get_trend_bias()).abs() < f64::EPSILON);
}

/// Verifies that all up wins positive bias.
#[test]
fn all_up_wins_positive_bias() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Up, true, i * 100);
    }

    let bias = tracker.get_trend_bias();
    assert!(bias > 0.0, "expected positive bias, got {bias}");
    assert!((bias - 0.5).abs() < f64::EPSILON);
}

/// Verifies that all down wins negative bias.
#[test]
fn all_down_wins_negative_bias() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Down, true, i * 100);
    }

    let bias = tracker.get_trend_bias();
    assert!(bias < 0.0, "expected negative bias, got {bias}");
    assert!((bias - (-0.5)).abs() < f64::EPSILON);
}

/// Verifies that mixed results correct bias.
#[test]
fn mixed_results_correct_bias() {
    let mut tracker = TrendTracker::new(10, true, 0.3);

    tracker.record_outcome(SignalDirection::Up, true, 100);
    tracker.record_outcome(SignalDirection::Up, true, 200);
    tracker.record_outcome(SignalDirection::Up, false, 300);

    tracker.record_outcome(SignalDirection::Down, true, 400);
    tracker.record_outcome(SignalDirection::Down, false, 500);
    tracker.record_outcome(SignalDirection::Down, false, 600);

    let bias = tracker.get_trend_bias();
    assert!((bias - 1.0 / 3.0).abs() < 1e-10);
}

/// Verifies that suppression when filter enabled and bias exceeds threshold.
#[test]
fn suppression_when_filter_enabled_and_bias_exceeds_threshold() {
    let mut tracker = TrendTracker::new(10, true, 0.3);

    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Up, true, i * 100);
    }

    assert!(tracker.should_suppress(SignalDirection::Down));

    assert!(!tracker.should_suppress(SignalDirection::Up));
}

/// Verifies that suppression up when down is winning.
#[test]
fn suppression_up_when_down_is_winning() {
    let mut tracker = TrendTracker::new(10, true, 0.3);

    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Down, true, i * 100);
    }

    assert!(tracker.should_suppress(SignalDirection::Up));

    assert!(!tracker.should_suppress(SignalDirection::Down));
}

/// Verifies that no suppression when filter disabled.
#[test]
fn no_suppression_when_filter_disabled() {
    let mut tracker = TrendTracker::new(10, false, 0.3);

    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Up, true, i * 100);
    }
    assert!(!tracker.should_suppress(SignalDirection::Down));
    assert!(!tracker.should_suppress(SignalDirection::Up));
}

/// Verifies that window size limits outcomes.
#[test]
fn window_size_limits_outcomes() {
    let mut tracker = TrendTracker::new(3, true, 0.3);

    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Up, true, i * 100);
    }
    assert_eq!(tracker.recent_outcomes.len(), 3);

    for i in 5..8 {
        tracker.record_outcome(SignalDirection::Down, true, i * 100);
    }
    assert_eq!(tracker.recent_outcomes.len(), 3);

    let bias = tracker.get_trend_bias();
    assert!((bias - (-0.5)).abs() < f64::EPSILON);
}

/// Verifies that no suppression when bias below threshold.
#[test]
fn no_suppression_when_bias_below_threshold() {
    let mut tracker = TrendTracker::new(10, true, 0.3);

    tracker.record_outcome(SignalDirection::Up, true, 100);
    tracker.record_outcome(SignalDirection::Up, true, 200);
    tracker.record_outcome(SignalDirection::Up, false, 300);
    tracker.record_outcome(SignalDirection::Down, true, 400);
    tracker.record_outcome(SignalDirection::Down, false, 500);

    let bias = tracker.get_trend_bias();
    assert!(bias.abs() < 0.3, "bias {bias} should be below threshold");
    assert!(!tracker.should_suppress(SignalDirection::Up));
    assert!(!tracker.should_suppress(SignalDirection::Down));
}

/// Verifies that equal performance zero bias.
#[test]
fn equal_performance_zero_bias() {
    let mut tracker = TrendTracker::new(10, true, 0.3);

    tracker.record_outcome(SignalDirection::Up, true, 100);
    tracker.record_outcome(SignalDirection::Up, true, 200);
    tracker.record_outcome(SignalDirection::Down, true, 300);
    tracker.record_outcome(SignalDirection::Down, true, 400);

    let bias = tracker.get_trend_bias();
    assert!((bias).abs() < f64::EPSILON);
}

/// Verifies that scoped tracking isolates suppression per strategy family.
#[test]
fn scoped_tracker_isolates_bias_by_strategy_family() {
    let mut tracker = ScopedTrendTracker::new(10, true, 0.3, true);

    for i in 0..5 {
        tracker.record_outcome(
            StrategyFamily::LatencyArb,
            SignalDirection::Up,
            true,
            i * 100,
        );
    }

    assert!(tracker.should_suppress(StrategyFamily::LatencyArb, SignalDirection::Down));
    assert!(!tracker.should_suppress(StrategyFamily::CalmPersistence, SignalDirection::Down));
}

/// Verifies that non-scoped tracking preserves the old shared suppression behavior.
#[test]
fn non_scoped_tracker_behaves_like_global_tracker() {
    let mut tracker = ScopedTrendTracker::new(10, true, 0.3, false);

    for i in 0..5 {
        tracker.record_outcome(
            StrategyFamily::LatencyArb,
            SignalDirection::Up,
            true,
            i * 100,
        );
    }

    assert!(tracker.should_suppress(StrategyFamily::CalmPersistence, SignalDirection::Down));
}
