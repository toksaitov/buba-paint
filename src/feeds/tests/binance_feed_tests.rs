use super::*;

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
    let result = parse_agg_trade(&json);
    assert!(result.is_some());
    let (price, timestamp) = result.unwrap();
    assert!((price - 42_000.50).abs() < f64::EPSILON);
    assert_eq!(timestamp, 1_700_000_000_001);
}

#[test]
fn parse_agg_trade_uses_event_time_as_fallback() {
    // When "T" is missing, fall back to "E".
    let json = serde_json::json!({
        "e": "aggTrade",
        "E": 1_700_000_000_000_u64,
        "p": "42000.50",
    });
    let result = parse_agg_trade(&json);
    assert!(result.is_some());
    let (_, timestamp) = result.unwrap();
    assert_eq!(timestamp, 1_700_000_000_000);
}

#[test]
fn parse_agg_trade_zero_timestamp_when_both_missing() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": "42000.50",
    });
    let result = parse_agg_trade(&json);
    assert!(result.is_some());
    let (_, timestamp) = result.unwrap();
    assert_eq!(timestamp, 0);
}

#[test]
fn parse_non_agg_trade_event_returns_none() {
    let json = serde_json::json!({
        "e": "trade",
        "p": "42000.50",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_agg_trade(&json).is_none());
}

#[test]
fn parse_missing_event_type_returns_none() {
    let json = serde_json::json!({
        "p": "42000.50",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_agg_trade(&json).is_none());
}

#[test]
fn parse_missing_price_returns_none() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_agg_trade(&json).is_none());
}

#[test]
fn parse_invalid_price_returns_none() {
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": "not_a_number",
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_agg_trade(&json).is_none());
}

#[test]
fn parse_numeric_price_returns_none() {
    // Binance sends price as a string, not a number.
    // Our parser specifically reads it as a string, so a raw number fails.
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": 42000.50,
        "T": 1_700_000_000_000_u64,
    });
    assert!(parse_agg_trade(&json).is_none());
}

// -- process_binance_text -------------------------------------------------

#[test]
fn process_binance_text_valid_agg_trade() {
    let text = r#"{"e":"aggTrade","E":1700000000000,"s":"BTCUSDT","a":123456,"p":"42000.50","q":"0.001","f":100,"l":101,"T":1700000000001,"m":false,"M":true}"#;
    let msg = process_binance_text(text);
    assert!(msg.is_some());
    match msg.unwrap() {
        FeedMessage::BinanceTick { price, timestamp } => {
            assert!((price - 42_000.50).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_001);
        }
        other => panic!("expected BinanceTick, got {other:?}"),
    }
}

#[test]
fn process_binance_text_invalid_json_returns_none() {
    assert!(process_binance_text("not json at all").is_none());
}

#[test]
fn process_binance_text_empty_string_returns_none() {
    assert!(process_binance_text("").is_none());
}

#[test]
fn process_binance_text_non_agg_trade_returns_none() {
    let text = r#"{"e":"trade","p":"42000.50","T":1700000000000}"#;
    assert!(process_binance_text(text).is_none());
}

#[test]
fn parse_agg_trade_empty_price_string() {
    // price="" should fail to parse as f64 → returns None.
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": "",
        "T": 1_700_000_000_000_u64,
    });
    assert!(
        parse_agg_trade(&json).is_none(),
        "empty price string should return None"
    );
}

#[test]
fn parse_agg_trade_infinity_price() {
    // "inf" parses as f64::INFINITY in Rust.
    // Our parser does `s.parse::<f64>().ok()` which succeeds for "inf".
    let json = serde_json::json!({
        "e": "aggTrade",
        "p": "inf",
        "T": 1_700_000_000_000_u64,
    });
    let result = parse_agg_trade(&json);
    // "inf" successfully parses to f64::INFINITY
    assert!(result.is_some(), "\"inf\" parses as f64::INFINITY");
    let (price, _) = result.unwrap();
    assert!(price.is_infinite(), "price should be infinity, got {price}");
}

#[test]
fn process_binance_text_numeric_price_returns_none() {
    // Binance sends price as string; numeric price is not handled by parse_agg_trade.
    let text = r#"{"e":"aggTrade","p":42000.50,"T":1700000000000}"#;
    assert!(process_binance_text(text).is_none());
}
