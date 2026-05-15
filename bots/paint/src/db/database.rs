use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use rusqlite::params;

use super::schema;
use crate::db::clob_replay_blocks::{self, EncodedClobReplayBlock};
use crate::types::{
    ControlAuditEntry, FeedEvent, FeedHealthEvent, LiveAccountSnapshot, LiveControlCommand,
    LiveControlState, LiveFill, LiveOrder, LiveOrderIntent, LiveReconciliationEvent,
    LiveRedemption, LiveSession, MarketWindow, ReplayFidelity, Signal, SignalDirection,
    SignalMetricRecord, SignalTelemetry, SimulatedTrade, StrategyRejectionSummaryRecord,
    TradeResult, TradeStatus,
};

/// Thin wrapper around a `SQLite` connection that mirrors the `TypeScript` `Database`
/// class.  All prepared statements use `prepare_cached` so the cache is reused
/// across repeated calls (matching `better-sqlite3` behaviour).
pub struct Database {
    conn: rusqlite::Connection,
    db_path: String,
}

#[derive(Debug, Clone)]
pub struct FeedEventFootprintRow {
    pub source: String,
    pub event_type: String,
    pub row_count: u64,
    pub approx_payload_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DbFootprint {
    pub db_bytes: u64,
    pub wal_bytes: u64,
    pub feed_event_count: u64,
    pub grouped_feed_events: Vec<FeedEventFootprintRow>,
}

#[derive(Debug, Clone)]
pub struct ExecutionStatusRollup {
    pub submitted: u64,
    pub filled: u64,
    pub missed: u64,
    pub rejected_before_queue: u64,
    pub partial: u64,
    pub mean_effective_arrival_delay_ms: Option<f64>,
    pub miss_reasons: Vec<(String, u64)>,
    pub queue_rejection_reasons: Vec<(String, u64)>,
}

#[derive(Debug, Clone)]
pub struct PendingResolutionMarket {
    pub window: MarketWindow,
    pub open_trade_count: u64,
}

#[derive(Debug, Clone)]
pub struct UnresolvedTradeExposure {
    pub trade_id: i64,
    pub market_id: String,
    pub strategy: String,
    pub entry_price: f64,
    pub size: f64,
    pub market_end_time: u64,
}

#[derive(Debug, Clone)]
pub struct OpenTradeSnapshot {
    pub trade: SimulatedTrade,
    pub market_end_time: u64,
}

/// Mutable status fields for one live venue order row.
#[derive(Debug, Clone, Copy)]
pub struct LiveOrderStatusUpdate<'a> {
    pub status: &'a str,
    pub status_reason: Option<&'a str>,
    pub acknowledged_at_ms: Option<u64>,
    pub updated_at_ms: u64,
    pub accepted_size: Option<f64>,
    pub details_json: Option<&'a str>,
}

/// Signal evidence that must be durable before a funded live order reaches the venue.
pub struct LiveDecisionEvidence<'a> {
    pub signal_id: i64,
    pub signal: &'a Signal,
    pub market_id: &'a str,
    pub execution_fidelity: ReplayFidelity,
    pub order_submitted_at_ms: Option<u64>,
    pub expected_arrival_at_ms: Option<u64>,
    pub decision_status: &'a str,
    pub rejection_reason: Option<&'a str>,
}

/// Return whether one feed event should use compact `CLOB` replay storage.
pub(crate) fn is_clob_top_of_book_event(event: &FeedEvent) -> bool {
    clob_replay_blocks::is_blockable_clob_event(event)
}

/// Return the compact side label for one `CLOB` source.
fn clob_side(source: &str) -> &'static str {
    if source == "clob_up" { "up" } else { "down" }
}

/// Insert one compressed CLOB replay block.
fn insert_clob_replay_block(
    tx: &rusqlite::Transaction<'_>,
    block: &EncodedClobReplayBlock,
) -> anyhow::Result<()> {
    tx.execute(
        "INSERT INTO clob_replay_blocks (
            min_received_at_ms, max_received_at_ms, min_received_at_us, max_received_at_us,
            row_count, up_rows, down_rows, codec, schema_version, compressed_bytes,
            uncompressed_bytes, checksum, payload
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'zstd', ?8, ?9, ?10, ?11, ?12)",
        params![
            block.min_received_at_ms,
            block.max_received_at_ms,
            block.min_received_at_us,
            block.max_received_at_us,
            block.row_count,
            block.up_rows,
            block.down_rows,
            block.schema_version,
            block.compressed_bytes,
            block.uncompressed_bytes,
            block.checksum,
            &block.payload,
        ],
    )
    .context("inserting CLOB replay block")?;
    Ok(())
}

/// Mutable lifecycle fields for one live redemption row.
#[derive(Debug, Clone, Copy)]
pub struct LiveRedemptionStatusUpdate<'a> {
    pub status: &'a str,
    pub submitted_at_ms: Option<u64>,
    pub confirmed_at_ms: Option<u64>,
    pub cash_credit_observed_at_ms: Option<u64>,
    pub tx_hash: Option<&'a str>,
    pub details_json: Option<&'a str>,
}

