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

/// Verifies that market fee schedules override legacy fee-profile defaults.
#[test]
fn resolve_fee_params_prefers_market_fee_schedule() {
    let market = crate::types::MarketWindow {
        market_id: "mkt-1".to_string(),
        question: "Will BTC go up?".to_string(),
        up_token_id: "tok-up".to_string(),
        down_token_id: "tok-down".to_string(),
        condition_id: "cond-1".to_string(),
        start_time: 1_000,
        end_time: 2_000,
        slug: "btc-updown-5m-1".to_string(),
        outcome: None,
        resolution_source: Some("gamma".to_string()),
        fee_profile: Some("crypto".to_string()),
        order_min_size: Some(5.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: None,
        taker_base_fee: None,
        rewards_min_size: None,
        rewards_max_spread: None,
        fees_enabled: Some(true),
        fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".to_string()),
        token_fee_rates_json: Some("{\"tok-up\":{\"base_fee\":1000}}".to_string()),
        accepting_orders: Some(true),
        accepting_orders_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
        clear_book_on_start: Some(false),
    };

    let params = resolve_fee_params(&crate::config::Config::default(), Some(&market), 1_700_000);
    assert_relative_eq!(params.fee_rate, 0.072, epsilon = 0.0001);
    assert_eq!(params.exponent, 1);
}
