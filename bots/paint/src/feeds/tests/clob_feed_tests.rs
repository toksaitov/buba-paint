use super::*;

/// Verifies that extract best bid highest price wins.
#[test]
fn extract_best_bid_highest_price_wins() {
    let levels = serde_json::json!([
        {"price": "0.40", "size": "100"},
        {"price": "0.45", "size": "200"},
        {"price": "0.42", "size": "50"},
    ]);
    let (price, size) = extract_best_level(Some(&levels), true);
    assert!((price - 0.45).abs() < f64::EPSILON);
    assert!((size - 200.0).abs() < f64::EPSILON);
}

/// Verifies that extract best ask lowest price wins.
#[test]
fn extract_best_ask_lowest_price_wins() {
    let levels = serde_json::json!([
        {"price": "0.55", "size": "100"},
        {"price": "0.52", "size": "300"},
        {"price": "0.60", "size": "50"},
    ]);
    let (price, size) = extract_best_level(Some(&levels), false);
    assert!((price - 0.52).abs() < f64::EPSILON);
    assert!((size - 300.0).abs() < f64::EPSILON);
}

/// Verifies that extract best level empty array returns zero.
#[test]
fn extract_best_level_empty_array_returns_zero() {
    let levels = serde_json::json!([]);
    let (price, size) = extract_best_level(Some(&levels), true);
    assert!((price - 0.0).abs() < f64::EPSILON);
    assert!((size - 0.0).abs() < f64::EPSILON);
}

/// Verifies that extract best level none returns zero.
#[test]
fn extract_best_level_none_returns_zero() {
    let (price, size) = extract_best_level(None, true);
    assert!((price - 0.0).abs() < f64::EPSILON);
    assert!((size - 0.0).abs() < f64::EPSILON);
}

/// Verifies that extract best level none ask returns zero.
#[test]
fn extract_best_level_none_ask_returns_zero() {
    let (price, size) = extract_best_level(None, false);
    assert!((price - 0.0).abs() < f64::EPSILON);
    assert!((size - 0.0).abs() < f64::EPSILON);
}

/// Verifies that extract best level single bid.
#[test]
fn extract_best_level_single_bid() {
    let levels = serde_json::json!([{"price": "0.50", "size": "500"}]);
    let (price, size) = extract_best_level(Some(&levels), true);
    assert!((price - 0.50).abs() < f64::EPSILON);
    assert!((size - 500.0).abs() < f64::EPSILON);
}

/// Verifies that extract best level single ask.
#[test]
fn extract_best_level_single_ask() {
    let levels = serde_json::json!([{"price": "0.55", "size": "250"}]);
    let (price, size) = extract_best_level(Some(&levels), false);
    assert!((price - 0.55).abs() < f64::EPSILON);
    assert!((size - 250.0).abs() < f64::EPSILON);
}

/// Verifies that parse f64 field number value.
#[test]
fn parse_f64_field_number_value() {
    let v = serde_json::json!({"price": 0.45});
    assert!((parse_f64_field(&v, "price").unwrap() - 0.45).abs() < f64::EPSILON);
}

/// Verifies that parse f64 field string value.
#[test]
fn parse_f64_field_string_value() {
    let v = serde_json::json!({"price": "0.45"});
    assert!((parse_f64_field(&v, "price").unwrap() - 0.45).abs() < f64::EPSILON);
}

/// Verifies that parse f64 field missing field returns none.
#[test]
fn parse_f64_field_missing_field_returns_none() {
    let v = serde_json::json!({"size": 100});
    assert!(parse_f64_field(&v, "price").is_none());
}

/// Verifies that parse f64 field non numeric string returns none.
#[test]
fn parse_f64_field_non_numeric_string_returns_none() {
    let v = serde_json::json!({"price": "abc"});
    assert!(parse_f64_field(&v, "price").is_none());
}

/// Verifies that parse f64 field null value returns none.
#[test]
fn parse_f64_field_null_value_returns_none() {
    let v = serde_json::json!({"price": null});
    assert!(parse_f64_field(&v, "price").is_none());
}

