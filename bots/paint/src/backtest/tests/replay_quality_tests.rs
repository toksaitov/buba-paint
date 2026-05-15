use super::*;
use crate::db::clob_replay_blocks;
use crate::db::schema::run_migrations;
use crate::types::{FeedEvent, ReplayFidelity};
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

/// Insert one compact CLOB top-of-book row.
fn insert_compact_clob_event(conn: &Connection, source: &str) {
    let side = if source == "clob_up" { "up" } else { "down" };
    conn.execute(
        "INSERT INTO clob_replay_events (
            received_at_ms, event_at_ms, side, source, event_type, market_id, asset_id,
            best_bid, best_ask, bid_size, ask_size, fidelity
         ) VALUES (1000, 1000, ?1, ?2, 'price_change', 'm1', 'asset-1',
                   0.45, 0.55, 20.0, 30.0, 'raw_event')",
        params![side, source],
    )
    .unwrap();
}

/// Build one CLOB top-of-book feed event for block-storage tests.
fn clob_block_event(source: &str) -> FeedEvent {
    FeedEvent {
        id: None,
        received_at_ms: 1_000,
        event_at_ms: 1_000,
        received_at_us: Some(1_000_000),
        event_at_us: Some(1_000_000),
        source: source.to_string(),
        event_type: "price_change".to_string(),
        source_topic: Some(format!("{source}-topic")),
        source_symbol: None,
        connection_id: Some("conn-1".to_string()),
        sequence_key: Some(format!("{source}-seq")),
        market_id: Some("m1".to_string()),
        asset_id: Some(format!("{source}-asset")),
        price: None,
        trade_size: None,
        signed_quantity: None,
        best_bid: Some(0.45),
        best_ask: Some(0.55),
        bid_size: Some(20.0),
        ask_size: Some(30.0),
        depth_bid_notional: None,
        depth_ask_notional: None,
        depth_imbalance: None,
        microprice: Some(0.50),
        payload_json: None,
        details_json: None,
        fidelity: ReplayFidelity::RawEvent,
    }
}

/// Insert one compressed CLOB block into a quality-test DB.
fn insert_clob_block(conn: &Connection, events: &[FeedEvent]) {
    let block = clob_replay_blocks::encode_events(events, 1).unwrap();
    conn.execute(
        "INSERT INTO clob_replay_blocks (
            min_received_at_ms, max_received_at_ms, min_received_at_us, max_received_at_us,
            row_count, up_rows, down_rows, codec, schema_version, compressed_bytes,
            uncompressed_bytes, checksum, payload
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'zstd', ?8, ?9, ?10, ?11, ?12)",
        params![
            block.min_received_at_ms,
            block.max_received_at_ms,
            block.min_received_at_us,
            block.max_received_at_us,
            block.row_count,
            block.up_rows,
            block.down_rows,
            block.schema_version,
            block.compressed_bytes,
            block.uncompressed_bytes,
            block.checksum,
            block.payload,
        ],
    )
    .unwrap();
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

/// Verify that compact CLOB top-of-book rows satisfy replay quality.
#[test]
fn compact_clob_rows_satisfy_top_of_book_requirements() {
    let conn = setup_db();
    insert_sweep_grade_feed_events(&conn);
    conn.execute(
        "DELETE FROM feed_events WHERE source IN ('clob_up', 'clob_down')",
        [],
    )
    .unwrap();
    insert_compact_clob_event(&conn, "clob_up");
    insert_compact_clob_event(&conn, "clob_down");

    let report = analyze_connection(&conn, 0, 2_000).unwrap();

    assert_eq!(report.class, ReplayQualityClass::SweepGrade);
    assert!(report.missing_required().is_empty());
}

/// Verify that compressed CLOB blocks satisfy replay quality.
#[test]
fn clob_replay_blocks_satisfy_top_of_book_requirements() {
    let conn = setup_db();
    insert_sweep_grade_feed_events(&conn);
    conn.execute(
        "DELETE FROM feed_events WHERE source IN ('clob_up', 'clob_down')",
        [],
    )
    .unwrap();
    insert_clob_block(
        &conn,
        &[clob_block_event("clob_up"), clob_block_event("clob_down")],
    );

    let report = analyze_connection(&conn, 0, 2_000).unwrap();

    assert_eq!(report.class, ReplayQualityClass::SweepGrade);
    assert_eq!(report.clob_replay_block_rows, 2);
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
