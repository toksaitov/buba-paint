use std::collections::VecDeque;

use serde_json::json;

use crate::bankroll::{BankrollManager, BankrollStats};
use crate::clock::Clock;
use crate::config::{Config, ExecutionMode};
use crate::db::database::{OpenTradeSnapshot, UnresolvedTradeExposure};
use crate::executor::{
    ExecutionStats, OrderOutcomeDisposition, ProcessedOrderOutcome, QueueRejectionReason,
    QueuedOrderIntent, SubmissionOutcome,
};
use crate::fees::{compute_taker_fee, resolve_fee_params, spread_net_edge};
use crate::live_persistence_writer::{
    LivePersistenceEvent, balance_event_for_trade, market_closed_event,
};
use crate::portfolio::{
    PortfolioRegime, StrategyFamily, detect_regime, select_family_for_candidates,
};
use crate::rejection_diagnostics::StrategyRejectionTracker;
use crate::signal_features::effective_book_timestamp;
use crate::strategies::{Strategy, StrategyResult};
use crate::strategy_cycle::{
    annotate_signal, assess_spread_batch_affordability, spread_budget_rejection_sample,
};
use crate::trend_tracker::ScopedTrendTracker;
use crate::types::{
    BookState, MarketWindow, ReplayFidelity, Signal, SignalDirection, SimulatedTrade,
    StrategyContext, StrategyRejection, StrategyRejectionReason, StrategyRejectionSample,
    TopOfBook, TradeResult, TradeStatus,
};

/// Runtime seed values loaded before the decision worker starts.
pub(crate) struct RuntimeDecisionSeed {
    pub starting_balance: f64,
    pub current_balance: f64,
    pub unresolved_exposures: Vec<UnresolvedTradeExposure>,
    pub open_trades: Vec<OpenTradeSnapshot>,
    pub now_ms: u64,
}

/// One pure strategy evaluation request.
pub(crate) struct RuntimeDecisionRequest {
    pub ctx: StrategyContext,
    pub window: MarketWindow,
    pub book_state: BookState,
    pub now_ms: u64,
    pub now_us: Option<u64>,
    pub live_trading_can_submit: bool,
}

/// Output emitted by one pure decision pass.
#[derive(Debug, Clone)]
pub(crate) struct RuntimeDecisionOutput {
    pub persistence_events: Vec<LivePersistenceEvent>,
    pub live_orders: Vec<QueuedOrderIntent>,
    pub processed_outcomes: Vec<ProcessedOrderOutcome>,
    pub log_events: Vec<RuntimeDecisionLogEvent>,
    pub submission_window: Option<MarketWindow>,
    pub now_ms: u64,
}

/// Terminal venue feedback for one live order known to have filled.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LiveOrderFillFeedback {
    pub signal_id: i64,
    pub fill_price: f64,
    pub filled_size: f64,
}

/// Operator-visible strategy events emitted without persistence side effects.
#[derive(Debug, Clone)]
pub(crate) enum RuntimeDecisionLogEvent {
    Suppressed {
        strategy: String,
        direction: SignalDirection,
        regime: PortfolioRegime,
    },
    SingleSubmitted {
        strategy: String,
        direction: SignalDirection,
        regime: PortfolioRegime,
        outcome: SubmissionOutcome,
    },
    BatchSubmitted {
        strategy: String,
        count: usize,
        regime: PortfolioRegime,
        outcome: SubmissionOutcome,
    },
}

/// In-memory decision engine used by paper, readonly, and disarmed live runtime.
pub(crate) struct RuntimeDecisionEngine {
    config: Config,
    strategies: Vec<Box<dyn Strategy>>,
    pending_orders: VecDeque<RuntimePendingOrder>,
    exposure: InMemoryExposureState,
    bankroll: BankrollManager,
    circuit_breaker: crate::circuit_breaker::CircuitBreaker,
    trend_tracker: ScopedTrendTracker,
    rejection_tracker: StrategyRejectionTracker,
    stats: ExecutionStats,
    next_signal_id: i64,
    next_trade_id: i64,
    next_group_id: u64,
}

#[derive(Debug, Clone)]
struct RuntimePendingOrder {
    signal_id: i64,
    signal_timestamp: u64,
    market_id: String,
    market_end_time: u64,
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
struct InMemoryExposureState {
    open: Vec<OpenRuntimeTrade>,
}

#[derive(Debug, Clone)]
struct OpenRuntimeTrade {
    trade: SimulatedTrade,
    market_end_time: u64,
    pending_settlement: bool,
}

#[derive(Debug)]
struct EvaluatedRuntimeCandidate {
    family: StrategyFamily,
    result: StrategyResult,
}

#[derive(Debug, Clone)]
enum RuntimeOrderDisposition {
    Deferred(RuntimePendingOrder),
    Missed {
        group_id: Option<String>,
        outcome: ProcessedOrderOutcome,
        event: Box<LivePersistenceEvent>,
    },
    Filled {
        trade: Box<SimulatedTrade>,
        group_id: Option<String>,
        outcome: ProcessedOrderOutcome,
        signal_event: Box<LivePersistenceEvent>,
        trade_event: Box<LivePersistenceEvent>,
    },
}

#[derive(Debug, Clone, Copy)]
struct RuntimeMissContext {
    track_group: bool,
    reason: &'static str,
    best_ask: Option<f64>,
    ask_size: Option<f64>,
    freshness_ms: Option<u64>,
}

struct DecisionClock {
    now_ms: u64,
}

impl Clock for DecisionClock {
    /// Return the decision timestamp for sizing and pause checks.
    fn now(&self) -> u64 {
        self.now_ms
    }
}

impl RuntimeDecisionEngine {
    /// Build a pure runtime decision engine from preloaded startup state.
    pub(crate) fn new(
        config: Config,
        strategies: Vec<Box<dyn Strategy>>,
        seed: RuntimeDecisionSeed,
    ) -> Self {
        let mut bankroll =
            BankrollManager::new_in_memory(seed.starting_balance, seed.current_balance, &config);
        bankroll.hydrate_unresolved_exposure_rows(&seed.unresolved_exposures, &config, seed.now_ms);
        Self {
            circuit_breaker: crate::circuit_breaker::CircuitBreaker::new(
                config.circuit_breaker_losses as u32,
                config.circuit_breaker_pause_ms,
            ),
            trend_tracker: ScopedTrendTracker::new(
                config.trend_filter_window as usize,
                config.trend_filter_enabled,
                config.trend_filter_threshold,
                config.trend_filter_per_strategy,
            ),
            exposure: InMemoryExposureState::from_snapshots(seed.open_trades, seed.now_ms),
            config,
            strategies,
            pending_orders: VecDeque::new(),
            bankroll,
            rejection_tracker: StrategyRejectionTracker::new(),
            stats: ExecutionStats::default(),
            next_signal_id: -1,
            next_trade_id: -1,
            next_group_id: 1,
        }
    }

