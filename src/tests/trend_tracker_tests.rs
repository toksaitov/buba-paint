use super::*;

#[test]
fn bias_zero_with_fewer_than_three_outcomes() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    assert!((tracker.get_trend_bias()).abs() < f64::EPSILON);

    tracker.record_outcome(SignalDirection::Up, true, 100);
    assert!((tracker.get_trend_bias()).abs() < f64::EPSILON);

    tracker.record_outcome(SignalDirection::Up, true, 200);
    assert!((tracker.get_trend_bias()).abs() < f64::EPSILON);
}

#[test]
fn all_up_wins_positive_bias() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Up, true, i * 100);
    }
    // up_rate = 1.0, down_rate = 0.5 (default) → bias = 0.5
    let bias = tracker.get_trend_bias();
    assert!(bias > 0.0, "expected positive bias, got {bias}");
    assert!((bias - 0.5).abs() < f64::EPSILON);
}

#[test]
fn all_down_wins_negative_bias() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Down, true, i * 100);
    }
    // up_rate = 0.5 (default), down_rate = 1.0 → bias = -0.5
    let bias = tracker.get_trend_bias();
    assert!(bias < 0.0, "expected negative bias, got {bias}");
    assert!((bias - (-0.5)).abs() < f64::EPSILON);
}

#[test]
fn mixed_results_correct_bias() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    // 2 UP wins, 1 UP loss → up_rate = 2/3
    tracker.record_outcome(SignalDirection::Up, true, 100);
    tracker.record_outcome(SignalDirection::Up, true, 200);
    tracker.record_outcome(SignalDirection::Up, false, 300);
    // 1 DOWN win, 2 DOWN losses → down_rate = 1/3
    tracker.record_outcome(SignalDirection::Down, true, 400);
    tracker.record_outcome(SignalDirection::Down, false, 500);
    tracker.record_outcome(SignalDirection::Down, false, 600);

    // bias = 2/3 - 1/3 = 1/3 ≈ 0.333
    let bias = tracker.get_trend_bias();
    assert!((bias - 1.0 / 3.0).abs() < 1e-10);
}

#[test]
fn suppression_when_filter_enabled_and_bias_exceeds_threshold() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    // All UP wins → bias = 0.5, which exceeds threshold 0.3
    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Up, true, i * 100);
    }
    // DOWN should be suppressed (bias > +threshold).
    assert!(tracker.should_suppress(SignalDirection::Down));
    // UP should NOT be suppressed.
    assert!(!tracker.should_suppress(SignalDirection::Up));
}

#[test]
fn suppression_up_when_down_is_winning() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    // All DOWN wins → bias = -0.5
    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Down, true, i * 100);
    }
    // UP should be suppressed (bias < -threshold).
    assert!(tracker.should_suppress(SignalDirection::Up));
    // DOWN should NOT be suppressed.
    assert!(!tracker.should_suppress(SignalDirection::Down));
}

#[test]
fn no_suppression_when_filter_disabled() {
    let mut tracker = TrendTracker::new(10, false, 0.3);
    // All UP wins → bias would be 0.5, but filter is off.
    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Up, true, i * 100);
    }
    assert!(!tracker.should_suppress(SignalDirection::Down));
    assert!(!tracker.should_suppress(SignalDirection::Up));
}

#[test]
fn window_size_limits_outcomes() {
    let mut tracker = TrendTracker::new(3, true, 0.3);
    // Record 5 UP wins — only last 3 should be kept.
    for i in 0..5 {
        tracker.record_outcome(SignalDirection::Up, true, i * 100);
    }
    assert_eq!(tracker.recent_outcomes.len(), 3);

    // Now record 3 DOWN wins — pushes out all UP outcomes.
    for i in 5..8 {
        tracker.record_outcome(SignalDirection::Down, true, i * 100);
    }
    assert_eq!(tracker.recent_outcomes.len(), 3);
    // bias should be -0.5 now (no UP outcomes, down_rate = 1.0, up_rate = 0.5 default)
    let bias = tracker.get_trend_bias();
    assert!((bias - (-0.5)).abs() < f64::EPSILON);
}

#[test]
fn no_suppression_when_bias_below_threshold() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    // 2 UP wins, 1 UP loss, 1 DOWN win, 1 DOWN loss
    // up_rate = 2/3, down_rate = 1/2 → bias = 2/3 - 1/2 = 1/6 ≈ 0.167
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

#[test]
fn equal_performance_zero_bias() {
    let mut tracker = TrendTracker::new(10, true, 0.3);
    // 2 UP wins, 2 DOWN wins → up_rate = 1.0, down_rate = 1.0 → bias = 0
    tracker.record_outcome(SignalDirection::Up, true, 100);
    tracker.record_outcome(SignalDirection::Up, true, 200);
    tracker.record_outcome(SignalDirection::Down, true, 300);
    tracker.record_outcome(SignalDirection::Down, true, 400);

    let bias = tracker.get_trend_bias();
    assert!((bias).abs() < f64::EPSILON);
}
