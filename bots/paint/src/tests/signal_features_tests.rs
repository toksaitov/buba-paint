use super::*;
use crate::types::{BookState, MarketWindow, OrderLevel, TopOfBook};

/// Build a representative crypto market window for feature-engine tests.
fn market_window() -> MarketWindow {
    MarketWindow {
        market_id: "mkt-1".to_string(),
        question: "Will BTC close up?".to_string(),
        up_token_id: "up-token".to_string(),
        down_token_id: "down-token".to_string(),
        condition_id: "cond-1".to_string(),
        start_time: 100_000,
        end_time: 400_000,
        slug: "btc-updown-5m".to_string(),
        outcome: None,
        resolution_source: Some("gamma".to_string()),
        fee_profile: Some("crypto".to_string()),
        order_min_size: Some(1.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: None,
        taker_base_fee: None,
        rewards_min_size: None,
        rewards_max_spread: None,
        fees_enabled: Some(true),
        fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".to_string()),
        token_fee_rates_json: Some("{\"up-token\":{\"base_fee\":1000}}".to_string()),
        accepting_orders: Some(true),
        accepting_orders_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
        clear_book_on_start: Some(false),
    }
}

/// Build a symmetric binary-market book snapshot.
fn book_state(timestamp: u64) -> BookState {
    BookState {
        up: Some(TopOfBook {
            best_bid: 0.48,
            best_ask: 0.49,
            bid_size: 100.0,
            ask_size: 120.0,
            timestamp,
            observed_at_ms: timestamp,
        }),
        down: Some(TopOfBook {
            best_bid: 0.49,
            best_ask: 0.50,
            bid_size: 90.0,
            ask_size: 110.0,
            timestamp,
            observed_at_ms: timestamp,
        }),
    }
}

/// Verify that legacy inputs still produce a stable legacy-core feature snapshot.
#[test]
fn compute_returns_legacy_core_without_raw_only_inputs() {
    let config = crate::config::Config::default();
    let window = market_window();
    let mut state = SignalState::new();
    state.update_binance_trade(70_000.0, 1.0, None, 100_000, None);
    state.update_binance_trade(70_030.0, 1.0, None, 100_600, None);
    state.update_chainlink(70_010.0, 100_650, None);
    state.update_clob(book_state(100_700), 100_700, None);

    let snapshot = SignalFeatureEngine::compute(
        &mut state,
        Some(&window),
        Some(69_900.0),
        0.0012,
        101_000,
        None,
        &config,
    );

    assert_eq!(snapshot.feature_mode, FeatureMode::LegacyCore);
    assert!(snapshot.distance_from_open_bps.is_some());
    assert!(snapshot.summed_ask_edge.is_some());
    assert!(snapshot.quote_age_ms.is_some());
    assert!(snapshot.expected_up_fee.is_some());
    assert!(snapshot.available_feature_count() >= 6);
}

/// Verify that raw-event inputs unlock the richer feature mode and lag metrics.
#[test]
fn compute_returns_raw_event_full_when_raw_features_are_available() {
    let config = crate::config::Config::default();
    let window = market_window();
    let mut state = SignalState::new();
    state.update_binance_trade(70_000.0, 1.0, Some(1.0), 100_000, Some(100_000_000));
    state.update_binance_trade(70_020.0, 2.0, Some(-2.0), 100_100, Some(100_100_000));
    state.update_binance_trade(70_060.0, 3.0, Some(3.0), 100_200, Some(100_200_000));
    state.update_binance_book(
        69_995.0,
        70_005.0,
        8.0,
        4.0,
        100_250,
        Some("42".to_string()),
    );
    state.update_binance_depth(
        vec![
            OrderLevel {
                price: 69_995.0,
                size: 8.0,
            },
            OrderLevel {
                price: 69_990.0,
                size: 6.0,
            },
        ],
        vec![
            OrderLevel {
                price: 70_005.0,
                size: 4.0,
            },
            OrderLevel {
                price: 70_010.0,
                size: 3.0,
            },
        ],
        100_260,
        Some("43".to_string()),
    );
    state.update_chainlink(70_010.0, 100_270, Some(100_270_000));
    state.update_clob(book_state(100_280), 100_280, Some(100_280_000));
    state.update_clob(
        BookState {
            up: Some(TopOfBook {
                best_bid: 0.47,
                best_ask: 0.48,
                bid_size: 110.0,
                ask_size: 100.0,
                timestamp: 100_500,
                observed_at_ms: 100_500,
            }),
            down: Some(TopOfBook {
                best_bid: 0.50,
                best_ask: 0.51,
                bid_size: 120.0,
                ask_size: 90.0,
                timestamp: 100_500,
                observed_at_ms: 100_500,
            }),
        },
        100_500,
        Some(100_500_000),
    );

    let snapshot = SignalFeatureEngine::compute(
        &mut state,
        Some(&window),
        Some(69_950.0),
        0.0014,
        100_700,
        Some(100_700_000),
        &config,
    );

    assert_eq!(snapshot.feature_mode, FeatureMode::RawEventFull);
    assert!(snapshot.binance_signed_trade_imbalance.is_some());
    assert!(snapshot.binance_book_imbalance.is_some());
    assert!(snapshot.binance_depth_sweep_cost.is_some());
    assert!(snapshot.polymarket_quote_churn_per_s.is_some());
    assert!(snapshot.polymarket_microprice_skew.is_some());
    assert!(snapshot.event_to_decision_lag_us.is_some());
}

/// Verify that pruning discards stale trade and quote history outside the configured window.
#[test]
fn prune_discards_stale_history() {
    let mut state = SignalState::new();
    state.update_binance_trade(70_000.0, 1.0, Some(1.0), 100_000, Some(100_000_000));
    state.update_binance_trade(70_010.0, 1.0, Some(1.0), 106_500, Some(106_500_000));
    state.update_clob(book_state(100_100), 100_100, Some(100_100_000));
    state.update_clob(book_state(106_500), 106_500, Some(106_500_000));
    state.prune(106_500);

    assert_eq!(state.binance_trades.len(), 2);
    assert_eq!(state.clob_quote_churn_per_s(106_500, 1_000), None);
}

/// Verify that live quote freshness uses observed receipt time when the raw
/// CLOB source timestamp is missing.
#[test]
fn compute_uses_observed_freshness_when_source_timestamp_is_zero() {
    let config = crate::config::Config::default();
    let window = market_window();
    let mut state = SignalState::new();
    state.update_binance_trade(70_000.0, 1.0, None, 100_000, None);
    state.update_chainlink(70_005.0, 100_600, None);
    state.update_clob(
        BookState {
            up: Some(TopOfBook {
                best_bid: 0.48,
                best_ask: 0.49,
                bid_size: 100.0,
                ask_size: 120.0,
                timestamp: 0,
                observed_at_ms: 100_650,
            }),
            down: Some(TopOfBook {
                best_bid: 0.49,
                best_ask: 0.50,
                bid_size: 90.0,
                ask_size: 110.0,
                timestamp: 0,
                observed_at_ms: 100_650,
            }),
        },
        100_650,
        None,
    );

    let snapshot = SignalFeatureEngine::compute(
        &mut state,
        Some(&window),
        Some(69_900.0),
        0.0012,
        100_700,
        None,
        &config,
    );

    assert_eq!(snapshot.quote_age_ms, Some(50));
    assert_eq!(snapshot.book_staleness_ms, Some(50));
}

/// Verify that combined freshness reflects the older side of the binary quote.
#[test]
fn compute_uses_stalest_side_for_combined_quote_age() {
    let config = crate::config::Config::default();
    let window = market_window();
    let mut state = SignalState::new();
    state.update_binance_trade(70_000.0, 1.0, None, 100_000, None);
    state.update_chainlink(70_005.0, 100_650, None);
    state.update_clob(
        BookState {
            up: Some(TopOfBook {
                best_bid: 0.48,
                best_ask: 0.49,
                bid_size: 100.0,
                ask_size: 120.0,
                timestamp: 0,
                observed_at_ms: 100_695,
            }),
            down: Some(TopOfBook {
                best_bid: 0.49,
                best_ask: 0.50,
                bid_size: 90.0,
                ask_size: 110.0,
                timestamp: 0,
                observed_at_ms: 100_100,
            }),
        },
        100_695,
        None,
    );

    let snapshot = SignalFeatureEngine::compute(
        &mut state,
        Some(&window),
        Some(69_900.0),
        0.0012,
        100_700,
        None,
        &config,
    );

    assert_eq!(snapshot.quote_age_ms, Some(600));
    assert_eq!(snapshot.book_staleness_ms, Some(600));
}

/// Verify that per-leg timestamps and skew are surfaced for live debugging.
#[test]
fn compute_records_leg_timestamps_and_skew() {
    let config = crate::config::Config::default();
    let window = market_window();
    let mut state = SignalState::new();
    state.update_binance_trade(70_000.0, 1.0, None, 100_000, None);
    state.update_chainlink(70_005.0, 100_650, None);
    state.update_clob(
        BookState {
            up: Some(TopOfBook {
                best_bid: 0.48,
                best_ask: 0.49,
                bid_size: 100.0,
                ask_size: 120.0,
                timestamp: 0,
                observed_at_ms: 100_690,
            }),
            down: Some(TopOfBook {
                best_bid: 0.49,
                best_ask: 0.50,
                bid_size: 90.0,
                ask_size: 110.0,
                timestamp: 0,
                observed_at_ms: 100_640,
            }),
        },
        100_690,
        None,
    );

    let snapshot = SignalFeatureEngine::compute(
        &mut state,
        Some(&window),
        Some(69_900.0),
        0.0012,
        100_700,
        None,
        &config,
    );

    assert_eq!(snapshot.up_ask, Some(0.49));
    assert_eq!(snapshot.down_ask, Some(0.50));
    assert_eq!(snapshot.total_ask, Some(0.99));
    assert_eq!(snapshot.up_effective_book_ts_ms, Some(100_690));
    assert_eq!(snapshot.down_effective_book_ts_ms, Some(100_640));
    assert_eq!(snapshot.inter_leg_skew_ms, Some(50));

    let json = snapshot.to_json();
    assert_eq!(json["upAsk"].as_f64(), Some(0.49));
    assert_eq!(json["downAsk"].as_f64(), Some(0.50));
    assert_eq!(json["totalAsk"].as_f64(), Some(0.99));
    assert_eq!(json["upEffectiveBookTsMs"].as_u64(), Some(100_690));
    assert_eq!(json["downEffectiveBookTsMs"].as_u64(), Some(100_640));
    assert_eq!(json["interLegSkewMs"].as_u64(), Some(50));
}

/// Verify that calm-specific realized-volatility, open-cross, and distance-range
/// features are populated from the extended 30-second trade history.
#[test]
fn compute_records_calm_regime_features() {
    let config = crate::config::Config::default();
    let window = market_window();
    let mut state = SignalState::new();

    for (offset_ms, price) in [
        (72_000, 69_990.0),
        (80_000, 70_010.0),
        (88_000, 69_995.0),
        (96_000, 70_015.0),
        (100_000, 70_030.0),
    ] {
        state.update_binance_trade(price, 1.0, Some(1.0), offset_ms, Some(offset_ms * 1_000));
    }
    state.update_chainlink(70_020.0, 100_050, Some(100_050_000));
    state.update_clob(book_state(100_100), 100_100, Some(100_100_000));

    let snapshot = SignalFeatureEngine::compute(
        &mut state,
        Some(&window),
        Some(70_000.0),
        0.0004,
        100_200,
        Some(100_200_000),
        &config,
    );

    assert!(snapshot.realized_vol_5s_bps.is_some());
    assert!(snapshot.realized_vol_15s_bps.is_some());
    assert!(snapshot.realized_vol_30s_bps.is_some());
    assert_eq!(snapshot.open_crosses_10s, Some(0));
    assert_eq!(snapshot.open_crosses_30s, Some(3));
    assert!(snapshot.min_signed_distance_10s_bps.is_some());
    assert!(snapshot.max_signed_distance_10s_bps.is_some());
    assert!(snapshot.min_signed_distance_30s_bps.is_some());
    assert!(snapshot.max_signed_distance_30s_bps.is_some());

    let json = snapshot.to_json();
    assert_eq!(json["openCrosses10s"].as_u64(), Some(0));
    assert_eq!(json["openCrosses30s"].as_u64(), Some(3));
    assert!(json["realizedVol15sBps"].as_f64().is_some());
    assert!(json["maxSignedDistance30sBps"].as_f64().is_some());
}
