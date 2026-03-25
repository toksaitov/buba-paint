use super::*;
use crate::types::TopOfBook;

#[test]
fn empty_state_zero_entries() {
    let state = TickLoggerState::default();
    let entries = build_tick_entries(&state);
    assert!(entries.is_empty());
}

#[test]
fn only_binance_one_entry() {
    let state = TickLoggerState {
        binance_price: Some(42_000.0),
        chainlink_price: None,
        book_state: BookState::default(),
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "binance");
    assert!((entries[0].price.unwrap() - 42_000.0).abs() < f64::EPSILON);
    assert!(entries[0].best_bid.is_none());
}

#[test]
fn only_chainlink_one_entry() {
    let state = TickLoggerState {
        binance_price: None,
        chainlink_price: Some(41_999.0),
        book_state: BookState::default(),
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "chainlink");
    assert!((entries[0].price.unwrap() - 41_999.0).abs() < f64::EPSILON);
}

#[test]
fn all_sources_four_entries() {
    let state = TickLoggerState {
        binance_price: Some(42_000.0),
        chainlink_price: Some(41_999.0),
        book_state: BookState {
            up: Some(TopOfBook {
                best_bid: 0.45,
                best_ask: 0.55,
                bid_size: 100.0,
                ask_size: 200.0,
                timestamp: 0,
            }),
            down: Some(TopOfBook {
                best_bid: 0.44,
                best_ask: 0.56,
                bid_size: 150.0,
                ask_size: 250.0,
                timestamp: 0,
            }),
        },
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 4);

    assert_eq!(entries[0].source, "binance");
    assert!(entries[0].price.is_some());
    assert!(entries[0].best_bid.is_none());

    assert_eq!(entries[1].source, "chainlink");
    assert!(entries[1].price.is_some());

    assert_eq!(entries[2].source, "clob_up");
    assert!(entries[2].price.is_none());
    assert!((entries[2].best_bid.unwrap() - 0.45).abs() < f64::EPSILON);
    assert!((entries[2].best_ask.unwrap() - 0.55).abs() < f64::EPSILON);
    assert!((entries[2].bid_size.unwrap() - 100.0).abs() < f64::EPSILON);
    assert!((entries[2].ask_size.unwrap() - 200.0).abs() < f64::EPSILON);

    assert_eq!(entries[3].source, "clob_down");
    assert!((entries[3].best_bid.unwrap() - 0.44).abs() < f64::EPSILON);
    assert!((entries[3].best_ask.unwrap() - 0.56).abs() < f64::EPSILON);
}

#[test]
fn only_book_up_two_entries_skipped_prices() {
    let state = TickLoggerState {
        binance_price: None,
        chainlink_price: None,
        book_state: BookState {
            up: Some(TopOfBook {
                best_bid: 0.50,
                best_ask: 0.52,
                bid_size: 500.0,
                ask_size: 300.0,
                timestamp: 0,
            }),
            down: None,
        },
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "clob_up");
}

#[test]
fn prices_and_one_book_side_three_entries() {
    let state = TickLoggerState {
        binance_price: Some(42_000.0),
        chainlink_price: Some(41_999.0),
        book_state: BookState {
            up: None,
            down: Some(TopOfBook {
                best_bid: 0.44,
                best_ask: 0.56,
                bid_size: 150.0,
                ask_size: 250.0,
                timestamp: 0,
            }),
        },
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].source, "binance");
    assert_eq!(entries[1].source, "chainlink");
    assert_eq!(entries[2].source, "clob_down");
}

#[test]
fn entry_order_is_deterministic() {
    let state = TickLoggerState {
        binance_price: Some(1.0),
        chainlink_price: Some(2.0),
        book_state: BookState {
            up: Some(TopOfBook {
                best_bid: 0.1,
                best_ask: 0.2,
                bid_size: 10.0,
                ask_size: 20.0,
                timestamp: 0,
            }),
            down: Some(TopOfBook {
                best_bid: 0.3,
                best_ask: 0.4,
                bid_size: 30.0,
                ask_size: 40.0,
                timestamp: 0,
            }),
        },
    };
    let sources: Vec<&str> = build_tick_entries(&state)
        .iter()
        .map(|e| e.source)
        .collect();
    assert_eq!(
        sources,
        vec!["binance", "chainlink", "clob_up", "clob_down"]
    );
}

// -- Phase D: additional edge-case tests ----------------------------------

#[test]
fn only_down_book_one_entry_clob_down() {
    let state = TickLoggerState {
        binance_price: None,
        chainlink_price: None,
        book_state: BookState {
            up: None,
            down: Some(TopOfBook {
                best_bid: 0.44,
                best_ask: 0.56,
                bid_size: 150.0,
                ask_size: 250.0,
                timestamp: 0,
            }),
        },
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "clob_down");
    assert!(entries[0].price.is_none());
    assert!((entries[0].best_bid.unwrap() - 0.44).abs() < f64::EPSILON);
    assert!((entries[0].best_ask.unwrap() - 0.56).abs() < f64::EPSILON);
    assert!((entries[0].bid_size.unwrap() - 150.0).abs() < f64::EPSILON);
    assert!((entries[0].ask_size.unwrap() - 250.0).abs() < f64::EPSILON);
}

#[test]
fn zero_binance_price_still_generates_entry() {
    let state = TickLoggerState {
        binance_price: Some(0.0),
        chainlink_price: None,
        book_state: BookState::default(),
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "binance");
    assert!((entries[0].price.unwrap() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn zero_chainlink_price_still_generates_entry() {
    let state = TickLoggerState {
        binance_price: None,
        chainlink_price: Some(0.0),
        book_state: BookState::default(),
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "chainlink");
    assert!((entries[0].price.unwrap() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn both_books_no_prices_two_entries() {
    let state = TickLoggerState {
        binance_price: None,
        chainlink_price: None,
        book_state: BookState {
            up: Some(TopOfBook {
                best_bid: 0.45,
                best_ask: 0.55,
                bid_size: 100.0,
                ask_size: 200.0,
                timestamp: 0,
            }),
            down: Some(TopOfBook {
                best_bid: 0.44,
                best_ask: 0.56,
                bid_size: 150.0,
                ask_size: 250.0,
                timestamp: 0,
            }),
        },
    };
    let entries = build_tick_entries(&state);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].source, "clob_up");
    assert_eq!(entries[1].source, "clob_down");
}

#[test]
fn tick_logger_state_default_is_all_none() {
    let state = TickLoggerState::default();
    assert!(state.binance_price.is_none());
    assert!(state.chainlink_price.is_none());
    assert!(state.book_state.up.is_none());
    assert!(state.book_state.down.is_none());
}
