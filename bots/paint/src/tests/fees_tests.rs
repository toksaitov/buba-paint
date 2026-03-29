use super::*;
use approx::assert_relative_eq;

// Current crypto params: fee_rate=0.25, exponent=2
// New crypto params (Mar 30+): fee_rate=0.072, exponent=1

#[test]
fn fee_at_fifty_cents_current_params() {
    // At price=0.50: variance = 0.25, variance^2 = 0.0625
    // fee_per_share = 0.50 * 0.25 * 0.0625 = 0.0078125
    // For 100 shares: 0.78125
    let fee = compute_taker_fee(0.50, 100.0, 0.25, 2);
    assert_relative_eq!(fee, 0.78125, epsilon = 0.0001);
}

#[test]
fn effective_rate_at_fifty_cents_current() {
    // 0.25 * (0.25)^2 = 0.25 * 0.0625 = 0.015625 = 1.5625%
    let rate = effective_fee_rate(0.50, 0.25, 2);
    assert_relative_eq!(rate, 0.015625, epsilon = 0.0001);
}

#[test]
fn effective_rate_at_fifty_cents_new_params() {
    // 0.072 * (0.25)^1 = 0.072 * 0.25 = 0.018 = 1.8%
    let rate = effective_fee_rate(0.50, 0.072, 1);
    assert_relative_eq!(rate, 0.018, epsilon = 0.0001);
}

#[test]
fn fee_at_extreme_price_near_one() {
    // At price=0.95: variance = 0.0475, variance^2 = 0.00225625
    // fee_per_share = 0.95 * 0.25 * 0.00225625 = 0.000535859
    let fee = compute_taker_fee(0.95, 1000.0, 0.25, 2);
    assert_relative_eq!(fee, 0.535859, epsilon = 0.001);
}

#[test]
fn fee_at_extreme_price_near_zero() {
    // At price=0.05: variance = 0.0475, same as 0.95 (symmetric)
    let fee = compute_taker_fee(0.05, 1000.0, 0.25, 2);
    // fee_per_share = 0.05 * 0.25 * 0.00225625 = 0.00002820
    assert_relative_eq!(fee, 0.028203, epsilon = 0.001);
}

#[test]
fn fee_zero_for_invalid_inputs() {
    assert_eq!(compute_taker_fee(0.0, 100.0, 0.25, 2), 0.0);
    assert_eq!(compute_taker_fee(1.0, 100.0, 0.25, 2), 0.0);
    assert_eq!(compute_taker_fee(0.50, 0.0, 0.25, 2), 0.0);
    assert_eq!(compute_taker_fee(0.50, 100.0, 0.0, 2), 0.0);
    assert_eq!(compute_taker_fee(-0.1, 100.0, 0.25, 2), 0.0);
}

#[test]
fn fee_symmetry_around_fifty() {
    // The variance term p*(1-p) is symmetric, but the full formula includes p,
    // so fee at 0.40 != fee at 0.60.
    let fee_40 = compute_taker_fee(0.40, 100.0, 0.25, 2);
    let fee_60 = compute_taker_fee(0.60, 100.0, 0.25, 2);
    // Both should be positive but not equal.
    assert!(fee_40 > 0.0);
    assert!(fee_60 > 0.0);
    assert!((fee_40 - fee_60).abs() > 0.01);
}

#[test]
fn effective_rate_zero_for_boundary_prices() {
    assert_eq!(effective_fee_rate(0.0, 0.25, 2), 0.0);
    assert_eq!(effective_fee_rate(1.0, 0.25, 2), 0.0);
}
