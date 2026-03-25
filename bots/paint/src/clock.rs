use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::{SystemTime, UNIX_EPOCH};

/// Abstraction over wall-clock time so the backtest engine can inject
/// a synthetic clock while production code uses real time.
pub trait Clock: Send + Sync {
    /// Returns the current timestamp in milliseconds since the Unix epoch.
    fn now(&self) -> u64;
}

// ---------------------------------------------------------------------------
// SystemClock — real wall-clock time
// ---------------------------------------------------------------------------

/// Returns real wall-clock time via `SystemTime::now()`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

// ---------------------------------------------------------------------------
// BacktestClock — deterministic, lock-free synthetic clock
// ---------------------------------------------------------------------------

/// A synthetic clock backed by an `AtomicU64`.
///
/// Safe to share across rayon threads (`Send + Sync` for free) without
/// a `Mutex`, which matters for parallel parameter sweeps.
pub struct BacktestClock {
    current: AtomicU64,
}

impl BacktestClock {
    /// Creates a new `BacktestClock` initialised to time **0**.
    pub fn new() -> Self {
        Self {
            current: AtomicU64::new(0),
        }
    }

    /// Advances (or rewinds) the clock to `ts` milliseconds since epoch.
    pub fn set(&self, ts: u64) {
        self.current.store(ts, Relaxed);
    }
}

impl Default for BacktestClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for BacktestClock {
    fn now(&self) -> u64 {
        self.current.load(Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/clock_tests.rs"]
mod tests;
