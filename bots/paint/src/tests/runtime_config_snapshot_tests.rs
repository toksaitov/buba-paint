use super::*;
use crate::config::{Config, ExecutionMode};
use tempfile::NamedTempFile;

/// Builds the snapshot from a default Config and asserts every top-level group is present.
#[test]
fn builds_expected_shape_for_default_config() {
    let cfg = Config::default();
    let snapshot = build_runtime_config_snapshot(&cfg, 1_700_000_000_000);

    assert_eq!(snapshot.execution_mode, "paper");
    assert_eq!(snapshot.process_start_time_ms, 1_700_000_000_000);
    assert_eq!(snapshot.package_version, env!("CARGO_PKG_VERSION"));
    assert!(!snapshot.config_fingerprint.is_empty());

    assert_eq!(snapshot.feed_event_storage_profile, "replay_grade");
    assert_eq!(
        snapshot.live_runtime_max_db_bytes,
        cfg.live_runtime_max_db_bytes
    );

    assert_eq!(snapshot.latency_arb.enabled, cfg.latency_arb_enabled);
    assert_eq!(
        snapshot.latency_arb.momentum_threshold,
        cfg.latency_arb_momentum_threshold
    );
    assert_eq!(snapshot.spread_capture.enabled, cfg.spread_capture_enabled);
    assert_eq!(
        snapshot.calm_persistence.enabled,
        cfg.calm_persistence_enabled
    );

    assert_eq!(snapshot.risk.starting_balance, cfg.starting_balance);
    assert_eq!(
        snapshot.risk.live_session_cash_cap_usd,
        cfg.live_session_cash_cap_usd
    );

    assert_eq!(
        snapshot.pending_settlement.family_reserve_fraction,
        cfg.pending_settlement_family_reserve_fraction
    );
    assert_eq!(snapshot.fees.taker_fee_rate, cfg.taker_fee_rate);
    assert_eq!(
        snapshot.feed_freshness.max_book_staleness_ms,
        cfg.max_book_staleness_ms
    );
    assert_eq!(
        snapshot.worker_budgets.feed_event_writer_queue_capacity,
        cfg.feed_event_writer_queue_capacity as u64
    );
}

/// Confirms the snapshot serializes to JSON that round-trips back into a structured value.
#[test]
fn serializes_to_valid_json() {
    let cfg = Config::default();
    let snapshot = build_runtime_config_snapshot(&cfg, 1_700_000_000_000);
    let json = serde_json::to_string(&snapshot).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["execution_mode"].as_str(), Some("paper"));
    assert_eq!(
        parsed["process_start_time_ms"].as_u64(),
        Some(1_700_000_000_000)
    );
    assert!(parsed["latency_arb"].is_object());
    assert!(parsed["spread_capture"].is_object());
    assert!(parsed["calm_persistence"].is_object());
    assert!(parsed["risk"].is_object());
    assert!(parsed["pending_settlement"].is_object());
    assert!(parsed["fees"].is_object());
    assert!(parsed["feed_freshness"].is_object());
    assert!(parsed["worker_budgets"].is_object());
}

/// The snapshot's fingerprint must match the readonly fingerprint when in live_readonly mode.
#[test]
fn fingerprint_matches_readonly_helper_in_readonly_mode() {
    let mut cfg = Config::default();
    cfg.execution_mode = ExecutionMode::LiveReadonly;
    let snapshot = build_runtime_config_snapshot(&cfg, 1);
    assert_eq!(
        snapshot.config_fingerprint,
        crate::live_readonly::readonly_config_fingerprint(&cfg)
    );
}

/// The snapshot's fingerprint must match the live-trading fingerprint when in live_trading mode.
#[test]
fn fingerprint_matches_live_trading_helper_in_live_trading_mode() {
    let mut cfg = Config::default();
    cfg.execution_mode = ExecutionMode::LiveTrading;
    let snapshot = build_runtime_config_snapshot(&cfg, 1);
    assert_eq!(
        snapshot.config_fingerprint,
        crate::live::live_trading_config_fingerprint(&cfg)
    );
}

/// Paranoia guard: no secret-bearing substrings ever appear in the serialized JSON, even when a poisoned db_path is configured.
#[test]
fn does_not_serialize_forbidden_substrings() {
    let mut cfg = Config::default();
    cfg.db_path = "/tmp/paint.db".to_string();
    let snapshot = build_runtime_config_snapshot(&cfg, 1);
    let json = serde_json::to_string(&snapshot).unwrap();

    for forbidden in [
        "AGENT_SECRET",
        "JWT_SECRET",
        "private_key",
        "relayer_secret",
        "password",
    ] {
        assert!(
            !json.contains(forbidden),
            "snapshot JSON unexpectedly contained '{forbidden}': {json}"
        );
    }
}

/// Persist the snapshot through the run_metadata table and read it back as a JSON value.
#[test]
fn persist_then_get_round_trips_through_run_metadata() {
    let tmp = NamedTempFile::new().unwrap();
    let db = crate::db::database::Database::new(tmp.path().to_str().unwrap()).unwrap();
    let cfg = Config::default();
    persist_runtime_config_snapshot(&db, &cfg, 1_700_000_000_000).unwrap();

    let raw = db
        .get_run_metadata("runtime_config_snapshot")
        .unwrap()
        .expect("snapshot row present");
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["execution_mode"].as_str(), Some("paper"));
    assert_eq!(
        parsed["process_start_time_ms"].as_u64(),
        Some(1_700_000_000_000)
    );
    assert_eq!(
        parsed["package_version"].as_str(),
        Some(env!("CARGO_PKG_VERSION"))
    );
}

/// usize worker-budget fields serialize as bounded u64 values in the JSON.
#[test]
fn worker_budget_fields_serialize_as_u64() {
    let cfg = Config::default();
    let snapshot = build_runtime_config_snapshot(&cfg, 1);
    let json = serde_json::to_value(&snapshot).unwrap();
    let budgets = &json["worker_budgets"];
    assert!(budgets["feed_event_writer_queue_capacity"].is_u64());
    assert!(budgets["live_decision_queue_capacity"].is_u64());
    assert!(budgets["live_submission_queue_capacity"].is_u64());
}
