use super::*;

// -- parse_string_or_array ------------------------------------------------

#[test]
fn parse_string_or_array_json_array() {
    let val = serde_json::json!(["Up", "Down"]);
    let result = parse_string_or_array(Some(&val));
    assert_eq!(result, vec!["Up", "Down"]);
}

#[test]
fn parse_string_or_array_string_encoded_array() {
    let val = serde_json::json!(r#"["Up","Down"]"#);
    let result = parse_string_or_array(Some(&val));
    assert_eq!(result, vec!["Up", "Down"]);
}

#[test]
fn parse_string_or_array_single_string() {
    let val = serde_json::json!("Up");
    let result = parse_string_or_array(Some(&val));
    assert_eq!(result, vec!["Up"]);
}

#[test]
fn parse_string_or_array_none() {
    let result = parse_string_or_array(None);
    assert!(result.is_empty());
}

#[test]
fn parse_string_or_array_empty_string() {
    let val = serde_json::json!("");
    let result = parse_string_or_array(Some(&val));
    assert!(result.is_empty());
}

#[test]
fn parse_string_or_array_empty_array() {
    let val = serde_json::json!([]);
    let result = parse_string_or_array(Some(&val));
    assert!(result.is_empty());
}

#[test]
fn parse_string_or_array_number_value() {
    // Non-string, non-array value → empty.
    let val = serde_json::json!(42);
    let result = parse_string_or_array(Some(&val));
    assert!(result.is_empty());
}

#[test]
fn parse_string_or_array_mixed_array_filters_non_strings() {
    let val = serde_json::json!(["Up", 42, "Down"]);
    let result = parse_string_or_array(Some(&val));
    // Non-string elements are filtered out by `filter_map`.
    assert_eq!(result, vec!["Up", "Down"]);
}

// -- parse_end_date -------------------------------------------------------

#[test]
fn parse_end_date_rfc3339() {
    let result = parse_end_date("2024-01-01T00:05:00Z");
    assert!(result.is_some());
    // 2024-01-01T00:05:00Z = 1704067500 seconds = 1704067500000 ms
    assert_eq!(result.unwrap(), 1_704_067_500_000);
}

#[test]
fn parse_end_date_with_offset() {
    let result = parse_end_date("2024-01-01T01:05:00+01:00");
    assert!(result.is_some());
    // Same instant as 2024-01-01T00:05:00Z.
    assert_eq!(result.unwrap(), 1_704_067_500_000);
}

#[test]
fn parse_end_date_without_timezone() {
    let result = parse_end_date("2024-01-01T00:05:00");
    assert!(result.is_some());
    // Should be interpreted as UTC.
    assert_eq!(result.unwrap(), 1_704_067_500_000);
}

#[test]
fn parse_end_date_empty_string() {
    let result = parse_end_date("");
    assert!(result.is_none());
}

#[test]
fn parse_end_date_invalid_string() {
    let result = parse_end_date("not-a-date");
    assert!(result.is_none());
}

// -- parse_gamma_event_response -------------------------------------------

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
    // Only 1 outcome, needs >= 2
    assert!(parse_gamma_event_response(&body).is_none());
}

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
    // Only 1 token ID, needs >= 2
    assert!(parse_gamma_event_response(&body).is_none());
}

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

#[test]
fn parse_gamma_no_markets_and_no_id_returns_none() {
    let body = serde_json::json!({"something": "else"});
    assert!(parse_gamma_event_response(&body).is_none());
}

#[test]
fn parse_gamma_empty_markets_array_and_no_id_returns_none() {
    let body = serde_json::json!({"markets": []});
    // markets is present but empty, and no "id" field.
    // The check: body.get("markets").is_none() => false (markets exists)
    // So we proceed but candidates will be from the empty markets vec.
    // Actually, markets.is_empty() => true so candidates = [event].
    // The event has no "id" so market_id = "" => skip. Returns None.
    assert!(parse_gamma_event_response(&body).is_none());
}

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
    // "Yes" maps to up, "No" maps to down
    assert_eq!(window.up_token_id, "tok-yes");
    assert_eq!(window.down_token_id, "tok-no");
}

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
    // Falls back: first = UP, second = DOWN
    assert_eq!(window.up_token_id, "tok-alpha");
    assert_eq!(window.down_token_id, "tok-beta");
}

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
    // Event-level slug takes priority
    assert_eq!(window.slug, "event-slug");
}

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
    // endDate falls through to event level
    assert_eq!(window.end_time, 1_704_067_500_000);
}

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
    // First market has empty id, so second one is used
    assert_eq!(window.market_id, "mkt-2");
}

#[test]
fn parse_gamma_missing_id_field_entirely_returns_none() {
    // Event has "id" so the initial guard passes, but the market inside
    // has no "id" or "conditionId" — market_id will be "" → skip.
    let body = serde_json::json!({
        "id": "event-id",
        "markets": [{
            "question": "Will BTC go up?",
            "outcomes": ["Up", "Down"],
            "clobTokenIds": ["tok-up", "tok-down"],
            "endDate": "2024-01-01T00:05:00Z"
        }]
    });
    // The market has no id/conditionId, so market_id="" → skipped.
    // No other candidates remain → None.
    assert!(parse_gamma_event_response(&body).is_none());
}

#[test]
fn parse_gamma_reversed_outcome_order() {
    // outcomes=["Down","Up"], clobTokenIds=["tok-down","tok-up"]
    // The parser should match by name, not position:
    //   up_idx = 1 (where "Up" is), down_idx = 0 (where "Down" is)
    //   up_token_id = clobTokenIds[1] = "tok-up"
    //   down_token_id = clobTokenIds[0] = "tok-down"
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

#[test]
fn parse_gamma_missing_outcomes_and_tokens_entirely() {
    let body = serde_json::json!({
        "id": "mkt-bare",
        "slug": "btc-test",
        "conditionId": "cond-bare",
        "endDate": "2024-01-01T00:05:00Z"
    });
    // No outcomes/clobTokenIds at all → both parse to empty vecs → < 2 → skip.
    assert!(parse_gamma_event_response(&body).is_none());
}

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
    // Both markets fail: first has < 2 outcomes, second has < 2 clob tokens.
    assert!(parse_gamma_event_response(&body).is_none());
}
