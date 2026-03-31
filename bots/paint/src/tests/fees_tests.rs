use super::*;
use approx::assert_relative_eq;

/// Verifies that fee at fifty cents current params.
#[test]
fn fee_at_fifty_cents_current_params() {
    let fee = compute_taker_fee(0.50, 100.0, 0.25, 2);
    assert_relative_eq!(fee, 0.78125, epsilon = 0.0001);
}

/// Verifies that effective rate at fifty cents current.
#[test]
fn effective_rate_at_fifty_cents_current() {
    let rate = effective_fee_rate(0.50, 0.25, 2);
    assert_relative_eq!(rate, 0.015625, epsilon = 0.0001);
}

/// Verifies that effective rate at fifty cents new params.
#[test]
fn effective_rate_at_fifty_cents_new_params() {
    let rate = effective_fee_rate(0.50, 0.072, 1);
    assert_relative_eq!(rate, 0.018, epsilon = 0.0001);
}

/// Verifies that fee at extreme price near one.
#[test]
fn fee_at_extreme_price_near_one() {
    let fee = compute_taker_fee(0.95, 1000.0, 0.25, 2);
    assert_relative_eq!(fee, 0.535859, epsilon = 0.001);
}

/// Verifies that fee at extreme price near zero.
#[test]
fn fee_at_extreme_price_near_zero() {
    let fee = compute_taker_fee(0.05, 1000.0, 0.25, 2);

    assert_relative_eq!(fee, 0.028203, epsilon = 0.001);
}

/// Verifies that fee zero for invalid inputs.
#[test]
fn fee_zero_for_invalid_inputs() {
    assert_eq!(compute_taker_fee(0.0, 100.0, 0.25, 2), 0.0);
    assert_eq!(compute_taker_fee(1.0, 100.0, 0.25, 2), 0.0);
    assert_eq!(compute_taker_fee(0.50, 0.0, 0.25, 2), 0.0);
    assert_eq!(compute_taker_fee(0.50, 100.0, 0.0, 2), 0.0);
    assert_eq!(compute_taker_fee(-0.1, 100.0, 0.25, 2), 0.0);
}

/// Verifies that fee symmetry around fifty.
#[test]
fn fee_symmetry_around_fifty() {
    let fee_40 = compute_taker_fee(0.40, 100.0, 0.25, 2);
    let fee_60 = compute_taker_fee(0.60, 100.0, 0.25, 2);

    assert!(fee_40 > 0.0);
    assert!(fee_60 > 0.0);
    assert!((fee_40 - fee_60).abs() > 0.01);
}

/// Verifies that effective rate zero for boundary prices.
#[test]
fn effective_rate_zero_for_boundary_prices() {
    assert_eq!(effective_fee_rate(0.0, 0.25, 2), 0.0);
    assert_eq!(effective_fee_rate(1.0, 0.25, 2), 0.0);
}
