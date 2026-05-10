use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use tracing::{error, info, warn};

use crate::db::database::Database;
use crate::types::FeedEvent;

/// Configuration for the feed-event writer worker.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FeedEventWriterConfig {
    pub queue_capacity: usize,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
    pub compact_clob_replay: bool,
}

/// Snapshot of writer counters for logs and runtime health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FeedEventWriterSnapshot {
    pub enqueued: u64,
    pub persisted: u64,
    pub dropped: u64,
    pub queue_full: u64,
    pub write_errors: u64,
    pub terminal_write_errors: u64,
    pub batches: u64,
    pub total_write_ms: u64,
    pub max_write_ms: u64,
    pub last_persisted_at_ms: u64,
    pub shutdown_timed_out: u64,
}

/// Non-blocking handle used by the trading loop to enqueue feed rows.
pub(crate) struct FeedEventWriter {
    tx: SyncSender<FeedEventWriterMessage>,
    metrics: Arc<FeedEventWriterMetrics>,
    join: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
enum FeedEventWriterMessage {
    Event(Box<FeedEvent>),
    Shutdown,
}

#[derive(Debug, Default)]
struct FeedEventWriterMetrics {
    enqueued: AtomicU64,
    persisted: AtomicU64,
    dropped: AtomicU64,
    queue_full: AtomicU64,
    write_errors: AtomicU64,
    terminal_write_errors: AtomicU64,
    batches: AtomicU64,
    total_write_ms: AtomicU64,
    max_write_ms: AtomicU64,
    last_persisted_at_ms: AtomicU64,
    shutdown_timed_out: AtomicU64,
}

impl FeedEventWriter {
    /// Spawns one feed-event writer for a database path.
    pub(crate) fn start(db_path: String, config: FeedEventWriterConfig) -> anyhow::Result<Self> {
        let capacity = config.queue_capacity.max(1);
        let batch_size = config.batch_size.max(1);
        let flush_interval = Duration::from_millis(config.flush_interval_ms.max(1));
        let compact_clob_replay = config.compact_clob_replay;
        let (tx, rx) = sync_channel(capacity);
        let metrics = Arc::new(FeedEventWriterMetrics::default());
        let worker_metrics = Arc::clone(&metrics);
        let join = thread::Builder::new()
            .name("buba-feed-writer".to_string())
            .spawn(move || {
                run_feed_writer(
                    db_path,
                    rx,
                    worker_metrics,
                    batch_size,
                    flush_interval,
                    compact_clob_replay,
                );
            })
            .context("spawning feed writer")?;
        Ok(Self {
            tx,
            metrics,
            join: Some(join),
        })
    }

    /// Enqueues one feed event without blocking the caller.
    pub(crate) fn try_enqueue(&self, event: FeedEvent) -> bool {
        match self
            .tx
            .try_send(FeedEventWriterMessage::Event(Box::new(event)))
        {
            Ok(()) => {
                self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Full(FeedEventWriterMessage::Event(event))) => {
                self.metrics.queue_full.fetch_add(1, Ordering::Relaxed);
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                drop(event);
                false
            }
            Err(TrySendError::Disconnected(FeedEventWriterMessage::Event(event))) => {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                drop(event);
                false
            }
            Err(
                TrySendError::Full(FeedEventWriterMessage::Shutdown)
                | TrySendError::Disconnected(FeedEventWriterMessage::Shutdown),
            ) => true,
        }
    }

    /// Returns the current writer counters.
    pub(crate) fn snapshot(&self) -> FeedEventWriterSnapshot {
        self.metrics.snapshot()
    }

    /// Requests shutdown and joins the worker after flushing queued rows.
    #[cfg(test)]
    pub(crate) fn shutdown(mut self) {
        self.shutdown_with_timeout(Duration::from_secs(5));
    }

    /// Requests shutdown and waits up to the supplied timeout.
    pub(crate) fn shutdown_with_timeout(&mut self, timeout: Duration) -> bool {
        let _ = self.tx.try_send(FeedEventWriterMessage::Shutdown);
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
            warn!("feed writer shutdown timed out; worker left detached");
            false
        }
    }
}

