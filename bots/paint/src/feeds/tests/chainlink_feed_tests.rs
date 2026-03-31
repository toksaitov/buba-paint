use super::*;

/// Verifies that parse regular update.
#[test]
fn parse_regular_update() {
    let msg = serde_json::json!({
        "topic": "crypto_prices_chainlink",
        "payload": {
            "symbol": "btc/usd",
            "value": 42000,
            "timestamp": 1_700_000_000_000_u64
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert_eq!(result.len(), 1);
    assert!((result[0].0 - 42_000.0).abs() < f64::EPSILON);
    assert_eq!(result[0].1, 1_700_000_000_000);
}

/// Verifies that parse regular update string value.
#[test]
fn parse_regular_update_string_value() {
    let msg = serde_json::json!({
        "topic": "crypto_prices_chainlink",
        "payload": {
            "value": "42000.50",
            "timestamp": 123
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert_eq!(result.len(), 1);
    assert!((result[0].0 - 42_000.50).abs() < f64::EPSILON);
}

/// Verifies that parse initial data dump.
#[test]
fn parse_initial_data_dump() {
    let msg = serde_json::json!({
        "payload": {
            "data": [
                {"value": 41999, "timestamp": 122},
                {"value": 42000, "timestamp": 123}
            ]
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert_eq!(result.len(), 2);
    assert!((result[0].0 - 41_999.0).abs() < f64::EPSILON);
    assert_eq!(result[0].1, 122);
    assert!((result[1].0 - 42_000.0).abs() < f64::EPSILON);
    assert_eq!(result[1].1, 123);
}

/// Verifies that parse initial data dump returns all entries.
#[test]
fn parse_initial_data_dump_returns_all_entries() {
    let msg = serde_json::json!({
        "payload": {
            "data": [
                {"value": 41999, "timestamp": 122},
                {"value": 42000, "timestamp": 123}
            ]
        }
    });
    let result = parse_chainlink_payload(&msg);
    let last = result.last().unwrap();
    assert!((last.0 - 42_000.0).abs() < f64::EPSILON);
    assert_eq!(last.1, 123);
}

/// Verifies that parse invalid value returns empty.
#[test]
fn parse_invalid_value_returns_empty() {
    let msg = serde_json::json!({
        "topic": "crypto_prices_chainlink",
        "payload": {
            "value": "not_a_number",
            "timestamp": 123
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert!(result.is_empty());
}

/// Verifies that parse missing value field returns empty.
#[test]
fn parse_missing_value_field_returns_empty() {
    let msg = serde_json::json!({
        "topic": "crypto_prices_chainlink",
        "payload": {
            "timestamp": 123
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert!(result.is_empty());
}

/// Verifies that parse wrong topic returns empty.
#[test]
fn parse_wrong_topic_returns_empty() {
    let msg = serde_json::json!({
        "topic": "some_other_topic",
        "payload": {
            "value": 42000,
            "timestamp": 123
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert!(result.is_empty());
}

/// Verifies that parse unrecognised message returns empty.
#[test]
fn parse_unrecognised_message_returns_empty() {
    let msg = serde_json::json!({"type": "ping"});
    let result = parse_chainlink_payload(&msg);
    assert!(result.is_empty());
}

/// Verifies that parse missing timestamp defaults to zero.
#[test]
fn parse_missing_timestamp_defaults_to_zero() {
    let msg = serde_json::json!({
        "topic": "crypto_prices_chainlink",
        "payload": {
            "value": 42000
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].1, 0);
}

/// Verifies that parse data dump with invalid entry skips it.
#[test]
fn parse_data_dump_with_invalid_entry_skips_it() {
    let msg = serde_json::json!({
        "payload": {
            "data": [
                {"value": "bad", "timestamp": 100},
                {"value": 42000, "timestamp": 200}
            ]
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert_eq!(result.len(), 1);
    assert!((result[0].0 - 42_000.0).abs() < f64::EPSILON);
    assert_eq!(result[0].1, 200);
}

/// Verifies that parse empty data array returns empty.
#[test]
fn parse_empty_data_array_returns_empty() {
    let msg = serde_json::json!({
        "payload": {
            "data": []
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert!(result.is_empty());
}

/// Verifies that process chainlink text valid regular update.
#[test]
fn process_chainlink_text_valid_regular_update() {
    let text = r#"{"topic":"crypto_prices_chainlink","payload":{"value":42000,"timestamp":100}}"#;
    let result = process_chainlink_text(text);
    assert_eq!(result.len(), 1);
    assert!((result[0].0 - 42_000.0).abs() < f64::EPSILON);
    assert_eq!(result[0].1, 100);
}

/// Verifies that process chainlink text initial data dump.
#[test]
fn process_chainlink_text_initial_data_dump() {
    let text =
        r#"{"payload":{"data":[{"value":41999,"timestamp":122},{"value":42000,"timestamp":123}]}}"#;
    let result = process_chainlink_text(text);
    assert_eq!(result.len(), 2);
}

/// Verifies that process chainlink text array wrapper.
#[test]
fn process_chainlink_text_array_wrapper() {
    let text = serde_json::to_string(&serde_json::json!([
        {
            "topic": "crypto_prices_chainlink",
            "payload": {"value": 41000, "timestamp": 1}
        },
        {
            "topic": "crypto_prices_chainlink",
            "payload": {"value": 42000, "timestamp": 2}
        }
    ]))
    .unwrap();
    let result = process_chainlink_text(&text);
    assert_eq!(result.len(), 2);
    assert!((result[0].0 - 41_000.0).abs() < f64::EPSILON);
    assert!((result[1].0 - 42_000.0).abs() < f64::EPSILON);
}

/// Verifies that process chainlink text invalid json returns empty.
#[test]
fn process_chainlink_text_invalid_json_returns_empty() {
    let result = process_chainlink_text("not json");
    assert!(result.is_empty());
}

/// Verifies that process chainlink text empty string returns empty.
#[test]
fn process_chainlink_text_empty_string_returns_empty() {
    let result = process_chainlink_text("");
    assert!(result.is_empty());
}

/// Verifies that parse chainlink null timestamp.
#[test]
fn parse_chainlink_null_timestamp() {
    let msg = serde_json::json!({
        "topic": "crypto_prices_chainlink",
        "payload": {
            "value": 42000,
            "timestamp": null
        }
    });
    let result = parse_chainlink_payload(&msg);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].1, 0, "null timestamp should default to 0");
}

/// Verifies that parse chainlink nested empty array.
#[test]
fn parse_chainlink_nested_empty_array() {
    let text = serde_json::to_string(&serde_json::json!([
        {
            "payload": {
                "data": []
            }
        }
    ]))
    .unwrap();
    let result = process_chainlink_text(&text);
    assert!(
        result.is_empty(),
        "nested empty data array should return empty vec"
    );
}

/// Verifies that process chainlink text unrecognised returns empty.
#[test]
fn process_chainlink_text_unrecognised_returns_empty() {
    let text = r#"{"type":"pong"}"#;
    let result = process_chainlink_text(text);
    assert!(result.is_empty());
}