    /// Evaluate one feed-derived strategy context without storage or network calls.
    pub(crate) fn evaluate(&mut self, request: RuntimeDecisionRequest) -> RuntimeDecisionOutput {
        let mut persistence_events = Vec::new();
        let mut processed_outcomes = if self.config.execution_mode == ExecutionMode::LiveTrading {
            Vec::new()
        } else {
            self.process_due_orders(&request, &mut persistence_events)
        };
        let mut live_orders = Vec::new();
        let mut log_events = Vec::new();

        if !self.circuit_breaker.can_trade(request.now_ms) {
            self.circuit_breaker.log_if_paused(request.now_ms);
            return Self::output(
                persistence_events,
                live_orders,
                processed_outcomes,
                log_events,
                Some(request.window),
                request.now_ms,
            );
        }
        if self.config.execution_mode == ExecutionMode::LiveTrading
            && !request.live_trading_can_submit
        {
            return Self::output(
                persistence_events,
                live_orders,
                processed_outcomes,
                log_events,
                Some(request.window),
                request.now_ms,
            );
        }

        let observed_regime = detect_regime(&request.ctx, &self.config);
        let candidates = self.collect_candidates(&request);
        let candidate_families = candidates
            .iter()
            .map(|candidate| candidate.family)
            .collect::<Vec<_>>();
        let selected_family = if self.config.regime_detection_enabled {
            select_family_for_candidates(&request.ctx, &self.config, &candidate_families)
                .selected_family
        } else {
            None
        };

        for candidate in candidates {
            if self.config.regime_detection_enabled && Some(candidate.family) != selected_family {
                self.record_router_block(&request, candidate.family);
                continue;
            }
            match candidate.result {
                StrategyResult::Single(signal) => self.handle_single_candidate(
                    &request,
                    *signal,
                    candidate.family,
                    observed_regime,
                    &mut persistence_events,
                    &mut live_orders,
                    &mut log_events,
                ),
                StrategyResult::Batch(signals) => self.handle_batch_candidate(
                    &request,
                    signals,
                    candidate.family,
                    observed_regime,
                    &mut persistence_events,
                    &mut live_orders,
                    &mut log_events,
                ),
                StrategyResult::None | StrategyResult::Rejected(_) => {}
            }
        }

        processed_outcomes.extend(Vec::<ProcessedOrderOutcome>::new());
        Self::output(
            persistence_events,
            live_orders,
            processed_outcomes,
            log_events,
            Some(request.window),
            request.now_ms,
        )
    }

    /// Mark one market closed without reading storage.
    pub(crate) fn window_closed(
        &mut self,
        window: &MarketWindow,
        now_ms: u64,
    ) -> RuntimeDecisionOutput {
        let mut persistence_events = vec![market_closed_event(window)];
        for trade in self.exposure.mark_pending_for_market(&window.market_id) {
            self.bankroll.transition_trade_to_pending_settlement(
                trade.entry_price * trade.size,
                &trade.strategy,
            );
        }
        let summaries = self
            .rejection_tracker
            .drain_market(&window.market_id, now_ms);
        if !summaries.is_empty() {
            persistence_events.push(LivePersistenceEvent::RejectionSummaries(summaries));
        }
        Self::output(
            persistence_events,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            now_ms,
        )
    }

    /// Apply one authoritative resolution to in-memory state and emit persistence work.
    pub(crate) fn authoritative_resolution(
        &mut self,
        window: &MarketWindow,
        outcome: SignalDirection,
        now_ms: u64,
    ) -> RuntimeDecisionOutput {
        let mut persistence_events = vec![LivePersistenceEvent::MarketResolved {
            market_id: window.market_id.clone(),
            outcome: outcome.to_string(),
        }];
        let trades = self.exposure.remove_market(&window.market_id);
        let fee_params = resolve_fee_params(&self.config, Some(window), now_ms);
        for trade in trades {
            let Some(trade_id) = trade.id else {
                continue;
            };
            let settlement_price = if trade.side == outcome { 1.0 } else { 0.0 };
            let fee_amount = compute_taker_fee(
                trade.entry_price,
                trade.size,
                fee_params.fee_rate,
                fee_params.exponent,
            );
            let pnl = self.bankroll.apply_trade_result_in_memory(
                trade.entry_price,
                trade.size,
                settlement_price,
                fee_amount,
                &trade.strategy,
            );
            let won = pnl > 0.0;
            self.trend_tracker.record_outcome(
                family_for_strategy(&trade.strategy),
                trade.side,
                won,
                now_ms,
            );
            self.circuit_breaker.record_result(won, now_ms);
            let result = TradeResult {
                trade_id,
                exit_price: settlement_price,
                settlement_price,
                pnl_0pct: settlement_price * trade.size - trade.entry_price * trade.size,
                pnl_1pct: 0.0,
                pnl_2pct: 0.0,
                pnl_3pct: 0.0,
                fee_amount,
                pnl_net: pnl,
                settlement_status: "confirmed".to_string(),
                provisional_pnl: None,
            };
            let balance = balance_event_for_trade(
                trade_id,
                now_ms,
                pnl,
                self.bankroll.get_stats().current_balance,
            );
            persistence_events.push(LivePersistenceEvent::CloseTrade {
                trade_id,
                result: Box::new(result),
                balance,
            });
        }
        Self::output(
            persistence_events,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            now_ms,
        )
    }

    /// Release live submission reservations after worker-side venue rejection.
    pub(crate) fn release_reservations(&mut self, releases: Vec<(String, f64)>) {
        for (strategy, amount) in releases {
            self.bankroll
                .release_reserved_for_strategy(amount, &strategy);
        }
    }

