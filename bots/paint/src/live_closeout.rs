use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use rusqlite::params;
use rusqlite::types::ValueRef;
use serde_json::{Map, Value, json};

use crate::backtest::replay_quality;
use crate::db::database::Database;
use crate::types::ControlAuditEntry;

/// Options for exporting one live-trading run closeout package.
#[derive(Debug, Clone)]
pub struct LiveCloseoutOptions {
    pub db_path: String,
    pub output_dir: String,
    pub actor: String,
    pub reason: String,
    pub generated_at_ms: u64,
}

struct CloseoutFileContext<'a> {
    options: &'a LiveCloseoutOptions,
    output_dir: &'a Path,
    session_id: i64,
    started_at_ms: u64,
    ended_at_ms: u64,
    quick_check: &'a str,
    replay_quality: &'a CloseoutReplayQuality,
    exported_files: &'a [String],
}

struct CloseoutReplayQuality {
    text: String,
    class: String,
    missing_required: Vec<String>,
    start_time: u64,
    end_time: u64,
    error: Option<String>,
}

/// Export live-trading closeout artifacts and record the export in the audit ledger.
pub fn run_live_closeout(options: &LiveCloseoutOptions) -> anyhow::Result<()> {
    validate_closeout_text("actor", &options.actor)?;
    validate_closeout_text("reason", &options.reason)?;
    let output_dir = PathBuf::from(&options.output_dir);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("creating closeout output dir {}", output_dir.display()))?;

    let db = Database::new(&options.db_path)?;
    let session = db
        .latest_live_trading_session()?
        .context("no live_trading session found for closeout")?;
    let session_id = session
        .id
        .context("latest live_trading session is missing database id")?;
    let quick_check = sqlite_quick_check(db.conn())?;
    let end_ms = if let Some(ended_at_ms) = session.ended_at_ms {
        ended_at_ms
    } else {
        latest_live_timestamp_ms(db.conn())?.unwrap_or(options.generated_at_ms)
    };
    let replay_quality = replay_quality_report(&options.db_path, session.started_at_ms, end_ms);
    persist_closeout_marker(&db, session_id, options, &output_dir)?;
    let exported_files = export_live_tables(db.conn(), session_id, &output_dir)?;
    write_closeout_files(&CloseoutFileContext {
        options,
        output_dir: &output_dir,
        session_id,
        started_at_ms: session.started_at_ms,
        ended_at_ms: end_ms,
        quick_check: &quick_check,
        replay_quality: &replay_quality,
        exported_files: &exported_files,
    })?;
    db.close();
    Ok(())
}

/// Reject empty closeout actor or reason fields.
fn validate_closeout_text(field: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}

/// Run the `SQLite` quick-check and return the raw result string.
fn sqlite_quick_check(conn: &rusqlite::Connection) -> anyhow::Result<String> {
    conn.query_row("PRAGMA quick_check", [], |row| row.get(0))
        .context("running SQLite quick_check")
}

/// Return the latest live timestamp observed in evidence tables.
fn latest_live_timestamp_ms(conn: &rusqlite::Connection) -> anyhow::Result<Option<u64>> {
    conn.query_row(
        "SELECT MAX(ts) FROM (
            SELECT started_at_ms AS ts FROM live_sessions
            UNION ALL SELECT COALESCE(ended_at_ms, started_at_ms) FROM live_sessions
            UNION ALL SELECT created_at_ms FROM live_order_intents
            UNION ALL SELECT updated_at_ms FROM live_orders
            UNION ALL SELECT filled_at_ms FROM live_fills
            UNION ALL SELECT timestamp_ms FROM live_account_snapshots
            UNION ALL SELECT detected_redeemable_at_ms FROM live_redemptions
            UNION ALL SELECT timestamp_ms FROM live_reconciliation_events
            UNION ALL SELECT updated_at_ms FROM live_control_state
            UNION ALL SELECT requested_at_ms FROM live_control_commands
            UNION ALL SELECT COALESCE(applied_at_ms, requested_at_ms) FROM live_control_commands
        )",
        [],
        |row| row.get(0),
    )
    .context("querying latest live timestamp")
}

