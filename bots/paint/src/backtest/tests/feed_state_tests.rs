use super::*;
use crate::backtest::tick_replay::TickSample;
use crate::signal_features::{FeatureMode, SignalFeatureEngine};
use crate::types::{ReplayFidelity, TopOfBook};

/// Verifies that default state is none.
#[test]
fn default_state_is_none() {
    let state = FeedState::new();
    assert!(state.binance_price.is_none());
    assert!(state.chainlink_price.is_none());
    assert!(state.book_state.up.is_none());
    assert!(state.book_state.down.is_none());
}

/// Verifies that update binance price.
#[test]
fn update_binance_price() {
    let mut state = FeedState::new();
    let group = TickGroup {
        timestamp: 1_000,
        timestamp_us: None,
        binance: Some(TickSample {
            price: Some(42_000.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    assert_eq!(state.binance_price, Some(42_000.0));
    assert!(state.chainlink_price.is_none());
}

/// Verifies that update chainlink price.
#[test]
fn update_chainlink_price() {
    let mut state = FeedState::new();
    let group = TickGroup {
        timestamp: 2_000,
        timestamp_us: None,
        binance: None,
        chainlink: Some(TickSample {
            price: Some(41_999.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
            ..TickSample::default()
        }),
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    assert!(state.binance_price.is_none());
    assert_eq!(state.chainlink_price, Some(41_999.0));
}

/// Verifies that update clob up with bid and ask.
#[test]
fn update_clob_up_with_bid_and_ask() {
    let mut state = FeedState::new();
    let group = TickGroup {
        timestamp: 3_000,
        timestamp_us: None,
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            price: None,
            bid: Some(0.45),
            ask: Some(0.55),
            bid_size: Some(100.0),
            ask_size: Some(200.0),
            ..TickSample::default()
        }),
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    let up = state.book_state.up.as_ref().unwrap();
    assert!((up.best_bid - 0.45).abs() < f64::EPSILON);
    assert!((up.best_ask - 0.55).abs() < f64::EPSILON);
    assert!((up.bid_size - 100.0).abs() < f64::EPSILON);
    assert!((up.ask_size - 200.0).abs() < f64::EPSILON);
    assert_eq!(up.timestamp, 3_000);
}

/// Verifies that clob up ignored when bid missing.
#[test]
fn clob_up_ignored_when_bid_missing() {
    let mut state = FeedState::new();
    let group = TickGroup {
        timestamp: 3_000,
        timestamp_us: None,
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            price: None,
            bid: None,
            ask: Some(0.55),
            bid_size: None,
            ask_size: Some(200.0),
            ..TickSample::default()
        }),
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    assert!(state.book_state.up.is_none());
}

/// Verifies that clob up ignored when ask missing.
#[test]
fn clob_up_ignored_when_ask_missing() {
    let mut state = FeedState::new();
    let group = TickGroup {
        timestamp: 3_000,
        timestamp_us: None,
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            price: None,
            bid: Some(0.45),
            ask: None,
            bid_size: Some(100.0),
            ask_size: None,
            ..TickSample::default()
        }),
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    assert!(state.book_state.up.is_none());
}

/// Verifies that update clob down.
#[test]
fn update_clob_down() {
    let mut state = FeedState::new();
    let group = TickGroup {
        timestamp: 4_000,
        timestamp_us: None,
        binance: None,
        chainlink: None,
        clob_up: None,
        clob_down: Some(TickSample {
            price: None,
            bid: Some(0.40),
            ask: Some(0.50),
            bid_size: Some(50.0),
            ask_size: Some(75.0),
            ..TickSample::default()
        }),
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    let down = state.book_state.down.as_ref().unwrap();
    assert!((down.best_bid - 0.40).abs() < f64::EPSILON);
    assert!((down.best_ask - 0.50).abs() < f64::EPSILON);
}

/// Verifies that update all fields at once.
#[test]
fn update_all_fields_at_once() {
    let mut state = FeedState::new();
    let group = TickGroup {
        timestamp: 5_000,
        timestamp_us: None,
        binance: Some(TickSample {
            price: Some(42_000.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
            ..TickSample::default()
        }),
        chainlink: Some(TickSample {
            price: Some(41_999.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
            ..TickSample::default()
        }),
        clob_up: Some(TickSample {
            price: None,
            bid: Some(0.45),
            ask: Some(0.55),
            bid_size: Some(100.0),
            ask_size: Some(200.0),
            ..TickSample::default()
        }),
        clob_down: Some(TickSample {
            price: None,
            bid: Some(0.40),
            ask: Some(0.50),
            bid_size: Some(50.0),
            ask_size: Some(75.0),
            ..TickSample::default()
        }),
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    assert_eq!(state.binance_price, Some(42_000.0));
    assert_eq!(state.chainlink_price, Some(41_999.0));
    assert!(state.book_state.up.is_some());
    assert!(state.book_state.down.is_some());
}

/// Verifies that subsequent updates overwrite.
#[test]
fn subsequent_updates_overwrite() {
    let mut state = FeedState::new();
    let group1 = TickGroup {
        timestamp: 1_000,
        timestamp_us: None,
        binance: Some(TickSample {
            price: Some(42_000.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group1);
    assert_eq!(state.binance_price, Some(42_000.0));

    let group2 = TickGroup {
        timestamp: 2_000,
        timestamp_us: None,
        binance: Some(TickSample {
            price: Some(42_100.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group2);
    assert_eq!(state.binance_price, Some(42_100.0));
}

/// Verifies that update without price does not clear.
#[test]
fn update_without_price_does_not_clear() {
    let mut state = FeedState::new();
    state.binance_price = Some(42_000.0);
    let group = TickGroup {
        timestamp: 2_000,
        timestamp_us: None,
        binance: Some(TickSample {
            price: None,
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);

    assert_eq!(state.binance_price, Some(42_000.0));
}

/// Verifies that reset clears everything.
#[test]
fn reset_clears_everything() {
    let mut state = FeedState::new();
    state.binance_price = Some(42_000.0);
    state.chainlink_price = Some(41_999.0);
    state.book_state.up = Some(TopOfBook {
        best_bid: 0.45,
        best_ask: 0.55,
        bid_size: 100.0,
        ask_size: 200.0,
        timestamp: 1_000,
        observed_at_ms: 1_000,
    });

    state.reset();
    assert!(state.binance_price.is_none());
    assert!(state.chainlink_price.is_none());
    assert!(state.book_state.up.is_none());
    assert!(state.book_state.down.is_none());
}

/// Verifies that missing bid ask size defaults to zero.
#[test]
fn missing_bid_ask_size_defaults_to_zero() {
    let mut state = FeedState::new();
    let group = TickGroup {
        timestamp: 1_000,
        timestamp_us: None,
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            price: None,
            bid: Some(0.45),
            ask: Some(0.55),
            bid_size: None,
            ask_size: None,
            ..TickSample::default()
        }),
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    let up = state.book_state.up.as_ref().unwrap();
    assert!((up.bid_size - 0.0).abs() < f64::EPSILON);
    assert!((up.ask_size - 0.0).abs() < f64::EPSILON);
}

/// Verifies that raw replay samples populate full signal features.
#[test]
fn raw_replay_samples_populate_full_signal_features() {
    let mut state = FeedState::new();
    state.update(&TickGroup {
        timestamp: 1_000,
        timestamp_us: Some(1_000_000),
        binance: Some(TickSample {
            event_type: "aggTrade".to_string(),
            price: Some(42_000.0),
            trade_size: Some(1.0),
            signed_quantity: Some(1.0),
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::RawEvent,
    });
    state.update(&TickGroup {
        timestamp: 1_001,
        timestamp_us: Some(1_001_000),
        binance: Some(TickSample {
            event_type: "aggTrade".to_string(),
            price: Some(42_002.0),
            trade_size: Some(2.0),
            signed_quantity: Some(-2.0),
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::RawEvent,
    });
    state.update(&TickGroup {
        timestamp: 1_002,
        timestamp_us: Some(1_002_000),
        binance: Some(TickSample {
            event_type: "depth".to_string(),
            bid: Some(42_001.0),
            ask: Some(42_003.0),
            bid_size: Some(5.0),
            ask_size: Some(3.0),
            depth_bid_notional: Some(210_005.0),
            depth_ask_notional: Some(126_009.0),
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::RawEvent,
    });
    state.update(&TickGroup {
        timestamp: 1_003,
        timestamp_us: Some(1_003_000),
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            bid: Some(0.48),
            ask: Some(0.50),
            bid_size: Some(100.0),
            ask_size: Some(100.0),
            ..TickSample::default()
        }),
        clob_down: None,
        fidelity: ReplayFidelity::RawEvent,
    });
    state.update(&TickGroup {
        timestamp: 1_004,
        timestamp_us: Some(1_004_000),
        binance: None,
        chainlink: None,
        clob_up: None,
        clob_down: Some(TickSample {
            bid: Some(0.49),
            ask: Some(0.51),
            bid_size: Some(100.0),
            ask_size: Some(100.0),
            ..TickSample::default()
        }),
        fidelity: ReplayFidelity::RawEvent,
    });

    let config = crate::config::Config::default();
    let features = SignalFeatureEngine::compute(
        &mut state.signal_state,
        None,
        None,
        0.0,
        1_004,
        Some(1_004_500),
        &config,
    );

    assert_eq!(features.feature_mode, FeatureMode::RawEventFull);
    assert_eq!(features.binance_signed_trade_imbalance, Some(-1.0 / 3.0));
    assert!(features.binance_book_imbalance.is_some());
    assert!(features.binance_depth_sweep_cost.is_some());
    assert!(features.polymarket_quote_churn_per_s.is_some());
    assert!(features.polymarket_microprice_skew.is_some());
    assert_eq!(features.event_to_decision_lag_us, Some(500));
}

/// Verifies that full-debug book ticker state is not overwritten by depth rows.
#[test]
fn depth_does_not_overwrite_replayed_book_ticker_state() {
    let mut state = FeedState::new();
    state.update(&TickGroup {
        timestamp: 1_000,
        timestamp_us: Some(1_000_000),
        binance: Some(TickSample {
            event_type: "bookTicker".to_string(),
            bid: Some(42_000.0),
            ask: Some(42_001.0),
            bid_size: Some(8.0),
            ask_size: Some(2.0),
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::RawEvent,
    });
    state.update(&TickGroup {
        timestamp: 1_001,
        timestamp_us: Some(1_001_000),
        binance: Some(TickSample {
            event_type: "depth".to_string(),
            bid: Some(41_900.0),
            ask: Some(41_901.0),
            bid_size: Some(1.0),
            ask_size: Some(9.0),
            depth_bid_notional: Some(41_900.0),
            depth_ask_notional: Some(377_109.0),
            ..TickSample::default()
        }),
        chainlink: None,
        clob_up: None,
        clob_down: None,
        fidelity: ReplayFidelity::RawEvent,
    });

    let book = state.signal_state.binance_book.as_ref().unwrap();
    assert_eq!(book.best_bid, 42_000.0);
    assert_eq!(book.best_ask, 42_001.0);
    assert!(state.signal_state.binance_depth.is_some());
}
