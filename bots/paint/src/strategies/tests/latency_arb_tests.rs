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

fn ctx_with(momentum: f64, book_state: BookState, remaining_ms: u64) -> StrategyContext {
    StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: momentum,
        chainlink_price: Some(41_999.0),
        book_state,
        window_time_remaining_ms: remaining_ms,
    }
}

// -- No signal when momentum is below threshold --

#[test]
fn no_signal_below_threshold() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0005, book(0.45, 0.50, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Signal fires when momentum exceeds threshold --

#[test]
fn signal_fires_above_threshold() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    // Positive momentum above 0.0015 default threshold
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::Single(_)));
}

// -- Correct direction: UP for positive momentum --

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

// -- Correct direction: DOWN for negative momentum --

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

// -- Cooldown blocks repeated signals --

#[test]
fn cooldown_blocks_repeated_signals() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);

    // First signal fires.
    let t1 = 1_000_000;
    let result1 = strat.evaluate(&ctx, &config, t1);
    assert!(matches!(result1, StrategyResult::Single(_)));

    // Second call within cooldown (default 60_000ms) should be blocked.
    let t2 = t1 + 30_000; // 30s later — within cooldown
    let result2 = strat.evaluate(&ctx, &config, t2);
    assert!(matches!(result2, StrategyResult::None));

    // After cooldown expires, signal fires again.
    let t3 = t1 + 60_001;
    let result3 = strat.evaluate(&ctx, &config, t3);
    assert!(matches!(result3, StrategyResult::Single(_)));
}

// -- Adaptive threshold with enough samples --

#[test]
fn adaptive_threshold_with_enough_samples() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // Fill 100 samples with large momentum values so p85 > base threshold.
    let large_momentum = 0.010;
    for i in 0..100 {
        let ctx = ctx_with(large_momentum, book(0.45, 0.50, 0.45, 0.50), 120_000);
        // Use widely spaced times so threshold recalc fires each iteration.
        let t = 100_000 + u64::try_from(i).unwrap() * 11_000;
        let _ = strat.evaluate(&ctx, &config, t);
    }

    // Now adaptive threshold should be >= p85 of the buffer (all ~0.010).
    // A momentum of 0.0020 (above base 0.0015 but well below 0.010) should NOT fire.
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);
    // Ensure we're past any cooldown by using a time far ahead.
    let t = 100_000 + 200 * 11_000;
    let result = strat.evaluate(&ctx, &config, t);
    assert!(
        matches!(result, StrategyResult::None),
        "adaptive threshold should have raised above 0.0020"
    );
}

// -- Min ask filter blocks cheap entries --

