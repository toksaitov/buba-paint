use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde_json::json;
use tracing::{error, info, warn};

use crate::backtest::momentum::MomentumCalculator;
use crate::bankroll::BankrollStats;
use crate::clock::{Clock, SystemClock};
use crate::config::{Config, FeedEventStorageProfile};
use crate::db::database::{Database, LiveDecisionEvidence};
use crate::executor::{
    OrderOutcomeDisposition, ProcessedOrderOutcome, QueuedOrderIntent, SubmissionOutcome,
};
use crate::feeds::FeedMessage;
use crate::feeds::util::now_us;
use crate::live_control::{LiveControlAction, record_live_control_state};
use crate::live_decision::{
    LiveOrderFillFeedback, RuntimeDecisionEngine, RuntimeDecisionLogEvent, RuntimeDecisionOutput,
    RuntimeDecisionRequest, RuntimeDecisionSeed,
};
use crate::live_feed_writer::{FeedEventWriter, FeedEventWriterConfig, FeedEventWriterSnapshot};
use crate::live_persistence_writer::{
    LivePersistenceEvent, LivePersistenceWriter, LivePersistenceWriterConfig,
    LivePersistenceWriterSnapshot,
};
use crate::live_sidecar::{
    LiveAccountState, LiveActivityResponse, LiveCheckStatus, LiveOrderIntentRequest,
    LiveOrderIntentResponse, LivePreflightResponse, LiveSidecarClient,
};
use crate::live_storage::FeedEventStorageState;
use crate::market_discovery::{self, MarketDiscoveryEvent};
use crate::signal_features::{SignalFeatureEngine, SignalState};
use crate::strategies::Strategy;
use crate::strategies::calm_persistence::CalmPersistenceStrategy;
use crate::strategies::latency_arb::LatencyArbStrategy;
use crate::strategies::spread_capture::SpreadCaptureStrategy;
use crate::tick_logger::{self, TickLoggerState};
use crate::types::{
    FeedEvent, FeedHealthEvent, LiveAccountSnapshot, LiveFill, LiveOrder, LiveOrderIntent,
    LiveReconciliationEvent, LiveRedemption, LiveSession, MarketWindow, ReplayFidelity,
    SignalDirection, StrategyContext,
};

struct PendingResolution {
    market_id: String,
    window: MarketWindow,
    next_attempt_at_ms: u64,
    seeded_from_startup: bool,
}

struct LiveState {
    signal_state: SignalState,
    current_window: Option<MarketWindow>,
    window_open_prices: std::collections::HashMap<String, f64>,
    known_windows: std::collections::HashMap<String, MarketWindow>,
    latest_decision_sequence: u64,
}

struct LiveTradingRuntimeBootstrap {
    live_starting_balance: f64,
    monitor: LiveTradingMonitor,
}

/// Feed-derived decision state awaiting one latest-state evaluation.
#[derive(Debug, Default)]
struct PendingDecisionEvaluation {
    dirty: bool,
    latest_input_at_ms: u64,
    latest_input_at_us: Option<u64>,
    dirty_events: u64,
    coalesced_events: u64,
    flushed_evaluations: u64,
}

impl PendingDecisionEvaluation {
    /// Mark the current feed state as needing one decision evaluation.
    fn mark_dirty(&mut self, observed_at_ms: u64, observed_at_micros: Option<u64>) {
        if self.dirty {
            self.coalesced_events = self.coalesced_events.saturating_add(1);
        }
        self.dirty = true;
        self.latest_input_at_ms = observed_at_ms;
        self.latest_input_at_us = observed_at_micros;
        self.dirty_events = self.dirty_events.saturating_add(1);
    }

    /// Return the pending evaluation timestamp if a decision is required.
    fn take(&mut self) -> Option<(u64, Option<u64>)> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        self.flushed_evaluations = self.flushed_evaluations.saturating_add(1);
        Some((self.latest_input_at_ms, self.latest_input_at_us))
    }
}

#[derive(Clone)]
struct LiveTradingMonitor {
    sidecar: LiveSidecarClient,
    session_id: i64,
    state: String,
    preflight: Option<LivePreflightResponse>,
    account: Option<LiveAccountState>,
    activity: Option<LiveActivityResponse>,
    risk: Option<LiveRiskMonitor>,
    degradation: LiveDegradationTracker,
    blocked_reason: Option<String>,
    finished: bool,
}

#[derive(Debug, Clone)]
struct LiveRiskMonitor {
    session_start_equity: f64,
    day_index: u64,
    day_baseline_equity: f64,
    high_water_mark: f64,
    trough_equity: f64,
    current_equity: f64,
    session_drawdown_usd: f64,
    daily_loss_usd: f64,
    terminal_reason: Option<String>,
    terminal_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct LiveRiskBreach {
    event_type: &'static str,
    reason: String,
    details: serde_json::Value,
}

#[derive(Debug, Clone)]
struct LiveDegradation {
    kind: String,
    started_at_ms: u64,
    latest_detail: String,
}

#[derive(Debug, Clone)]
struct LiveDegradationBreach {
    kind: String,
    duration_ms: u64,
    detail: String,
}

#[derive(Debug, Clone, Default)]
struct LiveDegradationTracker {
    active: Option<LiveDegradation>,
}

/// Build the enabled live strategy list for the current configuration.
fn build_strategies(config: &Config) -> Vec<Box<dyn Strategy>> {
    let mut strategies: Vec<Box<dyn Strategy>> = Vec::new();
    if config.latency_arb_enabled {
        strategies.push(Box::new(LatencyArbStrategy::new(
            config.latency_arb_momentum_threshold,
        )));
    }
    if config.spread_capture_enabled {
        strategies.push(Box::new(SpreadCaptureStrategy::new()));
    }
    if config.calm_persistence_enabled {
        strategies.push(Box::new(CalmPersistenceStrategy::new()));
    }
    strategies
}

impl StrategyWorker {
    /// Start one pure strategy worker outside the feed hot path.
    fn start(
        config: Config,
        strategies: Vec<Box<dyn Strategy>>,
        seed: RuntimeDecisionSeed,
        output_tx: tokio::sync::mpsc::Sender<StrategyWorkerOutput>,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = sync_channel(config.live_decision_queue_capacity.max(1));
        let latest = Arc::new(LatestStrategyEvaluation::default());
        let metrics = Arc::new(StrategyWorkerMetrics::default());
        let stats = Arc::new(std::sync::RwLock::new(initial_bankroll_stats(
            seed.starting_balance,
        )));
        let worker_latest = Arc::clone(&latest);
        let worker_metrics = Arc::clone(&metrics);
        let worker_stats = Arc::clone(&stats);
        let join = thread::Builder::new()
            .name("buba-strategy-worker".to_string())
            .spawn(move || {
                run_strategy_worker(StrategyWorkerRuntime {
                    config,
                    strategies,
                    seed,
                    rx,
                    latest: worker_latest,
                    metrics: worker_metrics,
                    stats: worker_stats,
                    output_tx,
                });
            })
            .context("spawning strategy worker")?;
        Ok(Self {
            tx,
            latest,
            metrics,
            stats,
            join: Some(join),
        })
    }

    /// Enqueue one strategy-evaluation request without blocking feed handling.
    fn try_evaluate(&self, request: StrategyEvaluationRequest) -> bool {
        if self.metrics.output_dropped.load(Ordering::Relaxed) > 0 {
            self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let Ok(mut latest) = self.latest.request.lock() else {
            self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        };
        if latest.replace(Box::new(request)).is_some() {
            self.metrics.replaced.fetch_add(1, Ordering::Relaxed);
        }
        self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
        self.latest.wake.notify_one();
        true
    }

    /// Enqueue one market-close event without blocking feed handling.
    fn try_window_closed(
        &self,
        window: MarketWindow,
        _open_price: f64,
        _close_price: f64,
        now_ms: u64,
    ) -> bool {
        self.try_send(StrategyWorkerMessage::WindowClosed { window, now_ms })
    }

    /// Enqueue one authoritative settlement result.
    fn try_authoritative_resolution(
        &self,
        window: MarketWindow,
        outcome: SignalDirection,
        _seeded_from_startup: bool,
    ) -> bool {
        self.try_send(StrategyWorkerMessage::AuthoritativeResolution { window, outcome })
    }

    /// Enqueue reserve releases requested by the live submission worker.
    fn try_release_reservations(&self, releases: Vec<(String, f64)>) -> bool {
        self.try_send(StrategyWorkerMessage::ReleaseReservations { releases })
    }

    /// Enqueue terminal live-submission feedback for in-memory exposure state.
    fn try_apply_live_submission_feedback(
        &self,
        fills: Vec<LiveOrderFillFeedback>,
        rejected_signal_ids: Vec<i64>,
        now_ms: u64,
    ) -> bool {
        self.try_send(StrategyWorkerMessage::LiveSubmissionFeedback {
            fills,
            rejected_signal_ids,
            now_ms,
        })
    }

    /// Ask the worker to flush all pending rejection summaries.
    fn try_flush_all(&self, now_ms: u64) -> bool {
        self.try_send(StrategyWorkerMessage::FlushAll { now_ms })
    }

    /// Return a snapshot of worker counters and the latest bankroll stats.
    fn snapshot(&self) -> StrategyWorkerSnapshot {
        let stats = self
            .stats
            .read()
            .map_or_else(|_| initial_bankroll_stats(0.0), |stats| stats.clone());
        StrategyWorkerSnapshot {
            enqueued: self.metrics.enqueued.load(Ordering::Relaxed),
            replaced: self.metrics.replaced.load(Ordering::Relaxed),
            dropped: self.metrics.dropped.load(Ordering::Relaxed),
            processed: self.metrics.processed.load(Ordering::Relaxed),
            output_dropped: self.metrics.output_dropped.load(Ordering::Relaxed),
            last_processed_at_ms: self.metrics.last_processed_at_ms.load(Ordering::Relaxed),
            shutdown_timed_out: self.metrics.shutdown_timed_out.load(Ordering::Relaxed),
            stats,
        }
    }

    /// Request shutdown and wait up to the supplied timeout.
    fn shutdown_with_timeout(&mut self, timeout: Duration) -> bool {
        let _ = self.tx.try_send(StrategyWorkerMessage::Shutdown);
        let (replacement_tx, _replacement_rx) = sync_channel(1);
        let original_tx = std::mem::replace(&mut self.tx, replacement_tx);
        drop(original_tx);
        let Some(join) = self.join.take() else {
            return true;
        };
        if join_with_timeout(join, timeout, "strategy worker") {
            true
        } else {
            self.metrics
                .shutdown_timed_out
                .fetch_add(1, Ordering::Relaxed);
            warn!("strategy worker shutdown timed out; worker left detached");
            false
        }
    }

    /// Enqueue one worker message without blocking the caller.
    fn try_send(&self, message: StrategyWorkerMessage) -> bool {
        match self.tx.try_send(message) {
            Ok(()) => {
                self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
                self.latest.wake.notify_one();
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

/// Join one runtime worker thread without waiting past the closeout budget.
fn join_with_timeout(join: thread::JoinHandle<()>, timeout: Duration, label: &str) -> bool {
    let started = Instant::now();
    while !join.is_finished() {
        if started.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
    if let Err(error) = join.join() {
        warn!(
            ?error,
            worker = label,
            "runtime worker panicked during shutdown"
        );
    }
    true
}

impl LiveSubmissionQueue {
    /// Start one asynchronous live submission worker.
    fn start(
        db_path: String,
        config: Config,
        session_id: i64,
        sidecar: LiveSidecarClient,
        feedback_tx: tokio::sync::mpsc::UnboundedSender<LiveSubmissionFeedback>,
    ) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<LiveSubmissionRequest>(
            config.live_submission_queue_capacity.max(1),
        );
        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                let feedback = submit_live_orders_from_worker(
                    &db_path, &config, session_id, &sidecar, request,
                )
                .await;
                if feedback_tx.send(feedback).is_err() {
                    warn!("live submission feedback receiver dropped");
                    break;
                }
            }
        });
        Self { tx }
    }

    /// Queue one live submission batch without blocking the feed loop.
    fn try_submit(&self, request: LiveSubmissionRequest) -> bool {
        self.tx.try_send(request).is_ok()
    }
}

/// Submit one live order batch outside the feed hot path.
async fn submit_live_orders_from_worker(
    db_path: &str,
    config: &Config,
    session_id: i64,
    sidecar: &LiveSidecarClient,
    request: LiveSubmissionRequest,
) -> LiveSubmissionFeedback {
    let mut feedback = LiveSubmissionFeedback {
        state_update: None,
        releases: Vec::new(),
        fills: Vec::new(),
        rejected_signal_ids: Vec::new(),
        now_ms: request.now_ms,
    };
    let mut successful = 0_u64;
    for (index, order) in request.orders.iter().enumerate() {
        let decision_event =
            critical_signal_event_for_order(&request.critical_events, order.signal_id);
        match submit_one_live_order_from_worker(LiveOrderWorkerSubmission {
            db_path,
            config,
            session_id,
            sidecar,
            window: &request.window,
            order,
            decision_event,
            now_ms: request.now_ms,
        })
        .await
        {
            LiveSubmissionOrderResult::Filled {
                accepted_size,
                fill_price,
            } => {
                successful += 1;
                feedback.fills.push(LiveOrderFillFeedback {
                    signal_id: order.signal_id,
                    fill_price,
                    filled_size: accepted_size,
                });
            }
            LiveSubmissionOrderResult::Rejected { release } => {
                feedback.releases.push((order.strategy.clone(), release));
                feedback.rejected_signal_ids.push(order.signal_id);
            }
            LiveSubmissionOrderResult::Blocked {
                reason,
                release,
                state,
            } => {
                if let Some(release) = release {
                    feedback.releases.push((order.strategy.clone(), release));
                    feedback.rejected_signal_ids.push(order.signal_id);
                }
                release_unsubmitted_orders_after_block(&mut feedback, &request.orders, index + 1);
                feedback.state_update = Some(LiveSubmissionStateUpdate { state, reason });
                break;
            }
        }
    }
    if feedback.state_update.is_none()
        && request.orders.len() == 2
        && request
            .orders
            .iter()
            .any(|order| order.strategy.as_str() == "spread-capture")
        && successful == 1
    {
        let reason = "spread residual exposure detected".to_string();
        if let Err(error) = persist_live_worker_state(
            db_path,
            session_id,
            "unknown_order",
            "system",
            &reason,
            request.now_ms,
            Some(
                json!({
                    "market_id": request.window.market_id,
                    "successful_legs": successful,
                    "submitted_legs": request.orders.len(),
                })
                .to_string(),
            )
            .as_deref(),
        ) {
            error!("failed to persist spread residual state: {error}");
        }
        feedback.state_update = Some(LiveSubmissionStateUpdate {
            state: "unknown_order",
            reason,
        });
    }
    feedback
}

/// Release not-yet-submitted reservations after a batch-level live blocker.
fn release_unsubmitted_orders_after_block(
    feedback: &mut LiveSubmissionFeedback,
    orders: &[QueuedOrderIntent],
    start_index: usize,
) {
    for order in orders.iter().skip(start_index) {
        feedback
            .releases
            .push((order.strategy.clone(), order.reserved_cost));
        feedback.rejected_signal_ids.push(order.signal_id);
    }
}

/// Result of one worker-side live order submission.
enum LiveSubmissionOrderResult {
    Filled {
        accepted_size: f64,
        fill_price: f64,
    },
    Rejected {
        release: f64,
    },
    Blocked {
        state: &'static str,
        reason: String,
        release: Option<f64>,
    },
}

/// Worker-side immutable context for one live order submission.
struct LiveOrderWorkerSubmission<'a> {
    db_path: &'a str,
    config: &'a Config,
    session_id: i64,
    sidecar: &'a LiveSidecarClient,
    window: &'a MarketWindow,
    order: &'a QueuedOrderIntent,
    decision_event: Option<&'a LivePersistenceEvent>,
    now_ms: u64,
}

/// Worker-side immutable context for live intent persistence.
struct LiveIntentPersistenceInput<'a> {
    db_path: &'a str,
    config: &'a Config,
    session_id: i64,
    window: &'a MarketWindow,
    order: &'a QueuedOrderIntent,
    decision_event: Option<&'a LivePersistenceEvent>,
    now_ms: u64,
    notional: f64,
}

/// Submit one live order intent from the submission worker.
async fn submit_one_live_order_from_worker(
    input: LiveOrderWorkerSubmission<'_>,
) -> LiveSubmissionOrderResult {
    let order = input.order;
    let notional = order.requested_price * order.requested_size;
    let (intent_id, reject_reason) =
        match persist_live_order_intent_from_worker(&LiveIntentPersistenceInput {
            db_path: input.db_path,
            config: input.config,
            session_id: input.session_id,
            window: input.window,
            order,
            decision_event: input.decision_event,
            now_ms: input.now_ms,
            notional,
        }) {
            Ok(result) => result,
            Err(error) => {
                let reason =
                    format!("failed to persist live decision evidence and intent: {error}");
                return LiveSubmissionOrderResult::Blocked {
                    state: "disarmed",
                    reason,
                    release: Some(order.reserved_cost),
                };
            }
        };
    if let Some(reason) = reject_reason {
        info!(
            intent_id,
            signal_id = order.signal_id,
            strategy = %order.strategy,
            reason,
            "live order rejected before venue submission"
        );
        return LiveSubmissionOrderResult::Rejected {
            release: order.reserved_cost,
        };
    }
    let request =
        build_live_order_request_from_worker(input.session_id, intent_id, order, notional);
    match input.sidecar.submit_order_intent(&request).await {
        Ok(response) => handle_live_order_response_from_worker(
            input.db_path,
            input.session_id,
            order,
            intent_id,
            input.now_ms,
            &response,
        ),
        Err(error) => {
            handle_live_order_error_from_worker(
                LiveOrderErrorContext {
                    db_path: input.db_path,
                    session_id: input.session_id,
                    sidecar: input.sidecar,
                    order,
                    request: &request,
                    intent_id,
                    now_ms: input.now_ms,
                },
                error,
            )
            .await
        }
    }
}

/// Persist one worker-side live order intent before sidecar submission.
fn persist_live_order_intent_from_worker(
    input: &LiveIntentPersistenceInput<'_>,
) -> anyhow::Result<(i64, Option<&'static str>)> {
    let order = input.order;
    let (intent_status, reject_reason) =
        live_order_pre_submit_rejection(order, input.notional, input.config);
    let db = Database::open_runtime(input.db_path)?;
    let decision_evidence =
        live_decision_evidence_from_event(input.decision_event, order.signal_id)?;
    let intent = LiveOrderIntent {
        id: None,
        session_id: input.session_id,
        signal_id: Some(order.signal_id),
        market_id: order.market_id.clone(),
        strategy: order.strategy.clone(),
        side: order.side.to_string(),
        order_type: "FOK".to_string(),
        status: intent_status.to_string(),
        created_at_ms: input.now_ms,
        requested_price: Some(order.requested_price),
        requested_size: Some(order.requested_size),
        limit_price: Some(order.limit_price),
        fee_schedule_json: input.window.fee_schedule_json.clone(),
        token_fee_rates_json: input.window.token_fee_rates_json.clone(),
        execution_group_id: order.execution_group_id.clone(),
        details_json: Some(
            json!({
                "signal_timestamp": order.signal_timestamp,
                "arrival_ts": order.arrival_ts,
                "execution_fidelity": order.execution_fidelity.to_string(),
                "amount_usd": input.notional,
                "reject_reason": reject_reason,
            })
            .to_string(),
        ),
    };
    let intent_id = db.log_live_order_intent_with_decision_evidence(&decision_evidence, &intent)?;
    db.close();
    Ok((intent_id, reject_reason))
}

/// Return the critical signal evidence for one live order signal id.
fn critical_signal_event_for_order(
    events: &[LivePersistenceEvent],
    signal_id: i64,
) -> Option<&LivePersistenceEvent> {
    events.iter().find(|event| {
        matches!(
            event,
            LivePersistenceEvent::Signal {
                signal_id: event_signal_id,
                ..
            } if *event_signal_id == signal_id
        )
    })
}

/// Convert a live persistence signal event into a DB-layer evidence view.
fn live_decision_evidence_from_event(
    event: Option<&LivePersistenceEvent>,
    expected_signal_id: i64,
) -> anyhow::Result<LiveDecisionEvidence<'_>> {
    let Some(LivePersistenceEvent::Signal {
        signal_id,
        signal,
        market_id,
        execution_fidelity,
        order_submitted_at_ms,
        expected_arrival_at_ms,
        decision_status,
        rejection_reason,
    }) = event
    else {
        bail!("missing critical decision evidence for signal_id={expected_signal_id}");
    };
    if *signal_id != expected_signal_id {
        bail!("critical decision evidence signal id mismatch for signal_id={expected_signal_id}");
    }
    Ok(LiveDecisionEvidence {
        signal_id: *signal_id,
        signal: signal.as_ref(),
        market_id,
        execution_fidelity: *execution_fidelity,
        order_submitted_at_ms: *order_submitted_at_ms,
        expected_arrival_at_ms: *expected_arrival_at_ms,
        decision_status,
        rejection_reason: rejection_reason.as_deref(),
    })
}

/// Build one worker-side sidecar order request.
fn build_live_order_request_from_worker(
    session_id: i64,
    intent_id: i64,
    order: &QueuedOrderIntent,
    notional: f64,
) -> LiveOrderIntentRequest {
    LiveOrderIntentRequest {
        session_id,
        intent_id,
        market_id: order.market_id.clone(),
        token_id: order.token_id.clone(),
        side: "BUY".to_string(),
        order_type: "FOK".to_string(),
        limit_price: order.limit_price,
        size: order.requested_size,
        amount_usd: Some(notional),
        client_order_id: format!("buba-live-{session_id}-{intent_id}"),
        details_json: Some(
            json!({
                "strategy": order.strategy,
                "signal_id": order.signal_id,
                "execution_group_id": order.execution_group_id,
            })
            .to_string(),
        ),
    }
}

/// Persist one worker-side live order response.
fn handle_live_order_response_from_worker(
    db_path: &str,
    session_id: i64,
    order: &QueuedOrderIntent,
    intent_id: i64,
    now_ms: u64,
    response: &LiveOrderIntentResponse,
) -> LiveSubmissionOrderResult {
    let db = match Database::open_runtime(db_path) {
        Ok(db) => db,
        Err(error) => {
            return LiveSubmissionOrderResult::Blocked {
                state: "unknown_order",
                reason: format!("failed to open database for live order response: {error}"),
                release: None,
            };
        }
    };
    let live_order_id = match persist_order_response_from_worker(
        &db, session_id, intent_id, order, now_ms, response,
    ) {
        Ok(live_order_id) => live_order_id,
        Err(error) => {
            db.close();
            return LiveSubmissionOrderResult::Blocked {
                state: "unknown_order",
                reason: format!("failed to persist live order response: {error}"),
                release: None,
            };
        }
    };
    if !response.ok {
        if live_order_response_is_blocking(response) {
            let details = response.details_json.clone().unwrap_or_else(|| {
                json!({ "status": response.status, "reason": response.status_reason }).to_string()
            });
            db.close();
            return block_live_response_with_state(
                db_path,
                session_id,
                now_ms,
                "blocking venue order response",
                Some(details.as_str()),
                Some(order.reserved_cost),
            );
        }
        db.close();
        return LiveSubmissionOrderResult::Rejected {
            release: order.reserved_cost,
        };
    }
    let accepted_size =
        match classify_accepted_live_size(db_path, session_id, order, now_ms, response) {
            Ok(Some(accepted_size)) => accepted_size,
            Ok(None) => {
                db.close();
                return LiveSubmissionOrderResult::Rejected {
                    release: order.reserved_cost,
                };
            }
            Err(blocked) => {
                db.close();
                return blocked;
            }
        };
    let fill_price = live_response_fill_price(order, response);
    if let Err(error) = persist_response_fill_from_worker(
        &db,
        session_id,
        intent_id,
        live_order_id,
        now_ms,
        response,
        fill_price,
    ) {
        error!("failed to persist live response fill: {error}");
    }
    db.close();
    LiveSubmissionOrderResult::Filled {
        accepted_size,
        fill_price,
    }
}

/// Classify the accepted live order size from a successful venue response.
fn classify_accepted_live_size(
    db_path: &str,
    session_id: i64,
    order: &QueuedOrderIntent,
    now_ms: u64,
    response: &LiveOrderIntentResponse,
) -> Result<Option<f64>, LiveSubmissionOrderResult> {
    let Some(accepted_size) = response.accepted_size else {
        let details = response.details_json.clone().unwrap_or_else(|| {
            json!({ "status": response.status, "reason": "missing accepted_size" }).to_string()
        });
        return Err(block_live_response_with_state(
            db_path,
            session_id,
            now_ms,
            "venue response missing accepted size",
            Some(details.as_str()),
            None,
        ));
    };
    if accepted_size <= 0.0 {
        return Ok(None);
    }
    if accepted_size > order.requested_size + f64::EPSILON {
        let details = response.details_json.clone().unwrap_or_else(|| {
            json!({
                "accepted_size": accepted_size,
                "requested_size": order.requested_size,
            })
            .to_string()
        });
        return Err(block_live_response_with_state(
            db_path,
            session_id,
            now_ms,
            "venue response accepted more size than requested",
            Some(details.as_str()),
            None,
        ));
    }
    Ok(Some(accepted_size))
}

/// Persist a blocked live-response state and build the worker result.
fn block_live_response_with_state(
    db_path: &str,
    session_id: i64,
    now_ms: u64,
    reason: &str,
    details: Option<&str>,
    release: Option<f64>,
) -> LiveSubmissionOrderResult {
    if let Err(error) = persist_live_worker_state(
        db_path,
        session_id,
        "unknown_order",
        "system",
        reason,
        now_ms,
        details,
    ) {
        error!("failed to persist blocked live order response state: {error}");
    }
    LiveSubmissionOrderResult::Blocked {
        state: "unknown_order",
        reason: reason.to_string(),
        release,
    }
}

