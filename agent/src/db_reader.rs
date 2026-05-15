use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::Mutex;

use crate::error::AgentError;
use crate::types::{
    BalanceEntry, BalanceResponse, BotStatus, EquitySeriesResponse, LiveAccountSnapshotRow,
    LiveControlAuditResponse, LiveControlAuditRow, LiveControlCommandResponse, LiveFillRow,
    LiveFillsResponse, LiveOrderRow, LiveOrdersResponse, LiveReconciliationResponse,
    LiveReconciliationRow, LiveRedemptionRow, LiveRedemptionsResponse, LiveSessionRow,
    LiveSessionsResponse, LiveStatusResponse, RunMetadataRow, SignalGroupRow, SignalGroupsResponse,
    SignalRow, SignalsResponse, StatsResponse, StrategyStats, TradeRow, TradesResponse, WindowInfo,
};

struct SignalGroupAccumulator {
    strategy: String,
    direction: String,
    market_id: Option<String>,
    start_timestamp: u64,
    end_timestamp: u64,
    count: u64,
    first_signal_id: i64,
    last_signal_id: i64,
    binance_price: Option<f64>,
    chainlink_price: Option<f64>,
    up_ask: Option<f64>,
    down_ask: Option<f64>,
    execution_fidelity: Option<String>,
}

#[derive(Debug, Clone)]
struct StatusCache {
    valid_until_ms: u64,
    status: BotStatus,
}

/// Returns whether the given table currently exposes the requested column.
fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let pragma = format!("PRAGMA table_info({table})");
    let Ok(mut stmt) = conn.prepare(&pragma) else {
        return false;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    rows.flatten().any(|name| name == column)
}

/// Returns whether the given table currently exists.
fn has_table(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM sqlite_master
            WHERE type = 'table' AND name = ?1
        )",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .is_ok_and(|exists| exists != 0)
}

/// Return wall-clock time in milliseconds for response-cache TTLs.
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Converts an accumulated signal burst into its serialized response row.
fn finish_signal_group(group: SignalGroupAccumulator) -> SignalGroupRow {
    let market_part = group
        .market_id
        .as_deref()
        .unwrap_or("none")
        .replace(':', "_");
    let id = format!(
        "{}:{}:{}:{}:{}",
        market_part, group.strategy, group.direction, group.start_timestamp, group.end_timestamp
    );
    SignalGroupRow {
        id,
        strategy: group.strategy,
        direction: group.direction,
        market_id: group.market_id,
        start_timestamp: group.start_timestamp,
        end_timestamp: group.end_timestamp,
        count: group.count,
        first_signal_id: group.first_signal_id,
        last_signal_id: group.last_signal_id,
        binance_price: group.binance_price,
        chainlink_price: group.chainlink_price,
        up_ask: group.up_ask,
        down_ask: group.down_ask,
        execution_fidelity: group.execution_fidelity,
    }
}

