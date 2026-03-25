use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use crate::error::AgentError;
use crate::types::{
    BalanceEntry, BalanceResponse, BotStatus, SignalRow, SignalsResponse, StatsResponse,
    StrategyStats, TradeRow, TradesResponse, WindowInfo,
};

/// Read-only database reader for the bot's `SQLite` database.
pub struct DbReader {
    conn: Arc<Mutex<Connection>>,
}

impl DbReader {
    /// Open the bot's database in read-only mode with WAL.
    pub fn new(db_path: &str) -> Result<Self, AgentError> {
        let conn = Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| AgentError::Internal(format!("failed to open database: {e}")))?;

        // WAL mode must already be set by the bot (writer). We only read.
        conn.execute_batch("PRAGMA query_only = ON;")
            .map_err(|e| AgentError::Internal(format!("failed to set query_only: {e}")))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create a `DbReader` from an existing connection (for testing).
    #[cfg(test)]
    pub fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Get the bot's current status.
    pub async fn get_status(&self) -> Result<BotStatus, AgentError> {
        let conn = self.conn.lock().await;

        // Latest balance
        let (balance, starting_balance) = conn
            .query_row(
                "SELECT balance, \
                 (SELECT balance FROM balance_log WHERE event = 'init' ORDER BY id LIMIT 1) \
                 FROM balance_log ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
            )
            .unwrap_or((0.0, 0.0));

        // Trade counts
        let total_trades: u64 = conn
            .query_row("SELECT COUNT(*) FROM trade_results", [], |row| row.get(0))
            .unwrap_or(0);

        let wins: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM trade_results WHERE pnl_0pct > 0",
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
                "SELECT COALESCE(SUM(pnl_0pct), 0.0) FROM trade_results",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        // High water mark and drawdown from balance_log
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

        // Uptime from first to last tick
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

        // Open trades
        let open_trades: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM simulated_trades WHERE status = 'open'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Current window (most recent active market)
        let current_window = conn
            .query_row(
                "SELECT market_id, question, end_time FROM markets \
                 WHERE status = 'active' ORDER BY end_time DESC LIMIT 1",
                [],
                |row| {
                    Ok(WindowInfo {
                        market_id: row.get(0)?,
                        question: row.get(1)?,
                        end_time: row.get(2)?,
                    })
                },
            )
            .ok();

        // Use current time if no tick data
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Compute max historical drawdown (not just current)
        let max_dd = compute_max_drawdown(&conn);

        Ok(BotStatus {
            balance,
            starting_balance,
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
        })
    }

    /// Get paginated trade history.
    pub async fn get_trades(&self, page: u64, per_page: u64) -> Result<TradesResponse, AgentError> {
        let conn = self.conn.lock().await;

        let total: u64 = conn.query_row("SELECT COUNT(*) FROM simulated_trades", [], |row| {
            row.get(0)
        })?;

        let offset = (page.saturating_sub(1)) * per_page;

        let mut stmt = conn.prepare_cached(
            "SELECT t.id, t.timestamp, t.market_id, t.strategy, t.side, t.entry_price, \
             t.size, t.status, r.pnl_0pct, r.settlement_price, r.resolved_at \
             FROM simulated_trades t \
             LEFT JOIN trade_results r ON r.trade_id = t.id \
             ORDER BY t.timestamp DESC \
             LIMIT ?1 OFFSET ?2",
        )?;

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

    /// Get recent signals.
    pub async fn get_signals(&self, limit: u64) -> Result<SignalsResponse, AgentError> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare_cached(
            "SELECT id, timestamp, strategy, direction, binance_price, chainlink_price, \
             up_ask, down_ask, metadata \
             FROM signals ORDER BY timestamp DESC LIMIT ?1",
        )?;

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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SignalsResponse { signals })
    }

    /// Get aggregated stats per strategy.
    pub async fn get_stats(&self) -> Result<StatsResponse, AgentError> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare_cached(
            "SELECT t.strategy, COUNT(*) as trades, \
             SUM(CASE WHEN r.pnl_0pct > 0 THEN 1 ELSE 0 END) as wins, \
             SUM(CASE WHEN r.pnl_0pct <= 0 THEN 1 ELSE 0 END) as losses, \
             COALESCE(SUM(r.pnl_0pct), 0.0) as total_pnl \
             FROM trade_results r \
             JOIN simulated_trades t ON r.trade_id = t.id \
             GROUP BY t.strategy",
        )?;

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
        let mut stmt = conn.prepare_cached(
            "SELECT t.id, t.timestamp, t.market_id, t.strategy, t.side, t.entry_price, \
             t.size, t.status, r.pnl_0pct, r.settlement_price, r.resolved_at \
             FROM simulated_trades t \
             LEFT JOIN trade_results r ON r.trade_id = t.id \
             WHERE t.id > ?1 ORDER BY t.id",
        )?;

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
        let mut stmt = conn.prepare_cached(
            "SELECT id, timestamp, strategy, direction, binance_price, chainlink_price, \
             up_ask, down_ask, metadata \
             FROM signals WHERE id > ?1 ORDER BY id",
        )?;

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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(signals)
    }
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
