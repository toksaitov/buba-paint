use std::collections::{HashMap, VecDeque};

use crate::bankroll::BankrollManager;
use crate::clock::Clock;
use crate::config::Config;
use crate::db::database::Database;
use crate::fees::{resolve_fee_params, spread_net_edge};
use crate::types::{
    BookState, MarketWindow, ReplayFidelity, Signal, SignalDirection, SimulatedTrade, TopOfBook,
    TradeStatus,
};

#[derive(Debug, Clone)]
struct PendingOrder {
    signal_id: i64,
    signal_timestamp: u64,
    market_id: String,
    strategy: String,
    side: SignalDirection,
    token_id: String,
    arrival_ts: u64,
    requested_price: f64,
    limit_price: f64,
    requested_size: f64,
    reserved_cost: f64,
    execution_group_id: Option<String>,
    execution_fidelity: ReplayFidelity,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionStats {
    pub submitted_orders: u64,
    pub filled_orders: u64,
    pub partial_fills: u64,
    pub no_fills: u64,
    pub spread_legging_failures: u64,
    pub residual_positions: u64,
    pub total_requested_size: f64,
    pub total_filled_size: f64,
    pub total_slippage: f64,
    pub total_fill_latency_ms: u64,
    pub raw_event_batches: u64,
    pub legacy_snapshot_batches: u64,
}

impl ExecutionStats {
    /// Return the share of submitted orders that filled at least partially.
    pub fn fill_rate(&self) -> f64 {
        if self.submitted_orders == 0 {
            0.0
        } else {
            self.filled_orders as f64 / self.submitted_orders as f64
        }
    }

    /// Return the average fill latency across filled orders.
    pub fn avg_fill_latency_ms(&self) -> Option<f64> {
        if self.filled_orders == 0 {
            None
        } else {
            Some(self.total_fill_latency_ms as f64 / self.filled_orders as f64)
        }
    }

