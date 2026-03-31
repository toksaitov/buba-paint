use super::*;

/// Verifies that empty returns zero.
#[test]
fn empty_returns_zero() {
    let calc = MomentumCalculator::new(1_000);
    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

/// Verifies that single point returns zero.
#[test]
fn single_point_returns_zero() {
    let mut calc = MomentumCalculator::new(1_000);
    calc.push(100.0, 1_000);
    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

/// Verifies that two points returns relative change.
#[test]
fn two_points_returns_relative_change() {
    let mut calc = MomentumCalculator::new(5_000);
    calc.push(100.0, 1_000);
    calc.push(105.0, 2_000);

    assert!((calc.get() - 0.05).abs() < 1e-12);
}

/// Verifies that negative momentum.
#[test]
fn negative_momentum() {
    let mut calc = MomentumCalculator::new(5_000);
    calc.push(100.0, 1_000);
    calc.push(95.0, 2_000);

    assert!((calc.get() - (-0.05)).abs() < 1e-12);
}

/// Verifies that window pruning.
#[test]
fn window_pruning() {
    let mut calc = MomentumCalculator::new(1_000);
    calc.push(100.0, 1_000);
    calc.push(110.0, 1_500);

    assert!((calc.get() - 0.10).abs() < 1e-12);

    calc.push(115.0, 2_001);

    let expected = (115.0 - 110.0) / 110.0;
    assert!((calc.get() - expected).abs() < 1e-12);
}

/// Verifies that window boundary exact.
#[test]
fn window_boundary_exact() {
    let mut calc = MomentumCalculator::new(1_000);
    calc.push(100.0, 1_000);
    calc.push(110.0, 2_000);

    assert!((calc.get() - 0.10).abs() < 1e-12);
}

/// Verifies that reset clears all.
#[test]
fn reset_clears_all() {
    let mut calc = MomentumCalculator::new(5_000);
    calc.push(100.0, 1_000);
    calc.push(110.0, 2_000);
    assert!((calc.get() - 0.10).abs() < 1e-12);

    calc.reset();
    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

/// Verifies that multiple pushes maintain order.
#[test]
fn multiple_pushes_maintain_order() {
    let mut calc = MomentumCalculator::new(10_000);
    calc.push(100.0, 1_000);
    calc.push(102.0, 2_000);
    calc.push(104.0, 3_000);
    calc.push(106.0, 4_000);

    assert!((calc.get() - 0.06).abs() < 1e-12);
}

/// Verifies that all pruned except one returns zero.
#[test]
fn all_pruned_except_one_returns_zero() {
    let mut calc = MomentumCalculator::new(100);
    calc.push(100.0, 1_000);
    calc.push(110.0, 1_050);

    calc.push(120.0, 5_000);

    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

/// Verifies that zero window keeps only latest.
#[test]
fn zero_window_keeps_only_latest() {
    let mut calc = MomentumCalculator::new(0);
    calc.push(100.0, 1_000);
    calc.push(110.0, 1_001);

    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

/// Verifies that zero oldest price returns zero.
#[test]
fn zero_oldest_price_returns_zero() {
    let mut calc = MomentumCalculator::new(5_000);
    calc.push(0.0, 1_000);
    calc.push(42_000.0, 2_000);
    let result = calc.get();
    assert!(
        (result - 0.0).abs() < f64::EPSILON,
        "zero oldest price should return 0.0, got {result}"
    );
    assert!(
        !result.is_nan() && !result.is_infinite(),
        "result should not be NaN or Inf"
    );
}

/// Verifies that negative price returns zero.
#[test]
fn negative_price_returns_zero() {
    let mut calc = MomentumCalculator::new(5_000);
    calc.push(-1.0, 1_000);
    calc.push(42_000.0, 2_000);
    let result = calc.get();
    assert!(
        (result - 0.0).abs() < f64::EPSILON,
        "negative oldest price should return 0.0, got {result}"
    );
    assert!(
        !result.is_nan() && !result.is_infinite(),
        "result should not be NaN or Inf"
    );
}