/// Verifies that parse clob event price change.
#[test]
fn parse_clob_event_price_change() {
    let v = serde_json::json!({
        "event_type": "price_change",
        "timestamp": 1_700_000_000_000_u64,
        "price_changes": [
            {"asset_id": "tok-up", "side": "BUY", "price": "0.45", "size": "100"},
            {"asset_id": "tok-down", "side": "SELL", "price": "0.55", "size": "200"},
        ]
    });
    let update = parse_clob_event(&v);
    match update {
        ClobUpdate::PriceChange { timestamp, changes } => {
            assert_eq!(timestamp, 1_700_000_000_000);
            assert_eq!(changes.len(), 2);
            assert_eq!(changes[0].asset_id, "tok-up");
            assert_eq!(changes[0].side, "BUY");
            assert!((changes[0].price - 0.45).abs() < f64::EPSILON);
            assert_eq!(changes[1].asset_id, "tok-down");
            assert_eq!(changes[1].side, "SELL");
        }
        other => panic!("expected PriceChange, got {other:?}"),
    }
}

/// Verifies that parse clob event book snapshot.
#[test]
fn parse_clob_event_book_snapshot() {
    let v = serde_json::json!({
        "asset_id": "tok-up",
        "timestamp": 1_700_000_000_000_u64,
        "bids": [
            {"price": "0.44", "size": "100"},
            {"price": "0.45", "size": "200"},
        ],
        "asks": [
            {"price": "0.55", "size": "150"},
            {"price": "0.56", "size": "300"},
        ]
    });
    let update = parse_clob_event(&v);
    match update {
        ClobUpdate::BookSnapshot {
            asset_id,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            timestamp,
        } => {
            assert_eq!(asset_id, "tok-up");
            assert!((best_bid - 0.45).abs() < f64::EPSILON);
            assert!((best_ask - 0.55).abs() < f64::EPSILON);
            assert!((bid_size - 200.0).abs() < f64::EPSILON);
            assert!((ask_size - 150.0).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_000);
        }
        other => panic!("expected BookSnapshot, got {other:?}"),
    }
}

/// Verifies that parse clob event last trade price ignored.
#[test]
fn parse_clob_event_last_trade_price_ignored() {
    let v = serde_json::json!({
        "event_type": "last_trade_price",
        "price": "0.50"
    });
    assert_eq!(parse_clob_event(&v), ClobUpdate::Ignored);
}

/// Verifies that parse clob event unknown event ignored.
#[test]
fn parse_clob_event_unknown_event_ignored() {
    let v = serde_json::json!({"event_type": "something_else"});
    assert_eq!(parse_clob_event(&v), ClobUpdate::Ignored);
}

/// Verifies that parse clob event no event type no asset ignored.
#[test]
fn parse_clob_event_no_event_type_no_asset_ignored() {
    let v = serde_json::json!({"foo": "bar"});
    assert_eq!(parse_clob_event(&v), ClobUpdate::Ignored);
}

/// Verifies that parse clob text json array multiple events.
#[test]
fn parse_clob_text_json_array_multiple_events() {
    let text = serde_json::to_string(&serde_json::json!([
        {
            "event_type": "last_trade_price",
            "price": "0.50"
        },
        {
            "asset_id": "tok-down",
            "timestamp": 100,
            "bids": [{"price": "0.40", "size": "50"}],
            "asks": [{"price": "0.60", "size": "75"}]
        }
    ]))
    .unwrap();

    let updates = parse_clob_text(&text);
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0], ClobUpdate::Ignored);
    match &updates[1] {
        ClobUpdate::BookSnapshot { asset_id, .. } => {
            assert_eq!(asset_id, "tok-down");
        }
        other => panic!("expected BookSnapshot, got {other:?}"),
    }
}

/// Verifies that parse clob text invalid json returns empty.
#[test]
fn parse_clob_text_invalid_json_returns_empty() {
    let updates = parse_clob_text("not json");
    assert!(updates.is_empty());
}

/// Verifies that parse clob text empty string returns empty.
#[test]
fn parse_clob_text_empty_string_returns_empty() {
    let updates = parse_clob_text("");
    assert!(updates.is_empty());
}

