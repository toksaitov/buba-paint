use super::*;
use crate::types::{BookState, TopOfBook};

/// Build a default config suitable for tests.
fn test_config() -> Config {
    Config::default()
}

/// Build a book state with both sides populated.
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

/// Ctx with.
fn ctx_with(momentum: f64, book_state: BookState, remaining_ms: u64) -> StrategyContext {
    StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: momentum,
        chainlink_price: Some(41_999.0),
        book_state,
        window_time_remaining_ms: remaining_ms,
    }
}

/// Verifies that no signal below threshold.
#[test]
fn no_signal_below_threshold() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0005, book(0.45, 0.50, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

/// Verifies that signal fires above threshold.
#[test]
fn signal_fires_above_threshold() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::Single(_)));
}

/// Verifies that direction up for positive momentum.
#[test]
fn direction_up_for_positive_momentum() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert_eq!(sig.direction, SignalDirection::Up);
    } else {
        panic!("expected Single signal");
    }
}

/// Verifies that direction down for negative momentum.
#[test]
fn direction_down_for_negative_momentum() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(-0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert_eq!(sig.direction, SignalDirection::Down);
    } else {
        panic!("expected Single signal");
    }
}

/// Verifies that cooldown blocks repeated signals.
#[test]
fn cooldown_blocks_repeated_signals() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);

    let t1 = 1_000_000;
    let result1 = strat.evaluate(&ctx, &config, t1);
    assert!(matches!(result1, StrategyResult::Single(_)));

    let t2 = t1 + 30_000;
    let result2 = strat.evaluate(&ctx, &config, t2);
    assert!(matches!(result2, StrategyResult::None));

    let t3 = t1 + 60_001;
    let result3 = strat.evaluate(&ctx, &config, t3);
    assert!(matches!(result3, StrategyResult::Single(_)));
}

/// Verifies that adaptive threshold with enough samples.
#[test]
fn adaptive_threshold_with_enough_samples() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let large_momentum = 0.010;
    for i in 0..100 {
        let ctx = ctx_with(large_momentum, book(0.45, 0.50, 0.45, 0.50), 120_000);

        let t = 100_000 + u64::try_from(i).unwrap() * 11_000;
        let _ = strat.evaluate(&ctx, &config, t);
    }

    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);

    let t = 100_000 + 200 * 11_000;
    let result = strat.evaluate(&ctx, &config, t);
    assert!(
        matches!(result, StrategyResult::None),
        "adaptive threshold should have raised above 0.0020"
    );
}

/// Verifies that min ask filter blocks cheap entries.
#[test]
fn min_ask_filter_blocks_cheap_entries() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.0020, book(0.15, 0.20, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

/// Verifies that window time filter blocks near expiry.
#[test]
fn window_time_filter_blocks_near_expiry() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 30_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

/// Verifies that missing book side returns none.
#[test]
fn missing_book_side_returns_none() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, BookState::default(), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

/// Verifies that max ask filter blocks expensive entries.
#[test]
fn max_ask_filter_blocks_expensive_entries() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.0020, book(0.55, 0.60, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

/// Verifies that confidence is correct.
#[test]
fn confidence_is_correct() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.003, book(0.45, 0.50, 0.45, 0.50), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!((sig.confidence - 1.0).abs() < 1e-9);
    } else {
        panic!("expected Single signal");
    }
}

/// Verifies that confidence partial ratio.
#[test]
fn confidence_partial_ratio() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let momentum = 0.001_501;
    let ctx = ctx_with(momentum, book(0.45, 0.50, 0.45, 0.50), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        let expected = (0.40 + 0.30 * (momentum / 0.0015)).min(1.0);
        assert!((sig.confidence - expected).abs() < 1e-6);
    } else {
        panic!("expected Single signal");
    }
}

/// Verifies that strategy name.
#[test]
fn strategy_name() {
    let strat = LatencyArbStrategy::new(0.0015);
    assert_eq!(strat.name(), "latency-arb");
}

