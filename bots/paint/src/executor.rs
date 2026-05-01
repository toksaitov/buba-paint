use std::collections::{HashMap, VecDeque};

use anyhow::Context;

use crate::bankroll::BankrollManager;
use crate::clock::Clock;
use crate::config::{Config, ExecutionMode};
use crate::db::database::Database;
use crate::fees::{resolve_fee_params, spread_net_edge};
use crate::portfolio::StrategyFamily;
use crate::signal_features::effective_book_timestamp;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderOutcomeDisposition {
    Filled,
    Missed,
}

#[derive(Debug, Clone)]
pub struct ProcessedOrderOutcome {
    pub signal_id: i64,
    pub market_id: String,
    pub strategy: String,
    pub side: SignalDirection,
    pub disposition: OrderOutcomeDisposition,
    pub reason: Option<String>,
    pub best_ask: Option<f64>,
    pub ask_size: Option<f64>,
    pub freshness_ms: Option<u64>,
    pub requested_size: f64,
    pub filled_size: f64,
    pub effective_arrival_delay_ms: u64,
    pub partial_fill: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueRejectionReason {
    MaxOpenPositions,
    DuplicateOpenPosition,
    DuplicatePendingOrder,
}

/// Read-only inputs needed to decide whether a new order can be queued.
struct QueueCheckContext<'a> {
    db: &'a Database,
    config: &'a Config,
    now_ms: u64,
}

impl QueueRejectionReason {
    #[must_use]
    /// Return the stable persisted label for one queue-rejection reason.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaxOpenPositions => "max_open_positions",
            Self::DuplicateOpenPosition => "duplicate_open_position",
            Self::DuplicatePendingOrder => "duplicate_pending_order",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueuedOrderIntent {
    pub signal_id: i64,
    pub signal_timestamp: u64,
    pub market_id: String,
    pub strategy: String,
    pub side: SignalDirection,
    pub token_id: String,
    pub arrival_ts: u64,
    pub requested_price: f64,
    pub limit_price: f64,
    pub requested_size: f64,
    pub reserved_cost: f64,
    pub execution_group_id: Option<String>,
    pub execution_fidelity: ReplayFidelity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SubmissionOutcome {
    Queued {
        signal_ids: Vec<i64>,
        orders: Vec<QueuedOrderIntent>,
    },
    Rejected {
        signal_ids: Vec<i64>,
        reason: String,
    },
}

/// Infer the portfolio family from one emitted signal.
fn family_for_signal(signal: &Signal) -> StrategyFamily {
    StrategyFamily::from_strategy_name(&signal.strategy).unwrap_or(StrategyFamily::LatencyArb)
}

/// Resolve the per-family single-order limit price used for reserve sizing and fills.
fn single_order_limit_price(signal: &Signal, config: &Config) -> f64 {
    match family_for_signal(signal) {
        StrategyFamily::CalmPersistence => config.calm_persistence_max_ask,
        StrategyFamily::LatencyArb | StrategyFamily::SpreadCapture => config.latency_arb_max_ask,
    }
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
    recent_outcomes: Vec<ProcessedOrderOutcome>,
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
            recent_outcomes: Vec::new(),
            stats: ExecutionStats::default(),
            next_group_id: 1,
        }
    }

    /// Return aggregate execution statistics collected so far.
    pub fn stats(&self) -> &ExecutionStats {
        &self.stats
    }

    /// Drain the processed-order outcomes accumulated since the last caller read them.
    pub fn take_recent_outcomes(&mut self) -> Vec<ProcessedOrderOutcome> {
        std::mem::take(&mut self.recent_outcomes)
    }