/// Persist one worker-side unknown submission and try cancel-all.
async fn handle_live_order_error_from_worker(
    context: LiveOrderErrorContext<'_>,
    error: anyhow::Error,
) -> LiveSubmissionOrderResult {
    let error_message = error.to_string();
    match Database::open_runtime(context.db_path) {
        Ok(db) => {
            if let Err(error) = db.log_live_order(&LiveOrder {
                id: None,
                session_id: context.session_id,
                intent_id: context.intent_id,
                venue_order_id: None,
                client_order_id: Some(context.request.client_order_id.clone()),
                market_id: context.order.market_id.clone(),
                token_id: Some(context.order.token_id.clone()),
                side: "BUY".to_string(),
                order_type: "FOK".to_string(),
                status: "unknown_submission".to_string(),
                status_reason: Some(error_message.clone()),
                created_at_ms: context.now_ms,
                acknowledged_at_ms: None,
                updated_at_ms: context.now_ms,
                requested_price: Some(context.order.requested_price),
                limit_price: Some(context.order.limit_price),
                requested_size: Some(context.order.requested_size),
                accepted_size: None,
                details_json: Some(json!({ "error": error_message }).to_string()),
            }) {
                error!("failed to persist unknown live order: {error}");
            }
            if let Err(error) = db.log_live_reconciliation_event(&LiveReconciliationEvent {
                id: None,
                session_id: context.session_id,
                timestamp_ms: context.now_ms,
                severity: "critical".to_string(),
                event_type: "unknown_submission".to_string(),
                local_value: None,
                remote_value: None,
                details_json: Some(
                    json!({
                        "intent_id": context.intent_id,
                        "client_order_id": context.request.client_order_id,
                        "market_id": context.order.market_id,
                        "error": error_message,
                    })
                    .to_string(),
                ),
            }) {
                error!("failed to persist unknown live reconciliation event: {error}");
            }
            db.close();
        }
        Err(error) => error!("failed to open database for unknown live order: {error}"),
    }
    let cancel_result = context.sidecar.cancel_all().await;
    let cancel_details = match cancel_result {
        Ok(response) => json!({ "ok": true, "response": response }),
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    };
    let details = json!({
        "intent_id": context.intent_id,
        "client_order_id": context.request.client_order_id,
        "cancel_all": cancel_details,
    })
    .to_string();
    if let Err(error) = persist_live_worker_state(
        context.db_path,
        context.session_id,
        "unknown_order",
        "system",
        "sidecar order submission outcome unknown",
        context.now_ms,
        Some(&details),
    ) {
        error!("failed to persist unknown order state: {error}");
    }
    LiveSubmissionOrderResult::Blocked {
        state: "unknown_order",
        reason: "sidecar order submission outcome unknown".to_string(),
        release: None,
    }
}

/// Persist one live state transition from a worker.
fn persist_live_worker_state(
    db_path: &str,
    session_id: i64,
    state: &str,
    actor: &str,
    reason: &str,
    now_ms: u64,
    details_json: Option<&str>,
) -> anyhow::Result<()> {
    let db = Database::open_runtime(db_path)?;
    record_live_control_state(&db, session_id, state, actor, reason, now_ms, details_json)?;
    db.update_live_session_metadata(session_id, state, None, None, details_json)?;
    db.close();
    Ok(())
}

/// Persist one sidecar order response as a live order row.
fn persist_order_response_from_worker(
    db: &Database,
    session_id: i64,
    intent_id: i64,
    order: &QueuedOrderIntent,
    now_ms: u64,
    response: &LiveOrderIntentResponse,
) -> anyhow::Result<i64> {
    db.log_live_order(&LiveOrder {
        id: None,
        session_id,
        intent_id,
        venue_order_id: response.venue_order_id.clone(),
        client_order_id: Some(response.client_order_id.clone()),
        market_id: order.market_id.clone(),
        token_id: Some(order.token_id.clone()),
        side: "BUY".to_string(),
        order_type: "FOK".to_string(),
        status: response.status.clone(),
        status_reason: response.status_reason.clone(),
        created_at_ms: now_ms,
        acknowledged_at_ms: Some(now_ms),
        updated_at_ms: now_ms,
        requested_price: Some(order.requested_price),
        limit_price: Some(order.limit_price),
        requested_size: Some(order.requested_size),
        accepted_size: response.accepted_size,
        details_json: response.details_json.clone(),
    })
}

/// Persist one fill inferred from a successful sidecar order response.
fn persist_response_fill_from_worker(
    db: &Database,
    session_id: i64,
    intent_id: i64,
    live_order_id: i64,
    now_ms: u64,
    response: &LiveOrderIntentResponse,
    fill_price: f64,
) -> anyhow::Result<()> {
    if let Some(accepted_size) = response.accepted_size
        && accepted_size > 0.0
    {
        db.log_live_fill(&LiveFill {
            id: None,
            session_id,
            intent_id: Some(intent_id),
            live_order_id: Some(live_order_id),
            venue_trade_id: None,
            filled_at_ms: now_ms,
            price: fill_price,
            size: accepted_size,
            fee_amount: None,
            fee_rate: None,
            liquidity_side: Some("taker".to_string()),
            tx_hash: None,
            status: "venue_response_pending_reconciliation".to_string(),
            details_json: response.details_json.clone(),
        })?;
    }
    Ok(())
}

/// Run the blocking strategy worker loop on its own OS thread.
fn run_strategy_worker(runtime: StrategyWorkerRuntime) {
    let StrategyWorkerRuntime {
        config,
        strategies,
        seed,
        rx,
        latest,
        metrics,
        stats,
        output_tx,
    } = runtime;
    let clock = SystemClock;
    let mut decision_engine = RuntimeDecisionEngine::new(config.clone(), strategies, seed);
    update_strategy_worker_stats(&stats, &decision_engine.stats());
    loop {
        let mut did_work = false;
        loop {
            match rx.try_recv() {
                Ok(StrategyWorkerMessage::WindowClosed { window, now_ms }) => {
                    forward_strategy_output(
                        decision_engine.window_closed(&window, now_ms),
                        &output_tx,
                        &mut decision_engine,
                        &metrics,
                    );
                    did_work = true;
                }
                Ok(StrategyWorkerMessage::AuthoritativeResolution { window, outcome }) => {
                    forward_strategy_output(
                        decision_engine.authoritative_resolution(&window, outcome, clock.now()),
                        &output_tx,
                        &mut decision_engine,
                        &metrics,
                    );
                    did_work = true;
                }
                Ok(StrategyWorkerMessage::ReleaseReservations { releases }) => {
                    decision_engine.release_reservations(releases);
                    did_work = true;
                }
                Ok(StrategyWorkerMessage::LiveSubmissionFeedback {
                    fills,
                    rejected_signal_ids,
                    now_ms,
                }) => {
                    decision_engine.apply_live_submission_feedback(
                        &fills,
                        &rejected_signal_ids,
                        now_ms,
                    );
                    did_work = true;
                }
                Ok(StrategyWorkerMessage::FlushAll { now_ms }) => {
                    forward_strategy_output(
                        decision_engine.flush_all(now_ms),
                        &output_tx,
                        &mut decision_engine,
                        &metrics,
                    );
                    did_work = true;
                }
                Ok(StrategyWorkerMessage::Shutdown) | Err(TryRecvError::Disconnected) => {
                    forward_strategy_output(
                        decision_engine.flush_all(clock.now()),
                        &output_tx,
                        &mut decision_engine,
                        &metrics,
                    );
                    update_strategy_worker_stats(&stats, &decision_engine.stats());
                    info!("strategy worker stopped");
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        let request = latest.request.lock().ok().and_then(|mut slot| slot.take());
        if let Some(request) = request {
            let output = decision_engine.evaluate(RuntimeDecisionRequest {
                decision_sequence: request.decision_sequence,
                ctx: request.ctx,
                window: request.window,
                book_state: request.book_state,
                now_ms: request.now_ms,
                now_us: request.now_us,
                live_trading_can_submit: request.live_trading_can_submit,
            });
            forward_strategy_output(output, &output_tx, &mut decision_engine, &metrics);
            did_work = true;
        }
        if did_work {
            update_strategy_worker_stats(&stats, &decision_engine.stats());
            metrics.processed.fetch_add(1, Ordering::Relaxed);
            metrics
                .last_processed_at_ms
                .store(clock.now(), Ordering::Relaxed);
        } else if let Ok(guard) = latest.request.lock() {
            let _ = latest.wake.wait_timeout(guard, Duration::from_millis(10));
        }
    }
}

/// Forward one decision-worker output to the async runtime.
fn forward_strategy_output(
    output: RuntimeDecisionOutput,
    output_tx: &tokio::sync::mpsc::Sender<StrategyWorkerOutput>,
    decision_engine: &mut RuntimeDecisionEngine,
    metrics: &StrategyWorkerMetrics,
) {
    match output_tx.try_send(StrategyWorkerOutput::Decision(output)) {
        Ok(()) => {}
        Err(
            tokio::sync::mpsc::error::TrySendError::Full(StrategyWorkerOutput::Decision(output))
            | tokio::sync::mpsc::error::TrySendError::Closed(StrategyWorkerOutput::Decision(output)),
        ) => {
            metrics.output_dropped.fetch_add(1, Ordering::Relaxed);
            rollback_live_orders_after_worker_output_drop(decision_engine, &output);
            error!("failed to forward strategy worker output");
        }
    }
}

/// Release in-memory live reservations when worker output cannot be delivered.
fn rollback_live_orders_after_worker_output_drop(
    decision_engine: &mut RuntimeDecisionEngine,
    output: &RuntimeDecisionOutput,
) {
    if output.live_orders.is_empty() {
        return;
    }
    let rejected_signal_ids = output
        .live_orders
        .iter()
        .map(|order| order.signal_id)
        .collect::<Vec<_>>();
    let releases = output
        .live_orders
        .iter()
        .map(|order| (order.strategy.clone(), order.reserved_cost))
        .collect::<Vec<_>>();
    decision_engine.apply_live_submission_feedback(&[], &rejected_signal_ids, output.now_ms);
    decision_engine.release_reservations(releases);
}

/// Log a pure strategy decision outcome.
fn log_strategy_worker_events(events: Vec<RuntimeDecisionLogEvent>) {
    for event in events {
        match event {
            RuntimeDecisionLogEvent::Suppressed {
                strategy,
                direction,
                regime,
            } => {
                info!(
                    strategy = %strategy,
                    direction = %direction,
                    regime = regime.as_str(),
                    "signal suppressed by trend filter"
                );
            }
            RuntimeDecisionLogEvent::SingleSubmitted {
                strategy,
                direction,
                regime,
                outcome,
            } => match outcome {
                SubmissionOutcome::Queued { signal_ids, .. } => {
                    info!(
                        signal_id = signal_ids.first().copied().unwrap_or_default(),
                        strategy = %strategy,
                        direction = %direction,
                        regime = regime.as_str(),
                        "signal queued"
                    );
                }
                SubmissionOutcome::Rejected { signal_ids, reason } => {
                    info!(
                        signal_id = signal_ids.first().copied().unwrap_or_default(),
                        strategy = %strategy,
                        direction = %direction,
                        reason = %reason,
                        regime = regime.as_str(),
                        "signal rejected before queue"
                    );
                }
            },
            RuntimeDecisionLogEvent::BatchSubmitted {
                strategy,
                count,
                regime,
                outcome,
            } => match outcome {
                SubmissionOutcome::Queued { signal_ids, .. } => {
                    info!(
                        strategy = %strategy,
                        count = signal_ids.len().max(count),
                        regime = regime.as_str(),
                        "batch queued"
                    );
                }
                SubmissionOutcome::Rejected { signal_ids, reason } => {
                    info!(
                        strategy = %strategy,
                        count = signal_ids.len().max(count),
                        reason = %reason,
                        regime = regime.as_str(),
                        "batch rejected before queue"
                    );
                }
            },
        }
    }
}

/// Update the shared bankroll stats snapshot.
fn update_strategy_worker_stats(
    stats_slot: &Arc<std::sync::RwLock<BankrollStats>>,
    source: &BankrollStats,
) {
    if let Ok(mut target) = stats_slot.write() {
        *target = source.clone();
    }
}

/// Build an initial bankroll stats snapshot before worker hydration completes.
fn initial_bankroll_stats(starting_balance: f64) -> BankrollStats {
    BankrollStats {
        starting_balance,
        current_balance: starting_balance,
        high_water_mark: starting_balance,
        max_drawdown_pct: 0.0,
        total_trades: 0,
        wins: 0,
        losses: 0,
        win_rate: 0.0,
        total_pnl: 0.0,
        total_fees: 0.0,
    }
}

/// Build the one-time decision-engine seed from storage before feed handling starts.
fn runtime_decision_seed(
    db: &Database,
    starting_balance: f64,
    config: &Config,
    now_ms: u64,
) -> RuntimeDecisionSeed {
    let current_balance = match db.get_latest_balance() {
        Ok(Some(balance)) => balance,
        Ok(None) => {
            if let Err(error) = db.log_balance_event(now_ms, "init", None, 0.0, starting_balance) {
                warn!("failed to persist startup balance event: {error}");
            }
            starting_balance
        }
        Err(error) => {
            warn!("failed to read latest balance for decision seed: {error}");
            starting_balance
        }
    };
    let unresolved_exposures = match db.unresolved_trade_exposures() {
        Ok(rows) => rows,
        Err(error) => {
            warn!("failed to seed unresolved exposure state: {error}");
            Vec::new()
        }
    };
    let open_trades = match db.open_trade_snapshots() {
        Ok(rows) => rows,
        Err(error) => {
            warn!("failed to seed open trade state: {error}");
            Vec::new()
        }
    };
    if !open_trades.is_empty() || !unresolved_exposures.is_empty() {
        info!(
            open_trades = open_trades.len(),
            unresolved_exposures = unresolved_exposures.len(),
            execution_mode = config.execution_mode.as_str(),
            "seeded decision worker from storage"
        );
    }
    RuntimeDecisionSeed {
        starting_balance,
        current_balance,
        unresolved_exposures,
        open_trades,
        now_ms,
    }
}

/// Emit one runtime persistence writer shutdown snapshot.
fn log_persistence_writer_snapshot(snapshot: &LivePersistenceWriterSnapshot) {
    info!(
        queued_events = snapshot.enqueued,
        persisted_events = snapshot.persisted,
        dropped_events = snapshot.dropped,
        queue_full = snapshot.queue_full,
        write_errors = snapshot.write_errors,
        max_batch_write_ms = snapshot.max_write_ms,
        last_persisted_at_ms = snapshot.last_persisted_at_ms,
        "stopping runtime persistence writer"
    );
}

/// Local receive-time pair captured when a feed message enters the live loop.
struct ReceiveTimes {
    ms: u64,
    micros: Option<u64>,
}

/// Context required to persist one live `CLOB` event into `feed_events`.
struct LiveClobLogEvent<'a> {
    receive_ms: u64,
    receive_micros: Option<u64>,
    event_ms: u64,
    event_micros: Option<u64>,
    event_type: &'a str,
    book_state: &'a crate::types::BookState,
    current_window: Option<&'a MarketWindow>,
    asset_id: Option<&'a str>,
    source_topic: Option<&'a str>,
    connection_id: &'a str,
    payload_json: Option<&'a str>,
    details_json: Option<&'a str>,
}

/// Owned feed-health record passed to the persistence worker task.
#[derive(Debug, Clone)]
struct OwnedFeedHealthLogEvent {
    timestamp_ms: u64,
    timestamp_micros: Option<u64>,
    source: String,
    event_type: String,
    connection_id: Option<String>,
    market_id: Option<String>,
    details_json: Option<String>,
}

/// One strategy-evaluation snapshot sent out of the feed hot path.
struct StrategyEvaluationRequest {
    decision_sequence: u64,
    ctx: StrategyContext,
    window: MarketWindow,
    book_state: crate::types::BookState,
    now_ms: u64,
    now_us: Option<u64>,
    live_trading_can_submit: bool,
}

#[derive(Default)]
struct LatestStrategyEvaluation {
    request: Mutex<Option<Box<StrategyEvaluationRequest>>>,
    wake: Condvar,
}

/// Work item accepted by the strategy worker.
enum StrategyWorkerMessage {
    WindowClosed {
        window: MarketWindow,
        now_ms: u64,
    },
    AuthoritativeResolution {
        window: MarketWindow,
        outcome: SignalDirection,
    },
    ReleaseReservations {
        releases: Vec<(String, f64)>,
    },
    LiveSubmissionFeedback {
        fills: Vec<LiveOrderFillFeedback>,
        rejected_signal_ids: Vec<i64>,
        now_ms: u64,
    },
    FlushAll {
        now_ms: u64,
    },
    Shutdown,
}

/// Strategy worker output consumed by the async runtime.
enum StrategyWorkerOutput {
    Decision(RuntimeDecisionOutput),
}

/// Snapshot of strategy worker health.
#[derive(Debug, Clone)]
struct StrategyWorkerSnapshot {
    enqueued: u64,
    replaced: u64,
    dropped: u64,
    processed: u64,
    output_dropped: u64,
    last_processed_at_ms: u64,
    shutdown_timed_out: u64,
    stats: BankrollStats,
}

/// Non-blocking handle for the pure strategy and paper-execution worker.
struct StrategyWorker {
    tx: SyncSender<StrategyWorkerMessage>,
    latest: Arc<LatestStrategyEvaluation>,
    metrics: Arc<StrategyWorkerMetrics>,
    stats: Arc<std::sync::RwLock<BankrollStats>>,
    join: Option<thread::JoinHandle<()>>,
}

/// Atomic strategy worker counters.
#[derive(Debug, Default)]
struct StrategyWorkerMetrics {
    enqueued: AtomicU64,
    replaced: AtomicU64,
    dropped: AtomicU64,
    processed: AtomicU64,
    output_dropped: AtomicU64,
    last_processed_at_ms: AtomicU64,
    shutdown_timed_out: AtomicU64,
}

/// Runtime context owned by the pure strategy worker thread.
struct StrategyWorkerRuntime {
    config: Config,
    strategies: Vec<Box<dyn Strategy>>,
    seed: RuntimeDecisionSeed,
    rx: Receiver<StrategyWorkerMessage>,
    latest: Arc<LatestStrategyEvaluation>,
    metrics: Arc<StrategyWorkerMetrics>,
    stats: Arc<std::sync::RwLock<BankrollStats>>,
    output_tx: tokio::sync::mpsc::Sender<StrategyWorkerOutput>,
}

/// Request sent to the live submission worker.
struct LiveSubmissionRequest {
    window: MarketWindow,
    orders: Vec<QueuedOrderIntent>,
    critical_events: Vec<LivePersistenceEvent>,
    now_ms: u64,
}

/// Feedback from the live submission worker back to the runtime.
struct LiveSubmissionFeedback {
    state_update: Option<LiveSubmissionStateUpdate>,
    releases: Vec<(String, f64)>,
    fills: Vec<LiveOrderFillFeedback>,
    rejected_signal_ids: Vec<i64>,
    now_ms: u64,
}

/// Worker context for persisting an unknown live order submission.
struct LiveOrderErrorContext<'a> {
    db_path: &'a str,
    session_id: i64,
    sidecar: &'a LiveSidecarClient,
    order: &'a QueuedOrderIntent,
    request: &'a LiveOrderIntentRequest,
    intent_id: i64,
    now_ms: u64,
}

/// State transition discovered by the live submission worker.
struct LiveSubmissionStateUpdate {
    state: &'static str,
    reason: String,
}

/// Non-blocking handle for live venue submissions.
struct LiveSubmissionQueue {
    tx: tokio::sync::mpsc::Sender<LiveSubmissionRequest>,
}

/// Kind of live monitor worker running outside the feed loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveMonitorWorkerKind {
    Controls,
    RemoteRefresh,
}

/// Live monitor worker result returned to the runtime.
struct LiveMonitorWorkerOutput {
    kind: LiveMonitorWorkerKind,
    result: Result<LiveTradingMonitor, String>,
}

/// One asynchronous authoritative-resolution fetch result.
struct ResolutionFetchResult {
    pending: PendingResolution,
    result: anyhow::Result<Option<SignalDirection>>,
}

/// Timer command sent to the runtime command queue.
#[derive(Debug, Clone, Copy)]
enum RuntimeTimerCommand {
    StorageReport,
    FeedHealthReport,
    ResolutionRetry,
    ReadonlyPoll,
    ReadonlyRollup,
    LiveControlPoll,
    LiveRemotePoll,
}

/// Normal-priority command consumed by the live runtime reactor.
enum RuntimeCommand {
    MarketDiscovery(MarketDiscoveryEvent),
    ActivateWindow(MarketWindow),
    Timer(RuntimeTimerCommand),
    ResolutionResult(Box<ResolutionFetchResult>),
    ReadonlyRefresh(Box<crate::live_readonly::ReadonlyRefreshResult>),
    LiveMonitorOutput(Box<LiveMonitorWorkerOutput>),
}

/// Urgent command consumed before normal maintenance work.
enum UrgentRuntimeCommand {
    StrategyOutput(Box<StrategyWorkerOutput>),
    LiveFeedback(LiveSubmissionFeedback),
    LiveMonitorOutput(Box<LiveMonitorWorkerOutput>),
}

