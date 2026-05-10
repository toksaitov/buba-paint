use std::thread;
use std::time::{Duration, Instant};

use tempfile::NamedTempFile;

use super::{FeedEventWriter, FeedEventWriterConfig, terminal_sqlite_write_error};
use crate::db::database::Database;
use crate::types::{FeedEvent, ReplayFidelity};

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

/// Verifies that the writer persists queued rows in a batch.
#[test]
fn writer_persists_queued_rows() {
    let tmp_db = NamedTempFile::new().unwrap();
    init_runtime_db(&tmp_db);
    let writer = FeedEventWriter::start(
        tmp_db.path().to_string_lossy().to_string(),
        FeedEventWriterConfig {
            queue_capacity: 16,
            batch_size: 2,
            flush_interval_ms: 10,
            compact_clob_replay: false,
        },
    )
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