    /// Apply terminal live-submission feedback to in-memory exposure state.
    pub(crate) fn apply_live_submission_feedback(
        &mut self,
        fills: &[LiveOrderFillFeedback],
        rejected_signal_ids: &[i64],
        now_ms: u64,
    ) {
        for signal_id in rejected_signal_ids {
            let _ = self.remove_pending_order(*signal_id);
        }
        for fill in fills {
            let Some(order) = self.remove_pending_order(fill.signal_id) else {
                continue;
            };
            if fill.filled_size <= 0.0 || fill.fill_price <= 0.0 {
                continue;
            }
            let fill_cost = fill.filled_size * fill.fill_price;
            if order.reserved_cost > fill_cost {
                self.bankroll.release_reserved_for_strategy(
                    order.reserved_cost - fill_cost,
                    &order.strategy,
                );
            }
            let trade_id = self.allocate_trade_id();
            let trade = build_trade(
                &order,
                trade_id,
                fill.fill_price,
                fill.filled_size,
                now_ms,
                "live",
            );
            self.record_fill(&order, fill.fill_price, fill.filled_size, now_ms);
            self.exposure.add_open_trade(trade, order.market_end_time);
        }
    }

    /// Flush all accumulated rejection summaries.
    pub(crate) fn flush_all(&mut self, now_ms: u64) -> RuntimeDecisionOutput {
        let summaries = self.rejection_tracker.drain_all(now_ms);
        let persistence_events = if summaries.is_empty() {
            Vec::new()
        } else {
            vec![LivePersistenceEvent::RejectionSummaries(summaries)]
        };
        Self::output(
            persistence_events,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            now_ms,
        )
    }

    /// Return current bankroll statistics.
    pub(crate) fn stats(&self) -> BankrollStats {
        self.bankroll.get_stats()
    }

    /// Collect actionable strategy candidates for one context.
    fn collect_candidates(
        &mut self,
        request: &RuntimeDecisionRequest,
    ) -> Vec<EvaluatedRuntimeCandidate> {
        let mut candidates = Vec::new();
        for strategy in &mut self.strategies {
            match strategy.evaluate(&request.ctx, &self.config, request.now_ms) {
                StrategyResult::None => {}
                StrategyResult::Rejected(rejection) => self
                    .rejection_tracker
                    .record(&request.window.market_id, &rejection),
                StrategyResult::Single(signal) => candidates.push(EvaluatedRuntimeCandidate {
                    family: strategy.family(),
                    result: StrategyResult::Single(signal),
                }),
                StrategyResult::Batch(signals) => candidates.push(EvaluatedRuntimeCandidate {
                    family: strategy.family(),
                    result: StrategyResult::Batch(signals),
                }),
            }
        }
        candidates
    }

    /// Record one router-blocked candidate without persisting synchronously.
    fn record_router_block(&mut self, request: &RuntimeDecisionRequest, family: StrategyFamily) {
        self.rejection_tracker.record(
            &request.window.market_id,
            &StrategyRejection::new(
                family.as_str(),
                StrategyRejectionReason::BlockedByRouter,
                StrategyRejectionSample::from_ctx(&request.ctx),
            ),
        );
    }

    /// Handle one single-side strategy candidate.
    #[allow(clippy::too_many_arguments)]
    fn handle_single_candidate(
        &mut self,
        request: &RuntimeDecisionRequest,
        mut signal: Signal,
        family: StrategyFamily,
        regime: PortfolioRegime,
        persistence_events: &mut Vec<LivePersistenceEvent>,
        live_orders: &mut Vec<QueuedOrderIntent>,
        log_events: &mut Vec<RuntimeDecisionLogEvent>,
    ) {
        annotate_signal(&mut signal, regime, family);
        let signal_id = self.allocate_signal_id();
        if self.trend_tracker.should_suppress(family, signal.direction) {
            persistence_events.push(decision_signal_event(
                signal_id,
                signal.clone(),
                request,
                ReplayFidelity::RawEvent,
                None,
                None,
                "suppressed",
                Some("trend_filter"),
            ));
            log_events.push(RuntimeDecisionLogEvent::Suppressed {
                strategy: signal.strategy,
                direction: signal.direction,
                regime,
            });
            return;
        }
        if family == StrategyFamily::CalmPersistence
            && let Some(reason) =
                self.can_queue_single(&signal, &request.window, false, 1, request.now_ms)
            && let Some(rejection_reason) = calm_duplicate_rejection_reason(reason)
        {
            self.rejection_tracker.record(
                &request.window.market_id,
                &StrategyRejection::new(
                    signal.strategy.as_str(),
                    rejection_reason,
                    calm_duplicate_rejection_sample(&request.ctx, &signal),
                ),
            );
            persistence_events.push(decision_signal_event(
                signal_id,
                signal,
                request,
                ReplayFidelity::RawEvent,
                None,
                None,
                "rejected",
                Some(reason.as_str()),
            ));
            return;
        }
        let submission = self.submit_single(request, signal, signal_id, family, persistence_events);
        if let SubmissionOutcome::Queued { orders, .. } = &submission
            && self.config.execution_mode == ExecutionMode::LiveTrading
        {
            live_orders.extend(orders.clone());
        }
        let (strategy, direction) = submission_label(&submission);
        log_events.push(RuntimeDecisionLogEvent::SingleSubmitted {
            strategy,
            direction,
            regime,
            outcome: submission,
        });
    }