/// Build a replay-quality report for the live closeout interval.
fn replay_quality_report(db_path: &str, start_ms: u64, end_ms: u64) -> CloseoutReplayQuality {
    match replay_quality::analyze_path(db_path, start_ms, end_ms) {
        Ok(report) => CloseoutReplayQuality {
            text: replay_quality::format_report(&report),
            class: report.class.as_str().to_string(),
            missing_required: report
                .missing_required_keys()
                .into_iter()
                .map(str::to_string)
                .collect(),
            start_time: report.start_time,
            end_time: report.end_time,
            error: None,
        },
        Err(error) => CloseoutReplayQuality {
            text: format!("replay-quality report failed: {error}"),
            class: "descriptive_only".to_string(),
            missing_required: Vec::new(),
            start_time: start_ms,
            end_time: end_ms,
            error: Some(error.to_string()),
        },
    }
}

/// Export live evidence tables as JSON files.
fn export_live_tables(
    conn: &rusqlite::Connection,
    session_id: i64,
    output_dir: &Path,
) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    files.push(write_session_query_json(
        conn,
        output_dir,
        "live_session.json",
        "SELECT * FROM live_sessions WHERE id = ?1",
        session_id,
    )?);
    for (file_name, query) in [
        (
            "live_order_intents.json",
            "SELECT * FROM live_order_intents WHERE session_id = ?1 ORDER BY created_at_ms, id",
        ),
        (
            "live_orders.json",
            "SELECT * FROM live_orders WHERE session_id = ?1 ORDER BY updated_at_ms, id",
        ),
        (
            "live_fills.json",
            "SELECT * FROM live_fills WHERE session_id = ?1 ORDER BY filled_at_ms, id",
        ),
        (
            "live_account_snapshots.json",
            "SELECT * FROM live_account_snapshots WHERE session_id = ?1 ORDER BY timestamp_ms, id",
        ),
        (
            "live_redemptions.json",
            "SELECT * FROM live_redemptions WHERE session_id = ?1 ORDER BY detected_redeemable_at_ms, id",
        ),
        (
            "live_reconciliation_events.json",
            "SELECT * FROM live_reconciliation_events WHERE session_id = ?1 ORDER BY timestamp_ms, id",
        ),
        (
            "live_control_state.json",
            "SELECT * FROM live_control_state WHERE session_id = ?1 ORDER BY updated_at_ms, id",
        ),
        (
            "live_control_commands.json",
            "SELECT * FROM live_control_commands WHERE session_id = ?1 ORDER BY requested_at_ms, id",
        ),
    ] {
        files.push(write_session_query_json(
            conn, output_dir, file_name, query, session_id,
        )?);
    }
    files.push(write_query_json(
        conn,
        output_dir,
        "control_audit.json",
        "SELECT * FROM control_audit ORDER BY timestamp_ms, id",
    )?);
    Ok(files)
}

/// Write a session-scoped SQL query result as pretty JSON.
fn write_session_query_json(
    conn: &rusqlite::Connection,
    output_dir: &Path,
    file_name: &str,
    query: &str,
    session_id: i64,
) -> anyhow::Result<String> {
    let mut stmt = conn
        .prepare(query)
        .with_context(|| format!("preparing closeout export query for {file_name}"))?;
    let column_names = statement_column_names(&stmt);
    let rows = stmt.query_map(params![session_id], |row| row_to_json(row, &column_names))?;
    write_rows_json(output_dir, file_name, rows)
}

/// Write an unscoped SQL query result as pretty JSON.
fn write_query_json(
    conn: &rusqlite::Connection,
    output_dir: &Path,
    file_name: &str,
    query: &str,
) -> anyhow::Result<String> {
    let mut stmt = conn
        .prepare(query)
        .with_context(|| format!("preparing closeout export query for {file_name}"))?;
    let column_names = statement_column_names(&stmt);
    let rows = stmt.query_map([], |row| row_to_json(row, &column_names))?;
    write_rows_json(output_dir, file_name, rows)
}

