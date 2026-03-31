use super::*;

/// Verifies that parse string or array json array.
#[test]
fn parse_string_or_array_json_array() {
    let val = serde_json::json!(["Up", "Down"]);
    let result = parse_string_or_array(Some(&val));
    assert_eq!(result, vec!["Up", "Down"]);
}

/// Verifies that parse string or array string encoded array.
#[test]
fn parse_string_or_array_string_encoded_array() {
    let val = serde_json::json!(r#"["Up","Down"]"#);
    let result = parse_string_or_array(Some(&val));
    assert_eq!(result, vec!["Up", "Down"]);
}

/// Verifies that parse string or array single string.
#[test]
fn parse_string_or_array_single_string() {
    let val = serde_json::json!("Up");
    let result = parse_string_or_array(Some(&val));
    assert_eq!(result, vec!["Up"]);
}

/// Verifies that parse string or array none.
#[test]
fn parse_string_or_array_none() {
    let result = parse_string_or_array(None);
    assert!(result.is_empty());
}

/// Verifies that parse string or array empty string.
#[test]
fn parse_string_or_array_empty_string() {
    let val = serde_json::json!("");
    let result = parse_string_or_array(Some(&val));
    assert!(result.is_empty());
}

/// Verifies that parse string or array empty array.
#[test]
fn parse_string_or_array_empty_array() {
    let val = serde_json::json!([]);
    let result = parse_string_or_array(Some(&val));
    assert!(result.is_empty());
}

/// Verifies that parse string or array number value.
#[test]
fn parse_string_or_array_number_value() {
    let val = serde_json::json!(42);
    let result = parse_string_or_array(Some(&val));
    assert!(result.is_empty());
}

/// Verifies that parse string or array mixed array filters non strings.
#[test]
fn parse_string_or_array_mixed_array_filters_non_strings() {
    let val = serde_json::json!(["Up", 42, "Down"]);
    let result = parse_string_or_array(Some(&val));

    assert_eq!(result, vec!["Up", "Down"]);
}

/// Verifies that parse end date rfc3339.
#[test]
fn parse_end_date_rfc3339() {
    let result = parse_end_date("2024-01-01T00:05:00Z");
    assert!(result.is_some());

    assert_eq!(result.unwrap(), 1_704_067_500_000);
}

/// Verifies that parse end date with offset.
#[test]
fn parse_end_date_with_offset() {
    let result = parse_end_date("2024-01-01T01:05:00+01:00");
    assert!(result.is_some());

    assert_eq!(result.unwrap(), 1_704_067_500_000);
}

/// Verifies that parse end date without timezone.
#[test]
fn parse_end_date_without_timezone() {
    let result = parse_end_date("2024-01-01T00:05:00");
    assert!(result.is_some());

    assert_eq!(result.unwrap(), 1_704_067_500_000);
}

/// Verifies that parse end date empty string.
#[test]
fn parse_end_date_empty_string() {
    let result = parse_end_date("");
    assert!(result.is_none());
}

/// Verifies that parse end date invalid string.
#[test]
fn parse_end_date_invalid_string() {
    let result = parse_end_date("not-a-date");
    assert!(result.is_none());
}

/// Verifies that parse gamma valid response with markets array.
#[test]
fn parse_gamma_valid_response_with_markets_array() {
    let body = serde_json::json!({
        "slug": "btc-updown-5m-1704067200",
        "markets": [{
            "id": "mkt-1",
            "question": "Will BTC go up?",
            "conditionId": "cond-1",
            "outcomes": ["Up", "Down"],
            "clobTokenIds": ["tok-up", "tok-down"],
            "endDate": "2024-01-01T00:05:00Z"
        }]
    });
    let window = parse_gamma_event_response(&body).unwrap();
    assert_eq!(window.market_id, "mkt-1");
    assert_eq!(window.slug, "btc-updown-5m-1704067200");
    assert_eq!(window.up_token_id, "tok-up");
    assert_eq!(window.down_token_id, "tok-down");
    assert_eq!(window.end_time, 1_704_067_500_000);
    assert_eq!(window.start_time, 1_704_067_500_000 - 300_000);
}

/// Verifies that parse gamma missing markets uses event itself.
#[test]
fn parse_gamma_missing_markets_uses_event_itself() {
    let body = serde_json::json!({
        "id": "mkt-direct",
        "slug": "btc-test",
        "question": "Direct event?",
        "conditionId": "cond-direct",
        "outcomes": ["Up", "Down"],
        "clobTokenIds": ["tok-u", "tok-d"],
        "endDate": "2024-01-01T00:05:00Z"
    });
    let window = parse_gamma_event_response(&body).unwrap();
    assert_eq!(window.market_id, "mkt-direct");
    assert_eq!(window.up_token_id, "tok-u");
    assert_eq!(window.down_token_id, "tok-d");
}

/// Verifies that parse gamma missing outcomes returns none.
#[test]
fn parse_gamma_missing_outcomes_returns_none() {
    let body = serde_json::json!({
        "markets": [{
            "id": "mkt-1",
            "conditionId": "cond-1",
            "outcomes": ["Up"],
            "clobTokenIds": ["tok-up", "tok-down"],
            "endDate": "2024-01-01T00:05:00Z"
        }]
    });

    assert!(parse_gamma_event_response(&body).is_none());
}

/// Verifies that parse gamma missing clob token ids returns none.
#[test]
fn parse_gamma_missing_clob_token_ids_returns_none() {
    let body = serde_json::json!({
        "markets": [{
            "id": "mkt-1",
            "conditionId": "cond-1",
            "outcomes": ["Up", "Down"],
            "clobTokenIds": ["tok-up"],
            "endDate": "2024-01-01T00:05:00Z"
        }]
    });

    assert!(parse_gamma_event_response(&body).is_none());
}

/// Verifies that parse gamma string encoded clob token ids.
#[test]
fn parse_gamma_string_encoded_clob_token_ids() {
    let body = serde_json::json!({
        "id": "mkt-str",
        "slug": "btc-test",
        "conditionId": "cond-str",
        "outcomes": ["Up", "Down"],
        "clobTokenIds": r#"["tok-up-str","tok-down-str"]"#,
        "endDate": "2024-01-01T00:05:00Z"
    });
    let window = parse_gamma_event_response(&body).unwrap();
    assert_eq!(window.up_token_id, "tok-up-str");
    assert_eq!(window.down_token_id, "tok-down-str");
}

/// Verifies that parse gamma missing end date gives zero times.
#[test]
fn parse_gamma_missing_end_date_gives_zero_times() {
    let body = serde_json::json!({
        "id": "mkt-nodate",
        "slug": "btc-test",
        "conditionId": "cond-nodate",
        "outcomes": ["Up", "Down"],
        "clobTokenIds": ["tok-up", "tok-down"]
    });
    let window = parse_gamma_event_response(&body).unwrap();
    assert_eq!(window.end_time, 0);
    assert_eq!(window.start_time, 0);
}

/// Verifies that parse gamma no markets and no id returns none.
#[test]
fn parse_gamma_no_markets_and_no_id_returns_none() {
    let body = serde_json::json!({"something": "else"});
    assert!(parse_gamma_event_response(&body).is_none());
}

/// Verifies that parse gamma empty markets array and no id returns none.
#[test]
fn parse_gamma_empty_markets_array_and_no_id_returns_none() {
    let body = serde_json::json!({"markets": []});

    assert!(parse_gamma_event_response(&body).is_none());
}

/// Verifies that parse gamma yes no outcomes.
#[test]
fn parse_gamma_yes_no_outcomes() {
    let body = serde_json::json!({
        "id": "mkt-yn",
        "slug": "btc-test",
        "conditionId": "cond-yn",
        "outcomes": ["Yes", "No"],
        "clobTokenIds": ["tok-yes", "tok-no"],
        "endDate": "2024-01-01T00:05:00Z"
    });
    let window = parse_gamma_event_response(&body).unwrap();

    assert_eq!(window.up_token_id, "tok-yes");
    assert_eq!(window.down_token_id, "tok-no");
}

/// Verifies that parse gamma unrecognised outcomes fall back to positional.
#[test]
fn parse_gamma_unrecognised_outcomes_fall_back_to_positional() {
    let body = serde_json::json!({
        "id": "mkt-pos",
        "slug": "btc-test",
        "conditionId": "cond-pos",
        "outcomes": ["Alpha", "Beta"],
        "clobTokenIds": ["tok-alpha", "tok-beta"],
        "endDate": "2024-01-01T00:05:00Z"
    });
    let window = parse_gamma_event_response(&body).unwrap();

    assert_eq!(window.up_token_id, "tok-alpha");
    assert_eq!(window.down_token_id, "tok-beta");
}

/// Verifies that parse gamma slug from event level.
#[test]
fn parse_gamma_slug_from_event_level() {
    let body = serde_json::json!({
        "slug": "event-slug",
        "markets": [{
            "id": "mkt-1",
            "conditionId": "cond-1",
            "slug": "market-slug",
            "outcomes": ["Up", "Down"],
            "clobTokenIds": ["tok-up", "tok-down"],
            "endDate": "2024-01-01T00:05:00Z"
        }]
    });
    let window = parse_gamma_event_response(&body).unwrap();

    assert_eq!(window.slug, "event-slug");
}

/// Verifies that parse gamma end date from event level.
#[test]
fn parse_gamma_end_date_from_event_level() {
    let body = serde_json::json!({
        "endDate": "2024-01-01T00:05:00Z",
        "markets": [{
            "id": "mkt-1",
            "conditionId": "cond-1",
            "outcomes": ["Up", "Down"],
            "clobTokenIds": ["tok-up", "tok-down"]
        }]
    });
    let window = parse_gamma_event_response(&body).unwrap();

    assert_eq!(window.end_time, 1_704_067_500_000);
}

/// Verifies that parse gamma market with empty id skipped.
#[test]
fn parse_gamma_market_with_empty_id_skipped() {
    let body = serde_json::json!({
        "markets": [
            {
                "id": "",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up", "tok-down"],
                "endDate": "2024-01-01T00:05:00Z"
            },
            {
                "id": "mkt-2",
                "conditionId": "cond-2",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-2", "tok-down-2"],
                "endDate": "2024-01-01T00:05:00Z"
            }
        ]
    });
    let window = parse_gamma_event_response(&body).unwrap();

    assert_eq!(window.market_id, "mkt-2");
}

/// Verifies that parse gamma missing id field entirely returns none.
#[test]
fn parse_gamma_missing_id_field_entirely_returns_none() {
    let body = serde_json::json!({
        "id": "event-id",
        "markets": [{
            "question": "Will BTC go up?",
            "outcomes": ["Up", "Down"],
            "clobTokenIds": ["tok-up", "tok-down"],
            "endDate": "2024-01-01T00:05:00Z"
        }]
    });

    assert!(parse_gamma_event_response(&body).is_none());
}

/// Verifies that parse gamma reversed outcome order.
#[test]
fn parse_gamma_reversed_outcome_order() {
    let body = serde_json::json!({
        "id": "mkt-rev",
        "slug": "btc-reversed",
        "conditionId": "cond-rev",
        "outcomes": ["Down", "Up"],
        "clobTokenIds": ["tok-down", "tok-up"],
        "endDate": "2024-01-01T00:05:00Z"
    });
    let window = parse_gamma_event_response(&body).unwrap();
    assert_eq!(
        window.up_token_id, "tok-up",
        "up_token_id should follow the 'Up' outcome, not positional index"
    );
    assert_eq!(
        window.down_token_id, "tok-down",
        "down_token_id should follow the 'Down' outcome, not positional index"
    );
}

/// Verifies that parse gamma missing outcomes and tokens entirely.
#[test]
fn parse_gamma_missing_outcomes_and_tokens_entirely() {
    let body = serde_json::json!({
        "id": "mkt-bare",
        "slug": "btc-test",
        "conditionId": "cond-bare",
        "endDate": "2024-01-01T00:05:00Z"
    });

    assert!(parse_gamma_event_response(&body).is_none());
}

/// Verifies that parse gamma all markets fail validation returns none.
#[test]
fn parse_gamma_all_markets_fail_validation_returns_none() {
    let body = serde_json::json!({
        "markets": [
            {
                "id": "mkt-a",
                "conditionId": "cond-a",
                "outcomes": ["Up"],
                "clobTokenIds": ["tok-up"],
                "endDate": "2024-01-01T00:05:00Z"
            },
            {
                "id": "mkt-b",
                "conditionId": "cond-b",
                "outcomes": ["Down"],
                "clobTokenIds": [],
                "endDate": "2024-01-01T00:05:00Z"
            }
        ]
    });

    assert!(parse_gamma_event_response(&body).is_none());
}
