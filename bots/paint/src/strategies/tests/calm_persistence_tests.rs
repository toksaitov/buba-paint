use super::*;
use crate::signal_features::SignalFeatureSnapshot;
use crate::types::{BookState, StrategyRejectionReason, TopOfBook};

/// Build a default test config for calm-persistence.
fn test_config() -> Config {
    let mut config = Config::default();
    config.calm_persistence_enabled = true;
    config.calm_persistence_max_window_time_ms = 90_000;
    config.calm_persistence_min_window_time_ms = 30_000;
    config.calm_persistence_max_ask = 0.65;
    config.calm_persistence_min_abs_distance_bps = 5.0;
    config.calm_persistence_distance_vol_ratio_threshold = 2.0;
    config.calm_persistence_max_realized_vol_15s_bps = 35.0;
    config.calm_persistence_max_open_crosses_30s = 1;
    config.calm_persistence_max_quote_churn_per_s = 20.0;
    config.calm_persistence_min_alignment_fraction = 0.60;
    config.calm_persistence_max_fair_bias = 0.18;
    config
}

/// Build a symmetric binary book.
fn book(up_ask: f64, down_ask: f64) -> BookState {
    BookState {
        up: Some(TopOfBook {
            best_bid: (up_ask - 0.02).max(0.0),
            best_ask: up_ask,
            bid_size: 100.0,
            ask_size: 100.0,
            timestamp: 100_000,
            observed_at_ms: 100_000,
        }),
        down: Some(TopOfBook {
            best_bid: (down_ask - 0.02).max(0.0),
            best_ask: down_ask,
            bid_size: 100.0,
            ask_size: 100.0,
            timestamp: 100_000,
            observed_at_ms: 100_000,
        }),
    }
}

/// Build a default calm-like feature snapshot.
fn calm_features() -> SignalFeatureSnapshot {
    SignalFeatureSnapshot {
        distance_from_open_bps: Some(12.0),
        realized_vol_15s_bps: Some(3.0),
        open_crosses_30s: Some(0),
        quote_age_ms: Some(2),
        book_staleness_ms: Some(2),
        polymarket_quote_churn_per_s: Some(5.0),
        return_250ms: Some(0.0004),
        return_500ms: Some(0.0006),
        return_1000ms: Some(0.0008),
        binance_signed_trade_imbalance: Some(0.30),
        binance_book_imbalance: Some(0.20),
        polymarket_microprice_skew: Some(0.03),
        expected_up_fee: Some(0.01),
        expected_down_fee: Some(0.01),
        expected_up_slippage: Some(0.01),
        expected_down_slippage: Some(0.01),
        ..SignalFeatureSnapshot::default()
    }
}

/// Build a strategy context around a provided book and features.
fn ctx_with(
    binance_price: f64,
    window_open_price: f64,
    book_state: BookState,
    features: SignalFeatureSnapshot,
) -> StrategyContext {
    StrategyContext {
        binance_price,
        binance_momentum: 0.0005,
        chainlink_price: Some(binance_price),
        book_state,
        window_open_price: Some(window_open_price),
        window_time_remaining_ms: 60_000,
        now_us: Some(1_000_000_000),
        features,
    }
}

/// Verify that calm-persistence emits a signal for a low-vol, low-cross, aligned late window.
#[test]
fn signal_fires_for_low_vol_late_window_persistence_case() {
    let config = test_config();
    let mut strat = CalmPersistenceStrategy::new();
    let ctx = ctx_with(100.12, 100.0, book(0.55, 0.52), calm_features());

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Single(signal) => {
            assert_eq!(signal.strategy, "calm-persistence");
            assert_eq!(signal.direction, SignalDirection::Up);
            assert!(signal.expected_edge.is_some_and(|edge| edge > 0.0));
            assert_eq!(signal.metadata["openCrosses30s"].as_u64(), Some(0));
        }
        other => panic!("expected Single signal, got {other:?}"),
    }
}

