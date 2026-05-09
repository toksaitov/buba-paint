use std::thread;
use std::time::{Duration, Instant};

use tempfile::NamedTempFile;

use super::{
    LivePersistenceEvent, LivePersistenceWriter, LivePersistenceWriterConfig,
    terminal_sqlite_write_error,
};
use crate::db::database::Database;
use crate::types::{
    FeedHealthEvent, MarketWindow, ReplayFidelity, Signal, SignalDirection, SignalTelemetry,
};

/// Initialize one temporary runtime database for worker tests.
fn init_runtime_db(tmp_db: &NamedTempFile) {
    let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
    db.close();
}

/// Build one telemetry-bearing signal for persistence writer tests.
fn sample_signal(index: i64) -> Signal {
    Signal {
        timestamp: 10_000 + index.unsigned_abs(),
        strategy: "latency-arb".to_string(),
        strategy_version: "test".to_string(),
        feature_mode: "raw_event_full".to_string(),
        direction: SignalDirection::Up,
        confidence: 1.0,
        binance_price: 75_000.0,
        chainlink_price: 75_000.0,
        up_ask: 0.50,
        down_ask: 0.50,
        up_bid: 0.49,
        down_bid: 0.49,
        expected_edge: Some(0.05),
        metadata: serde_json::json!({"writerTest": true}),
        telemetry: Some(SignalTelemetry {
            generated_at_ms: 10_000 + index.unsigned_abs(),
            generated_at_us: Some((10_000 + index.unsigned_abs()) * 1_000),
            order_submitted_at_ms: Some(10_100),
            order_submitted_at_us: Some(10_100_000),
            expected_arrival_at_ms: Some(10_125),
            expected_arrival_at_us: Some(10_125_000),
            order_processed_at_ms: None,
            order_processed_at_us: None,
            effective_arrival_delay_ms: None,
            binance_age_ms: Some(0),
            chainlink_age_ms: Some(0),
            clob_age_ms: Some(0),
            quote_age_ms: Some(0),
            book_staleness_ms: Some(0),
            expected_fee: Some(0.01),
            expected_slippage: Some(0.0),
            expected_edge: Some(0.05),
            available_feature_count: 3,
            decision_status: "generated".to_string(),
            rejection_reason: None,
            features_json: serde_json::json!({"writerTest": true}),
        }),
    }
}

/// Build one decision-evidence event with an explicit runtime signal id.
fn signal_event(signal_id: i64) -> LivePersistenceEvent {
    LivePersistenceEvent::Signal {
        signal_id,
        signal: Box::new(sample_signal(signal_id)),
        market_id: "mkt-test".to_string(),
        execution_fidelity: ReplayFidelity::RawEvent,
        order_submitted_at_ms: Some(10_100),
        expected_arrival_at_ms: Some(10_125),
        decision_status: "submitted".to_string(),
        rejection_reason: None,
    }
}