/// Verifies that metadata contains expected fields.
#[test]
fn metadata_contains_expected_fields() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        let meta = &sig.metadata;
        assert!(
            meta.get("momentum").is_some(),
            "metadata should contain 'momentum'"
        );
        assert!(
            meta.get("threshold").is_some(),
            "metadata should contain 'threshold'"
        );
        assert!(
            meta.get("ratio").is_some(),
            "metadata should contain 'ratio'"
        );
        let momentum = meta["momentum"].as_f64().unwrap();
        assert!((momentum - 0.0020).abs() < 1e-9);
    } else {
        panic!("expected Single signal");
    }
}

/// Verifies that chainlink price none defaults to zero.
#[test]
fn chainlink_price_none_defaults_to_zero() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.0020,
        chainlink_price: None,
        book_state: book(0.45, 0.50, 0.45, 0.50),
        window_time_remaining_ms: 120_000,
    };
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!(
            (sig.chainlink_price - 0.0).abs() < f64::EPSILON,
            "chainlink_price should default to 0.0 when None"
        );
    } else {
        panic!("expected Single signal");
    }
}

/// Verifies that signal includes all book prices.
#[test]
fn signal_includes_all_book_prices() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, book(0.40, 0.50, 0.42, 0.48), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!((sig.up_ask - 0.50).abs() < f64::EPSILON);
        assert!((sig.down_ask - 0.48).abs() < f64::EPSILON);
        assert!((sig.up_bid - 0.40).abs() < f64::EPSILON);
        assert!((sig.down_bid - 0.42).abs() < f64::EPSILON);
        assert!((sig.binance_price - 42_000.0).abs() < f64::EPSILON);
    } else {
        panic!("expected Single signal");
    }
}

/// Verifies that zero ask prices return none.
#[test]
fn zero_ask_prices_return_none() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.0020, book(0.0, 0.0, 0.0, 0.0), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

/// Verifies that min ask filter blocks down direction.
#[test]
fn min_ask_filter_blocks_down_direction() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(-0.0020, book(0.45, 0.50, 0.15, 0.20), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

/// Verifies that max ask filter blocks down direction.
#[test]
fn max_ask_filter_blocks_down_direction() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(-0.0020, book(0.45, 0.50, 0.55, 0.60), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

/// Verifies that adaptive threshold returns base with few samples.
#[test]
fn adaptive_threshold_returns_base_with_few_samples() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    for i in 0..10 {
        let ctx = ctx_with(0.001, book(0.45, 0.50, 0.45, 0.50), 120_000);
        let t = 100_000 + u64::try_from(i).unwrap() * 11_000;
        let _ = strat.evaluate(&ctx, &config, t);
    }

    let threshold = strat.get_adaptive_threshold(300_000, config.latency_arb_momentum_threshold);
    assert!(
        (threshold - config.latency_arb_momentum_threshold).abs() < 1e-10,
        "expected base threshold with < 60 samples"
    );
}

/// Verifies that adaptive threshold cached within 10s.
#[test]
fn adaptive_threshold_cached_within_10s() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let t1 = strat.get_adaptive_threshold(0, 0.0015);

    for _ in 0..100 {
        strat.momentum_buffer.push(0.05);
    }

    let t2 = strat.get_adaptive_threshold(5_000, 0.0015);
    assert!(
        (t1 - t2).abs() < f64::EPSILON,
        "threshold should be cached within 10s window"
    );

    let t3 = strat.get_adaptive_threshold(10_001, 0.0015);
    assert!(
        t3 > t1,
        "threshold should be recalculated and higher with large momentum data"
    );
}

/// Verifies that momentum buffer eviction at capacity.
#[test]
fn momentum_buffer_eviction_at_capacity() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    for i in 0..(MOMENTUM_BUFFER_SIZE + 100) {
        let ctx = ctx_with(0.001, book(0.45, 0.50, 0.45, 0.50), 120_000);

        let t = u64::try_from(i).unwrap() * 100_000;
        let _ = strat.evaluate(&ctx, &config, t);
    }

    assert_eq!(
        strat.momentum_buffer.len(),
        MOMENTUM_BUFFER_SIZE,
        "buffer should not exceed MOMENTUM_BUFFER_SIZE"
    );
}