#[test]
fn min_ask_filter_blocks_cheap_entries() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // Up ask = 0.20, which is below min_ask (default 0.30). Positive momentum -> UP.
    let ctx = ctx_with(0.0020, book(0.15, 0.20, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Window time filter blocks near-expiry --

#[test]
fn window_time_filter_blocks_near_expiry() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // remaining_ms = 30_000, below min_window_time_ms (default 90_000)
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 30_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Missing book side returns None --

#[test]
fn missing_book_side_returns_none() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, BookState::default(), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Max ask filter blocks expensive entries --

#[test]
fn max_ask_filter_blocks_expensive_entries() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // up_ask = 0.60, which is >= max_ask (default 0.55)
    let ctx = ctx_with(0.0020, book(0.55, 0.60, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

// -- Confidence calculation --

#[test]
fn confidence_is_correct() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // momentum = 0.003 (2x threshold of 0.0015) -> ratio = 2.0
    // confidence = min(1.0, 0.40 + 0.30 * 2.0) = min(1.0, 1.0) = 1.0
    let ctx = ctx_with(0.003, book(0.45, 0.50, 0.45, 0.50), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        assert!((sig.confidence - 1.0).abs() < 1e-9);
    } else {
        panic!("expected Single signal");
    }
}

#[test]
fn confidence_partial_ratio() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // momentum = 0.0015 (exactly at threshold) -> ratio = 1.0
    // But this is NOT > threshold, so it won't fire. Use 0.00151.
    let momentum = 0.001_501;
    let ctx = ctx_with(momentum, book(0.45, 0.50, 0.45, 0.50), 120_000);
    if let StrategyResult::Single(sig) = strat.evaluate(&ctx, &config, 1_000_000) {
        // ratio ~ 1.0006..., confidence ~ 0.40 + 0.30 * 1.0006... ~ 0.7002
        let expected = (0.40 + 0.30 * (momentum / 0.0015)).min(1.0);
        assert!((sig.confidence - expected).abs() < 1e-6);
    } else {
        panic!("expected Single signal");
    }
}

// -- Strategy name --

#[test]
fn strategy_name() {
    let strat = LatencyArbStrategy::new(0.0015);
    assert_eq!(strat.name(), "latency-arb");
}

// -- Phase D: additional edge-case tests ----------------------------------

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

#[test]
fn zero_ask_prices_return_none() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    // Both asks are 0 → early return.
    let ctx = ctx_with(0.0020, book(0.0, 0.0, 0.0, 0.0), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

#[test]
fn min_ask_filter_blocks_down_direction() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    // Negative momentum → DOWN direction. down_ask = 0.20 < min_ask (0.30).
    let ctx = ctx_with(-0.0020, book(0.45, 0.50, 0.15, 0.20), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

#[test]
fn max_ask_filter_blocks_down_direction() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    // Negative momentum → would pick DOWN. down_ask = 0.60 >= max_ask (0.55).
    let ctx = ctx_with(-0.0020, book(0.45, 0.50, 0.55, 0.60), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(matches!(result, StrategyResult::None));
}

#[test]
fn adaptive_threshold_returns_base_with_few_samples() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // Push only 10 samples (< 60 minimum for adaptive).
    for i in 0..10 {
        let ctx = ctx_with(0.001, book(0.45, 0.50, 0.45, 0.50), 120_000);
        let t = 100_000 + u64::try_from(i).unwrap() * 11_000;
        let _ = strat.evaluate(&ctx, &config, t);
    }

    // The adaptive threshold should still equal the base threshold.
    let threshold = strat.get_adaptive_threshold(300_000, config.latency_arb_momentum_threshold);
    assert!(
        (threshold - config.latency_arb_momentum_threshold).abs() < 1e-10,
        "expected base threshold with < 60 samples"
    );
}

#[test]
fn adaptive_threshold_cached_within_10s() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // First call at t=0 sets the threshold.
    let t1 = strat.get_adaptive_threshold(0, 0.0015);

    // Add some data that would change p85.
    for _ in 0..100 {
        strat.momentum_buffer.push(0.05);
    }

    // Call at t=5000 (within 10s) should return cached value.
    let t2 = strat.get_adaptive_threshold(5_000, 0.0015);
    assert!(
        (t1 - t2).abs() < f64::EPSILON,
        "threshold should be cached within 10s window"
    );

    // Call at t=10001 (past 10s) should recalculate.
    let t3 = strat.get_adaptive_threshold(10_001, 0.0015);
    assert!(
        t3 > t1,
        "threshold should be recalculated and higher with large momentum data"
    );
}

#[test]
fn momentum_buffer_eviction_at_capacity() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);

    // Fill the buffer well past MOMENTUM_BUFFER_SIZE.
    for i in 0..(MOMENTUM_BUFFER_SIZE + 100) {
        let ctx = ctx_with(0.001, book(0.45, 0.50, 0.45, 0.50), 120_000);
        // Use times far apart to avoid cooldown blocking (but we don't care about the result).
        let t = u64::try_from(i).unwrap() * 100_000;
        let _ = strat.evaluate(&ctx, &config, t);
    }

    assert_eq!(
        strat.momentum_buffer.len(),
        MOMENTUM_BUFFER_SIZE,
        "buffer should not exceed MOMENTUM_BUFFER_SIZE"
    );
}

