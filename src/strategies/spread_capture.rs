use crate::config::Config;
use crate::strategies::{Strategy, StrategyResult};
use crate::types::{Signal, SignalDirection, StrategyContext};

pub struct SpreadCaptureStrategy;

impl SpreadCaptureStrategy {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for SpreadCaptureStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl Strategy for SpreadCaptureStrategy {
    fn name(&self) -> &'static str {
        "spread-capture"
    }

    fn evaluate(&mut self, ctx: &StrategyContext, config: &Config, now: u64) -> StrategyResult {
        let (Some(up_book), Some(down_book)) = (&ctx.book_state.up, &ctx.book_state.down) else {
            return StrategyResult::None;
        };

        let up_ask = up_book.best_ask;
        let down_ask = down_book.best_ask;
        let up_bid = up_book.best_bid;
        let down_bid = down_book.best_bid;

        if up_ask <= 0.0 || down_ask <= 0.0 {
            return StrategyResult::None;
        }

        // Min ask filter: reject degenerate books.
        if up_ask < config.spread_capture_min_ask || down_ask < config.spread_capture_min_ask {
            return StrategyResult::None;
        }

        let total_ask = up_ask + down_ask;
        if total_ask >= config.spread_capture_threshold {
            return StrategyResult::None;
        }

        let edge = 1.0 - total_ask;
        let max_edge = 1.0 - config.spread_capture_threshold;
        let confidence = (0.5 + 0.5 * (edge / max_edge)).min(1.0);

        let metadata = serde_json::json!({
            "totalAsk": total_ask,
            "threshold": config.spread_capture_threshold,
            "spreadEdge": edge,
        });

        let chainlink_price = ctx.chainlink_price.unwrap_or(0.0);

        let up_signal = Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            direction: SignalDirection::Up,
            confidence,
            binance_price: ctx.binance_price,
            chainlink_price,
            up_ask,
            down_ask,
            up_bid,
            down_bid,
            metadata: metadata.clone(),
        };

        let down_signal = Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            direction: SignalDirection::Down,
            confidence,
            binance_price: ctx.binance_price,
            chainlink_price,
            up_ask,
            down_ask,
            up_bid,
            down_bid,
            metadata,
        };

        StrategyResult::Batch(vec![up_signal, down_signal])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/spread_capture_tests.rs"]
mod tests;