const FEED_HEALTH_ROLLUP_INTERVAL_SECS: u64 = 5 * 60;
const LIVE_TRADING_CONTROL_POLL_INTERVAL_SECS: u64 = 1;
const LIVE_TRADING_POLL_INTERVAL_SECS: u64 = 15;
const LIVE_TERMINAL_DEGRADATION_MS: u64 = 120_000;
const CASH_CHANGE_EPSILON_USD: f64 = 0.01;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FeedHealthWindowStats {
    disconnect_count: u64,
    cumulative_downtime_ms: u64,
    max_downtime_ms: u64,
    cause_counts: std::collections::HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveFeedDisconnect {
    started_at_ms: u64,
    cause_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeedHealthRollupRow {
    source: String,
    disconnect_count: u64,
    cumulative_downtime_ms: u64,
    max_downtime_ms: u64,
    active_outage_ms: Option<u64>,
    active_cause_class: Option<String>,
    cause_counts: Vec<(String, u64)>,
}

#[derive(Default)]
struct FeedHealthTracker {
    window: std::collections::HashMap<String, FeedHealthWindowStats>,
    active: std::collections::HashMap<String, ActiveFeedDisconnect>,
}

impl FeedHealthTracker {
    /// Mark one feed connection as healthy again and accumulate completed downtime.
    fn note_connected(&mut self, source: &str, now_ms: u64) {
        if let Some(active) = self.active.remove(source) {
            let downtime_ms = now_ms.saturating_sub(active.started_at_ms);
            let stats = self.window.entry(source.to_string()).or_default();
            stats.cumulative_downtime_ms = stats.cumulative_downtime_ms.saturating_add(downtime_ms);
            stats.max_downtime_ms = stats.max_downtime_ms.max(downtime_ms);
        }
    }

    /// Record one feed disconnect if the feed is not already marked as down.
    fn note_disconnected(&mut self, source: &str, cause_class: &str, now_ms: u64) {
        if self.active.contains_key(source) {
            return;
        }

        let stats = self.window.entry(source.to_string()).or_default();
        stats.disconnect_count = stats.disconnect_count.saturating_add(1);
        *stats
            .cause_counts
            .entry(cause_class.to_string())
            .or_insert(0) += 1;
        self.active.insert(
            source.to_string(),
            ActiveFeedDisconnect {
                started_at_ms: now_ms,
                cause_class: cause_class.to_string(),
            },
        );
    }

    /// Drain the current rollup window into operator-facing rows while preserving active outages.
    fn take_rollups(&mut self, now_ms: u64) -> Vec<FeedHealthRollupRow> {
        let window = std::mem::take(&mut self.window);
        let mut sources = window.keys().cloned().collect::<Vec<_>>();
        for source in self.active.keys() {
            if !sources.iter().any(|existing| existing == source) {
                sources.push(source.clone());
            }
        }
        sources.sort();

        let mut rows = Vec::new();
        for source in sources {
            let mut stats = window.get(&source).cloned().unwrap_or_default();
            let active = self.active.get(&source);
            if stats.disconnect_count == 0 && active.is_none() {
                continue;
            }
            let mut cause_counts = stats.cause_counts.drain().collect::<Vec<_>>();
            cause_counts
                .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            rows.push(FeedHealthRollupRow {
                source: source.clone(),
                disconnect_count: stats.disconnect_count,
                cumulative_downtime_ms: stats.cumulative_downtime_ms,
                max_downtime_ms: stats.max_downtime_ms,
                active_outage_ms: active.map(|entry| now_ms.saturating_sub(entry.started_at_ms)),
                active_cause_class: active.map(|entry| entry.cause_class.clone()),
                cause_counts,
            });
        }
        rows
    }
}

impl LiveRiskMonitor {
    /// Build live risk state from the first authoritative account snapshot.
    fn new(account: &LiveAccountState) -> Self {
        let equity = account.total_equity;
        Self {
            session_start_equity: equity,
            day_index: utc_day_index(account.timestamp_ms),
            day_baseline_equity: equity,
            high_water_mark: equity,
            trough_equity: equity,
            current_equity: equity,
            session_drawdown_usd: 0.0,
            daily_loss_usd: 0.0,
            terminal_reason: None,
            terminal_at_ms: None,
        }
    }

    /// Update risk state from one fresh account snapshot and return any terminal breach.
    fn update_from_account(
        &mut self,
        account: &LiveAccountState,
        config: &Config,
    ) -> Option<LiveRiskBreach> {
        let equity = account.total_equity;
        let day_index = utc_day_index(account.timestamp_ms);
        if day_index != self.day_index {
            self.day_index = day_index;
            self.day_baseline_equity = equity;
        }
        self.current_equity = equity;
        self.high_water_mark = self.high_water_mark.max(equity);
        self.trough_equity = self.trough_equity.min(equity);
        self.session_drawdown_usd = (self.high_water_mark - equity).max(0.0);
        self.daily_loss_usd = (self.day_baseline_equity - equity).max(0.0);

        if self.session_drawdown_usd + CASH_CHANGE_EPSILON_USD
            >= config.live_max_session_drawdown_usd
        {
            return Some(self.breach(
                "live_session_drawdown_halt",
                "live session drawdown cap breached",
                config,
            ));
        }
        if self.daily_loss_usd + CASH_CHANGE_EPSILON_USD >= config.live_max_daily_loss_usd {
            return Some(self.breach(
                "live_daily_loss_halt",
                "live daily loss cap breached",
                config,
            ));
        }
        if self.high_water_mark > 0.0
            && self.session_drawdown_usd / self.high_water_mark + f64::EPSILON
                >= config.max_drawdown_pct
        {
            return Some(self.breach(
                "live_percent_drawdown_halt",
                "live percentage drawdown cap breached",
                config,
            ));
        }
        None
    }

    /// Mark this risk monitor as terminal for presentation and closeout.
    fn mark_terminal(&mut self, now_ms: u64, reason: &str) {
        self.terminal_reason = Some(reason.to_string());
        self.terminal_at_ms = Some(now_ms);
    }

    /// Return compact JSON persisted into the live session details.
    fn to_json(&self) -> serde_json::Value {
        json!({
            "session_start_equity": self.session_start_equity,
            "day_baseline_equity": self.day_baseline_equity,
            "high_water_mark": self.high_water_mark,
            "trough_equity": self.trough_equity,
            "current_equity": self.current_equity,
            "session_drawdown_usd": self.session_drawdown_usd,
            "daily_loss_usd": self.daily_loss_usd,
            "terminal_reason": self.terminal_reason,
            "terminal_at_ms": self.terminal_at_ms,
        })
    }

    /// Build one terminal breach payload from the current risk state.
    fn breach(&self, event_type: &'static str, reason: &str, config: &Config) -> LiveRiskBreach {
        LiveRiskBreach {
            event_type,
            reason: reason.to_string(),
            details: json!({
                "reason": reason,
                "session_start_equity": self.session_start_equity,
                "day_baseline_equity": self.day_baseline_equity,
                "high_water_mark": self.high_water_mark,
                "trough_equity": self.trough_equity,
                "current_equity": self.current_equity,
                "session_drawdown_usd": self.session_drawdown_usd,
                "daily_loss_usd": self.daily_loss_usd,
                "max_session_drawdown_usd": config.live_max_session_drawdown_usd,
                "max_daily_loss_usd": config.live_max_daily_loss_usd,
                "max_drawdown_pct": config.max_drawdown_pct,
            }),
        }
    }
}

impl LiveDegradationTracker {
    /// Record one active degradation and return a breach once it exceeds the terminal threshold.
    fn note(
        &mut self,
        kind: &str,
        detail: &str,
        now_ms: u64,
        threshold_ms: u64,
    ) -> Option<LiveDegradationBreach> {
        match self.active.as_mut() {
            Some(active) if active.kind == kind => {
                active.latest_detail = detail.to_string();
                let duration_ms = now_ms.saturating_sub(active.started_at_ms);
                if duration_ms >= threshold_ms {
                    Some(LiveDegradationBreach {
                        kind: active.kind.clone(),
                        duration_ms,
                        detail: active.latest_detail.clone(),
                    })
                } else {
                    None
                }
            }
            _ => {
                self.active = Some(LiveDegradation {
                    kind: kind.to_string(),
                    started_at_ms: now_ms,
                    latest_detail: detail.to_string(),
                });
                None
            }
        }
    }

    /// Clear active degradation after a fully healthy refresh.
    fn clear(&mut self) {
        self.active = None;
    }
}

impl LiveState {
    /// Creates a new `LiveState`.
    fn new() -> Self {
        Self {
            signal_state: SignalState::new(),
            current_window: None,
            window_open_prices: std::collections::HashMap::new(),
            known_windows: std::collections::HashMap::new(),
            latest_decision_sequence: 0,
        }
    }
}

/// Return the UTC day index for one millisecond timestamp.
fn utc_day_index(timestamp_ms: u64) -> u64 {
    timestamp_ms / 86_400_000
}

/// Capture the current local receive timestamps for one incoming live event.
fn capture_receive_times(clock: &dyn Clock) -> ReceiveTimes {
    ReceiveTimes {
        ms: clock.now(),
        micros: Some(now_us()),
    }
}

/// Run the long-lived runtime for paper or readonly venue execution.
///
/// The bot runs until `shutdown_rx` fires, at which point it performs a
/// graceful shutdown (logs final stats, closes the database).
///
/// In the CLI binary, `shutdown_rx` is wired to `tokio::signal::ctrl_c()`.
/// In tests, it is fired by the test harness after a scripted scenario.
pub async fn run_live(
    config: Config,
    db_path: &str,
    balance: f64,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    config.validate()?;
    run_live_runtime(config, db_path, balance, shutdown_rx).await
}

/// Forward market-discovery events into the runtime command queue.
fn spawn_market_discovery_forwarder(
    mut rx: tokio::sync::mpsc::Receiver<MarketDiscoveryEvent>,
    tx: tokio::sync::mpsc::Sender<RuntimeCommand>,
) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if tx
                .send(RuntimeCommand::MarketDiscovery(event))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

/// Forward resolution fetch results into the runtime command queue.
fn spawn_resolution_result_forwarder(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<ResolutionFetchResult>,
    tx: tokio::sync::mpsc::Sender<RuntimeCommand>,
) {
    tokio::spawn(async move {
        while let Some(result) = rx.recv().await {
            if tx
                .send(RuntimeCommand::ResolutionResult(Box::new(result)))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

/// Forward readonly refresh results into the runtime command queue.
fn spawn_readonly_refresh_forwarder(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::live_readonly::ReadonlyRefreshResult>,
    tx: tokio::sync::mpsc::Sender<RuntimeCommand>,
) {
    tokio::spawn(async move {
        while let Some(result) = rx.recv().await {
            if tx
                .send(RuntimeCommand::ReadonlyRefresh(Box::new(result)))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

/// Forward live monitor worker results to urgent or normal command queues.
fn spawn_live_monitor_forwarder(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<LiveMonitorWorkerOutput>,
    urgent_tx: tokio::sync::mpsc::Sender<UrgentRuntimeCommand>,
    normal_tx: tokio::sync::mpsc::Sender<RuntimeCommand>,
) {
    tokio::spawn(async move {
        while let Some(output) = rx.recv().await {
            let sent = match output.kind {
                LiveMonitorWorkerKind::Controls => urgent_tx
                    .send(UrgentRuntimeCommand::LiveMonitorOutput(Box::new(output)))
                    .await
                    .is_ok(),
                LiveMonitorWorkerKind::RemoteRefresh => normal_tx
                    .send(RuntimeCommand::LiveMonitorOutput(Box::new(output)))
                    .await
                    .is_ok(),
            };
            if !sent {
                break;
            }
        }
    });
}

/// Forward strategy outputs into the urgent runtime command queue.
fn spawn_strategy_output_forwarder(
    mut rx: tokio::sync::mpsc::Receiver<StrategyWorkerOutput>,
    tx: tokio::sync::mpsc::Sender<UrgentRuntimeCommand>,
) {
    tokio::spawn(async move {
        while let Some(output) = rx.recv().await {
            if tx
                .send(UrgentRuntimeCommand::StrategyOutput(Box::new(output)))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

/// Forward live submission feedback into the urgent runtime command queue.
fn spawn_live_feedback_forwarder(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<LiveSubmissionFeedback>,
    tx: tokio::sync::mpsc::Sender<UrgentRuntimeCommand>,
) {
    tokio::spawn(async move {
        while let Some(feedback) = rx.recv().await {
            if tx
                .send(UrgentRuntimeCommand::LiveFeedback(feedback))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

/// Send one repeated timer command without adding timer branches to the feed reactor.
fn spawn_runtime_timer(
    tx: tokio::sync::mpsc::Sender<RuntimeCommand>,
    period: Duration,
    missed_tick_behavior: tokio::time::MissedTickBehavior,
    command: RuntimeTimerCommand,
) {
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(period);
        timer.set_missed_tick_behavior(missed_tick_behavior);
        let _ = timer.tick().await;
        loop {
            timer.tick().await;
            if tx.send(RuntimeCommand::Timer(command)).await.is_err() {
                break;
            }
        }
    });
}

/// Send one delayed market activation command.
fn spawn_market_activation(
    tx: tokio::sync::mpsc::Sender<RuntimeCommand>,
    window: MarketWindow,
    delay_ms: u64,
) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let _ = tx.send(RuntimeCommand::ActivateWindow(window)).await;
    });
}

/// Run the shared paper or readonly live runtime.
#[allow(clippy::too_many_lines)]
async fn run_live_runtime(
    config: Config,
    db_path: &str,
    balance: f64,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let db = Database::new(db_path)?;
    let clock = SystemClock;
    let mut readonly_monitor = None;
    let mut live_trading_monitor = None;
    let (db, runtime_balance) =
        if config.execution_mode == crate::config::ExecutionMode::LiveReadonly {
            let (db, bootstrap) =
                crate::live_readonly::bootstrap_readonly_runtime(&config, db, &clock).await?;
            readonly_monitor = Some(bootstrap.monitor);
            (db, bootstrap.shadow_starting_balance)
        } else if config.execution_mode == crate::config::ExecutionMode::LiveTrading {
            let (db, bootstrap) =
                bootstrap_live_trading_runtime(&config, db, db_path, balance, &clock).await?;
            live_trading_monitor = Some(bootstrap.monitor);
            (db, bootstrap.live_starting_balance)
        } else {
            (db, balance)
        };
    let runtime_started_at_ms = clock.now();
    let pending_policy = config.pending_settlement_policy_unchecked();
    persist_runtime_capture_metadata(
        &db,
        config.feed_event_storage_profile,
        runtime_started_at_ms,
        "empty",
        "runtime_start",
    )?;
    crate::runtime_config_snapshot::persist_runtime_config_snapshot(
        &db,
        &config,
        runtime_started_at_ms,
    )?;
    info!(
        balance = runtime_balance,
        db = %db_path,
        execution_mode = config.execution_mode.as_str(),
        feed_event_storage_profile = config.feed_event_storage_profile.as_str(),
        runtime_capture_health = "empty",
        pending_settlement_mode = pending_policy.mode.as_str(),
        pending_settlement_family_reserve_fraction = pending_policy.family_reserve_fraction,
        pending_settlement_global_reserve_fraction = pending_policy.global_reserve_fraction,
        pending_settlement_counts_as_open_position = pending_policy.counts_as_open_position,
        "starting live runtime"
    );
    let enabled_strategies = config.enabled_strategy_names();
    if enabled_strategies.is_empty() {
        bail!(
            "no strategies enabled after config parsing; boolean env values must be true/false or 1/0"
        );
    }
    info!(
        enabled_strategies = %enabled_strategies.join(","),
        strategy_count = enabled_strategies.len(),
        "live strategy set resolved"
    );

    let mut momentum = MomentumCalculator::new(config.momentum_window_ms);
    let (strategy_output_tx, strategy_output_rx) = tokio::sync::mpsc::channel::<StrategyWorkerOutput>(
        config.live_decision_output_queue_capacity.max(1),
    );
    let worker_seed = runtime_decision_seed(&db, runtime_balance, &config, runtime_started_at_ms);
    let mut strategy_worker = StrategyWorker::start(
        config.clone(),
        build_strategies(&config),
        worker_seed,
        strategy_output_tx,
    )?;
    let (live_feedback_tx, live_feedback_rx) =
        tokio::sync::mpsc::unbounded_channel::<LiveSubmissionFeedback>();
    let live_submission_queue = live_trading_monitor.as_ref().map(|monitor| {
        LiveSubmissionQueue::start(
            db_path.to_string(),
            config.clone(),
            monitor.session_id,
            monitor.sidecar.clone(),
            live_feedback_tx,
        )
    });

    let (feed_tx, mut feed_rx) = tokio::sync::mpsc::channel::<FeedMessage>(512);
    let normal_command_capacity = config.live_runtime_persistence_queue_capacity.max(1);
    let urgent_command_capacity = config.live_decision_output_queue_capacity.max(1);
    let (runtime_command_tx, mut runtime_command_rx) =
        tokio::sync::mpsc::channel::<RuntimeCommand>(normal_command_capacity);
    let (urgent_command_tx, mut urgent_command_rx) =
        tokio::sync::mpsc::channel::<UrgentRuntimeCommand>(urgent_command_capacity);
    spawn_strategy_output_forwarder(strategy_output_rx, urgent_command_tx.clone());
    spawn_live_feedback_forwarder(live_feedback_rx, urgent_command_tx.clone());

    let binance_config = config.clone();
    let binance_tx = feed_tx.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::feeds::binance_feed::run_binance_feed(&binance_config, binance_tx).await
        {
            error!(feed = "binance", "feed exited with error: {e}");
        }
    });

    let chainlink_config = config.clone();
    let chainlink_tx = feed_tx.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::feeds::chainlink_feed::run_chainlink_feed(&chainlink_config, chainlink_tx).await
        {
            error!(feed = "chainlink", "feed exited with error: {e}");
        }
    });

    let clob_config = config.clone();
    let clob_tx = feed_tx.clone();
    let (clob_handle, _clob_join) =
        crate::feeds::clob_feed::run_clob_feed(&clob_config, clob_tx).await;

    let discovery = market_discovery::run_market_discovery(&config).await;
    spawn_market_discovery_forwarder(discovery.window_rx, runtime_command_tx.clone());

    let tick_logger_state = Arc::new(tokio::sync::RwLock::new(TickLoggerState::default()));
    if config.tick_data_logging_enabled {
        let tick_logger_state_clone = Arc::clone(&tick_logger_state);
        let tick_interval = config.tick_interval;
        let tick_logger_db_path = db_path.to_string();
        tokio::spawn(async move {
            tick_logger::run_tick_logger(
                tick_logger_db_path,
                tick_interval,
                tick_logger_state_clone,
            )
            .await;
        });
    }

    let mut state = LiveState::new();
    let mut storage_state = FeedEventStorageState::new(config.feed_event_storage_profile);
    let mut feed_writer = FeedEventWriter::start(
        db_path.to_string(),
        FeedEventWriterConfig {
            queue_capacity: config.feed_event_writer_queue_capacity,
            batch_size: config.feed_event_writer_batch_size,
            flush_interval_ms: config.feed_event_writer_flush_ms,
            compact_clob_replay: config.feed_event_storage_profile
                == FeedEventStorageProfile::ReplayGrade,
            clob_block_max_rows: config.clob_replay_block_max_rows,
            clob_block_max_ms: config.clob_replay_block_max_ms,
            clob_block_zstd_level: config.clob_replay_block_zstd_level,
        },
    )?;
    let mut persistence_writer = LivePersistenceWriter::start(
        db_path.to_string(),
        LivePersistenceWriterConfig {
            queue_capacity: config.live_runtime_persistence_queue_capacity,
            batch_size: config.feed_event_writer_batch_size,
            flush_interval_ms: config.feed_event_writer_flush_ms,
        },
    )?;
    let mut feed_health_tracker = FeedHealthTracker::default();
    let mut pending_resolutions = seed_pending_resolutions(&db, &config, &clock);
    let (resolution_result_tx, resolution_result_rx) =
        tokio::sync::mpsc::unbounded_channel::<ResolutionFetchResult>();
    spawn_resolution_result_forwarder(resolution_result_rx, runtime_command_tx.clone());
    let (readonly_refresh_tx, readonly_refresh_rx) =
        tokio::sync::mpsc::unbounded_channel::<crate::live_readonly::ReadonlyRefreshResult>();
    spawn_readonly_refresh_forwarder(readonly_refresh_rx, runtime_command_tx.clone());
    let mut readonly_refresh_inflight = false;
    let (live_monitor_tx, live_monitor_rx) =
        tokio::sync::mpsc::unbounded_channel::<LiveMonitorWorkerOutput>();
    spawn_live_monitor_forwarder(
        live_monitor_rx,
        urgent_command_tx.clone(),
        runtime_command_tx.clone(),
    );
    let mut live_control_inflight = false;
    let mut live_remote_refresh_inflight = false;

    spawn_runtime_timer(
        runtime_command_tx.clone(),
        Duration::from_secs(60),
        tokio::time::MissedTickBehavior::Delay,
        RuntimeTimerCommand::StorageReport,
    );
    spawn_runtime_timer(
        runtime_command_tx.clone(),
        Duration::from_secs(FEED_HEALTH_ROLLUP_INTERVAL_SECS),
        tokio::time::MissedTickBehavior::Delay,
        RuntimeTimerCommand::FeedHealthReport,
    );
    spawn_runtime_timer(
        runtime_command_tx.clone(),
        Duration::from_secs(1),
        tokio::time::MissedTickBehavior::Delay,
        RuntimeTimerCommand::ResolutionRetry,
    );
    if readonly_monitor.is_some() {
        spawn_runtime_timer(
            runtime_command_tx.clone(),
            Duration::from_secs(crate::live_readonly::READONLY_POLL_INTERVAL_SECS),
            tokio::time::MissedTickBehavior::Skip,
            RuntimeTimerCommand::ReadonlyPoll,
        );
        spawn_runtime_timer(
            runtime_command_tx.clone(),
            Duration::from_secs(crate::live_readonly::READONLY_ROLLUP_INTERVAL_SECS),
            tokio::time::MissedTickBehavior::Delay,
            RuntimeTimerCommand::ReadonlyRollup,
        );
    }
    if live_trading_monitor.is_some() {
        spawn_runtime_timer(
            runtime_command_tx.clone(),
            Duration::from_secs(LIVE_TRADING_CONTROL_POLL_INTERVAL_SECS),
            tokio::time::MissedTickBehavior::Skip,
            RuntimeTimerCommand::LiveControlPoll,
        );
        spawn_runtime_timer(
            runtime_command_tx.clone(),
            Duration::from_secs(LIVE_TRADING_POLL_INTERVAL_SECS),
            tokio::time::MissedTickBehavior::Skip,
            RuntimeTimerCommand::LiveRemotePoll,
        );
    }

    info!("all tasks spawned, entering main loop");

    let mut shutdown_rx = shutdown_rx;
    let mut feed_batch = Vec::with_capacity(config.live_feed_batch_max_messages.max(1));
    let mut pending_decision = PendingDecisionEvaluation::default();

    loop {
        tokio::select! {

            feed_count = feed_rx.recv_many(
                &mut feed_batch,
                config.live_feed_batch_max_messages.max(1),
            ) => {
                if feed_count == 0 {
                    warn!("all feed senders dropped, shutting down");
                    break;
                }

                for msg in feed_batch.drain(..) {
                    match msg {
                    FeedMessage::BinanceTrade {
                        price,
                        quantity,
                        signed_quantity,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        source_topic,
                        source_symbol,
                        sequence_key,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state.signal_state.update_binance_trade(
                            price,
                            quantity,
                            signed_quantity,
                            receive.ms,
                            receive.micros,
                        );
                        momentum.push(price, receive.ms);
                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.binance_price = Some(price);
                        }

                        let event = storage_state.prepare_binance_trade(
                            FeedEvent {
                                id: None,
                                received_at_ms: receive.ms,
                                event_at_ms: timestamp_ms,
                                received_at_us: receive.micros,
                                event_at_us: source_micros,
                                source: "binance".to_string(),
                                event_type: "aggTrade".to_string(),
                                source_topic,
                                source_symbol,
                                connection_id: Some(connection_id),
                                sequence_key,
                                market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                                asset_id: None,
                                price: Some(price),
                                trade_size: None,
                                signed_quantity: None,
                                best_bid: None,
                                best_ask: None,
                                bid_size: None,
                                ask_size: None,
                                depth_bid_notional: None,
                                depth_ask_notional: None,
                                depth_imbalance: None,
                                microprice: None,
                                payload_json,
                                details_json,
                                fidelity: ReplayFidelity::RawEvent,
                            },
                            quantity,
                            signed_quantity,
                        );
                        mark_capture_enqueue_result(
                            live_trading_monitor.as_mut(),
                            enqueue_counted_feed_event(&feed_writer, &mut storage_state, event),
                        );

                        if let Some(ref w) = state.current_window {
                            state.window_open_prices.entry(w.market_id.clone()).or_insert(price);
                        }

                        pending_decision.mark_dirty(receive.ms, receive.micros);
                    }

                    FeedMessage::BinanceBookTicker {
                        best_bid,
                        best_ask,
                        bid_size,
                        ask_size,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        source_topic,
                        source_symbol,
                        sequence_key,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state.signal_state.update_binance_book(
                            best_bid,
                            best_ask,
                            bid_size,
                            ask_size,
                            receive.ms,
                            sequence_key.clone(),
                        );
                        if let Some(event) = storage_state.prepare_binance_book_ticker(FeedEvent {
                            id: None,
                            received_at_ms: receive.ms,
                            event_at_ms: timestamp_ms,
                            received_at_us: receive.micros,
                            event_at_us: source_micros,
                            source: "binance".to_string(),
                            event_type: "bookTicker".to_string(),
                            source_topic,
                            source_symbol,
                            connection_id: Some(connection_id),
                            sequence_key,
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            asset_id: None,
                            price: None,
                            trade_size: None,
                            signed_quantity: None,
                            best_bid: Some(best_bid),
                            best_ask: Some(best_ask),
                            bid_size: Some(bid_size),
                            ask_size: Some(ask_size),
                            depth_bid_notional: None,
                            depth_ask_notional: None,
                            depth_imbalance: None,
                            microprice: None,
                            payload_json,
                            details_json,
                            fidelity: ReplayFidelity::RawEvent,
                        }) {
                            mark_capture_enqueue_result(
                                live_trading_monitor.as_mut(),
                                enqueue_counted_feed_event(&feed_writer, &mut storage_state, event),
                            );
                        }

                        pending_decision.mark_dirty(receive.ms, receive.micros);
                    }

                    FeedMessage::BinanceDepth {
                        bid_levels,
                        ask_levels,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        source_topic,
                        source_symbol,
                        sequence_key,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        let best_bid = bid_levels.first().map(|level| level.price);
                        let best_ask = ask_levels.first().map(|level| level.price);
                        let bid_size = bid_levels.first().map(|level| level.size);
                        let ask_size = ask_levels.first().map(|level| level.size);
                        let depth_event = storage_state.prepare_binance_depth(
                            FeedEvent {
                                id: None,
                                received_at_ms: receive.ms,
                                event_at_ms: timestamp_ms,
                                received_at_us: receive.micros,
                                event_at_us: source_micros,
                                source: "binance".to_string(),
                                event_type: "depth".to_string(),
                                source_topic,
                                source_symbol: source_symbol.clone(),
                                connection_id: Some(connection_id.clone()),
                                sequence_key: sequence_key.clone(),
                                market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                                asset_id: None,
                                price: None,
                                trade_size: None,
                                signed_quantity: None,
                                best_bid,
                                best_ask,
                                bid_size,
                                ask_size,
                                depth_bid_notional: None,
                                depth_ask_notional: None,
                                depth_imbalance: None,
                                microprice: None,
                                payload_json,
                                details_json,
                                fidelity: ReplayFidelity::RawEvent,
                            },
                            source_symbol.as_deref(),
                            &bid_levels,
                            &ask_levels,
                        );
                        state.signal_state.update_binance_depth(
                            bid_levels,
                            ask_levels,
                            receive.ms,
                            sequence_key.clone(),
                        );
                        if let Some(event) = depth_event {
                            mark_capture_enqueue_result(
                                live_trading_monitor.as_mut(),
                                enqueue_counted_feed_event(&feed_writer, &mut storage_state, event),
                            );
                        }

                        pending_decision.mark_dirty(receive.ms, receive.micros);
                    }

                    FeedMessage::ChainlinkPrice {
                        price,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        source_topic,
                        source_symbol,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state
                            .signal_state
                            .update_chainlink(price, receive.ms, receive.micros);
                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.chainlink_price = Some(price);
                        }

                        let event = storage_state.prepare_chainlink_price(FeedEvent {
                            id: None,
                            received_at_ms: receive.ms,
                            event_at_ms: timestamp_ms,
                            received_at_us: receive.micros,
                            event_at_us: source_micros,
                            source: "chainlink".to_string(),
                            event_type: "chainlink_price".to_string(),
                            source_topic,
                            source_symbol,
                            connection_id: Some(connection_id),
                            sequence_key: None,
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            asset_id: None,
                            price: Some(price),
                            trade_size: None,
                            signed_quantity: None,
                            best_bid: None,
                            best_ask: None,
                            bid_size: None,
                            ask_size: None,
                            depth_bid_notional: None,
                            depth_ask_notional: None,
                            depth_imbalance: None,
                            microprice: None,
                            payload_json,
                            details_json,
                            fidelity: ReplayFidelity::RawEvent,
                        });
                        mark_capture_enqueue_result(
                            live_trading_monitor.as_mut(),
                            enqueue_counted_feed_event(&feed_writer, &mut storage_state, event),
                        );

                        pending_decision.mark_dirty(receive.ms, receive.micros);
                    }

                    FeedMessage::ClobBook {
                        book_state,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        asset_id,
                        source_topic,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state
                            .signal_state
                            .update_clob(book_state.clone(), receive.ms, receive.micros);
                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state.clone();
                        }

                        mark_capture_enqueue_result(
                            live_trading_monitor.as_mut(),
                            log_live_clob_event(
                            &feed_writer,
                            &mut storage_state,
                            &LiveClobLogEvent {
                                receive_ms: receive.ms,
                                receive_micros: receive.micros,
                                event_ms: timestamp_ms,
                                event_micros: source_micros,
                                event_type: "book",
                                book_state: &book_state,
                                current_window: state.current_window.as_ref(),
                                asset_id: asset_id.as_deref(),
                                source_topic: source_topic.as_deref(),
                                connection_id: &connection_id,
                                payload_json: payload_json.as_deref(),
                                details_json: details_json.as_deref(),
                            },
                            ),
                        );

                        pending_decision.mark_dirty(receive.ms, receive.micros);
                    }

                    FeedMessage::ClobPriceChange {
                        book_state,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        asset_id,
                        source_topic,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state
                            .signal_state
                            .update_clob(book_state.clone(), receive.ms, receive.micros);
                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state.clone();
                        }

                        mark_capture_enqueue_result(
                            live_trading_monitor.as_mut(),
                            log_live_clob_event(
                            &feed_writer,
                            &mut storage_state,
                            &LiveClobLogEvent {
                                receive_ms: receive.ms,
                                receive_micros: receive.micros,
                                event_ms: timestamp_ms,
                                event_micros: source_micros,
                                event_type: "price_change",
                                book_state: &book_state,
                                current_window: state.current_window.as_ref(),
                                asset_id: asset_id.as_deref(),
                                source_topic: source_topic.as_deref(),
                                connection_id: &connection_id,
                                payload_json: payload_json.as_deref(),
                                details_json: details_json.as_deref(),
                            },
                            ),
                        );

                        pending_decision.mark_dirty(receive.ms, receive.micros);
                    }

                    FeedMessage::ClobBestBidAsk {
                        book_state,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        asset_id,
                        source_topic,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        state
                            .signal_state
                            .update_clob(book_state.clone(), receive.ms, receive.micros);
                        if let Ok(mut tls) = tick_logger_state.try_write() {
                            tls.book_state = book_state.clone();
                        }

                        mark_capture_enqueue_result(
                            live_trading_monitor.as_mut(),
                            log_live_clob_event(
                            &feed_writer,
                            &mut storage_state,
                            &LiveClobLogEvent {
                                receive_ms: receive.ms,
                                receive_micros: receive.micros,
                                event_ms: timestamp_ms,
                                event_micros: source_micros,
                                event_type: "best_bid_ask",
                                book_state: &book_state,
                                current_window: state.current_window.as_ref(),
                                asset_id: asset_id.as_deref(),
                                source_topic: source_topic.as_deref(),
                                connection_id: &connection_id,
                                payload_json: payload_json.as_deref(),
                                details_json: details_json.as_deref(),
                            },
                            ),
                        );

                        pending_decision.mark_dirty(receive.ms, receive.micros);
                    }

                    FeedMessage::ClobMetaEvent {
                        event_type,
                        timestamp_ms,
                        timestamp_us: source_micros,
                        asset_id,
                        source_topic,
                        connection_id,
                        payload_json,
                        details_json,
                    } => {
                        let receive = capture_receive_times(&clock);
                        if let Some(event) = storage_state.prepare_clob_meta(FeedEvent {
                            id: None,
                            received_at_ms: receive.ms,
                            event_at_ms: timestamp_ms,
                            received_at_us: receive.micros,
                            event_at_us: source_micros,
                            source: "clob".to_string(),
                            event_type,
                            source_topic,
                            source_symbol: None,
                            connection_id: Some(connection_id),
                            sequence_key: None,
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            asset_id,
                            price: None,
                            trade_size: None,
                            signed_quantity: None,
                            best_bid: None,
                            best_ask: None,
                            bid_size: None,
                            ask_size: None,
                            depth_bid_notional: None,
                            depth_ask_notional: None,
                            depth_imbalance: None,
                            microprice: None,
                            payload_json,
                            details_json,
                            fidelity: ReplayFidelity::RawEvent,
                        }) {
                            mark_capture_enqueue_result(
                                live_trading_monitor.as_mut(),
                                enqueue_counted_feed_event(&feed_writer, &mut storage_state, event),
                            );
                        }
                    }

                    FeedMessage::FeedConnected { name, connection_id } => {
                        info!(feed = %name, "feed connected");
                        feed_health_tracker.note_connected(&name, clock.now());
                        enqueue_feed_health_event(&persistence_writer, OwnedFeedHealthLogEvent {
                            timestamp_ms: clock.now(),
                            timestamp_micros: Some(now_us()),
                            source: name,
                            event_type: "connected".to_string(),
                            connection_id: Some(connection_id),
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            details_json: None,
                        }, live_trading_monitor.as_mut());
                    }

                    FeedMessage::FeedDisconnected {
                        name,
                        connection_id,
                        cause_class,
                        details_json,
                    } => {
                        warn!(feed = %name, cause_class, "feed disconnected");
                        feed_health_tracker.note_disconnected(&name, cause_class, clock.now());
                        enqueue_feed_health_event(&persistence_writer, OwnedFeedHealthLogEvent {
                            timestamp_ms: clock.now(),
                            timestamp_micros: Some(now_us()),
                            source: name,
                            event_type: "disconnected".to_string(),
                            connection_id,
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            details_json,
                        }, live_trading_monitor.as_mut());
                    }

                    FeedMessage::ChainlinkStale {
                        connection_id,
                        details_json,
                    } => {
                        warn!("chainlink price is stale");
                        let chainlink_stale_ms = clock.now();
                        let chainlink_stale_micros = Some(now_us());
                        state.signal_state.chainlink_price = None;
                        enqueue_feed_health_event(&persistence_writer, OwnedFeedHealthLogEvent {
                            timestamp_ms: chainlink_stale_ms,
                            timestamp_micros: chainlink_stale_micros,
                            source: "chainlink".to_string(),
                            event_type: "stale".to_string(),
                            connection_id,
                            market_id: state.current_window.as_ref().map(|w| w.market_id.clone()),
                            details_json,
                        }, live_trading_monitor.as_mut());
                        pending_decision.mark_dirty(chainlink_stale_ms, chainlink_stale_micros);
                    }
                }
                }
                if let Some((decision_at_ms, decision_at_us)) = pending_decision.take()
                    && !enqueue_strategy_evaluation(
                        &mut state,
                        &momentum,
                        &config,
                        &strategy_worker,
                        live_trading_monitor
                            .as_ref()
                            .is_none_or(LiveTradingMonitor::can_submit_orders),
                        decision_at_ms,
                        decision_at_us,
                    )
                {
                    mark_live_submission_blocked(
                        live_trading_monitor.as_mut(),
                        "strategy worker queue rejected evaluation",
                    );
                }
            }

            urgent = urgent_command_rx.recv() => {
                match urgent {
                    Some(UrgentRuntimeCommand::StrategyOutput(output)) => match *output {
                        StrategyWorkerOutput::Decision(output) => {
                        if stale_live_decision_output(
                            &output,
                            state.latest_decision_sequence,
                            clock.now(),
                            &config,
                        ) {
                            reject_stale_live_decision_output(
                                &output,
                                &persistence_writer,
                                &strategy_worker,
                                live_trading_monitor.as_mut(),
                            );
                        } else {
                            handle_strategy_decision_output(
                                output,
                                &persistence_writer,
                                &strategy_worker,
                                live_submission_queue.as_ref(),
                                live_trading_monitor.as_mut(),
                            );
                        }
                        }
                    },
                    Some(UrgentRuntimeCommand::LiveFeedback(feedback)) => {
                        apply_live_submission_feedback(
                            live_trading_monitor.as_mut(),
                            &strategy_worker,
                            feedback,
                        );
                    }
                    Some(UrgentRuntimeCommand::LiveMonitorOutput(output)) => {
                        live_control_inflight = false;
                        if let Some(monitor) = live_trading_monitor.as_mut() {
                            apply_live_monitor_worker_output(monitor, *output);
                        }
                    }
                    None => {}
                }
            }

            command = runtime_command_rx.recv() => {
                let Some(command) = command else {
                    warn!("runtime command channel closed, shutting down");
                    break;
                };

                match command {
                    RuntimeCommand::MarketDiscovery(MarketDiscoveryEvent::NewWindow(window)) => {
                        info!(
                            market_id = %window.market_id,
                            question = %window.question,
                            "new market window discovered"
                        );

                        if !persistence_writer.try_enqueue(LivePersistenceEvent::MarketUpsert(
                            Box::new(window.clone()),
                        )) {
                            mark_live_submission_blocked(
                                live_trading_monitor.as_mut(),
                                "runtime persistence queue rejected market metadata",
                            );
                        }

                        state.known_windows.insert(window.market_id.clone(), window.clone());

                        let now_ms = clock.now();
                        if window.start_time <= now_ms {
                            activate_window(&mut state, &window, &clob_handle);
                        } else {
                            let delay_ms = window.start_time.saturating_sub(now_ms);
                            spawn_market_activation(runtime_command_tx.clone(), window.clone(), delay_ms);
                            info!(
                                market_id = %window.market_id,
                                delay_ms,
                                "window activation scheduled"
                            );
                        }
                    }

                    RuntimeCommand::MarketDiscovery(MarketDiscoveryEvent::WindowClosed(closed_window)) => {
                        info!(market_id = %closed_window.market_id, "market window closed");

                        let closed_id = closed_window.market_id.clone();
                        if let Some(window) = state.known_windows.remove(&closed_id) {
                            let open = state.window_open_prices.remove(&closed_id).unwrap_or_else(|| {
                                warn!(market_id = %closed_id, "no cached open price captured, using latest in-memory Binance price");
                                state.signal_state.binance_price.unwrap_or(0.0)
                            });
                            let close = state.signal_state.binance_price.unwrap_or(open);
                            if !strategy_worker.try_window_closed(window.clone(), open, close, clock.now()) {
                                warn!(market_id = %closed_id, "strategy worker dropped market-close event");
                            }
                            let first_attempt_at_ms = window
                                .end_time
                                .saturating_add(config.resolution_initial_delay_ms)
                                .max(clock.now());
                            schedule_pending_resolution(
                                &mut pending_resolutions,
                                &window,
                                first_attempt_at_ms,
                                false,
                            );

                            if state.current_window.as_ref().is_some_and(|w| w.market_id == closed_id) {
                                state.current_window = None;
                                state.signal_state.book_state = crate::types::BookState::default();
                            }
                        }
                    }
                    RuntimeCommand::ActivateWindow(window) => {
                        activate_window(&mut state, &window, &clob_handle);
                    }
                    RuntimeCommand::Timer(RuntimeTimerCommand::StorageReport) => {
                        let rows = storage_state.take_row_counts();
                        let row_summary = rows
                            .iter()
                            .map(|(key, count)| format!("{key}={count}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let writer_snapshot = feed_writer.snapshot();
                        let now = clock.now();
                        let db_bytes = file_size_bytes(db_path);
                        let wal_bytes = file_size_bytes(&format!("{db_path}-wal"));
                        let total_db_bytes = db_bytes.saturating_add(wal_bytes);
                        let capture_health = runtime_capture_health(
                            &writer_snapshot,
                            &row_summary,
                            now,
                            config.feed_event_writer_max_lag_ms,
                        );
                        info!(
                            queued_rows = writer_snapshot.enqueued,
                            persisted_rows = writer_snapshot.persisted,
                            dropped_rows = writer_snapshot.dropped,
                            queue_full = writer_snapshot.queue_full,
                            write_errors = writer_snapshot.write_errors,
                            terminal_write_errors = writer_snapshot.terminal_write_errors,
                            max_batch_write_ms = writer_snapshot.max_write_ms,
                            last_persisted_at_ms = writer_snapshot.last_persisted_at_ms,
                            db_bytes,
                            wal_bytes,
                            total_db_bytes,
                            max_db_bytes = config.live_runtime_max_db_bytes,
                            rows = row_summary,
                            replay_runtime_capture_health = %capture_health,
                            decision_dirty_events = pending_decision.dirty_events,
                            decision_coalesced_events = pending_decision.coalesced_events,
                            decision_flushed_evaluations = pending_decision.flushed_evaluations,
                            "live storage writer rollup"
                        );
                        if config.live_runtime_max_db_bytes > 0
                            && total_db_bytes > config.live_runtime_max_db_bytes
                        {
                            warn!(
                                total_db_bytes,
                                max_db_bytes = config.live_runtime_max_db_bytes,
                                "runtime database size exceeded configured guard"
                            );
                            mark_live_submission_blocked(
                                live_trading_monitor.as_mut(),
                                "runtime database size exceeded configured guard",
                            );
                        }
                        if !persistence_writer.try_enqueue(LivePersistenceEvent::RunMetadata(
                            runtime_capture_metadata_rows(
                                config.feed_event_storage_profile,
                                now,
                                capture_health.to_string(),
                                row_summary.clone(),
                            ),
                        )) {
                            mark_live_submission_blocked(
                                live_trading_monitor.as_mut(),
                                "runtime persistence queue rejected capture metadata",
                            );
                        }
                        let strategy_snapshot = strategy_worker.snapshot();
                        info!(
                            strategy_enqueued = strategy_snapshot.enqueued,
                            strategy_replaced = strategy_snapshot.replaced,
                            strategy_processed = strategy_snapshot.processed,
                            strategy_dropped = strategy_snapshot.dropped,
                            strategy_output_dropped = strategy_snapshot.output_dropped,
                            strategy_last_processed_at_ms = strategy_snapshot.last_processed_at_ms,
                            "strategy worker rollup"
                        );
                    }
                    RuntimeCommand::Timer(RuntimeTimerCommand::FeedHealthReport) => {
                        log_feed_health_rollups(&feed_health_tracker.take_rollups(clock.now()));
                    }
                    RuntimeCommand::Timer(RuntimeTimerCommand::ResolutionRetry) => {
                        let ready_pending =
                            take_ready_pending_resolutions(&mut pending_resolutions, clock.now());
                        for pending in ready_pending {
                            let slug = pending.window.slug.clone();
                            let gamma_api_url = config.gamma_api_url.clone();
                            let tx = resolution_result_tx.clone();
                            tokio::spawn(async move {
                                let result =
                                    crate::market_discovery::fetch_resolution_once(&gamma_api_url, &slug)
                                        .await;
                                let _ = tx.send(ResolutionFetchResult { pending, result });
                            });
                        }
                    }
                    RuntimeCommand::ResolutionResult(resolution) => {
                        let mut resolution = *resolution;
                        match resolution.result {
                        Ok(Some(outcome)) => {
                            if !strategy_worker.try_authoritative_resolution(
                                resolution.pending.window,
                                outcome,
                                resolution.pending.seeded_from_startup,
                            ) {
                                warn!(
                                    market_id = %resolution.pending.market_id,
                                    "strategy worker dropped authoritative settlement"
                                );
                            }
                        }
                        Ok(None) => {
                            tracing::debug!(
                                market_id = %resolution.pending.market_id,
                                slug = %resolution.pending.window.slug,
                                "authoritative settlement still unresolved, will retry"
                            );
                            resolution.pending.next_attempt_at_ms =
                                clock.now().saturating_add(config.resolution_poll_delay_ms);
                            pending_resolutions.insert(
                                resolution.pending.market_id.clone(),
                                resolution.pending,
                            );
                        }
                        Err(error) => {
                            warn!(
                                market_id = %resolution.pending.market_id,
                                slug = %resolution.pending.window.slug,
                                "authoritative settlement fetch failed: {error}"
                            );
                            resolution.pending.next_attempt_at_ms =
                                clock.now().saturating_add(config.resolution_poll_delay_ms);
                            pending_resolutions.insert(
                                resolution.pending.market_id.clone(),
                                resolution.pending,
                            );
                        }
                        }
                    }
                    RuntimeCommand::Timer(RuntimeTimerCommand::ReadonlyPoll) => {
                        if !readonly_refresh_inflight
                            && let Some(monitor) = readonly_monitor.as_ref()
                        {
                            monitor.spawn_account_refresh(
                                db_path.to_string(),
                                config.clone(),
                                clock.now(),
                                readonly_refresh_tx.clone(),
                            );
                            readonly_refresh_inflight = true;
                        }
                    }
                    RuntimeCommand::ReadonlyRefresh(result) => {
                        readonly_refresh_inflight = false;
                        if let Some(monitor) = readonly_monitor.as_mut() {
                            monitor.apply_refresh_result(*result);
                        }
                    }
                    RuntimeCommand::Timer(RuntimeTimerCommand::ReadonlyRollup) => {
                        if let Some(monitor) = readonly_monitor.as_ref() {
                            monitor.log_shadow_rollup(&strategy_worker.snapshot().stats);
                        }
                    }
                    RuntimeCommand::Timer(RuntimeTimerCommand::LiveControlPoll) => {
                        if !live_control_inflight
                            && let Some(monitor) = live_trading_monitor.as_ref()
                        {
                            spawn_live_monitor_worker(
                                monitor,
                                LiveMonitorWorkerKind::Controls,
                                db_path.to_string(),
                                config.clone(),
                                live_monitor_tx.clone(),
                            );
                            live_control_inflight = true;
                        }
                    }
                    RuntimeCommand::Timer(RuntimeTimerCommand::LiveRemotePoll) => {
                        if !live_remote_refresh_inflight
                            && let Some(monitor) = live_trading_monitor.as_ref()
                        {
                            spawn_live_monitor_worker(
                                monitor,
                                LiveMonitorWorkerKind::RemoteRefresh,
                                db_path.to_string(),
                                config.clone(),
                                live_monitor_tx.clone(),
                            );
                            live_remote_refresh_inflight = true;
                        }
                    }
                    RuntimeCommand::LiveMonitorOutput(output) => {
                        live_remote_refresh_inflight = false;
                        if let Some(monitor) = live_trading_monitor.as_mut() {
                            apply_live_monitor_worker_output(monitor, *output);
                        }
                    }
                }
            }

            _ = &mut shutdown_rx => {
                info!("shutdown signal received, shutting down");
                break;
            }
        }
    }

    let _ = strategy_worker.try_flush_all(clock.now());
    for _ in 0..25 {
        let mut drained = false;
        while let Ok(command) = urgent_command_rx.try_recv() {
            drained = true;
            match command {
                UrgentRuntimeCommand::StrategyOutput(output) => match *output {
                    StrategyWorkerOutput::Decision(output) => {
                        if stale_live_decision_output(
                            &output,
                            state.latest_decision_sequence,
                            clock.now(),
                            &config,
                        ) {
                            reject_stale_live_decision_output(
                                &output,
                                &persistence_writer,
                                &strategy_worker,
                                live_trading_monitor.as_mut(),
                            );
                        } else {
                            handle_strategy_decision_output(
                                output,
                                &persistence_writer,
                                &strategy_worker,
                                live_submission_queue.as_ref(),
                                live_trading_monitor.as_mut(),
                            );
                        }
                    }
                },
                UrgentRuntimeCommand::LiveFeedback(feedback) => {
                    apply_live_submission_feedback(
                        live_trading_monitor.as_mut(),
                        &strategy_worker,
                        feedback,
                    );
                }
                UrgentRuntimeCommand::LiveMonitorOutput(output) => {
                    if let Some(monitor) = live_trading_monitor.as_mut() {
                        apply_live_monitor_worker_output(monitor, *output);
                    }
                }
            }
        }
        if drained {
            continue;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let final_stats = strategy_worker.snapshot().stats;
    info!(
        starting_balance = final_stats.starting_balance,
        final_balance = final_stats.current_balance,
        total_pnl = final_stats.total_pnl,
        trades = final_stats.total_trades,
        wins = final_stats.wins,
        losses = final_stats.losses,
        win_rate = format!("{:.1}%", final_stats.win_rate * 100.0),
        max_drawdown = format!("{:.1}%", final_stats.max_drawdown_pct * 100.0),
        hwm = final_stats.high_water_mark,
        "final bankroll stats"
    );

    if let Some(monitor) = readonly_monitor.as_mut() {
        monitor.finish_stopped(&db, &clock)?;
    }
    if let Some(monitor) = live_trading_monitor.as_mut() {
        monitor.finish_stopped(&db, &clock)?;
    }

    let writer_snapshot = feed_writer.snapshot();
    let persistence_snapshot = persistence_writer.snapshot();
    info!(
        queued_rows = writer_snapshot.enqueued,
        persisted_rows = writer_snapshot.persisted,
        dropped_rows = writer_snapshot.dropped,
        queue_full = writer_snapshot.queue_full,
        write_errors = writer_snapshot.write_errors,
        max_batch_write_ms = writer_snapshot.max_write_ms,
        last_persisted_at_ms = writer_snapshot.last_persisted_at_ms,
        "stopping feed writer"
    );
    log_persistence_writer_snapshot(&persistence_snapshot);
    let strategy_snapshot = strategy_worker.snapshot();
    info!(
        enqueued = strategy_snapshot.enqueued,
        replaced = strategy_snapshot.replaced,
        processed = strategy_snapshot.processed,
        dropped = strategy_snapshot.dropped,
        output_dropped = strategy_snapshot.output_dropped,
        last_processed_at_ms = strategy_snapshot.last_processed_at_ms,
        shutdown_timed_out = strategy_snapshot.shutdown_timed_out,
        "stopping strategy worker"
    );
    let worker_shutdown_timeout = Duration::from_millis(config.worker_shutdown_timeout_ms.max(1));
    strategy_worker.shutdown_with_timeout(worker_shutdown_timeout);
    persistence_writer.shutdown_with_timeout(worker_shutdown_timeout);
    feed_writer.shutdown_with_timeout(worker_shutdown_timeout);

    db.close();
    info!("database closed, goodbye");

    Ok(())
}

impl LiveTradingMonitor {
    /// Return whether new venue submissions may be attempted right now.
    fn can_submit_orders(&self) -> bool {
        self.state == "armed" && self.blocked_reason.is_none()
    }

    /// Return whether live risk/degradation checks are terminal right now.
    fn terminal_risk_is_active(&self) -> bool {
        matches!(self.state.as_str(), "armed" | "stop_after_flat")
    }

    /// Apply every queued operator command in durable request order.
    async fn apply_pending_controls(
        &mut self,
        db_path: &str,
        config: &Config,
        clock: &dyn Clock,
    ) -> anyhow::Result<()> {
        let commands = {
            let db = Database::open_runtime(db_path)?;
            let commands = db.pending_live_control_commands(self.session_id)?;
            db.close();
            commands
        };
        for command in commands {
            let command_id = command
                .id
                .context("pending live-control command has no id")?;
            let action = match LiveControlAction::from_str(&command.action) {
                Ok(action) => action,
                Err(error) => {
                    let details = json!({ "error": error.to_string() }).to_string();
                    let db = Database::open_runtime(db_path)?;
                    db.update_live_control_command_status(
                        command_id,
                        clock.now(),
                        "rejected",
                        Some(&details),
                    )?;
                    db.close();
                    continue;
                }
            };
            let applied_at_ms = clock.now();
            let result = self
                .apply_control_action(
                    db_path,
                    config,
                    clock,
                    action,
                    &command.actor,
                    &command.reason,
                )
                .await;
            let (status, details) = match result {
                Ok(details) => ("applied", details),
                Err(error) => {
                    warn!(
                        command_id,
                        action = %action.as_str(),
                        "live-control command rejected: {error}"
                    );
                    (
                        "rejected",
                        json!({
                            "action": action.as_str(),
                            "error": error.to_string(),
                        }),
                    )
                }
            };
            let db = Database::open_runtime(db_path)?;
            db.update_live_control_command_status(
                command_id,
                applied_at_ms,
                status,
                Some(&details.to_string()),
            )?;
            db.close();
        }
        Ok(())
    }

    /// Refresh preflight, account, and activity state from the sidecar.
    async fn refresh_remote_state(
        &mut self,
        db_path: &str,
        config: &Config,
        clock: &dyn Clock,
    ) -> anyhow::Result<Vec<String>> {
        let preflight = match self.sidecar.preflight(config).await {
            Ok(preflight) => preflight,
            Err(error) => {
                return self
                    .record_remote_refresh_failure(
                        db_path,
                        clock,
                        "preflight_refresh_failure",
                        error,
                    )
                    .await;
            }
        };
        let account = match self.sidecar.account_state().await {
            Ok(account) => account,
            Err(error) => {
                return self
                    .record_remote_refresh_failure(db_path, clock, "account_refresh_failure", error)
                    .await;
            }
        };
        let activity = match self.sidecar.activity().await {
            Ok(activity) => activity,
            Err(error) => {
                return self
                    .record_remote_refresh_failure(
                        db_path,
                        clock,
                        "activity_refresh_failure",
                        error,
                    )
                    .await;
            }
        };
        let db = Database::open_runtime(db_path)?;
        db.log_live_account_snapshot(&live_account_snapshot(self.session_id, &account))?;
        self.persist_activity_recovery(&db, &activity)?;
        let mut issues = live_gate_issues(&preflight, &account, &activity, config);
        issues.extend(runtime_capture_issues_for_live(&db, config, clock.now())?);
        issues.sort();
        issues.dedup();
        self.preflight = Some(preflight);
        self.account = Some(account);
        self.activity = Some(activity);
        self.blocked_reason = issues.first().cloned();
        let mut breach = None;
        if self.terminal_risk_is_active() {
            breach = self.evaluate_terminal_remote_state(&db, clock.now(), &issues)?;
            if breach.is_none() {
                if let Some(risk) = self.risk.as_mut() {
                    breach = risk.update_from_account(
                        self.account
                            .as_ref()
                            .context("missing refreshed live account")?,
                        config,
                    );
                } else {
                    let account = self
                        .account
                        .as_ref()
                        .context("missing refreshed live account")?;
                    let mut risk = LiveRiskMonitor::new(account);
                    breach = risk.update_from_account(account, config);
                    self.risk = Some(risk);
                }
            }
            if breach.is_none() && db.critical_live_reconciliation_count(self.session_id)? > 0 {
                breach = Some(live_terminal_breach(
                    "critical_reconciliation_halt",
                    "critical live reconciliation event present",
                    json!({ "session_id": self.session_id }),
                ));
            }
        } else if self.risk.is_none() {
            self.risk = self.account.as_ref().map(LiveRiskMonitor::new);
        }
        self.update_session_metadata(&db)?;
        db.close();
        if let Some(breach) = breach {
            self.terminal_halt(
                db_path,
                clock,
                "system",
                &breach.reason,
                breach.event_type,
                breach.details,
            )
            .await?;
        }
        Ok(issues)
    }

    /// Record one failed remote-state refresh and maybe escalate to terminal halt.
    async fn record_remote_refresh_failure(
        &mut self,
        db_path: &str,
        clock: &dyn Clock,
        kind: &str,
        error: anyhow::Error,
    ) -> anyhow::Result<Vec<String>> {
        let now_ms = clock.now();
        let detail = error.to_string();
        self.blocked_reason = Some(format!("{kind}: {detail}"));
        let breach = if self.terminal_risk_is_active() {
            self.degradation
                .note(kind, &detail, now_ms, LIVE_TERMINAL_DEGRADATION_MS)
        } else {
            None
        };
        let db = Database::open_runtime(db_path)?;
        self.update_session_metadata(&db)?;
        db.close();
        if let Some(breach) = breach {
            let reason = format!("{} persisted for {}ms", breach.kind, breach.duration_ms);
            self.terminal_halt(
                db_path,
                clock,
                "system",
                &reason,
                "live_remote_degradation_halt",
                json!({
                    "kind": breach.kind,
                    "duration_ms": breach.duration_ms,
                    "detail": breach.detail,
                }),
            )
            .await?;
        }
        bail!("{kind}: {detail}")
    }

    /// Evaluate refreshed sidecar state for terminal live-money blockers.
    fn evaluate_terminal_remote_state(
        &mut self,
        db: &Database,
        now_ms: u64,
        issues: &[String],
    ) -> anyhow::Result<Option<LiveRiskBreach>> {
        if issues.iter().any(|issue| {
            issue.contains("geoblock")
                || issue.contains("auth")
                || issue.contains("replay-grade")
                || issue.contains("clock drift")
        }) {
            return Ok(Some(live_terminal_breach(
                "live_gate_terminal_halt",
                "terminal live gate failed while armed",
                json!({ "issues": issues }),
            )));
        }
        let degraded = issues.iter().any(|issue| issue.contains("user stream"))
            || self
                .preflight
                .as_ref()
                .is_some_and(|preflight| !preflight.ok);
        if degraded {
            let detail = if issues.is_empty() {
                "preflight failed without structured issue".to_string()
            } else {
                issues.join("; ")
            };
            if let Some(breach) = self.degradation.note(
                "venue_or_user_stream_degraded",
                &detail,
                now_ms,
                LIVE_TERMINAL_DEGRADATION_MS,
            ) {
                return Ok(Some(live_terminal_breach(
                    "live_remote_degradation_halt",
                    "live remote state stayed degraded past terminal threshold",
                    json!({
                        "kind": breach.kind,
                        "duration_ms": breach.duration_ms,
                        "detail": breach.detail,
                    }),
                )));
            }
        } else {
            self.degradation.clear();
        }
        if db.critical_live_reconciliation_count(self.session_id)? > 0 {
            return Ok(Some(live_terminal_breach(
                "critical_reconciliation_halt",
                "critical live reconciliation event present",
                json!({ "session_id": self.session_id }),
            )));
        }
        Ok(None)
    }

    /// Finish the current live-trading session after normal process shutdown.
    fn finish_stopped(&mut self, db: &Database, clock: &dyn Clock) -> anyhow::Result<()> {
        if self.finished {
            return Ok(());
        }
        let now = clock.now();
        let status = if self.state == "armed" {
            "disarmed"
        } else {
            self.state.as_str()
        };
        let finish_status = if matches!(status, "halted" | "unknown_order") {
            status
        } else {
            "live_stopped"
        };
        let details_json = (!matches!(status, "halted" | "unknown_order")).then(|| {
            json!({
                "previous_state": status,
                "reason": "process_shutdown",
            })
            .to_string()
        });
        db.finish_live_session(self.session_id, now, finish_status, details_json.as_deref())?;
        self.finished = true;
        Ok(())
    }

    /// Apply one parsed operator action.
    async fn apply_control_action(
        &mut self,
        db_path: &str,
        config: &Config,
        clock: &dyn Clock,
        action: LiveControlAction,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        match action {
            LiveControlAction::Preflight => {
                let issues = self.refresh_remote_state(db_path, config, clock).await?;
                Ok(json!({ "state": self.state, "issues": issues }))
            }
            LiveControlAction::Arm => self.arm(db_path, config, clock, actor, reason).await,
            LiveControlAction::Disarm => {
                ensure_state_allows_disarm(&self.state)?;
                let db = Database::open_runtime(db_path)?;
                self.set_state(&db, "disarmed", actor, reason, clock.now(), None)?;
                db.close();
                Ok(json!({ "state": self.state }))
            }
            LiveControlAction::StopAfterFlat => {
                ensure_state_is(&self.state, "armed", "stop-after-flat")?;
                let db = Database::open_runtime(db_path)?;
                self.set_state(&db, "stop_after_flat", actor, reason, clock.now(), None)?;
                db.close();
                Ok(json!({ "state": self.state }))
            }
            LiveControlAction::KillSwitch => self.kill_switch(db_path, clock, actor, reason).await,
            LiveControlAction::CancelAll => self.cancel_all(db_path, clock, actor, reason).await,
            LiveControlAction::RedeemAll => self.redeem_all(db_path, clock, actor, reason).await,
        }
    }

    /// Arm live trading only after every safety gate is healthy.
    async fn arm(
        &mut self,
        db_path: &str,
        config: &Config,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        ensure_state_is(&self.state, "disarmed", "arm")?;
        let issues = self.refresh_remote_state(db_path, config, clock).await?;
        if !issues.is_empty() {
            bail!("live arming blocked: {}", issues.join("; "));
        }
        let db = Database::open_runtime(db_path)?;
        self.set_state(&db, "armed", actor, reason, clock.now(), None)?;
        db.close();
        Ok(json!({ "state": self.state }))
    }

    /// Halt trading, attempt cancel-all, and keep the session blocked.
    async fn kill_switch(
        &mut self,
        db_path: &str,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        self.terminal_halt(
            db_path,
            clock,
            actor,
            reason,
            "kill_switch_activated",
            json!({ "reason": reason }),
        )
        .await
    }

    /// Persist a terminal halt, attempt cancel-all, and block future submissions.
    async fn terminal_halt(
        &mut self,
        db_path: &str,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
        event_type: &'static str,
        details: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let now = clock.now();
        let cancel_result = self.sidecar.cancel_all().await;
        let cancel_details = match cancel_result {
            Ok(response) => json!({ "ok": true, "response": response }),
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        };
        if let Some(risk) = self.risk.as_mut() {
            risk.mark_terminal(now, reason);
        }
        let details = json!({
            "event_type": event_type,
            "reason": reason,
            "halt_at_ms": now,
            "details": details,
            "risk": self.risk.as_ref().map(LiveRiskMonitor::to_json),
            "cancel_all": cancel_details,
        });
        let db = Database::open_runtime(db_path)?;
        let details_json = details.to_string();
        self.set_state(&db, "halted", actor, reason, now, Some(&details_json))?;
        db.log_live_reconciliation_event(&LiveReconciliationEvent {
            id: None,
            session_id: self.session_id,
            timestamp_ms: now,
            severity: "critical".to_string(),
            event_type: event_type.to_string(),
            local_value: None,
            remote_value: None,
            details_json: Some(details_json.clone()),
        })?;
        db.log_control_audit(&crate::types::ControlAuditEntry {
            id: None,
            timestamp_ms: now,
            actor: actor.to_string(),
            action: "live_terminal_halt".to_string(),
            target: Some(self.session_id.to_string()),
            details_json: Some(details_json),
        })?;
        db.close();
        Ok(json!({ "state": self.state, "details": details }))
    }

    /// Cancel every open venue order without changing the arming state.
    async fn cancel_all(
        &mut self,
        db_path: &str,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let response = self.sidecar.cancel_all().await?;
        let db = Database::open_runtime(db_path)?;
        db.log_control_audit(&crate::types::ControlAuditEntry {
            id: None,
            timestamp_ms: clock.now(),
            actor: actor.to_string(),
            action: "live_cancel_all_submitted".to_string(),
            target: Some(self.session_id.to_string()),
            details_json: Some(
                json!({
                    "reason": reason,
                    "response": response,
                })
                .to_string(),
            ),
        })?;
        db.close();
        Ok(json!({ "cancel_all": response }))
    }

    /// Trigger redemption for all redeemable positions and persist the command result.
    async fn redeem_all(
        &mut self,
        db_path: &str,
        clock: &dyn Clock,
        actor: &str,
        reason: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let now = clock.now();
        let response = self.sidecar.redeem_all().await?;
        let db = Database::open_runtime(db_path)?;
        if response.submitted > 0 {
            db.log_live_redemption(&LiveRedemption {
                id: None,
                session_id: self.session_id,
                market_id: "all".to_string(),
                detected_redeemable_at_ms: now,
                submitted_at_ms: Some(now),
                confirmed_at_ms: None,
                cash_credit_observed_at_ms: None,
                status: "submitted".to_string(),
                redeemable_value: 0.0,
                tx_hash: None,
                details_json: response.details_json.clone(),
            })?;
        }
        db.log_control_audit(&crate::types::ControlAuditEntry {
            id: None,
            timestamp_ms: now,
            actor: actor.to_string(),
            action: "live_redeem_all_submitted".to_string(),
            target: Some(self.session_id.to_string()),
            details_json: Some(
                json!({
                    "reason": reason,
                    "response": response,
                })
                .to_string(),
            ),
        })?;
        db.close();
        Ok(json!({ "redeem_all": response }))
    }

    /// Persist activity-recovered trades and user-stream health events.
    fn persist_activity_recovery(
        &self,
        db: &Database,
        activity: &LiveActivityResponse,
    ) -> anyhow::Result<()> {
        if activity.user_stream_status == LiveCheckStatus::Failed {
            db.log_live_reconciliation_event(&LiveReconciliationEvent {
                id: None,
                session_id: self.session_id,
                timestamp_ms: activity.timestamp_ms,
                severity: "critical".to_string(),
                event_type: "user_stream_unhealthy".to_string(),
                local_value: None,
                remote_value: None,
                details_json: activity.details_json.clone(),
            })?;
        }
        for trade in &activity.clob_trades {
            let Some(trade_id) = trade.trade_id.as_deref() else {
                continue;
            };
            if db.live_fill_exists(self.session_id, trade_id)? {
                continue;
            }
            let (Some(price), Some(size)) = (trade.price, trade.size) else {
                continue;
            };
            db.log_live_fill(&LiveFill {
                id: None,
                session_id: self.session_id,
                intent_id: None,
                live_order_id: None,
                venue_trade_id: Some(trade_id.to_string()),
                filled_at_ms: trade.timestamp_ms,
                price,
                size,
                fee_amount: None,
                fee_rate: None,
                liquidity_side: None,
                tx_hash: None,
                status: "confirmed_from_activity".to_string(),
                details_json: trade.details_json.clone(),
            })?;
        }
        Ok(())
    }

    /// Persist a new live-control state and refresh the session metadata.
    fn set_state(
        &mut self,
        db: &Database,
        state: &str,
        actor: &str,
        reason: &str,
        now_ms: u64,
        details_json: Option<&str>,
    ) -> anyhow::Result<()> {
        record_live_control_state(
            db,
            self.session_id,
            state,
            actor,
            reason,
            now_ms,
            details_json,
        )?;
        self.state = state.to_string();
        self.blocked_reason =
            (state == "unknown_order" || state == "halted").then(|| reason.to_string());
        self.update_session_metadata(db)?;
        Ok(())
    }

    /// Persist the current live-trading presentation state to the session row.
    fn update_session_metadata(&self, db: &Database) -> anyhow::Result<()> {
        let wallet = self
            .account
            .as_ref()
            .and_then(|account| account.wallet_address.as_deref())
            .or_else(|| {
                self.preflight
                    .as_ref()
                    .and_then(|preflight| preflight.wallet_address.as_deref())
            });
        let proxy = self
            .account
            .as_ref()
            .and_then(|account| account.proxy_wallet.as_deref())
            .or_else(|| {
                self.preflight
                    .as_ref()
                    .and_then(|preflight| preflight.proxy_wallet.as_deref())
            });
        db.update_live_session_metadata(
            self.session_id,
            &self.state,
            wallet,
            proxy,
            Some(&live_session_details_json(self)),
        )
    }
}

/// Reject arming or stop-after-flat when the live-control state is not eligible.
fn ensure_state_is(state: &str, expected: &str, action: &str) -> anyhow::Result<()> {
    if state != expected {
        bail!("{action} requires state {expected}; current state is {state}");
    }
    Ok(())
}

/// Reject disarm commands that would hide a terminal or unresolved critical state.
fn ensure_state_allows_disarm(state: &str) -> anyhow::Result<()> {
    match state {
        "armed" | "stop_after_flat" => Ok(()),
        "halted" => bail!("disarm cannot clear a halted live session"),
        "unknown_order" => bail!("disarm cannot clear unknown order state; reconcile first"),
        _ => bail!("disarm requires state armed or stop_after_flat; current state is {state}"),
    }
}

/// Build and enqueue one strategy-evaluation snapshot from current feed state.
fn enqueue_strategy_evaluation(
    state: &mut LiveState,
    momentum: &MomentumCalculator,
    config: &Config,
    strategy_worker: &StrategyWorker,
    live_trading_can_submit: bool,
    now_ms: u64,
    receive_us: Option<u64>,
) -> bool {
    let Some(window) = state.current_window.as_ref() else {
        return true;
    };
    let Some(binance_price) = state.signal_state.binance_price else {
        return true;
    };
    let window_open_price = state.window_open_prices.get(&window.market_id).copied();
    state.latest_decision_sequence = state.latest_decision_sequence.saturating_add(1);
    let decision_sequence = state.latest_decision_sequence;
    let features = SignalFeatureEngine::compute(
        &mut state.signal_state,
        Some(window),
        window_open_price,
        momentum.get(),
        now_ms,
        receive_us,
        config,
    );
    let ctx = StrategyContext {
        binance_price,
        binance_momentum: momentum.get(),
        chainlink_price: state.signal_state.chainlink_price,
        book_state: state.signal_state.book_state.clone(),
        window_open_price,
        window_time_remaining_ms: window.end_time.saturating_sub(now_ms),
        now_us: receive_us,
        features,
    };
    strategy_worker.try_evaluate(StrategyEvaluationRequest {
        decision_sequence,
        ctx,
        window: window.clone(),
        book_state: state.signal_state.book_state.clone(),
        now_ms,
        now_us: receive_us,
        live_trading_can_submit,
    })
}

/// Apply one pure decision output to asynchronous persistence and submission queues.
fn handle_strategy_decision_output(
    output: RuntimeDecisionOutput,
    persistence_writer: &LivePersistenceWriter,
    strategy_worker: &StrategyWorker,
    live_submission_queue: Option<&LiveSubmissionQueue>,
    mut live_trading_monitor: Option<&mut LiveTradingMonitor>,
) {
    log_processed_order_outcomes(output.processed_outcomes);
    log_strategy_worker_events(output.log_events);
    log_rejection_summary_events(&output.persistence_events);
    let critical_signal_ids = output
        .live_orders
        .iter()
        .map(|order| order.signal_id)
        .collect::<std::collections::HashSet<_>>();
    let mut critical_events = Vec::new();
    let mut critical_persistence_failed = false;
    for event in output.persistence_events {
        if live_order_critical_signal_event(&event, &critical_signal_ids) {
            critical_events.push(event);
            continue;
        }
        if !persistence_writer.try_enqueue(event) {
            critical_persistence_failed = true;
            mark_live_submission_blocked(
                live_trading_monitor.as_deref_mut(),
                "runtime persistence queue rejected decision evidence",
            );
        }
    }
    if critical_persistence_failed && !output.live_orders.is_empty() {
        rollback_live_orders_after_runtime_rejection(
            strategy_worker,
            &output.live_orders,
            output.now_ms,
            "runtime persistence queue rejected decision evidence",
            live_trading_monitor,
        );
        return;
    }
    if !output.live_orders.is_empty()
        && !critical_live_evidence_is_complete(&critical_events, &critical_signal_ids)
    {
        rollback_live_orders_after_runtime_rejection(
            strategy_worker,
            &output.live_orders,
            output.now_ms,
            "missing critical live decision evidence",
            live_trading_monitor,
        );
        return;
    }
    if !output.live_orders.is_empty()
        && let Some(queue) = live_submission_queue
        && let Some(window) = output.submission_window
        && !queue.try_submit(LiveSubmissionRequest {
            window,
            orders: output.live_orders.clone(),
            critical_events,
            now_ms: output.now_ms,
        })
    {
        rollback_live_orders_after_runtime_rejection(
            strategy_worker,
            &output.live_orders,
            output.now_ms,
            "live submission queue rejected strategy output",
            live_trading_monitor.as_deref_mut(),
        );
        mark_live_submission_blocked(
            live_trading_monitor,
            "live submission queue rejected strategy output",
        );
    }
}

/// Log concise rejection rollups from already-built decision output events.
fn log_rejection_summary_events(events: &[LivePersistenceEvent]) {
    for event in events {
        if let LivePersistenceEvent::RejectionSummaries(rows) = event {
            log_rejection_rollups(rows);
        }
    }
}

/// Log one concise operator line per market and strategy rejection rollup.
fn log_rejection_rollups(rows: &[crate::types::StrategyRejectionSummaryRecord]) {
    for rollup in build_rejection_rollups(rows) {
        info!(
            market_id = %rollup.market_id,
            strategy = %rollup.strategy,
            evaluations = rollup.total_count,
            top_reasons = %rollup.reason_summary,
            metrics = %rollup.metrics_summary,
            "strategy rejection rollup"
        );
    }
}

/// Return whether one live decision output is too stale to submit.
fn stale_live_decision_output(
    output: &RuntimeDecisionOutput,
    latest_sequence: u64,
    now_ms: u64,
    config: &Config,
) -> bool {
    if output.live_orders.is_empty() || output.decision_sequence == 0 {
        return false;
    }
    if output.decision_sequence < latest_sequence {
        return true;
    }
    output
        .decision_input_at_ms
        .saturating_add(config.max_live_decision_age_ms.max(1))
        < now_ms
}

/// Reject one stale live output before it can reach the venue.
fn reject_stale_live_decision_output(
    output: &RuntimeDecisionOutput,
    persistence_writer: &LivePersistenceWriter,
    strategy_worker: &StrategyWorker,
    mut live_trading_monitor: Option<&mut LiveTradingMonitor>,
) {
    let mut persistence_failed = false;
    for event in stale_decision_signal_events(output) {
        if !persistence_writer.try_enqueue(event) {
            mark_live_submission_blocked(
                live_trading_monitor.as_deref_mut(),
                "runtime persistence queue rejected stale-decision evidence",
            );
            persistence_failed = true;
            break;
        }
    }
    rollback_live_orders_after_runtime_rejection(
        strategy_worker,
        &output.live_orders,
        output.now_ms,
        "stale live decision output",
        live_trading_monitor,
    );
    if persistence_failed {
        warn!(
            orders = output.live_orders.len(),
            "rolled back stale live decision after evidence queue failure"
        );
    }
}

/// Build rejected signal evidence for live orders discarded as stale.
fn stale_decision_signal_events(output: &RuntimeDecisionOutput) -> Vec<LivePersistenceEvent> {
    let signal_ids = output
        .live_orders
        .iter()
        .map(|order| order.signal_id)
        .collect::<std::collections::HashSet<_>>();
    output
        .persistence_events
        .iter()
        .filter_map(|event| match event {
            LivePersistenceEvent::Signal {
                signal_id,
                signal,
                market_id,
                execution_fidelity,
                ..
            } if signal_ids.contains(signal_id) => Some(LivePersistenceEvent::Signal {
                signal_id: *signal_id,
                signal: signal.clone(),
                market_id: market_id.clone(),
                execution_fidelity: *execution_fidelity,
                order_submitted_at_ms: None,
                expected_arrival_at_ms: None,
                decision_status: "rejected".to_string(),
                rejection_reason: Some("stale_decision_output".to_string()),
            }),
            _ => None,
        })
        .collect()
}

/// Return whether one event is critical evidence for a live order in this output.
fn live_order_critical_signal_event(
    event: &LivePersistenceEvent,
    signal_ids: &std::collections::HashSet<i64>,
) -> bool {
    matches!(
        event,
        LivePersistenceEvent::Signal { signal_id, .. } if signal_ids.contains(signal_id)
    )
}

/// Return whether every live order signal has exactly one critical evidence row.
fn critical_live_evidence_is_complete(
    events: &[LivePersistenceEvent],
    signal_ids: &std::collections::HashSet<i64>,
) -> bool {
    let event_ids = events
        .iter()
        .filter_map(|event| match event {
            LivePersistenceEvent::Signal { signal_id, .. } => Some(*signal_id),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    signal_ids
        .iter()
        .all(|signal_id| event_ids.contains(signal_id))
}

/// Release in-memory pending live orders when runtime queues reject them.
fn rollback_live_orders_after_runtime_rejection(
    strategy_worker: &StrategyWorker,
    orders: &[QueuedOrderIntent],
    now_ms: u64,
    reason: &str,
    mut monitor: Option<&mut LiveTradingMonitor>,
) {
    let rejected_signal_ids = orders
        .iter()
        .map(|order| order.signal_id)
        .collect::<Vec<_>>();
    let releases = orders
        .iter()
        .map(|order| (order.strategy.clone(), order.reserved_cost))
        .collect::<Vec<_>>();
    if !strategy_worker.try_apply_live_submission_feedback(Vec::new(), rejected_signal_ids, now_ms)
    {
        mark_live_submission_blocked_from_reborrow(
            &mut monitor,
            "strategy worker rejected runtime queue rollback",
        );
    }
    if !strategy_worker.try_release_reservations(releases) {
        mark_live_submission_blocked_from_reborrow(
            &mut monitor,
            "strategy worker rejected runtime queue reserve release",
        );
    }
    warn!(
        reason,
        orders = orders.len(),
        "rolled back live orders before venue submission"
    );
}

/// Mark a live submission blocker through a reborrowed optional monitor.
fn mark_live_submission_blocked_from_reborrow(
    monitor: &mut Option<&mut LiveTradingMonitor>,
    reason: &str,
) {
    if let Some(monitor) = monitor.as_mut()
        && matches!(monitor.state.as_str(), "armed" | "stop_after_flat")
    {
        monitor.blocked_reason = Some(reason.to_string());
    }
}

/// Mark live submissions blocked when a worker queue rejects critical work.
fn mark_live_submission_blocked(monitor: Option<&mut LiveTradingMonitor>, reason: &str) {
    if let Some(monitor) = monitor
        && matches!(monitor.state.as_str(), "armed" | "stop_after_flat")
    {
        monitor.blocked_reason = Some(reason.to_string());
    }
}

/// Mark live submission blocked when replay-grade capture backpressure drops evidence.
fn mark_capture_enqueue_result(monitor: Option<&mut LiveTradingMonitor>, accepted: bool) {
    if !accepted {
        mark_live_submission_blocked(
            monitor,
            "replay-grade capture queue rejected decision input",
        );
    }
}

/// Apply feedback emitted by the live submission worker.
fn apply_live_submission_feedback(
    mut monitor: Option<&mut LiveTradingMonitor>,
    strategy_worker: &StrategyWorker,
    feedback: LiveSubmissionFeedback,
) {
    let applied_feedback = if feedback.fills.is_empty() && feedback.rejected_signal_ids.is_empty() {
        true
    } else {
        strategy_worker.try_apply_live_submission_feedback(
            feedback.fills,
            feedback.rejected_signal_ids,
            feedback.now_ms,
        )
    };
    if !applied_feedback {
        if let Some(monitor) = monitor.as_deref_mut() {
            mark_live_submission_blocked(
                Some(monitor),
                "strategy worker rejected live submission feedback",
            );
        }
        return;
    }
    if !feedback.releases.is_empty() && !strategy_worker.try_release_reservations(feedback.releases)
    {
        if let Some(monitor) = monitor.as_deref_mut() {
            mark_live_submission_blocked(Some(monitor), "strategy worker rejected reserve release");
        }
        return;
    }
    if let Some(update) = feedback.state_update
        && let Some(monitor) = monitor
    {
        monitor.state = update.state.to_string();
        monitor.blocked_reason = Some(update.reason);
    }
}

/// Start one live monitor worker if no previous monitor task is still running.
fn spawn_live_monitor_worker(
    monitor: &LiveTradingMonitor,
    kind: LiveMonitorWorkerKind,
    db_path: String,
    config: Config,
    tx: tokio::sync::mpsc::UnboundedSender<LiveMonitorWorkerOutput>,
) {
    let mut worker_monitor = monitor.clone();
    tokio::spawn(async move {
        let clock = SystemClock;
        let result = match kind {
            LiveMonitorWorkerKind::Controls => {
                worker_monitor
                    .apply_pending_controls(&db_path, &config, &clock)
                    .await
            }
            LiveMonitorWorkerKind::RemoteRefresh => worker_monitor
                .refresh_remote_state(&db_path, &config, &clock)
                .await
                .map(|_| ()),
        };
        let result = result
            .map(|()| worker_monitor)
            .map_err(|error| error.to_string());
        let _ = tx.send(LiveMonitorWorkerOutput { kind, result });
    });
}

/// Merge one background monitor result without clearing terminal local state.
fn apply_live_monitor_worker_output(
    monitor: &mut LiveTradingMonitor,
    output: LiveMonitorWorkerOutput,
) {
    let kind = output.kind;
    match output.result {
        Ok(updated) => merge_live_monitor_state(monitor, updated, kind),
        Err(error) => {
            if matches!(monitor.state.as_str(), "armed" | "stop_after_flat") {
                monitor.blocked_reason = Some(format!("{:?} worker failed: {error}", output.kind));
            }
            warn!(kind = ?output.kind, %error, "live monitor worker failed");
        }
    }
}

/// Merge a worker-owned monitor snapshot back into the runtime snapshot.
fn merge_live_monitor_state(
    target: &mut LiveTradingMonitor,
    updated: LiveTradingMonitor,
    kind: LiveMonitorWorkerKind,
) {
    if kind == LiveMonitorWorkerKind::RemoteRefresh {
        merge_live_remote_refresh_state(target, updated);
        return;
    }
    let target_terminal = matches!(target.state.as_str(), "halted" | "unknown_order");
    let updated_terminal = matches!(updated.state.as_str(), "halted" | "unknown_order");
    if target_terminal && !updated_terminal {
        target.preflight = updated.preflight;
        target.account = updated.account;
        target.activity = updated.activity;
        target.risk = updated.risk;
        target.degradation = updated.degradation;
        return;
    }
    *target = updated;
}

/// Merge remote health/account data without overwriting newer control state.
fn merge_live_remote_refresh_state(target: &mut LiveTradingMonitor, updated: LiveTradingMonitor) {
    let target_terminal = matches!(target.state.as_str(), "halted" | "unknown_order");
    let updated_terminal = matches!(updated.state.as_str(), "halted" | "unknown_order");
    target.preflight = updated.preflight;
    target.account = updated.account;
    target.activity = updated.activity;
    target.risk = updated.risk;
    target.degradation = updated.degradation;
    if updated_terminal || !target_terminal {
        target.blocked_reason = updated.blocked_reason;
    }
    if updated_terminal {
        target.state = updated.state;
        target.finished = updated.finished;
    }
}

/// Bootstrap a disarmed live-trading runtime session.
async fn bootstrap_live_trading_runtime(
    config: &Config,
    db: Database,
    db_path: &str,
    fallback_balance: f64,
    clock: &dyn Clock,
) -> anyhow::Result<(Database, LiveTradingRuntimeBootstrap)> {
    let started_at_ms = clock.now();
    if config.feed_event_storage_profile == FeedEventStorageProfile::Compact {
        bail!(
            "live_trading requires replay-grade feed capture; FEED_EVENT_STORAGE_PROFILE=compact is descriptive-only"
        )
    }
    if db.terminal_live_trading_halt_exists()? {
        bail!(
            "live_trading cannot start against a DB with a halted or unknown-order live session; run live-closeout and start a new run DB"
        );
    }
    let enabled_strategies = serde_json::to_string(&config.enabled_strategy_names())
        .context("serializing strategies")?;
    let session_id = db.insert_live_session(&LiveSession {
        id: None,
        started_at_ms,
        ended_at_ms: None,
        status: "disarmed".to_string(),
        execution_mode: config.execution_mode.as_str().to_string(),
        wallet_address: None,
        proxy_wallet: None,
        enabled_strategies_json: enabled_strategies,
        config_fingerprint: live_trading_config_fingerprint(config),
        cash_cap_usd: config.live_session_cash_cap_usd,
        details_json: Some(json!({ "state": "disarmed" }).to_string()),
    })?;
    record_live_control_state(
        &db,
        session_id,
        "disarmed",
        "system",
        "live_trading starts disarmed",
        started_at_ms,
        None,
    )?;
    let mut monitor = LiveTradingMonitor {
        sidecar: LiveSidecarClient::from_config(config),
        session_id,
        state: "disarmed".to_string(),
        preflight: None,
        account: None,
        activity: None,
        risk: None,
        degradation: LiveDegradationTracker::default(),
        blocked_reason: None,
        finished: false,
    };
    db.close();
    if let Err(error) = monitor.refresh_remote_state(db_path, config, clock).await {
        warn!(
            session_id,
            "live_trading started with degraded sidecar state: {error}"
        );
        monitor.blocked_reason = Some(error.to_string());
        let db = Database::open_runtime(db_path)?;
        monitor.update_session_metadata(&db)?;
        db.close();
    }
    let live_starting_balance = monitor
        .account
        .as_ref()
        .map_or(fallback_balance, |account| {
            account
                .cash_available
                .max(0.0)
                .min(config.live_session_cash_cap_usd)
        });
    info!(
        session_id,
        live_starting_balance,
        state = %monitor.state,
        "live_trading runtime bootstrapped disarmed"
    );
    let db = Database::open_runtime(db_path)?;
    Ok((
        db,
        LiveTradingRuntimeBootstrap {
            live_starting_balance,
            monitor,
        },
    ))
}

/// Convert one sidecar account response into the persistent account snapshot.
fn live_account_snapshot(session_id: i64, account: &LiveAccountState) -> LiveAccountSnapshot {
    LiveAccountSnapshot {
        id: None,
        session_id,
        timestamp_ms: account.timestamp_ms,
        cash_available: account.cash_available,
        cash_reserved_for_orders: account.cash_reserved_for_orders,
        inventory_mark_value: account.inventory_mark_value,
        redeemable_value: account.redeemable_value,
        pending_redeem_value: account.pending_redeem_value,
        total_equity: account.total_equity,
        allowance_available: account.allowance_available,
        details_json: account.details_json.clone(),
    }
}

/// Return all current arming blockers from venue and local safety state.
fn live_gate_issues(
    preflight: &LivePreflightResponse,
    account: &LiveAccountState,
    activity: &LiveActivityResponse,
    config: &Config,
) -> Vec<String> {
    let mut issues = Vec::new();
    if !preflight.ok {
        issues.push("preflight failed".to_string());
    }
    if preflight.geoblock_status == LiveCheckStatus::Failed {
        issues.push("geoblock check failed".to_string());
    }
    if preflight.auth_status == LiveCheckStatus::Failed {
        issues.push("auth bootstrap failed".to_string());
    }
    if preflight.clock_status == LiveCheckStatus::Failed {
        issues.push("clock drift check failed".to_string());
    }
    if preflight.user_stream_status == LiveCheckStatus::Failed
        || activity.user_stream_status == LiveCheckStatus::Failed
    {
        issues.push("user stream is unhealthy".to_string());
    }
    if account.cash_available + CASH_CHANGE_EPSILON_USD < config.live_min_required_cash_usd {
        issues.push("cash below live minimum".to_string());
    }
    if account.allowance_available.is_none() {
        issues.push("allowance unavailable".to_string());
    }
    if config.feed_event_storage_profile == FeedEventStorageProfile::Compact {
        issues.push("feed capture is not replay-grade".to_string());
    }
    issues.extend(preflight.errors.clone());
    issues.sort();
    issues.dedup();
    issues
}

/// Return one deterministic live-trading config fingerprint.
pub(crate) fn live_trading_config_fingerprint(config: &Config) -> String {
    json!({
        "execution_mode": config.execution_mode.as_str(),
        "live_sidecar_url": config.live_sidecar_url,
        "enabled_strategies": config.enabled_strategy_names(),
        "cash_cap_usd": config.live_session_cash_cap_usd,
        "max_single_order_usd": config.live_max_single_order_usd,
        "max_open_notional_usd": config.live_max_open_notional_usd,
        "max_daily_loss_usd": config.live_max_daily_loss_usd,
        "max_session_drawdown_usd": config.live_max_session_drawdown_usd,
        "feed_event_storage_profile": config.feed_event_storage_profile.as_str(),
    })
    .to_string()
}

/// Build compact live-trading details for the session row.
fn live_session_details_json(monitor: &LiveTradingMonitor) -> String {
    json!({
        "state": monitor.state,
        "blocked_reason": monitor.blocked_reason,
        "preflight": monitor.preflight.as_ref().map(live_preflight_summary_json),
        "account": monitor.account.as_ref().map(live_account_summary_json),
        "activity": monitor.activity.as_ref().map(live_activity_summary_json),
        "risk": monitor.risk.as_ref().map(LiveRiskMonitor::to_json),
        "degradation": monitor.degradation.active.as_ref().map(live_degradation_json),
    })
    .to_string()
}

/// Build one terminal breach without coupling it to account risk arithmetic.
fn live_terminal_breach(
    event_type: &'static str,
    reason: &str,
    details: serde_json::Value,
) -> LiveRiskBreach {
    LiveRiskBreach {
        event_type,
        reason: reason.to_string(),
        details,
    }
}

/// Build compact degradation JSON for live session details.
fn live_degradation_json(degradation: &LiveDegradation) -> serde_json::Value {
    json!({
        "kind": degradation.kind,
        "started_at_ms": degradation.started_at_ms,
        "latest_detail": degradation.latest_detail,
    })
}

/// Build a compact preflight summary for live session details.
fn live_preflight_summary_json(preflight: &LivePreflightResponse) -> serde_json::Value {
    json!({
        "ok": preflight.ok,
        "mode": preflight.mode,
        "geoblock_status": live_check_status_label(preflight.geoblock_status),
        "auth_status": live_check_status_label(preflight.auth_status),
        "clock_status": live_check_status_label(preflight.clock_status),
        "allowance_status": live_check_status_label(preflight.allowance_status),
        "user_stream_status": live_check_status_label(preflight.user_stream_status),
        "available_cash_usd": preflight.available_cash_usd,
        "legal_order_min_usd": preflight.legal_order_min_usd,
        "errors": preflight.errors,
    })
}

/// Build a compact account summary for live session details.
fn live_account_summary_json(account: &LiveAccountState) -> serde_json::Value {
    json!({
        "timestamp_ms": account.timestamp_ms,
        "cash_available": account.cash_available,
        "cash_reserved_for_orders": account.cash_reserved_for_orders,
        "inventory_mark_value": account.inventory_mark_value,
        "redeemable_value": account.redeemable_value,
        "pending_redeem_value": account.pending_redeem_value,
        "total_equity": account.total_equity,
        "allowance_available": account.allowance_available,
    })
}

/// Build a compact activity summary for live session details.
fn live_activity_summary_json(activity: &LiveActivityResponse) -> serde_json::Value {
    json!({
        "timestamp_ms": activity.timestamp_ms,
        "user_stream_status": live_check_status_label(activity.user_stream_status),
        "user_stream_event_count": activity.user_stream_events.len(),
        "clob_trade_count": activity.clob_trades.len(),
    })
}

/// Return the serialized label for a live check status.
fn live_check_status_label(status: LiveCheckStatus) -> &'static str {
    match status {
        LiveCheckStatus::Ok => "ok",
        LiveCheckStatus::Failed => "failed",
    }
}

/// Return the pre-submit status and rejection reason for one live order.
fn live_order_pre_submit_rejection(
    order: &QueuedOrderIntent,
    notional: f64,
    config: &Config,
) -> (&'static str, Option<&'static str>) {
    if order.requested_price <= 0.0 || order.requested_size <= 0.0 || order.limit_price <= 0.0 {
        return ("rejected_before_submit", Some("invalid_price_or_size"));
    }
    if notional + CASH_CHANGE_EPSILON_USD < config.min_bet_usd {
        return ("rejected_before_submit", Some("below_min_bet"));
    }
    if notional > config.live_max_single_order_usd + CASH_CHANGE_EPSILON_USD {
        return (
            "rejected_before_submit",
            Some("above_live_max_single_order"),
        );
    }
    ("submitted", None)
}

/// Return whether one order response blocks all future live submissions.
fn live_order_response_is_blocking(response: &LiveOrderIntentResponse) -> bool {
    matches!(
        response.status.as_str(),
        "unknown_submission" | "venue_restart" | "timeout" | "pending_unknown"
    )
}

/// Return the best typed fill price available from one sidecar response.
fn live_response_fill_price(order: &QueuedOrderIntent, response: &LiveOrderIntentResponse) -> f64 {
    response
        .details_json
        .as_deref()
        .and_then(|details| serde_json::from_str::<serde_json::Value>(details).ok())
        .and_then(|details| {
            let raw = details.get("raw_response")?;
            let taking = raw_amount(raw.get("takingAmount")?)?;
            let making = raw_amount(raw.get("makingAmount")?)?;
            (taking > 0.0 && making > 0.0).then_some(making / taking)
        })
        .filter(|price| price.is_finite() && *price > 0.0)
        .unwrap_or(order.requested_price)
}

/// Parse one numeric CLOB amount from a raw order response fragment.
fn raw_amount(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

/// Activate a market window: set it as current, resubscribe the `CLOB` feed to
/// the window's tokens, reset the book state, and capture the open price.
fn activate_window(
    state: &mut LiveState,
    window: &MarketWindow,
    clob_handle: &crate::feeds::clob_feed::ClobFeedHandle,
) {
    info!(
        market_id = %window.market_id,
        "window activated (now trading)"
    );

    let up_token = window.up_token_id.clone();
    let down_token = window.down_token_id.clone();
    let handle = clob_handle.clone();
    tokio::spawn(async move {
        if let Err(e) = handle.resubscribe(up_token, down_token).await {
            tracing::error!("failed to resubscribe CLOB feed: {e}");
        }
    });

    state.signal_state.book_state = crate::types::BookState::default();

    if let Some(open_price) = recover_window_open_price(state, window) {
        state
            .window_open_prices
            .entry(window.market_id.clone())
            .or_insert(open_price);
    }

    state.current_window = Some(window.clone());
}

/// Returns the best available market-open price for one live window.
fn recover_window_open_price(state: &LiveState, window: &MarketWindow) -> Option<f64> {
    let price = state.signal_state.binance_price;
    if price.is_none() {
        warn!(market_id = %window.market_id, "no in-memory Binance price available for window open");
    }
    price
}

/// Track one weighted mean across already-aggregated rejection summaries.
#[derive(Default)]
struct WeightedMetric {
    sum: f64,
    weight: u64,
}

impl WeightedMetric {
    /// Record one optional value together with the number of evaluations it represents.
    fn record(&mut self, value: Option<f64>, weight: u64) {
        if let Some(value) = value {
            self.sum += value * weight as f64;
            self.weight += weight;
        }
    }

    /// Return the weighted arithmetic mean when at least one sample was recorded.
    fn mean(&self) -> Option<f64> {
        if self.weight == 0 {
            return None;
        }
        Some(self.sum / self.weight as f64)
    }
}

/// Aggregate all rejection reasons and numeric means for one market/strategy pair.
#[derive(Default)]
struct RejectionRollup {
    market_id: String,
    strategy: String,
    total_count: u64,
    reasons: std::collections::HashMap<String, u64>,
    quote_age_ms: WeightedMetric,
    book_staleness_ms: WeightedMetric,
    up_ask: WeightedMetric,
    down_ask: WeightedMetric,
    total_ask: WeightedMetric,
    distance_from_open_bps: WeightedMetric,
    realized_vol_15s_bps: WeightedMetric,
    distance_vol_ratio: WeightedMetric,
    open_crosses_30s: WeightedMetric,
    alignment_fraction: WeightedMetric,
    quote_churn_per_s: WeightedMetric,
    move_velocity: WeightedMetric,
}

/// Human-readable rejection rollup emitted into the operator log.
#[derive(Debug, PartialEq, Eq)]
struct FormattedRejectionRollup {
    market_id: String,
    strategy: String,
    total_count: u64,
    reason_summary: String,
    metrics_summary: String,
}

/// Build concise operator-facing rejection rollups from persisted summaries.
#[allow(clippy::too_many_lines)]
fn build_rejection_rollups(
    rows: &[crate::types::StrategyRejectionSummaryRecord],
) -> Vec<FormattedRejectionRollup> {
    if rows.is_empty() {
        return Vec::new();
    }

    let mut rollups: std::collections::HashMap<(String, String), RejectionRollup> =
        std::collections::HashMap::new();
    for row in rows {
        let details = match serde_json::from_str::<serde_json::Value>(&row.details_json) {
            Ok(details) => details,
            Err(error) => {
                warn!(
                    market_id = %row.market_id,
                    strategy = %row.strategy,
                    reason = %row.reason,
                    "failed to parse rejection summary details for rollup: {error}"
                );
                continue;
            }
        };
        let mean = details.get("mean").cloned().unwrap_or_default();
        let entry = rollups
            .entry((row.market_id.clone(), row.strategy.clone()))
            .or_insert_with(|| RejectionRollup {
                market_id: row.market_id.clone(),
                strategy: row.strategy.clone(),
                ..RejectionRollup::default()
            });
        entry.total_count += row.count;
        *entry.reasons.entry(row.reason.clone()).or_insert(0) += row.count;
        entry
            .quote_age_ms
            .record(json_u64_as_f64(mean.get("quoteAgeMs")), row.count);
        entry
            .book_staleness_ms
            .record(json_u64_as_f64(mean.get("bookStalenessMs")), row.count);
        entry.up_ask.record(json_f64(mean.get("upAsk")), row.count);
        entry
            .down_ask
            .record(json_f64(mean.get("downAsk")), row.count);
        entry
            .total_ask
            .record(json_f64(mean.get("totalAsk")), row.count);
        entry
            .distance_from_open_bps
            .record(json_f64(mean.get("distanceFromOpenBps")), row.count);
        entry
            .realized_vol_15s_bps
            .record(json_f64(mean.get("realizedVol15sBps")), row.count);
        entry
            .distance_vol_ratio
            .record(json_f64(mean.get("distanceVolRatio")), row.count);
        entry
            .open_crosses_30s
            .record(json_u64_as_f64(mean.get("openCrosses30s")), row.count);
        entry
            .alignment_fraction
            .record(json_f64(mean.get("alignmentFraction")), row.count);
        entry
            .quote_churn_per_s
            .record(json_f64(mean.get("quoteChurnPerS")), row.count);
        entry
            .move_velocity
            .record(json_f64(mean.get("moveVelocity")), row.count);
    }

    let mut rollups = rollups.into_values().collect::<Vec<_>>();
    rollups.sort_by(|left, right| {
        left.market_id
            .cmp(&right.market_id)
            .then_with(|| left.strategy.cmp(&right.strategy))
    });

    rollups
        .into_iter()
        .map(|rollup| {
            let mut reasons = rollup.reasons.into_iter().collect::<Vec<_>>();
            reasons.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            let reason_summary = reasons
                .into_iter()
                .take(3)
                .map(|(reason, count)| {
                    format!(
                        "{reason}={:.1}%",
                        (count as f64 / rollup.total_count.max(1) as f64) * 100.0
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut metrics = vec![
                format!(
                    "quoteAgeMs={}",
                    format_optional_u64(rollup.quote_age_ms.mean())
                ),
                format!(
                    "bookStalenessMs={}",
                    format_optional_u64(rollup.book_staleness_ms.mean())
                ),
                format!("upAsk={}", format_optional_f64(rollup.up_ask.mean(), 3)),
                format!("downAsk={}", format_optional_f64(rollup.down_ask.mean(), 3)),
                format!(
                    "totalAsk={}",
                    format_optional_f64(rollup.total_ask.mean(), 3)
                ),
            ];
            append_metric(
                &mut metrics,
                "distanceFromOpenBps",
                rollup.distance_from_open_bps.mean(),
                2,
            );
            append_metric(
                &mut metrics,
                "realizedVol15sBps",
                rollup.realized_vol_15s_bps.mean(),
                2,
            );
            append_metric(
                &mut metrics,
                "distanceVolRatio",
                rollup.distance_vol_ratio.mean(),
                2,
            );
            append_integer_metric(
                &mut metrics,
                "openCrosses30s",
                rollup.open_crosses_30s.mean(),
            );
            append_metric(
                &mut metrics,
                "alignmentFraction",
                rollup.alignment_fraction.mean(),
                2,
            );
            metrics.push(format!(
                "quoteChurnPerS={}",
                format_optional_f64(rollup.quote_churn_per_s.mean(), 1)
            ));
            metrics.push(format!(
                "moveVelocity={}",
                format_optional_f64(rollup.move_velocity.mean(), 6)
            ));
            let metrics_summary = metrics.join(" ");
            FormattedRejectionRollup {
                market_id: rollup.market_id,
                strategy: rollup.strategy,
                total_count: rollup.total_count,
                reason_summary,
                metrics_summary,
            }
        })
        .collect()
}

/// Emit one concise operator-facing log line for each processed paper order.
fn log_processed_order_outcomes(outcomes: Vec<ProcessedOrderOutcome>) {
    for outcome in outcomes {
        match outcome.disposition {
            OrderOutcomeDisposition::Filled => {
                info!(
                    signal_id = outcome.signal_id,
                    market_id = %outcome.market_id,
                    strategy = %outcome.strategy,
                    side = %outcome.side,
                    best_ask = outcome.best_ask.unwrap_or_default(),
                    ask_size = outcome.ask_size.unwrap_or_default(),
                    freshness_ms = outcome.freshness_ms.unwrap_or_default(),
                    requested_size = outcome.requested_size,
                    filled_size = outcome.filled_size,
                    effective_arrival_delay_ms = outcome.effective_arrival_delay_ms,
                    partial_fill = outcome.partial_fill,
                    "paper order filled"
                );
            }
            OrderOutcomeDisposition::Missed => {
                info!(
                    signal_id = outcome.signal_id,
                    market_id = %outcome.market_id,
                    strategy = %outcome.strategy,
                    side = %outcome.side,
                    reason = %outcome.reason.as_deref().unwrap_or("unknown"),
                    best_ask = outcome.best_ask.unwrap_or_default(),
                    ask_size = outcome.ask_size.unwrap_or_default(),
                    freshness_ms = outcome.freshness_ms.unwrap_or_default(),
                    requested_size = outcome.requested_size,
                    effective_arrival_delay_ms = outcome.effective_arrival_delay_ms,
                    "paper order missed"
                );
            }
        }
    }
}

/// Return one optional numeric value from a JSON summary node.
fn json_f64(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(serde_json::Value::as_f64)
}

/// Return one optional integer-like JSON value as a floating-point sample.
fn json_u64_as_f64(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(|value| value.as_u64().map(|value| value as f64))
}

/// Format one optional floating-point metric for concise operator logs.
fn format_optional_f64(value: Option<f64>, precision: usize) -> String {
    value.map_or_else(|| "na".to_string(), |value| format!("{value:.precision$}"))
}

/// Format one optional millisecond metric for concise operator logs.
fn format_optional_u64(value: Option<f64>) -> String {
    value.map_or_else(
        || "na".to_string(),
        |value| format!("{}", value.round() as u64),
    )
}

/// Append one optional floating-point metric to a concise rollup string.
fn append_metric(metrics: &mut Vec<String>, label: &str, value: Option<f64>, precision: usize) {
    if let Some(value) = value {
        metrics.push(format!("{label}={value:.precision$}"));
    }
}

/// Append one optional integer-like metric to a concise rollup string.
fn append_integer_metric(metrics: &mut Vec<String>, label: &str, value: Option<f64>) {
    if let Some(value) = value {
        metrics.push(format!("{label}={}", value.round() as u64));
    }
}

/// Queue one feed row and count the accepted row by feed class.
fn enqueue_counted_feed_event(
    writer: &FeedEventWriter,
    storage_state: &mut FeedEventStorageState,
    event: FeedEvent,
) -> bool {
    let source = event.source.clone();
    let event_type = event.event_type.clone();
    if writer.try_enqueue(event) {
        storage_state.record_enqueued_key(&source, &event_type);
        true
    } else {
        warn!(%source, %event_type, "feed writer queue rejected event");
        false
    }
}

/// Carry the fields that vary between materialized per-side `CLOB` rows.
struct LiveClobRowSpec {
    source: &'static str,
    asset_id: Option<String>,
    market_id: Option<String>,
    payload_json: Option<String>,
    details_json: Option<String>,
}

/// Build the relevant per-side `CLOB` rows for one live top-of-book event.
fn build_live_clob_events(event: &LiveClobLogEvent<'_>) -> Vec<FeedEvent> {
    let mut events = Vec::new();
    let current_market_id = event.current_window.map(|window| window.market_id.clone());
    let payload_json = event.payload_json.map(ToString::to_string);
    let details_json = event.details_json.map(ToString::to_string);

    if let Some(window) = event.current_window {
        if let Some(asset_id) = event.asset_id {
            if asset_id == window.up_token_id {
                push_live_clob_event(
                    &mut events,
                    event,
                    event.book_state.up.as_ref(),
                    LiveClobRowSpec {
                        source: "clob_up",
                        asset_id: Some(window.up_token_id.clone()),
                        market_id: current_market_id,
                        payload_json,
                        details_json,
                    },
                );
                return events;
            }
            if asset_id == window.down_token_id {
                push_live_clob_event(
                    &mut events,
                    event,
                    event.book_state.down.as_ref(),
                    LiveClobRowSpec {
                        source: "clob_down",
                        asset_id: Some(window.down_token_id.clone()),
                        market_id: current_market_id,
                        payload_json,
                        details_json,
                    },
                );
                return events;
            }
        }
        push_live_clob_event(
            &mut events,
            event,
            event.book_state.up.as_ref(),
            LiveClobRowSpec {
                source: "clob_up",
                asset_id: Some(window.up_token_id.clone()),
                market_id: current_market_id.clone(),
                payload_json: payload_json.clone(),
                details_json: details_json.clone(),
            },
        );
        push_live_clob_event(
            &mut events,
            event,
            event.book_state.down.as_ref(),
            LiveClobRowSpec {
                source: "clob_down",
                asset_id: Some(window.down_token_id.clone()),
                market_id: current_market_id,
                payload_json,
                details_json,
            },
        );
        return events;
    }

    push_live_clob_event(
        &mut events,
        event,
        event.book_state.up.as_ref(),
        LiveClobRowSpec {
            source: "clob_up",
            asset_id: event.asset_id.map(str::to_string),
            market_id: None,
            payload_json: payload_json.clone(),
            details_json: details_json.clone(),
        },
    );
    push_live_clob_event(
        &mut events,
        event,
        event.book_state.down.as_ref(),
        LiveClobRowSpec {
            source: "clob_down",
            asset_id: event.asset_id.map(str::to_string),
            market_id: None,
            payload_json,
            details_json,
        },
    );
    events
}

/// Append one materialized `CLOB` row when the corresponding book side exists.
fn push_live_clob_event(
    events: &mut Vec<FeedEvent>,
    context: &LiveClobLogEvent<'_>,
    book: Option<&crate::types::TopOfBook>,
    spec: LiveClobRowSpec,
) {
    let Some(book) = book else {
        return;
    };
    events.push(FeedEvent {
        id: None,
        received_at_ms: context.receive_ms,
        event_at_ms: context.event_ms,
        received_at_us: context.receive_micros,
        event_at_us: context.event_micros,
        source: spec.source.to_string(),
        event_type: context.event_type.to_string(),
        source_topic: context.source_topic.map(str::to_string),
        source_symbol: None,
        connection_id: Some(context.connection_id.to_string()),
        sequence_key: None,
        market_id: spec.market_id,
        asset_id: spec.asset_id,
        price: None,
        trade_size: None,
        signed_quantity: None,
        best_bid: Some(book.best_bid),
        best_ask: Some(book.best_ask),
        bid_size: Some(book.bid_size),
        ask_size: Some(book.ask_size),
        depth_bid_notional: None,
        depth_ask_notional: None,
        depth_imbalance: None,
        microprice: None,
        payload_json: spec.payload_json,
        details_json: spec.details_json,
        fidelity: ReplayFidelity::RawEvent,
    });
}

/// Logs live clob event.
fn log_live_clob_event(
    writer: &FeedEventWriter,
    storage_state: &mut FeedEventStorageState,
    event: &LiveClobLogEvent<'_>,
) -> bool {
    let mut accepted = true;
    for feed_event in build_live_clob_events(event) {
        let prepared = match event.event_type {
            "book" => storage_state.prepare_clob_book_snapshot(feed_event),
            "price_change" | "best_bid_ask" => storage_state.prepare_clob_top_of_book(feed_event),
            _ => Some(feed_event),
        };
        if let Some(feed_event) = prepared {
            accepted &= enqueue_counted_feed_event(writer, storage_state, feed_event);
        }
    }
    accepted
}

/// Persist one feed lifecycle event outside the feed loop.
fn enqueue_feed_health_event(
    persistence_writer: &LivePersistenceWriter,
    event: OwnedFeedHealthLogEvent,
    monitor: Option<&mut LiveTradingMonitor>,
) {
    if !persistence_writer.try_enqueue(LivePersistenceEvent::FeedHealth(Box::new(
        FeedHealthEvent {
            id: None,
            timestamp_ms: event.timestamp_ms,
            timestamp_us: event.timestamp_micros,
            source: event.source,
            event_type: event.event_type,
            connection_id: event.connection_id,
            market_id: event.market_id,
            details_json: event.details_json,
        },
    ))) {
        mark_live_submission_blocked(
            monitor,
            "runtime persistence queue rejected feed-health event",
        );
    }
}

/// Emit concise operator-facing feed-health rollups for recent disconnect activity.
fn log_feed_health_rollups(rows: &[FeedHealthRollupRow]) {
    for row in rows {
        let cause_summary = if row.cause_counts.is_empty() {
            "none".to_string()
        } else {
            row.cause_counts
                .iter()
                .map(|(cause, count)| format!("{cause}={count}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let active_outage_s = row.active_outage_ms.map_or_else(
            || "none".to_string(),
            |ms| format!("{:.1}", ms as f64 / 1000.0),
        );
        let active_cause = row.active_cause_class.as_deref().unwrap_or("none");

        info!(
            feed = %row.source,
            disconnects = row.disconnect_count,
            cumulative_downtime_s = format!("{:.1}", row.cumulative_downtime_ms as f64 / 1000.0),
            max_downtime_s = format!("{:.1}", row.max_downtime_ms as f64 / 1000.0),
            active_outage_s,
            active_cause,
            causes = %cause_summary,
            "feed health rollup"
        );
    }
}

/// Return the configured replay-quality capability for the storage profile.
fn configured_replay_quality_class(profile: FeedEventStorageProfile) -> &'static str {
    match profile {
        FeedEventStorageProfile::Compact => "descriptive_only",
        FeedEventStorageProfile::ReplayGrade | FeedEventStorageProfile::FullDebug => {
            "sweep_capable"
        }
    }
}

/// Persist cheap runtime capture metadata without scanning historical feed rows.
fn persist_runtime_capture_metadata(
    db: &Database,
    profile: FeedEventStorageProfile,
    recorded_at_ms: u64,
    capture_health: &str,
    row_summary: &str,
) -> anyhow::Result<()> {
    for (key, value, recorded_at_ms) in runtime_capture_metadata_rows(
        profile,
        recorded_at_ms,
        capture_health.to_string(),
        row_summary.to_string(),
    ) {
        db.set_run_metadata(&key, &value, recorded_at_ms)?;
    }
    Ok(())
}

/// Build cheap runtime capture metadata rows without scanning historical feed rows.
fn runtime_capture_metadata_rows(
    profile: FeedEventStorageProfile,
    recorded_at_ms: u64,
    capture_health: String,
    row_summary: String,
) -> Vec<(String, String, u64)> {
    let missing = missing_required_feed_classes(&row_summary).join(", ");
    vec![
        (
            "feed_event_storage_profile".to_string(),
            profile.as_str().to_string(),
            recorded_at_ms,
        ),
        (
            "configured_replay_quality_class".to_string(),
            configured_replay_quality_class(profile).to_string(),
            recorded_at_ms,
        ),
        (
            "runtime_capture_health".to_string(),
            capture_health.clone(),
            recorded_at_ms,
        ),
        (
            "runtime_observed_replay_quality_class".to_string(),
            capture_health,
            recorded_at_ms,
        ),
        (
            "runtime_capture_recorded_at_ms".to_string(),
            recorded_at_ms.to_string(),
            recorded_at_ms,
        ),
        (
            "runtime_capture_rows_window".to_string(),
            row_summary.clone(),
            recorded_at_ms,
        ),
        (
            "feed_event_classes".to_string(),
            row_summary.clone(),
            recorded_at_ms,
        ),
        (
            "replay_quality_validated_at_ms".to_string(),
            "not_validated_in_runtime".to_string(),
            recorded_at_ms,
        ),
        (
            "replay_quality_validation_interval".to_string(),
            "not_validated_in_runtime".to_string(),
            recorded_at_ms,
        ),
        (
            "replay_quality_missing_required_classes".to_string(),
            missing,
            recorded_at_ms,
        ),
        (
            "replay_quality_observed_feed_classes".to_string(),
            row_summary,
            recorded_at_ms,
        ),
        (
            "required_feed_event_classes".to_string(),
            required_feed_event_classes().to_string(),
            recorded_at_ms,
        ),
    ]
}

/// Classify current writer health from incremental counters.
fn runtime_capture_health(
    snapshot: &FeedEventWriterSnapshot,
    row_summary: &str,
    now_ms: u64,
    max_lag_ms: u64,
) -> &'static str {
    let writer_degraded = snapshot.write_errors > 0
        || snapshot.terminal_write_errors > 0
        || snapshot.queue_full > 0
        || snapshot.dropped > 0;
    let writer_stale = snapshot.last_persisted_at_ms > 0
        && snapshot
            .last_persisted_at_ms
            .saturating_add(max_lag_ms.max(1))
            < now_ms;
    if writer_degraded || writer_stale {
        "degraded"
    } else if missing_required_feed_classes(row_summary).is_empty() {
        "sweep_grade"
    } else if snapshot.persisted > 0 {
        "observing"
    } else {
        "empty"
    }
}

/// Return one file size or zero when the file does not exist.
fn file_size_bytes(path: &str) -> u64 {
    std::fs::metadata(path).map_or(0, |metadata| metadata.len())
}

/// Return live arming issues from cheap runtime capture metadata only.
fn runtime_capture_issues_for_live(
    db: &Database,
    config: &Config,
    now_ms: u64,
) -> anyhow::Result<Vec<String>> {
    if config.feed_event_storage_profile == FeedEventStorageProfile::Compact {
        return Ok(vec![
            "live trading requires replay-grade feed capture; compact capture is descriptive-only"
                .to_string(),
        ]);
    }
    let Some(health) = db.get_run_metadata("runtime_capture_health")? else {
        return Ok(vec![
            "live trading requires observed replay-grade capture health before arming".to_string(),
        ]);
    };
    let mut issues = Vec::new();
    match health.as_str() {
        "sweep_grade" => {}
        "observing" => issues.push(
            "live trading requires every replay-grade feed class in recent capture metadata"
                .to_string(),
        ),
        "empty" => issues.push(
            "live trading requires persisted replay-grade feed rows before arming".to_string(),
        ),
        "degraded" => issues.push(
            "live trading capture writer is degraded; reconcile persistence before arming"
                .to_string(),
        ),
        other => issues.push(format!("live trading capture health is not ready: {other}")),
    }
    if let Some(missing) = db.get_run_metadata("replay_quality_missing_required_classes")?
        && !missing.trim().is_empty()
        && health == "observing"
    {
        issues.push(format!("missing replay-grade feed classes: {missing}"));
    }
    if let Some(recorded) = db.get_run_metadata("runtime_capture_recorded_at_ms")?
        && let Ok(recorded_at_ms) = recorded.parse::<u64>()
    {
        let max_age_ms = config.feed_event_writer_max_lag_ms.max(60_000);
        if recorded_at_ms.saturating_add(max_age_ms) < now_ms {
            issues.push("live trading capture health metadata is stale".to_string());
        }
    }
    Ok(issues)
}

/// Return the required feed classes for sweep-grade replay.
fn required_feed_event_classes() -> &'static str {
    "binance:aggTrade, binance:bookTicker, binance:depth, chainlink:chainlink_price, clob_up:best_bid_ask, clob_down:best_bid_ask"
}

/// Return required feed classes absent from one row-count summary.
fn missing_required_feed_classes(row_summary: &str) -> Vec<&'static str> {
    required_feed_event_classes()
        .split(", ")
        .filter(|required| !row_summary.contains(required))
        .collect()
}

/// Register or update one market that still needs authoritative settlement.
fn schedule_pending_resolution(
    pending_resolutions: &mut std::collections::HashMap<String, PendingResolution>,
    window: &MarketWindow,
    next_attempt_at_ms: u64,
    seeded_from_startup: bool,
) {
    use std::collections::hash_map::Entry;

    match pending_resolutions.entry(window.market_id.clone()) {
        Entry::Occupied(mut entry) => {
            let pending = entry.get_mut();
            pending.next_attempt_at_ms = pending.next_attempt_at_ms.min(next_attempt_at_ms);
            pending.seeded_from_startup &= seeded_from_startup;
        }
        Entry::Vacant(entry) => {
            entry.insert(PendingResolution {
                market_id: window.market_id.clone(),
                window: window.clone(),
                next_attempt_at_ms,
                seeded_from_startup,
            });
        }
    }
}

/// Seed the durable authoritative-settlement registry from unresolved open trades.
fn seed_pending_resolutions(
    db: &Database,
    config: &Config,
    clock: &dyn Clock,
) -> std::collections::HashMap<String, PendingResolution> {
    let now = clock.now();
    let unresolved = match db.unresolved_open_trade_markets(now) {
        Ok(markets) => markets,
        Err(e) => {
            warn!("failed to load unresolved open-trade markets for reconciliation: {e}");
            return std::collections::HashMap::new();
        }
    };

    let mut pending_resolutions = std::collections::HashMap::new();
    for market in unresolved {
        if let Err(e) = db.resolve_market(&market.window.market_id, "closed") {
            warn!(
                market_id = %market.window.market_id,
                "failed to normalize unresolved market to closed on startup: {e}"
            );
        }
        let next_attempt_at_ms = market
            .window
            .end_time
            .saturating_add(config.resolution_initial_delay_ms)
            .max(now);
        schedule_pending_resolution(
            &mut pending_resolutions,
            &market.window,
            next_attempt_at_ms,
            true,
        );
    }

    if !pending_resolutions.is_empty() {
        info!(
            pending_resolution_markets = pending_resolutions.len(),
            "seeded authoritative settlement reconciliation from unresolved open trades"
        );
    }

    pending_resolutions
}

/// Remove and return pending authoritative settlements whose next attempt is due.
fn take_ready_pending_resolutions(
    pending_resolutions: &mut std::collections::HashMap<String, PendingResolution>,
    now: u64,
) -> Vec<PendingResolution> {
    let ready_market_ids = pending_resolutions
        .iter()
        .filter(|(_, pending)| pending.next_attempt_at_ms <= now)
        .map(|(market_id, _)| market_id.clone())
        .collect::<Vec<_>>();

    let mut ready = Vec::with_capacity(ready_market_ids.len());
    for market_id in ready_market_ids {
        if let Some(pending) = pending_resolutions.remove(&market_id) {
            ready.push(pending);
        }
    }
    ready
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::BacktestClock;
    use crate::config::ExecutionMode;
    use tempfile::NamedTempFile;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Verifies that live window-open recovery uses only in-memory feed state.
    #[test]
    fn recover_window_open_price_uses_live_price_without_db() {
        let mut state = LiveState::new();
        state.signal_state.binance_price = Some(43_000.0);
        let window = MarketWindow {
            market_id: "mkt-1".to_string(),
            question: "Will BTC go up?".to_string(),
            up_token_id: "up".to_string(),
            down_token_id: "down".to_string(),
            condition_id: "cond-1".to_string(),
            start_time: 1_000,
            end_time: 2_000,
            slug: "btc-updown-5m-1".to_string(),
            outcome: None,
            resolution_source: None,
            fee_profile: None,
            order_min_size: None,
            order_price_min_tick_size: None,
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
        };

        let open_price = recover_window_open_price(&state, &window);
        assert_eq!(open_price, Some(43_000.0));
    }

    /// Verifies that missing live price returns no synthetic open value.
    #[test]
    fn recover_window_open_price_returns_none_without_live_price() {
        let state = LiveState::new();
        let window = MarketWindow {
            market_id: "mkt-1".to_string(),
            question: "Will BTC go up?".to_string(),
            up_token_id: "up".to_string(),
            down_token_id: "down".to_string(),
            condition_id: "cond-1".to_string(),
            start_time: 1_000,
            end_time: 2_000,
            slug: "btc-updown-5m-1".to_string(),
            outcome: None,
            resolution_source: None,
            fee_profile: None,
            order_min_size: None,
            order_price_min_tick_size: None,
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
        };

        let open_price = recover_window_open_price(&state, &window);
        assert_eq!(open_price, None);
    }

    /// Verifies that operator-facing rejection rollups stay concise while
    /// preserving reason percentages and mean quote context.
    #[test]
    fn build_rejection_rollups_formats_concise_reason_summary() {
        let rows = vec![
            crate::types::StrategyRejectionSummaryRecord {
                timestamp_ms: 1_000,
                market_id: "mkt-1".to_string(),
                strategy: "latency-arb".to_string(),
                reason: "features_stale".to_string(),
                count: 75,
                details_json: serde_json::json!({
                    "last": {"upAsk": 0.50},
                    "mean": {
                        "quoteAgeMs": 120,
                        "bookStalenessMs": 140,
                        "upAsk": 0.51,
                        "downAsk": 0.49,
                        "totalAsk": 1.00,
                        "quoteChurnPerS": 8.0,
                        "moveVelocity": 0.0002
                    }
                })
                .to_string(),
            },
            crate::types::StrategyRejectionSummaryRecord {
                timestamp_ms: 1_000,
                market_id: "mkt-1".to_string(),
                strategy: "latency-arb".to_string(),
                reason: "window_too_late".to_string(),
                count: 25,
                details_json: serde_json::json!({
                    "last": {"upAsk": 0.55},
                    "mean": {
                        "quoteAgeMs": 200,
                        "bookStalenessMs": 240,
                        "upAsk": 0.55,
                        "downAsk": 0.45,
                        "totalAsk": 1.00,
                        "quoteChurnPerS": 4.0,
                        "moveVelocity": 0.0001
                    }
                })
                .to_string(),
            },
        ];

        let rollups = build_rejection_rollups(&rows);
        assert_eq!(rollups.len(), 1);
        assert_eq!(rollups[0].market_id, "mkt-1");
        assert_eq!(rollups[0].strategy, "latency-arb");
        assert_eq!(rollups[0].total_count, 100);
        assert_eq!(
            rollups[0].reason_summary,
            "features_stale=75.0%, window_too_late=25.0%"
        );
        assert!(rollups[0].metrics_summary.contains("quoteAgeMs=140"));
        assert!(rollups[0].metrics_summary.contains("bookStalenessMs=165"));
        assert!(!rollups[0].metrics_summary.contains('{'));
    }

    /// Verify that calm-specific rejection metrics are included only when present.
    #[test]
    fn build_rejection_rollups_includes_calm_specific_metrics() {
        let rows = vec![crate::types::StrategyRejectionSummaryRecord {
            timestamp_ms: 1_000,
            market_id: "mkt-2".to_string(),
            strategy: "calm-persistence".to_string(),
            reason: "distance_below_threshold".to_string(),
            count: 10,
            details_json: serde_json::json!({
                "last": {"distanceFromOpenBps": 4.5},
                "mean": {
                    "quoteAgeMs": 5,
                    "bookStalenessMs": 7,
                    "upAsk": 0.61,
                    "downAsk": 0.71,
                    "totalAsk": 1.32,
                    "distanceFromOpenBps": 4.75,
                    "realizedVol15sBps": 6.5,
                    "distanceVolRatio": 0.73,
                    "openCrosses30s": 1,
                    "alignmentFraction": 0.50,
                    "quoteChurnPerS": 12.0,
                    "moveVelocity": 0.00003
                }
            })
            .to_string(),
        }];

        let rollups = build_rejection_rollups(&rows);
        assert_eq!(rollups.len(), 1);
        assert!(
            rollups[0]
                .metrics_summary
                .contains("distanceFromOpenBps=4.75")
        );
        assert!(
            rollups[0]
                .metrics_summary
                .contains("realizedVol15sBps=6.50")
        );
        assert!(rollups[0].metrics_summary.contains("distanceVolRatio=0.73"));
        assert!(rollups[0].metrics_summary.contains("openCrosses30s=1"));
        assert!(
            rollups[0]
                .metrics_summary
                .contains("alignmentFraction=0.50")
        );
    }

    /// Verifies that completed feed outages contribute bounded downtime rollups.
    #[test]
    fn feed_health_tracker_rolls_up_completed_outage() {
        let mut tracker = FeedHealthTracker::default();
        tracker.note_disconnected("clob", "websocket_error", 1_000);
        tracker.note_connected("clob", 1_450);

        let rows = tracker.take_rollups(2_000);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "clob");
        assert_eq!(rows[0].disconnect_count, 1);
        assert_eq!(rows[0].cumulative_downtime_ms, 450);
        assert_eq!(rows[0].max_downtime_ms, 450);
        assert_eq!(rows[0].active_outage_ms, None);
        assert_eq!(rows[0].active_cause_class, None);
        assert_eq!(
            rows[0].cause_counts,
            vec![("websocket_error".to_string(), 1)]
        );
    }

    /// Verifies that ongoing outages stay visible in periodic feed-health rollups.
    #[test]
    fn feed_health_tracker_reports_active_outage() {
        let mut tracker = FeedHealthTracker::default();
        tracker.note_disconnected("binance", "idle_timeout", 2_000);

        let rows = tracker.take_rollups(2_750);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "binance");
        assert_eq!(rows[0].disconnect_count, 1);
        assert_eq!(rows[0].cumulative_downtime_ms, 0);
        assert_eq!(rows[0].max_downtime_ms, 0);
        assert_eq!(rows[0].active_outage_ms, Some(750));
        assert_eq!(rows[0].active_cause_class.as_deref(), Some("idle_timeout"));
        assert_eq!(rows[0].cause_counts, vec![("idle_timeout".to_string(), 1)]);
    }

    /// Verifies that live-trading runtime can start disarmed and shut down locally.
    #[tokio::test]
    async fn run_live_starts_live_trading_disarmed() {
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveTrading;
        config.live_sidecar_url = "http://127.0.0.1:9".to_string();
        let tmp_db = NamedTempFile::new().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        shutdown_tx.send(()).unwrap();

        let result = run_live(config, tmp_db.path().to_str().unwrap(), 100.0, shutdown_rx).await;

        assert!(result.is_ok());
        let conn = rusqlite::Connection::open(tmp_db.path()).unwrap();
        let runtime_quality: String = conn
            .query_row(
                "SELECT value FROM run_metadata WHERE key = 'runtime_observed_replay_quality_class'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let configured: String = conn
            .query_row(
                "SELECT value FROM run_metadata WHERE key = 'configured_replay_quality_class'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let offline_quality_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM run_metadata WHERE key = 'replay_quality_class'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(runtime_quality, "empty");
        assert_eq!(offline_quality_count, 0);
        assert_eq!(configured, "sweep_capable");
    }

    /// Verifies that live-trading refuses descriptive-only compact capture at startup.
    #[tokio::test]
    async fn run_live_trading_rejects_compact_capture() {
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveTrading;
        config.feed_event_storage_profile = FeedEventStorageProfile::Compact;
        let tmp_db = NamedTempFile::new().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        shutdown_tx.send(()).unwrap();

        let result = run_live(config, tmp_db.path().to_str().unwrap(), 100.0, shutdown_rx).await;

        let error = result.unwrap_err().to_string();
        assert!(error.contains("replay-grade"));
        assert!(error.contains("compact"));
    }

    /// Verifies that compact capture blocks live arming.
    #[test]
    fn live_gate_issues_require_replay_grade_capture() {
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveTrading;
        config.feed_event_storage_profile = FeedEventStorageProfile::Compact;
        let issues = live_gate_issues(
            &test_preflight_response(),
            &test_account_state(),
            &test_activity_response(),
            &config,
        );

        assert!(issues.iter().any(|issue| issue.contains("replay-grade")));
    }

    /// Verifies that runtime capture metadata does not scan rows or claim offline sweep grade.
    #[test]
    fn runtime_capture_metadata_uses_incremental_health() {
        let tmp_db = NamedTempFile::new().unwrap();
        let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();

        persist_runtime_capture_metadata(
            &db,
            FeedEventStorageProfile::ReplayGrade,
            2_000,
            "empty",
            "",
        )
        .unwrap();
        persist_runtime_capture_metadata(
            &db,
            FeedEventStorageProfile::ReplayGrade,
            2_500,
            "observing",
            "binance:aggTrade=10",
        )
        .unwrap();

        assert!(
            db.get_run_metadata("replay_quality_class")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            db.get_run_metadata("runtime_observed_replay_quality_class")
                .unwrap()
                .unwrap(),
            "observing"
        );
        assert_eq!(
            db.get_run_metadata("configured_replay_quality_class")
                .unwrap()
                .unwrap(),
            "sweep_capable"
        );
        assert_eq!(
            db.get_run_metadata("replay_quality_missing_required_classes")
                .unwrap()
                .unwrap(),
            "binance:bookTicker, binance:depth, chainlink:chainlink_price, clob_up:best_bid_ask, clob_down:best_bid_ask"
        );
    }

    /// Verifies that live arming uses cheap runtime capture health metadata.
    #[test]
    fn runtime_capture_issues_use_metadata_not_full_scan() {
        let tmp_db = NamedTempFile::new().unwrap();
        let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
        let config = Config::default();

        let missing = runtime_capture_issues_for_live(&db, &config, 2_000).unwrap();
        assert!(missing[0].contains("capture health"));

        persist_runtime_capture_metadata(
            &db,
            FeedEventStorageProfile::ReplayGrade,
            2_000,
            "observing",
            "binance:aggTrade=10",
        )
        .unwrap();
        let missing_classes = runtime_capture_issues_for_live(&db, &config, 3_000).unwrap();
        assert!(
            missing_classes
                .iter()
                .any(|issue| issue.contains("missing replay-grade"))
        );

        persist_runtime_capture_metadata(
            &db,
            FeedEventStorageProfile::ReplayGrade,
            3_500,
            "sweep_grade",
            required_feed_event_classes(),
        )
        .unwrap();
        let ready = runtime_capture_issues_for_live(&db, &config, 3_750).unwrap();
        assert!(ready.is_empty());

        persist_runtime_capture_metadata(
            &db,
            FeedEventStorageProfile::ReplayGrade,
            4_000,
            "degraded",
            "binance:aggTrade=10",
        )
        .unwrap();
        let degraded = runtime_capture_issues_for_live(&db, &config, 5_000).unwrap();
        assert!(degraded[0].contains("degraded"));
    }

    /// Verifies that runtime capture health reflects writer loss counters.
    #[test]
    fn runtime_capture_health_degrades_on_writer_loss() {
        let mut snapshot = FeedEventWriterSnapshot {
            enqueued: 1,
            persisted: 1,
            dropped: 0,
            queue_full: 0,
            write_errors: 0,
            terminal_write_errors: 0,
            batches: 1,
            total_write_ms: 1,
            max_write_ms: 1,
            last_persisted_at_ms: 2_000,
            shutdown_timed_out: 0,
        };
        assert_eq!(
            runtime_capture_health(&snapshot, "binance:aggTrade=1", 2_100, 5_000),
            "observing"
        );
        assert_eq!(
            runtime_capture_health(&snapshot, required_feed_event_classes(), 2_100, 5_000),
            "sweep_grade"
        );

        snapshot.queue_full = 1;
        assert_eq!(
            runtime_capture_health(&snapshot, required_feed_event_classes(), 2_100, 5_000),
            "degraded"
        );
    }

    /// Verifies that compact capture is rejected from metadata-only live gates.
    #[test]
    fn runtime_capture_issues_reject_compact_profile() {
        let tmp_db = NamedTempFile::new().unwrap();
        let db = Database::new(tmp_db.path().to_str().unwrap()).unwrap();
        let mut config = Config::default();
        config.feed_event_storage_profile = FeedEventStorageProfile::Compact;

        persist_runtime_capture_metadata(
            &db,
            FeedEventStorageProfile::Compact,
            2_000,
            "observing",
            "binance:aggTrade=10",
        )
        .unwrap();

        let issues = runtime_capture_issues_for_live(&db, &config, 3_000).unwrap();

        assert!(issues[0].contains("replay-grade"));
    }

    /// Verifies that live risk state halts on session drawdown.
    #[test]
    fn live_risk_monitor_detects_session_drawdown_halt() {
        let mut config = Config::default();
        config.live_max_session_drawdown_usd = 20.0;
        config.live_max_daily_loss_usd = 50.0;
        config.max_drawdown_pct = 1.0;
        let mut risk = LiveRiskMonitor::new(&test_account_state());
        let mut account = test_account_state();
        account.total_equity = 79.9;

        let breach = risk.update_from_account(&account, &config).unwrap();

        assert_eq!(breach.event_type, "live_session_drawdown_halt");
        assert!(breach.details["session_drawdown_usd"].as_f64().unwrap() >= 20.0);
    }

    /// Verifies that live risk state halts on UTC-day loss.
    #[test]
    fn live_risk_monitor_detects_daily_loss_halt() {
        let mut config = Config::default();
        config.live_max_session_drawdown_usd = 50.0;
        config.live_max_daily_loss_usd = 15.0;
        config.max_drawdown_pct = 1.0;
        let mut risk = LiveRiskMonitor::new(&test_account_state());
        let mut account = test_account_state();
        account.total_equity = 84.9;

        let breach = risk.update_from_account(&account, &config).unwrap();

        assert_eq!(breach.event_type, "live_daily_loss_halt");
        assert!(breach.details["daily_loss_usd"].as_f64().unwrap() >= 15.0);
    }

    /// Verifies that remote degradation becomes terminal only after the threshold.
    #[test]
    fn live_degradation_tracker_requires_terminal_threshold() {
        let mut tracker = LiveDegradationTracker::default();

        assert!(
            tracker
                .note("user_stream", "down", 1_000, LIVE_TERMINAL_DEGRADATION_MS)
                .is_none()
        );
        let breach = tracker
            .note(
                "user_stream",
                "still down",
                1_000 + LIVE_TERMINAL_DEGRADATION_MS,
                LIVE_TERMINAL_DEGRADATION_MS,
            )
            .unwrap();

        assert_eq!(breach.kind, "user_stream");
        assert_eq!(breach.duration_ms, LIVE_TERMINAL_DEGRADATION_MS);
    }

    /// Verifies that a halted live-trading DB cannot bootstrap a new live session.
    #[tokio::test]
    async fn halted_live_trading_db_rejects_bootstrap() {
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveTrading;
        let tmp_db = NamedTempFile::new().unwrap();
        let db_path = tmp_db.path().to_str().unwrap();
        let db = Database::new(db_path).unwrap();
        db.insert_live_session(&LiveSession {
            id: None,
            started_at_ms: 1_000,
            ended_at_ms: None,
            status: "halted".to_string(),
            execution_mode: "live_trading".to_string(),
            wallet_address: None,
            proxy_wallet: None,
            enabled_strategies_json: "[]".to_string(),
            config_fingerprint: "{}".to_string(),
            cash_cap_usd: 100.0,
            details_json: Some("{}".to_string()),
        })
        .unwrap();
        let clock = BacktestClock::new();

        let result = bootstrap_live_trading_runtime(&config, db, db_path, 100.0, &clock).await;

        assert!(result.is_err());
        let error = result.err().unwrap();
        assert!(error.to_string().contains("halted"));
    }

    /// Verifies that unknown venue outcomes block future live submissions.
    #[test]
    fn live_order_response_blocking_statuses_are_terminal() {
        let response = LiveOrderIntentResponse {
            ok: false,
            venue_order_id: None,
            client_order_id: "client-1".to_string(),
            status: "unknown_submission".to_string(),
            status_reason: Some("timeout".to_string()),
            accepted_size: None,
            details_json: None,
        };

        assert!(live_order_response_is_blocking(&response));
    }

    /// Verifies fill price is derived from CLOB raw making/taking amounts when present.
    #[test]
    fn live_response_fill_price_uses_raw_response_amounts() {
        let order = test_queued_order();
        let response = LiveOrderIntentResponse {
            ok: true,
            venue_order_id: Some("venue-1".to_string()),
            client_order_id: "client-1".to_string(),
            status: "matched".to_string(),
            status_reason: None,
            accepted_size: Some(10.0),
            details_json: Some(
                json!({
                    "raw_response": {
                        "takingAmount": "10000000",
                        "makingAmount": "5300000"
                    }
                })
                .to_string(),
            ),
        };

        assert!((live_response_fill_price(&order, &response) - 0.53).abs() < 1e-12);
    }

    /// Verifies a successful venue response without accepted size becomes unknown.
    #[test]
    fn live_order_response_missing_accepted_size_blocks() {
        let tmp_db = NamedTempFile::new().unwrap();
        let db_path = tmp_db.path().to_str().unwrap();
        let db = Database::new(db_path).unwrap();
        db.log_signal_with_context_and_id(
            -1,
            &test_signal(),
            Some("mkt-test"),
            Some(ReplayFidelity::RawEvent),
            None,
            None,
        )
        .unwrap();
        let session_id = db
            .insert_live_session(&LiveSession {
                id: None,
                started_at_ms: 1_000,
                ended_at_ms: None,
                status: "armed".to_string(),
                execution_mode: "live_trading".to_string(),
                wallet_address: None,
                proxy_wallet: None,
                enabled_strategies_json: "[]".to_string(),
                config_fingerprint: "{}".to_string(),
                cash_cap_usd: 100.0,
                details_json: Some("{}".to_string()),
            })
            .unwrap();
        db.close();
        let order = test_queued_order();
        let evidence = test_critical_signal_event(order.signal_id);
        let (intent_id, reject_reason) =
            persist_live_order_intent_from_worker(&LiveIntentPersistenceInput {
                db_path,
                config: &Config::default(),
                session_id,
                window: &test_market_window(),
                order: &order,
                decision_event: Some(&evidence),
                now_ms: 2_000,
                notional: order.requested_price * order.requested_size,
            })
            .unwrap();
        assert_eq!(reject_reason, None);
        let response = LiveOrderIntentResponse {
            ok: true,
            venue_order_id: Some("venue-1".to_string()),
            client_order_id: "client-1".to_string(),
            status: "matched".to_string(),
            status_reason: None,
            accepted_size: None,
            details_json: None,
        };

        let result = handle_live_order_response_from_worker(
            db_path, session_id, &order, intent_id, 2_100, &response,
        );

        assert!(matches!(
            result,
            LiveSubmissionOrderResult::Blocked {
                state: "unknown_order",
                ..
            }
        ));
    }

    /// Verifies critical evidence and live intent are durable before sidecar submission.
    #[test]
    fn live_intent_persistence_writes_critical_decision_evidence() {
        let tmp_db = NamedTempFile::new().unwrap();
        let db_path = tmp_db.path().to_str().unwrap();
        let db = Database::new(db_path).unwrap();
        let session_id = db
            .insert_live_session(&LiveSession {
                id: None,
                started_at_ms: 1_000,
                ended_at_ms: None,
                status: "armed".to_string(),
                execution_mode: "live_trading".to_string(),
                wallet_address: None,
                proxy_wallet: None,
                enabled_strategies_json: "[]".to_string(),
                config_fingerprint: "{}".to_string(),
                cash_cap_usd: 100.0,
                details_json: Some("{}".to_string()),
            })
            .unwrap();
        db.close();
        let order = test_queued_order();
        let evidence = test_critical_signal_event(order.signal_id);
        let (intent_id, reject_reason) =
            persist_live_order_intent_from_worker(&LiveIntentPersistenceInput {
                db_path,
                config: &Config::default(),
                session_id,
                window: &test_market_window(),
                order: &order,
                decision_event: Some(&evidence),
                now_ms: 2_000,
                notional: order.requested_price * order.requested_size,
            })
            .unwrap();

        assert!(intent_id > 0);
        assert_eq!(reject_reason, None);
        let db = Database::new(db_path).unwrap();
        let signal_count: u64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM signals WHERE id = -1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let intent_count: u64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM live_order_intents WHERE signal_id = -1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        db.close();
        assert_eq!(signal_count, 1);
        assert_eq!(intent_count, 1);
    }

    /// Verifies a live order is blocked before any venue call when evidence is absent.
    #[tokio::test]
    async fn live_submission_missing_critical_evidence_blocks_before_sidecar() {
        let tmp_db = NamedTempFile::new().unwrap();
        let db_path = tmp_db.path().to_str().unwrap();
        let db = Database::new(db_path).unwrap();
        let session_id = db
            .insert_live_session(&LiveSession {
                id: None,
                started_at_ms: 1_000,
                ended_at_ms: None,
                status: "armed".to_string(),
                execution_mode: "live_trading".to_string(),
                wallet_address: None,
                proxy_wallet: None,
                enabled_strategies_json: "[]".to_string(),
                config_fingerprint: "{}".to_string(),
                cash_cap_usd: 100.0,
                details_json: Some("{}".to_string()),
            })
            .unwrap();
        db.close();
        let sidecar = LiveSidecarClient::new("http://127.0.0.1:9");
        let window = test_market_window();
        let order = test_queued_order();
        let result = submit_one_live_order_from_worker(LiveOrderWorkerSubmission {
            db_path,
            config: &Config::default(),
            session_id,
            sidecar: &sidecar,
            window: &window,
            order: &order,
            decision_event: None,
            now_ms: 2_000,
        })
        .await;

        assert!(matches!(
            result,
            LiveSubmissionOrderResult::Blocked {
                state: "disarmed",
                ..
            }
        ));
        let db = Database::new(db_path).unwrap();
        let intent_count: u64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM live_order_intents", [], |row| {
                row.get(0)
            })
            .unwrap();
        db.close();
        assert_eq!(intent_count, 0);
    }

    /// Verifies remote refresh cannot overwrite a newer operator control state.
    #[test]
    fn remote_refresh_merge_preserves_control_state() {
        let mut target = test_live_monitor("disarmed");
        let mut updated = test_live_monitor("armed");
        updated.blocked_reason = Some("remote gate".to_string());
        updated.account = Some(test_account_state());

        merge_live_monitor_state(&mut target, updated, LiveMonitorWorkerKind::RemoteRefresh);

        assert_eq!(target.state, "disarmed");
        assert_eq!(target.blocked_reason.as_deref(), Some("remote gate"));
        assert!(target.account.is_some());
    }

    /// Verifies that a queued preflight command refreshes sidecar state without arming.
    #[tokio::test]
    async fn preflight_control_refreshes_remote_state() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/preflight"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::to_value(test_preflight_response()).unwrap()),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::to_value(test_account_state()).unwrap()),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/activity"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::to_value(test_activity_response()).unwrap()),
            )
            .mount(&server)
            .await;

        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveTrading;
        config.live_sidecar_url = server.uri();
        config.feed_event_storage_profile = FeedEventStorageProfile::ReplayGrade;
        let tmp_db = NamedTempFile::new().unwrap();
        let db_path = tmp_db.path().to_str().unwrap();
        let db = Database::new(db_path).unwrap();
        let session_id = db
            .insert_live_session(&LiveSession {
                id: None,
                started_at_ms: 1_000,
                ended_at_ms: None,
                status: "disarmed".to_string(),
                execution_mode: "live_trading".to_string(),
                wallet_address: Some("0xwallet".to_string()),
                proxy_wallet: Some("0xproxy".to_string()),
                enabled_strategies_json: "[\"latency-arb\"]".to_string(),
                config_fingerprint: "{}".to_string(),
                cash_cap_usd: 100.0,
                details_json: Some("{}".to_string()),
            })
            .unwrap();
        db.insert_live_control_command(&crate::types::LiveControlCommand {
            id: None,
            session_id,
            action: "preflight".to_string(),
            actor: "admin".to_string(),
            reason: "refresh gates".to_string(),
            requested_at_ms: 1_100,
            applied_at_ms: None,
            status: "pending".to_string(),
            details_json: None,
        })
        .unwrap();
        db.close();

        let clock = BacktestClock::new();
        clock.set(2_000);
        let mut monitor = LiveTradingMonitor {
            sidecar: LiveSidecarClient::new(&server.uri()),
            session_id,
            state: "disarmed".to_string(),
            preflight: None,
            account: None,
            activity: None,
            risk: None,
            degradation: LiveDegradationTracker::default(),
            blocked_reason: None,
            finished: false,
        };
        monitor
            .apply_pending_controls(db_path, &config, &clock)
            .await
            .unwrap();

        let conn = rusqlite::Connection::open(db_path).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM live_control_commands ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let snapshot_count: u64 = conn
            .query_row("SELECT COUNT(*) FROM live_account_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(status, "applied");
        assert_eq!(snapshot_count, 1);
        assert_eq!(monitor.state, "disarmed");
        assert!(
            monitor
                .blocked_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("replay-grade"))
        );
    }

    /// Verifies that terminal live-control states cannot be cleared by disarm.
    #[test]
    fn disarm_rejects_terminal_or_unknown_state() {
        assert!(ensure_state_allows_disarm("armed").is_ok());
        assert!(ensure_state_allows_disarm("stop_after_flat").is_ok());
        assert!(ensure_state_allows_disarm("unknown_order").is_err());
        assert!(ensure_state_allows_disarm("halted").is_err());
    }

    /// Verifies that arming is limited to the disarmed state.
    #[test]
    fn arm_state_gate_requires_disarmed() {
        assert!(ensure_state_is("disarmed", "disarmed", "arm").is_ok());
        assert!(ensure_state_is("halted", "disarmed", "arm").is_err());
        assert!(ensure_state_is("unknown_order", "disarmed", "arm").is_err());
    }

    /// Verifies older decision outputs cannot submit live orders after newer feed state arrives.
    #[test]
    fn stale_live_decision_rejects_older_sequence() {
        let config = Config::default();
        let output = RuntimeDecisionOutput {
            decision_sequence: 3,
            decision_input_at_ms: 10_000,
            persistence_events: vec![test_critical_signal_event(-1)],
            live_orders: vec![test_queued_order()],
            processed_outcomes: Vec::new(),
            log_events: Vec::new(),
            submission_window: Some(test_market_window()),
            now_ms: 10_000,
        };

        assert!(stale_live_decision_output(&output, 4, 10_001, &config));
    }

    /// Verifies live decisions past the configured age budget are blocked.
    #[test]
    fn stale_live_decision_rejects_old_input_timestamp() {
        let mut config = Config::default();
        config.max_live_decision_age_ms = 50;
        let output = RuntimeDecisionOutput {
            decision_sequence: 4,
            decision_input_at_ms: 10_000,
            persistence_events: vec![test_critical_signal_event(-1)],
            live_orders: vec![test_queued_order()],
            processed_outcomes: Vec::new(),
            log_events: Vec::new(),
            submission_window: Some(test_market_window()),
            now_ms: 10_000,
        };

        assert!(stale_live_decision_output(&output, 4, 10_051, &config));
    }

    /// Verifies current, fresh live decisions are allowed through the runtime gate.
    #[test]
    fn fresh_live_decision_allows_current_sequence() {
        let mut config = Config::default();
        config.max_live_decision_age_ms = 50;
        let output = RuntimeDecisionOutput {
            decision_sequence: 4,
            decision_input_at_ms: 10_000,
            persistence_events: vec![test_critical_signal_event(-1)],
            live_orders: vec![test_queued_order()],
            processed_outcomes: Vec::new(),
            log_events: Vec::new(),
            submission_window: Some(test_market_window()),
            now_ms: 10_000,
        };

        assert!(!stale_live_decision_output(&output, 4, 10_050, &config));
    }

    /// Verifies unsubmitted orders are released after a batch-level blocker.
    #[test]
    fn blocked_live_batch_releases_unsubmitted_orders() {
        let first = test_queued_order();
        let mut second = test_queued_order();
        second.signal_id = -2;
        second.strategy = "spread-capture".to_string();
        second.reserved_cost = 4.0;
        let mut feedback = LiveSubmissionFeedback {
            state_update: None,
            releases: Vec::new(),
            fills: Vec::new(),
            rejected_signal_ids: Vec::new(),
            now_ms: 2_000,
        };

        release_unsubmitted_orders_after_block(&mut feedback, &[first, second], 1);

        assert_eq!(feedback.releases, vec![("spread-capture".to_string(), 4.0)]);
        assert_eq!(feedback.rejected_signal_ids, vec![-2]);
    }

    /// Verifies latest-state decision storage replaces stale feed snapshots.
    #[test]
    fn latest_strategy_evaluation_slot_keeps_newest_request() {
        let latest = LatestStrategyEvaluation::default();
        {
            let mut slot = latest.request.lock().unwrap();
            assert!(slot.replace(Box::new(test_strategy_request(1))).is_none());
            assert!(slot.replace(Box::new(test_strategy_request(2))).is_some());
        }

        let request = latest.request.lock().unwrap().take().unwrap();
        assert_eq!(request.decision_sequence, 2);
    }

    /// Verifies feed-derived decision requests collapse to one latest-state flush.
    #[test]
    fn pending_decision_evaluation_coalesces_feed_batch() {
        let mut pending = PendingDecisionEvaluation::default();

        pending.mark_dirty(1_000, Some(1_000_000));
        pending.mark_dirty(1_001, Some(1_001_000));
        pending.mark_dirty(1_002, Some(1_002_000));

        assert_eq!(pending.dirty_events, 3);
        assert_eq!(pending.coalesced_events, 2);
        assert_eq!(pending.take(), Some((1_002, Some(1_002_000))));
        assert_eq!(pending.take(), None);
        assert_eq!(pending.flushed_evaluations, 1);
    }

    /// Verifies an isolated feed event flushes immediately without artificial debounce.
    #[test]
    fn pending_decision_evaluation_flushes_single_event() {
        let mut pending = PendingDecisionEvaluation::default();

        pending.mark_dirty(2_000, None);

        assert_eq!(pending.take(), Some((2_000, None)));
        assert_eq!(pending.coalesced_events, 0);
    }

    /// Verifies runtime capture metadata rows are bounded payloads for the persistence writer.
    #[test]
    fn runtime_capture_metadata_rows_use_recent_counter_payload() {
        let rows = runtime_capture_metadata_rows(
            FeedEventStorageProfile::ReplayGrade,
            10_000,
            "observing".to_string(),
            "binance:aggTrade=10".to_string(),
        );
        let keys = rows
            .iter()
            .map(|(key, _, _)| key.as_str())
            .collect::<std::collections::HashSet<_>>();

        assert!(keys.contains("runtime_capture_health"));
        assert!(keys.contains("feed_event_classes"));
        assert!(keys.contains("replay_quality_missing_required_classes"));
        assert!(!keys.contains("replay_quality_class"));
    }

    /// Build one passing preflight fixture for live gate tests.
    fn test_preflight_response() -> LivePreflightResponse {
        LivePreflightResponse {
            ok: true,
            mode: "live_trading".to_string(),
            wallet_address: Some("0xwallet".to_string()),
            proxy_wallet: Some("0xproxy".to_string()),
            geoblock_status: LiveCheckStatus::Ok,
            geoblock_country_code: Some("IE".to_string()),
            auth_status: LiveCheckStatus::Ok,
            clock_status: LiveCheckStatus::Ok,
            allowance_status: LiveCheckStatus::Ok,
            user_stream_status: LiveCheckStatus::Ok,
            available_cash_usd: Some(100.0),
            legal_order_min_usd: Some(5.0),
            details_json: None,
            errors: Vec::new(),
        }
    }

    /// Build one passing account fixture for live gate tests.
    fn test_account_state() -> LiveAccountState {
        LiveAccountState {
            timestamp_ms: 1_000,
            wallet_address: Some("0xwallet".to_string()),
            proxy_wallet: Some("0xproxy".to_string()),
            cash_available: 100.0,
            cash_reserved_for_orders: 0.0,
            inventory_mark_value: 0.0,
            redeemable_value: 0.0,
            pending_redeem_value: 0.0,
            total_equity: 100.0,
            allowance_available: Some(100.0),
            details_json: None,
        }
    }

    /// Build one passing activity fixture for live gate tests.
    fn test_activity_response() -> LiveActivityResponse {
        LiveActivityResponse {
            timestamp_ms: 1_000,
            user_stream_status: LiveCheckStatus::Ok,
            user_stream_events: Vec::new(),
            clob_trades: Vec::new(),
            details_json: None,
        }
    }

    /// Build one strategy-evaluation request for latest-slot tests.
    fn test_strategy_request(decision_sequence: u64) -> StrategyEvaluationRequest {
        let now_ms = 10_000 + decision_sequence;
        let ctx = StrategyContext {
            binance_price: 75_000.0,
            binance_momentum: 0.01,
            chainlink_price: Some(75_000.0),
            book_state: crate::types::BookState::default(),
            window_open_price: Some(74_900.0),
            window_time_remaining_ms: 120_000,
            now_us: Some(now_ms * 1_000),
            features: crate::signal_features::SignalFeatureSnapshot::default(),
        };
        StrategyEvaluationRequest {
            decision_sequence,
            book_state: ctx.book_state.clone(),
            now_us: ctx.now_us,
            ctx,
            window: test_market_window(),
            now_ms,
            live_trading_can_submit: true,
        }
    }

    /// Build one market window with live order metadata.
    fn test_market_window() -> MarketWindow {
        MarketWindow {
            market_id: "mkt-test".to_string(),
            question: "Will BTC go up?".to_string(),
            up_token_id: "up-token".to_string(),
            down_token_id: "down-token".to_string(),
            condition_id: "condition".to_string(),
            start_time: 1_000,
            end_time: 301_000,
            slug: "btc-test".to_string(),
            outcome: None,
            resolution_source: Some("gamma".to_string()),
            fee_profile: Some("crypto".to_string()),
            order_min_size: Some(1.0),
            order_price_min_tick_size: Some(0.01),
            maker_base_fee: None,
            taker_base_fee: None,
            rewards_min_size: None,
            rewards_max_spread: None,
            fees_enabled: Some(true),
            fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".to_string()),
            token_fee_rates_json: None,
            accepting_orders: Some(true),
            accepting_orders_timestamp: Some("2026-05-08T00:00:00Z".to_string()),
            clear_book_on_start: Some(false),
        }
    }

    /// Build one queued order intent for live submission tests.
    fn test_queued_order() -> QueuedOrderIntent {
        QueuedOrderIntent {
            signal_id: -1,
            signal_timestamp: 1_900,
            market_id: "mkt-test".to_string(),
            strategy: "latency-arb".to_string(),
            side: SignalDirection::Up,
            token_id: "up-token".to_string(),
            arrival_ts: 2_000,
            requested_price: 0.50,
            limit_price: 0.60,
            requested_size: 10.0,
            reserved_cost: 6.0,
            execution_group_id: None,
            execution_fidelity: ReplayFidelity::RawEvent,
        }
    }

    /// Build one signal row for live intent foreign-key fixtures.
    fn test_signal() -> crate::types::Signal {
        crate::types::Signal {
            timestamp: 1_900,
            strategy: "latency-arb".to_string(),
            strategy_version: "test".to_string(),
            feature_mode: "raw_event_full".to_string(),
            direction: SignalDirection::Up,
            confidence: 0.75,
            binance_price: 75_000.0,
            chainlink_price: 75_000.0,
            up_ask: 0.50,
            down_ask: 0.50,
            up_bid: 0.49,
            down_bid: 0.49,
            expected_edge: Some(0.05),
            metadata: json!({}),
            telemetry: None,
        }
    }

    /// Build critical decision evidence for one live order test signal.
    fn test_critical_signal_event(signal_id: i64) -> LivePersistenceEvent {
        LivePersistenceEvent::Signal {
            signal_id,
            signal: Box::new(test_signal()),
            market_id: "mkt-test".to_string(),
            execution_fidelity: ReplayFidelity::RawEvent,
            order_submitted_at_ms: Some(2_000),
            expected_arrival_at_ms: Some(2_250),
            decision_status: "submitted".to_string(),
            rejection_reason: None,
        }
    }

    /// Build a compact live monitor for merge tests.
    fn test_live_monitor(state: &str) -> LiveTradingMonitor {
        LiveTradingMonitor {
            sidecar: LiveSidecarClient::new("http://127.0.0.1:9"),
            session_id: 1,
            state: state.to_string(),
            preflight: None,
            account: None,
            activity: None,
            risk: None,
            degradation: LiveDegradationTracker::default(),
            blocked_reason: None,
            finished: false,
        }
    }
}