/// Verifies that parse clob text single object.
#[test]
fn parse_clob_text_single_object() {
    let text = r#"{"event_type": "last_trade_price", "price": "0.50"}"#;
    let updates = parse_clob_text(text);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0], ClobUpdate::Ignored);
}

/// Verifies that parse clob event price change empty changes.
#[test]
fn parse_clob_event_price_change_empty_changes() {
    let v = serde_json::json!({
        "event_type": "price_change",
        "timestamp": 100,
        "price_changes": []
    });
    let update = parse_clob_event(&v);
    match update {
        ClobUpdate::PriceChange { timestamp, changes } => {
            assert_eq!(timestamp, 100);
            assert!(changes.is_empty());
        }
        other => panic!("expected PriceChange, got {other:?}"),
    }
}

/// Verifies that parse clob event price change no price changes field.
#[test]
fn parse_clob_event_price_change_no_price_changes_field() {
    let v = serde_json::json!({
        "event_type": "price_change",
        "timestamp": 100
    });
    let update = parse_clob_event(&v);
    match update {
        ClobUpdate::PriceChange { timestamp, changes } => {
            assert_eq!(timestamp, 100);
            assert!(changes.is_empty());
        }
        other => panic!("expected PriceChange, got {other:?}"),
    }
}

/// Verifies that parse clob event book snapshot only bids.
#[test]
fn parse_clob_event_book_snapshot_only_bids() {
    let v = serde_json::json!({
        "asset_id": "tok-up",
        "timestamp": 50,
        "bids": [{"price": "0.45", "size": "100"}]
    });
    match parse_clob_event(&v) {
        ClobUpdate::BookSnapshot {
            best_bid, best_ask, ..
        } => {
            assert!((best_bid - 0.45).abs() < f64::EPSILON);

            assert!((best_ask - 0.0).abs() < f64::EPSILON);
        }
        other => panic!("expected BookSnapshot, got {other:?}"),
    }
}

/// Verifies that parse clob event book snapshot only asks.
#[test]
fn parse_clob_event_book_snapshot_only_asks() {
    let v = serde_json::json!({
        "asset_id": "tok-up",
        "timestamp": 50,
        "asks": [{"price": "0.55", "size": "100"}]
    });
    match parse_clob_event(&v) {
        ClobUpdate::BookSnapshot {
            best_bid, best_ask, ..
        } => {
            assert!((best_bid - 0.0).abs() < f64::EPSILON);
            assert!((best_ask - 0.55).abs() < f64::EPSILON);
        }
        other => panic!("expected BookSnapshot, got {other:?}"),
    }
}

/// Verifies that process clob message invalid json returns err.
#[tokio::test]
async fn process_clob_message_invalid_json_returns_err() {
    let (tx, _rx) = mpsc::channel::<FeedMessage>(16);
    let mut book_state = BookState::default();
    let result =
        process_clob_message("not valid json", "tok-up", "tok-down", &mut book_state, &tx).await;
    assert!(result.is_err(), "invalid JSON should return Err");
}

/// Verifies that process clob message ignored event returns ok.
#[tokio::test]
async fn process_clob_message_ignored_event_returns_ok() {
    let (tx, _rx) = mpsc::channel::<FeedMessage>(16);
    let mut book_state = BookState::default();

    let text = r#"{"event_type": "last_trade_price", "price": "0.50"}"#;
    let result = process_clob_message(text, "tok-up", "tok-down", &mut book_state, &tx).await;
    assert!(result.is_ok(), "ignored event should return Ok");
}

/// Verifies that process clob message price change updates book state.
#[tokio::test]
async fn process_clob_message_price_change_updates_book_state() {
    let (tx, mut rx) = mpsc::channel::<FeedMessage>(16);
    let mut book_state = BookState::default();
    let text = serde_json::json!({
        "event_type": "price_change",
        "timestamp": 1000,
        "price_changes": [
            {"asset_id": "tok-up", "side": "BUY", "price": "0.45", "size": "100"},
            {"asset_id": "tok-down", "side": "SELL", "price": "0.55", "size": "200"},
        ]
    })
    .to_string();
    let result = process_clob_message(&text, "tok-up", "tok-down", &mut book_state, &tx).await;
    assert!(result.is_ok());

    let up = book_state.up.as_ref().expect("up book should be set");
    assert!((up.best_bid - 0.45).abs() < f64::EPSILON);
    let down = book_state.down.as_ref().expect("down book should be set");
    assert!((down.best_ask - 0.55).abs() < f64::EPSILON);

    let msg = rx.try_recv().expect("should have received a message");
    assert!(matches!(msg, FeedMessage::ClobPriceChange { .. }));
}