/// Build one market window for metadata persistence tests.
fn market_window() -> MarketWindow {
    MarketWindow {
        market_id: "mkt-test".to_string(),
        question: "Will BTC go up?".to_string(),
        up_token_id: "up-token".to_string(),
        down_token_id: "down-token".to_string(),
        condition_id: "condition".to_string(),
        start_time: 1_000,
        end_time: 301_000,
        slug: "btc-test".to_string(),
        outcome: None,
        resolution_source: Some("gamma".to_string()),
        fee_profile: Some("crypto".to_string()),
        order_min_size: Some(1.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: None,
        taker_base_fee: None,
        rewards_min_size: None,
        rewards_max_spread: None,
        fees_enabled: Some(true),
        fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".to_string()),
        token_fee_rates_json: None,
        accepting_orders: Some(true),
        accepting_orders_timestamp: Some("2026-05-08T00:00:00Z".to_string()),
        clear_book_on_start: Some(false),
    }
}

#[test]
/// Verifies queued decision evidence is persisted off the hot path.
fn writer_persists_decision_signal_events() {
    let tmp_db = NamedTempFile::new().unwrap();
    init_runtime_db(&tmp_db);
    let writer = LivePersistenceWriter::start(
        tmp_db.path().to_string_lossy().to_string(),
        LivePersistenceWriterConfig {
            queue_capacity: 16,
            batch_size: 2,
            flush_interval_ms: 10,
        },
    )
    .unwrap();

    assert!(writer.try_enqueue(signal_event(-1)));
    assert!(writer.try_enqueue(signal_event(-2)));
    writer.shutdown();

    let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
    let signal_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM signals", [], |row| row.get(0))
        .unwrap();
    let metric_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM signal_metrics", [], |row| row.get(0))
        .unwrap();
    db.close();
    assert_eq!(signal_count, 2);
    assert_eq!(metric_count, 2);
}

#[test]
/// Verifies a failed writer is observable without blocking the caller.
fn writer_reports_disconnected_worker_drop() {
    let temp_dir = tempfile::tempdir().unwrap();
    let writer = LivePersistenceWriter::start(
        temp_dir.path().to_string_lossy().to_string(),
        LivePersistenceWriterConfig {
            queue_capacity: 1,
            batch_size: 1,
            flush_interval_ms: 10,
        },
    )
    .unwrap();

    thread::sleep(Duration::from_millis(20));
    let result = writer.try_enqueue(signal_event(-1));
    let snapshot = writer.snapshot();
    writer.shutdown();

    assert!(!result);
    assert!(snapshot.dropped > 0);
}

/// Verifies shutdown is bounded even when the persistence queue is full.
#[test]
fn writer_shutdown_with_full_queue_is_bounded() {
    let tmp_db = NamedTempFile::new().unwrap();
    init_runtime_db(&tmp_db);
    let mut writer = LivePersistenceWriter::start(
        tmp_db.path().to_string_lossy().to_string(),
        LivePersistenceWriterConfig {
            queue_capacity: 1,
            batch_size: 10_000,
            flush_interval_ms: 60_000,
        },
    )
    .unwrap();
    let _ = writer.try_enqueue(signal_event(-1));
    let _ = writer.try_enqueue(signal_event(-2));

    let started = Instant::now();
    assert!(writer.shutdown_with_timeout(Duration::from_millis(500)));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
/// Verifies runtime maintenance rows are persisted through the bounded writer.
fn writer_persists_feed_health_metadata_and_market_upsert() {
    let tmp_db = NamedTempFile::new().unwrap();
    init_runtime_db(&tmp_db);
    let writer = LivePersistenceWriter::start(
        tmp_db.path().to_string_lossy().to_string(),
        LivePersistenceWriterConfig {
            queue_capacity: 16,
            batch_size: 4,
            flush_interval_ms: 10,
        },
    )
    .unwrap();

    assert!(writer.try_enqueue(LivePersistenceEvent::MarketUpsert(
        Box::new(market_window())
    )));
    assert!(
        writer.try_enqueue(LivePersistenceEvent::FeedHealth(Box::new(
            FeedHealthEvent {
                id: None,
                timestamp_ms: 2_000,
                timestamp_us: Some(2_000_000),
                source: "clob".to_string(),
                event_type: "connected".to_string(),
                connection_id: Some("conn-1".to_string()),
                market_id: Some("mkt-test".to_string()),
                details_json: None,
            },
        )))
    );
    assert!(writer.try_enqueue(LivePersistenceEvent::RunMetadata(vec![(
        "runtime_capture_health".to_string(),
        "observing".to_string(),
        2_000,
    )])));
    writer.shutdown();

    let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
    let market_count: u64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM markets WHERE market_id = 'mkt-test'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let feed_health_count: u64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM feed_health_events WHERE source = 'clob'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let metadata = db.get_run_metadata("runtime_capture_health").unwrap();
    db.close();

    assert_eq!(market_count, 1);
    assert_eq!(feed_health_count, 1);
    assert_eq!(metadata.as_deref(), Some("observing"));
}

/// Verifies terminal SQLite errors are fatal for runtime evidence persistence.
#[test]
fn writer_classifies_terminal_sqlite_errors() {
    assert!(terminal_sqlite_write_error(&anyhow::anyhow!(
        "database disk image is malformed"
    )));
    assert!(!terminal_sqlite_write_error(&anyhow::anyhow!(
        "constraint failed"
    )));
}