/// Return the best-effort execution mode and latest live-session status for this DB.
fn current_execution_mode_and_session_status(conn: &Connection) -> (String, Option<String>) {
    let latest_live_session = if has_table(conn, "live_sessions") {
        conn.query_row(
            "SELECT execution_mode, status, started_at_ms, id
             FROM live_sessions
             ORDER BY started_at_ms DESC, id DESC
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .ok()
    } else {
        None
    };

    let latest_trade_mode = if has_table(conn, "simulated_trades")
        && has_column(conn, "simulated_trades", "execution_mode")
    {
        conn.query_row(
            "SELECT execution_mode, timestamp, id
             FROM simulated_trades
             WHERE execution_mode IS NOT NULL
             ORDER BY timestamp DESC, id DESC
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .ok()
    } else {
        None
    };

    let execution_mode = match (&latest_live_session, &latest_trade_mode) {
        (
            Some((live_mode, _, live_started_at, live_id)),
            Some((trade_mode, trade_timestamp, trade_id)),
        ) => {
            if (*live_started_at, *live_id) >= (*trade_timestamp, *trade_id) {
                live_mode.clone()
            } else {
                trade_mode.clone()
            }
        }
        (Some((live_mode, _, _, _)), None) => live_mode.clone(),
        (None, Some((trade_mode, _, _))) => trade_mode.clone(),
        (None, None) => "paper".to_string(),
    };
    let live_session_status = latest_live_session.map(|(_, status, _, _)| status);
    (execution_mode, live_session_status)
}

/// Read-only database reader for the bot's `SQLite` database.
pub struct DbReader {
    db_path: Option<String>,
    conn: Arc<Mutex<Connection>>,
    status_cache: Arc<Mutex<Option<StatusCache>>>,
}

impl DbReader {
    /// Open the bot's database in read-only mode with WAL.
    pub fn new(db_path: &str) -> Result<Self, AgentError> {
        let conn = Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| AgentError::Internal(format!("failed to open database: {e}")))?;

        conn.execute_batch("PRAGMA query_only = ON;")
            .map_err(|e| AgentError::Internal(format!("failed to set query_only: {e}")))?;

        Ok(Self {
            db_path: Some(db_path.to_string()),
            conn: Arc::new(Mutex::new(conn)),
            status_cache: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a `DbReader` from an existing connection (for testing).
    #[cfg(test)]
    pub fn from_connection(conn: Connection) -> Self {
        Self {
            db_path: None,
            conn: Arc::new(Mutex::new(conn)),
            status_cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the bot's current status.
    pub async fn get_status(&self) -> Result<BotStatus, AgentError> {
        let now_ms = current_time_ms();
        if let Some(cache) = self.status_cache.lock().await.as_ref()
            && cache.valid_until_ms > now_ms
        {
            return Ok(cache.status.clone());
        }
        let conn = self.conn.lock().await;
        let pnl_expr = if has_column(&conn, "trade_results", "pnl_net") {
            "COALESCE(pnl_net, pnl_0pct)"
        } else {
            "pnl_0pct"
        };

        let (balance, starting_balance) = conn
            .query_row(
                "SELECT balance, \
                 (SELECT balance FROM balance_log WHERE event = 'init' ORDER BY id LIMIT 1) \
                 FROM balance_log ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
            )
            .unwrap_or((0.0, 0.0));
        let (execution_mode, live_session_status) =
            current_execution_mode_and_session_status(&conn);

        let total_trades: u64 = conn
            .query_row("SELECT COUNT(*) FROM trade_results", [], |row| row.get(0))
            .unwrap_or(0);

        let wins: u64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM trade_results WHERE {pnl_expr} > 0"),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let losses = total_trades.saturating_sub(wins);

        let win_rate = if total_trades > 0 {
            wins as f64 / total_trades as f64
        } else {
            0.0
        };

        let total_pnl: f64 = conn
            .query_row(
                &format!("SELECT COALESCE(SUM({pnl_expr}), 0.0) FROM trade_results"),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let high_water_mark: f64 = conn
            .query_row(
                "SELECT COALESCE(MAX(balance), 0.0) FROM balance_log",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let max_drawdown_pct = if high_water_mark > 0.0 {
            (high_water_mark - balance) / high_water_mark
        } else {
            0.0
        };

        let (first_tick, last_tick): (Option<u64>, Option<u64>) = conn
            .query_row(
                "SELECT MIN(timestamp), MAX(timestamp) FROM tick_data",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((None, None));

        let uptime_hours = match (first_tick, last_tick) {
            (Some(first), Some(last)) if last > first => (last - first) as f64 / 3_600_000.0,
            _ => 0.0,
        };

        let open_trades: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM simulated_trades WHERE status = 'open'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let current_window_ref_ms = last_tick.unwrap_or(now_ms);
        let current_window = conn
            .query_row(
                "SELECT market_id, question, end_time FROM markets \
                 WHERE status = 'active' AND start_time <= ?1 AND end_time > ?1 \
                 ORDER BY start_time DESC LIMIT 1",
                [current_window_ref_ms],
                |row| {
                    Ok(WindowInfo {
                        market_id: row.get(0)?,
                        question: row.get(1)?,
                        end_time: row.get(2)?,
                    })
                },
            )
            .ok();

        let max_dd = compute_max_drawdown(&conn);

        let status = BotStatus {
            balance,
            starting_balance,
            execution_mode,
            live_session_status,
            total_trades,
            wins,
            losses,
            win_rate,
            total_pnl,
            max_drawdown_pct: if max_dd > max_drawdown_pct {
                max_dd
            } else {
                max_drawdown_pct
            },
            high_water_mark,
            uptime_hours,
            open_trades,
            last_tick_at: last_tick.or(Some(now_ms)),
            current_window,
        };
        drop(conn);
        *self.status_cache.lock().await = Some(StatusCache {
            valid_until_ms: now_ms.saturating_add(2_000),
            status: status.clone(),
        });
        Ok(status)
    }

    /// Get paginated trade history.
    pub async fn get_trades(&self, page: u64, per_page: u64) -> Result<TradesResponse, AgentError> {
        let conn = self.conn.lock().await;
        let pnl_expr = if has_column(&conn, "trade_results", "pnl_net") {
            "COALESCE(r.pnl_net, r.pnl_0pct)".to_string()
        } else {
            "r.pnl_0pct".to_string()
        };
        let fill_status_expr = if has_column(&conn, "simulated_trades", "fill_status") {
            "t.fill_status".to_string()
        } else {
            "NULL AS fill_status".to_string()
        };
        let execution_group_id_expr = if has_column(&conn, "simulated_trades", "execution_group_id")
        {
            "t.execution_group_id".to_string()
        } else {
            "NULL AS execution_group_id".to_string()
        };
        let execution_fidelity_expr = if has_column(&conn, "simulated_trades", "execution_fidelity")
        {
            "t.execution_fidelity".to_string()
        } else {
            "NULL AS execution_fidelity".to_string()
        };
        let filled_size_expr = if has_column(&conn, "simulated_trades", "filled_size") {
            "t.filled_size".to_string()
        } else {
            "NULL AS filled_size".to_string()
        };
        let avg_fill_price_expr = if has_column(&conn, "simulated_trades", "avg_fill_price") {
            "t.avg_fill_price".to_string()
        } else {
            "NULL AS avg_fill_price".to_string()
        };

        let total: u64 = conn.query_row("SELECT COUNT(*) FROM simulated_trades", [], |row| {
            row.get(0)
        })?;

        let offset = (page.saturating_sub(1)) * per_page;

        let sql = format!(
            "SELECT t.id, t.timestamp, t.market_id, t.strategy, t.side, t.entry_price, \
             t.size, t.status, {pnl_expr}, r.settlement_price, r.resolved_at, \
             {fill_status_expr}, {execution_group_id_expr}, {execution_fidelity_expr}, \
             {filled_size_expr}, {avg_fill_price_expr} \
             FROM simulated_trades t \
             LEFT JOIN trade_results r ON r.trade_id = t.id \
             ORDER BY t.timestamp DESC \
             LIMIT ?1 OFFSET ?2"
        );
        let mut stmt = conn.prepare_cached(&sql)?;

        let trades = stmt
            .query_map(params![per_page, offset], |row| {
                Ok(TradeRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    market_id: row.get(2)?,
                    strategy: row.get(3)?,
                    side: row.get(4)?,
                    entry_price: row.get(5)?,
                    size: row.get(6)?,
                    status: row.get(7)?,
                    pnl: row.get(8)?,
                    settlement_price: row.get(9)?,
                    resolved_at: row.get(10)?,
                    fill_status: row.get(11)?,
                    execution_group_id: row.get(12)?,
                    execution_fidelity: row.get(13)?,
                    filled_size: row.get(14)?,
                    avg_fill_price: row.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TradesResponse {
            trades,
            total,
            page,
            per_page,
        })
    }

    /// Get balance log entries since a given timestamp.
    pub async fn get_balance_log(&self, since: u64) -> Result<BalanceResponse, AgentError> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare_cached(
            "SELECT id, timestamp, event, trade_id, amount, balance \
             FROM balance_log WHERE timestamp >= ?1 ORDER BY timestamp",
        )?;

        let entries = stmt
            .query_map(params![since], |row| {
                Ok(BalanceEntry {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    event: row.get(2)?,
                    trade_id: row.get(3)?,
                    amount: row.get(4)?,
                    balance: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(BalanceResponse { entries })
    }

    /// Get chart-safe equity data with timestamp-zero baseline rows separated.
    pub async fn get_equity_series(&self, since: u64) -> Result<EquitySeriesResponse, AgentError> {
        let entries = self.get_balance_log(since).await?.entries;
        let mut baseline = None;
        let mut points = Vec::new();

        for entry in entries {
            if entry.timestamp == 0 {
                if baseline.is_none() {
                    baseline = Some(entry);
                }
            } else {
                points.push(entry);
            }
        }

        Ok(EquitySeriesResponse { baseline, points })
    }

    /// Get recent signals.
    pub async fn get_signals(&self, limit: u64) -> Result<SignalsResponse, AgentError> {
        let conn = self.conn.lock().await;
        let market_id_expr = if has_column(&conn, "signals", "market_id") {
            "market_id".to_string()
        } else {
            "NULL AS market_id".to_string()
        };
        let execution_fidelity_expr = if has_column(&conn, "signals", "execution_fidelity") {
            "execution_fidelity".to_string()
        } else {
            "NULL AS execution_fidelity".to_string()
        };

        let sql = format!(
            "SELECT id, timestamp, strategy, direction, binance_price, chainlink_price, \
             up_ask, down_ask, metadata, {market_id_expr}, {execution_fidelity_expr} \
             FROM signals ORDER BY timestamp DESC LIMIT ?1"
        );
        let mut stmt = conn.prepare_cached(&sql)?;

        let signals = stmt
            .query_map(params![limit], |row| {
                Ok(SignalRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    strategy: row.get(2)?,
                    direction: row.get(3)?,
                    binance_price: row.get(4)?,
                    chainlink_price: row.get(5)?,
                    up_ask: row.get(6)?,
                    down_ask: row.get(7)?,
                    metadata: row.get(8)?,
                    market_id: row.get(9)?,
                    execution_fidelity: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SignalsResponse { signals })
    }

    /// Get recent signal bursts grouped for operator-level dashboard review.
    pub async fn get_signal_groups(
        &self,
        limit: u64,
        quiet_gap_ms: u64,
        raw_limit: u64,
    ) -> Result<SignalGroupsResponse, AgentError> {
        if limit == 0 || raw_limit == 0 {
            return Ok(SignalGroupsResponse {
                groups: Vec::new(),
                raw_rows_scanned: 0,
                quiet_gap_ms,
            });
        }

        let signals = self.get_signals(raw_limit).await?.signals;
        let raw_rows_scanned = signals.len() as u64;
        let mut groups = Vec::new();
        let mut current: Option<SignalGroupAccumulator> = None;

        for signal in signals {
            if groups.len() as u64 >= limit {
                break;
            }

            let should_extend = current.as_ref().is_some_and(|group| {
                group.strategy == signal.strategy
                    && group.direction == signal.direction
                    && group.market_id == signal.market_id
                    && group.start_timestamp.saturating_sub(signal.timestamp) <= quiet_gap_ms
            });

            if should_extend {
                if let Some(group) = current.as_mut() {
                    group.start_timestamp = signal.timestamp;
                    group.count += 1;
                    group.last_signal_id = signal.id;
                }
                continue;
            }

            if let Some(group) = current.take() {
                groups.push(finish_signal_group(group));
                if groups.len() as u64 >= limit {
                    break;
                }
            }

            current = Some(SignalGroupAccumulator {
                strategy: signal.strategy,
                direction: signal.direction,
                market_id: signal.market_id,
                start_timestamp: signal.timestamp,
                end_timestamp: signal.timestamp,
                count: 1,
                first_signal_id: signal.id,
                last_signal_id: signal.id,
                binance_price: signal.binance_price,
                chainlink_price: signal.chainlink_price,
                up_ask: signal.up_ask,
                down_ask: signal.down_ask,
                execution_fidelity: signal.execution_fidelity,
            });
        }

        if (groups.len() as u64) < limit
            && let Some(group) = current.take()
        {
            groups.push(finish_signal_group(group));
        }

        Ok(SignalGroupsResponse {
            groups,
            raw_rows_scanned,
            quiet_gap_ms,
        })
    }

    /// Get aggregated stats per strategy.
    pub async fn get_stats(&self) -> Result<StatsResponse, AgentError> {
        let conn = self.conn.lock().await;
        let pnl_expr = if has_column(&conn, "trade_results", "pnl_net") {
            "COALESCE(r.pnl_net, r.pnl_0pct)"
        } else {
            "r.pnl_0pct"
        };

        let sql = format!(
            "SELECT t.strategy, COUNT(*) as trades, \
             SUM(CASE WHEN {pnl_expr} > 0 THEN 1 ELSE 0 END) as wins, \
             SUM(CASE WHEN {pnl_expr} <= 0 THEN 1 ELSE 0 END) as losses, \
             COALESCE(SUM({pnl_expr}), 0.0) as total_pnl \
             FROM trade_results r \
             JOIN simulated_trades t ON r.trade_id = t.id \
             GROUP BY t.strategy"
        );
        let mut stmt = conn.prepare_cached(&sql)?;

        let mut by_strategy = HashMap::new();

        let rows = stmt.query_map([], |row| {
            let strategy: String = row.get(0)?;
            let trades: u64 = row.get(1)?;
            let wins: u64 = row.get(2)?;
            let losses: u64 = row.get(3)?;
            let total_pnl: f64 = row.get(4)?;
            Ok((strategy, trades, wins, losses, total_pnl))
        })?;

        for row in rows {
            let (strategy, trades, wins, losses, total_pnl) = row?;
            let win_rate = if trades > 0 {
                wins as f64 / trades as f64
            } else {
                0.0
            };
            by_strategy.insert(
                strategy,
                StrategyStats {
                    trades,
                    wins,
                    losses,
                    total_pnl,
                    win_rate,
                },
            );
        }

        Ok(StatsResponse { by_strategy })
    }

    /// Get a compact live-status summary from the additive live tables.
    pub async fn get_live_status(&self) -> Result<LiveStatusResponse, AgentError> {
        let conn = self.conn.lock().await;
        if !has_table(&conn, "live_sessions") {
            return Ok(LiveStatusResponse {
                latest_session: None,
                latest_account_snapshot: None,
                open_orders: 0,
                pending_redemptions: 0,
                critical_reconciliation_events: 0,
            });
        }

        let latest_session = conn
            .query_row(
                "SELECT id, started_at_ms, ended_at_ms, status, execution_mode, wallet_address,
                        proxy_wallet, enabled_strategies_json, config_fingerprint, cash_cap_usd,
                        details_json
                 FROM live_sessions
                 ORDER BY started_at_ms DESC, id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok(LiveSessionRow {
                        id: row.get(0)?,
                        started_at_ms: row.get(1)?,
                        ended_at_ms: row.get(2)?,
                        status: row.get(3)?,
                        execution_mode: row.get(4)?,
                        wallet_address: row.get(5)?,
                        proxy_wallet: row.get(6)?,
                        enabled_strategies_json: row.get(7)?,
                        config_fingerprint: row.get(8)?,
                        cash_cap_usd: row.get(9)?,
                        details_json: row.get(10)?,
                    })
                },
            )
            .optional()?;

        let latest_account_snapshot = if has_table(&conn, "live_account_snapshots") {
            conn.query_row(
                "SELECT id, session_id, timestamp_ms, cash_available, cash_reserved_for_orders,
                        inventory_mark_value, redeemable_value, pending_redeem_value,
                        total_equity, allowance_available, details_json
                 FROM live_account_snapshots
                 ORDER BY timestamp_ms DESC, id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok(LiveAccountSnapshotRow {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        timestamp_ms: row.get(2)?,
                        cash_available: row.get(3)?,
                        cash_reserved_for_orders: row.get(4)?,
                        inventory_mark_value: row.get(5)?,
                        redeemable_value: row.get(6)?,
                        pending_redeem_value: row.get(7)?,
                        total_equity: row.get(8)?,
                        allowance_available: row.get(9)?,
                        details_json: row.get(10)?,
                    })
                },
            )
            .optional()?
        } else {
            None
        };

        let open_orders = if has_table(&conn, "live_orders") {
            conn.query_row(
                "SELECT COUNT(*) FROM live_orders WHERE status IN ('open', 'submitted', 'partially_filled')",
                [],
                |row| row.get(0),
            )?
        } else {
            0
        };
        let pending_redemptions = if has_table(&conn, "live_redemptions") {
            conn.query_row(
                "SELECT COUNT(*) FROM live_redemptions WHERE status NOT IN ('credited', 'failed')",
                [],
                |row| row.get(0),
            )?
        } else {
            0
        };
        let critical_reconciliation_events = if has_table(&conn, "live_reconciliation_events") {
            conn.query_row(
                "SELECT COUNT(*) FROM live_reconciliation_events WHERE severity = 'critical'",
                [],
                |row| row.get(0),
            )?
        } else {
            0
        };

        Ok(LiveStatusResponse {
            latest_session,
            latest_account_snapshot,
            open_orders,
            pending_redemptions,
            critical_reconciliation_events,
        })
    }

    /// Get the most recent live sessions.
    pub async fn get_live_sessions(&self, limit: u64) -> Result<LiveSessionsResponse, AgentError> {
        let conn = self.conn.lock().await;
        if !has_table(&conn, "live_sessions") {
            return Ok(LiveSessionsResponse { sessions: vec![] });
        }

        let mut stmt = conn.prepare_cached(
            "SELECT id, started_at_ms, ended_at_ms, status, execution_mode, wallet_address,
                    proxy_wallet, enabled_strategies_json, config_fingerprint, cash_cap_usd,
                    details_json
             FROM live_sessions
             ORDER BY started_at_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let sessions = stmt
            .query_map(params![limit], |row| {
                Ok(LiveSessionRow {
                    id: row.get(0)?,
                    started_at_ms: row.get(1)?,
                    ended_at_ms: row.get(2)?,
                    status: row.get(3)?,
                    execution_mode: row.get(4)?,
                    wallet_address: row.get(5)?,
                    proxy_wallet: row.get(6)?,
                    enabled_strategies_json: row.get(7)?,
                    config_fingerprint: row.get(8)?,
                    cash_cap_usd: row.get(9)?,
                    details_json: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LiveSessionsResponse { sessions })
    }

    /// Get the most recent live orders.
    pub async fn get_live_orders(&self, limit: u64) -> Result<LiveOrdersResponse, AgentError> {
        let conn = self.conn.lock().await;
        if !has_table(&conn, "live_orders") {
            return Ok(LiveOrdersResponse { orders: vec![] });
        }

        let mut stmt = conn.prepare_cached(
            "SELECT id, session_id, intent_id, venue_order_id, client_order_id, market_id,
                    token_id, side, order_type, status, status_reason, created_at_ms,
                    acknowledged_at_ms, updated_at_ms, requested_price, limit_price,
                    requested_size, accepted_size, details_json
             FROM live_orders
             ORDER BY updated_at_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let orders = stmt
            .query_map(params![limit], |row| {
                Ok(LiveOrderRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    intent_id: row.get(2)?,
                    venue_order_id: row.get(3)?,
                    client_order_id: row.get(4)?,
                    market_id: row.get(5)?,
                    token_id: row.get(6)?,
                    side: row.get(7)?,
                    order_type: row.get(8)?,
                    status: row.get(9)?,
                    status_reason: row.get(10)?,
                    created_at_ms: row.get(11)?,
                    acknowledged_at_ms: row.get(12)?,
                    updated_at_ms: row.get(13)?,
                    requested_price: row.get(14)?,
                    limit_price: row.get(15)?,
                    requested_size: row.get(16)?,
                    accepted_size: row.get(17)?,
                    details_json: row.get(18)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LiveOrdersResponse { orders })
    }

    /// Get the most recent live fills.
    pub async fn get_live_fills(&self, limit: u64) -> Result<LiveFillsResponse, AgentError> {
        let conn = self.conn.lock().await;
        if !has_table(&conn, "live_fills") {
            return Ok(LiveFillsResponse { fills: vec![] });
        }

        let mut stmt = conn.prepare_cached(
            "SELECT id, session_id, intent_id, live_order_id, venue_trade_id, filled_at_ms,
                    price, size, fee_amount, fee_rate, liquidity_side, tx_hash, status,
                    details_json
             FROM live_fills
             ORDER BY filled_at_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let fills = stmt
            .query_map(params![limit], |row| {
                Ok(LiveFillRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    intent_id: row.get(2)?,
                    live_order_id: row.get(3)?,
                    venue_trade_id: row.get(4)?,
                    filled_at_ms: row.get(5)?,
                    price: row.get(6)?,
                    size: row.get(7)?,
                    fee_amount: row.get(8)?,
                    fee_rate: row.get(9)?,
                    liquidity_side: row.get(10)?,
                    tx_hash: row.get(11)?,
                    status: row.get(12)?,
                    details_json: row.get(13)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LiveFillsResponse { fills })
    }

    /// Get the most recent live redemptions.
    pub async fn get_live_redemptions(
        &self,
        limit: u64,
    ) -> Result<LiveRedemptionsResponse, AgentError> {
        let conn = self.conn.lock().await;
        if !has_table(&conn, "live_redemptions") {
            return Ok(LiveRedemptionsResponse {
                redemptions: vec![],
            });
        }

        let mut stmt = conn.prepare_cached(
            "SELECT id, session_id, market_id, detected_redeemable_at_ms, submitted_at_ms,
                    confirmed_at_ms, cash_credit_observed_at_ms, status, redeemable_value,
                    tx_hash, details_json
             FROM live_redemptions
             ORDER BY detected_redeemable_at_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let redemptions = stmt
            .query_map(params![limit], |row| {
                Ok(LiveRedemptionRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    market_id: row.get(2)?,
                    detected_redeemable_at_ms: row.get(3)?,
                    submitted_at_ms: row.get(4)?,
                    confirmed_at_ms: row.get(5)?,
                    cash_credit_observed_at_ms: row.get(6)?,
                    status: row.get(7)?,
                    redeemable_value: row.get(8)?,
                    tx_hash: row.get(9)?,
                    details_json: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LiveRedemptionsResponse { redemptions })
    }

    /// Get the most recent live reconciliation events.
    pub async fn get_live_reconciliation(
        &self,
        limit: u64,
    ) -> Result<LiveReconciliationResponse, AgentError> {
        let conn = self.conn.lock().await;
        if !has_table(&conn, "live_reconciliation_events") {
            return Ok(LiveReconciliationResponse { events: vec![] });
        }

        let mut stmt = conn.prepare_cached(
            "SELECT id, session_id, timestamp_ms, severity, event_type, local_value,
                    remote_value, details_json
             FROM live_reconciliation_events
             ORDER BY timestamp_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let events = stmt
            .query_map(params![limit], |row| {
                Ok(LiveReconciliationRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    timestamp_ms: row.get(2)?,
                    severity: row.get(3)?,
                    event_type: row.get(4)?,
                    local_value: row.get(5)?,
                    remote_value: row.get(6)?,
                    details_json: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LiveReconciliationResponse { events })
    }

    /// Queue one audited live-control command for the running bot to apply.
    pub fn queue_live_control_command(
        &self,
        action: &str,
        actor: &str,
        reason: &str,
        confirmation: Option<&str>,
        bot_id: &str,
    ) -> Result<LiveControlCommandResponse, AgentError> {
        let actor = validated_operator_text("actor", actor)?;
        let reason = validated_operator_text("reason", reason)?;
        let bot_id = validated_operator_text("bot_id", bot_id)?;
        let action = normalized_live_control_action(action)?;
        let mut conn = self.open_write_connection()?;
        if !has_table(&conn, "live_control_commands") || !has_table(&conn, "control_audit") {
            return Err(AgentError::BotControlUnavailable(
                "live-control tables are not present in this database".to_string(),
            ));
        }
        let tx = conn.transaction()?;
        let (session_id, execution_mode, config_fingerprint) = tx
            .query_row(
                "SELECT id, execution_mode, config_fingerprint
                 FROM live_sessions
                 WHERE ended_at_ms IS NULL
                 ORDER BY started_at_ms DESC, id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                AgentError::BotControlUnavailable(
                    "no active live session found for live-control command".to_string(),
                )
            })?;
        if execution_mode != "live_trading" {
            return Err(AgentError::BotControlUnavailable(format!(
                "active live session is {execution_mode}; live-control requires live_trading"
            )));
        }
        validate_confirmation(action, confirmation, bot_id, &config_fingerprint)?;
        let now = now_ms();
        let details_json = serde_json::json!({
            "source": "dashboard",
            "action": action,
            "api_action": action.replace('-', "_"),
            "bot_id": bot_id,
            "confirmation_required": confirmation_is_required(action),
        })
        .to_string();
        tx.execute(
            "INSERT INTO live_control_commands (
                session_id, action, actor, reason, requested_at_ms, applied_at_ms, status,
                details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'pending', ?6)",
            params![session_id, action, actor, reason, now, details_json],
        )?;
        let command_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO control_audit (timestamp_ms, actor, action, target, details_json)
             VALUES (?1, ?2, 'live_control_requested', ?3, ?4)",
            params![
                now,
                actor,
                format!("live_control_commands:{command_id}"),
                serde_json::json!({
                    "session_id": session_id,
                    "command_id": command_id,
                    "action": action,
                    "reason": reason,
                    "source": "dashboard",
                    "bot_id": bot_id,
                })
                .to_string(),
            ],
        )?;
        tx.commit()?;
        Ok(LiveControlCommandResponse {
            ok: true,
            command_id,
            action: action.to_string(),
            status: "pending".to_string(),
        })
    }

    /// Get recent live-control audit events.
    pub async fn get_live_control_audit(
        &self,
        limit: u64,
    ) -> Result<LiveControlAuditResponse, AgentError> {
        let conn = self.conn.lock().await;
        if !has_table(&conn, "control_audit") {
            return Ok(LiveControlAuditResponse { entries: vec![] });
        }
        let mut stmt = conn.prepare_cached(
            "SELECT id, timestamp_ms, actor, action, target, details_json
             FROM control_audit
             ORDER BY timestamp_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(params![limit], |row| {
                Ok(LiveControlAuditRow {
                    id: row.get(0)?,
                    timestamp_ms: row.get(1)?,
                    actor: row.get(2)?,
                    action: row.get(3)?,
                    target: row.get(4)?,
                    details_json: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LiveControlAuditResponse { entries })
    }

    /// Return the sanitized runtime-config snapshot row written by the bot at startup.
    pub async fn get_runtime_config_snapshot(&self) -> Result<Option<RunMetadataRow>, AgentError> {
        let conn = self.conn.lock().await;
        if !has_table(&conn, "run_metadata") {
            return Ok(None);
        }
        conn.query_row(
            "SELECT value, recorded_at_ms FROM run_metadata WHERE key = 'runtime_config_snapshot' LIMIT 1",
            [],
            |row| {
                Ok(RunMetadataRow {
                    value: row.get(0)?,
                    recorded_at_ms: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(AgentError::from)
    }

    /// Get the latest trade ID (for WS polling).
    pub async fn get_latest_trade_id(&self) -> Result<i64, AgentError> {
        let conn = self.conn.lock().await;
        let id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM simulated_trades",
            [],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Get the latest balance log ID (for WS polling).
    pub async fn get_latest_balance_id(&self) -> Result<i64, AgentError> {
        let conn = self.conn.lock().await;
        let id: i64 =
            conn.query_row("SELECT COALESCE(MAX(id), 0) FROM balance_log", [], |row| {
                row.get(0)
            })?;
        Ok(id)
    }

    /// Get the latest signal ID (for WS polling).
    pub async fn get_latest_signal_id(&self) -> Result<i64, AgentError> {
        let conn = self.conn.lock().await;
        let id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM signals", [], |row| {
            row.get(0)
        })?;
        Ok(id)
    }

    /// Get trades newer than the given ID.
    pub async fn get_trades_since(&self, since_id: i64) -> Result<Vec<TradeRow>, AgentError> {
        let conn = self.conn.lock().await;
        let pnl_expr = if has_column(&conn, "trade_results", "pnl_net") {
            "COALESCE(r.pnl_net, r.pnl_0pct)".to_string()
        } else {
            "r.pnl_0pct".to_string()
        };
        let fill_status_expr = if has_column(&conn, "simulated_trades", "fill_status") {
            "t.fill_status".to_string()
        } else {
            "NULL AS fill_status".to_string()
        };
        let execution_group_id_expr = if has_column(&conn, "simulated_trades", "execution_group_id")
        {
            "t.execution_group_id".to_string()
        } else {
            "NULL AS execution_group_id".to_string()
        };
        let execution_fidelity_expr = if has_column(&conn, "simulated_trades", "execution_fidelity")
        {
            "t.execution_fidelity".to_string()
        } else {
            "NULL AS execution_fidelity".to_string()
        };
        let filled_size_expr = if has_column(&conn, "simulated_trades", "filled_size") {
            "t.filled_size".to_string()
        } else {
            "NULL AS filled_size".to_string()
        };
        let avg_fill_price_expr = if has_column(&conn, "simulated_trades", "avg_fill_price") {
            "t.avg_fill_price".to_string()
        } else {
            "NULL AS avg_fill_price".to_string()
        };
        let sql = format!(
            "SELECT t.id, t.timestamp, t.market_id, t.strategy, t.side, t.entry_price, \
             t.size, t.status, {pnl_expr}, r.settlement_price, r.resolved_at, \
             {fill_status_expr}, {execution_group_id_expr}, {execution_fidelity_expr}, \
             {filled_size_expr}, {avg_fill_price_expr} \
             FROM simulated_trades t \
             LEFT JOIN trade_results r ON r.trade_id = t.id \
             WHERE t.id > ?1 ORDER BY t.id"
        );
        let mut stmt = conn.prepare_cached(&sql)?;

        let trades = stmt
            .query_map(params![since_id], |row| {
                Ok(TradeRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    market_id: row.get(2)?,
                    strategy: row.get(3)?,
                    side: row.get(4)?,
                    entry_price: row.get(5)?,
                    size: row.get(6)?,
                    status: row.get(7)?,
                    pnl: row.get(8)?,
                    settlement_price: row.get(9)?,
                    resolved_at: row.get(10)?,
                    fill_status: row.get(11)?,
                    execution_group_id: row.get(12)?,
                    execution_fidelity: row.get(13)?,
                    filled_size: row.get(14)?,
                    avg_fill_price: row.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(trades)
    }

    /// Get balance entries newer than the given ID.
    pub async fn get_balance_since(&self, since_id: i64) -> Result<Vec<BalanceEntry>, AgentError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare_cached(
            "SELECT id, timestamp, event, trade_id, amount, balance \
             FROM balance_log WHERE id > ?1 ORDER BY id",
        )?;

        let entries = stmt
            .query_map(params![since_id], |row| {
                Ok(BalanceEntry {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    event: row.get(2)?,
                    trade_id: row.get(3)?,
                    amount: row.get(4)?,
                    balance: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    /// Get signals newer than the given ID.
    pub async fn get_signals_since(&self, since_id: i64) -> Result<Vec<SignalRow>, AgentError> {
        let conn = self.conn.lock().await;
        let market_id_expr = if has_column(&conn, "signals", "market_id") {
            "market_id".to_string()
        } else {
            "NULL AS market_id".to_string()
        };
        let execution_fidelity_expr = if has_column(&conn, "signals", "execution_fidelity") {
            "execution_fidelity".to_string()
        } else {
            "NULL AS execution_fidelity".to_string()
        };
        let sql = format!(
            "SELECT id, timestamp, strategy, direction, binance_price, chainlink_price, \
             up_ask, down_ask, metadata, {market_id_expr}, {execution_fidelity_expr} \
             FROM signals WHERE id > ?1 ORDER BY id"
        );
        let mut stmt = conn.prepare_cached(&sql)?;

        let signals = stmt
            .query_map(params![since_id], |row| {
                Ok(SignalRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    strategy: row.get(2)?,
                    direction: row.get(3)?,
                    binance_price: row.get(4)?,
                    chainlink_price: row.get(5)?,
                    up_ask: row.get(6)?,
                    down_ask: row.get(7)?,
                    metadata: row.get(8)?,
                    market_id: row.get(9)?,
                    execution_fidelity: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(signals)
    }

    /// Open a short-lived write connection for the control-command queue.
    fn open_write_connection(&self) -> Result<Connection, AgentError> {
        let Some(db_path) = &self.db_path else {
            return Err(AgentError::BotControlUnavailable(
                "live-control writes require a database path".to_string(),
            ));
        };
        Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| {
            AgentError::Internal(format!("failed to open database for control write: {e}"))
        })
    }
}

/// Return current unix time in milliseconds for control ledger rows.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Validate and trim an operator-supplied text field.
fn validated_operator_text<'a>(field: &str, value: &'a str) -> Result<&'a str, AgentError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AgentError::BadRequest(format!("{field} must not be empty")));
    }
    Ok(trimmed)
}

/// Normalize the dashboard API spelling into the stored CLI command spelling.
fn normalized_live_control_action(action: &str) -> Result<&'static str, AgentError> {
    match action {
        "preflight" => Ok("preflight"),
        "arm" => Ok("arm"),
        "disarm" => Ok("disarm"),
        "stop_after_flat" | "stop-after-flat" => Ok("stop-after-flat"),
        "kill_switch" | "kill-switch" => Ok("kill-switch"),
        "cancel_all" | "cancel-all" => Ok("cancel-all"),
        "redeem_all" | "redeem-all" => Ok("redeem-all"),
        other => Err(AgentError::BadRequest(format!(
            "invalid live-control action {other}"
        ))),
    }
}

/// Return whether a dashboard action needs an exact typed confirmation.
fn confirmation_is_required(action: &str) -> bool {
    matches!(action, "arm" | "kill-switch" | "cancel-all" | "redeem-all")
}

/// Validate dangerous dashboard confirmations without persisting the typed phrase.
fn validate_confirmation(
    action: &str,
    confirmation: Option<&str>,
    bot_id: &str,
    config_fingerprint: &str,
) -> Result<(), AgentError> {
    if !confirmation_is_required(action) {
        return Ok(());
    }
    let expected = match action {
        "arm" => format!("ARM {bot_id} {}", fingerprint_prefix(config_fingerprint)),
        "kill-switch" => format!("KILL {bot_id}"),
        "cancel-all" => format!("CANCEL ALL {bot_id}"),
        "redeem-all" => format!("REDEEM ALL {bot_id}"),
        _ => String::new(),
    };
    if confirmation != Some(expected.as_str()) {
        return Err(AgentError::BadRequest(format!(
            "confirmation must exactly match {expected}"
        )));
    }
    Ok(())
}

/// Return the stable fingerprint prefix used in arming confirmations.
fn fingerprint_prefix(config_fingerprint: &str) -> String {
    config_fingerprint.chars().take(12).collect()
}

/// Compute the maximum historical drawdown from the balance log.
fn compute_max_drawdown(conn: &Connection) -> f64 {
    let Ok(mut stmt) = conn.prepare("SELECT balance FROM balance_log ORDER BY id") else {
        return 0.0;
    };

    let Ok(rows) = stmt.query_map([], |row| row.get::<_, f64>(0)) else {
        return 0.0;
    };

    let balances: Vec<f64> = rows.filter_map(Result::ok).collect();

    let mut hwm = 0.0_f64;
    let mut max_dd = 0.0_f64;
    for b in balances {
        hwm = hwm.max(b);
        if hwm > 0.0 {
            let dd = (hwm - b) / hwm;
            max_dd = max_dd.max(dd);
        }
    }
    max_dd
}

#[cfg(test)]
#[path = "tests/db_reader_tests.rs"]
mod tests;
