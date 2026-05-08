use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use tracing::{error, warn};

use crate::db::database::Database;
use crate::types::{
    MarketWindow, ReplayFidelity, Signal, SimulatedTrade, StrategyRejectionSummaryRecord,
    TradeResult,
};

/// Configuration for the runtime persistence writer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LivePersistenceWriterConfig {
    pub queue_capacity: usize,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
}

/// Snapshot of runtime persistence writer counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LivePersistenceWriterSnapshot {
    pub enqueued: u64,
    pub persisted: u64,
    pub dropped: u64,
    pub queue_full: u64,
    pub write_errors: u64,
    pub batches: u64,
    pub total_write_ms: u64,
    pub max_write_ms: u64,
    pub last_persisted_at_ms: u64,
    pub shutdown_timed_out: u64,
}

/// Non-blocking handle for persistence work emitted by the decision path.
pub(crate) struct LivePersistenceWriter {
    tx: SyncSender<LivePersistenceMessage>,
    metrics: Arc<LivePersistenceWriterMetrics>,
    join: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
enum LivePersistenceMessage {
    Event(Box<LivePersistenceEvent>),
    Shutdown,
}

#[derive(Debug, Default)]
struct LivePersistenceWriterMetrics {
    enqueued: AtomicU64,
    persisted: AtomicU64,
    dropped: AtomicU64,
    queue_full: AtomicU64,
    write_errors: AtomicU64,
    batches: AtomicU64,
    total_write_ms: AtomicU64,
    max_write_ms: AtomicU64,
    last_persisted_at_ms: AtomicU64,
    shutdown_timed_out: AtomicU64,
}

/// Runtime evidence that must be persisted away from the decision path.
#[derive(Debug, Clone)]
pub(crate) enum LivePersistenceEvent {
    Signal {
        signal_id: i64,
        signal: Box<Signal>,
        market_id: String,
        execution_fidelity: ReplayFidelity,
        order_submitted_at_ms: Option<u64>,
        expected_arrival_at_ms: Option<u64>,
        decision_status: String,
        rejection_reason: Option<String>,
    },
    SignalOutcome {
        signal_id: i64,
        processed_at_ms: u64,
        processed_at_us: Option<u64>,
        effective_arrival_delay_ms: u64,
        decision_status: String,
        rejection_reason: Option<String>,
    },
    OpenTrade(Box<SimulatedTrade>),
    CloseTrade {
        trade_id: i64,
        result: Box<TradeResult>,
        balance: BalancePersistenceEvent,
    },
    MarketClosed {
        market_id: String,
    },
    MarketResolved {
        market_id: String,
        outcome: String,
    },
    RejectionSummaries(Vec<StrategyRejectionSummaryRecord>),
}

/// Balance-log row emitted by the in-memory bankroll.
#[derive(Debug, Clone)]
pub(crate) struct BalancePersistenceEvent {
    pub timestamp_ms: u64,
    pub event: String,
    pub trade_id: Option<i64>,
    pub amount: f64,
    pub balance: f64,
}

impl LivePersistenceWriter {
    /// Start one runtime persistence writer for a database path.
    pub(crate) fn start(
        db_path: String,
        config: LivePersistenceWriterConfig,
    ) -> anyhow::Result<Self> {
        let capacity = config.queue_capacity.max(1);
        let batch_size = config.batch_size.max(1);
        let flush_interval = Duration::from_millis(config.flush_interval_ms.max(1));
        let (tx, rx) = sync_channel(capacity);
        let metrics = Arc::new(LivePersistenceWriterMetrics::default());
        let worker_metrics = Arc::clone(&metrics);
        let join = thread::Builder::new()
            .name("buba-live-persistence".to_string())
            .spawn(move || {
                run_persistence_writer(db_path, rx, worker_metrics, batch_size, flush_interval);
            })
            .context("spawning live persistence writer")?;
        Ok(Self {
            tx,
            metrics,
            join: Some(join),
        })
    }

    /// Enqueue one persistence event without blocking the caller.
    pub(crate) fn try_enqueue(&self, event: LivePersistenceEvent) -> bool {
        match self
            .tx
            .try_send(LivePersistenceMessage::Event(Box::new(event)))
        {
            Ok(()) => {
                self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Full(LivePersistenceMessage::Event(event))) => {
                self.metrics.queue_full.fetch_add(1, Ordering::Relaxed);
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                drop(event);
                false
            }
            Err(TrySendError::Disconnected(LivePersistenceMessage::Event(event))) => {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                drop(event);
                false
            }
            Err(
                TrySendError::Full(LivePersistenceMessage::Shutdown)
                | TrySendError::Disconnected(LivePersistenceMessage::Shutdown),
            ) => true,
        }
    }

