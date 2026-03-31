use super::*;

/// Verifies that parse up wins.
#[test]
fn parse_up_wins() {
    let body = serde_json::json!([{
        "slug": "btc-updown-5m-1774006800",
        "markets": [{
            "outcomePrices": ["1", "0"],
            "outcomes": ["Up", "Down"]
        }]
    }]);
    assert_eq!(parse_gamma_outcome(&body), Some("UP".to_string()));
}

/// Verifies that parse down wins.
#[test]
fn parse_down_wins() {
    let body = serde_json::json!([{
        "slug": "btc-updown-5m-1774006800",
        "markets": [{
            "outcomePrices": ["0", "1"],
            "outcomes": ["Up", "Down"]
        }]
    }]);
    assert_eq!(parse_gamma_outcome(&body), Some("DOWN".to_string()));
}

/// Verifies that parse not resolved.
#[test]
fn parse_not_resolved() {
    let body = serde_json::json!([{
        "slug": "btc-updown-5m-1774006800",
        "markets": [{
            "outcomePrices": ["0.52", "0.48"],
            "outcomes": ["Up", "Down"]
        }]
    }]);
    assert_eq!(parse_gamma_outcome(&body), None);
}

/// Verifies that parse empty array.
#[test]
fn parse_empty_array() {
    let body = serde_json::json!([]);
    assert_eq!(parse_gamma_outcome(&body), None);
}

/// Verifies that parse no markets.
#[test]
fn parse_no_markets() {
    let body = serde_json::json!([{
        "slug": "btc-updown-5m-1774006800"
    }]);
    assert_eq!(parse_gamma_outcome(&body), None);
}

/// Verifies that parse missing outcome prices.
#[test]
fn parse_missing_outcome_prices() {
    let body = serde_json::json!([{
        "slug": "btc-updown-5m-1774006800",
        "markets": [{"outcomes": ["Up", "Down"]}]
    }]);
    assert_eq!(parse_gamma_outcome(&body), None);
}

/// Verifies that parse near one tolerance.
#[test]
fn parse_near_one_tolerance() {
    let body = serde_json::json!([{
        "markets": [{
            "outcomePrices": ["0.999", "0.001"],
            "outcomes": ["Up", "Down"]
        }]
    }]);
    assert_eq!(parse_gamma_outcome(&body), Some("UP".to_string()));
}

/// Verifies that parse dict format up wins.
#[test]
fn parse_dict_format_up_wins() {
    let body = serde_json::json!({
        "slug": "btc-updown-5m-1774006800",
        "markets": [{
            "outcomePrices": "[\"1\", \"0\"]",
            "outcomes": "[\"Up\", \"Down\"]"
        }]
    });
    assert_eq!(parse_gamma_outcome(&body), Some("UP".to_string()));
}

/// Verifies that parse dict format down wins.
#[test]
fn parse_dict_format_down_wins() {
    let body = serde_json::json!({
        "slug": "btc-updown-5m-1774006800",
        "markets": [{
            "outcomePrices": "[\"0\", \"1\"]",
            "outcomes": "[\"Up\", \"Down\"]"
        }]
    });
    assert_eq!(parse_gamma_outcome(&body), Some("DOWN".to_string()));
}

/// Verifies that parse string encoded outcome prices.
#[test]
fn parse_string_encoded_outcome_prices() {
    let body = serde_json::json!([{
        "markets": [{
            "outcomePrices": "[\"1\", \"0\"]",
            "outcomes": "[\"Up\", \"Down\"]"
        }]
    }]);
    assert_eq!(parse_gamma_outcome(&body), Some("UP".to_string()));
}

/// Verifies that parse string encoded down wins.
#[test]
fn parse_string_encoded_down_wins() {
    let body = serde_json::json!([{
        "markets": [{
            "outcomePrices": "[\"0\", \"1\"]",
            "outcomes": "[\"Up\", \"Down\"]"
        }]
    }]);
    assert_eq!(parse_gamma_outcome(&body), Some("DOWN".to_string()));
}

/// Verifies that parse single outcome price.
#[test]
fn parse_single_outcome_price() {
    let body = serde_json::json!([{
        "markets": [{
            "outcomePrices": ["1"],
            "outcomes": ["Up"]
        }]
    }]);
    assert_eq!(parse_gamma_outcome(&body), None);
}

/// Verifies that load markets from empty db.
#[test]
fn load_markets_from_empty_db() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::schema::run_migrations(&conn).unwrap();
    let markets = load_markets(&conn).unwrap();
    assert!(markets.is_empty());
}

/// Verifies that ensure column idempotent.
#[test]
fn ensure_column_idempotent() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::schema::run_migrations(&conn).unwrap();

    ensure_polymarket_outcome_column(&conn).unwrap();

    ensure_polymarket_outcome_column(&conn).unwrap();
}

/// Verifies that store and read outcome.
#[test]
fn store_and_read_outcome() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::schema::run_migrations(&conn).unwrap();
    ensure_polymarket_outcome_column(&conn).unwrap();

    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status)
         VALUES ('m1', 'BTC Up or Down', 'c1', 'btc-updown-5m-100', 'up1', 'dn1', 1000, 2000, 'resolved')",
        [],
    )
    .unwrap();

    store_polymarket_outcome(&conn, "m1", "UP").unwrap();

    let stored: String = conn
        .query_row(
            "SELECT polymarket_outcome FROM markets WHERE market_id = 'm1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, "UP");
}

/// Verifies that load markets returns slug and outcome.
#[test]
fn load_markets_returns_slug_and_outcome() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::schema::run_migrations(&conn).unwrap();

    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status)
         VALUES ('m1', 'q', 'c1', 'btc-updown-5m-100', 'u', 'd', 1000, 2000, 'resolved')",
        [],
    )
    .unwrap();

    let markets = load_markets(&conn).unwrap();
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].slug, "btc-updown-5m-100");
    assert_eq!(markets[0].market_id, "m1");

    assert!(markets[0].our_outcome.is_none());
}