impl FeedEventWriterMetrics {
    /// Returns an immutable snapshot of the current metrics.
    fn snapshot(&self) -> FeedEventWriterSnapshot {
        FeedEventWriterSnapshot {
            enqueued: self.enqueued.load(Ordering::Relaxed),
            persisted: self.persisted.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            queue_full: self.queue_full.load(Ordering::Relaxed),
            write_errors: self.write_errors.load(Ordering::Relaxed),
            terminal_write_errors: self.terminal_write_errors.load(Ordering::Relaxed),
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
        warn!(?error, "feed writer thread panicked during shutdown");
    }
    true
}

/// Runs the blocking writer loop on its own OS thread.
#[allow(clippy::needless_pass_by_value)]
fn run_feed_writer(
    db_path: String,
    rx: Receiver<FeedEventWriterMessage>,
    metrics: Arc<FeedEventWriterMetrics>,
    batch_size: usize,
    flush_interval: Duration,
    compact_clob_replay: bool,
) {
    let db = match Database::open_runtime(&db_path) {
        Ok(db) => db,
        Err(error) => {
            error!(%error, "feed writer failed to open database");
            return;
        }
    };
    let mut batch = Vec::with_capacity(batch_size);
    loop {
        match rx.recv_timeout(flush_interval) {
            Ok(FeedEventWriterMessage::Event(event)) => {
                batch.push(*event);
                if batch.len() >= batch_size
                    && !flush_batch(&db, &metrics, &mut batch, compact_clob_replay)
                {
                    break;
                }
            }
            Ok(FeedEventWriterMessage::Shutdown) => {
                flush_batch(&db, &metrics, &mut batch, compact_clob_replay);
                info!("feed writer stopped");
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                if !flush_batch(&db, &metrics, &mut batch, compact_clob_replay) {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                flush_batch(&db, &metrics, &mut batch, compact_clob_replay);
                break;
            }
        }
    }
    db.close();
}

/// Flushes the current batch and records writer metrics.
fn flush_batch(
    db: &Database,
    metrics: &FeedEventWriterMetrics,
    batch: &mut Vec<FeedEvent>,
    compact_clob_replay: bool,
) -> bool {
    if batch.is_empty() {
        return true;
    }
    let started = Instant::now();
    match db.log_feed_events_batch_with_compact_clob(batch, compact_clob_replay) {
        Ok(count) => {
            let elapsed_ms = started.elapsed().as_millis() as u64;
            metrics.persisted.fetch_add(count, Ordering::Relaxed);
            metrics.batches.fetch_add(1, Ordering::Relaxed);
            metrics
                .total_write_ms
                .fetch_add(elapsed_ms, Ordering::Relaxed);
            update_max(&metrics.max_write_ms, elapsed_ms);
            metrics
                .last_persisted_at_ms
                .store(now_ms(), Ordering::Relaxed);
            batch.clear();
            true
        }
        Err(error) => {
            let terminal = terminal_sqlite_write_error(&error);
            metrics.write_errors.fetch_add(1, Ordering::Relaxed);
            metrics
                .dropped
                .fetch_add(batch.len() as u64, Ordering::Relaxed);
            if terminal {
                metrics
                    .terminal_write_errors
                    .fetch_add(1, Ordering::Relaxed);
                error!(%error, dropped = batch.len(), "feed writer terminal persistence failure");
            } else {
                error!(%error, dropped = batch.len(), "feed writer failed to persist batch");
            }
            batch.clear();
            !terminal
        }
    }
}

/// Return whether one writer error indicates the DB is no longer safe to use.
fn terminal_sqlite_write_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("disk i/o error")
        || message.contains("database disk image is malformed")
        || message.contains("database is corrupt")
        || message.contains("file is not a database")
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

/// Returns the current wall-clock time in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
#[path = "tests/live_feed_writer_tests.rs"]
mod tests;
