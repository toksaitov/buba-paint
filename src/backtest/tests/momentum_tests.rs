use super::*;

#[test]
fn empty_returns_zero() {
    let calc = MomentumCalculator::new(1_000);
    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn single_point_returns_zero() {
    let mut calc = MomentumCalculator::new(1_000);
    calc.push(100.0, 1_000);
    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn two_points_returns_relative_change() {
    let mut calc = MomentumCalculator::new(5_000);
    calc.push(100.0, 1_000);
    calc.push(105.0, 2_000);
    // (105 - 100) / 100 = 0.05
    assert!((calc.get() - 0.05).abs() < 1e-12);
}

#[test]
fn negative_momentum() {
    let mut calc = MomentumCalculator::new(5_000);
    calc.push(100.0, 1_000);
    calc.push(95.0, 2_000);
    // (95 - 100) / 100 = -0.05
    assert!((calc.get() - (-0.05)).abs() < 1e-12);
}

#[test]
fn window_pruning() {
    let mut calc = MomentumCalculator::new(1_000);
    calc.push(100.0, 1_000);
    calc.push(110.0, 1_500);
    // Both within window: (110 - 100) / 100 = 0.10
    assert!((calc.get() - 0.10).abs() < 1e-12);

    // Push a point that causes the first to be pruned (timestamp 2001
    // means cutoff = 2001 - 1000 = 1001, so point at 1000 is < 1001).
    calc.push(115.0, 2_001);
    // Now oldest is 110.0 at 1500: (115 - 110) / 110
    let expected = (115.0 - 110.0) / 110.0;
    assert!((calc.get() - expected).abs() < 1e-12);
}

#[test]
fn window_boundary_exact() {
    let mut calc = MomentumCalculator::new(1_000);
    calc.push(100.0, 1_000);
    calc.push(110.0, 2_000);
    // cutoff = 2000 - 1000 = 1000, point at 1000 is NOT < 1000, so kept.
    assert!((calc.get() - 0.10).abs() < 1e-12);
}

#[test]
fn reset_clears_all() {
    let mut calc = MomentumCalculator::new(5_000);
    calc.push(100.0, 1_000);
    calc.push(110.0, 2_000);
    assert!((calc.get() - 0.10).abs() < 1e-12);

    calc.reset();
    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn multiple_pushes_maintain_order() {
    let mut calc = MomentumCalculator::new(10_000);
    calc.push(100.0, 1_000);
    calc.push(102.0, 2_000);
    calc.push(104.0, 3_000);
    calc.push(106.0, 4_000);
    // (106 - 100) / 100 = 0.06
    assert!((calc.get() - 0.06).abs() < 1e-12);
}

#[test]
fn all_pruned_except_one_returns_zero() {
    let mut calc = MomentumCalculator::new(100);
    calc.push(100.0, 1_000);
    calc.push(110.0, 1_050);
    // Jump far ahead so both old points are pruned.
    calc.push(120.0, 5_000);
    // Only one point left -> 0.
    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn zero_window_keeps_only_latest() {
    let mut calc = MomentumCalculator::new(0);
    calc.push(100.0, 1_000);
    calc.push(110.0, 1_001);
    // cutoff = 1001 - 0 = 1001, point at 1000 < 1001 -> pruned, only 1 left.
    assert!((calc.get() - 0.0).abs() < f64::EPSILON);
}

// -- Boundary-condition tests ---------------------------------------------

#[test]
fn zero_oldest_price_returns_zero() {
    // The guard `oldest.price <= 0.0` should prevent division by zero.
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

#[test]
fn negative_price_returns_zero() {
    // The guard `oldest.price <= 0.0` should catch negative prices too.
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
