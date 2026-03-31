/// Rolling window momentum calculator.
///
/// Tracks `(price, timestamp)` pairs within a configurable time window and
/// reports the relative price change from the oldest to the newest observation.
/// Direct port of the `TypeScript` `MomentumCalculator` class.
use std::collections::VecDeque;

#[derive(Debug, Clone)]
struct PricePoint {
    price: f64,
    timestamp: u64,
}

#[derive(Debug)]
pub struct MomentumCalculator {
    window: VecDeque<PricePoint>,
    window_ms: u64,
}

impl MomentumCalculator {
    /// Creates a new `MomentumCalculator`.
    pub fn new(window_ms: u64) -> Self {
        Self {
            window: VecDeque::new(),
            window_ms,
        }
    }

    /// Record a new price observation and prune stale entries that fall outside
    /// the rolling window.
    pub fn push(&mut self, price: f64, timestamp: u64) {
        self.window.push_back(PricePoint { price, timestamp });
        let cutoff = timestamp.saturating_sub(self.window_ms);
        while let Some(front) = self.window.front() {
            if front.timestamp < cutoff {
                self.window.pop_front();
            } else {
                break;
            }
        }
    }

    /// Return the relative price change `(latest - oldest) / oldest`.
    ///
    /// Returns `0.0` when fewer than two observations are present.
    pub fn get(&self) -> f64 {
        if self.window.len() < 2 {
            return 0.0;
        }
        let oldest = &self.window[0];
        if oldest.price <= 0.0 {
            return 0.0;
        }
        let latest = &self.window[self.window.len() - 1];
        (latest.price - oldest.price) / oldest.price
    }

    /// Discard all observations.
    pub fn reset(&mut self) {
        self.window.clear();
    }
}

#[cfg(test)]
#[path = "tests/momentum_tests.rs"]
mod tests;