/// Verify that high realized volatility blocks the strategy.
#[test]
fn rejects_when_realized_volatility_is_too_high() {
    let mut features = calm_features();
    features.realized_vol_15s_bps = Some(60.0);
    let config = test_config();
    let mut strat = CalmPersistenceStrategy::new();
    let ctx = ctx_with(100.12, 100.0, book(0.55, 0.52), features);

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Rejected(rejection) => {
            assert_eq!(rejection.reason, StrategyRejectionReason::VolatilityTooHigh);
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

/// Verify that too many recent open-crosses block the strategy.
#[test]
fn rejects_when_open_crosses_are_too_high() {
    let mut features = calm_features();
    features.open_crosses_30s = Some(3);
    let config = test_config();
    let mut strat = CalmPersistenceStrategy::new();
    let ctx = ctx_with(100.12, 100.0, book(0.55, 0.52), features);

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Rejected(rejection) => {
            assert_eq!(
                rejection.reason,
                StrategyRejectionReason::OpenCrossesTooHigh
            );
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

/// Verify that positive alignment alone is not enough when the expected edge is still non-positive.
#[test]
fn rejects_when_expected_edge_is_non_positive_after_costs() {
    let mut features = calm_features();
    features.expected_up_fee = Some(0.05);
    features.expected_up_slippage = Some(0.05);
    let config = test_config();
    let mut strat = CalmPersistenceStrategy::new();
    let ctx = ctx_with(100.12, 100.0, book(0.64, 0.52), features);

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Rejected(rejection) => {
            assert_eq!(
                rejection.reason,
                StrategyRejectionReason::ExpectedEdgeNonPositive
            );
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

/// Verify that the configurable max-ask guard blocks expensive entries.
#[test]
fn rejects_when_entry_ask_is_above_max() {
    let config = test_config();
    let mut strat = CalmPersistenceStrategy::new();
    let ctx = ctx_with(100.12, 100.0, book(0.70, 0.52), calm_features());

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Rejected(rejection) => {
            assert_eq!(rejection.reason, StrategyRejectionReason::EntryAskAboveMax);
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

/// Verify that calm-specific rejection diagnostics include distance and ratio context.
#[test]
fn distance_rejection_records_calm_specific_diagnostics() {
    let mut features = calm_features();
    features.distance_from_open_bps = Some(4.0);
    features.realized_vol_15s_bps = Some(5.0);
    let config = test_config();
    let mut strat = CalmPersistenceStrategy::new();
    let ctx = ctx_with(100.04, 100.0, book(0.55, 0.52), features);

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Rejected(rejection) => {
            assert_eq!(
                rejection.reason,
                StrategyRejectionReason::DistanceBelowThreshold
            );
            assert_eq!(rejection.sample.distance_from_open_bps, Some(4.0));
            assert_eq!(rejection.sample.realized_vol_15s_bps, Some(5.0));
            assert_eq!(rejection.sample.open_crosses_30s, Some(0));
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

/// Verify that orderflow rejections retain the computed alignment and distance ratio.
#[test]
fn orderflow_rejection_records_alignment_context() {
    let mut features = calm_features();
    features.return_250ms = Some(-0.0004);
    features.return_500ms = Some(-0.0005);
    features.return_1000ms = Some(0.0001);
    features.binance_signed_trade_imbalance = Some(-0.25);
    features.binance_book_imbalance = Some(-0.10);
    features.polymarket_microprice_skew = Some(-0.02);
    let config = test_config();
    let mut strat = CalmPersistenceStrategy::new();
    let ctx = ctx_with(100.12, 100.0, book(0.55, 0.52), features);

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Rejected(rejection) => {
            assert_eq!(
                rejection.reason,
                StrategyRejectionReason::OrderflowNotAligned
            );
            assert!(
                rejection
                    .sample
                    .distance_vol_ratio
                    .is_some_and(|value| value > 0.0)
            );
            assert!(
                rejection
                    .sample
                    .alignment_fraction
                    .is_some_and(|value| value < 0.60)
            );
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}
