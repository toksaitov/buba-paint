use super::*;
use crate::types::{BookState, TopOfBook};

fn test_config() -> Config {
    Config::default()
}

fn book(up_bid: f64, up_ask: f64, down_bid: f64, down_ask: f64) -> BookState {
    BookState {
        up: Some(TopOfBook {
            best_bid: up_bid,
            best_ask: up_ask,
            bid_size: 100.0,
            ask_size: 100.0,
            timestamp: 0,
        }),
        down: Some(TopOfBook {
            best_bid: down_bid,
            best_ask: down_ask,
            bid_size: 100.0,
            ask_size: 100.0,
            timestamp: 0,
        }),
    }
}

fn ctx_with_book(book_state: BookState) -> StrategyContext {
    StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.0,
        chainlink_price: Some(41_999.0),
        book_state,
        window_time_remaining_ms: 120_000,
    }
}

// -- No signal when total ask >= threshold --

#[test]
fn no_signal_when_total_ask_above_threshold() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    // total_ask = 0.50 + 0.50 = 1.00 >= 0.998
    let ctx = ctx_with_book(book(0.45, 0.50, 0.45, 0.50));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Batch of 2 signals when total ask < threshold --

#[test]
fn batch_signals_when_total_ask_below_threshold() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    // total_ask = 0.49 + 0.49 = 0.98 < 0.998
    let ctx = ctx_with_book(book(0.45, 0.49, 0.45, 0.49));
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

// -- Min ask filter rejects degenerate books --

#[test]
fn min_ask_filter_rejects_degenerate_books() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    // up_ask = 0.10 < min_ask (default 0.15), even though total would be < threshold
    let ctx = ctx_with_book(book(0.05, 0.10, 0.45, 0.49));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

#[test]
fn min_ask_filter_rejects_down_side() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    // down_ask = 0.10 < min_ask
    let ctx = ctx_with_book(book(0.45, 0.49, 0.05, 0.10));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Correct confidence calculation --

#[test]
fn confidence_calculation() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    // total_ask = 0.49 + 0.49 = 0.98
    // edge = 1.0 - 0.98 = 0.02
    // max_edge = 1.0 - 0.998 = 0.002
    // confidence = min(1.0, 0.5 + 0.5 * (0.02 / 0.002)) = min(1.0, 0.5 + 5.0) = 1.0
    let ctx = ctx_with_book(book(0.45, 0.49, 0.45, 0.49));
    if let StrategyResult::Batch(signals) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!((signals[0].confidence - 1.0).abs() < 1e-9);
        assert!((signals[1].confidence - 1.0).abs() < 1e-9);
    } else {
        panic!("expected Batch result");
    }
}

#[test]
fn confidence_always_saturates_at_one() {
    // The formula: conf = min(1.0, 0.5 + 0.5 * (edge / max_edge))
    // where edge = 1.0 - total_ask, max_edge = 1.0 - threshold.
    // Since total_ask < threshold implies edge > max_edge, the ratio is always > 1
    // and confidence always caps at 1.0. Verify this with various thresholds.
    let mut config = test_config();
    let mut strat = SpreadCaptureStrategy::new();

    for threshold in [0.998, 0.95, 0.90] {
        config.spread_capture_threshold = threshold;
        config.spread_capture_min_ask = 0.10;
        // total_ask just barely below threshold
        let half = (threshold - 0.001) / 2.0;
        let ctx = ctx_with_book(book(0.40, half, 0.40, half));
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

#[test]
fn metadata_contains_spread_edge() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    // total = 0.49 + 0.49 = 0.98
    let ctx = ctx_with_book(book(0.45, 0.49, 0.45, 0.49));
    if let StrategyResult::Batch(signals) = strat.evaluate(&ctx, &config, 1_000_000) {
        let meta = &signals[0].metadata;
        let total_ask = meta["totalAsk"].as_f64().unwrap();
        let spread_edge = meta["spreadEdge"].as_f64().unwrap();
        assert!((total_ask - 0.98).abs() < 1e-9);
        assert!((spread_edge - 0.02).abs() < 1e-9);
    } else {
        panic!("expected Batch result");
    }
}

// -- Missing book side --

#[test]
fn missing_book_returns_none() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let ctx = ctx_with_book(BookState::default());
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Zero ask price --

#[test]
fn zero_ask_returns_none() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let ctx = ctx_with_book(book(0.0, 0.0, 0.45, 0.49));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Strategy name --

#[test]
fn strategy_name() {
    let strat = SpreadCaptureStrategy::new();
    assert_eq!(strat.name(), "spread-capture");
}

// -- Default trait --

#[test]
fn default_creates_instance() {
    let strat = SpreadCaptureStrategy;
    assert_eq!(strat.name(), "spread-capture");
}

// -- Phase D: additional edge-case tests ----------------------------------

#[test]
fn exact_threshold_boundary_returns_none() {
    let mut config = test_config();
    config.spread_capture_threshold = 0.998;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();
    // total_ask = 0.499 + 0.499 = 0.998 == threshold → NOT < threshold
    let ctx = ctx_with_book(book(0.45, 0.499, 0.45, 0.499));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

#[test]
fn just_below_threshold_fires_batch() {
    let mut config = test_config();
    config.spread_capture_threshold = 0.998;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();
    // total_ask = 0.4989 + 0.4989 = 0.9978 < 0.998
    let ctx = ctx_with_book(book(0.45, 0.4989, 0.45, 0.4989));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::Batch(_)));
}