    /// Count one replay batch for the provided fidelity tier.
    pub fn note_replay_fidelity(&mut self, fidelity: ReplayFidelity) {
        match fidelity {
            ReplayFidelity::RawEvent => self.stats.raw_event_batches += 1,
            ReplayFidelity::LegacySnapshot => self.stats.legacy_snapshot_batches += 1,
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Queue one single-side order for later simulated execution.
    pub fn submit_single(
        &mut self,
        signal: &Signal,
        window: &MarketWindow,
        db: &Database,
        bankroll: &mut BankrollManager,
        config: &Config,
        clock: &dyn Clock,
        execution_fidelity: ReplayFidelity,
    ) -> anyhow::Result<SubmissionOutcome> {
        let signal_id = db.log_signal_with_context(
            signal,
            Some(&window.market_id),
            Some(execution_fidelity),
            Some(signal.timestamp),
            Some(signal.timestamp.saturating_add(config.sim_order_latency_ms)),
        )?;

        if let Some(reason) = self.precheck_single_submission(signal, window, db, config, clock)? {
            return Ok(reject_single_submission(
                db,
                signal,
                signal_id,
                reason.as_str(),
            ));
        }

        let requested_price = match signal.direction {
            SignalDirection::Up => signal.up_ask,
            SignalDirection::Down => signal.down_ask,
        };
        let limit_price = single_order_limit_price(signal, config);
        let requested_size = bankroll.reserve_capital_with_reserve_price(
            requested_price,
            limit_price,
            signal.confidence,
            &signal.strategy,
            family_for_signal(signal),
            config,
            clock,
        );
        if requested_size <= 0.0 {
            return Ok(reject_single_submission(
                db,
                signal,
                signal_id,
                "strategy_sleeve_exhausted",
            ));
        }
        if let Some(reason) = submission_size_rejection_reason(
            requested_size,
            requested_price,
            window.order_min_size.unwrap_or(0.0),
            config.min_bet_usd,
        ) {
            bankroll.release_reserved_for_strategy(requested_size * limit_price, &signal.strategy);
            return Ok(reject_single_submission(db, signal, signal_id, reason));
        }
        Ok(self.queue_single_submission(
            signal,
            signal_id,
            window,
            requested_price,
            limit_price,
            requested_size,
            db,
            config,
            execution_fidelity,
        ))
    }

    /// Return any single-order queue rejection without persisting signals or mutating state.
    pub fn precheck_single_submission(
        &self,
        signal: &Signal,
        window: &MarketWindow,
        db: &Database,
        config: &Config,
        clock: &dyn Clock,
    ) -> anyhow::Result<Option<QueueRejectionReason>> {
        let queue_ctx = QueueCheckContext {
            db,
            config,
            now_ms: clock.now(),
        };
        self.can_queue_order(signal, window, false, 1, &queue_ctx)
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
    ) -> anyhow::Result<SubmissionOutcome> {
        let arrival_ts = spread_arrival_ts(signals, config, clock);
        let signal_ids = log_spread_signals(signals, window, db, arrival_ts, execution_fidelity)?;
        let queue_ctx = QueueCheckContext {
            db,
            config,
            now_ms: clock.now(),
        };
        if let Some(reason) = self.can_queue_spread(signals, window, &queue_ctx)? {
            return Ok(reject_spread_submission(
                db,
                signals,
                signal_ids,
                reason.as_str(),
            ));
        }
        let Some((up_signal, down_signal, up_signal_id, down_signal_id)) =
            spread_signal_pair(signals, &signal_ids)
        else {
            return Ok(SubmissionOutcome::Queued {
                signal_ids,
                orders: Vec::new(),
            });
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
            return Ok(reject_spread_submission(
                db, signals, signal_ids, "net_edge",
            ));
        }
        let (up_tokens, down_tokens) = bankroll.reserve_spread_capital(
            up_signal.up_ask,
            down_signal.down_ask,
            up_signal.confidence.max(down_signal.confidence),
            config,
            clock,
        );
        if up_tokens <= 0.0 || down_tokens <= 0.0 {
            return Ok(reject_spread_submission(
                db,
                signals,
                signal_ids,
                "strategy_sleeve_exhausted",
            ));
        }
        if let Some(reason) = spread_submission_rejection_reason(
            up_tokens,
            down_tokens,
            up_signal.up_ask,
            down_signal.down_ask,
            window.order_min_size.unwrap_or(0.0),
            config.min_bet_usd,
        ) {
            let reserved_spread_cost =
                (up_tokens * up_signal.up_ask) + (down_tokens * down_signal.down_ask);
            bankroll.release_reserved_for_strategy(reserved_spread_cost, &up_signal.strategy);
            return Ok(reject_spread_submission(db, signals, signal_ids, reason));
        }
        let orders = self.queue_spread_submission_orders(
            &SpreadQueuePlan {
                up_signal,
                down_signal,
                up_signal_id,
                down_signal_id,
                window,
                arrival_ts,
                up_tokens,
                down_tokens,
                execution_fidelity,
            },
            config,
        );
        update_spread_signal_metrics(
            db,
            signals,
            &signal_ids,
            Some(arrival_ts),
            "submitted",
            None,
        );
        Ok(SubmissionOutcome::Queued { signal_ids, orders })
    }

    /// Queue both legs of an accepted spread submission.
    fn queue_spread_submission_orders(
        &mut self,
        plan: &SpreadQueuePlan<'_>,
        config: &Config,
    ) -> Vec<QueuedOrderIntent> {
        let group_id = format!("spread-{}", self.next_group_id);
        self.next_group_id += 1;
        let up_order = build_spread_order(
            plan.up_signal,
            plan.up_signal_id,
            SpreadOrderContext {
                market_id: &plan.window.market_id,
                token_id: &plan.window.up_token_id,
                arrival_ts: plan.arrival_ts,
                execution_group_id: Some(group_id.clone()),
                execution_fidelity: plan.execution_fidelity,
            },
            plan.up_tokens,
        );
        let down_order = build_spread_order(
            plan.down_signal,
            plan.down_signal_id,
            SpreadOrderContext {
                market_id: &plan.window.market_id,
                token_id: &plan.window.down_token_id,
                arrival_ts: plan.arrival_ts,
                execution_group_id: Some(group_id),
                execution_fidelity: plan.execution_fidelity,
            },
            plan.down_tokens,
        );
        let orders = vec![
            queued_order_intent(&up_order),
            queued_order_intent(&down_order),
        ];
        self.record_submitted_order(&up_order);
        self.record_submitted_order(&down_order);
        if config.execution_mode != ExecutionMode::LiveTrading {
            self.pending_orders.push_back(up_order);
            self.pending_orders.push_back(down_order);
        }
        orders
    }

    #[allow(clippy::too_many_arguments)]
    /// Attempt to fill all pending orders whose simulated arrival time has passed.
    pub fn process_due_orders(
        &mut self,
        now: u64,
        now_us: Option<u64>,
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
            now_us,
            current_window,
            book_state,
            db,
            bankroll,
            config,
        };

        while let Some(order) = self.pending_orders.pop_front() {
            let disposition = match self.process_due_order(order, &mut context) {
                Ok(disposition) => disposition,
                Err(error) => {
                    remaining.append(&mut self.pending_orders);
                    self.pending_orders = remaining;
                    return Err(error);
                }
            };
            match disposition {
                OrderDisposition::Deferred(order) => remaining.push_back(order),
                OrderDisposition::Missed { group_id, outcome } => {
                    self.recent_outcomes.push(outcome);
                    record_group_outcome(&mut group_outcomes, group_id.as_deref(), false);
                }
                OrderDisposition::Filled {
                    trade,
                    group_id,
                    outcome,
                } => {
                    self.recent_outcomes.push(outcome);
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
        queue_ctx: &QueueCheckContext<'_>,
    ) -> anyhow::Result<Option<QueueRejectionReason>> {
        let open_count =
            count_queue_relevant_open_trades(queue_ctx.db, queue_ctx.config, queue_ctx.now_ms)?;
        let pending_count = self.pending_orders.len() as u64;
        if open_count + pending_count + 2 > queue_ctx.config.max_open_positions {
            return Ok(Some(QueueRejectionReason::MaxOpenPositions));
        }

        let existing = queue_ctx.db.get_open_trades_for_market(&window.market_id)?;
        if signals.iter().any(|signal| {
            existing
                .iter()
                .any(|trade| trade.strategy == signal.strategy && trade.side == signal.direction)
        }) {
            return Ok(Some(QueueRejectionReason::DuplicateOpenPosition));
        }

        if signals.iter().any(|signal| {
            self.pending_orders.iter().any(|order| {
                order.market_id == window.market_id
                    && order.strategy == signal.strategy
                    && order.side == signal.direction
            })
        }) {
            return Ok(Some(QueueRejectionReason::DuplicatePendingOrder));
        }

        Ok(None)
    }

    /// Update aggregate counters for one accepted submission.
    fn record_submitted_order(&mut self, order: &PendingOrder) {
        self.stats.submitted_orders += 1;
        self.stats.total_requested_size += order.requested_size;
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
            return self.reject_order(
                order,
                context,
                MissedOrderContext {
                    track_group: false,
                    reason: "window_missing_on_arrival",
                    best_ask: None,
                    ask_size: None,
                    freshness_ms: None,
                },
            );
        };
        let Some(book) = side_book(context.book_state, order.side) else {
            return self.reject_order(
                order,
                context,
                MissedOrderContext {
                    track_group: true,
                    reason: "book_unavailable_on_arrival",
                    best_ask: None,
                    ask_size: None,
                    freshness_ms: None,
                },
            );
        };
        let freshness_ms = Some(context.now.saturating_sub(effective_book_timestamp(book)));
        if let Some(reason) = book_fill_rejection_reason(
            book,
            context.now,
            order.limit_price,
            context.config.max_book_staleness_ms,
        ) {
            return self.reject_order(
                order,
                context,
                MissedOrderContext {
                    track_group: true,
                    reason,
                    best_ask: Some(book.best_ask),
                    ask_size: Some(book.ask_size),
                    freshness_ms,
                },
            );
        }
        let tick_size = window.order_price_min_tick_size.unwrap_or(0.0);
        if tick_size > 0.0
            && (!price_is_tick_aligned(order.limit_price, tick_size)
                || !price_is_tick_aligned(book.best_ask, tick_size))
        {
            return self.reject_order(
                order,
                context,
                MissedOrderContext {
                    track_group: true,
                    reason: "tick_misaligned_on_arrival",
                    best_ask: Some(book.best_ask),
                    ask_size: Some(book.ask_size),
                    freshness_ms,
                },
            );
        }
        let filled_size = order.requested_size.min(book.ask_size);
        let market_min_size = window.order_min_size.unwrap_or(0.0);
        if fill_size_rejected(
            filled_size,
            book.best_ask,
            market_min_size,
            context.config.min_bet_usd,
        ) {
            return self.reject_order(
                order,
                context,
                MissedOrderContext {
                    track_group: true,
                    reason: "fill_below_min_on_arrival",
                    best_ask: Some(book.best_ask),
                    ask_size: Some(book.ask_size),
                    freshness_ms,
                },
            );
        }
        self.fill_due_order(order, context, book, freshness_ms, filled_size)
    }

    /// Persist one successful delayed paper fill and return the resulting trade.
    fn fill_due_order(
        &mut self,
        order: PendingOrder,
        context: &mut DueOrderContext<'_>,
        book: &TopOfBook,
        freshness_ms: Option<u64>,
        filled_size: f64,
    ) -> anyhow::Result<OrderDisposition> {
        let fill_cost = filled_size * book.best_ask;
        if order.reserved_cost > fill_cost {
            context
                .bankroll
                .release_reserved_for_strategy(order.reserved_cost - fill_cost, &order.strategy);
        }
        self.record_fill(&order, book.best_ask, filled_size, context.now);

        let mut trade = build_trade(
            &order,
            book.best_ask,
            filled_size,
            context.now,
            context.config.execution_mode.as_str(),
        );
        let trade_id = context.db.open_trade(&trade)?;
        trade.id = Some(trade_id);
        let effective_arrival_delay_ms = context.now.saturating_sub(order.signal_timestamp);
        context
            .db
            .update_signal_execution_outcome(
                order.signal_id,
                context.now,
                context.now_us,
                effective_arrival_delay_ms,
                "filled",
                None,
            )
            .with_context(|| {
                format!(
                    "failed to persist filled execution outcome for signal_id={} market_id={}",
                    order.signal_id, order.market_id
                )
            })?;
        Ok(OrderDisposition::Filled {
            trade: Box::new(trade),
            group_id: order.execution_group_id.clone(),
            outcome: ProcessedOrderOutcome {
                signal_id: order.signal_id,
                market_id: order.market_id,
                strategy: order.strategy,
                side: order.side,
                disposition: OrderOutcomeDisposition::Filled,
                reason: None,
                best_ask: Some(book.best_ask),
                ask_size: Some(book.ask_size),
                freshness_ms,
                requested_size: order.requested_size,
                filled_size,
                effective_arrival_delay_ms,
                partial_fill: filled_size < order.requested_size,
            },
        })
    }

    /// Release the reserved capital for an unfilled order.
    fn reject_order(
        &mut self,
        order: PendingOrder,
        context: &mut DueOrderContext<'_>,
        miss: MissedOrderContext,
    ) -> anyhow::Result<OrderDisposition> {
        context
            .bankroll
            .release_reserved_for_strategy(order.reserved_cost, &order.strategy);
        self.stats.no_fills += 1;
        let effective_arrival_delay_ms = context.now.saturating_sub(order.signal_timestamp);
        context
            .db
            .update_signal_execution_outcome(
                order.signal_id,
                context.now,
                context.now_us,
                effective_arrival_delay_ms,
                "missed",
                Some(miss.reason),
            )
            .with_context(|| {
                format!(
                    "failed to persist missed execution outcome for signal_id={} market_id={} reason={}",
                    order.signal_id, order.market_id, miss.reason
                )
            })?;
        Ok(OrderDisposition::Missed {
            group_id: miss
                .track_group
                .then_some(order.execution_group_id.clone())
                .flatten(),
            outcome: ProcessedOrderOutcome {
                signal_id: order.signal_id,
                market_id: order.market_id,
                strategy: order.strategy,
                side: order.side,
                disposition: OrderOutcomeDisposition::Missed,
                reason: Some(miss.reason.to_string()),
                best_ask: miss.best_ask,
                ask_size: miss.ask_size,
                freshness_ms: miss.freshness_ms,
                requested_size: order.requested_size,
                filled_size: 0.0,
                effective_arrival_delay_ms,
                partial_fill: false,
            },
        })
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
        queue_ctx: &QueueCheckContext<'_>,
    ) -> anyhow::Result<Option<QueueRejectionReason>> {
        let open_count =
            count_queue_relevant_open_trades(queue_ctx.db, queue_ctx.config, queue_ctx.now_ms)?;
        let pending_count = self.pending_orders.len() as u64;
        if open_count + pending_count + required_slots > queue_ctx.config.max_open_positions {
            return Ok(Some(QueueRejectionReason::MaxOpenPositions));
        }

        let existing = queue_ctx.db.get_open_trades_for_market(&window.market_id)?;
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
            return Ok(Some(QueueRejectionReason::DuplicateOpenPosition));
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

        Ok(duplicate_pending.then_some(QueueRejectionReason::DuplicatePendingOrder))
    }

    /// Queue one single-side order and persist its submitted telemetry snapshot.
    #[allow(clippy::too_many_arguments)]
    fn queue_single_submission(
        &mut self,
        signal: &Signal,
        signal_id: i64,
        window: &MarketWindow,
        requested_price: f64,
        limit_price: f64,
        requested_size: f64,
        db: &Database,
        config: &Config,
        execution_fidelity: ReplayFidelity,
    ) -> SubmissionOutcome {
        let arrival_ts = signal.timestamp.saturating_add(config.sim_order_latency_ms);
        let order = PendingOrder {
            signal_id,
            signal_timestamp: signal.timestamp,
            market_id: window.market_id.clone(),
            strategy: signal.strategy.clone(),
            side: signal.direction,
            token_id: match signal.direction {
                SignalDirection::Up => window.up_token_id.clone(),
                SignalDirection::Down => window.down_token_id.clone(),
            },
            arrival_ts,
            requested_price,
            limit_price,
            requested_size,
            reserved_cost: requested_size * limit_price,
            execution_group_id: None,
            execution_fidelity,
        };
        let queued = queued_order_intent(&order);
        self.record_submitted_order(&order);
        if config.execution_mode != ExecutionMode::LiveTrading {
            self.pending_orders.push_back(order);
        }
        if let Some(telemetry) = signal.telemetry.as_ref() {
            let _ = db.upsert_signal_telemetry(
                signal_id,
                telemetry,
                Some(signal.timestamp),
                Some(arrival_ts),
                Some("submitted"),
                None,
            );
        }
        SubmissionOutcome::Queued {
            signal_ids: vec![signal_id],
            orders: vec![queued],
        }
    }
}

/// Convert one accepted submission into the public intent snapshot.
fn queued_order_intent(order: &PendingOrder) -> QueuedOrderIntent {
    QueuedOrderIntent {
        signal_id: order.signal_id,
        signal_timestamp: order.signal_timestamp,
        market_id: order.market_id.clone(),
        strategy: order.strategy.clone(),
        side: order.side,
        token_id: order.token_id.clone(),
        arrival_ts: order.arrival_ts,
        requested_price: order.requested_price,
        limit_price: order.limit_price,
        requested_size: order.requested_size,
        reserved_cost: order.reserved_cost,
        execution_group_id: order.execution_group_id.clone(),
        execution_fidelity: order.execution_fidelity,
    }
}

/// Count the open trades that still consume one queue slot under the current config.
fn count_queue_relevant_open_trades(
    db: &Database,
    config: &Config,
    now_ms: u64,
) -> anyhow::Result<u64> {
    if config
        .pending_settlement_policy_unchecked()
        .counts_as_open_position
    {
        db.count_open_trades()
    } else {
        db.count_active_open_trades(now_ms)
    }
}

/// Describe the outcome of processing one pending order.
enum OrderDisposition {
    Deferred(PendingOrder),
    Missed {
        group_id: Option<String>,
        outcome: ProcessedOrderOutcome,
    },
    Filled {
        trade: Box<SimulatedTrade>,
        group_id: Option<String>,
        outcome: ProcessedOrderOutcome,
    },
}

/// Carry the shared context needed to settle due orders.
struct DueOrderContext<'a> {
    now: u64,
    now_us: Option<u64>,
    current_window: Option<&'a MarketWindow>,
    book_state: &'a BookState,
    db: &'a Database,
    bankroll: &'a mut BankrollManager,
    config: &'a Config,
}

/// Describe the fillability snapshot captured when a queued order misses.
#[derive(Debug, Clone, Copy)]
struct MissedOrderContext {
    track_group: bool,
    reason: &'static str,
    best_ask: Option<f64>,
    ask_size: Option<f64>,
    freshness_ms: Option<u64>,
}

/// Carry the common metadata for a queued spread order.
struct SpreadOrderContext<'a> {
    market_id: &'a str,
    token_id: &'a str,
    arrival_ts: u64,
    execution_group_id: Option<String>,
    execution_fidelity: ReplayFidelity,
}

/// Carry both legs and metadata for accepted spread order queueing.
struct SpreadQueuePlan<'a> {
    up_signal: &'a Signal,
    down_signal: &'a Signal,
    up_signal_id: i64,
    down_signal_id: i64,
    window: &'a MarketWindow,
    arrival_ts: u64,
    up_tokens: f64,
    down_tokens: f64,
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

/// Persist telemetry updates for all legs in one spread bundle.
fn update_spread_signal_metrics(
    db: &Database,
    signals: &[Signal],
    signal_ids: &[i64],
    arrival_ts: Option<u64>,
    decision_status: &str,
    rejection_reason: Option<&str>,
) {
    for (signal, signal_id) in signals.iter().zip(signal_ids.iter().copied()) {
        if let Some(telemetry) = signal.telemetry.as_ref() {
            let _ = db.upsert_signal_telemetry(
                signal_id,
                telemetry,
                arrival_ts.map(|_| signal.timestamp),
                arrival_ts,
                Some(decision_status),
                rejection_reason,
            );
        }
    }
}

/// Persist one rejected single-order submission and return the operator outcome.
fn reject_single_submission(
    db: &Database,
    signal: &Signal,
    signal_id: i64,
    reason: &str,
) -> SubmissionOutcome {
    if let Some(telemetry) = signal.telemetry.as_ref() {
        let _ = db.upsert_signal_telemetry(
            signal_id,
            telemetry,
            None,
            None,
            Some("rejected"),
            Some(reason),
        );
    }
    SubmissionOutcome::Rejected {
        signal_ids: vec![signal_id],
        reason: reason.to_string(),
    }
}

/// Persist one rejected spread submission and return the operator outcome.
fn reject_spread_submission(
    db: &Database,
    signals: &[Signal],
    signal_ids: Vec<i64>,
    reason: &str,
) -> SubmissionOutcome {
    update_spread_signal_metrics(db, signals, &signal_ids, None, "rejected", Some(reason));
    SubmissionOutcome::Rejected {
        signal_ids,
        reason: reason.to_string(),
    }
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
fn book_fill_rejection_reason(
    book: &TopOfBook,
    now: u64,
    limit_price: f64,
    max_staleness_ms: u64,
) -> Option<&'static str> {
    if now.saturating_sub(effective_book_timestamp(book)) > max_staleness_ms {
        return Some("book_stale_on_arrival");
    }
    if book.best_ask <= 0.0 {
        return Some("book_unavailable_on_arrival");
    }
    if book.best_ask > limit_price {
        return Some("limit_price_not_crossed_on_arrival");
    }
    if book.ask_size <= 0.0 {
        return Some("zero_liquidity_on_arrival");
    }
    None
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

/// Return whether a newly queued order should be rejected before it ever hits the book.
fn submission_size_rejection_reason(
    requested_size: f64,
    requested_price: f64,
    market_min_size: f64,
    min_bet_usd: f64,
) -> Option<&'static str> {
    if market_min_size > 0.0 && requested_size < market_min_size {
        return Some("below_market_min_size_on_submit");
    }
    if requested_size * requested_price < min_bet_usd {
        return Some("below_min_bet_on_submit");
    }
    None
}

/// Return whether a spread bundle should be rejected before any leg is queued.
fn spread_submission_rejection_reason(
    up_size: f64,
    down_size: f64,
    up_price: f64,
    down_price: f64,
    market_min_size: f64,
    min_bet_usd: f64,
) -> Option<&'static str> {
    submission_size_rejection_reason(up_size, up_price, market_min_size, min_bet_usd).or_else(
        || submission_size_rejection_reason(down_size, down_price, market_min_size, min_bet_usd),
    )
}