/// Return owned column names for one prepared statement.
fn statement_column_names(stmt: &rusqlite::Statement<'_>) -> Vec<String> {
    stmt.column_names()
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Convert one `SQLite` row into a JSON object.
fn row_to_json(row: &rusqlite::Row<'_>, column_names: &[String]) -> rusqlite::Result<Value> {
    let mut object = Map::new();
    for (index, column_name) in column_names.iter().enumerate() {
        object.insert(
            column_name.clone(),
            sqlite_value_to_json(row.get_ref(index)?),
        );
    }
    Ok(Value::Object(object))
}

/// Convert one `SQLite` value reference into JSON.
fn sqlite_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => json!(String::from_utf8_lossy(value).to_string()),
        ValueRef::Blob(value) => json!(format!("<blob:{} bytes>", value.len())),
    }
}

/// Collect and write query rows to one JSON file.
fn write_rows_json<'stmt>(
    output_dir: &Path,
    file_name: &str,
    rows: impl Iterator<Item = rusqlite::Result<Value>> + 'stmt,
) -> anyhow::Result<String> {
    let values = rows.collect::<Result<Vec<_>, _>>()?;
    let path = output_dir.join(file_name);
    fs::write(&path, serde_json::to_string_pretty(&values)?)
        .with_context(|| format!("writing closeout export {}", path.display()))?;
    Ok(file_name.to_string())
}

/// Write summary, manifest, integrity, replay-quality, and postmortem files.
fn write_closeout_files(context: &CloseoutFileContext<'_>) -> anyhow::Result<()> {
    let replay_quality = closeout_replay_quality_json(context.replay_quality);
    let summary = json!({
        "session_id": context.session_id,
        "db_path": context.options.db_path,
        "started_at_ms": context.started_at_ms,
        "ended_at_ms": context.ended_at_ms,
        "generated_at_ms": context.options.generated_at_ms,
        "actor": context.options.actor,
        "reason": context.options.reason,
        "sqlite_quick_check": context.quick_check,
        "replay_quality": replay_quality.clone(),
    });
    write_json_file(context.output_dir, "summary.json", &summary)?;
    let manifest = json!({
        "kind": "buba_live_closeout",
        "session_id": context.session_id,
        "db_path": context.options.db_path,
        "output_dir": context.output_dir.display().to_string(),
        "generated_at_ms": context.options.generated_at_ms,
        "replay_quality": replay_quality,
        "files": context.exported_files,
    });
    write_json_file(context.output_dir, "manifest.json", &manifest)?;
    write_text_file(context.output_dir, "db_integrity.txt", context.quick_check)?;
    write_text_file(
        context.output_dir,
        "replay_quality.txt",
        &context.replay_quality.text,
    )?;
    let replay_status = if context.replay_quality.class == "sweep_grade" {
        "sweep-grade"
    } else {
        "descriptive only, not research-grade"
    };
    write_text_file(
        context.output_dir,
        "postmortem.md",
        &format!(
            "# Live Trading Closeout Postmortem\n\nStatus: draft, required before another funded run.\nReplay quality: {}.\n\nSession: {}\nActor: {}\nReason: {}\n\n## Executive Summary\n\n- Fill this in before starting a new run DB.\n\n## Halt Or Closeout Trigger\n\n- Fill this in from `live_reconciliation_events.json`, `live_control_state.json`, and `summary.json`.\n\n## Replay-Quality Review\n\n- Review `replay_quality.txt` and `summary.json` before using this run for research.\n\n## Risk Review\n\n- Review high-water mark, trough/current equity, daily loss, and session drawdown from `live_session.json`.\n\n## Order And Reconciliation Review\n\n- Review `live_order_intents.json`, `live_orders.json`, `live_fills.json`, and `live_reconciliation_events.json`.\n\n## Required Decision\n\n- Do not reuse this DB for another funded run. Start a new run DB only after this postmortem is complete.\n",
            replay_status, context.session_id, context.options.actor, context.options.reason
        ),
    )?;
    Ok(())
}

/// Build the closeout replay-quality JSON summary.
fn closeout_replay_quality_json(replay_quality: &CloseoutReplayQuality) -> Value {
    json!({
        "class": replay_quality.class.clone(),
        "missing_required": replay_quality.missing_required.clone(),
        "start_time": replay_quality.start_time,
        "end_time": replay_quality.end_time,
        "error": replay_quality.error.clone(),
    })
}