    /// Handle one spread batch candidate.
    #[allow(clippy::too_many_arguments)]
    fn handle_batch_candidate(
        &mut self,
        request: &RuntimeDecisionRequest,
        mut signals: Vec<Signal>,
        family: StrategyFamily,
        regime: PortfolioRegime,
        persistence_events: &mut Vec<LivePersistenceEvent>,
        live_orders: &mut Vec<QueuedOrderIntent>,
        log_events: &mut Vec<RuntimeDecisionLogEvent>,
    ) {
        for signal in &mut signals {
            annotate_signal(signal, regime, family);
        }
        let strategy_name = signals
            .first()
            .map_or_else(|| "?".to_string(), |signal| signal.strategy.clone());
        if let Some(affordability) = assess_spread_batch_affordability(
            &signals,
            &request.window,
            &self.bankroll,
            &self.config,
        ) && !affordability.queueable
        {
            self.rejection_tracker.record(
                &request.window.market_id,
                &StrategyRejection::new(
                    strategy_name.as_str(),
                    StrategyRejectionReason::SpreadBudgetTooSmall,
                    spread_budget_rejection_sample(&request.ctx, &signals, affordability),
                ),
            );
            return;
        }
        let signal_ids = (0..signals.len())
            .map(|_| self.allocate_signal_id())
            .collect::<Vec<_>>();
        let submission = self.submit_spread(request, &signals, &signal_ids, persistence_events);
        if let SubmissionOutcome::Queued { orders, .. } = &submission
            && self.config.execution_mode == ExecutionMode::LiveTrading
        {
            live_orders.extend(orders.clone());
        }
        log_events.push(RuntimeDecisionLogEvent::BatchSubmitted {
            strategy: strategy_name,
            count: signals.len(),
            regime,
            outcome: submission,
        });
    }

    /// Submit one single-side candidate into in-memory queues.
    fn submit_single(
        &mut self,
        request: &RuntimeDecisionRequest,
        signal: Signal,
        signal_id: i64,
        family: StrategyFamily,
        persistence_events: &mut Vec<LivePersistenceEvent>,
    ) -> SubmissionOutcome {
        if let Some(reason) =
            self.can_queue_single(&signal, &request.window, false, 1, request.now_ms)
        {
            persistence_events.push(decision_signal_event(
                signal_id,
                signal.clone(),
                request,
                ReplayFidelity::RawEvent,
                None,
                None,
                "rejected",
                Some(reason.as_str()),
            ));
            return rejected(vec![signal_id], reason.as_str());
        }
        let requested_price = signal_entry_ask(&signal);
        let limit_price = single_order_limit_price(&signal, &self.config);
        let clock = DecisionClock {
            now_ms: request.now_ms,
        };
        let requested_size = self.bankroll.reserve_capital_with_reserve_price(
            requested_price,
            limit_price,
            signal.confidence,
            &signal.strategy,
            family,
            &self.config,
            &clock,
        );
        if requested_size <= 0.0 {
            persistence_events.push(decision_signal_event(
                signal_id,
                signal,
                request,
                ReplayFidelity::RawEvent,
                None,
                None,
                "rejected",
                Some("strategy_sleeve_exhausted"),
            ));
            return rejected(vec![signal_id], "strategy_sleeve_exhausted");
        }
        if let Some(reason) = submission_size_rejection_reason(
            requested_size,
            requested_price,
            request.window.order_min_size.unwrap_or(0.0),
            self.config.min_bet_usd,
        ) {
            self.bankroll
                .release_reserved_for_strategy(requested_size * limit_price, &signal.strategy);
            persistence_events.push(decision_signal_event(
                signal_id,
                signal,
                request,
                ReplayFidelity::RawEvent,
                None,
                None,
                "rejected",
                Some(reason),
            ));
            return rejected(vec![signal_id], reason);
        }
        let arrival_ts = signal
            .timestamp
            .saturating_add(self.config.sim_order_latency_ms);
        let order = RuntimePendingOrder {
            signal_id,
            signal_timestamp: signal.timestamp,
            market_id: request.window.market_id.clone(),
            market_end_time: request.window.end_time,
            strategy: signal.strategy.clone(),
            side: signal.direction,
            token_id: token_id_for_signal(&signal, &request.window),
            arrival_ts,
            requested_price,
            limit_price,
            requested_size,
            reserved_cost: requested_size * limit_price,
            execution_group_id: None,
            execution_fidelity: ReplayFidelity::RawEvent,
        };
        let queued = queued_order_intent(&order);
        self.record_submitted_order(&order);
        self.pending_orders.push_back(order);
        persistence_events.push(decision_signal_event(
            signal_id,
            signal,
            request,
            ReplayFidelity::RawEvent,
            Some(request.now_ms),
            Some(arrival_ts),
            "submitted",
            None,
        ));
        SubmissionOutcome::Queued {
            signal_ids: vec![signal_id],
            orders: vec![queued],
        }
    }

    /// Submit one spread candidate into in-memory queues.
    #[allow(clippy::too_many_lines)]
    fn submit_spread(
        &mut self,
        request: &RuntimeDecisionRequest,
        signals: &[Signal],
        signal_ids: &[i64],
        persistence_events: &mut Vec<LivePersistenceEvent>,
    ) -> SubmissionOutcome {
        let Some((up_signal, down_signal, up_signal_id, down_signal_id)) =
            spread_signal_pair(signals, signal_ids)
        else {
            return SubmissionOutcome::Queued {
                signal_ids: signal_ids.to_vec(),
                orders: Vec::new(),
            };
        };
        if let Some(reason) = self.can_queue_spread(signals, &request.window, request.now_ms) {
            Self::persist_spread_signals(
                request,
                signals,
                signal_ids,
                persistence_events,
                None,
                None,
                "rejected",
                Some(reason.as_str()),
            );
            return rejected(signal_ids.to_vec(), reason.as_str());
        }
        let fee_params =
            resolve_fee_params(&self.config, Some(&request.window), request.window.end_time);
        if spread_net_edge(
            up_signal.up_ask,
            down_signal.down_ask,
            1.0,
            fee_params.fee_rate,
            fee_params.exponent,
        ) <= 0.0
        {
            Self::persist_spread_signals(
                request,
                signals,
                signal_ids,
                persistence_events,
                None,
                None,
                "rejected",
                Some("net_edge"),
            );
            return rejected(signal_ids.to_vec(), "net_edge");
        }
        let clock = DecisionClock {
            now_ms: request.now_ms,
        };
        let (up_tokens, down_tokens) = self.bankroll.reserve_spread_capital(
            up_signal.up_ask,
            down_signal.down_ask,
            up_signal.confidence.max(down_signal.confidence),
            &self.config,
            &clock,
        );
        if up_tokens <= 0.0 || down_tokens <= 0.0 {
            Self::persist_spread_signals(
                request,
                signals,
                signal_ids,
                persistence_events,
                None,
                None,
                "rejected",
                Some("strategy_sleeve_exhausted"),
            );
            return rejected(signal_ids.to_vec(), "strategy_sleeve_exhausted");
        }
        if let Some(reason) = spread_submission_rejection_reason(
            up_tokens,
            down_tokens,
            up_signal.up_ask,
            down_signal.down_ask,
            request.window.order_min_size.unwrap_or(0.0),
            self.config.min_bet_usd,
        ) {
            let reserved = (up_tokens * up_signal.up_ask) + (down_tokens * down_signal.down_ask);
            self.bankroll
                .release_reserved_for_strategy(reserved, &up_signal.strategy);
            Self::persist_spread_signals(
                request,
                signals,
                signal_ids,
                persistence_events,
                None,
                None,
                "rejected",
                Some(reason),
            );
            return rejected(signal_ids.to_vec(), reason);
        }
        let arrival_ts = spread_arrival_ts(signals, &self.config, request.now_ms);
        let group_id = format!("spread-{}", self.next_group_id);
        self.next_group_id = self.next_group_id.saturating_add(1);
        let up_order = build_spread_order(
            up_signal,
            up_signal_id,
            &request.window,
            arrival_ts,
            up_tokens,
            Some(group_id.clone()),
        );
        let down_order = build_spread_order(
            down_signal,
            down_signal_id,
            &request.window,
            arrival_ts,
            down_tokens,
            Some(group_id),
        );
        self.record_submitted_order(&up_order);
        self.record_submitted_order(&down_order);
        let orders = vec![
            queued_order_intent(&up_order),
            queued_order_intent(&down_order),
        ];
        self.pending_orders.push_back(up_order);
        self.pending_orders.push_back(down_order);
        Self::persist_spread_signals(
            request,
            signals,
            signal_ids,
            persistence_events,
            Some(request.now_ms),
            Some(arrival_ts),
            "submitted",
            None,
        );
        SubmissionOutcome::Queued {
            signal_ids: signal_ids.to_vec(),
            orders,
        }
    }

