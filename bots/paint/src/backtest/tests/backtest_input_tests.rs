use super::*;
use rusqlite::params;

/// Create an in-memory DB with the live-runtime market schema.
fn setup_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::schema::run_migrations(&conn).unwrap();
    conn
}

/// Insert one raw feed event with the supplied typed fields.
#[allow(clippy::too_many_arguments)]
fn insert_feed_event(
    conn: &rusqlite::Connection,
    timestamp: i64,
    source: &str,
    event_type: &str,
    price: Option<f64>,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
    bid_size: Option<f64>,
    ask_size: Option<f64>,
    trade_size: Option<f64>,
    signed_quantity: Option<f64>,
    depth_bid_notional: Option<f64>,
    depth_ask_notional: Option<f64>,
    depth_imbalance: Option<f64>,
) {
    conn.execute(
        "INSERT INTO feed_events (
            received_at_ms, event_at_ms, received_at_us, event_at_us,
            source, event_type, price, best_bid, best_ask, bid_size, ask_size,
            trade_size, signed_quantity, depth_bid_notional, depth_ask_notional,
            depth_imbalance, fidelity
         ) VALUES (?1, ?1, ?1 * 1000, ?1 * 1000, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                   ?9, ?10, ?11, ?12, ?13, 'raw_event')",
        params![
            timestamp,
            source,
            event_type,
            price,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            trade_size,
            signed_quantity,
            depth_bid_notional,
            depth_ask_notional,
            depth_imbalance,
        ],
    )
    .unwrap();
}

/// Insert one settled market without derived backtest price columns.
fn insert_market(conn: &rusqlite::Connection) {
    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug,
                              up_token_id, down_token_id, start_time, end_time,
                              outcome, status)
         VALUES ('m1', 'Bitcoin Up or Down', 'cond-1', 'slug-1',
                 'up-1', 'down-1', 1000, 2000, 'UP', 'resolved')",
        [],
    )
    .unwrap();
}

/// Insert every public feed class required by replay-grade validation.
fn insert_sweep_grade_feed_rows(conn: &rusqlite::Connection) {
    insert_feed_event(
        conn,
        1100,
        "binance",
        "aggTrade",
        Some(42_000.0),
        None,
        None,
        None,
        None,
        Some(0.25),
        Some(0.25),
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1200,
        "binance",
        "bookTicker",
        None,
        Some(41_999.0),
        Some(42_001.0),
        Some(1.0),
        Some(1.2),
        None,
        None,
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1300,
        "binance",
        "depth",
        None,
        Some(41_999.0),
        Some(42_001.0),
        Some(1.0),
        Some(1.2),
        None,
        None,
        Some(100_000.0),
        Some(95_000.0),
        Some(0.025),
    );
    insert_feed_event(
        conn,
        1400,
        "chainlink",
        "chainlink_price",
        Some(42_000.5),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1500,
        "clob_up",
        "best_bid_ask",
        None,
        Some(0.48),
        Some(0.52),
        Some(100.0),
        Some(120.0),
        None,
        None,
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1600,
        "clob_down",
        "best_bid_ask",
        None,
        Some(0.47),
        Some(0.53),
        Some(110.0),
        Some(130.0),
        None,
        None,
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1900,
        "binance",
        "aggTrade",
        Some(42_050.0),
        None,
        None,
        None,
        None,
        Some(0.10),
        Some(-0.10),
        None,
        None,
        None,
    );
}

/// Verifies that live-runtime DBs can be backtest-ready without derived columns.
#[test]
fn live_runtime_schema_with_raw_rows_is_backtest_ready() {
    let conn = setup_db();
    insert_market(&conn);
    insert_sweep_grade_feed_rows(&conn);

    let report = analyze_connection(&conn, 1000, 2000).unwrap();

    assert!(report.is_backtest_ready());
    assert_eq!(report.settled_windows, 1);
    assert_eq!(report.missing_open_prices, 0);
    assert_eq!(report.missing_close_prices, 0);
    assert!(report.dry_run_ticks > 0);
}

/// Verifies that public replay quality alone is not enough for backtesting.
#[test]
fn sweep_grade_without_windows_is_not_backtest_ready() {
    let conn = setup_db();
    insert_sweep_grade_feed_rows(&conn);

    let report = analyze_connection(&conn, 1000, 2000).unwrap();

    assert!(!report.is_backtest_ready());
    assert!(
        report
            .blockers
            .iter()
            .any(|blocker| blocker == "no_settled_windows")
    );
}

/// Verifies that readiness reports expose actionable blockers.
#[test]
fn format_report_includes_backtest_blockers() {
    let conn = setup_db();
    insert_sweep_grade_feed_rows(&conn);
    let report = analyze_connection(&conn, 1000, 2000).unwrap();
    let text = format_report(&report);

    assert!(text.contains("backtest_input=not_backtest_ready"));
    assert!(text.contains("blocker=no_settled_windows"));
}
