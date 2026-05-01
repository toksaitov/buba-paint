use std::str::FromStr;

use super::*;
use crate::db::database::Database;
use crate::types::LiveSession;

/// Create one temporary test database.
fn temp_db() -> (Database, tempfile::NamedTempFile) {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
    (db, tmp)
}

/// Insert one unfinished live session for control tests.
fn insert_session(db: &Database, execution_mode: &str) -> i64 {
    db.insert_live_session(&LiveSession {
        id: None,
        started_at_ms: 1_000,
        ended_at_ms: None,
        status: "disarmed".to_string(),
        execution_mode: execution_mode.to_string(),
        wallet_address: Some("0xwallet".to_string()),
        proxy_wallet: Some("0xproxy".to_string()),
        enabled_strategies_json: "[\"latency-arb\"]".to_string(),
        config_fingerprint: "{}".to_string(),
        cash_cap_usd: 100.0,
        details_json: Some("{}".to_string()),
    })
    .unwrap()
}

/// Queuing a live-control command requires an active live-trading session.
#[test]
fn queues_command_for_active_live_trading_session() {
    let (db, _tmp) = temp_db();
    let session_id = insert_session(&db, "live_trading");

    let command_id = enqueue_live_control_command(
        &db,
        LiveControlAction::Arm,
        "operator",
        "fresh preflight passed",
        2_000,
    )
    .unwrap();

    let pending = db.pending_live_control_commands(session_id).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, Some(command_id));
    assert_eq!(pending[0].action, "arm");
    assert_eq!(pending[0].actor, "operator");
}

/// The control CLI must fail closed when only readonly sessions exist.
#[test]
fn rejects_command_without_live_trading_session() {
    let (db, _tmp) = temp_db();
    insert_session(&db, "live_readonly");

    let error =
        enqueue_live_control_command(&db, LiveControlAction::Arm, "operator", "wrong mode", 2_000)
            .unwrap_err();

    assert!(error.to_string().contains("live_trading"));
}

/// State transitions are recorded as durable state plus audit rows.
#[test]
fn records_live_control_state_transition() {
    let (db, _tmp) = temp_db();
    let session_id = insert_session(&db, "live_trading");

    let state_id = record_live_control_state(
        &db,
        session_id,
        "armed",
        "operator",
        "all gates healthy",
        3_000,
        Some("{\"preflight\":\"ok\"}"),
    )
    .unwrap();

    let latest = db.latest_live_control_state(session_id).unwrap().unwrap();
    assert_eq!(latest.id, Some(state_id));
    assert_eq!(latest.state, "armed");
    assert_eq!(latest.actor, "operator");
}

/// Operator action parsing only accepts the intentional control vocabulary.
#[test]
fn parses_known_actions() {
    assert_eq!(
        LiveControlAction::from_str("stop-after-flat").unwrap(),
        LiveControlAction::StopAfterFlat
    );
    assert!(LiveControlAction::from_str("restart").is_err());
}
