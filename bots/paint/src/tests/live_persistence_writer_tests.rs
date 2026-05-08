use std::thread;
use std::time::Duration;

use tempfile::NamedTempFile;

use super::{LivePersistenceEvent, LivePersistenceWriter, LivePersistenceWriterConfig};
use crate::db::database::Database;
use crate::types::{ReplayFidelity, Signal, SignalDirection, SignalTelemetry};

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

#[test]
/// Verifies queued decision evidence is persisted off the hot path.
fn writer_persists_decision_signal_events() {
    let tmp_db = NamedTempFile::new().unwrap();
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
