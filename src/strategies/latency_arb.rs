use crate::config::Config;
use crate::strategies::{Strategy, StrategyResult};
use crate::types::{Signal, SignalDirection, StrategyContext};

const MOMENTUM_BUFFER_SIZE: usize = 1800;

pub struct LatencyArbStrategy {
    last_signal_time: u64,
    momentum_buffer: Vec<f64>,
    adaptive_threshold: f64,
    last_threshold_calc: u64,
}

impl LatencyArbStrategy {
    #[must_use]
    pub fn new(initial_threshold: f64) -> Self {
        Self {
            last_signal_time: 0,
            momentum_buffer: Vec::with_capacity(MOMENTUM_BUFFER_SIZE),
            adaptive_threshold: initial_threshold,
            last_threshold_calc: 0,
        }
    }

    fn get_adaptive_threshold(&mut self, now: u64, base_threshold: f64) -> f64 {
        if now - self.last_threshold_calc < 10_000 {
            return self.adaptive_threshold;
        }
        self.last_threshold_calc = now;

        if self.momentum_buffer.len() < 60 {
            self.adaptive_threshold = base_threshold;
            return self.adaptive_threshold;
        }

        let mut sorted = self.momentum_buffer.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let p85_idx = (sorted.len() as f64 * 0.85).floor() as usize;
        let p85 = sorted[p85_idx];

        self.adaptive_threshold = base_threshold.max(p85);
        self.adaptive_threshold
    }
}

impl Strategy for LatencyArbStrategy {
    fn name(&self) -> &'static str {
        "latency-arb"
    }

    fn evaluate(&mut self, ctx: &StrategyContext, config: &Config, now: u64) -> StrategyResult {
        // Accumulate absolute momentum into the rolling buffer.
        self.momentum_buffer.push(ctx.binance_momentum.abs());
        if self.momentum_buffer.len() > MOMENTUM_BUFFER_SIZE {
            self.momentum_buffer.remove(0);
        }

        // Window-time guard: skip if too close to expiry.
        if ctx.window_time_remaining_ms < config.min_window_time_ms {
            return StrategyResult::None;
        }

        // Need both sides of the book.
        let (Some(up_book), Some(down_book)) = (&ctx.book_state.up, &ctx.book_state.down) else {
            return StrategyResult::None;
        };

        // Cooldown guard.
        if now - self.last_signal_time < config.latency_arb_cooldown_ms {
            return StrategyResult::None;
        }

        let up_ask = up_book.best_ask;
        let down_ask = down_book.best_ask;
        let up_bid = up_book.best_bid;
        let down_bid = down_book.best_bid;

        if up_ask <= 0.0 || down_ask <= 0.0 {
            return StrategyResult::None;
        }

        let effective_threshold =
            self.get_adaptive_threshold(now, config.latency_arb_momentum_threshold);

        // Determine direction based on momentum vs threshold.
        let direction =
            if ctx.binance_momentum > effective_threshold && up_ask < config.latency_arb_max_ask {
                Some(SignalDirection::Up)
            } else if ctx.binance_momentum < -effective_threshold
                && down_ask < config.latency_arb_max_ask
            {
                Some(SignalDirection::Down)
            } else {
                None
            };

        let Some(direction) = direction else {
            return StrategyResult::None;
        };

        let entry_ask = match direction {
            SignalDirection::Up => up_ask,
            SignalDirection::Down => down_ask,
        };

        // Min ask filter: reject degenerate / too-cheap entries.
        if entry_ask < config.latency_arb_min_ask {
            return StrategyResult::None;
        }

        let abs_momentum = ctx.binance_momentum.abs();
        let ratio = abs_momentum / effective_threshold;
        let confidence = (0.40 + 0.30 * ratio).min(1.0);

        self.last_signal_time = now;

        let signal = Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            direction,
            confidence,
            binance_price: ctx.binance_price,
            chainlink_price: ctx.chainlink_price.unwrap_or(0.0),
            up_ask,
            down_ask,
            up_bid,
            down_bid,
            metadata: serde_json::json!({
                "momentum": ctx.binance_momentum,
                "threshold": effective_threshold,
                "ratio": ratio,
            }),
        };

        StrategyResult::Single(signal)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/latency_arb_tests.rs"]
mod tests;
