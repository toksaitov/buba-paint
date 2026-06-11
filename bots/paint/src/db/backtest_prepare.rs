use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::json;

use crate::backtest::backtest_input::BacktestInputReport;

/// Options for preparing a sweep-optimized backtest database.
pub struct PrepareBacktestInputOptions {
    pub data_path: String,
    pub output_path: String,
    pub start_time: u64,
    pub end_time: u64,
}

/// Result returned after preparing one backtest database.
pub struct PrepareBacktestInputReport {
    pub output_path: String,
    pub manifest_path: String,
    pub source_bytes: u64,
    pub output_bytes: u64,
    pub generic_feed_rows: i64,
    pub compact_clob_rows: i64,
    pub source_signal_rows: i64,
    pub source_trade_rows: i64,
    pub source_trade_result_rows: i64,
    pub readiness: BacktestInputReport,
}

/// Row counts for prepared replay and source-audit data.
#[derive(Debug, Clone, Copy)]
struct PreparedCounts {
    generic_feed: i64,
    compact_clob: i64,
    source_signals: i64,
    source_trades: i64,
    source_trade_results: i64,
}

/// Build a derived indexed DB for large backtests and sweeps.
pub fn prepare_backtest_input(
    options: &PrepareBacktestInputOptions,
) -> anyhow::Result<PrepareBacktestInputReport> {
    if Path::new(&options.output_path).exists() {
        bail!("output DB already exists: {}", options.output_path);
    }
    if let Some(parent) = Path::new(&options.output_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory: {}", parent.display()))?;
    }

    let source_bytes = file_size(&options.data_path)?;
    let output = Connection::open(&options.output_path)
        .with_context(|| format!("creating output DB: {}", options.output_path))?;
    crate::db::schema::run_migrations(&output)?;
    output.execute("ATTACH DATABASE ?1 AS src", [&options.data_path])?;
    copy_markets(&output, options.start_time, options.end_time)?;
    copy_generic_feed_events(&output, options.start_time, options.end_time)?;
    copy_legacy_clob_rows_as_compact(&output, options.start_time, options.end_time)?;
    copy_compact_clob_rows(&output, options.start_time, options.end_time)?;
    copy_clob_replay_blocks(&output, options.start_time, options.end_time)?;
    copy_table_intersection(&output, "run_metadata", None)?;
    copy_small_live_tables(&output)?;
    copy_source_audit_tables(&output, options.start_time, options.end_time)?;
    crate::db::schema::create_replay_indexes(&output)?;
    output.execute("DETACH DATABASE src", [])?;
    drop(output);

    let readiness = crate::backtest::backtest_input::validate_input(
        &options.output_path,
        options.start_time,
        options.end_time,
    )?;
    let output_bytes = file_size(&options.output_path)?;
    let counts = prepared_counts(&options.output_path)?;
    let manifest_path = manifest_path(&options.output_path);
    write_manifest(
        &manifest_path,
        options,
        source_bytes,
        output_bytes,
        counts,
        &readiness,
    )?;

    Ok(PrepareBacktestInputReport {
        output_path: options.output_path.clone(),
        manifest_path: manifest_path.to_string_lossy().to_string(),
        source_bytes,
        output_bytes,
        generic_feed_rows: counts.generic_feed,
        compact_clob_rows: counts.compact_clob,
        source_signal_rows: counts.source_signals,
        source_trade_rows: counts.source_trades,
        source_trade_result_rows: counts.source_trade_results,
        readiness,
    })
}