#[test]
fn confidence_caps_at_one() {
    let config = test_config();
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    // Very large momentum → ratio >> 1 → confidence should cap at 1.0.
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

// -- Boundary-condition tests ---------------------------------------------

#[test]
fn momentum_exactly_at_threshold_does_not_fire() {
    // The code uses `>` (not `>=`), so momentum == threshold should NOT fire.
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

#[test]
fn ask_exactly_at_max_does_not_fire() {
    // The code uses `up_ask < max_ask` (not `<=`), so exact match should NOT fire.
    let config = test_config(); // max_ask = 0.55
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    // Positive momentum → UP direction. up_ask == max_ask (0.55).
    let ctx = ctx_with(0.0020, book(0.50, 0.55, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    assert!(
        matches!(result, StrategyResult::None),
        "ask exactly at max_ask should not fire (uses < not <=)"
    );
}

#[test]
fn ask_exactly_at_min_does_not_fire() {
    // The code uses `entry_ask < min_ask` for rejection.
    // When entry_ask == min_ask (0.30), `0.30 < 0.30` is false, so it
    // passes the min_ask filter. However, we need to verify the full path.
    let config = test_config(); // min_ask = 0.30
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    // Positive momentum → UP direction. up_ask == min_ask (0.30).
    let ctx = ctx_with(0.0020, book(0.25, 0.30, 0.45, 0.50), 120_000);
    let result = strat.evaluate(&ctx, &config, 1_000_000);
    // entry_ask = 0.30, min_ask = 0.30: `0.30 < 0.30` is false → passes filter.
    // up_ask = 0.30 < max_ask (0.55) → passes max_ask check.
    // Should fire.
    assert!(
        matches!(result, StrategyResult::Single(_)),
        "ask exactly at min_ask should fire (uses < not <=)"
    );
}

#[test]
fn cooldown_exact_boundary_allows_signal() {
    // The code uses `now - last_signal_time < cooldown_ms` for blocking.
    // When elapsed == cooldown_ms, `elapsed < cooldown` is false → NOT blocked.
    let config = test_config(); // cooldown_ms = 60_000
    let mut strat = LatencyArbStrategy::new(config.latency_arb_momentum_threshold);
    let ctx = ctx_with(0.0020, book(0.45, 0.50, 0.45, 0.50), 120_000);

    // First signal fires at t=1_000_000.
    let t1 = 1_000_000;
    let result1 = strat.evaluate(&ctx, &config, t1);
    assert!(matches!(result1, StrategyResult::Single(_)));

    // Second call at exactly t1 + cooldown_ms should NOT be blocked.
    let t2 = t1 + config.latency_arb_cooldown_ms; // exactly at boundary
    let result2 = strat.evaluate(&ctx, &config, t2);
    assert!(
        matches!(result2, StrategyResult::Single(_)),
        "elapsed == cooldown_ms should not be blocked (uses < not <=)"
    );
}

#[test]
fn adaptive_threshold_with_exactly_60_samples() {
    // MIN_SAMPLES_FOR_ADAPTIVE = 60. With exactly 60 samples the adaptive
    // branch should activate and compute p85.
    let base_threshold = 0.001;
    let mut strat = LatencyArbStrategy::new(base_threshold);

    // Push exactly 60 momentum values into the buffer directly.
    // Use values 1..=60 so the sorted order is known.
    for i in 1..=60 {
        strat.momentum_buffer.push(f64::from(i) * 0.001);
    }

    // Force recalculation by setting last_threshold_calc far in the past.
    strat.last_threshold_calc = 0;
    let threshold = strat.get_adaptive_threshold(100_000, base_threshold);

    // p85_idx = floor(60 * 0.85) = floor(51.0) = 51
    // sorted buffer: [0.001, 0.002, ..., 0.060]
    // sorted[51] = 0.052 (index 51 = 52nd element = 52 * 0.001)
    let expected_p85 = 0.052;
    // adaptive_threshold = max(base_threshold, p85)
    let expected = base_threshold.max(expected_p85);
    assert!(
        (threshold - expected).abs() < 1e-12,
        "expected adaptive threshold {expected}, got {threshold}"
    );
}

#[test]
fn adaptive_threshold_with_nan_values() {
    // Verify that NaN values in the momentum buffer don't cause a panic.
    // The sort uses `partial_cmp(...).unwrap_or(Equal)` which should handle NaN.
    let base_threshold = 0.001;
    let mut strat = LatencyArbStrategy::new(base_threshold);

    // Push 60 values with some NaN mixed in.
    for i in 0..55 {
        strat.momentum_buffer.push(f64::from(i) * 0.001);
    }
    for _ in 0..5 {
        strat.momentum_buffer.push(f64::NAN);
    }
    assert_eq!(strat.momentum_buffer.len(), 60);

    strat.last_threshold_calc = 0;
    // This should not panic.
    let threshold = strat.get_adaptive_threshold(100_000, base_threshold);
    // Threshold should be a finite number (NaN comparisons fall back to Equal).
    assert!(
        threshold.is_finite(),
        "adaptive threshold should be finite even with NaN inputs, got {threshold}"
    );
}
