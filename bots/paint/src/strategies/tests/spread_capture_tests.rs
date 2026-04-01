use super::*;
use crate::signal_features::SignalFeatureSnapshot;
use crate::types::{BookState, StrategyRejectionReason, TopOfBook};

/// Test config.
fn test_config() -> Config {
    Config::default()
}

/// Builds a symmetric top-of-book snapshot for spread-capture tests.
fn book(up_bid: f64, up_ask: f64, down_bid: f64, down_ask: f64) -> BookState {
    BookState {
        up: Some(TopOfBook {
            best_bid: up_bid,
            best_ask: up_ask,
            bid_size: 100.0,
            ask_size: 100.0,
            timestamp: 0,
            observed_at_ms: 0,
        }),
        down: Some(TopOfBook {
            best_bid: down_bid,
            best_ask: down_ask,
            bid_size: 100.0,
            ask_size: 100.0,
            timestamp: 0,
            observed_at_ms: 0,
        }),
    }
}

/// Ctx with book.
fn ctx_with_book(book_state: BookState) -> StrategyContext {
    StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.0,
        chainlink_price: Some(41_999.0),
        book_state,
        window_open_price: Some(41_900.0),
        window_time_remaining_ms: 120_000,
        now_us: Some(1_000_000_000),
        features: SignalFeatureSnapshot::default(),
    }
}

/// Verifies that one spread-capture result is the expected explicit rejection.
fn assert_rejected(result: StrategyResult, reason: StrategyRejectionReason) {
    match result {
        StrategyResult::Rejected(rejection) => assert_eq!(rejection.reason, reason),
        other => panic!("expected Rejected({reason}), got {other:?}"),
    }
}

/// Verifies that no signal when total ask above threshold.
#[test]
fn no_signal_when_total_ask_above_threshold() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.45, 0.50, 0.45, 0.50));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::SpreadThresholdNotMet);
}

/// Verifies that batch signals when total ask below threshold.
#[test]
fn batch_signals_when_total_ask_below_threshold() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.40, 0.45, 0.40, 0.45));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Batch(signals) => {
            assert_eq!(signals.len(), 2);
            assert_eq!(signals[0].direction, SignalDirection::Up);
            assert_eq!(signals[1].direction, SignalDirection::Down);
            assert_eq!(signals[0].strategy, "spread-capture");
            assert_eq!(signals[1].strategy, "spread-capture");
        }
        _ => panic!("expected Batch result"),
    }
}

/// Verifies that min ask filter rejects degenerate books.
#[test]
fn min_ask_filter_rejects_degenerate_books() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.05, 0.10, 0.45, 0.49));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::EntryAskBelowMin);
}

/// Verifies that min ask filter rejects down side.
#[test]
fn min_ask_filter_rejects_down_side() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.45, 0.49, 0.05, 0.10));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::EntryAskBelowMin);
}

/// Verifies that confidence calculation.
#[test]
fn confidence_calculation() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.40, 0.45, 0.40, 0.45));
    if let StrategyResult::Batch(signals) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!((signals[0].confidence - 1.0).abs() < 1e-9);
        assert!((signals[1].confidence - 1.0).abs() < 1e-9);
    } else {
        panic!("expected Batch result");
    }
}

/// Verifies that confidence always saturates at one.
#[test]
fn confidence_always_saturates_at_one() {
    let mut config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    for threshold in [0.998, 0.95, 0.90] {
        config.spread_capture_threshold = threshold;
        config.spread_capture_min_ask = 0.10;

        let ctx = ctx_with_book(book(0.35, 0.40, 0.35, 0.40));
        if let StrategyResult::Batch(signals) = strat.evaluate(&ctx, &config, 1_000_000) {
            assert!(
                (signals[0].confidence - 1.0).abs() < 1e-9,
                "confidence should saturate at 1.0 for threshold={threshold}"
            );
        } else {
            panic!("expected Batch result for threshold={threshold}");
        }
    }
}

/// Verifies that metadata contains spread edge.
#[test]
fn metadata_contains_spread_edge() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.40, 0.45, 0.40, 0.45));
    if let StrategyResult::Batch(signals) = strat.evaluate(&ctx, &config, 1_000_000) {
        let meta = &signals[0].metadata;
        let total_ask = meta["totalAsk"].as_f64().unwrap();
        let spread_edge = meta["spreadEdge"].as_f64().unwrap();
        assert!((total_ask - 0.90).abs() < 1e-9);
        assert!((spread_edge - 0.10).abs() < 1e-9);
    } else {
        panic!("expected Batch result");
    }
}

/// Verifies that missing book returns none.
#[test]
fn missing_book_returns_none() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let ctx = ctx_with_book(BookState::default());
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::BookUnavailable);
}

/// Verifies that zero ask returns none.
#[test]
fn zero_ask_returns_none() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let ctx = ctx_with_book(book(0.0, 0.0, 0.45, 0.49));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::NonPositiveQuotes);
}

/// Verifies that strategy name.
#[test]
fn strategy_name() {
    let strat = SpreadCaptureStrategy::new();
    assert_eq!(strat.name(), "spread-capture");
}