    /// Process due paper orders without storage calls.
    fn process_due_orders(
        &mut self,
        request: &RuntimeDecisionRequest,
        persistence_events: &mut Vec<LivePersistenceEvent>,
    ) -> Vec<ProcessedOrderOutcome> {
        let mut remaining = VecDeque::new();
        let mut outcomes = Vec::new();
        let mut group_outcomes = std::collections::HashMap::<String, (u64, u64)>::new();
        while let Some(order) = self.pending_orders.pop_front() {
            let disposition = self.process_due_order(order, request);
            match disposition {
                RuntimeOrderDisposition::Deferred(order) => remaining.push_back(order),
                RuntimeOrderDisposition::Missed {
                    group_id,
                    outcome,
                    event,
                } => {
                    record_group_outcome(&mut group_outcomes, group_id.as_deref(), false);
                    persistence_events.push(*event);
                    outcomes.push(outcome);
                }
                RuntimeOrderDisposition::Filled {
                    trade,
                    group_id,
                    outcome,
                    signal_event,
                    trade_event,
                } => {
                    record_group_outcome(&mut group_outcomes, group_id.as_deref(), true);
                    self.exposure
                        .add_open_trade(*trade, request.window.end_time);
                    persistence_events.push(*signal_event);
                    persistence_events.push(*trade_event);
                    outcomes.push(outcome);
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
        outcomes
    }

    /// Process one pending paper order against the current book.
    fn process_due_order(
        &mut self,
        order: RuntimePendingOrder,
        request: &RuntimeDecisionRequest,
    ) -> RuntimeOrderDisposition {
        if order.arrival_ts > request.now_ms {
            return RuntimeOrderDisposition::Deferred(order);
        }
        if request.window.market_id != order.market_id {
            return self.reject_order(
                order,
                request,
                RuntimeMissContext {
                    track_group: false,
                    reason: "window_missing_on_arrival",
                    best_ask: None,
                    ask_size: None,
                    freshness_ms: None,
                },
            );
        }
        let Some(book) = side_book(&request.book_state, order.side) else {
            return self.reject_order(
                order,
                request,
                RuntimeMissContext {
                    track_group: true,
                    reason: "book_unavailable_on_arrival",
                    best_ask: None,
                    ask_size: None,
                    freshness_ms: None,
                },
            );
        };
        let freshness_ms = Some(
            request
                .now_ms
                .saturating_sub(effective_book_timestamp(book)),
        );
        if let Some(reason) = book_fill_rejection_reason(
            book,
            request.now_ms,
            order.limit_price,
            self.config.max_book_staleness_ms,
        ) {
            return self.reject_order(
                order,
                request,
                RuntimeMissContext {
                    track_group: true,
                    reason,
                    best_ask: Some(book.best_ask),
                    ask_size: Some(book.ask_size),
                    freshness_ms,
                },
            );
        }
        let tick_size = request.window.order_price_min_tick_size.unwrap_or(0.0);
        if tick_size > 0.0
            && (!price_is_tick_aligned(order.limit_price, tick_size)
                || !price_is_tick_aligned(book.best_ask, tick_size))
        {
            return self.reject_order(
                order,
                request,
                RuntimeMissContext {
                    track_group: true,
                    reason: "tick_misaligned_on_arrival",
                    best_ask: Some(book.best_ask),
                    ask_size: Some(book.ask_size),
                    freshness_ms,
                },
            );
        }
        let filled_size = order.requested_size.min(book.ask_size);
        if fill_size_rejected(
            filled_size,
            book.best_ask,
            request.window.order_min_size.unwrap_or(0.0),
            self.config.min_bet_usd,
        ) {
            return self.reject_order(
                order,
                request,
                RuntimeMissContext {
                    track_group: true,
                    reason: "fill_below_min_on_arrival",
                    best_ask: Some(book.best_ask),
                    ask_size: Some(book.ask_size),
                    freshness_ms,
                },
            );
        }
        self.fill_order(order, request, book, freshness_ms, filled_size)
    }

    /// Fill one pending order and emit persistence events.
    fn fill_order(
        &mut self,
        order: RuntimePendingOrder,
        request: &RuntimeDecisionRequest,
        book: &TopOfBook,
        freshness_ms: Option<u64>,
        filled_size: f64,
    ) -> RuntimeOrderDisposition {
        let fill_cost = filled_size * book.best_ask;
        if order.reserved_cost > fill_cost {
            self.bankroll
                .release_reserved_for_strategy(order.reserved_cost - fill_cost, &order.strategy);
        }
        self.record_fill(&order, book.best_ask, filled_size, request.now_ms);
        let trade_id = self.allocate_trade_id();
        let trade = build_trade(
            &order,
            trade_id,
            book.best_ask,
            filled_size,
            request.now_ms,
            self.config.execution_mode.as_str(),
        );
        RuntimeOrderDisposition::Filled {
            trade: Box::new(trade.clone()),
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
                effective_arrival_delay_ms: request.now_ms.saturating_sub(order.signal_timestamp),
                partial_fill: filled_size < order.requested_size,
            },
            signal_event: Box::new(LivePersistenceEvent::SignalOutcome {
                signal_id: order.signal_id,
                processed_at_ms: request.now_ms,
                processed_at_us: request.now_us,
                effective_arrival_delay_ms: request.now_ms.saturating_sub(order.signal_timestamp),
                decision_status: "filled".to_string(),
                rejection_reason: None,
            }),
            trade_event: Box::new(LivePersistenceEvent::OpenTrade(Box::new(trade))),
        }
    }

    /// Reject one pending order and emit persistence events.
    fn reject_order(
        &mut self,
        order: RuntimePendingOrder,
        request: &RuntimeDecisionRequest,
        miss: RuntimeMissContext,
    ) -> RuntimeOrderDisposition {
        self.bankroll
            .release_reserved_for_strategy(order.reserved_cost, &order.strategy);
        self.stats.no_fills += 1;
        let effective_arrival_delay_ms = request.now_ms.saturating_sub(order.signal_timestamp);
        RuntimeOrderDisposition::Missed {
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
            event: Box::new(LivePersistenceEvent::SignalOutcome {
                signal_id: order.signal_id,
                processed_at_ms: request.now_ms,
                processed_at_us: request.now_us,
                effective_arrival_delay_ms,
                decision_status: "missed".to_string(),
                rejection_reason: Some(miss.reason.to_string()),
            }),
        }
    }

    /// Return any single-order queue rejection from in-memory state.
    fn can_queue_single(
        &self,
        signal: &Signal,
        window: &MarketWindow,
        is_batch: bool,
        required_slots: u64,
        now_ms: u64,
    ) -> Option<QueueRejectionReason> {
        let open_count = self.exposure.queue_relevant_count(&self.config, now_ms);
        let pending_count = self.pending_orders.len() as u64;
        if open_count + pending_count + required_slots > self.config.max_open_positions {
            return Some(QueueRejectionReason::MaxOpenPositions);
        }
        if self.exposure.duplicate_open(
            &window.market_id,
            &signal.strategy,
            signal.direction,
            is_batch,
        ) {
            return Some(QueueRejectionReason::DuplicateOpenPosition);
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
        duplicate_pending.then_some(QueueRejectionReason::DuplicatePendingOrder)
    }

    /// Return any spread queue rejection from in-memory state.
    fn can_queue_spread(
        &self,
        signals: &[Signal],
        window: &MarketWindow,
        now_ms: u64,
    ) -> Option<QueueRejectionReason> {
        let open_count = self.exposure.queue_relevant_count(&self.config, now_ms);
        let pending_count = self.pending_orders.len() as u64;
        if open_count + pending_count + 2 > self.config.max_open_positions {
            return Some(QueueRejectionReason::MaxOpenPositions);
        }
        if signals.iter().any(|signal| {
            self.exposure.duplicate_open(
                &window.market_id,
                &signal.strategy,
                signal.direction,
                true,
            )
        }) {
            return Some(QueueRejectionReason::DuplicateOpenPosition);
        }
        let duplicate_pending = signals.iter().any(|signal| {
            self.pending_orders.iter().any(|order| {
                order.market_id == window.market_id
                    && order.strategy == signal.strategy
                    && order.side == signal.direction
            })
        });
        duplicate_pending.then_some(QueueRejectionReason::DuplicatePendingOrder)
    }

    /// Persist spread signal evidence through the writer event stream.
    #[allow(clippy::too_many_arguments)]
    fn persist_spread_signals(
        request: &RuntimeDecisionRequest,
        signals: &[Signal],
        signal_ids: &[i64],
        persistence_events: &mut Vec<LivePersistenceEvent>,
        order_submitted_at_ms: Option<u64>,
        expected_arrival_at_ms: Option<u64>,
        decision_status: &str,
        rejection_reason: Option<&str>,
    ) {
        for (signal, signal_id) in signals.iter().zip(signal_ids.iter().copied()) {
            persistence_events.push(decision_signal_event(
                signal_id,
                signal.clone(),
                request,
                ReplayFidelity::RawEvent,
                order_submitted_at_ms,
                expected_arrival_at_ms,
                decision_status,
                rejection_reason,
            ));
        }
    }

    /// Record aggregate submitted-order counters.
    fn record_submitted_order(&mut self, order: &RuntimePendingOrder) {
        self.stats.submitted_orders += 1;
        self.stats.total_requested_size += order.requested_size;
    }

    /// Remove one pending order by signal id.
    fn remove_pending_order(&mut self, signal_id: i64) -> Option<RuntimePendingOrder> {
        let position = self
            .pending_orders
            .iter()
            .position(|order| order.signal_id == signal_id)?;
        self.pending_orders.remove(position)
    }

    /// Record aggregate fill counters.
    fn record_fill(
        &mut self,
        order: &RuntimePendingOrder,
        fill_price: f64,
        filled_size: f64,
        now_ms: u64,
    ) {
        if filled_size < order.requested_size {
            self.stats.partial_fills += 1;
        }
        self.stats.filled_orders += 1;
        self.stats.total_filled_size += filled_size;
        self.stats.total_slippage += (fill_price - order.requested_price).max(0.0) * filled_size;
        self.stats.total_fill_latency_ms += now_ms.saturating_sub(order.signal_timestamp);
    }

    /// Allocate one negative runtime signal id.
    fn allocate_signal_id(&mut self) -> i64 {
        let id = self.next_signal_id;
        self.next_signal_id = self.next_signal_id.saturating_sub(1);
        id
    }

    /// Allocate one negative runtime trade id.
    fn allocate_trade_id(&mut self) -> i64 {
        let id = self.next_trade_id;
        self.next_trade_id = self.next_trade_id.saturating_sub(1);
        id
    }

    /// Build one output with current bankroll stats.
    fn output(
        persistence_events: Vec<LivePersistenceEvent>,
        live_orders: Vec<QueuedOrderIntent>,
        processed_outcomes: Vec<ProcessedOrderOutcome>,
        log_events: Vec<RuntimeDecisionLogEvent>,
        submission_window: Option<MarketWindow>,
        now_ms: u64,
    ) -> RuntimeDecisionOutput {
        RuntimeDecisionOutput {
            persistence_events,
            live_orders,
            processed_outcomes,
            log_events,
            submission_window,
            now_ms,
        }
    }
}

impl InMemoryExposureState {
    /// Build exposure state from storage snapshots loaded before runtime starts.
    fn from_snapshots(snapshots: Vec<OpenTradeSnapshot>, now_ms: u64) -> Self {
        Self {
            open: snapshots
                .into_iter()
                .map(|snapshot| OpenRuntimeTrade {
                    pending_settlement: snapshot.market_end_time <= now_ms,
                    trade: snapshot.trade,
                    market_end_time: snapshot.market_end_time,
                })
                .collect(),
        }
    }

    /// Add one newly filled paper trade.
    fn add_open_trade(&mut self, trade: SimulatedTrade, market_end_time: u64) {
        self.open.push(OpenRuntimeTrade {
            trade,
            market_end_time,
            pending_settlement: false,
        });
    }

    /// Count queue-relevant open trades.
    fn queue_relevant_count(&self, config: &Config, now_ms: u64) -> u64 {
        if config
            .pending_settlement_policy_unchecked()
            .counts_as_open_position
        {
            return self.open.len() as u64;
        }
        self.open
            .iter()
            .filter(|entry| !entry.pending_settlement && entry.market_end_time > now_ms)
            .count() as u64
    }

    /// Return whether one open trade duplicates a candidate.
    fn duplicate_open(
        &self,
        market_id: &str,
        strategy: &str,
        side: SignalDirection,
        is_batch: bool,
    ) -> bool {
        self.open.iter().any(|entry| {
            entry.trade.market_id == market_id
                && entry.trade.strategy == strategy
                && (!is_batch || entry.trade.side == side)
        })
    }

    /// Mark all open trades in one market as pending settlement.
    fn mark_pending_for_market(&mut self, market_id: &str) -> Vec<SimulatedTrade> {
        let mut trades = Vec::new();
        for entry in &mut self.open {
            if entry.trade.market_id == market_id && !entry.pending_settlement {
                entry.pending_settlement = true;
                trades.push(entry.trade.clone());
            }
        }
        trades
    }

    /// Remove all open trades for one resolved market.
    fn remove_market(&mut self, market_id: &str) -> Vec<SimulatedTrade> {
        let mut removed = Vec::new();
        self.open.retain(|entry| {
            if entry.trade.market_id == market_id {
                removed.push(entry.trade.clone());
                false
            } else {
                true
            }
        });
        removed
    }
}

/// Infer one family from a strategy string.
fn family_for_strategy(strategy: &str) -> StrategyFamily {
    StrategyFamily::from_strategy_name(strategy).unwrap_or(StrategyFamily::LatencyArb)
}

/// Return the fill-side ask price from one signal.
fn signal_entry_ask(signal: &Signal) -> f64 {
    match signal.direction {
        SignalDirection::Up => signal.up_ask,
        SignalDirection::Down => signal.down_ask,
    }
}

/// Resolve the single-order limit price used by the runtime decision engine.
fn single_order_limit_price(signal: &Signal, config: &Config) -> f64 {
    match family_for_strategy(&signal.strategy) {
        StrategyFamily::CalmPersistence => config.calm_persistence_max_ask,
        StrategyFamily::LatencyArb | StrategyFamily::SpreadCapture => config.latency_arb_max_ask,
    }
}

/// Resolve the token id used by one signal.
fn token_id_for_signal(signal: &Signal, window: &MarketWindow) -> String {
    match signal.direction {
        SignalDirection::Up => window.up_token_id.clone(),
        SignalDirection::Down => window.down_token_id.clone(),
    }
}

/// Build one persistence event with compact decision evidence.
#[allow(clippy::too_many_arguments)]
fn decision_signal_event(
    signal_id: i64,
    signal: Signal,
    request: &RuntimeDecisionRequest,
    execution_fidelity: ReplayFidelity,
    order_submitted_at_ms: Option<u64>,
    expected_arrival_at_ms: Option<u64>,
    decision_status: &str,
    rejection_reason: Option<&str>,
) -> LivePersistenceEvent {
    signal_event(
        signal_id,
        signal_with_decision_evidence(signal, request, decision_status, rejection_reason),
        &request.window.market_id,
        execution_fidelity,
        order_submitted_at_ms,
        expected_arrival_at_ms,
        decision_status,
        rejection_reason,
    )
}

/// Attach compact decision evidence to a signal without changing the schema.
fn signal_with_decision_evidence(
    mut signal: Signal,
    request: &RuntimeDecisionRequest,
    status: &str,
    reason: Option<&str>,
) -> Signal {
    let evidence = decision_evidence_json(&signal, request, status, reason);
    if let Some(metadata) = signal.metadata.as_object_mut() {
        metadata.insert("decisionEvidence".to_string(), evidence);
    } else {
        signal.metadata = json!({
            "originalMetadata": signal.metadata,
            "decisionEvidence": evidence,
        });
    }
    signal
}

/// Build one persistence event for signal evidence.
#[allow(clippy::too_many_arguments)]
fn signal_event(
    signal_id: i64,
    signal: Signal,
    market_id: &str,
    execution_fidelity: ReplayFidelity,
    order_submitted_at_ms: Option<u64>,
    expected_arrival_at_ms: Option<u64>,
    decision_status: &str,
    rejection_reason: Option<&str>,
) -> LivePersistenceEvent {
    LivePersistenceEvent::Signal {
        signal_id,
        signal: Box::new(signal),
        market_id: market_id.to_string(),
        execution_fidelity,
        order_submitted_at_ms,
        expected_arrival_at_ms,
        decision_status: decision_status.to_string(),
        rejection_reason: rejection_reason.map(str::to_string),
    }
}

/// Build one rejected submission outcome.
fn rejected(signal_ids: Vec<i64>, reason: &str) -> SubmissionOutcome {
    SubmissionOutcome::Rejected {
        signal_ids,
        reason: reason.to_string(),
    }
}

/// Extract a reasonable log label from a submission.
fn submission_label(outcome: &SubmissionOutcome) -> (String, SignalDirection) {
    match outcome {
        SubmissionOutcome::Queued { orders, .. } => orders.first().map_or_else(
            || ("?".to_string(), SignalDirection::Up),
            |order| (order.strategy.clone(), order.side),
        ),
        SubmissionOutcome::Rejected { .. } => ("?".to_string(), SignalDirection::Up),
    }
}

/// Map duplicate queue reasons to calm strategy diagnostics.
fn calm_duplicate_rejection_reason(
    reason: QueueRejectionReason,
) -> Option<StrategyRejectionReason> {
    match reason {
        QueueRejectionReason::DuplicateOpenPosition => {
            Some(StrategyRejectionReason::DuplicateOpenPosition)
        }
        QueueRejectionReason::DuplicatePendingOrder => {
            Some(StrategyRejectionReason::DuplicatePendingOrder)
        }
        QueueRejectionReason::MaxOpenPositions => None,
    }
}

/// Build one calm duplicate rejection sample.
fn calm_duplicate_rejection_sample(
    ctx: &StrategyContext,
    signal: &Signal,
) -> StrategyRejectionSample {
    let mut sample = StrategyRejectionSample::from_ctx(ctx);
    sample.up_ask = Some(signal.up_ask);
    sample.down_ask = Some(signal.down_ask);
    sample.expected_edge = signal.expected_edge;
    sample
}

/// Return the spread up/down pair and corresponding signal ids.
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

/// Calculate the arrival timestamp for a spread bundle.
fn spread_arrival_ts(signals: &[Signal], config: &Config, now_ms: u64) -> u64 {
    signals
        .iter()
        .map(|signal| signal.timestamp.saturating_add(config.sim_order_latency_ms))
        .max()
        .unwrap_or_else(|| now_ms.saturating_add(config.sim_order_latency_ms))
}

/// Build one spread leg order.
fn build_spread_order(
    signal: &Signal,
    signal_id: i64,
    window: &MarketWindow,
    arrival_ts: u64,
    requested_size: f64,
    execution_group_id: Option<String>,
) -> RuntimePendingOrder {
    let requested_price = signal_entry_ask(signal);
    RuntimePendingOrder {
        signal_id,
        signal_timestamp: signal.timestamp,
        market_id: window.market_id.clone(),
        market_end_time: window.end_time,
        strategy: signal.strategy.clone(),
        side: signal.direction,
        token_id: token_id_for_signal(signal, window),
        arrival_ts,
        requested_price,
        limit_price: requested_price,
        requested_size,
        reserved_cost: requested_size * requested_price,
        execution_group_id,
        execution_fidelity: ReplayFidelity::RawEvent,
    }
}

/// Convert one runtime order into a live submission intent.
fn queued_order_intent(order: &RuntimePendingOrder) -> QueuedOrderIntent {
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

/// Return whether one order size is illegal before queueing.
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

/// Return whether one spread bundle is illegal before queueing.
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

/// Return the top of book for one side.
fn side_book(book_state: &BookState, side: SignalDirection) -> Option<&TopOfBook> {
    match side {
        SignalDirection::Up => book_state.up.as_ref(),
        SignalDirection::Down => book_state.down.as_ref(),
    }
}

/// Return why the current book cannot fill one taker order.
fn book_fill_rejection_reason(
    book: &TopOfBook,
    now_ms: u64,
    limit_price: f64,
    max_staleness_ms: u64,
) -> Option<&'static str> {
    if now_ms.saturating_sub(effective_book_timestamp(book)) > max_staleness_ms {
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

/// Return whether a computed fill violates market or min-bet limits.
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

/// Build one in-memory and persisted paper trade row.
fn build_trade(
    order: &RuntimePendingOrder,
    trade_id: i64,
    fill_price: f64,
    filled_size: f64,
    now_ms: u64,
    execution_mode: &str,
) -> SimulatedTrade {
    SimulatedTrade {
        id: Some(trade_id),
        timestamp: now_ms,
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
        fill_latency_ms: Some(now_ms.saturating_sub(order.signal_timestamp)),
        execution_group_id: order.execution_group_id.clone(),
        execution_fidelity: Some(order.execution_fidelity.to_string()),
        execution_mode: Some(execution_mode.to_string()),
        order_id: Some(format!("paper-{}", order.signal_id)),
        fill_price: Some(fill_price),
    }
}

/// Track spread group fill outcomes.
fn record_group_outcome(
    outcomes: &mut std::collections::HashMap<String, (u64, u64)>,
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

/// Return whether one price is aligned to the market tick size.
fn price_is_tick_aligned(price: f64, tick_size: f64) -> bool {
    if tick_size <= 0.0 {
        return true;
    }
    let units = price / tick_size;
    (units - units.round()).abs() < 1e-9
}

/// Build compact decision evidence for future live-fidelity debugging.
fn decision_evidence_json(
    signal: &Signal,
    request: &RuntimeDecisionRequest,
    status: &str,
    reason: Option<&str>,
) -> serde_json::Value {
    json!({
        "strategy": signal.strategy,
        "side": signal.direction.to_string(),
        "market_id": request.window.market_id,
        "decision_at_ms": request.now_ms,
        "features": request.ctx.features.to_json(),
        "book_state": {
            "up": request.ctx.book_state.up.as_ref().map(top_of_book_json),
            "down": request.ctx.book_state.down.as_ref().map(top_of_book_json),
        },
        "status": status,
        "reason": reason,
    })
}

/// Convert one top-of-book snapshot into JSON evidence.
fn top_of_book_json(book: &TopOfBook) -> serde_json::Value {
    json!({
        "best_bid": book.best_bid,
        "best_ask": book.best_ask,
        "bid_size": book.bid_size,
        "ask_size": book.ask_size,
        "timestamp": book.timestamp,
        "observed_at_ms": book.observed_at_ms,
    })
}

#[cfg(test)]
#[path = "tests/live_decision_tests.rs"]
mod tests;