    /// Return the average positive slippage per filled token.
    pub fn avg_slippage(&self) -> Option<f64> {
        if self.total_filled_size <= 0.0 {
            None
        } else {
            Some(self.total_slippage / self.total_filled_size)
        }
    }
}

pub struct ExecutionEngine {
    pending_orders: VecDeque<PendingOrder>,
    stats: ExecutionStats,
    next_group_id: u64,
}

impl Default for ExecutionEngine {
    /// Create a fresh execution engine with empty queues and counters.
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEngine {
    #[must_use]
    /// Create a fresh execution engine with no pending orders.
    pub fn new() -> Self {
        Self {
            pending_orders: VecDeque::new(),
            stats: ExecutionStats::default(),
            next_group_id: 1,
        }
    }

    /// Return aggregate execution statistics collected so far.
    pub fn stats(&self) -> &ExecutionStats {
        &self.stats
    }

    /// Count one replay batch for the provided fidelity tier.
    pub fn note_replay_fidelity(&mut self, fidelity: ReplayFidelity) {
        match fidelity {
            ReplayFidelity::RawEvent => self.stats.raw_event_batches += 1,
            ReplayFidelity::LegacySnapshot => self.stats.legacy_snapshot_batches += 1,
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Queue one latency-arbitrage order for later simulated execution.
    pub fn submit_single(
        &mut self,
        signal: &Signal,
        window: &MarketWindow,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
        execution_fidelity: ReplayFidelity,
    ) -> anyhow::Result<Option<i64>> {
        let signal_id = db.log_signal_with_context(
            signal,
            Some(&window.market_id),
            Some(execution_fidelity),
            Some(signal.timestamp),
            Some(signal.timestamp.saturating_add(config.sim_order_latency_ms)),
        )?;

        if !self.can_queue_order(signal, window, false, 1, db, config)? {
            return Ok(Some(signal_id));
        }

        let requested_price = match signal.direction {
            SignalDirection::Up => signal.up_ask,
            SignalDirection::Down => signal.down_ask,
        };
        let limit_price = config.latency_arb_max_ask;
        let requested_size = bankroll.reserve_capital(
            limit_price,
            signal.confidence,
            &signal.strategy,
            config,
            clock,
        );
        if requested_size <= 0.0 {
            return Ok(Some(signal_id));
        }

        let token_id = match signal.direction {
            SignalDirection::Up => window.up_token_id.clone(),
            SignalDirection::Down => window.down_token_id.clone(),
        };

        self.pending_orders.push_back(PendingOrder {
            signal_id,
            signal_timestamp: signal.timestamp,
            market_id: window.market_id.clone(),
            strategy: signal.strategy.clone(),
            side: signal.direction,
            token_id,
            arrival_ts: signal.timestamp.saturating_add(config.sim_order_latency_ms),
            requested_price,
            limit_price,
            requested_size,
            reserved_cost: requested_size * limit_price,
            execution_group_id: None,
            execution_fidelity,
        });

        self.stats.submitted_orders += 1;
        self.stats.total_requested_size += requested_size;

        Ok(Some(signal_id))
    }

    #[allow(clippy::too_many_arguments)]
    /// Queue a two-leg spread order bundle for later simulated execution.
    pub fn submit_spread(
        &mut self,
        signals: &[Signal],
        window: &MarketWindow,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
        execution_fidelity: ReplayFidelity,
    ) -> anyhow::Result<Vec<i64>> {
        let arrival_ts = spread_arrival_ts(signals, config, clock);
        let signal_ids = log_spread_signals(signals, window, db, arrival_ts, execution_fidelity)?;
        if !self.can_queue_spread(signals, window, db, config)? {
            return Ok(signal_ids);
        }
        let Some((up_signal, down_signal, up_signal_id, down_signal_id)) =
            spread_signal_pair(signals, &signal_ids)
        else {
            return Ok(signal_ids);
        };
        let fee_params = resolve_fee_params(config, Some(window), window.end_time);
        if spread_net_edge(
            up_signal.up_ask,
            down_signal.down_ask,
            1.0,
            fee_params.fee_rate,
            fee_params.exponent,
        ) <= 0.0
        {
            return Ok(signal_ids);
        }
        let (up_tokens, down_tokens) = bankroll.reserve_spread_capital(
            up_signal.up_ask,
            down_signal.down_ask,
            up_signal.confidence.max(down_signal.confidence),
            config,
            clock,
        );
        if up_tokens <= 0.0 || down_tokens <= 0.0 {
            return Ok(signal_ids);
        }
        let group_id = format!("spread-{}", self.next_group_id);
        self.next_group_id += 1;
        self.queue_order(build_spread_order(
            up_signal,
            up_signal_id,
            SpreadOrderContext {
                market_id: &window.market_id,
                token_id: &window.up_token_id,
                arrival_ts,
                execution_group_id: Some(group_id.clone()),
                execution_fidelity,
            },
            up_tokens,
        ));
        self.queue_order(build_spread_order(
            down_signal,
            down_signal_id,
            SpreadOrderContext {
                market_id: &window.market_id,
                token_id: &window.down_token_id,
                arrival_ts,
                execution_group_id: Some(group_id),
                execution_fidelity,
            },
            down_tokens,
        ));
        Ok(signal_ids)
    }

    #[allow(clippy::too_many_arguments)]
    /// Attempt to fill all pending orders whose simulated arrival time has passed.
    pub fn process_due_orders(
        &mut self,
        now: u64,
        current_window: Option<&MarketWindow>,
        book_state: &BookState,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        _clock: &dyn Clock,
    ) -> anyhow::Result<Vec<SimulatedTrade>> {
        let mut remaining = VecDeque::new();
        let mut opened = Vec::new();
        let mut group_outcomes: HashMap<String, (u64, u64)> = HashMap::new();
        let mut context = DueOrderContext {
            now,
            current_window,
            book_state,
            db,
            bankroll,
            config,
        };

        while let Some(order) = self.pending_orders.pop_front() {
            match self.process_due_order(order, &mut context)? {
                OrderDisposition::Deferred(order) => remaining.push_back(order),
                OrderDisposition::Missed(group_id) => {
                    record_group_outcome(&mut group_outcomes, group_id.as_deref(), false);
                }
                OrderDisposition::Filled(trade, group_id) => {
                    record_group_outcome(&mut group_outcomes, group_id.as_deref(), true);
                    opened.push(*trade);
                }
            }
        }

        self.pending_orders = remaining;

        for (filled, total) in group_outcomes.values() {
            if *total == 2 && *filled == 1 {
                self.stats.spread_legging_failures += 1;
                self.stats.residual_positions += 1;
            }
        }

        Ok(opened)
    }

    /// Check whether a spread can be queued at all.
    fn can_queue_spread(
        &self,
        signals: &[Signal],
        window: &MarketWindow,
        db: &Database,
        config: &Config,
    ) -> anyhow::Result<bool> {
        let fallback_signal = Signal {
            timestamp: 0,
            strategy: "spread-capture".to_string(),
            direction: SignalDirection::Up,
            confidence: 1.0,
            binance_price: 0.0,
            chainlink_price: 0.0,
            up_ask: 0.0,
            down_ask: 0.0,
            up_bid: 0.0,
            down_bid: 0.0,
            metadata: serde_json::json!({}),
        };
        let signal = signals.first().unwrap_or(&fallback_signal);
        self.can_queue_order(signal, window, true, 2, db, config)
    }

    /// Queue a pending order and update aggregate counters.
    fn queue_order(&mut self, order: PendingOrder) {
        self.stats.submitted_orders += 1;
        self.stats.total_requested_size += order.requested_size;
        self.pending_orders.push_back(order);
    }

    /// Attempt to execute one pending order at the current replay timestamp.
    fn process_due_order(
        &mut self,
        order: PendingOrder,
        context: &mut DueOrderContext<'_>,
    ) -> anyhow::Result<OrderDisposition> {
        if order.arrival_ts > context.now {
            return Ok(OrderDisposition::Deferred(order));
        }
        let Some(window) = context
            .current_window
            .filter(|window| window.market_id == order.market_id)
        else {
            return Ok(self.reject_order(order, context.bankroll, false));
        };
        let Some(book) = side_book(context.book_state, order.side) else {
            return Ok(self.reject_order(order, context.bankroll, true));
        };
        if !book_is_fillable(
            book,
            context.now,
            order.limit_price,
            context.config.max_book_staleness_ms,
        ) {
            return Ok(self.reject_order(order, context.bankroll, true));
        }
        let tick_size = window.order_price_min_tick_size.unwrap_or(0.0);
        if tick_size > 0.0
            && (!price_is_tick_aligned(order.limit_price, tick_size)
                || !price_is_tick_aligned(book.best_ask, tick_size))
        {
            return Ok(self.reject_order(order, context.bankroll, true));
        }
        let filled_size = order.requested_size.min(book.ask_size);
        let market_min_size = window.order_min_size.unwrap_or(0.0);
        if fill_size_rejected(
            filled_size,
            book.best_ask,
            market_min_size,
            context.config.min_bet_usd,
        ) {
            return Ok(self.reject_order(order, context.bankroll, true));
        }

        let fill_cost = filled_size * book.best_ask;
        if order.reserved_cost > fill_cost {
            context
                .bankroll
                .release_reserved(order.reserved_cost - fill_cost);
        }
        self.record_fill(&order, book.best_ask, filled_size, context.now);

        let mut trade = build_trade(&order, book.best_ask, filled_size, context.now);
        let trade_id = context.db.open_trade(&trade)?;
        trade.id = Some(trade_id);
        Ok(OrderDisposition::Filled(
            Box::new(trade),
            order.execution_group_id,
        ))
    }

    /// Release the reserved capital for an unfilled order.
    fn reject_order(
        &mut self,
        order: PendingOrder,
        bankroll: &mut BankrollManager,
        track_group: bool,
    ) -> OrderDisposition {
        bankroll.release_reserved(order.reserved_cost);
        self.stats.no_fills += 1;
        OrderDisposition::Missed(track_group.then_some(order.execution_group_id).flatten())
    }

    /// Update aggregate execution statistics for a successful fill.
    fn record_fill(&mut self, order: &PendingOrder, fill_price: f64, filled_size: f64, now: u64) {
        if filled_size < order.requested_size {
            self.stats.partial_fills += 1;
        }
        self.stats.filled_orders += 1;
        self.stats.total_filled_size += filled_size;
        self.stats.total_slippage += (fill_price - order.requested_price).max(0.0) * filled_size;
        self.stats.total_fill_latency_ms += now.saturating_sub(order.signal_timestamp);
    }

    /// Return whether a new order can be queued without violating position rules.
    fn can_queue_order(
        &self,
        signal: &Signal,
        window: &MarketWindow,
        is_batch: bool,
        required_slots: u64,
        db: &Database,
        config: &Config,
    ) -> anyhow::Result<bool> {
        let open_count = db.count_open_trades()?;
        let pending_count = self.pending_orders.len() as u64;
        if open_count + pending_count + required_slots > config.max_open_positions {
            return Ok(false);
        }

        let existing = db.get_open_trades_for_market(&window.market_id)?;
        let duplicate_open = if is_batch {
            existing
                .iter()
                .any(|trade| trade.strategy == signal.strategy && trade.side == signal.direction)
        } else {
            existing
                .iter()
                .any(|trade| trade.strategy == signal.strategy)
        };
        if duplicate_open {
            return Ok(false);
        }

        let duplicate_pending = if is_batch {
            self.pending_orders.iter().any(|order| {
                order.market_id == window.market_id
                    && order.strategy == signal.strategy
                    && order.side == signal.direction
            })
        } else {
            self.pending_orders.iter().any(|order| {
                order.market_id == window.market_id && order.strategy == signal.strategy
            })
        };

        Ok(!duplicate_pending)
    }
}

/// Describe the outcome of processing one pending order.
enum OrderDisposition {
    Deferred(PendingOrder),
    Missed(Option<String>),
    Filled(Box<SimulatedTrade>, Option<String>),
}

/// Carry the shared context needed to settle due orders.
struct DueOrderContext<'a> {
    now: u64,
    current_window: Option<&'a MarketWindow>,
    book_state: &'a BookState,
    db: &'a Database,
    bankroll: &'a mut BankrollManager,
    config: &'a Config,
}

/// Carry the common metadata for a queued spread order.
struct SpreadOrderContext<'a> {
    market_id: &'a str,
    token_id: &'a str,
    arrival_ts: u64,
    execution_group_id: Option<String>,
    execution_fidelity: ReplayFidelity,
}

/// Persist spread signals and return their DB ids.
fn log_spread_signals(
    signals: &[Signal],
    window: &MarketWindow,
    db: &Database,
    arrival_ts: u64,
    execution_fidelity: ReplayFidelity,
) -> anyhow::Result<Vec<i64>> {
    let mut signal_ids = Vec::with_capacity(signals.len());
    for signal in signals {
        let signal_id = db.log_signal_with_context(
            signal,
            Some(&window.market_id),
            Some(execution_fidelity),
            Some(signal.timestamp),
            Some(arrival_ts),
        )?;
        signal_ids.push(signal_id);
    }
    Ok(signal_ids)
}

/// Calculate the arrival timestamp for a spread signal bundle.
fn spread_arrival_ts(signals: &[Signal], config: &Config, clock: &dyn Clock) -> u64 {
    signals
        .iter()
        .map(|signal| signal.timestamp.saturating_add(config.sim_order_latency_ms))
        .max()
        .unwrap_or_else(|| clock.now().saturating_add(config.sim_order_latency_ms))
}

/// Build the queued spread order for one signal leg.
fn build_spread_order(
    signal: &Signal,
    signal_id: i64,
    context: SpreadOrderContext<'_>,
    requested_size: f64,
) -> PendingOrder {
    let requested_price = match signal.direction {
        SignalDirection::Up => signal.up_ask,
        SignalDirection::Down => signal.down_ask,
    };
    PendingOrder {
        signal_id,
        signal_timestamp: signal.timestamp,
        market_id: context.market_id.to_string(),
        strategy: signal.strategy.clone(),
        side: signal.direction,
        token_id: context.token_id.to_string(),
        arrival_ts: context.arrival_ts,
        requested_price,
        limit_price: requested_price,
        requested_size,
        reserved_cost: requested_size * requested_price,
        execution_group_id: context.execution_group_id,
        execution_fidelity: context.execution_fidelity,
    }
}

/// Extract the up/down spread pair together with their persisted signal ids.
fn spread_signal_pair<'a>(
    signals: &'a [Signal],
    signal_ids: &[i64],
) -> Option<(&'a Signal, &'a Signal, i64, i64)> {
    let up_index = signals
        .iter()
        .position(|signal| signal.direction == SignalDirection::Up)?;
    let down_index = signals
        .iter()
        .position(|signal| signal.direction == SignalDirection::Down)?;
    Some((
        &signals[up_index],
        &signals[down_index],
        *signal_ids.get(up_index)?,
        *signal_ids.get(down_index)?,
    ))
}

/// Return whether the current top of book can satisfy a taker order.
fn book_is_fillable(book: &TopOfBook, now: u64, limit_price: f64, max_staleness_ms: u64) -> bool {
    now.saturating_sub(book.timestamp) <= max_staleness_ms
        && book.best_ask > 0.0
        && book.best_ask <= limit_price
        && book.ask_size > 0.0
}

/// Return whether the computed fill should be rejected before persistence.
fn fill_size_rejected(
    filled_size: f64,
    fill_price: f64,
    market_min_size: f64,
    min_bet_usd: f64,
) -> bool {
    filled_size <= 0.0
        || (market_min_size > 0.0 && filled_size < market_min_size)
        || filled_size * fill_price < min_bet_usd
}

/// Build the persisted trade row for a successful execution.
fn build_trade(
    order: &PendingOrder,
    fill_price: f64,
    filled_size: f64,
    now: u64,
) -> SimulatedTrade {
    SimulatedTrade {
        id: None,
        timestamp: now,
        market_id: order.market_id.clone(),
        strategy: order.strategy.clone(),
        side: order.side,
        token_id: order.token_id.clone(),
        entry_price: fill_price,
        size: filled_size,
        status: TradeStatus::Open,
        signal_id: Some(order.signal_id),
        requested_price: Some(order.limit_price),
        requested_size: Some(order.requested_size),
        filled_size: Some(filled_size),
        avg_fill_price: Some(fill_price),
        fill_status: Some(if filled_size < order.requested_size {
            "partial".to_string()
        } else {
            "filled".to_string()
        }),
        fill_reason: Some("book_fill".to_string()),
        fill_latency_ms: Some(now.saturating_sub(order.signal_timestamp)),
        execution_group_id: order.execution_group_id.clone(),
        execution_fidelity: Some(order.execution_fidelity.to_string()),
        execution_mode: Some("paper".to_string()),
        order_id: Some(format!("paper-{}", order.signal_id)),
        fill_price: Some(fill_price),
    }
}

/// Return the relevant side of book for the provided signal direction.
fn side_book(book_state: &BookState, side: SignalDirection) -> Option<&TopOfBook> {
    match side {
        SignalDirection::Up => book_state.up.as_ref(),
        SignalDirection::Down => book_state.down.as_ref(),
    }
}

/// Accumulate fill outcomes for spread groups so legging can be measured later.
fn record_group_outcome(
    outcomes: &mut HashMap<String, (u64, u64)>,
    execution_group_id: Option<&str>,
    filled: bool,
) {
    let Some(group_id) = execution_group_id else {
        return;
    };
    let entry = outcomes.entry(group_id.to_string()).or_insert((0, 0));
    if filled {
        entry.0 += 1;
    }
    entry.1 += 1;
}

/// Return whether the price lands exactly on the configured tick size.
fn price_is_tick_aligned(price: f64, tick_size: f64) -> bool {
    if tick_size <= 0.0 {
        return true;
    }
    let units = price / tick_size;
    (units - units.round()).abs() < 1e-9
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    use crate::clock::BacktestClock;
    use crate::db::database::Database;
    use crate::types::{ReplayFidelity, TradeStatus};

    #[test]
    /// Verify that exact tick multiples are treated as valid prices.
    fn tick_alignment_accepts_exact_multiple() {
        assert!(price_is_tick_aligned(0.55, 0.01));
        assert!(price_is_tick_aligned(0.5, 0.05));
    }

    #[test]
    /// Verify that off-tick prices are rejected.
    fn tick_alignment_rejects_non_multiple() {
        assert!(!price_is_tick_aligned(0.551, 0.01));
    }

    /// Create a temporary `SQLite` database for executor tests.
    fn temp_db() -> (TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("paint.db");
        let db = Database::new(db_path.to_str().unwrap()).unwrap();
        (dir, db)
    }

    /// Build the standard executor test configuration.
    fn test_config() -> Config {
        let mut config = Config::default();
        config.latency_arb_max_ask = 0.60;
        config.spread_capture_threshold = 0.99;
        config.max_position_fraction = 1.0;
        config.max_position_usd_fraction = 1.0;
        config.max_position_usd = 1_000.0;
        config.min_bet_usd = 1.0;
        config.max_open_positions = 10;
        config.sim_order_latency_ms = 250;
        config.max_book_staleness_ms = 1_000;
        config
    }

    /// Build a representative market window for executor tests.
    fn test_window() -> MarketWindow {
        MarketWindow {
            market_id: "mkt-1".to_string(),
            question: "Will BTC go up?".to_string(),
            up_token_id: "tok-up".to_string(),
            down_token_id: "tok-down".to_string(),
            condition_id: "cond-1".to_string(),
            start_time: 1_000,
            end_time: 301_000,
            slug: "btc-updown-5m".to_string(),
            outcome: None,
            resolution_source: Some("gamma".to_string()),
            fee_profile: Some("crypto".to_string()),
            order_min_size: Some(1.0),
            order_price_min_tick_size: Some(0.01),
            maker_base_fee: None,
            taker_base_fee: None,
            rewards_min_size: None,
            rewards_max_spread: None,
        }
    }

    /// Build a latency-arbitrage signal for the requested side.
    fn latency_signal(timestamp: u64, side: SignalDirection) -> Signal {
        Signal {
            timestamp,
            strategy: "latency-arb".to_string(),
            direction: side,
            confidence: 1.0,
            binance_price: 68_000.0,
            chainlink_price: 68_010.0,
            up_ask: 0.55,
            down_ask: 0.45,
            up_bid: 0.54,
            down_bid: 0.44,
            metadata: serde_json::json!({ "momentum": 0.0012 }),
        }
    }

    /// Build the two spread-capture signals for one timestamp.
    fn spread_signals(timestamp: u64, up_ask: f64, down_ask: f64) -> Vec<Signal> {
        vec![
            Signal {
                timestamp,
                strategy: "spread-capture".to_string(),
                direction: SignalDirection::Up,
                confidence: 1.0,
                binance_price: 68_000.0,
                chainlink_price: 68_000.0,
                up_ask,
                down_ask,
                up_bid: up_ask - 0.01,
                down_bid: down_ask - 0.01,
                metadata: serde_json::json!({ "spread": up_ask + down_ask }),
            },
            Signal {
                timestamp,
                strategy: "spread-capture".to_string(),
                direction: SignalDirection::Down,
                confidence: 1.0,
                binance_price: 68_000.0,
                chainlink_price: 68_000.0,
                up_ask,
                down_ask,
                up_bid: up_ask - 0.01,
                down_bid: down_ask - 0.01,
                metadata: serde_json::json!({ "spread": up_ask + down_ask }),
            },
        ]
    }

    /// Build a one-sided top-of-book snapshot for the UP leg.
    fn up_book(best_ask: f64, ask_size: f64, timestamp: u64) -> BookState {
        BookState {
            up: Some(TopOfBook {
                best_bid: best_ask - 0.01,
                best_ask,
                bid_size: ask_size,
                ask_size,
                timestamp,
            }),
            down: None,
        }
    }

    #[test]
    /// Verify that replay-fidelity counters track raw and legacy batches separately.
    fn note_replay_fidelity_tracks_batch_counts() {
        let mut engine = ExecutionEngine::new();
        engine.note_replay_fidelity(ReplayFidelity::RawEvent);
        engine.note_replay_fidelity(ReplayFidelity::LegacySnapshot);
        engine.note_replay_fidelity(ReplayFidelity::LegacySnapshot);

        assert_eq!(engine.stats().raw_event_batches, 1);
        assert_eq!(engine.stats().legacy_snapshot_batches, 2);
    }

    #[test]
    /// Verify that partial fills persist the expected execution metadata.
    fn process_due_orders_records_partial_fill_metadata() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal(10_000, SignalDirection::Up);

        engine
            .submit_single(
                &signal,
                &window,
                &db,
                &mut bankroll,
                &config,
                &clock,
                ReplayFidelity::LegacySnapshot,
            )
            .unwrap();

        let opened = engine
            .process_due_orders(
                10_250,
                Some(&window),
                &up_book(0.55, 40.0, 10_250),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].status, TradeStatus::Open);
        assert_eq!(opened[0].fill_status.as_deref(), Some("partial"));
        assert_eq!(opened[0].filled_size, Some(40.0));
        assert_eq!(engine.stats().filled_orders, 1);
        assert_eq!(engine.stats().partial_fills, 1);

