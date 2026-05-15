use super::*;
use crate::db::clob_replay_blocks;
use crate::types::{FeedEvent, ReplayFidelity};
use rusqlite::{Connection, params};

/// Create one source DB file with enough rows for backtest preparation.
fn create_source_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    crate::db::schema::run_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO markets (
            market_id, question, condition_id, slug, up_token_id, down_token_id,
            start_time, end_time, status, outcome
         ) VALUES ('m1', 'BTC up?', 'cond-1', 'slug-1', 'up-1', 'down-1',
                   1000, 2000, 'resolved', 'UP')",
        [],
    )
    .unwrap();
    insert_feed_event(
        &conn,
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
        &conn,
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
        &conn,
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
        &conn,
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
        &conn,
        1500,
        "clob_up",
        "price_change",
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
        &conn,
        1600,
        "clob_down",
        "price_change",
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
        &conn,
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

/// Insert one source feed-event row with typed replay fields.
#[allow(clippy::too_many_arguments)]
fn insert_feed_event(
    conn: &Connection,
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

/// Insert one source compact CLOB replay row.
fn insert_compact_clob_event(conn: &Connection, timestamp: i64, source: &str) {
    conn.execute(
        "INSERT INTO clob_replay_events (
            received_at_ms, event_at_ms, received_at_us, event_at_us, side, source, event_type,
            market_id, asset_id, best_bid, best_ask, bid_size, ask_size, microprice, fidelity
         ) VALUES (
            ?1, ?1, ?1 * 1000, ?1 * 1000, ?2, ?3, 'price_change',
            'm1', ?4, 0.48, 0.52, 100.0, 120.0, 0.50, 'raw_event'
         )",
        params![
            timestamp,
            if source == "clob_up" { "up" } else { "down" },
            source,
            if source == "clob_up" {
                "up-1"
            } else {
                "down-1"
            },
        ],
    )
    .unwrap();
}

/// Build one CLOB event for compressed-block preparation tests.
fn clob_block_event(timestamp: u64, source: &str) -> FeedEvent {
    FeedEvent {
        id: None,
        received_at_ms: timestamp,
        event_at_ms: timestamp,
        received_at_us: Some(timestamp.saturating_mul(1_000)),
        event_at_us: Some(timestamp.saturating_mul(1_000)),
        source: source.to_string(),
        event_type: "price_change".to_string(),
        source_topic: Some(format!("{source}-topic")),
        source_symbol: None,
        connection_id: Some("conn-1".to_string()),
        sequence_key: Some(format!("seq-{timestamp}")),
        market_id: Some("m1".to_string()),
        asset_id: Some(if source == "clob_up" {
            "up-1".to_string()
        } else {
            "down-1".to_string()
        }),
        price: None,
        trade_size: None,
        signed_quantity: None,
        best_bid: Some(0.48),
        best_ask: Some(0.52),
        bid_size: Some(100.0),
        ask_size: Some(120.0),
        depth_bid_notional: None,
        depth_ask_notional: None,
        depth_imbalance: None,
        microprice: Some(0.50),
        payload_json: None,
        details_json: None,
        fidelity: ReplayFidelity::RawEvent,
    }
}

/// Insert one compressed CLOB replay block.
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

/// Verifies that preparation converts CLOB rows and creates replay indexes.
#[test]
fn prepare_backtest_input_builds_compact_indexed_output() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("source.db");
    let output_path = temp.path().join("prepared.db");
    create_source_db(&source_path);

    let report = prepare_backtest_input(&PrepareBacktestInputOptions {
        data_path: source_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        start_time: 1000,
        end_time: 2000,
    })
    .unwrap();

    let conn = Connection::open(&output_path).unwrap();
    let generic_clob_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM feed_events WHERE source IN ('clob_up', 'clob_down')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let compact_clob_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM clob_replay_events", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert!(report.readiness.is_backtest_ready());
    assert_eq!(generic_clob_rows, 0);
    assert_eq!(compact_clob_rows, 2);
    assert!(crate::db::schema::has_replay_indexes(&conn).unwrap());
    assert!(std::path::Path::new(&report.manifest_path).exists());
}

/// Verifies that mixed legacy and compact CLOB sources do not conflict on row IDs.
#[test]
fn prepare_backtest_input_handles_mixed_clob_storage() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("source.db");
    let output_path = temp.path().join("prepared.db");
    create_source_db(&source_path);
    let source = Connection::open(&source_path).unwrap();
    insert_compact_clob_event(&source, 1700, "clob_up");
    insert_compact_clob_event(&source, 1800, "clob_down");
    drop(source);

    let report = prepare_backtest_input(&PrepareBacktestInputOptions {
        data_path: source_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        start_time: 1000,
        end_time: 2000,
    })
    .unwrap();

    let conn = Connection::open(&output_path).unwrap();
    let compact_clob_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM clob_replay_events", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert!(report.readiness.is_backtest_ready());
    assert_eq!(compact_clob_rows, 4);
}

/// Verifies that preparation preserves compressed CLOB blocks.
#[test]
fn prepare_backtest_input_preserves_clob_replay_blocks() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("source.db");
    let output_path = temp.path().join("prepared.db");
    create_source_db(&source_path);
    let source = Connection::open(&source_path).unwrap();
    source
        .execute(
            "DELETE FROM feed_events WHERE source IN ('clob_up', 'clob_down')",
            [],
        )
        .unwrap();
    insert_clob_block(
        &source,
        &[
            clob_block_event(1500, "clob_up"),
            clob_block_event(1600, "clob_down"),
        ],
    );
    drop(source);

    let report = prepare_backtest_input(&PrepareBacktestInputOptions {
        data_path: source_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        start_time: 1000,
        end_time: 2000,
    })
    .unwrap();

    let conn = Connection::open(&output_path).unwrap();
    let row_storage_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM clob_replay_events", [], |row| {
            row.get(0)
        })
        .unwrap();
    let block_rows: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(row_count), 0) FROM clob_replay_blocks",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(report.readiness.is_backtest_ready());
    assert_eq!(report.compact_clob_rows, 2);
    assert_eq!(row_storage_rows, 0);
    assert_eq!(block_rows, 2);
}
