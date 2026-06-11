use anyhow::Context;
use rusqlite::{Connection, OpenFlags};

/// Source-run aggregate metrics used to calibrate replay output.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRunMetrics {
    pub net_pnl: Option<f64>,
    pub total_fees: Option<f64>,
    pub gross_pnl: Option<f64>,
    pub final_balance: Option<f64>,
    pub trade_count: Option<u64>,
    pub signal_count: Option<u64>,
}

impl SourceRunMetrics {
    /// Return whether this metric set contains any source-run evidence.
    #[must_use]
    pub fn has_evidence(&self) -> bool {
        self.net_pnl.is_some() || self.trade_count.is_some() || self.signal_count.is_some()
    }
}

/// Read source-run metrics from one `SQLite` database path.
pub fn read_source_run_metrics(
    db_path: &str,
    start_time: u64,
    end_time: u64,
    starting_balance: f64,
) -> anyhow::Result<Option<SourceRunMetrics>> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening source-run DB: {db_path}"))?;
    read_source_run_metrics_from_connection(&conn, start_time, end_time, starting_balance)
}

/// Read source-run metrics from one opened `SQLite` connection.
pub fn read_source_run_metrics_from_connection(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
    starting_balance: f64,
) -> anyhow::Result<Option<SourceRunMetrics>> {
    let (trade_count, net_pnl, total_fees) = read_source_trade_metrics(conn, start_time, end_time)?;
    let signal_count = read_time_window_count(conn, "signals", "timestamp", start_time, end_time)?;
    let gross_pnl = metric_sum(net_pnl, total_fees);
    let final_balance = net_pnl.map(|value| starting_balance + value);
    let metrics = SourceRunMetrics {
        net_pnl,
        total_fees,
        gross_pnl,
        final_balance,
        trade_count,
        signal_count,
    };
    Ok(metrics.has_evidence().then_some(metrics))
}

/// Read source trade count and `PnL` metrics.
fn read_source_trade_metrics(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<(Option<u64>, Option<f64>, Option<f64>)> {
    if !table_exists(conn, "trade_results")? || !column_exists(conn, "trade_results", "pnl_net")? {
        return Ok((None, None, None));
    }
    let fee_expr = if column_exists(conn, "trade_results", "fee_amount")? {
        "COALESCE(SUM(fee_amount), 0.0)"
    } else {
        "0.0"
    };
    let predicate = source_time_predicate(conn, "trade_results", "resolved_at")?;
    let query = format!(
        "SELECT COUNT(*), COALESCE(SUM(pnl_net), 0.0), {fee_expr} FROM trade_results{predicate}",
    );
    let row = if predicate.is_empty() {
        conn.query_row(&query, [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?
    } else {
        conn.query_row(
            &query,
            [sqlite_timestamp(start_time)?, sqlite_timestamp(end_time)?],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )?
    };
    Ok((
        Some(u64::try_from(row.0).unwrap_or(0)),
        Some(row.1),
        Some(row.2),
    ))
}

/// Count rows in one table when it exists.
fn read_time_window_count(
    conn: &Connection,
    table: &str,
    column: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<Option<u64>> {
    if !table_exists(conn, table)? {
        return Ok(None);
    }
    let predicate = source_time_predicate(conn, table, column)?;
    let query = format!("SELECT COUNT(*) FROM {}{predicate}", sqlite_ident(table));
    let count: i64 = if predicate.is_empty() {
        conn.query_row(&query, [], |row| row.get(0))?
    } else {
        conn.query_row(
            &query,
            [sqlite_timestamp(start_time)?, sqlite_timestamp(end_time)?],
            |row| row.get(0),
        )?
    };
    Ok(Some(u64::try_from(count).unwrap_or(0)))
}

/// Return a time predicate for tables that have the requested timestamp column.
fn source_time_predicate(conn: &Connection, table: &str, column: &str) -> anyhow::Result<String> {
    if column_exists(conn, table, column)? {
        Ok(format!(" WHERE {} BETWEEN ?1 AND ?2", sqlite_ident(column)))
    } else {
        Ok(String::new())
    }
}

/// Return whether one table exists.
fn table_exists(conn: &Connection, table: &str) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Return whether one table has one column.
fn column_exists(conn: &Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", sqlite_ident(table)))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Quote one `SQLite` identifier from a trusted internal string.
fn sqlite_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Convert one timestamp into `SQLite` integer form.
fn sqlite_timestamp(timestamp: u64) -> anyhow::Result<i64> {
    i64::try_from(timestamp).context("timestamp does not fit in i64")
}

/// Return an optional numeric sum.
fn metric_sum(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    Some(left? + right?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one source DB with source metrics around an interval boundary.
    fn source_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO markets
             (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status)
             VALUES ('m1', 'm1?', 'cond-1', 'slug-1', 'up-1', 'down-1', 500, 2000, 'resolved'),
                    ('m2', 'm2?', 'cond-2', 'slug-2', 'up-2', 'down-2', 2500, 4000, 'resolved')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO signals (id, timestamp, strategy, direction)
             VALUES (1, 1000, 'latency_arb', 'UP'), (2, 3000, 'latency_arb', 'DOWN')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO simulated_trades
             (id, timestamp, market_id, strategy, side, token_id, entry_price, size, status)
             VALUES
             (10, 1000, 'm1', 'latency_arb', 'UP', 'up-1', 0.4, 10.0, 'closed'),
             (11, 3000, 'm2', 'latency_arb', 'DOWN', 'down-1', 0.5, 10.0, 'closed')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO trade_results
             (trade_id, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, pnl_net, fee_amount, resolved_at)
             VALUES (10, 1.0, 8.0, 8.0, 8.0, 8.0, 7.5, 0.5, 1500),
                    (11, 0.0, -5.0, -5.0, -5.0, -5.0, -5.2, 0.2, 3500)",
            [],
        )
        .unwrap();
        conn
    }

    /// Verifies source metrics use the requested interval.
    #[test]
    fn metrics_filter_source_rows_by_interval() {
        let conn = source_db();
        let metrics = read_source_run_metrics_from_connection(&conn, 900, 2000, 100.0)
            .unwrap()
            .unwrap();

        assert_eq!(metrics.trade_count, Some(1));
        assert_eq!(metrics.signal_count, Some(1));
        assert_eq!(metrics.net_pnl, Some(7.5));
        assert_eq!(metrics.total_fees, Some(0.5));
        assert_eq!(metrics.gross_pnl, Some(8.0));
        assert_eq!(metrics.final_balance, Some(107.5));
    }

    /// Verifies empty source evidence returns no metrics.
    #[test]
    fn missing_source_tables_return_none() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();
        conn.execute("DROP TABLE signals", []).unwrap();
        conn.execute("DROP TABLE trade_results", []).unwrap();

        let metrics = read_source_run_metrics_from_connection(&conn, 900, 2000, 100.0).unwrap();

        assert!(metrics.is_none());
    }
}