        let persisted: (String, f64, String) = db
            .conn()
            .query_row(
                "SELECT fill_status, filled_size, execution_fidelity FROM simulated_trades LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(persisted.0, "partial");
        assert_eq!(persisted.1, 40.0);
        assert_eq!(persisted.2, "legacy_snapshot");
    }

    #[test]
    /// Verify that stale books cause pending orders to miss instead of filling.
    fn process_due_orders_rejects_stale_books() {
        let (_dir, db) = temp_db();
        let mut config = test_config();
        config.max_book_staleness_ms = 50;
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal(20_000, SignalDirection::Up);

        engine
            .submit_single(
                &signal,
                &window,
                &db,
                &mut bankroll,
                &config,
                &clock,
                ReplayFidelity::LegacySnapshot,
            )
            .unwrap();

        let opened = engine
            .process_due_orders(
                20_250,
                Some(&window),
                &up_book(0.55, 100.0, 20_000),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert!(opened.is_empty());
        assert_eq!(engine.stats().no_fills, 1);
        let open_trades = db.count_open_trades().unwrap();
        assert_eq!(open_trades, 0);
    }

    #[test]
    /// Verify that spread orders are skipped when fees remove the net edge.
    fn submit_spread_skips_orders_when_fees_wipe_out_edge() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal_ids = engine
            .submit_spread(
                &spread_signals(30_000, 0.50, 0.50),
                &window,
                &db,
                &mut bankroll,
                &config,
                &clock,
                ReplayFidelity::LegacySnapshot,
            )
            .unwrap();

        assert_eq!(signal_ids.len(), 2);
        assert_eq!(engine.stats().submitted_orders, 0);
        assert_eq!(db.count_open_trades().unwrap(), 0);
    }

    #[test]
    /// Verify that one-legged spread fills record legging and residual-position stats.
    fn process_due_orders_tracks_spread_legging_failures() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signals = spread_signals(40_000, 0.49, 0.48);

        engine
            .submit_spread(
                &signals,
                &window,
                &db,
                &mut bankroll,
                &config,
                &clock,
                ReplayFidelity::LegacySnapshot,
            )
            .unwrap();

        let opened = engine
            .process_due_orders(
                40_250,
                Some(&window),
                &up_book(0.49, 50.0, 40_250),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert_eq!(opened.len(), 1);
        assert_eq!(engine.stats().filled_orders, 1);
        assert_eq!(engine.stats().no_fills, 1);
        assert_eq!(engine.stats().spread_legging_failures, 1);
        assert_eq!(engine.stats().residual_positions, 1);
    }
}