/// Verifies that process clob message book snapshot updates book state.
#[tokio::test]
async fn process_clob_message_book_snapshot_updates_book_state() {
    let (tx, mut rx) = mpsc::channel::<FeedMessage>(16);
    let mut book_state = BookState::default();
    let text = serde_json::json!({
        "asset_id": "tok-up",
        "timestamp": 500,
        "bids": [{"price": "0.44", "size": "100"}],
        "asks": [{"price": "0.56", "size": "200"}]
    })
    .to_string();
    let result = process_clob_message(&text, "tok-up", "tok-down", &mut book_state, &tx).await;
    assert!(result.is_ok());

    let up = book_state.up.as_ref().expect("up book should be set");
    assert!((up.best_bid - 0.44).abs() < f64::EPSILON);
    assert!((up.best_ask - 0.56).abs() < f64::EPSILON);

    let msg = rx.try_recv().expect("should have received a message");
    assert!(matches!(msg, FeedMessage::ClobBook { .. }));
}

/// Verifies that parse clob event negative price.
#[test]
fn parse_clob_event_negative_price() {
    let v = serde_json::json!({
        "event_type": "price_change",
        "timestamp": 1000,
        "price_changes": [
            {"asset_id": "tok-up", "side": "BUY", "price": "-1.0", "size": "100"}
        ]
    });
    let update = parse_clob_event(&v);
    match update {
        ClobUpdate::PriceChange { changes, .. } => {
            assert_eq!(changes.len(), 1);
            assert!(
                (changes[0].price - (-1.0)).abs() < f64::EPSILON,
                "negative price should parse as -1.0, got {}",
                changes[0].price
            );
        }
        other => panic!("expected PriceChange, got {other:?}"),
    }
}

/// Verifies that parse clob event lowercase side.
#[test]
fn parse_clob_event_lowercase_side() {
    let v = serde_json::json!({
        "event_type": "price_change",
        "timestamp": 1000,
        "price_changes": [
            {"asset_id": "tok-up", "side": "buy", "price": "0.45", "size": "100"}
        ]
    });
    let update = parse_clob_event(&v);
    match update {
        ClobUpdate::PriceChange { changes, .. } => {
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].side, "buy", "side should be stored as-is");

            assert_ne!(changes[0].side, "BUY");
        }
        other => panic!("expected PriceChange, got {other:?}"),
    }
}

/// Verifies that process clob message only ignored events.
#[tokio::test]
async fn process_clob_message_only_ignored_events() {
    let (tx, _rx) = mpsc::channel::<FeedMessage>(16);
    let mut book_state = BookState::default();
    let text = serde_json::json!([
        {"event_type": "last_trade_price", "price": "0.50"},
        {"event_type": "last_trade_price", "price": "0.51"}
    ])
    .to_string();
    let result = process_clob_message(&text, "tok-up", "tok-down", &mut book_state, &tx).await;
    assert!(
        result.is_ok(),
        "array of only ignored events should return Ok, got {result:?}"
    );
}

/// Verifies that process clob message unrelated asset id not stored.
#[tokio::test]
async fn process_clob_message_unrelated_asset_id_not_stored() {
    let (tx, _rx) = mpsc::channel::<FeedMessage>(16);
    let mut book_state = BookState::default();
    let text = serde_json::json!({
        "event_type": "price_change",
        "timestamp": 1000,
        "price_changes": [
            {"asset_id": "tok-other", "side": "BUY", "price": "0.45", "size": "100"},
        ]
    })
    .to_string();
    let result = process_clob_message(&text, "tok-up", "tok-down", &mut book_state, &tx).await;
    assert!(result.is_ok());

    assert!(book_state.up.is_none());
    assert!(book_state.down.is_none());
}