/// Write one pretty JSON file.
fn write_json_file(output_dir: &Path, file_name: &str, value: &Value) -> anyhow::Result<()> {
    let path = output_dir.join(file_name);
    fs::write(&path, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("writing closeout JSON file {}", path.display()))
}

/// Write one text file.
fn write_text_file(output_dir: &Path, file_name: &str, value: &str) -> anyhow::Result<()> {
    let path = output_dir.join(file_name);
    fs::write(&path, value).with_context(|| format!("writing closeout file {}", path.display()))
}

/// Persist the closeout marker into session details and control audit.
fn persist_closeout_marker(
    db: &Database,
    session_id: i64,
    options: &LiveCloseoutOptions,
    output_dir: &Path,
) -> anyhow::Result<()> {
    let session = db
        .latest_live_trading_session()?
        .context("no live_trading session found after closeout export")?;
    let mut details = session
        .details_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}));
    if !details.is_object() {
        details = json!({});
    }
    details["closeout"] = json!({
        "exported_at_ms": options.generated_at_ms,
        "output_dir": output_dir.display().to_string(),
        "actor": options.actor,
        "reason": options.reason,
    });
    db.update_live_session_metadata(
        session_id,
        &session.status,
        session.wallet_address.as_deref(),
        session.proxy_wallet.as_deref(),
        Some(&details.to_string()),
    )?;
    db.log_control_audit(&ControlAuditEntry {
        id: None,
        timestamp_ms: options.generated_at_ms,
        actor: options.actor.clone(),
        action: "live_closeout_exported".to_string(),
        target: Some(session_id.to_string()),
        details_json: Some(
            json!({
                "reason": options.reason,
                "output_dir": output_dir.display().to_string(),
            })
            .to_string(),
        ),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LiveSession;
    use tempfile::{NamedTempFile, TempDir};

    /// Verifies that closeout writes required artifacts and audit state.
    #[test]
    fn live_closeout_writes_artifacts_and_audit_marker() {
        let db_file = NamedTempFile::new().unwrap();
        let db_path = db_file.path().to_str().unwrap();
        let db = Database::new(db_path).unwrap();
        let session_id = db
            .insert_live_session(&LiveSession {
                id: None,
                started_at_ms: 1_000,
                ended_at_ms: None,
                status: "halted".to_string(),
                execution_mode: "live_trading".to_string(),
                wallet_address: Some("0xwallet".to_string()),
                proxy_wallet: Some("0xproxy".to_string()),
                enabled_strategies_json: "[\"latency-arb\"]".to_string(),
                config_fingerprint: "fingerprint".to_string(),
                cash_cap_usd: 100.0,
                details_json: Some(
                    json!({
                        "state": "halted",
                        "risk": {
                            "terminal_reason": "test halt",
                            "terminal_at_ms": 2_000,
                            "current_equity": 80.0
                        }
                    })
                    .to_string(),
                ),
            })
            .unwrap();
        db.close();
        let output_dir = TempDir::new().unwrap();

        run_live_closeout(&LiveCloseoutOptions {
            db_path: db_path.to_string(),
            output_dir: output_dir.path().to_str().unwrap().to_string(),
            actor: "operator".to_string(),
            reason: "halt analysis".to_string(),
            generated_at_ms: 3_000,
        })
        .unwrap();

        for file in [
            "summary.json",
            "manifest.json",
            "db_integrity.txt",
            "replay_quality.txt",
            "postmortem.md",
            "live_session.json",
            "control_audit.json",
        ] {
            assert!(output_dir.path().join(file).exists(), "{file} missing");
        }
        let summary = fs::read_to_string(output_dir.path().join("summary.json")).unwrap();
        let postmortem = fs::read_to_string(output_dir.path().join("postmortem.md")).unwrap();
        assert!(summary.contains("\"class\": \"empty\""));
        assert!(postmortem.contains("descriptive only, not research-grade"));
        let conn = rusqlite::Connection::open(db_path).unwrap();
        let action: String = conn
            .query_row(
                "SELECT action FROM control_audit ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let details: String = conn
            .query_row(
                "SELECT details_json FROM live_sessions WHERE id = ?1",
                [session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(action, "live_closeout_exported");
        assert!(details.contains("\"closeout\""));
    }
}
