use super::*;
use crate::db::schema::run_migrations;
use rusqlite::params;
use tempfile::NamedTempFile;

/// Build a migrated in-memory database for replay-quality tests.
fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Insert one feed-event row used by quality classification tests.
#[allow(clippy::too_many_arguments)]
fn insert_feed_event(
    conn: &Connection,
    source: &str,
    event_type: &str,
    price: Option<f64>,
    trade_size: Option<f64>,
    signed_quantity: Option<f64>,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
    bid_size: Option<f64>,
    ask_size: Option<f64>,
    depth_bid_notional: Option<f64>,
    depth_ask_notional: Option<f64>,
    depth_imbalance: Option<f64>,
) {
    conn.execute(
        "INSERT INTO feed_events (
            received_at_ms,
            event_at_ms,
            source,
            event_type,
            price,
            trade_size,
            signed_quantity,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            depth_bid_notional,
            depth_ask_notional,
            depth_imbalance,
            fidelity
        ) VALUES (1000, 1000, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'raw_event')",
        params![
            source,
            event_type,
            price,
            trade_size,
            signed_quantity,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            depth_bid_notional,
            depth_ask_notional,
            depth_imbalance
        ],
    )
    .unwrap();
}

/// Insert every feed class required for sweep-grade replay.
fn insert_sweep_grade_feed_events(conn: &Connection) {
    insert_feed_event(
        conn,
        "binance",
        "aggTrade",
        Some(42_000.0),
        Some(0.2),
        Some(-0.2),
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
        "binance",
        "bookTicker",
        None,
        None,
        None,
        Some(41_999.0),
        Some(42_001.0),
        Some(0.4),
        Some(0.5),
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        "binance",
        "depth",
        None,
        None,
        None,
        Some(41_999.0),
        Some(42_001.0),
        Some(0.4),
        Some(0.5),
        Some(10_000.0),
        Some(11_000.0),
        Some(-0.047),
    );
    insert_feed_event(
        conn,
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
        "clob_up",
        "best_bid_ask",
        None,
        None,
        None,
        Some(0.45),
        Some(0.55),
        Some(20.0),
        Some(30.0),
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        "clob_down",
        "best_bid_ask",
        None,
        None,
        None,
        Some(0.44),
        Some(0.56),
        Some(21.0),
        Some(31.0),
        None,
        None,
        None,
    );
}

/// Verify that complete raw feed classes classify as sweep-grade.
#[test]
fn complete_feed_classes_are_sweep_grade() {
    let conn = setup_db();
    insert_sweep_grade_feed_events(&conn);
    let report = analyze_connection(&conn, 0, 2_000).unwrap();
    assert_eq!(report.class, ReplayQualityClass::SweepGrade);
    assert!(report.missing_required().is_empty());
}

/// Verify that missing book-ticker rows block sweep-grade classification.
#[test]
fn missing_book_ticker_is_descriptive_only() {
    let conn = setup_db();
    insert_sweep_grade_feed_events(&conn);
    conn.execute(
        "DELETE FROM feed_events WHERE source = 'binance' AND event_type = 'bookTicker'",
        [],
    )
    .unwrap();
    let report = analyze_connection(&conn, 0, 2_000).unwrap();
    assert_eq!(report.class, ReplayQualityClass::DescriptiveOnly);
    assert!(
        report
            .missing_required()
            .iter()
            .any(|requirement| requirement.key == "binance_book_ticker")
    );
}

/// Verify that legacy tick-only databases are not sweep-grade.
#[test]
fn legacy_tick_only_data_is_legacy_snapshot() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO tick_data (timestamp, source, price) VALUES (1000, 'binance', 42000.0)",
        [],
    )
    .unwrap();
    let report = analyze_connection(&conn, 0, 2_000).unwrap();
    assert_eq!(report.class, ReplayQualityClass::LegacySnapshot);
}

/// Verify that the blocking error names missing required classes.
#[test]
fn blocking_error_names_missing_requirements() {
    let conn = setup_db();
    insert_feed_event(
        &conn,
        "binance",
        "aggTrade",
        Some(42_000.0),
        Some(0.2),
        Some(-0.2),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let report = analyze_connection(&conn, 0, 2_000).unwrap();
    let error = blocking_error(&report);
    assert!(error.contains("binance_book_ticker"));
    assert!(error.contains("descriptive_only"));
}

/// Verify that missing depth rows block sweep-grade classification.
#[test]
fn missing_depth_is_descriptive_only() {
    let conn = setup_db();
    insert_sweep_grade_feed_events(&conn);
    conn.execute(
        "DELETE FROM feed_events WHERE source = 'binance' AND event_type = 'depth'",
        [],
    )
    .unwrap();

    let report = analyze_connection(&conn, 0, 2_000).unwrap();

    assert_eq!(report.class, ReplayQualityClass::DescriptiveOnly);
    assert_eq!(report.missing_required_keys(), vec!["binance_depth"]);
}

/// Verify that an empty database is never labeled sweep-grade.
#[test]
fn empty_data_is_not_sweep_grade() {
    let conn = setup_db();

    let report = analyze_connection(&conn, 0, 2_000).unwrap();

    assert_eq!(report.class, ReplayQualityClass::Empty);
    assert!(!report.is_sweep_grade());
}

/// Verify that path-based validation accepts a complete replay-grade fixture.
#[test]
fn validate_sweep_input_accepts_complete_fixture_path() {
    let db_file = NamedTempFile::new().unwrap();
    let conn = Connection::open(db_file.path()).unwrap();
    run_migrations(&conn).unwrap();
    insert_sweep_grade_feed_events(&conn);
    drop(conn);

    let report = validate_sweep_input(db_file.path().to_str().unwrap(), 0, 2_000).unwrap();

    assert_eq!(report.class, ReplayQualityClass::SweepGrade);
}