impl Database {
    /// Open (or create) the database at `db_path`, enable WAL mode + NORMAL
    /// synchronous, and run all schema migrations.
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        Self::open_inner(db_path, true, false)
    }

    /// Open an already-initialized runtime database without running migrations.
    pub fn open_runtime(db_path: &str) -> anyhow::Result<Self> {
        Self::open_inner(db_path, false, true)
    }

    /// Open one `SQLite` connection with runtime PRAGMAs and optional migrations.
    fn open_inner(
        db_path: &str,
        run_migrations: bool,
        require_existing: bool,
    ) -> anyhow::Result<Self> {
        if require_existing && !Path::new(db_path).exists() {
            bail!("runtime SQLite database does not exist at {db_path}; initialize it first");
        }
        if let Some(parent) = Path::new(db_path).parent()
            && !parent.as_os_str().is_empty()
            && !require_existing
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory for DB: {}", parent.display()))?;
        }

        let conn = rusqlite::Connection::open(db_path)
            .with_context(|| format!("opening SQLite database at {db_path}"))?;

        apply_runtime_pragmas(&conn)?;

        if run_migrations {
            schema::run_migrations(&conn)?;
        }

        Ok(Self {
            conn,
            db_path: db_path.to_string(),
        })
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
                received_at_ms, event_at_ms, received_at_us, event_at_us, source, event_type,
                source_topic, source_symbol, connection_id, sequence_key, market_id, asset_id,
                price, trade_size, signed_quantity, best_bid, best_ask, bid_size, ask_size,
                depth_bid_notional, depth_ask_notional, depth_imbalance, microprice,
                payload_json, details_json, fidelity
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                ?20, ?21, ?22, ?23, ?24, ?25, ?26
             )",
        )?;
        stmt.execute(params![
            event.received_at_ms,
            event.event_at_ms,
            event.received_at_us,
            event.event_at_us,
            event.source,
            event.event_type,
            event.source_topic,
            event.source_symbol,
            event.connection_id,
            event.sequence_key,
            event.market_id,
            event.asset_id,
            event.price,
            event.trade_size,
            event.signed_quantity,
            event.best_bid,
            event.best_ask,
            event.bid_size,
            event.ask_size,
            event.depth_bid_notional,
            event.depth_ask_notional,
            event.depth_imbalance,
            event.microprice,
            event.payload_json,
            event.details_json,
            event.fidelity.to_string(),
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Logs a batch of generic feed events inside one transaction.
    pub fn log_feed_events_batch(&self, events: &[FeedEvent]) -> anyhow::Result<u64> {
        self.log_feed_events_batch_with_compact_clob(events, false)
    }

    /// Logs a batch of feed events with optional compact `CLOB` replay routing.
    pub fn log_feed_events_batch_with_compact_clob(
        &self,
        events: &[FeedEvent],
        compact_clob_replay: bool,
    ) -> anyhow::Result<u64> {
        if events.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO feed_events (
                    received_at_ms, event_at_ms, received_at_us, event_at_us, source, event_type,
                    source_topic, source_symbol, connection_id, sequence_key, market_id, asset_id,
                    price, trade_size, signed_quantity, best_bid, best_ask, bid_size, ask_size,
                    depth_bid_notional, depth_ask_notional, depth_imbalance, microprice,
                    payload_json, details_json, fidelity
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                    ?20, ?21, ?22, ?23, ?24, ?25, ?26
                 )",
            )?;
            let mut clob_stmt = tx.prepare_cached(
                "INSERT INTO clob_replay_events (
                    received_at_ms, event_at_ms, received_at_us, event_at_us, side, source,
                    event_type, source_topic, connection_id, sequence_key, market_id, asset_id,
                    best_bid, best_ask, bid_size, ask_size, microprice, fidelity
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16, ?17, ?18
                 )",
            )?;
            for event in events {
                if compact_clob_replay && is_clob_top_of_book_event(event) {
                    clob_stmt.execute(params![
                        event.received_at_ms,
                        event.event_at_ms,
                        event.received_at_us,
                        event.event_at_us,
                        clob_side(&event.source),
                        event.source,
                        event.event_type,
                        event.source_topic,
                        event.connection_id,
                        event.sequence_key,
                        event.market_id,
                        event.asset_id,
                        event.best_bid,
                        event.best_ask,
                        event.bid_size,
                        event.ask_size,
                        event.microprice,
                        event.fidelity.to_string(),
                    ])?;
                } else {
                    stmt.execute(params![
                        event.received_at_ms,
                        event.event_at_ms,
                        event.received_at_us,
                        event.event_at_us,
                        event.source,
                        event.event_type,
                        event.source_topic,
                        event.source_symbol,
                        event.connection_id,
                        event.sequence_key,
                        event.market_id,
                        event.asset_id,
                        event.price,
                        event.trade_size,
                        event.signed_quantity,
                        event.best_bid,
                        event.best_ask,
                        event.bid_size,
                        event.ask_size,
                        event.depth_bid_notional,
                        event.depth_ask_notional,
                        event.depth_imbalance,
                        event.microprice,
                        event.payload_json,
                        event.details_json,
                        event.fidelity.to_string(),
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(events.len() as u64)
    }

    /// Logs generic feed rows and one optional compressed `CLOB` replay block.
    pub fn log_feed_events_and_clob_block(
        &self,
        generic_events: &[FeedEvent],
        clob_events: &[FeedEvent],
        zstd_level: i32,
    ) -> anyhow::Result<u64> {
        if generic_events.is_empty() && clob_events.is_empty() {
            return Ok(0);
        }
        let encoded_block = if clob_events.is_empty() {
            None
        } else {
            Some(clob_replay_blocks::encode_events(clob_events, zstd_level)?)
        };
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO feed_events (
                    received_at_ms, event_at_ms, received_at_us, event_at_us, source, event_type,
                    source_topic, source_symbol, connection_id, sequence_key, market_id, asset_id,
                    price, trade_size, signed_quantity, best_bid, best_ask, bid_size, ask_size,
                    depth_bid_notional, depth_ask_notional, depth_imbalance, microprice,
                    payload_json, details_json, fidelity
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                    ?20, ?21, ?22, ?23, ?24, ?25, ?26
                 )",
            )?;
            for event in generic_events {
                stmt.execute(params![
                    event.received_at_ms,
                    event.event_at_ms,
                    event.received_at_us,
                    event.event_at_us,
                    event.source,
                    event.event_type,
                    event.source_topic,
                    event.source_symbol,
                    event.connection_id,
                    event.sequence_key,
                    event.market_id,
                    event.asset_id,
                    event.price,
                    event.trade_size,
                    event.signed_quantity,
                    event.best_bid,
                    event.best_ask,
                    event.bid_size,
                    event.ask_size,
                    event.depth_bid_notional,
                    event.depth_ask_notional,
                    event.depth_imbalance,
                    event.microprice,
                    event.payload_json,
                    event.details_json,
                    event.fidelity.to_string(),
                ])?;
            }
            if let Some(block) = &encoded_block {
                insert_clob_replay_block(&tx, block)?;
            }
        }
        tx.commit()?;
        Ok(generic_events.len() as u64 + clob_events.len() as u64)
    }

    /// Upsert market.
    pub fn upsert_market(&self, window: &MarketWindow) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO markets (
                market_id, question, condition_id, slug, up_token_id, down_token_id, start_time,
                end_time, outcome, resolution_source, fee_profile, order_min_size,
                order_price_min_tick_size, maker_base_fee, taker_base_fee, rewards_min_size,
                rewards_max_spread, fees_enabled, fee_schedule_json, token_fee_rates_json,
                accepting_orders, accepting_orders_timestamp, clear_book_on_start
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                ?18, ?19, ?20, ?21, ?22, ?23
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
               rewards_max_spread = COALESCE(excluded.rewards_max_spread, markets.rewards_max_spread), \
               fees_enabled = COALESCE(excluded.fees_enabled, markets.fees_enabled), \
               fee_schedule_json = COALESCE(excluded.fee_schedule_json, markets.fee_schedule_json), \
               token_fee_rates_json = COALESCE(excluded.token_fee_rates_json, markets.token_fee_rates_json), \
               accepting_orders = COALESCE(excluded.accepting_orders, markets.accepting_orders), \
               accepting_orders_timestamp = COALESCE(excluded.accepting_orders_timestamp, markets.accepting_orders_timestamp), \
               clear_book_on_start = COALESCE(excluded.clear_book_on_start, markets.clear_book_on_start)",
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
            window.fees_enabled,
            window.fee_schedule_json,
            window.token_fee_rates_json,
            window.accepting_orders,
            window.accepting_orders_timestamp,
            window.clear_book_on_start,
        ])?;
        Ok(())
    }

    /// Return ended markets that still have unresolved open trades.
    pub fn unresolved_open_trade_markets(
        &self,
        now_ms: u64,
    ) -> anyhow::Result<Vec<PendingResolutionMarket>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT
                m.market_id,
                m.question,
                m.condition_id,
                m.slug,
                m.up_token_id,
                m.down_token_id,
                m.start_time,
                m.end_time,
                m.outcome,
                m.resolution_source,
                m.fee_profile,
                m.order_min_size,
                m.order_price_min_tick_size,
                m.maker_base_fee,
                m.taker_base_fee,
                m.rewards_min_size,
                m.rewards_max_spread,
                m.fees_enabled,
                m.fee_schedule_json,
                m.token_fee_rates_json,
                m.accepting_orders,
                m.accepting_orders_timestamp,
                m.clear_book_on_start,
                COUNT(st.id) AS open_trade_count
             FROM markets m
             JOIN simulated_trades st
               ON st.market_id = m.market_id
              AND st.status = 'open'
             LEFT JOIN trade_results tr
               ON tr.trade_id = st.id
             WHERE tr.trade_id IS NULL
               AND m.end_time <= ?1
               AND m.status != 'resolved'
             GROUP BY
                m.market_id,
                m.question,
                m.condition_id,
                m.slug,
                m.up_token_id,
                m.down_token_id,
                m.start_time,
                m.end_time,
                m.outcome,
                m.resolution_source,
                m.fee_profile,
                m.order_min_size,
                m.order_price_min_tick_size,
                m.maker_base_fee,
                m.taker_base_fee,
                m.rewards_min_size,
                m.rewards_max_spread,
                m.fees_enabled,
                m.fee_schedule_json,
                m.token_fee_rates_json,
                m.accepting_orders,
                m.accepting_orders_timestamp,
                m.clear_book_on_start
             ORDER BY m.end_time ASC, m.market_id ASC",
        )?;
        let rows = stmt.query_map(params![now_ms], |row| {
            Ok(PendingResolutionMarket {
                window: MarketWindow {
                    market_id: row.get(0)?,
                    question: row.get(1)?,
                    condition_id: row.get(2)?,
                    slug: row.get(3)?,
                    up_token_id: row.get(4)?,
                    down_token_id: row.get(5)?,
                    start_time: row.get(6)?,
                    end_time: row.get(7)?,
                    outcome: row.get(8)?,
                    resolution_source: row.get(9)?,
                    fee_profile: row.get(10)?,
                    order_min_size: row.get(11)?,
                    order_price_min_tick_size: row.get(12)?,
                    maker_base_fee: row.get(13)?,
                    taker_base_fee: row.get(14)?,
                    rewards_min_size: row.get(15)?,
                    rewards_max_spread: row.get(16)?,
                    fees_enabled: row.get(17)?,
                    fee_schedule_json: row.get(18)?,
                    token_fee_rates_json: row.get(19)?,
                    accepting_orders: row.get(20)?,
                    accepting_orders_timestamp: row.get(21)?,
                    clear_book_on_start: row.get(22)?,
                },
                open_trade_count: row.get(23)?,
            })
        })?;

        let mut markets = Vec::new();
        for row in rows {
            markets.push(row?);
        }
        Ok(markets)
    }

    /// Return every unresolved open trade together with its market end time.
    pub fn unresolved_trade_exposures(&self) -> anyhow::Result<Vec<UnresolvedTradeExposure>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT
                st.id,
                st.market_id,
                st.strategy,
                st.entry_price,
                st.size,
                m.end_time
             FROM simulated_trades st
             JOIN markets m
               ON m.market_id = st.market_id
             LEFT JOIN trade_results tr
               ON tr.trade_id = st.id
             WHERE st.status = 'open'
               AND tr.trade_id IS NULL
             ORDER BY m.end_time ASC, st.id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UnresolvedTradeExposure {
                trade_id: row.get(0)?,
                market_id: row.get(1)?,
                strategy: row.get(2)?,
                entry_price: row.get(3)?,
                size: row.get(4)?,
                market_end_time: row.get(5)?,
            })
        })?;

        let mut exposures = Vec::new();
        for row in rows {
            exposures.push(row?);
        }
        Ok(exposures)
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
        self.log_signal_with_optional_id(
            None,
            signal,
            market_id,
            execution_fidelity,
            order_submitted_at_ms,
            order_arrival_at_ms,
        )
    }

    /// Log one signal with an explicit ID when supplied by the runtime decision path.
    pub fn log_signal_with_context_and_id(
        &self,
        signal_id: i64,
        signal: &Signal,
        market_id: Option<&str>,
        execution_fidelity: Option<ReplayFidelity>,
        order_submitted_at_ms: Option<u64>,
        order_arrival_at_ms: Option<u64>,
    ) -> anyhow::Result<i64> {
        self.log_signal_with_optional_id(
            Some(signal_id),
            signal,
            market_id,
            execution_fidelity,
            order_submitted_at_ms,
            order_arrival_at_ms,
        )
    }

    /// Log one signal using either an explicit runtime ID or the database sequence.
    fn log_signal_with_optional_id(
        &self,
        signal_id: Option<i64>,
        signal: &Signal,
        market_id: Option<&str>,
        execution_fidelity: Option<ReplayFidelity>,
        order_submitted_at_ms: Option<u64>,
        order_arrival_at_ms: Option<u64>,
    ) -> anyhow::Result<i64> {
        let metadata_json = serde_json::to_string(&signal.metadata)?;
        if let Some(signal_id) = signal_id {
            let mut stmt = self.conn.prepare_cached(
                "INSERT OR REPLACE INTO signals (
                    id, timestamp, strategy, direction, binance_price, chainlink_price,
                    up_ask, down_ask, up_bid, down_bid, metadata, market_id, execution_fidelity,
                    strategy_version, feature_mode, order_submitted_at_ms, order_arrival_at_ms
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                    ?14, ?15, ?16, ?17
                 )",
            )?;
            stmt.execute(params![
                signal_id,
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
                signal.strategy_version,
                signal.feature_mode,
                order_submitted_at_ms,
                order_arrival_at_ms,
            ])?;
            return Ok(signal_id);
        }
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO signals (
                timestamp, strategy, direction, binance_price, chainlink_price,
                up_ask, down_ask, up_bid, down_bid, metadata, market_id, execution_fidelity,
                strategy_version, feature_mode, order_submitted_at_ms, order_arrival_at_ms
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16
             )",
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
            signal.strategy_version,
            signal.feature_mode,
            order_submitted_at_ms,
            order_arrival_at_ms,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Persist or update per-signal telemetry and feature values.
    pub fn upsert_signal_metrics(&self, record: &SignalMetricRecord) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO signal_metrics (
                signal_id, generated_at_ms, generated_at_us, order_submitted_at_ms,
                order_submitted_at_us, expected_arrival_at_ms, expected_arrival_at_us,
                order_processed_at_ms, order_processed_at_us, effective_arrival_delay_ms,
                binance_age_ms, chainlink_age_ms, clob_age_ms, quote_age_ms,
                book_staleness_ms, expected_fee, expected_slippage, expected_edge,
                available_feature_count, decision_status, rejection_reason, features_json
             ) VALUES (
                ?1, ?2, ?3, ?4,
                ?5, ?6, ?7, ?8,
                ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16,
                ?17, ?18, ?19, ?20,
                ?21, ?22
             )
             ON CONFLICT(signal_id) DO UPDATE SET
                generated_at_ms = excluded.generated_at_ms,
                generated_at_us = excluded.generated_at_us,
                order_submitted_at_ms = excluded.order_submitted_at_ms,
                order_submitted_at_us = excluded.order_submitted_at_us,
                expected_arrival_at_ms = excluded.expected_arrival_at_ms,
                expected_arrival_at_us = excluded.expected_arrival_at_us,
                order_processed_at_ms = excluded.order_processed_at_ms,
                order_processed_at_us = excluded.order_processed_at_us,
                effective_arrival_delay_ms = excluded.effective_arrival_delay_ms,
                binance_age_ms = excluded.binance_age_ms,
                chainlink_age_ms = excluded.chainlink_age_ms,
                clob_age_ms = excluded.clob_age_ms,
                quote_age_ms = excluded.quote_age_ms,
                book_staleness_ms = excluded.book_staleness_ms,
                expected_fee = excluded.expected_fee,
                expected_slippage = excluded.expected_slippage,
                expected_edge = excluded.expected_edge,
                available_feature_count = excluded.available_feature_count,
                decision_status = excluded.decision_status,
                rejection_reason = excluded.rejection_reason,
                features_json = excluded.features_json",
        )?;
        stmt.execute(params![
            record.signal_id,
            record.generated_at_ms,
            record.generated_at_us,
            record.order_submitted_at_ms,
            record.order_submitted_at_us,
            record.expected_arrival_at_ms,
            record.expected_arrival_at_us,
            record.order_processed_at_ms,
            record.order_processed_at_us,
            record.effective_arrival_delay_ms,
            record.binance_age_ms,
            record.chainlink_age_ms,
            record.clob_age_ms,
            record.quote_age_ms,
            record.book_staleness_ms,
            record.expected_fee,
            record.expected_slippage,
            record.expected_edge,
            record.available_feature_count,
            record.decision_status,
            record.rejection_reason,
            record.features_json,
        ])?;
        Ok(())
    }

    /// Persist one signal telemetry snapshot if the signal carries telemetry fields.
    pub fn upsert_signal_telemetry(
        &self,
        signal_id: i64,
        telemetry: &SignalTelemetry,
        order_submitted_at_ms: Option<u64>,
        expected_arrival_at_ms: Option<u64>,
        decision_status: Option<&str>,
        rejection_reason: Option<&str>,
    ) -> anyhow::Result<()> {
        let features_json = serde_json::to_string(&telemetry.features_json)?;
        let record = SignalMetricRecord {
            signal_id,
            generated_at_ms: telemetry.generated_at_ms,
            generated_at_us: telemetry.generated_at_us,
            order_submitted_at_ms: order_submitted_at_ms.or(telemetry.order_submitted_at_ms),
            order_submitted_at_us: resolve_related_us(
                telemetry.generated_at_ms,
                telemetry.generated_at_us,
                order_submitted_at_ms.or(telemetry.order_submitted_at_ms),
                telemetry.order_submitted_at_us,
            ),
            expected_arrival_at_ms: expected_arrival_at_ms.or(telemetry.expected_arrival_at_ms),
            expected_arrival_at_us: resolve_related_us(
                telemetry.generated_at_ms,
                telemetry.generated_at_us,
                expected_arrival_at_ms.or(telemetry.expected_arrival_at_ms),
                telemetry.expected_arrival_at_us,
            ),
            order_processed_at_ms: telemetry.order_processed_at_ms,
            order_processed_at_us: telemetry.order_processed_at_us,
            effective_arrival_delay_ms: telemetry.effective_arrival_delay_ms,
            binance_age_ms: telemetry.binance_age_ms,
            chainlink_age_ms: telemetry.chainlink_age_ms,
            clob_age_ms: telemetry.clob_age_ms,
            quote_age_ms: telemetry.quote_age_ms,
            book_staleness_ms: telemetry.book_staleness_ms,
            expected_fee: telemetry.expected_fee,
            expected_slippage: telemetry.expected_slippage,
            expected_edge: telemetry.expected_edge,
            available_feature_count: telemetry.available_feature_count,
            decision_status: decision_status
                .unwrap_or(&telemetry.decision_status)
                .to_string(),
            rejection_reason: rejection_reason
                .map(str::to_string)
                .or_else(|| telemetry.rejection_reason.clone()),
            features_json,
        };
        self.upsert_signal_metrics(&record)
    }

    /// Update one queued order's final execution outcome after arrival processing.
    pub fn update_signal_execution_outcome(
        &self,
        signal_id: i64,
        processed_at_ms: u64,
        processed_at_micros: Option<u64>,
        effective_arrival_delay_ms: u64,
        decision_status: &str,
        rejection_reason: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE signal_metrics
             SET order_processed_at_ms = ?2,
                 order_processed_at_us = ?3,
                 effective_arrival_delay_ms = ?4,
                 decision_status = ?5,
                 rejection_reason = ?6
             WHERE signal_id = ?1",
        )?;
        let updated = stmt.execute(params![
            signal_id,
            processed_at_ms,
            processed_at_micros,
            effective_arrival_delay_ms,
            decision_status,
            rejection_reason,
        ])?;
        if updated != 1 {
            anyhow::bail!(
                "expected exactly one signal_metrics row for signal_id={signal_id}, updated {updated}"
            );
        }
        Ok(())
    }

    /// Persist one feed-health lifecycle or anomaly event.
    pub fn log_feed_health_event(&self, event: &FeedHealthEvent) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO feed_health_events (
                timestamp_ms, timestamp_us, source, event_type, connection_id, market_id,
                details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        stmt.execute(params![
            event.timestamp_ms,
            event.timestamp_us,
            event.source,
            event.event_type,
            event.connection_id,
            event.market_id,
            event.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Persists one aggregated strategy-rejection summary row.
    pub fn log_strategy_rejection_summary(
        &self,
        record: &StrategyRejectionSummaryRecord,
    ) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO strategy_rejection_summaries (
                timestamp_ms, market_id, strategy, reason, count, details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        stmt.execute(params![
            record.timestamp_ms,
            record.market_id,
            record.strategy,
            record.reason,
            record.count,
            record.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Returns the earliest persisted Binance price within one market window.
    pub fn earliest_binance_price_in_window(
        &self,
        start_time_ms: u64,
        end_time_ms: u64,
    ) -> anyhow::Result<Option<f64>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT price
             FROM tick_data
             WHERE source = 'binance'
               AND timestamp >= ?1
               AND timestamp < ?2
               AND price IS NOT NULL
             ORDER BY timestamp ASC, id ASC
             LIMIT 1",
        )?;
        let price = stmt
            .query_row(params![start_time_ms, end_time_ms], |row| row.get(0))
            .optional()?;
        Ok(price)
    }

    /// Summarize submitted and processed execution outcomes for one market window.
    pub fn execution_rollup_for_market(
        &self,
        market_id: &str,
    ) -> anyhow::Result<ExecutionStatusRollup> {
        let (submitted, filled, missed, rejected_before_queue, mean_effective_arrival_delay_ms): (
            u64,
            u64,
            u64,
            u64,
            Option<f64>,
        ) = self.conn.query_row(
            "SELECT
                SUM(CASE WHEN sm.decision_status IN ('submitted', 'filled', 'missed') THEN 1 ELSE 0 END),
                SUM(CASE WHEN sm.decision_status = 'filled' THEN 1 ELSE 0 END),
                SUM(CASE WHEN sm.decision_status = 'missed' THEN 1 ELSE 0 END),
                SUM(CASE WHEN sm.decision_status = 'rejected' THEN 1 ELSE 0 END),
                AVG(sm.effective_arrival_delay_ms)
             FROM signal_metrics sm
             JOIN signals s ON s.id = sm.signal_id
             WHERE s.market_id = ?1",
            params![market_id],
            |row| {
                Ok((
                    row.get::<_, Option<u64>>(0)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(2)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(3)?.unwrap_or(0),
                    row.get(4)?,
                ))
            },
        )?;

        let partial: u64 = self.conn.query_row(
            "SELECT COUNT(*)
             FROM simulated_trades
             WHERE market_id = ?1
               AND fill_status = 'partial'",
            params![market_id],
            |row| row.get(0),
        )?;

        let mut stmt = self.conn.prepare_cached(
            "SELECT rejection_reason, COUNT(*)
             FROM signal_metrics sm
             JOIN signals s ON s.id = sm.signal_id
             WHERE s.market_id = ?1
               AND sm.decision_status = 'missed'
               AND sm.rejection_reason IS NOT NULL
             GROUP BY rejection_reason
             ORDER BY COUNT(*) DESC, rejection_reason ASC
             LIMIT 3",
        )?;
        let miss_reasons = stmt
            .query_map(params![market_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<(String, u64)>>>()?;

        let mut stmt = self.conn.prepare_cached(
            "SELECT rejection_reason, COUNT(*)
             FROM signal_metrics sm
             JOIN signals s ON s.id = sm.signal_id
             WHERE s.market_id = ?1
               AND sm.decision_status = 'rejected'
               AND sm.rejection_reason IS NOT NULL
             GROUP BY rejection_reason
             ORDER BY COUNT(*) DESC, rejection_reason ASC
             LIMIT 3",
        )?;
        let queue_rejection_reasons = stmt
            .query_map(params![market_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<(String, u64)>>>()?;

        Ok(ExecutionStatusRollup {
            submitted,
            filled,
            missed,
            rejected_before_queue,
            partial,
            mean_effective_arrival_delay_ms,
            miss_reasons,
            queue_rejection_reasons,
        })
    }

    /// Insert a new open trade and return the auto-generated row ID.
    pub fn open_trade(&self, trade: &SimulatedTrade) -> anyhow::Result<i64> {
        if let Some(trade_id) = trade.id {
            return self.open_trade_with_id(trade_id, trade);
        }
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

    /// Insert a new open trade with a caller-supplied ID.
    fn open_trade_with_id(&self, trade_id: i64, trade: &SimulatedTrade) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT OR REPLACE INTO simulated_trades (
                id, timestamp, market_id, strategy, side, token_id, entry_price, size, status,
                signal_id, execution_mode, order_id, fill_price, requested_price, requested_size,
                filled_size, avg_fill_price, fill_status, fill_reason, fill_latency_ms,
                execution_group_id, execution_fidelity
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19, ?20,
                ?21, ?22
             )",
        )?;
        stmt.execute(params![
            trade_id,
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
        Ok(trade_id)
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

    /// Return every open trade with its market end time for runtime bootstrapping.
    pub fn open_trade_snapshots(&self) -> anyhow::Result<Vec<OpenTradeSnapshot>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT st.id, st.timestamp, st.market_id, st.strategy, st.side, st.token_id,
                    st.entry_price, st.size, st.status, st.signal_id, st.requested_price,
                    st.requested_size, st.filled_size, st.avg_fill_price, st.fill_status,
                    st.fill_reason, st.fill_latency_ms, st.execution_group_id,
                    st.execution_fidelity, st.execution_mode, st.order_id, st.fill_price,
                    m.end_time
             FROM simulated_trades st
             JOIN markets m
               ON m.market_id = st.market_id
             WHERE st.status = 'open'
             ORDER BY st.id ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            let side_str: String = row.get(4)?;
            let status_str: String = row.get(8)?;
            Ok(OpenTradeSnapshot {
                trade: SimulatedTrade {
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
                },
                market_end_time: row.get(22)?,
            })
        })?;

        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    /// Count open trades.
    pub fn count_open_trades(&self) -> anyhow::Result<u64> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT COUNT(*) FROM simulated_trades WHERE status = 'open'")?;
        let count = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    /// Count open trades whose market has not closed yet.
    pub fn count_active_open_trades(&self, now_ms: u64) -> anyhow::Result<u64> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT COUNT(*)
             FROM simulated_trades st
             JOIN markets m
               ON m.market_id = st.market_id
             LEFT JOIN trade_results tr
               ON tr.trade_id = st.id
             WHERE st.status = 'open'
               AND tr.trade_id IS NULL
               AND m.end_time > ?1",
        )?;
        let count = stmt.query_row(params![now_ms], |row| row.get(0))?;
        Ok(count)
    }

    /// Count open trades whose market has already closed but not settled yet.
    pub fn count_pending_settlement_open_trades(&self, now_ms: u64) -> anyhow::Result<u64> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT COUNT(*)
             FROM simulated_trades st
             JOIN markets m
               ON m.market_id = st.market_id
             LEFT JOIN trade_results tr
               ON tr.trade_id = st.id
             WHERE st.status = 'open'
               AND tr.trade_id IS NULL
               AND m.end_time <= ?1",
        )?;
        let count = stmt.query_row(params![now_ms], |row| row.get(0))?;
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

    /// Insert one live trading session and return its row ID.
    pub fn insert_live_session(&self, session: &LiveSession) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_sessions (
                started_at_ms, ended_at_ms, status, execution_mode, wallet_address,
                proxy_wallet, enabled_strategies_json, config_fingerprint, cash_cap_usd,
                details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        stmt.execute(params![
            session.started_at_ms,
            session.ended_at_ms,
            session.status,
            session.execution_mode,
            session.wallet_address,
            session.proxy_wallet,
            session.enabled_strategies_json,
            session.config_fingerprint,
            session.cash_cap_usd,
            session.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Mark one live trading session as finished.
    pub fn finish_live_session(
        &self,
        session_id: i64,
        ended_at_ms: u64,
        status: &str,
        details_json: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE live_sessions
             SET ended_at_ms = ?1,
                 status = ?2,
                 details_json = COALESCE(?3, details_json)
             WHERE id = ?4",
        )?;
        stmt.execute(params![ended_at_ms, status, details_json, session_id])?;
        Ok(())
    }

    /// Update one live trading session without marking it finished.
    pub fn update_live_session_metadata(
        &self,
        session_id: i64,
        status: &str,
        wallet_address: Option<&str>,
        proxy_wallet: Option<&str>,
        details_json: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE live_sessions
             SET status = ?1,
                 wallet_address = COALESCE(?2, wallet_address),
                 proxy_wallet = COALESCE(?3, proxy_wallet),
                 details_json = ?4
             WHERE id = ?5",
        )?;
        stmt.execute(params![
            status,
            wallet_address,
            proxy_wallet,
            details_json,
            session_id,
        ])?;
        Ok(())
    }

    /// Insert one live order intent and return its row ID.
    pub fn log_live_order_intent(&self, intent: &LiveOrderIntent) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_order_intents (
                session_id, signal_id, market_id, strategy, side, order_type, status,
                created_at_ms, requested_price, requested_size, limit_price,
                fee_schedule_json, token_fee_rates_json, execution_group_id, details_json
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
             )",
        )?;
        stmt.execute(params![
            intent.session_id,
            intent.signal_id,
            intent.market_id,
            intent.strategy,
            intent.side,
            intent.order_type,
            intent.status,
            intent.created_at_ms,
            intent.requested_price,
            intent.requested_size,
            intent.limit_price,
            intent.fee_schedule_json,
            intent.token_fee_rates_json,
            intent.execution_group_id,
            intent.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Atomically persist critical decision evidence and its live order intent.
    pub fn log_live_order_intent_with_decision_evidence(
        &self,
        evidence: &LiveDecisionEvidence<'_>,
        intent: &LiveOrderIntent,
    ) -> anyhow::Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        insert_live_decision_signal_on_tx(&tx, evidence)?;
        if let Some(record) = signal_metric_record_from_evidence(evidence)? {
            upsert_signal_metrics_on_tx(&tx, &record)?;
        }
        insert_live_order_intent_on_tx(&tx, intent)?;
        let intent_id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(intent_id)
    }

    /// Insert one live venue order and return its row ID.
    pub fn log_live_order(&self, order: &LiveOrder) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_orders (
                session_id, intent_id, venue_order_id, client_order_id, market_id, token_id,
                side, order_type, status, status_reason, created_at_ms, acknowledged_at_ms,
                updated_at_ms, requested_price, limit_price, requested_size, accepted_size,
                details_json
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18
             )",
        )?;
        stmt.execute(params![
            order.session_id,
            order.intent_id,
            order.venue_order_id,
            order.client_order_id,
            order.market_id,
            order.token_id,
            order.side,
            order.order_type,
            order.status,
            order.status_reason,
            order.created_at_ms,
            order.acknowledged_at_ms,
            order.updated_at_ms,
            order.requested_price,
            order.limit_price,
            order.requested_size,
            order.accepted_size,
            order.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Update the mutable status fields for one live venue order.
    pub fn update_live_order_status(
        &self,
        live_order_id: i64,
        update: LiveOrderStatusUpdate<'_>,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE live_orders
             SET status = ?1,
                 status_reason = ?2,
                 acknowledged_at_ms = COALESCE(?3, acknowledged_at_ms),
                 updated_at_ms = ?4,
                 accepted_size = COALESCE(?5, accepted_size),
                 details_json = COALESCE(?6, details_json)
             WHERE id = ?7",
        )?;
        stmt.execute(params![
            update.status,
            update.status_reason,
            update.acknowledged_at_ms,
            update.updated_at_ms,
            update.accepted_size,
            update.details_json,
            live_order_id,
        ])?;
        Ok(())
    }

    /// Insert one live fill and return its row ID.
    pub fn log_live_fill(&self, fill: &LiveFill) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_fills (
                session_id, intent_id, live_order_id, venue_trade_id, filled_at_ms, price,
                size, fee_amount, fee_rate, liquidity_side, tx_hash, status, details_json
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11, ?12, ?13
             )",
        )?;
        stmt.execute(params![
            fill.session_id,
            fill.intent_id,
            fill.live_order_id,
            fill.venue_trade_id,
            fill.filled_at_ms,
            fill.price,
            fill.size,
            fill.fee_amount,
            fill.fee_rate,
            fill.liquidity_side,
            fill.tx_hash,
            fill.status,
            fill.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return whether one recovered venue trade was already persisted.
    pub fn live_fill_exists(&self, session_id: i64, venue_trade_id: &str) -> anyhow::Result<bool> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT 1
             FROM live_fills
             WHERE session_id = ?1 AND venue_trade_id = ?2
             LIMIT 1",
        )?;
        let exists = stmt
            .query_row(params![session_id, venue_trade_id], |_| Ok(()))
            .optional()?
            .is_some();
        Ok(exists)
    }

    /// Insert one live account snapshot and return its row ID.
    pub fn log_live_account_snapshot(&self, snapshot: &LiveAccountSnapshot) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_account_snapshots (
                session_id, timestamp_ms, cash_available, cash_reserved_for_orders,
                inventory_mark_value, redeemable_value, pending_redeem_value, total_equity,
                allowance_available, details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        stmt.execute(params![
            snapshot.session_id,
            snapshot.timestamp_ms,
            snapshot.cash_available,
            snapshot.cash_reserved_for_orders,
            snapshot.inventory_mark_value,
            snapshot.redeemable_value,
            snapshot.pending_redeem_value,
            snapshot.total_equity,
            snapshot.allowance_available,
            snapshot.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert one live redemption row and return its row ID.
    pub fn log_live_redemption(&self, redemption: &LiveRedemption) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_redemptions (
                session_id, market_id, detected_redeemable_at_ms, submitted_at_ms,
                confirmed_at_ms, cash_credit_observed_at_ms, status, redeemable_value, tx_hash,
                details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        stmt.execute(params![
            redemption.session_id,
            redemption.market_id,
            redemption.detected_redeemable_at_ms,
            redemption.submitted_at_ms,
            redemption.confirmed_at_ms,
            redemption.cash_credit_observed_at_ms,
            redemption.status,
            redemption.redeemable_value,
            redemption.tx_hash,
            redemption.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Update one live redemption lifecycle row in place.
    pub fn update_live_redemption_status(
        &self,
        redemption_id: i64,
        update: LiveRedemptionStatusUpdate<'_>,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE live_redemptions
             SET status = ?1,
                 submitted_at_ms = COALESCE(?2, submitted_at_ms),
                 confirmed_at_ms = COALESCE(?3, confirmed_at_ms),
                 cash_credit_observed_at_ms = COALESCE(?4, cash_credit_observed_at_ms),
                 tx_hash = COALESCE(?5, tx_hash),
                 details_json = COALESCE(?6, details_json)
             WHERE id = ?7",
        )?;
        stmt.execute(params![
            update.status,
            update.submitted_at_ms,
            update.confirmed_at_ms,
            update.cash_credit_observed_at_ms,
            update.tx_hash,
            update.details_json,
            redemption_id,
        ])?;
        Ok(())
    }

    /// Insert one live reconciliation event and return its row ID.
    pub fn log_live_reconciliation_event(
        &self,
        event: &LiveReconciliationEvent,
    ) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_reconciliation_events (
                session_id, timestamp_ms, severity, event_type, local_value, remote_value,
                details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        stmt.execute(params![
            event.session_id,
            event.timestamp_ms,
            event.severity,
            event.event_type,
            event.local_value,
            event.remote_value,
            event.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert one control-plane audit event and return its row ID.
    pub fn log_control_audit(&self, entry: &ControlAuditEntry) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO control_audit (timestamp_ms, actor, action, target, details_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        stmt.execute(params![
            entry.timestamp_ms,
            entry.actor,
            entry.action,
            entry.target,
            entry.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return the latest unfinished live session, if one exists.
    pub fn latest_active_live_session(&self) -> anyhow::Result<Option<LiveSession>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, started_at_ms, ended_at_ms, status, execution_mode, wallet_address,
                    proxy_wallet, enabled_strategies_json, config_fingerprint, cash_cap_usd,
                    details_json
             FROM live_sessions
             WHERE ended_at_ms IS NULL
             ORDER BY started_at_ms DESC, id DESC
             LIMIT 1",
        )?;
        stmt.query_row([], |row| {
            Ok(LiveSession {
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
        })
        .optional()
        .context("querying latest active live session")
    }

    /// Insert one durable live control-state transition.
    pub fn insert_live_control_state(&self, state: &LiveControlState) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_control_state (
                session_id, state, updated_at_ms, actor, reason, details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        stmt.execute(params![
            state.session_id,
            state.state,
            state.updated_at_ms,
            state.actor,
            state.reason,
            state.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return the latest live control state for one session, if any.
    pub fn latest_live_control_state(
        &self,
        session_id: i64,
    ) -> anyhow::Result<Option<LiveControlState>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, session_id, state, updated_at_ms, actor, reason, details_json
             FROM live_control_state
             WHERE session_id = ?1
             ORDER BY updated_at_ms DESC, id DESC
             LIMIT 1",
        )?;
        stmt.query_row(params![session_id], |row| {
            Ok(LiveControlState {
                id: row.get(0)?,
                session_id: row.get(1)?,
                state: row.get(2)?,
                updated_at_ms: row.get(3)?,
                actor: row.get(4)?,
                reason: row.get(5)?,
                details_json: row.get(6)?,
            })
        })
        .optional()
        .context("querying latest live control state")
    }

    /// Return whether this DB contains a terminal live-trading state.
    pub fn terminal_live_trading_halt_exists(&self) -> anyhow::Result<bool> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT EXISTS(
                SELECT 1
                FROM live_sessions s
                WHERE s.execution_mode = 'live_trading'
                  AND (
                    s.status IN ('halted', 'unknown_order')
                    OR EXISTS (
                        SELECT 1
                        FROM live_control_state st
                        WHERE st.session_id = s.id
                          AND st.state IN ('halted', 'unknown_order')
                    )
                  )
             )",
        )?;
        let exists: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(exists != 0)
    }

    /// Count critical reconciliation events for one live session.
    pub fn critical_live_reconciliation_count(&self, session_id: i64) -> anyhow::Result<u64> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT COUNT(*)
             FROM live_reconciliation_events
             WHERE session_id = ?1
               AND severity = 'critical'
               AND event_type != 'user_stream_unhealthy'",
        )?;
        stmt.query_row(params![session_id], |row| row.get(0))
            .context("counting critical live reconciliation events")
    }

    /// Return the latest live-trading session, finished or unfinished.
    pub fn latest_live_trading_session(&self) -> anyhow::Result<Option<LiveSession>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, started_at_ms, ended_at_ms, status, execution_mode, wallet_address,
                    proxy_wallet, enabled_strategies_json, config_fingerprint, cash_cap_usd,
                    details_json
             FROM live_sessions
             WHERE execution_mode = 'live_trading'
             ORDER BY started_at_ms DESC, id DESC
             LIMIT 1",
        )?;
        stmt.query_row([], |row| {
            Ok(LiveSession {
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
        })
        .optional()
        .context("querying latest live-trading session")
    }

    /// Insert one operator live-control command and return its row ID.
    pub fn insert_live_control_command(&self, command: &LiveControlCommand) -> anyhow::Result<i64> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO live_control_commands (
                session_id, action, actor, reason, requested_at_ms, applied_at_ms, status,
                details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        stmt.execute(params![
            command.session_id,
            command.action,
            command.actor,
            command.reason,
            command.requested_at_ms,
            command.applied_at_ms,
            command.status,
            command.details_json,
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return pending live-control commands for one session in request order.
    pub fn pending_live_control_commands(
        &self,
        session_id: i64,
    ) -> anyhow::Result<Vec<LiveControlCommand>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, session_id, action, actor, reason, requested_at_ms, applied_at_ms,
                    status, details_json
             FROM live_control_commands
             WHERE session_id = ?1 AND status = 'pending'
             ORDER BY requested_at_ms, id",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(LiveControlCommand {
                id: row.get(0)?,
                session_id: row.get(1)?,
                action: row.get(2)?,
                actor: row.get(3)?,
                reason: row.get(4)?,
                requested_at_ms: row.get(5)?,
                applied_at_ms: row.get(6)?,
                status: row.get(7)?,
                details_json: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .context("querying pending live control commands")
    }

    /// Mark one live-control command as applied or rejected.
    pub fn update_live_control_command_status(
        &self,
        command_id: i64,
        applied_at_ms: u64,
        status: &str,
        details_json: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE live_control_commands
             SET applied_at_ms = ?1,
                 status = ?2,
                 details_json = COALESCE(?3, details_json)
             WHERE id = ?4",
        )?;
        stmt.execute(params![applied_at_ms, status, details_json, command_id])?;
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

    /// Returns the initial balance recorded for the current run, if any.
    pub fn get_initial_balance(&self) -> anyhow::Result<Option<f64>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT balance FROM balance_log WHERE event = 'init' ORDER BY id LIMIT 1",
        )?;
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

    /// Upsert one run metadata value.
    pub fn set_run_metadata(
        &self,
        key: &str,
        value: &str,
        recorded_at_ms: u64,
    ) -> anyhow::Result<()> {
        let recorded_at_ms = i64::try_from(recorded_at_ms)
            .with_context(|| format!("run metadata timestamp exceeds i64: {recorded_at_ms}"))?;
        self.conn.execute(
            "INSERT INTO run_metadata (key, value, recorded_at_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                recorded_at_ms = excluded.recorded_at_ms",
            params![key, value, recorded_at_ms],
        )?;
        Ok(())
    }

    /// Return one run metadata value.
    pub fn get_run_metadata(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM run_metadata WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .with_context(|| format!("reading run metadata: {key}"))
    }

    /// Summarize the current on-disk footprint of the live database.
    pub fn storage_footprint(&self) -> anyhow::Result<DbFootprint> {
        let db_bytes = file_size_at(&self.db_path)?;
        let wal_bytes = file_size_at(&format!("{}-wal", self.db_path))?;
        let feed_event_count: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))
            .context("counting feed_events")?;
        let mut stmt = self.conn.prepare(
            "SELECT
                source,
                event_type,
                COUNT(*) AS row_count,
                SUM(
                    LENGTH(COALESCE(payload_json, '')) +
                    LENGTH(COALESCE(details_json, '')) +
                    LENGTH(COALESCE(source_topic, '')) +
                    LENGTH(COALESCE(source_symbol, '')) +
                    LENGTH(COALESCE(connection_id, '')) +
                    LENGTH(COALESCE(sequence_key, '')) +
                    LENGTH(COALESCE(market_id, '')) +
                    LENGTH(COALESCE(asset_id, ''))
                ) AS approx_payload_bytes
             FROM feed_events
             GROUP BY source, event_type
             ORDER BY row_count DESC, source ASC, event_type ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FeedEventFootprintRow {
                source: row.get(0)?,
                event_type: row.get(1)?,
                row_count: row.get(2)?,
                approx_payload_bytes: row.get(3)?,
            })
        })?;
        let mut grouped_feed_events = Vec::new();
        for row in rows {
            grouped_feed_events.push(row?);
        }
        Ok(DbFootprint {
            db_bytes,
            wal_bytes,
            feed_event_count,
            grouped_feed_events,
        })
    }

    /// Returns the underlying `SQLite` connection for read-only helper queries.
    pub fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }
}

/// Insert a critical live decision signal inside an existing transaction.
fn insert_live_decision_signal_on_tx(
    tx: &rusqlite::Transaction<'_>,
    evidence: &LiveDecisionEvidence<'_>,
) -> anyhow::Result<()> {
    let metadata_json = serde_json::to_string(&evidence.signal.metadata)?;
    tx.prepare_cached(
        "INSERT OR REPLACE INTO signals (
            id, timestamp, strategy, direction, binance_price, chainlink_price,
            up_ask, down_ask, up_bid, down_bid, metadata, market_id, execution_fidelity,
            strategy_version, feature_mode, order_submitted_at_ms, order_arrival_at_ms
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17
         )",
    )?
    .execute(params![
        evidence.signal_id,
        evidence.signal.timestamp,
        evidence.signal.strategy,
        evidence.signal.direction.to_string(),
        evidence.signal.binance_price,
        evidence.signal.chainlink_price,
        evidence.signal.up_ask,
        evidence.signal.down_ask,
        evidence.signal.up_bid,
        evidence.signal.down_bid,
        metadata_json,
        evidence.market_id,
        evidence.execution_fidelity.to_string(),
        evidence.signal.strategy_version,
        evidence.signal.feature_mode,
        evidence.order_submitted_at_ms,
        evidence.expected_arrival_at_ms,
    ])?;
    Ok(())
}

/// Build a critical signal metric row from live decision evidence.
fn signal_metric_record_from_evidence(
    evidence: &LiveDecisionEvidence<'_>,
) -> anyhow::Result<Option<SignalMetricRecord>> {
    let Some(telemetry) = evidence.signal.telemetry.as_ref() else {
        return Ok(None);
    };
    let features_json = serde_json::to_string(&telemetry.features_json)?;
    Ok(Some(SignalMetricRecord {
        signal_id: evidence.signal_id,
        generated_at_ms: telemetry.generated_at_ms,
        generated_at_us: telemetry.generated_at_us,
        order_submitted_at_ms: evidence
            .order_submitted_at_ms
            .or(telemetry.order_submitted_at_ms),
        order_submitted_at_us: resolve_related_us(
            telemetry.generated_at_ms,
            telemetry.generated_at_us,
            evidence
                .order_submitted_at_ms
                .or(telemetry.order_submitted_at_ms),
            telemetry.order_submitted_at_us,
        ),
        expected_arrival_at_ms: evidence
            .expected_arrival_at_ms
            .or(telemetry.expected_arrival_at_ms),
        expected_arrival_at_us: resolve_related_us(
            telemetry.generated_at_ms,
            telemetry.generated_at_us,
            evidence
                .expected_arrival_at_ms
                .or(telemetry.expected_arrival_at_ms),
            telemetry.expected_arrival_at_us,
        ),
        order_processed_at_ms: telemetry.order_processed_at_ms,
        order_processed_at_us: telemetry.order_processed_at_us,
        effective_arrival_delay_ms: telemetry.effective_arrival_delay_ms,
        binance_age_ms: telemetry.binance_age_ms,
        chainlink_age_ms: telemetry.chainlink_age_ms,
        clob_age_ms: telemetry.clob_age_ms,
        quote_age_ms: telemetry.quote_age_ms,
        book_staleness_ms: telemetry.book_staleness_ms,
        expected_fee: telemetry.expected_fee,
        expected_slippage: telemetry.expected_slippage,
        expected_edge: telemetry.expected_edge,
        available_feature_count: telemetry.available_feature_count,
        decision_status: evidence.decision_status.to_string(),
        rejection_reason: evidence
            .rejection_reason
            .map(str::to_string)
            .or_else(|| telemetry.rejection_reason.clone()),
        features_json,
    }))
}

/// Upsert one signal metric row inside an existing transaction.
fn upsert_signal_metrics_on_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &SignalMetricRecord,
) -> anyhow::Result<()> {
    tx.prepare_cached(
        "INSERT INTO signal_metrics (
            signal_id, generated_at_ms, generated_at_us, order_submitted_at_ms,
            order_submitted_at_us, expected_arrival_at_ms, expected_arrival_at_us,
            order_processed_at_ms, order_processed_at_us, effective_arrival_delay_ms,
            binance_age_ms, chainlink_age_ms, clob_age_ms, quote_age_ms,
            book_staleness_ms, expected_fee, expected_slippage, expected_edge,
            available_feature_count, decision_status, rejection_reason, features_json
         ) VALUES (
            ?1, ?2, ?3, ?4,
            ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16,
            ?17, ?18, ?19, ?20,
            ?21, ?22
         )
         ON CONFLICT(signal_id) DO UPDATE SET
            generated_at_ms = excluded.generated_at_ms,
            generated_at_us = excluded.generated_at_us,
            order_submitted_at_ms = excluded.order_submitted_at_ms,
            order_submitted_at_us = excluded.order_submitted_at_us,
            expected_arrival_at_ms = excluded.expected_arrival_at_ms,
            expected_arrival_at_us = excluded.expected_arrival_at_us,
            order_processed_at_ms = excluded.order_processed_at_ms,
            order_processed_at_us = excluded.order_processed_at_us,
            effective_arrival_delay_ms = excluded.effective_arrival_delay_ms,
            binance_age_ms = excluded.binance_age_ms,
            chainlink_age_ms = excluded.chainlink_age_ms,
            clob_age_ms = excluded.clob_age_ms,
            quote_age_ms = excluded.quote_age_ms,
            book_staleness_ms = excluded.book_staleness_ms,
            expected_fee = excluded.expected_fee,
            expected_slippage = excluded.expected_slippage,
            expected_edge = excluded.expected_edge,
            available_feature_count = excluded.available_feature_count,
            decision_status = excluded.decision_status,
            rejection_reason = excluded.rejection_reason,
            features_json = excluded.features_json",
    )?
    .execute(params![
        record.signal_id,
        record.generated_at_ms,
        record.generated_at_us,
        record.order_submitted_at_ms,
        record.order_submitted_at_us,
        record.expected_arrival_at_ms,
        record.expected_arrival_at_us,
        record.order_processed_at_ms,
        record.order_processed_at_us,
        record.effective_arrival_delay_ms,
        record.binance_age_ms,
        record.chainlink_age_ms,
        record.clob_age_ms,
        record.quote_age_ms,
        record.book_staleness_ms,
        record.expected_fee,
        record.expected_slippage,
        record.expected_edge,
        record.available_feature_count,
        record.decision_status,
        record.rejection_reason,
        record.features_json,
    ])?;
    Ok(())
}

/// Insert one live order intent inside an existing transaction.
fn insert_live_order_intent_on_tx(
    tx: &rusqlite::Transaction<'_>,
    intent: &LiveOrderIntent,
) -> anyhow::Result<()> {
    tx.prepare_cached(
        "INSERT INTO live_order_intents (
            session_id, signal_id, market_id, strategy, side, order_type, status,
            created_at_ms, requested_price, requested_size, limit_price,
            fee_schedule_json, token_fee_rates_json, execution_group_id, details_json
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
         )",
    )?
    .execute(params![
        intent.session_id,
        intent.signal_id,
        intent.market_id,
        intent.strategy,
        intent.side,
        intent.order_type,
        intent.status,
        intent.created_at_ms,
        intent.requested_price,
        intent.requested_size,
        intent.limit_price,
        intent.fee_schedule_json,
        intent.token_fee_rates_json,
        intent.execution_group_id,
        intent.details_json,
    ])?;
    Ok(())
}

/// Derive a related microsecond timestamp from a millisecond delta when possible.
fn resolve_related_us(
    base_timestamp_ms: u64,
    base_epoch_us: Option<u64>,
    target_ms: Option<u64>,
    explicit_us: Option<u64>,
) -> Option<u64> {
    explicit_us.or_else(|| {
        let base_epoch_us = base_epoch_us?;
        let target_ms = target_ms?;
        let delta_ms = target_ms.saturating_sub(base_timestamp_ms);
        Some(base_epoch_us.saturating_add(delta_ms.saturating_mul(1_000)))
    })
}

/// Read one file's size in bytes, returning zero when it does not exist.
fn file_size_at(path: &str) -> anyhow::Result<u64> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error).with_context(|| format!("reading file size for {path}")),
    }
}

/// Apply runtime `SQLite` connection settings without changing schema.
fn apply_runtime_pragmas(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "wal_autocheckpoint", 10_000_i64)?;
    conn.pragma_update(None, "journal_size_limit", 64_i64 * 1024 * 1024)?;
    Ok(())
}

/// Print a compact storage report for one database path.
pub fn print_db_footprint(db_path: &str) -> anyhow::Result<()> {
    let db = Database::new(db_path)?;
    let footprint = db.storage_footprint()?;
    println!("db_path={db_path}");
    println!("db_bytes={}", footprint.db_bytes);
    println!("wal_bytes={}", footprint.wal_bytes);
    println!("feed_events={}", footprint.feed_event_count);
    for row in footprint.grouped_feed_events {
        println!(
            "source={} event_type={} rows={} approx_payload_bytes={}",
            row.source, row.event_type, row.row_count, row.approx_payload_bytes
        );
    }
    db.close();
    Ok(())
}

/// Re-export for callers that need the `optional()` extension.
use rusqlite::OptionalExtension;

#[cfg(test)]
#[path = "tests/database_tests.rs"]
mod tests;
