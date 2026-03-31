use super::*;
use crate::backtest::tick_replay::TickSample;
use crate::types::ReplayFidelity;

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
        binance: Some(TickSample {
            price: Some(42_000.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
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
        binance: None,
        chainlink: Some(TickSample {
            price: Some(41_999.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
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
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            price: None,
            bid: Some(0.45),
            ask: Some(0.55),
            bid_size: Some(100.0),
            ask_size: Some(200.0),
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
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            price: None,
            bid: None,
            ask: Some(0.55),
            bid_size: None,
            ask_size: Some(200.0),
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
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            price: None,
            bid: Some(0.45),
            ask: None,
            bid_size: Some(100.0),
            ask_size: None,
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
        binance: None,
        chainlink: None,
        clob_up: None,
        clob_down: Some(TickSample {
            price: None,
            bid: Some(0.40),
            ask: Some(0.50),
            bid_size: Some(50.0),
            ask_size: Some(75.0),
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
        binance: Some(TickSample {
            price: Some(42_000.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
        }),
        chainlink: Some(TickSample {
            price: Some(41_999.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
        }),
        clob_up: Some(TickSample {
            price: None,
            bid: Some(0.45),
            ask: Some(0.55),
            bid_size: Some(100.0),
            ask_size: Some(200.0),
        }),
        clob_down: Some(TickSample {
            price: None,
            bid: Some(0.40),
            ask: Some(0.50),
            bid_size: Some(50.0),
            ask_size: Some(75.0),
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
        binance: Some(TickSample {
            price: Some(42_000.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
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
        binance: Some(TickSample {
            price: Some(42_100.0),
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
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
        binance: Some(TickSample {
            price: None,
            bid: None,
            ask: None,
            bid_size: None,
            ask_size: None,
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
        binance: None,
        chainlink: None,
        clob_up: Some(TickSample {
            price: None,
            bid: Some(0.45),
            ask: Some(0.55),
            bid_size: None,
            ask_size: None,
        }),
        clob_down: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    };
    state.update(&group);
    let up = state.book_state.up.as_ref().unwrap();
    assert!((up.bid_size - 0.0).abs() < f64::EPSILON);
    assert!((up.ask_size - 0.0).abs() < f64::EPSILON);
}