/// Verifies that default creates instance.
#[test]
fn default_creates_instance() {
    let strat = SpreadCaptureStrategy;
    assert_eq!(strat.name(), "spread-capture");
}

/// Verifies that exact threshold boundary returns none.
#[test]
fn exact_threshold_boundary_returns_none() {
    let mut config = test_config();
    config.spread_capture_threshold = 0.998;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.45, 0.499, 0.45, 0.499));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::SpreadThresholdNotMet);
}

/// Verifies that just below threshold fires batch.
#[test]
fn just_below_threshold_fires_batch() {
    let mut config = test_config();
    config.spread_capture_threshold = 0.998;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.40, 0.45, 0.40, 0.45));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::Batch(_)));
}

/// Verifies that chainlink none uses zero.
#[test]
fn chainlink_none_uses_zero() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.0,
        chainlink_price: None,
        book_state: book(0.40, 0.45, 0.40, 0.45),
        window_open_price: Some(41_900.0),
        window_time_remaining_ms: 120_000,
        now_us: Some(1_000_000_000),
        features: SignalFeatureSnapshot::default(),
    };
    if let StrategyResult::Batch(signals) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!(
            (signals[0].chainlink_price - 0.0).abs() < f64::EPSILON,
            "chainlink_price should default to 0.0 when None"
        );
        assert!((signals[1].chainlink_price - 0.0).abs() < f64::EPSILON,);
    } else {
        panic!("expected Batch result");
    }
}

/// Verifies that signal prices match book.
#[test]
fn signal_prices_match_book() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let ctx = ctx_with_book(book(0.38, 0.45, 0.40, 0.44));
    if let StrategyResult::Batch(signals) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!((signals[0].up_ask - 0.45).abs() < f64::EPSILON);
        assert!((signals[0].down_ask - 0.44).abs() < f64::EPSILON);
        assert!((signals[0].up_bid - 0.38).abs() < f64::EPSILON);
        assert!((signals[0].down_bid - 0.40).abs() < f64::EPSILON);
        assert!((signals[0].binance_price - 42_000.0).abs() < f64::EPSILON);
    } else {
        panic!("expected Batch result");
    }
}

/// Verifies that missing only up book returns none.
#[test]
fn missing_only_up_book_returns_none() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let ctx = ctx_with_book(BookState {
        up: None,
        down: Some(TopOfBook {
            best_bid: 0.44,
            best_ask: 0.49,
            bid_size: 100.0,
            ask_size: 100.0,
            timestamp: 0,
            observed_at_ms: 0,
        }),
    });
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::BookUnavailable);
}

/// Verifies that down ask zero returns none.
#[test]
fn down_ask_zero_returns_none() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.45, 0.49, 0.00, 0.00));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::NonPositiveQuotes);
}

/// Verifies that threshold exactly one no divide by zero.
#[test]
fn threshold_exactly_one_no_divide_by_zero() {
    let mut config = test_config();
    config.spread_capture_threshold = 1.0;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.45, 0.49, 0.45, 0.49));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Batch(signals) => {
            assert_eq!(signals.len(), 2);

            assert!(
                (signals[0].confidence - 1.0).abs() < 1e-9,
                "confidence should be 1.0, got {}",
                signals[0].confidence
            );
        }
        _ => panic!("expected Batch result"),
    }
}

/// Verifies that total ask exactly at threshold returns none.
#[test]
fn total_ask_exactly_at_threshold_returns_none() {
    let mut config = test_config();
    config.spread_capture_threshold = 0.90;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();

    let ctx = ctx_with_book(book(0.40, 0.45, 0.40, 0.45));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::SpreadThresholdNotMet);
}

/// Verifies that stale quotes are rejected explicitly.
#[test]
fn stale_quotes_return_features_stale_rejection() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let mut ctx = ctx_with_book(book(0.40, 0.45, 0.40, 0.45));
    ctx.features.quote_age_ms = Some(config.max_quote_age_ms + 1);
    ctx.features.book_staleness_ms = Some(config.max_book_staleness_ms + 1);

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::FeaturesStale);
}

/// Verifies that high quote churn and move velocity are rejected explicitly.
#[test]
fn elevated_legging_risk_returns_rejection() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let mut ctx = ctx_with_book(book(0.40, 0.45, 0.40, 0.45));
    ctx.features.return_250ms = Some(config.latency_arb_momentum_threshold * 2.5);
    ctx.features.polymarket_quote_churn_per_s = Some(9.0);

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::LeggingRiskTooHigh);
}

/// Verifies that negative net edge is rejected explicitly.
#[test]
fn negative_net_edge_returns_expected_edge_rejection() {
    let mut config = test_config();
    config.spread_capture_threshold = 0.98;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();
    let mut ctx = ctx_with_book(book(0.01, 0.45, 0.01, 0.45));
    ctx.features.expected_up_slippage = Some(0.06);
    ctx.features.expected_down_slippage = Some(0.06);

    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert_rejected(result, StrategyRejectionReason::ExpectedEdgeNonPositive);
}
