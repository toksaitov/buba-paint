use super::*;

/// Parse one agg-trade payload and return the resulting feed message.
fn parse_trade_message(payload: serde_json::Value) -> FeedMessage {
    parse_trade(&payload, None, "binance-test", &payload.to_string(), true).unwrap()
}

/// Verify that a valid agg-trade payload becomes a Binance trade message.
#[test]
fn parse_valid_agg_trade() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "E": 1_700_000_000_000_u64,
        "s": "BTCUSDT",
        "a": 123_456,
        "p": "42000.50",
        "q": "0.001",
        "f": 100,
        "l": 101,
        "T": 1_700_000_000_001_u64,
        "m": false,
        "M": true
    });
    match parse_trade_message(json) {
        FeedMessage::BinanceTrade {
            price,
            quantity,
            signed_quantity,
            timestamp_ms,
            source_symbol,
            ..
        } => {
            assert!((price - 42_000.50).abs() < f64::EPSILON);
            assert!((quantity - 0.001).abs() < f64::EPSILON);
            assert_eq!(signed_quantity, Some(0.001));
            assert_eq!(timestamp_ms, 1_700_000_000_001);
            assert_eq!(source_symbol.as_deref(), Some("BTCUSDT"));
        }
        other => panic!("expected BinanceTrade, got {other:?}"),
    }
}

/// Verify that agg-trade parsing falls back to event time when trade time is absent.
#[test]
fn parse_agg_trade_uses_event_time_as_fallback() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "E": 1_700_000_000_000_u64,
        "p": "42000.50",
        "q": "0.001"
    });
    match parse_trade_message(json) {
        FeedMessage::BinanceTrade { timestamp_ms, .. } => {
            assert_eq!(timestamp_ms, 1_700_000_000_000);
        }
        other => panic!("expected BinanceTrade, got {other:?}"),
    }
}

/// Verify that agg-trade parsing uses zero when both timestamps are absent.
#[test]
fn parse_agg_trade_zero_timestamp_when_both_missing() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": "42000.50",
        "q": "0.001"
    });
    match parse_trade_message(json) {
        FeedMessage::BinanceTrade { timestamp_ms, .. } => {
            assert_eq!(timestamp_ms, 0);
        }
        other => panic!("expected BinanceTrade, got {other:?}"),
    }
}

/// Verify that non-agg-trade payloads are ignored by the trade parser.
#[test]
fn parse_non_agg_trade_event_returns_none() {
    let json = serde_json::json!({
        "e": "trade",
        "p": "42000.50",
        "q": "0.001",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_trade(&json, None, "binance-test", &json.to_string(), true).is_none());
}

/// Verify that missing event types are ignored by the trade parser.
#[test]
fn parse_missing_event_type_returns_none() {
    let json = serde_json::json!({
        "p": "42000.50",
        "q": "0.001",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_trade(&json, None, "binance-test", &json.to_string(), true).is_none());
}

/// Verify that missing prices are rejected.
#[test]
fn parse_missing_price_returns_none() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "q": "0.001",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_trade(&json, None, "binance-test", &json.to_string(), true).is_none());
}

/// Verify that invalid prices are rejected.
#[test]
fn parse_invalid_price_returns_none() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": "not_a_number",
        "q": "0.001",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_trade(&json, None, "binance-test", &json.to_string(), true).is_none());
}

/// Verify that numeric price payloads are accepted by the generic float parser.
#[test]
fn parse_numeric_price_returns_trade() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": 42000.50,
        "q": "0.001",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_trade(&json, None, "binance-test", &json.to_string(), true).is_some());
}

/// Verify that raw text parsing produces a Binance trade message.
#[test]
fn process_binance_text_valid_agg_trade() {
    let text = r#"{"e":"aggTrade","E":1700000000000,"s":"BTCUSDT","a":123456,"p":"42000.50","q":"0.001","f":100,"l":101,"T":1700000000001,"m":false,"M":true}"#;
    match process_binance_text(text, "binance-test", true).unwrap() {
        FeedMessage::BinanceTrade {
            price,
            timestamp_ms,
            ..
        } => {
            assert!((price - 42_000.50).abs() < f64::EPSILON);
            assert_eq!(timestamp_ms, 1_700_000_000_001);
        }
        other => panic!("expected BinanceTrade, got {other:?}"),
    }
}

/// Verify that malformed raw text is rejected.
#[test]
fn process_binance_text_invalid_json_returns_none() {
    assert!(process_binance_text("not json at all", "binance-test", true).is_none());
}

/// Verify that empty raw text is rejected.
#[test]
fn process_binance_text_empty_string_returns_none() {
    assert!(process_binance_text("", "binance-test", true).is_none());
}

/// Verify that non-supported event types are ignored by the frame parser.
#[test]
fn process_binance_text_non_agg_trade_returns_none() {
    let text = r#"{"e":"trade","p":"42000.50","T":1700000000000}"#;
    assert!(process_binance_text(text, "binance-test", true).is_none());
}

/// Verify that empty price strings are rejected.
#[test]
fn parse_agg_trade_empty_price_string() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": "",
        "q": "0.001",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_trade(&json, None, "binance-test", &json.to_string(), true).is_none());
}

/// Verify that infinite prices still parse as floating-point infinity.
#[test]
fn parse_agg_trade_infinity_price() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": "inf",
        "q": "0.001",
        "T": 1_700_000_000_000_u64,
    });
    match parse_trade_message(json) {
        FeedMessage::BinanceTrade { price, .. } => {
            assert!(price.is_infinite());
        }
        other => panic!("expected BinanceTrade, got {other:?}"),
    }
}

/// Verify that numeric-price frames are accepted by the top-level parser too.
#[test]
fn process_binance_text_numeric_price_returns_trade() {
    let text = r#"{"e":"aggTrade","p":42000.50,"q":"0.001","T":1700000000000}"#;
    assert!(process_binance_text(text, "binance-test", true).is_some());
}

/// Verify that compact parsing avoids cloning raw payloads.
#[test]
fn process_binance_text_compact_mode_omits_payloads() {
    let text = r#"{"e":"aggTrade","E":1700000000000,"s":"BTCUSDT","a":123456,"p":"42000.50","q":"0.001","T":1700000000001,"m":false}"#;
    match process_binance_text(text, "binance-test", false).unwrap() {
        FeedMessage::BinanceTrade {
            payload_json,
            details_json,
            ..
        } => {
            assert!(payload_json.is_none());
            assert!(details_json.is_none());
        }
        other => panic!("expected BinanceTrade, got {other:?}"),
    }
}
