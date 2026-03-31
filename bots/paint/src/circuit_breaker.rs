/// Pauses trading for a configurable duration after a streak of consecutive
/// losses.  Once the pause expires the breaker resets and trading resumes.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    consecutive_losses: u32,
    pause_until: u64,
    max_losses: u32,
    pause_ms: u64,
    /// Timestamp of the last rate-limited "trading paused" log message.
    pub last_paused_log_ms: u64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    ///
    /// * `max_losses` – number of consecutive losses that triggers the pause.
    /// * `pause_ms`   – how long (in ms) to pause trading after triggering.
    pub fn new(max_losses: u32, pause_ms: u64) -> Self {
        Self {
            consecutive_losses: 0,
            pause_until: 0,
            max_losses,
            pause_ms,
            last_paused_log_ms: 0,
        }
    }

    /// Record a trade result.
    ///
    /// A win resets the consecutive-loss counter.  A loss increments it, and if
    /// the counter reaches `max_losses` the breaker triggers: `pause_until` is
    /// set to `now + pause_ms` and the counter resets to 0 (matching the
    /// `TypeScript` behavior).
    pub fn record_result(&mut self, won: bool, now: u64) {
        if won {
            self.consecutive_losses = 0;
            return;
        }
        self.consecutive_losses += 1;
        if self.consecutive_losses >= self.max_losses {
            self.pause_until = now + self.pause_ms;
            self.consecutive_losses = 0;
        }
    }

    /// Returns `true` when the breaker is **not** paused (trading is allowed).
    pub fn can_trade(&self, now: u64) -> bool {
        now >= self.pause_until
    }

    /// Returns `true` when the breaker **is** paused (trading is blocked).
    pub fn is_paused(&self, now: u64) -> bool {
        now < self.pause_until
    }

    /// Log a rate-limited warning when the circuit breaker is paused.
    /// Logs at most once per 60 seconds.
    pub fn log_if_paused(&mut self, now: u64) {
        if !self.can_trade(now)
            && (self.last_paused_log_ms == 0
                || now.saturating_sub(self.last_paused_log_ms) >= 60_000)
        {
            tracing::warn!(
                consecutive_losses = self.max_losses,
                resumes_in_ms = self.pause_until.saturating_sub(now),
                "circuit breaker: trading paused"
            );
            self.last_paused_log_ms = now;
        }
    }

    /// Current consecutive-loss count (resets to 0 after a win or a trigger).
    pub fn consecutive_losses(&self) -> u32 {
        self.consecutive_losses
    }
}

#[cfg(test)]
#[path = "tests/circuit_breaker_tests.rs"]
mod tests;