    /// Return current persistence writer counters.
    pub(crate) fn snapshot(&self) -> LivePersistenceWriterSnapshot {
        self.metrics.snapshot()
    }

    /// Request shutdown and join after flushing queued rows.
    #[cfg(test)]
    pub(crate) fn shutdown(mut self) {
        self.shutdown_with_timeout(Duration::from_secs(5));
    }

    /// Request shutdown and wait up to the supplied timeout.
    pub(crate) fn shutdown_with_timeout(&mut self, timeout: Duration) -> bool {
        let _ = self.tx.try_send(LivePersistenceMessage::Shutdown);
        let (replacement_tx, _replacement_rx) = sync_channel(1);
        let original_tx = std::mem::replace(&mut self.tx, replacement_tx);
        drop(original_tx);
        let Some(join) = self.join.take() else {
            return true;
        };
        if join_with_timeout(join, timeout) {
            true
        } else {
            self.metrics
                .shutdown_timed_out
                .fetch_add(1, Ordering::Relaxed);
            warn!("live persistence writer shutdown timed out; worker left detached");
            false
        }
    }
}

impl LivePersistenceWriterMetrics {
    /// Return an immutable metrics snapshot.
    fn snapshot(&self) -> LivePersistenceWriterSnapshot {
        LivePersistenceWriterSnapshot {
            enqueued: self.enqueued.load(Ordering::Relaxed),
            persisted: self.persisted.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            queue_full: self.queue_full.load(Ordering::Relaxed),
            write_errors: self.write_errors.load(Ordering::Relaxed),
            batches: self.batches.load(Ordering::Relaxed),
            total_write_ms: self.total_write_ms.load(Ordering::Relaxed),
            max_write_ms: self.max_write_ms.load(Ordering::Relaxed),
            last_persisted_at_ms: self.last_persisted_at_ms.load(Ordering::Relaxed),
            shutdown_timed_out: self.shutdown_timed_out.load(Ordering::Relaxed),
        }
    }
}

/// Join one worker thread without waiting past the configured shutdown budget.
fn join_with_timeout(join: thread::JoinHandle<()>, timeout: Duration) -> bool {
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
            "live persistence writer thread panicked during shutdown"
        );
    }
    true
}

/// Run the blocking persistence loop on a dedicated OS thread.
#[allow(clippy::needless_pass_by_value)]
fn run_persistence_writer(
    db_path: String,
    rx: Receiver<LivePersistenceMessage>,
    metrics: Arc<LivePersistenceWriterMetrics>,
    batch_size: usize,
    flush_interval: Duration,
) {
    let db = match Database::new(&db_path) {
        Ok(db) => db,
        Err(error) => {
            error!(%error, "live persistence writer failed to open database");
            return;
        }
    };
    let mut batch = Vec::with_capacity(batch_size);
    loop {
        match rx.recv_timeout(flush_interval) {
            Ok(LivePersistenceMessage::Event(event)) => {
                batch.push(*event);
                if batch.len() >= batch_size {
                    flush_batch(&db, &metrics, &mut batch);
                }
            }
            Err(RecvTimeoutError::Timeout) => flush_batch(&db, &metrics, &mut batch),
            Ok(LivePersistenceMessage::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                flush_batch(&db, &metrics, &mut batch);
                break;
            }
        }
    }
    db.close();
}

/// Flush one batch of persistence events and update metrics.
fn flush_batch(
    db: &Database,
    metrics: &LivePersistenceWriterMetrics,
    batch: &mut Vec<LivePersistenceEvent>,
) {
    if batch.is_empty() {
        return;
    }
    let started = Instant::now();
    let mut persisted = 0_u64;
    let mut failed = 0_u64;
    for event in batch.drain(..) {
        match persist_event(db, event) {
            Ok(()) => persisted += 1,
            Err(error) => {
                failed += 1;
                error!(%error, "failed to persist live runtime evidence");
            }
        }
    }
    let elapsed_ms = started.elapsed().as_millis() as u64;
    metrics.persisted.fetch_add(persisted, Ordering::Relaxed);
    metrics.dropped.fetch_add(failed, Ordering::Relaxed);
    if failed > 0 {
        metrics.write_errors.fetch_add(failed, Ordering::Relaxed);
    }
    metrics.batches.fetch_add(1, Ordering::Relaxed);
    metrics
        .total_write_ms
        .fetch_add(elapsed_ms, Ordering::Relaxed);
    update_max(&metrics.max_write_ms, elapsed_ms);
    metrics
        .last_persisted_at_ms
        .store(now_ms(), Ordering::Relaxed);
}