/// Copy overlapping market windows into the prepared database.
fn copy_markets(conn: &Connection, start_time: u64, end_time: u64) -> anyhow::Result<()> {
    let columns = common_columns(conn, "markets")?;
    if columns.is_empty() {
        return Ok(());
    }
    let column_list = columns.join(", ");
    conn.execute(
        &format!(
            "INSERT INTO markets ({column_list})
             SELECT {column_list}
             FROM src.markets
             WHERE end_time >= ?1 AND start_time <= ?2"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .context("copying markets")?;
    Ok(())
}

/// Copy non-CLOB or non-top-of-book generic feed rows.
fn copy_generic_feed_events(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<()> {
    if !source_table_exists(conn, "feed_events")? {
        return Ok(());
    }
    let columns = common_columns(conn, "feed_events")?;
    if columns.is_empty() {
        return Ok(());
    }
    let column_list = columns.join(", ");
    conn.execute(
        &format!(
            "INSERT INTO feed_events ({column_list})
             SELECT {column_list}
             FROM src.feed_events
             WHERE received_at_ms >= ?1
               AND received_at_ms <= ?2
               AND NOT (
                 source IN ('clob_up', 'clob_down')
                 AND event_type IN ('book', 'price_change', 'best_bid_ask')
                 AND best_bid IS NOT NULL
                 AND best_ask IS NOT NULL
                 AND bid_size IS NOT NULL
                 AND ask_size IS NOT NULL
               )"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .context("copying generic feed_events")?;
    Ok(())
}

/// Convert legacy generic CLOB rows into compact replay rows.
fn copy_legacy_clob_rows_as_compact(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<()> {
    if !source_table_exists(conn, "feed_events")? {
        return Ok(());
    }
    let received_at_us = source_column_expr(conn, "feed_events", "received_at_us", "NULL")?;
    let event_at_us = source_column_expr(conn, "feed_events", "event_at_us", "NULL")?;
    let source_topic = source_column_expr(conn, "feed_events", "source_topic", "NULL")?;
    let connection_id = source_column_expr(conn, "feed_events", "connection_id", "NULL")?;
    let sequence_key = source_column_expr(conn, "feed_events", "sequence_key", "NULL")?;
    let market_id = source_column_expr(conn, "feed_events", "market_id", "NULL")?;
    let asset_id = source_column_expr(conn, "feed_events", "asset_id", "NULL")?;
    let microprice = source_column_expr(conn, "feed_events", "microprice", "NULL")?;
    let fidelity = source_column_expr(conn, "feed_events", "fidelity", "'raw_event'")?;
    conn.execute(
        &format!(
            "INSERT INTO clob_replay_events (
            received_at_ms, event_at_ms, received_at_us, event_at_us, side, source, event_type,
            source_topic, connection_id, sequence_key, market_id, asset_id, best_bid, best_ask,
            bid_size, ask_size, microprice, fidelity
         )
         SELECT received_at_ms, event_at_ms, {received_at_us}, {event_at_us},
                CASE source WHEN 'clob_up' THEN 'up' ELSE 'down' END,
                source, event_type, {source_topic}, {connection_id}, {sequence_key}, {market_id},
                {asset_id}, best_bid, best_ask, bid_size, ask_size, {microprice}, {fidelity}
         FROM src.feed_events
         WHERE received_at_ms >= ?1
           AND received_at_ms <= ?2
           AND source IN ('clob_up', 'clob_down')
           AND event_type IN ('book', 'price_change', 'best_bid_ask')
           AND best_bid IS NOT NULL
           AND best_ask IS NOT NULL
           AND bid_size IS NOT NULL
           AND ask_size IS NOT NULL"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .context("copying legacy CLOB feed rows into compact replay storage")?;
    Ok(())
}

/// Copy compact CLOB rows from source databases that already use the new shape.
fn copy_compact_clob_rows(conn: &Connection, start_time: u64, end_time: u64) -> anyhow::Result<()> {
    if !source_table_exists(conn, "clob_replay_events")? {
        return Ok(());
    }
    let columns = common_columns(conn, "clob_replay_events")?;
    if columns.is_empty() {
        return Ok(());
    }
    let columns_without_id = columns
        .into_iter()
        .filter(|column| column != "id")
        .collect::<Vec<_>>();
    if columns_without_id.is_empty() {
        return Ok(());
    }
    let column_list = columns_without_id.join(", ");
    conn.execute(
        &format!(
            "INSERT INTO clob_replay_events ({column_list})
             SELECT {column_list}
             FROM src.clob_replay_events
             WHERE received_at_ms >= ?1 AND received_at_ms <= ?2"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .context("copying compact clob_replay_events")?;
    Ok(())
}

/// Copy compressed CLOB replay blocks without expanding them.
fn copy_clob_replay_blocks(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<()> {
    if !source_table_exists(conn, "clob_replay_blocks")? {
        return Ok(());
    }
    let columns = common_columns(conn, "clob_replay_blocks")?;
    if columns.is_empty() {
        return Ok(());
    }
    let columns_without_id = columns
        .into_iter()
        .filter(|column| column != "id")
        .collect::<Vec<_>>();
    if columns_without_id.is_empty() {
        return Ok(());
    }
    let column_list = columns_without_id.join(", ");
    conn.execute(
        &format!(
            "INSERT INTO clob_replay_blocks ({column_list})
             SELECT {column_list}
             FROM src.clob_replay_blocks
             WHERE max_received_at_ms >= ?1 AND min_received_at_ms <= ?2"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .context("copying CLOB replay blocks")?;
    Ok(())
}

/// Return a source column expression or a fallback literal.
fn source_column_expr(
    conn: &Connection,
    table: &str,
    column: &str,
    fallback: &str,
) -> anyhow::Result<String> {
    let columns = table_columns(conn, "src", table)?;
    if columns.iter().any(|candidate| candidate == column) {
        Ok(column.to_string())
    } else {
        Ok(fallback.to_string())
    }
}

/// Copy small live and audit tables by column intersection.
fn copy_small_live_tables(conn: &Connection) -> anyhow::Result<()> {
    for table in [
        "live_sessions",
        "live_order_intents",
        "live_orders",
        "live_fills",
        "live_account_snapshots",
        "live_redemptions",
        "live_reconciliation_events",
        "control_audit",
        "live_control_state",
        "live_control_commands",
        "feed_health_events",
        "strategy_rejection_summaries",
    ] {
        copy_table_intersection(conn, table, None)?;
    }
    Ok(())
}

/// Copy source audit rows needed for replay calibration.
fn copy_source_audit_tables(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<()> {
    copy_time_window_table(conn, "signals", "timestamp", start_time, end_time)?;
    copy_signal_metrics_for_copied_signals(conn)?;
    copy_source_trades(conn, start_time, end_time)?;
    copy_trade_results_for_copied_trades(conn, start_time, end_time)?;
    copy_balance_log(conn, start_time, end_time)?;
    Ok(())
}

/// Copy one table by a timestamp window.
fn copy_time_window_table(
    conn: &Connection,
    table: &str,
    time_column: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<()> {
    if !source_table_exists(conn, table)? || !source_column_exists(conn, table, time_column)? {
        return Ok(());
    }
    let columns = common_columns(conn, table)?;
    if columns.is_empty() {
        return Ok(());
    }
    let column_list = columns.join(", ");
    conn.execute(
        &format!(
            "INSERT INTO {table} ({column_list})
             SELECT {column_list}
             FROM src.{table}
             WHERE {time_column} >= ?1 AND {time_column} <= ?2"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .with_context(|| format!("copying source table {table}"))?;
    Ok(())
}

/// Copy source signal metrics for the source signals already retained.
fn copy_signal_metrics_for_copied_signals(conn: &Connection) -> anyhow::Result<()> {
    if !source_table_exists(conn, "signal_metrics")? {
        return Ok(());
    }
    let columns = common_columns(conn, "signal_metrics")?;
    if columns.is_empty() {
        return Ok(());
    }
    let column_list = columns.join(", ");
    conn.execute(
        &format!(
            "INSERT INTO signal_metrics ({column_list})
             SELECT {column_list}
             FROM src.signal_metrics
             WHERE signal_id IN (SELECT id FROM signals)"
        ),
        [],
    )
    .context("copying source signal_metrics")?;
    Ok(())
}

/// Copy source trades that opened or settled in the selected interval.
fn copy_source_trades(conn: &Connection, start_time: u64, end_time: u64) -> anyhow::Result<()> {
    if !source_table_exists(conn, "simulated_trades")? {
        return Ok(());
    }
    let columns = common_columns(conn, "simulated_trades")?;
    if columns.is_empty() {
        return Ok(());
    }
    let column_list = columns.join(", ");
    let settled_clause = if source_table_exists(conn, "trade_results")?
        && source_column_exists(conn, "trade_results", "resolved_at")?
    {
        " OR id IN (
                   SELECT trade_id
                   FROM src.trade_results
                   WHERE resolved_at >= ?1 AND resolved_at <= ?2
               )"
    } else {
        ""
    };
    conn.execute(
        &format!(
            "INSERT INTO simulated_trades ({column_list})
             SELECT {column_list}
             FROM src.simulated_trades
             WHERE (timestamp >= ?1 AND timestamp <= ?2){settled_clause}"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .context("copying source simulated_trades")?;
    Ok(())
}

/// Copy source trade results for retained trades and in-window settlements.
fn copy_trade_results_for_copied_trades(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<()> {
    if !source_table_exists(conn, "trade_results")? {
        return Ok(());
    }
    let columns = common_columns(conn, "trade_results")?;
    if columns.is_empty() {
        return Ok(());
    }
    let column_list = columns.join(", ");
    let resolved_clause = if source_column_exists(conn, "trade_results", "resolved_at")? {
        " OR (resolved_at >= ?1 AND resolved_at <= ?2)"
    } else {
        ""
    };
    conn.execute(
        &format!(
            "INSERT OR IGNORE INTO trade_results ({column_list})
             SELECT {column_list}
             FROM src.trade_results
             WHERE trade_id IN (SELECT id FROM simulated_trades){resolved_clause}"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .context("copying source trade_results")?;
    Ok(())
}

/// Copy source balance events needed to audit source-run equity.
fn copy_balance_log(conn: &Connection, start_time: u64, end_time: u64) -> anyhow::Result<()> {
    if !source_table_exists(conn, "balance_log")?
        || !source_column_exists(conn, "balance_log", "timestamp")?
    {
        return Ok(());
    }
    let columns = common_columns(conn, "balance_log")?;
    if columns.is_empty() {
        return Ok(());
    }
    let column_list = columns.join(", ");
    conn.execute(
        &format!(
            "INSERT INTO balance_log ({column_list})
             SELECT {column_list}
             FROM src.balance_log
             WHERE timestamp >= ?1 AND timestamp <= ?2
                OR trade_id IN (SELECT id FROM simulated_trades)
                OR event = 'init'"
        ),
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
    )
    .context("copying source balance_log")?;
    Ok(())
}

/// Copy one source table into the destination using shared column names.
fn copy_table_intersection(
    conn: &Connection,
    table: &str,
    where_clause: Option<&str>,
) -> anyhow::Result<()> {
    if !source_table_exists(conn, table)? {
        return Ok(());
    }
    let columns = common_columns(conn, table)?;
    if columns.is_empty() {
        return Ok(());
    }
    let column_list = columns.join(", ");
    let filter = where_clause.map_or(String::new(), |clause| format!(" WHERE {clause}"));
    conn.execute(
        &format!(
            "INSERT INTO {table} ({column_list})
             SELECT {column_list}
             FROM src.{table}{filter}"
        ),
        [],
    )
    .with_context(|| format!("copying table {table}"))?;
    Ok(())
}

/// Return destination/source columns shared by one table.
fn common_columns(conn: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let destination = table_columns(conn, "main", table)?;
    let source = table_columns(conn, "src", table)?;
    Ok(destination
        .into_iter()
        .filter(|column| source.contains(column))
        .collect())
}

/// Return column names for one schema-qualified table.
fn table_columns(conn: &Connection, schema: &str, table: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA {schema}.table_info({table})"))
        .with_context(|| format!("reading columns for {schema}.{table}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Return whether the attached source database has one table.
fn source_table_exists(conn: &Connection, table: &str) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM src.sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Return whether the attached source database has one column.
fn source_column_exists(conn: &Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let columns = table_columns(conn, "src", table)?;
    Ok(columns.iter().any(|candidate| candidate == column))
}

/// Return row counts from a prepared output DB.
fn prepared_counts(output_path: &str) -> anyhow::Result<PreparedCounts> {
    let conn = Connection::open_with_flags(output_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let generic_feed_rows =
        conn.query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))?;
    let compact_clob_event_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM clob_replay_events", [], |row| {
            row.get(0)
        })?;
    let compact_clob_block_rows: i64 = conn.query_row(
        "SELECT COALESCE(SUM(row_count), 0) FROM clob_replay_blocks",
        [],
        |row| row.get(0),
    )?;
    let source_signal_rows = table_count(&conn, "signals")?;
    let source_trade_rows = table_count(&conn, "simulated_trades")?;
    let source_trade_result_rows = table_count(&conn, "trade_results")?;
    Ok(PreparedCounts {
        generic_feed: generic_feed_rows,
        compact_clob: compact_clob_event_rows + compact_clob_block_rows,
        source_signals: source_signal_rows,
        source_trades: source_trade_rows,
        source_trade_results: source_trade_result_rows,
    })
}

/// Count rows in one destination table.
fn table_count(conn: &Connection, table: &str) -> anyhow::Result<i64> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .with_context(|| format!("counting prepared table {table}"))
}

/// Write one JSON manifest beside the prepared database.
fn write_manifest(
    manifest_path: &Path,
    options: &PrepareBacktestInputOptions,
    source_bytes: u64,
    output_bytes: u64,
    counts: PreparedCounts,
    readiness: &BacktestInputReport,
) -> anyhow::Result<()> {
    let payload = json!({
        "prepared_at_ms": now_ms(),
        "source": options.data_path,
        "output": options.output_path,
        "start_time": options.start_time,
        "end_time": options.end_time,
        "source_bytes": source_bytes,
        "output_bytes": output_bytes,
        "generic_feed_rows": counts.generic_feed,
        "compact_clob_rows": counts.compact_clob,
        "source_signal_rows": counts.source_signals,
        "source_trade_rows": counts.source_trades,
        "source_trade_result_rows": counts.source_trade_results,
        "backtest_input": readiness.class.as_str(),
        "replay_quality": readiness.replay_quality.class.as_str(),
        "settled_windows": readiness.settled_windows,
        "dry_run_ticks": readiness.dry_run_ticks,
    });
    std::fs::write(manifest_path, serde_json::to_string_pretty(&payload)?)
        .with_context(|| format!("writing manifest: {}", manifest_path.display()))
}

/// Return a manifest path for one prepared database path.
fn manifest_path(output_path: &str) -> PathBuf {
    PathBuf::from(format!("{output_path}.manifest.json"))
}

/// Return the size of one filesystem path.
fn file_size(path: &str) -> anyhow::Result<u64> {
    Ok(std::fs::metadata(path)
        .with_context(|| format!("reading metadata for {path}"))?
        .len())
}

/// Convert one timestamp into a `SQLite` integer.
fn sqlite_timestamp(timestamp: u64, label: &str) -> anyhow::Result<i64> {
    i64::try_from(timestamp).with_context(|| format!("{label} does not fit in i64"))
}

/// Return the current wall-clock time in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
#[path = "tests/backtest_prepare_tests.rs"]
mod tests;