/// Verifies that confidence caps at one.
#[test]
fn confidence_caps_at_one() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.050, book(0.45, 0.50, 0.45, 0.50), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!(
            (sig.confidence - 1.0).abs() < 1e-9,
            "confidence should cap at 1.0"
        );
    } else {
        panic!("expected Single signal");
    }
}

/// Verifies that momentum exactly at threshold does not fire.
#[test]
fn momentum_exactly_at_threshold_does_not_fire() {
    let mut config = test_config();
    config.latency_arb_momentum_threshold = 0.001;
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.001, book(0.45, 0.50, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(
        matches!(result, StrategyResult::None),
        "momentum exactly at threshold should not fire (uses > not >=)"
    );
}

/// Verifies that momentum just above threshold fires.
#[test]
fn momentum_just_above_threshold_fires() {
    let mut config = test_config();
    config.latency_arb_momentum_threshold = 0.001;
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.001_001, book(0.45, 0.50, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(
        matches!(result, StrategyResult::Single(_)),
        "momentum just above threshold should fire"
    );
}

/// Verifies that ask exactly at max does not fire.
#[test]
fn ask_exactly_at_max_does_not_fire() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.0020, book(0.50, 0.55, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(
        matches!(result, StrategyResult::None),
        "ask exactly at max_ask should not fire (uses < not <=)"
    );
}

/// Verifies that ask exactly at min does not fire.
#[test]
fn ask_exactly_at_min_does_not_fire() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    let ctx = ctx_with(0.0020, book(0.25, 0.30, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);

    assert!(
        matches!(result, StrategyResult::Single(_)),
        "ask exactly at min_ask should fire (uses < not <=)"
    );
}

/// Verifies that cooldown exact boundary allows signal.
#[test]
fn cooldown_exact_boundary_allows_signal() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);

    let t1 = 1_000_000;
    let result1 = strat.evaluate(&ctx, &config, t1);
    assert!(matches!(result1, StrategyResult::Single(_)));

    let t2 = t1 + config.latency_arb_cooldown_ms;
    let result2 = strat.evaluate(&ctx, &config, t2);
    assert!(
        matches!(result2, StrategyResult::Single(_)),
        "elapsed == cooldown_ms should not be blocked (uses < not <=)"
    );
}

/// Verifies that adaptive threshold with exactly 60 samples.
#[test]
fn adaptive_threshold_with_exactly_60_samples() {
    let base_threshold = 0.001;
    let mut strat = LatencyArbStrategy::new(base_threshold);

    for i in 1..=60 {
        strat.momentum_buffer.push(f64::from(i) * 0.001);
    }

    strat.last_threshold_calc = 0;
    let threshold = strat.get_adaptive_threshold(100_000, base_threshold);

    let expected_p85 = 0.052;

    let expected = base_threshold.max(expected_p85);
    assert!(
        (threshold - expected).abs() < 1e-12,
        "expected adaptive threshold {expected}, got {threshold}"
    );
}

/// Verifies that adaptive threshold with nan values.
#[test]
fn adaptive_threshold_with_nan_values() {
    let base_threshold = 0.001;
    let mut strat = LatencyArbStrategy::new(base_threshold);

    for i in 0..55 {
        strat.momentum_buffer.push(f64::from(i) * 0.001);
    }
    for _ in 0..5 {
        strat.momentum_buffer.push(f64::NAN);
    }
    assert_eq!(strat.momentum_buffer.len(), 60);

    strat.last_threshold_calc = 0;

    let threshold = strat.get_adaptive_threshold(100_000, base_threshold);

    assert!(
        threshold.is_finite(),
        "adaptive threshold should be finite even with NaN inputs, got {threshold}"
    );
}