#[test]
fn chainlink_none_uses_zero() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    // total_ask = 0.49 + 0.49 = 0.98 < 0.998
    let ctx = StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.0,
        chainlink_price: None,
        book_state: book(0.45, 0.49, 0.45, 0.49),
        window_time_remaining_ms: 120_000,
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

#[test]
fn signal_prices_match_book() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    let ctx = ctx_with_book(book(0.40, 0.49, 0.42, 0.48));
    if let StrategyResult::Batch(signals) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!((signals[0].up_ask - 0.49).abs() < f64::EPSILON);
        assert!((signals[0].down_ask - 0.48).abs() < f64::EPSILON);
        assert!((signals[0].up_bid - 0.40).abs() < f64::EPSILON);
        assert!((signals[0].down_bid - 0.42).abs() < f64::EPSILON);
        assert!((signals[0].binance_price - 42_000.0).abs() < f64::EPSILON);
    } else {
        panic!("expected Batch result");
    }
}

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
        }),
    });
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

#[test]
fn down_ask_zero_returns_none() {
    let config = test_config();
    let mut strat = SpreadCaptureStrategy::new();
    // down_ask = 0.0 (first zero check)
    let ctx = ctx_with_book(book(0.45, 0.49, 0.00, 0.00));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Boundary-condition tests ---------------------------------------------

#[test]
fn threshold_exactly_one_no_divide_by_zero() {
    // When threshold=1.0, max_edge = 1.0 - 1.0 = 0.0.
    // The confidence formula is `0.5 + 0.5 * (edge / max_edge)` which would
    // divide by zero. However, with threshold=1.0 the guard
    // `total_ask >= threshold` should catch any total_ask >= 1.0.
    // For total_ask < 1.0 we DO reach the divide-by-zero line.
    // This test verifies the code handles it correctly.
    let mut config = test_config();
    config.spread_capture_threshold = 1.0;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();
    // total_ask = 0.49 + 0.49 = 0.98 < 1.0, so it passes the threshold guard.
    // edge = 1.0 - 0.98 = 0.02, max_edge = 1.0 - 1.0 = 0.0
    // confidence = 0.5 + 0.5 * (0.02 / 0.0) = 0.5 + Inf = Inf → min(1.0, Inf) = 1.0
    // This works in Rust because f64 division by 0.0 yields Inf (not panic),
    // and min(1.0, Inf) == 1.0.
    let ctx = ctx_with_book(book(0.45, 0.49, 0.45, 0.49));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    match result {
        StrategyResult::Batch(signals) => {
            assert_eq!(signals.len(), 2);
            // Confidence should be clamped to 1.0 (Inf.min(1.0) == 1.0).
            assert!(
                (signals[0].confidence - 1.0).abs() < 1e-9,
                "confidence should be 1.0, got {}",
                signals[0].confidence
            );
        }
        _ => panic!("expected Batch result"),
    }
}

#[test]
fn total_ask_exactly_at_threshold_returns_none() {
    // The code uses `total_ask >= threshold` (not `>`), so exact match returns None.
    let mut config = test_config();
    config.spread_capture_threshold = 0.90;
    config.spread_capture_min_ask = 0.10;
    let mut strat = SpreadCaptureStrategy::new();
    // total_ask = 0.45 + 0.45 = 0.90 == threshold
    let ctx = ctx_with_book(book(0.40, 0.45, 0.40, 0.45));
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(
        matches!(result, StrategyResult::None),
        "total_ask exactly at threshold should return None (uses >= not >)"
    );
}