/// Persist one runtime evidence event.
fn persist_event(db: &Database, event: LivePersistenceEvent) -> anyhow::Result<()> {
    match event {
        LivePersistenceEvent::Signal {
            signal_id,
            signal,
            market_id,
            execution_fidelity,
            order_submitted_at_ms,
            expected_arrival_at_ms,
            decision_status,
            rejection_reason,
        } => persist_signal_event(
            db,
            signal_id,
            &signal,
            &market_id,
            execution_fidelity,
            order_submitted_at_ms,
            expected_arrival_at_ms,
            &decision_status,
            rejection_reason.as_deref(),
        ),
        LivePersistenceEvent::SignalOutcome {
            signal_id,
            processed_at_ms,
            processed_at_us,
            effective_arrival_delay_ms,
            decision_status,
            rejection_reason,
        } => db.update_signal_execution_outcome(
            signal_id,
            processed_at_ms,
            processed_at_us,
            effective_arrival_delay_ms,
            &decision_status,
            rejection_reason.as_deref(),
        ),
        LivePersistenceEvent::OpenTrade(trade) => db.open_trade(&trade).map(|_| ()),
        LivePersistenceEvent::CloseTrade {
            trade_id,
            result,
            balance,
        } => {
            db.close_trade(trade_id, &result)?;
            db.log_balance_event(
                balance.timestamp_ms,
                &balance.event,
                balance.trade_id,
                balance.amount,
                balance.balance,
            )
        }
        LivePersistenceEvent::MarketClosed { market_id } => db.resolve_market(&market_id, "closed"),
        LivePersistenceEvent::MarketResolved { market_id, outcome } => {
            db.resolve_market_with_outcome(&market_id, "resolved", &outcome)
        }
        LivePersistenceEvent::RejectionSummaries(rows) => {
            for row in rows {
                db.log_strategy_rejection_summary(&row)?;
            }
            Ok(())
        }
    }
}

/// Persist one signal row and its telemetry snapshot.
#[allow(clippy::too_many_arguments)]
fn persist_signal_event(
    db: &Database,
    signal_id: i64,
    signal: &Signal,
    market_id: &str,
    execution_fidelity: ReplayFidelity,
    order_submitted_at_ms: Option<u64>,
    expected_arrival_at_ms: Option<u64>,
    decision_status: &str,
    rejection_reason: Option<&str>,
) -> anyhow::Result<()> {
    db.log_signal_with_context_and_id(
        signal_id,
        signal,
        Some(market_id),
        Some(execution_fidelity),
        order_submitted_at_ms,
        expected_arrival_at_ms,
    )?;
    if let Some(telemetry) = signal.telemetry.as_ref() {
        db.upsert_signal_telemetry(
            signal_id,
            telemetry,
            order_submitted_at_ms,
            expected_arrival_at_ms,
            Some(decision_status),
            rejection_reason,
        )?;
    }
    Ok(())
}

/// Updates an atomic maximum value.
fn update_max(slot: &AtomicU64, candidate: u64) {
    let mut current = slot.load(Ordering::Relaxed);
    while candidate > current {
        match slot.compare_exchange(current, candidate, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

/// Return wall-clock milliseconds for writer metrics.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Convert one resolved trade into a balance persistence row.
#[must_use]
pub(crate) fn balance_event_for_trade(
    trade_id: i64,
    timestamp_ms: u64,
    pnl: f64,
    balance: f64,
) -> BalancePersistenceEvent {
    BalancePersistenceEvent {
        timestamp_ms,
        event: "trade_close".to_string(),
        trade_id: Some(trade_id),
        amount: pnl,
        balance,
    }
}

/// Build a closed-market persistence event.
#[must_use]
pub(crate) fn market_closed_event(window: &MarketWindow) -> LivePersistenceEvent {
    LivePersistenceEvent::MarketClosed {
        market_id: window.market_id.clone(),
    }
}

#[cfg(test)]
#[path = "tests/live_persistence_writer_tests.rs"]
mod tests;