/// Build the persisted trade row for a successful execution.
fn build_trade(
    order: &PendingOrder,
    fill_price: f64,
    filled_size: f64,
    now: u64,
    execution_mode: &str,
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
        execution_mode: Some(execution_mode.to_string()),
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
            fees_enabled: None,
            fee_schedule_json: None,
            token_fee_rates_json: None,
            accepting_orders: None,
            accepting_orders_timestamp: None,
            clear_book_on_start: None,
        }
    }

    /// Build a latency-arbitrage signal for the requested side.
    fn latency_signal(timestamp: u64, side: SignalDirection) -> Signal {
        Signal {
            timestamp,
            strategy: "latency-arb".to_string(),
            strategy_version: "v2".to_string(),
            feature_mode: "legacy_core".to_string(),
            direction: side,
            confidence: 1.0,
            binance_price: 68_000.0,
            chainlink_price: 68_010.0,
            up_ask: 0.55,
            down_ask: 0.45,
            up_bid: 0.54,
            down_bid: 0.44,
            expected_edge: None,
            metadata: serde_json::json!({ "momentum": 0.0012 }),
            telemetry: None,
        }
    }

    /// Build a latency-arbitrage signal that persists telemetry rows.
    fn latency_signal_with_telemetry(timestamp: u64, side: SignalDirection) -> Signal {
        let mut signal = latency_signal(timestamp, side);
        signal.telemetry = Some(crate::types::SignalTelemetry {
            generated_at_ms: timestamp,
            generated_at_us: None,
            order_submitted_at_ms: None,
            order_submitted_at_us: None,
            expected_arrival_at_ms: None,
            expected_arrival_at_us: None,
            order_processed_at_ms: None,
            order_processed_at_us: None,
            effective_arrival_delay_ms: None,
            binance_age_ms: Some(0),
            chainlink_age_ms: Some(0),
            clob_age_ms: Some(0),
            quote_age_ms: Some(0),
            book_staleness_ms: Some(0),
            expected_fee: Some(0.01),
            expected_slippage: Some(0.01),
            expected_edge: Some(0.05),
            available_feature_count: 1,
            decision_status: "generated".to_string(),
            rejection_reason: None,
            features_json: serde_json::json!({"test": true}),
        });
        signal
    }

    /// Build a calm-persistence signal that persists telemetry rows.
    fn calm_signal_with_telemetry(timestamp: u64, side: SignalDirection, ask: f64) -> Signal {
        Signal {
            timestamp,
            strategy: "calm-persistence".to_string(),
            strategy_version: "v1".to_string(),
            feature_mode: "raw_event".to_string(),
            direction: side,
            confidence: 1.0,
            binance_price: 68_000.0,
            chainlink_price: 68_000.0,
            up_ask: if side == SignalDirection::Up {
                ask
            } else {
                0.40
            },
            down_ask: if side == SignalDirection::Down {
                ask
            } else {
                0.40
            },
            up_bid: 0.39,
            down_bid: 0.39,
            expected_edge: Some(0.12),
            metadata: serde_json::json!({ "expectedEdge": 0.12 }),
            telemetry: Some(crate::types::SignalTelemetry {
                generated_at_ms: timestamp,
                generated_at_us: None,
                order_submitted_at_ms: None,
                order_submitted_at_us: None,
                expected_arrival_at_ms: None,
                expected_arrival_at_us: None,
                order_processed_at_ms: None,
                order_processed_at_us: None,
                effective_arrival_delay_ms: None,
                binance_age_ms: Some(0),
                chainlink_age_ms: Some(0),
                clob_age_ms: Some(0),
                quote_age_ms: Some(0),
                book_staleness_ms: Some(0),
                expected_fee: Some(0.01),
                expected_slippage: Some(0.01),
                expected_edge: Some(0.12),
                available_feature_count: 1,
                decision_status: "generated".to_string(),
                rejection_reason: None,
                features_json: serde_json::json!({"test": true}),
            }),
        }
    }

    /// Build the two spread-capture signals for one timestamp.
    fn spread_signals(timestamp: u64, up_ask: f64, down_ask: f64) -> Vec<Signal> {
        vec![
            Signal {
                timestamp,
                strategy: "spread-capture".to_string(),
                strategy_version: "v2".to_string(),
                feature_mode: "legacy_core".to_string(),
                direction: SignalDirection::Up,
                confidence: 1.0,
                binance_price: 68_000.0,
                chainlink_price: 68_000.0,
                up_ask,
                down_ask,
                up_bid: up_ask - 0.01,
                down_bid: down_ask - 0.01,
                expected_edge: None,
                metadata: serde_json::json!({ "spread": up_ask + down_ask }),
                telemetry: None,
            },
            Signal {
                timestamp,
                strategy: "spread-capture".to_string(),
                strategy_version: "v2".to_string(),
                feature_mode: "legacy_core".to_string(),
                direction: SignalDirection::Down,
                confidence: 1.0,
                binance_price: 68_000.0,
                chainlink_price: 68_000.0,
                up_ask,
                down_ask,
                up_bid: up_ask - 0.01,
                down_bid: down_ask - 0.01,
                expected_edge: None,
                metadata: serde_json::json!({ "spread": up_ask + down_ask }),
                telemetry: None,
            },
        ]
    }

    /// Build the two spread-capture signals for one timestamp and persist telemetry rows.
    fn spread_signals_with_telemetry(timestamp: u64, up_ask: f64, down_ask: f64) -> Vec<Signal> {
        let mut signals = spread_signals(timestamp, up_ask, down_ask);
        for signal in &mut signals {
            signal.telemetry = Some(crate::types::SignalTelemetry {
                generated_at_ms: timestamp,
                generated_at_us: None,
                order_submitted_at_ms: None,
                order_submitted_at_us: None,
                expected_arrival_at_ms: None,
                expected_arrival_at_us: None,
                order_processed_at_ms: None,
                order_processed_at_us: None,
                effective_arrival_delay_ms: None,
                binance_age_ms: Some(0),
                chainlink_age_ms: Some(0),
                clob_age_ms: Some(0),
                quote_age_ms: Some(0),
                book_staleness_ms: Some(0),
                expected_fee: Some(0.01),
                expected_slippage: Some(0.01),
                expected_edge: Some(0.05),
                available_feature_count: 1,
                decision_status: "generated".to_string(),
                rejection_reason: None,
                features_json: serde_json::json!({"test": true}),
            });
        }
        signals
    }

    /// Unwrap one queued single-order submission and return the persisted signal id.
    fn expect_single_queued(outcome: SubmissionOutcome) -> i64 {
        match outcome {
            SubmissionOutcome::Queued { signal_ids, .. } => {
                assert_eq!(signal_ids.len(), 1);
                signal_ids[0]
            }
            SubmissionOutcome::Rejected { reason, .. } => {
                panic!("expected queued submission, got rejected: {reason}")
            }
        }
    }

    /// Unwrap one rejected submission and return the persisted rejection reason.
    fn expect_rejected_reason(outcome: SubmissionOutcome) -> String {
        match outcome {
            SubmissionOutcome::Queued { signal_ids, .. } => {
                panic!("expected rejected submission, got queued: {signal_ids:?}")
            }
            SubmissionOutcome::Rejected { reason, .. } => reason,
        }
    }

    /// Unwrap one queued spread submission and return both persisted signal ids.
    fn expect_batch_queued(outcome: SubmissionOutcome) -> Vec<i64> {
        match outcome {
            SubmissionOutcome::Queued { signal_ids, .. } => signal_ids,
            SubmissionOutcome::Rejected { reason, .. } => {
                panic!("expected queued spread submission, got rejected: {reason}")
            }
        }
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
                observed_at_ms: timestamp,
            }),
            down: None,
        }
    }

    /// Build a one-sided UP book with independent source and observed timestamps.
    fn up_book_with_observed(
        best_ask: f64,
        ask_size: f64,
        timestamp: u64,
        observed_at_ms: u64,
    ) -> BookState {
        BookState {
            up: Some(TopOfBook {
                best_bid: best_ask - 0.01,
                best_ask,
                bid_size: ask_size,
                ask_size,
                timestamp,
                observed_at_ms,
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
        let signal = latency_signal_with_telemetry(10_000, SignalDirection::Up);

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
                None,
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
    /// Verify that shadow-paper fills persist the readonly execution mode.
    fn process_due_orders_persists_live_readonly_execution_mode() {
        let (_dir, db) = temp_db();
        let mut config = test_config();
        config.execution_mode = crate::config::ExecutionMode::LiveReadonly;
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(15_000, SignalDirection::Up);

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
                15_250,
                None,
                Some(&window),
                &up_book(0.55, 100.0, 15_250),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert_eq!(opened.len(), 1);
        let execution_mode: String = db
            .conn()
            .query_row(
                "SELECT execution_mode FROM simulated_trades LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(execution_mode, "live_readonly");
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
        let signal = latency_signal_with_telemetry(20_000, SignalDirection::Up);

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
                None,
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
        let outcome = engine
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
        let signal_ids = match outcome {
            SubmissionOutcome::Rejected { signal_ids, reason } => {
                assert_eq!(reason, "net_edge");
                signal_ids
            }
            SubmissionOutcome::Queued { signal_ids, .. } => {
                panic!("expected net-edge rejection, got queued: {signal_ids:?}")
            }
        };

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
        let signals = spread_signals_with_telemetry(40_000, 0.49, 0.48);

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
                None,
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

    #[test]
    /// Verify that executor fillability uses observed freshness when the raw source timestamp is zero.
    fn process_due_orders_uses_observed_book_freshness() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(50_000, SignalDirection::Up);

        engine
            .submit_single(
                &signal,
                &window,
                &db,
                &mut bankroll,
                &config,
                &clock,
                ReplayFidelity::RawEvent,
            )
            .unwrap();

        let opened = engine
            .process_due_orders(
                50_250,
                Some(50_250_000),
                Some(&window),
                &up_book_with_observed(0.55, 100.0, 0, 50_250),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert_eq!(opened.len(), 1);
        assert_eq!(engine.stats().filled_orders, 1);
    }

    #[test]
    /// Verify that processed orders fail loudly when the signal-metrics row is missing.
    fn process_due_orders_errors_when_signal_metric_row_is_missing() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal(55_000, SignalDirection::Up);

        engine
            .submit_single(
                &signal,
                &window,
                &db,
                &mut bankroll,
                &config,
                &clock,
                ReplayFidelity::RawEvent,
            )
            .unwrap();

        let error = engine
            .process_due_orders(
                55_250,
                Some(55_250_000),
                Some(&window),
                &up_book_with_observed(0.55, 100.0, 0, 55_250),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap_err();

        assert!(error.chain().any(|cause| {
            cause
                .to_string()
                .contains("expected exactly one signal_metrics row")
        }));
    }

    #[test]
    /// Verify that post-submission misses update signal telemetry with an explicit reason.
    fn process_due_orders_marks_zero_liquidity_signals_as_missed() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(60_000, SignalDirection::Up);

        let signal_id = expect_single_queued(
            engine
                .submit_single(
                    &signal,
                    &window,
                    &db,
                    &mut bankroll,
                    &config,
                    &clock,
                    ReplayFidelity::RawEvent,
                )
                .unwrap(),
        );

        let opened = engine
            .process_due_orders(
                60_250,
                Some(60_250_000),
                Some(&window),
                &up_book(0.55, 0.0, 60_250),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert!(opened.is_empty());
        let metric: (String, Option<String>, Option<u64>) = db
            .conn()
            .query_row(
                "SELECT decision_status, rejection_reason, effective_arrival_delay_ms
                 FROM signal_metrics
                 WHERE signal_id = ?1",
                rusqlite::params![signal_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(metric.0, "missed");
        assert_eq!(metric.1.as_deref(), Some("zero_liquidity_on_arrival"));
        assert_eq!(metric.2, Some(250));
    }

    #[test]
    /// Verify that successful fills update signal telemetry and emit a filled outcome.
    fn process_due_orders_marks_filled_signals_and_records_delay() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(70_000, SignalDirection::Up);

        let signal_id = expect_single_queued(
            engine
                .submit_single(
                    &signal,
                    &window,
                    &db,
                    &mut bankroll,
                    &config,
                    &clock,
                    ReplayFidelity::RawEvent,
                )
                .unwrap(),
        );

        let opened = engine
            .process_due_orders(
                70_275,
                Some(70_275_000),
                Some(&window),
                &up_book_with_observed(0.55, 100.0, 0, 70_275),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert_eq!(opened.len(), 1);
        let metric: (String, Option<String>, Option<u64>, Option<u64>) = db
            .conn()
            .query_row(
                "SELECT decision_status, rejection_reason, order_processed_at_ms, effective_arrival_delay_ms
                 FROM signal_metrics
                 WHERE signal_id = ?1",
                rusqlite::params![signal_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(metric.0, "filled");
        assert_eq!(metric.1, None);
        assert_eq!(metric.2, Some(70_275));
        assert_eq!(metric.3, Some(275));
    }

    #[test]
    /// Verify that duplicate pending spread bundles are labeled explicitly.
    fn submit_spread_labels_duplicate_pending_order_rejections() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signals = spread_signals_with_telemetry(80_000, 0.49, 0.48);

        let queued_ids = expect_batch_queued(
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
                .unwrap(),
        );
        assert_eq!(queued_ids.len(), 2);

        let rejected_reason = expect_rejected_reason(
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
                .unwrap(),
        );
        assert_eq!(rejected_reason, "duplicate_pending_order");
    }

    #[test]
    /// Verify that duplicate open positions are labeled explicitly.
    fn submit_single_labels_duplicate_open_position_rejections() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        db.open_trade(&SimulatedTrade {
            id: None,
            timestamp: 10_000,
            market_id: window.market_id.clone(),
            strategy: "latency-arb".to_string(),
            side: SignalDirection::Up,
            token_id: window.up_token_id.clone(),
            entry_price: 0.55,
            size: 10.0,
            status: TradeStatus::Open,
            signal_id: None,
            requested_price: None,
            requested_size: None,
            filled_size: None,
            avg_fill_price: None,
            fill_status: None,
            fill_reason: None,
            fill_latency_ms: None,
            execution_group_id: None,
            execution_fidelity: None,
            execution_mode: None,
            order_id: None,
            fill_price: None,
        })
        .unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(81_000, SignalDirection::Up);

        let rejected_reason = expect_rejected_reason(
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
                .unwrap(),
        );
        assert_eq!(rejected_reason, "duplicate_open_position");
    }

    #[test]
    /// Verify that true position-cap breaches are labeled explicitly.
    fn submit_single_labels_max_open_position_rejections() {
        let (_dir, db) = temp_db();
        let mut config = test_config();
        config.max_open_positions = 1;
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        db.open_trade(&SimulatedTrade {
            id: None,
            timestamp: 10_000,
            market_id: window.market_id.clone(),
            strategy: "other".to_string(),
            side: SignalDirection::Up,
            token_id: window.up_token_id.clone(),
            entry_price: 0.55,
            size: 10.0,
            status: TradeStatus::Open,
            signal_id: None,
            requested_price: None,
            requested_size: None,
            filled_size: None,
            avg_fill_price: None,
            fill_status: None,
            fill_reason: None,
            fill_latency_ms: None,
            execution_group_id: None,
            execution_fidelity: None,
            execution_mode: None,
            order_id: None,
            fill_price: None,
        })
        .unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(82_000, SignalDirection::Up);

        let rejected_reason = expect_rejected_reason(
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
                .unwrap(),
        );
        assert_eq!(rejected_reason, "max_open_positions");
    }

    #[test]
    /// Verify that closed-but-unsettled trades stop consuming queue slots when configured.
    fn submit_single_ignores_pending_settlement_trades_for_open_position_cap() {
        let (_dir, db) = temp_db();
        let mut config = test_config();
        config.max_open_positions = 1;
        config.pending_settlement_counts_as_open_position = false;
        let clock = BacktestClock::new();
        clock.set(400_000);

        let mut pending_window = test_window();
        pending_window.market_id = "mkt-old".to_string();
        pending_window.up_token_id = "tok-old-up".to_string();
        pending_window.down_token_id = "tok-old-down".to_string();
        pending_window.end_time = 300_000;
        db.upsert_market(&pending_window).unwrap();
        db.open_trade(&SimulatedTrade {
            id: None,
            timestamp: 10_000,
            market_id: pending_window.market_id.clone(),
            strategy: "other".to_string(),
            side: SignalDirection::Up,
            token_id: pending_window.up_token_id.clone(),
            entry_price: 0.55,
            size: 10.0,
            status: TradeStatus::Open,
            signal_id: None,
            requested_price: None,
            requested_size: None,
            filled_size: None,
            avg_fill_price: None,
            fill_status: None,
            fill_reason: None,
            fill_latency_ms: None,
            execution_group_id: None,
            execution_fidelity: None,
            execution_mode: None,
            order_id: None,
            fill_price: None,
        })
        .unwrap();

        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(1_000.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(82_000, SignalDirection::Up);

        let submission = engine
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
        assert!(matches!(submission, SubmissionOutcome::Queued { .. }));
    }

    #[test]
    /// Verify that too-small single orders are rejected before queue with an explicit reason.
    fn submit_single_rejects_below_min_bet_before_queue() {
        let (_dir, db) = temp_db();
        let mut config = test_config();
        config.min_bet_usd = 5.0;
        config.max_position_fraction = 0.10;
        config.max_position_usd_fraction = 1.0;
        config.max_position_usd = 1.0;
        config.min_balance_threshold = 1.0;
        config.peak_dd_pause_pct = 1.0;
        config.max_drawdown_pct = 1.0;
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(20.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(83_000, SignalDirection::Up);

        let rejected_reason = expect_rejected_reason(
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
                .unwrap(),
        );
        assert_eq!(rejected_reason, "below_min_bet_on_submit");
    }

    #[test]
    /// Verify that calm single-order submissions use the calm-specific max ask.
    fn submit_single_uses_calm_specific_limit_price() {
        let (_dir, db) = temp_db();
        let mut config = test_config();
        config.calm_persistence_max_ask = 0.75;
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = calm_signal_with_telemetry(84_000, SignalDirection::Up, 0.72);

        let signal_id = expect_single_queued(
            engine
                .submit_single(
                    &signal,
                    &window,
                    &db,
                    &mut bankroll,
                    &config,
                    &clock,
                    ReplayFidelity::RawEvent,
                )
                .unwrap(),
        );

        let opened = engine
            .process_due_orders(
                84_250,
                Some(84_250_000),
                Some(&window),
                &up_book(0.72, 100.0, 84_250),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert_eq!(opened.len(), 1);
        let metric: (String, Option<String>) = db
            .conn()
            .query_row(
                "SELECT decision_status, rejection_reason FROM signal_metrics WHERE signal_id = ?1",
                rusqlite::params![signal_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(metric.0, "filled");
        assert_eq!(metric.1, None);
    }

    #[test]
    /// Verify that latency single-order submissions still honor the latency cap.
    fn submit_single_keeps_latency_limit_price_behavior() {
        let (_dir, db) = temp_db();
        let config = test_config();
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(500.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signal = latency_signal_with_telemetry(85_000, SignalDirection::Up);
        let mut signal = signal;
        signal.up_ask = 0.62;
        signal.metadata = serde_json::json!({"momentum": 0.0012});

        let signal_id = expect_single_queued(
            engine
                .submit_single(
                    &signal,
                    &window,
                    &db,
                    &mut bankroll,
                    &config,
                    &clock,
                    ReplayFidelity::RawEvent,
                )
                .unwrap(),
        );

        let opened = engine
            .process_due_orders(
                85_250,
                Some(85_250_000),
                Some(&window),
                &up_book(0.62, 100.0, 85_250),
                &db,
                &mut bankroll,
                &config,
                &clock,
            )
            .unwrap();

        assert!(opened.is_empty());
        let metric: (String, Option<String>) = db
            .conn()
            .query_row(
                "SELECT decision_status, rejection_reason FROM signal_metrics WHERE signal_id = ?1",
                rusqlite::params![signal_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(metric.0, "missed");
        assert_eq!(
            metric.1.as_deref(),
            Some("limit_price_not_crossed_on_arrival")
        );
    }

    #[test]
    /// Verify that too-small spread bundles are rejected before queue with an explicit reason.
    fn submit_spread_rejects_below_min_bet_before_queue() {
        let (_dir, db) = temp_db();
        let mut config = test_config();
        config.min_bet_usd = 5.0;
        config.max_position_fraction = 0.10;
        config.max_position_usd_fraction = 1.0;
        config.max_position_usd = 1.0;
        config.min_balance_threshold = 1.0;
        config.peak_dd_pause_pct = 1.0;
        config.max_drawdown_pct = 1.0;
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(20.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signals = spread_signals_with_telemetry(84_000, 0.30, 0.30);

        let rejected_reason = expect_rejected_reason(
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
                .unwrap(),
        );
        assert_eq!(rejected_reason, "below_min_bet_on_submit");
    }

    #[test]
    /// Verify that raising only the spread-specific cap can turn a rejected spread into a queued one.
    fn submit_spread_can_queue_with_spread_specific_fraction() {
        let (_dir, db) = temp_db();
        let mut config = test_config();
        config.min_bet_usd = 5.0;
        config.max_position_fraction = 0.10;
        config.spread_capture_max_position_fraction = Some(0.60);
        config.max_position_usd_fraction = 1.0;
        config.max_position_usd = 1_000.0;
        config.min_balance_threshold = 1.0;
        config.peak_dd_pause_pct = 1.0;
        config.max_drawdown_pct = 1.0;
        let clock = BacktestClock::new();
        let window = test_window();
        db.upsert_market(&window).unwrap();

        let mut bankroll = BankrollManager::new(20.0, &config, &db, &clock);
        let mut engine = ExecutionEngine::new();
        let signals = spread_signals_with_telemetry(85_000, 0.30, 0.30);

        let queued_ids = expect_batch_queued(
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
                .unwrap(),
        );

        assert_eq!(queued_ids.len(), 2);
        assert_eq!(engine.stats().submitted_orders, 2);
    }
}
