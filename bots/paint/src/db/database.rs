use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use rusqlite::params;

use super::schema;
use crate::types::{
    FeedEvent, MarketWindow, ReplayFidelity, Signal, SignalDirection, SimulatedTrade, TradeResult,
    TradeStatus,
};

/// Thin wrapper around a `SQLite` connection that mirrors the `TypeScript` `Database`
/// class.  All prepared statements use `prepare_cached` so the cache is reused
/// across repeated calls (matching `better-sqlite3` behaviour).
pub struct Database {
    conn: rusqlite::Connection,
}

impl Database {
    /// Open (or create) the database at `db_path`, enable WAL mode + NORMAL
    /// synchronous, and run all schema migrations.
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        if let Some(parent) = Path::new(db_path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating directory for DB: {}", parent.display()))?;
            }
        }

        let conn = rusqlite::Connection::open(db_path)
            .with_context(|| format!("opening SQLite database at {db_path}"))?;

        conn.busy_timeout(std::time::Duration::from_secs(30))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        schema::run_migrations(&conn)?;

        Ok(Self { conn })
    }

    /// Logs tick.
    #[allow(clippy::too_many_arguments)]
    pub fn log_tick(
        &self,
        timestamp: u64,
        source: &str,
        price: Option<f64>,
        bid: Option<f64>,
        ask: Option<f64>,
        bid_size: Option<f64>,
        ask_size: Option<f64>,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO tick_data (timestamp, source, price, bid, ask, bid_size, ask_size) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        stmt.execute(params![
            timestamp, source, price, bid, ask, bid_size, ask_size
        ])?;
        Ok(())
    }

    /// Logs feed event.
    pub fn log_feed_event(&self, event: &FeedEvent) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO feed_events (
                received_at_ms, event_at_ms, source, event_type, market_id, asset_id,
                price, best_bid, best_ask, bid_size, ask_size, payload_json, fidelity
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        stmt.execute(params![
            event.received_at_ms,
            event.event_at_ms,
            event.source,
            event.event_type,
            event.market_id,
            event.asset_id,
            event.price,
            event.best_bid,
            event.best_ask,
            event.bid_size,
            event.ask_size,
            event.payload_json,
            event.fidelity.to_string(),
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Upsert market.
    pub fn upsert_market(&self, window: &MarketWindow) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO markets (
                market_id, question, condition_id, slug, up_token_id, down_token_id, start_time,
                end_time, outcome, resolution_source, fee_profile, order_min_size,
                order_price_min_tick_size, maker_base_fee, taker_base_fee, rewards_min_size,
                rewards_max_spread
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
             ) \
             ON CONFLICT(market_id) DO UPDATE SET \
               question      = excluded.question, \
               condition_id  = excluded.condition_id, \
               slug          = excluded.slug, \
               up_token_id   = excluded.up_token_id, \
               down_token_id = excluded.down_token_id, \
               start_time    = excluded.start_time, \
               end_time      = excluded.end_time, \
               outcome       = COALESCE(excluded.outcome, markets.outcome), \
               resolution_source = COALESCE(excluded.resolution_source, markets.resolution_source), \
               fee_profile   = COALESCE(excluded.fee_profile, markets.fee_profile), \
               order_min_size = COALESCE(excluded.order_min_size, markets.order_min_size), \
               order_price_min_tick_size = COALESCE(excluded.order_price_min_tick_size, markets.order_price_min_tick_size), \
               maker_base_fee = COALESCE(excluded.maker_base_fee, markets.maker_base_fee), \
               taker_base_fee = COALESCE(excluded.taker_base_fee, markets.taker_base_fee), \
               rewards_min_size = COALESCE(excluded.rewards_min_size, markets.rewards_min_size), \
               rewards_max_spread = COALESCE(excluded.rewards_max_spread, markets.rewards_max_spread)",
        )?;
        stmt.execute(params![
            window.market_id,
            window.question,
            window.condition_id,
            window.slug,
            window.up_token_id,
            window.down_token_id,
            window.start_time,
            window.end_time,
            window.outcome,
            window.resolution_source,
            window.fee_profile,
            window.order_min_size,
            window.order_price_min_tick_size,
            window.maker_base_fee,
            window.taker_base_fee,
            window.rewards_min_size,
            window.rewards_max_spread,
        ])?;
        Ok(())
    }

    /// Logs signal.
    pub fn log_signal(&self, signal: &Signal) -> anyhow::Result<()> {
        let _ = self.log_signal_with_context(signal, None, None, None, None)?;
        Ok(())
    }

    /// Logs signal with context.
    pub fn log_signal_with_context(
        &self,
        signal: &Signal,
        market_id: Option<&str>,
        execution_fidelity: Option<ReplayFidelity>,
        order_submitted_at_ms: Option<u64>,
        order_arrival_at_ms: Option<u64>,
    ) -> anyhow::Result<i64> {
        let metadata_json = serde_json::to_string(&signal.metadata)?;
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO signals (
                timestamp, strategy, direction, binance_price, chainlink_price,
                up_ask, down_ask, up_bid, down_bid, metadata, market_id, execution_fidelity,
                order_submitted_at_ms, order_arrival_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )?;
        stmt.execute(params![
            signal.timestamp,
            signal.strategy,
            signal.direction.to_string(),
            signal.binance_price,
            signal.chainlink_price,
            signal.up_ask,
            signal.down_ask,
            signal.up_bid,
            signal.down_bid,
            metadata_json,
            market_id,
            execution_fidelity.map(|f| f.to_string()),
            order_submitted_at_ms,
            order_arrival_at_ms,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert a new open trade and return the auto-generated row ID.
    pub fn open_trade(&self, trade: &SimulatedTrade) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO simulated_trades (
                timestamp, market_id, strategy, side, token_id, entry_price, size, status,
                signal_id, execution_mode, order_id, fill_price, requested_price, requested_size,
                filled_size, avg_fill_price, fill_status, fill_reason, fill_latency_ms,
                execution_group_id, execution_fidelity
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19,
                ?20, ?21
             )",
        )?;
        stmt.execute(params![
            trade.timestamp,
            trade.market_id,
            trade.strategy,
            trade.side.to_string(),
            trade.token_id,
            trade.entry_price,
            trade.size,
            trade.status.to_string(),
            trade.signal_id,
            trade.execution_mode,
            trade.order_id,
            trade.fill_price,
            trade.requested_price,
            trade.requested_size,
            trade.filled_size,
            trade.avg_fill_price,
            trade.fill_status,
            trade.fill_reason,
            trade.fill_latency_ms,
            trade.execution_group_id,
            trade.execution_fidelity,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Close an open trade: update its status to `closed` and insert the
    /// corresponding `trade_results` row.  Both operations run in a single
    /// transaction.
    pub fn close_trade(&self, trade_id: i64, result: &TradeResult) -> anyhow::Result<()> {
        let resolved_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let tx = self.conn.unchecked_transaction()?;

        tx.prepare_cached("UPDATE simulated_trades SET status = 'closed' WHERE id = ?1")?
            .execute(params![trade_id])?;

        tx.prepare_cached(
            "INSERT INTO trade_results (trade_id, exit_price, settlement_price, \
             pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, fee_amount, pnl_net, \
             settlement_status, provisional_pnl, resolved_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?
        .execute(params![
            trade_id,
            result.exit_price,
            result.settlement_price,
            result.pnl_0pct,
            result.pnl_1pct,
            result.pnl_2pct,
            result.pnl_3pct,
            result.fee_amount,
            result.pnl_net,
            result.settlement_status,
            result.provisional_pnl,
            resolved_at,
        ])?;

        tx.commit()?;
        Ok(())
    }

    /// Returns open trades for market.
    pub fn get_open_trades_for_market(
        &self,
        market_id: &str,
    ) -> anyhow::Result<Vec<SimulatedTrade>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, timestamp, market_id, strategy, side, token_id, entry_price, size, status,
                    signal_id, requested_price, requested_size, filled_size, avg_fill_price,
                    fill_status, fill_reason, fill_latency_ms, execution_group_id,
                    execution_fidelity, execution_mode, order_id, fill_price \
             FROM simulated_trades WHERE market_id = ?1 AND status = 'open'",
        )?;

        let rows = stmt.query_map(params![market_id], |row| {
            let side_str: String = row.get(4)?;
            let status_str: String = row.get(8)?;
            Ok(SimulatedTrade {
                id: Some(row.get(0)?),
                timestamp: row.get(1)?,
                market_id: row.get(2)?,
                strategy: row.get(3)?,
                side: SignalDirection::from_str(&side_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::from(e),
                    )
                })?,
                token_id: row.get(5)?,
                entry_price: row.get(6)?,
                size: row.get(7)?,
                status: TradeStatus::from_str(&status_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        8,
                        rusqlite::types::Type::Text,
                        Box::from(e),
                    )
                })?,
                signal_id: row.get(9)?,
                requested_price: row.get(10)?,
                requested_size: row.get(11)?,
                filled_size: row.get(12)?,
                avg_fill_price: row.get(13)?,
                fill_status: row.get(14)?,
                fill_reason: row.get(15)?,
                fill_latency_ms: row.get(16)?,
                execution_group_id: row.get(17)?,
                execution_fidelity: row.get(18)?,
                execution_mode: row.get(19)?,
                order_id: row.get(20)?,
                fill_price: row.get(21)?,
            })
        })?;

        let mut trades = Vec::new();
        for row in rows {
            trades.push(row?);
        }
        Ok(trades)
    }

    /// Count open trades.
    pub fn count_open_trades(&self) -> anyhow::Result<u64> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT COUNT(*) FROM simulated_trades WHERE status = 'open'")?;
        let count = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    /// Logs balance event.
    pub fn log_balance_event(
        &self,
        timestamp: u64,
        event: &str,
        trade_id: Option<i64>,
        amount: f64,
        balance: f64,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        stmt.execute(params![timestamp, event, trade_id, amount, balance])?;
        Ok(())
    }

    /// Returns latest balance.
    pub fn get_latest_balance(&self) -> anyhow::Result<Option<f64>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT balance FROM balance_log ORDER BY id DESC LIMIT 1")?;
        let result = stmt.query_row([], |row| row.get(0)).optional()?;
        Ok(result)
    }

    /// Resolves market.
    pub fn resolve_market(&self, market_id: &str, status: &str) -> anyhow::Result<()> {
        let mut stmt = self
            .conn
            .prepare_cached("UPDATE markets SET status = ?1 WHERE market_id = ?2")?;
        stmt.execute(params![status, market_id])?;
        Ok(())
    }

    /// Resolve a market and record the outcome (UP or DOWN).
    pub fn resolve_market_with_outcome(
        &self,
        market_id: &str,
        status: &str,
        outcome: &str,
    ) -> anyhow::Result<()> {
        let mut stmt = self
            .conn
            .prepare_cached("UPDATE markets SET status = ?1, outcome = ?2 WHERE market_id = ?3")?;
        stmt.execute(params![status, outcome, market_id])?;
        Ok(())
    }

    /// Log a settlement audit entry comparing our prediction against Polymarket's outcome.
    pub fn log_settlement_audit(
        &self,
        trade_id: i64,
        market_id: &str,
        our_prediction: &str,
        polymarket_outcome: &str,
        timestamp: u64,
    ) -> anyhow::Result<()> {
        let matched = i32::from(our_prediction == polymarket_outcome);
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO settlement_audit (trade_id, market_id, our_prediction, polymarket_outcome, matched, timestamp) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        #[allow(clippy::cast_possible_wrap)]
        stmt.execute(params![
            trade_id,
            market_id,
            our_prediction,
            polymarket_outcome,
            matched,
            timestamp as i64,
        ])?;
        Ok(())
    }

    /// Read a single trade by its ID (regardless of status).
    pub fn get_trade_by_id(&self, trade_id: i64) -> anyhow::Result<Option<SimulatedTrade>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, timestamp, market_id, strategy, side, token_id, entry_price, size, status,
                    signal_id, requested_price, requested_size, filled_size, avg_fill_price,
                    fill_status, fill_reason, fill_latency_ms, execution_group_id,
                    execution_fidelity, execution_mode, order_id, fill_price \
             FROM simulated_trades WHERE id = ?1",
        )?;
        let result = stmt
            .query_row(params![trade_id], |row| {
                let side_str: String = row.get(4)?;
                let status_str: String = row.get(8)?;
                Ok(SimulatedTrade {
                    id: Some(row.get(0)?),
                    timestamp: row.get(1)?,
                    market_id: row.get(2)?,
                    strategy: row.get(3)?,
                    side: SignalDirection::from_str(&side_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::from(e),
                        )
                    })?,
                    token_id: row.get(5)?,
                    entry_price: row.get(6)?,
                    size: row.get(7)?,
                    status: TradeStatus::from_str(&status_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            8,
                            rusqlite::types::Type::Text,
                            Box::from(e),
                        )
                    })?,
                    signal_id: row.get(9)?,
                    requested_price: row.get(10)?,
                    requested_size: row.get(11)?,
                    filled_size: row.get(12)?,
                    avg_fill_price: row.get(13)?,
                    fill_status: row.get(14)?,
                    fill_reason: row.get(15)?,
                    fill_latency_ms: row.get(16)?,
                    execution_group_id: row.get(17)?,
                    execution_fidelity: row.get(18)?,
                    execution_mode: row.get(19)?,
                    order_id: row.get(20)?,
                    fill_price: row.get(21)?,
                })
            })
            .optional()?;
        Ok(result)
    }

    /// Read a trade result by its trade ID.
    pub fn get_trade_result(&self, trade_id: i64) -> anyhow::Result<Option<TradeResult>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, \
             COALESCE(fee_amount, 0), COALESCE(pnl_net, pnl_0pct), \
             COALESCE(settlement_status, 'confirmed'), provisional_pnl \
             FROM trade_results WHERE trade_id = ?1",
        )?;
        let result = stmt
            .query_row(params![trade_id], |row| {
                Ok(TradeResult {
                    trade_id: row.get(0)?,
                    exit_price: row.get(1)?,
                    settlement_price: row.get(2)?,
                    pnl_0pct: row.get(3)?,
                    pnl_1pct: row.get(4)?,
                    pnl_2pct: row.get(5)?,
                    pnl_3pct: row.get(6)?,
                    fee_amount: row.get(7)?,
                    pnl_net: row.get(8)?,
                    settlement_status: row.get(9)?,
                    provisional_pnl: row.get(10)?,
                })
            })
            .optional()?;
        Ok(result)
    }

    /// Update a trade result with corrected settlement data.
    pub fn update_trade_settlement(
        &self,
        trade_id: i64,
        result: &TradeResult,
        status: &str,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE trade_results SET \
             settlement_price = ?1, pnl_0pct = ?2, pnl_1pct = ?3, pnl_2pct = ?4, pnl_3pct = ?5, \
             fee_amount = ?6, pnl_net = ?7, settlement_status = ?8 \
             WHERE trade_id = ?9",
        )?;
        stmt.execute(params![
            result.settlement_price,
            result.pnl_0pct,
            result.pnl_1pct,
            result.pnl_2pct,
            result.pnl_3pct,
            result.fee_amount,
            result.pnl_net,
            status,
            trade_id,
        ])?;
        Ok(())
    }

    /// Consume the `Database`, closing the underlying connection.
    ///
    /// In `Rust` the connection is dropped automatically, so this is mostly a
    /// semantic mirror of the `TypeScript` `close()` method.
    pub fn close(self) {
        drop(self);
    }

    /// Returns the underlying `SQLite` connection for read-only helper queries.
    pub fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }
}

/// Re-export for callers that need the `optional()` extension.
use rusqlite::OptionalExtension;

#[cfg(test)]
#[path = "tests/database_tests.rs"]
mod tests;
