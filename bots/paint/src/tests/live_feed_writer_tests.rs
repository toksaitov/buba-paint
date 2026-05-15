use std::thread;
use std::time::{Duration, Instant};

use tempfile::NamedTempFile;

use super::{FeedEventWriter, FeedEventWriterConfig, terminal_sqlite_write_error};
use crate::db::database::Database;
use crate::types::{FeedEvent, ReplayFidelity};

/// Build a default writer config for tests.
fn writer_config() -> FeedEventWriterConfig {
    FeedEventWriterConfig {
        queue_capacity: 16,
        batch_size: 2,
        flush_interval_ms: 10,
        compact_clob_replay: false,
        clob_block_max_rows: 10,
        clob_block_max_ms: 10,
        clob_block_zstd_level: 1,
    }
}

/// Initialize one temporary runtime database for worker tests.
fn init_runtime_db(tmp_db: &NamedTempFile) {
    let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
    db.close();
}

/// Build one replay-grade feed event for writer tests.
fn sample_event(index: u64) -> FeedEvent {
    FeedEvent {
        id: None,
        received_at_ms: 1_000 + index,
        event_at_ms: 1_000 + index,
        received_at_us: Some((1_000 + index) * 1_000),
        event_at_us: Some((1_000 + index) * 1_000),
        source: "binance".to_string(),
        event_type: "aggTrade".to_string(),
        source_topic: Some("btcusdt@aggTrade".to_string()),
        source_symbol: Some("BTCUSDT".to_string()),
        connection_id: None,
        sequence_key: Some(index.to_string()),
        market_id: None,
        asset_id: None,
        price: Some(42_000.0 + index as f64),
        trade_size: Some(0.1),
        signed_quantity: Some(0.1),
        best_bid: None,
        best_ask: None,
        bid_size: None,
        ask_size: None,
        depth_bid_notional: None,
        depth_ask_notional: None,
        depth_imbalance: None,
        microprice: None,
        payload_json: None,
        details_json: None,
        fidelity: ReplayFidelity::RawEvent,
    }
}

/// Build one compact CLOB top-of-book event for writer tests.
fn sample_clob_event(index: u64) -> FeedEvent {
    FeedEvent {
        id: None,
        received_at_ms: 2_000 + index,
        event_at_ms: 2_000 + index,
        received_at_us: Some((2_000 + index) * 1_000),
        event_at_us: Some((2_000 + index) * 1_000),
        source: "clob_up".to_string(),
        event_type: "price_change".to_string(),
        source_topic: Some("btc-up".to_string()),
        source_symbol: None,
        connection_id: Some("conn-1".to_string()),
        sequence_key: Some(index.to_string()),
        market_id: Some("m1".to_string()),
        asset_id: Some("up-1".to_string()),
        price: None,
        trade_size: None,
        signed_quantity: None,
        best_bid: Some(0.48),
        best_ask: Some(0.52),
        bid_size: Some(100.0 + index as f64),
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

/// Verifies that the writer persists queued rows in a batch.
#[test]
fn writer_persists_queued_rows() {
    let tmp_db = NamedTempFile::new().unwrap();
    init_runtime_db(&tmp_db);
    let writer =
        FeedEventWriter::start(tmp_db.path().to_string_lossy().to_string(), writer_config())
            .unwrap();

    assert!(writer.try_enqueue(sample_event(1)));
    assert!(writer.try_enqueue(sample_event(2)));
    writer.shutdown();

    let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
    let count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))
        .unwrap();
    db.close();
    assert_eq!(count, 2);
}

/// Verifies that replay-grade CLOB rows are persisted as compressed blocks.
#[test]
fn writer_persists_replay_grade_clob_rows_as_blocks() {
    let tmp_db = NamedTempFile::new().unwrap();
    init_runtime_db(&tmp_db);
    let writer = FeedEventWriter::start(
        tmp_db.path().to_string_lossy().to_string(),
        FeedEventWriterConfig {
            compact_clob_replay: true,
            clob_block_max_rows: 2,
            ..writer_config()
        },
    )
    .unwrap();

    assert!(writer.try_enqueue(sample_clob_event(1)));
    assert!(writer.try_enqueue(sample_clob_event(2)));
    writer.shutdown();

    let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
    let generic_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))
        .unwrap();
    let compact_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM clob_replay_events", [], |row| {
            row.get(0)
        })
        .unwrap();
    let block_rows: u64 = db
        .conn()
        .query_row(
            "SELECT COALESCE(SUM(row_count), 0) FROM clob_replay_blocks",
            [],
            |row| row.get(0),
        )
        .unwrap();
    db.close();

    assert_eq!(generic_count, 0);
    assert_eq!(compact_count, 0);
    assert_eq!(block_rows, 2);
}

/// Verifies that the writer records disconnected worker drops without blocking callers.
#[test]
fn writer_reports_disconnected_worker_drop() {
    let temp_dir = tempfile::tempdir().unwrap();
    let writer = FeedEventWriter::start(
        temp_dir.path().to_string_lossy().to_string(),
        FeedEventWriterConfig {
            queue_capacity: 1,
            batch_size: 1,
            flush_interval_ms: 10,
            compact_clob_replay: false,
            clob_block_max_rows: 10,
            clob_block_max_ms: 10,
            clob_block_zstd_level: 1,
        },
    )
    .unwrap();

    thread::sleep(Duration::from_millis(20));
    let result = writer.try_enqueue(sample_event(1));
    let snapshot = writer.snapshot();
    writer.shutdown();

    assert!(!result);
    assert!(snapshot.dropped > 0);
}

/// Verifies shutdown is bounded even when the channel is full.
#[test]
fn writer_shutdown_with_full_queue_is_bounded() {
    let tmp_db = NamedTempFile::new().unwrap();
    init_runtime_db(&tmp_db);
    let mut writer = FeedEventWriter::start(
        tmp_db.path().to_string_lossy().to_string(),
        FeedEventWriterConfig {
            queue_capacity: 1,
            batch_size: 10_000,
            flush_interval_ms: 60_000,
            compact_clob_replay: false,
            clob_block_max_rows: 10,
            clob_block_max_ms: 10,
            clob_block_zstd_level: 1,
        },
    )
    .unwrap();
    let _ = writer.try_enqueue(sample_event(1));
    let _ = writer.try_enqueue(sample_event(2));

    let started = Instant::now();
    assert!(writer.shutdown_with_timeout(Duration::from_millis(500)));
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// Verifies terminal SQLite writer errors are classified as runtime-fatal.
#[test]
fn writer_classifies_terminal_sqlite_errors() {
    assert!(terminal_sqlite_write_error(&anyhow::anyhow!(
        "database disk image is malformed"
    )));
    assert!(terminal_sqlite_write_error(&anyhow::anyhow!(
        "disk I/O error"
    )));
    assert!(!terminal_sqlite_write_error(&anyhow::anyhow!(
        "database is locked"
    )));
}
