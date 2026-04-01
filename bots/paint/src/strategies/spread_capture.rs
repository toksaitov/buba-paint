use crate::config::Config;
use crate::fees::{resolve_fee_params, spread_net_edge};
use crate::strategies::{Strategy, StrategyResult};
use crate::types::{Signal, SignalDirection, SignalTelemetry, StrategyContext};

pub struct SpreadCaptureStrategy;

impl SpreadCaptureStrategy {
    /// Creates a new `SpreadCaptureStrategy`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Return whether the current quote snapshot is fresh enough to trade.
    fn quotes_are_fresh(ctx: &StrategyContext, config: &Config) -> bool {
        if ctx
            .features
            .book_staleness_ms
            .is_some_and(|age| age > config.max_book_staleness_ms)
        {
            return false;
        }
        ctx.features
            .quote_age_ms
            .is_none_or(|age| age <= config.max_quote_age_ms)
    }

    /// Return whether the external move and quote churn imply excessive legging risk.
    fn legging_risk_is_acceptable(ctx: &StrategyContext, config: &Config) -> bool {
        let move_velocity = ctx
            .features
            .return_250ms
            .or(ctx.features.return_500ms)
            .or(ctx.features.return_1000ms)
            .unwrap_or(ctx.binance_momentum)
            .abs();
        let quote_churn = ctx.features.polymarket_quote_churn_per_s.unwrap_or(0.0);
        move_velocity <= config.latency_arb_momentum_threshold * 2.0 && quote_churn <= 8.0
    }

    /// Build the persisted telemetry snapshot for both spread legs.
    fn build_telemetry(
        ctx: &StrategyContext,
        now: u64,
        expected_fee: Option<f64>,
        expected_slippage: Option<f64>,
        expected_edge: Option<f64>,
    ) -> SignalTelemetry {
        SignalTelemetry {
            generated_at_ms: now,
            generated_at_us: ctx.now_us,
            order_submitted_at_ms: None,
            order_submitted_at_us: None,
            expected_arrival_at_ms: None,
            expected_arrival_at_us: None,
            binance_age_ms: ctx.features.binance_age_ms,
            chainlink_age_ms: ctx.features.chainlink_age_ms,
            clob_age_ms: ctx.features.clob_age_ms,
            quote_age_ms: ctx.features.quote_age_ms,
            book_staleness_ms: ctx.features.book_staleness_ms,
            expected_fee,
            expected_slippage,
            expected_edge,
            available_feature_count: ctx.features.available_feature_count(),
            decision_status: "generated".to_string(),
            rejection_reason: None,
            features_json: ctx.features.to_json(),
        }
    }
}

impl Default for SpreadCaptureStrategy {
    /// Builds the default `SpreadCaptureStrategy`.
    fn default() -> Self {
        Self::new()
    }
}

impl Strategy for SpreadCaptureStrategy {
    /// Returns the persisted name for the spread-capture strategy.
    fn name(&self) -> &'static str {
        "spread-capture"
    }

    /// Evaluates the current book snapshot for a two-leg spread-capture setup.
    fn evaluate(&mut self, ctx: &StrategyContext, config: &Config, now: u64) -> StrategyResult {
        let (Some(up_book), Some(down_book)) = (&ctx.book_state.up, &ctx.book_state.down) else {
            return StrategyResult::None;
        };

        if !Self::quotes_are_fresh(ctx, config) || !Self::legging_risk_is_acceptable(ctx, config) {
            return StrategyResult::None;
        }

        let up_ask = up_book.best_ask;
        let down_ask = down_book.best_ask;
        let up_bid = up_book.best_bid;
        let down_bid = down_book.best_bid;

        if up_ask <= 0.0 || down_ask <= 0.0 {
            return StrategyResult::None;
        }

        if up_ask < config.spread_capture_min_ask || down_ask < config.spread_capture_min_ask {
            return StrategyResult::None;
        }

        let total_ask = up_ask + down_ask;
        if total_ask >= config.spread_capture_threshold {
            return StrategyResult::None;
        }

        let raw_edge = ctx.features.summed_ask_edge.unwrap_or(1.0 - total_ask);
        let fee_params = resolve_fee_params(config, None, now);
        let expected_fee = ctx.features.expected_up_fee.unwrap_or(0.0)
            + ctx.features.expected_down_fee.unwrap_or(0.0);
        let expected_slippage = ctx.features.expected_up_slippage.unwrap_or(0.0)
            + ctx.features.expected_down_slippage.unwrap_or(0.0);
        let expected_edge = spread_net_edge(
            up_ask,
            down_ask,
            1.0,
            fee_params.fee_rate,
            fee_params.exponent,
        ) - expected_slippage;
        if expected_edge <= 0.0 {
            return StrategyResult::None;
        }

        let max_edge = (1.0 - config.spread_capture_threshold).max(1e-6);
        let confidence = (0.5 + 0.5 * (expected_edge / max_edge)).clamp(0.5, 1.0);

        let metadata = serde_json::json!({
            "totalAsk": total_ask,
            "threshold": config.spread_capture_threshold,
            "spreadEdge": raw_edge,
            "expectedFee": expected_fee,
            "expectedSlippage": expected_slippage,
            "expectedEdge": expected_edge,
            "quoteChurnPerS": ctx.features.polymarket_quote_churn_per_s,
            "micropriceSkew": ctx.features.polymarket_microprice_skew,
        });

        let chainlink_price = ctx.chainlink_price.unwrap_or(0.0);
        let telemetry = Some(Self::build_telemetry(
            ctx,
            now,
            Some(expected_fee),
            Some(expected_slippage),
            Some(expected_edge),
        ));

        let up_signal = Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            strategy_version: "v2".to_string(),
            feature_mode: ctx.features.feature_mode.to_string(),
            direction: SignalDirection::Up,
            confidence,
            binance_price: ctx.binance_price,
            chainlink_price,
            up_ask,
            down_ask,
            up_bid,
            down_bid,
            expected_edge: Some(expected_edge),
            metadata: metadata.clone(),
            telemetry: telemetry.clone(),
        };

        let down_signal = Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            strategy_version: "v2".to_string(),
            feature_mode: ctx.features.feature_mode.to_string(),
            direction: SignalDirection::Down,
            confidence,
            binance_price: ctx.binance_price,
            chainlink_price,
            up_ask,
            down_ask,
            up_bid,
            down_bid,
            expected_edge: Some(expected_edge),
            metadata,
            telemetry,
        };

        StrategyResult::Batch(vec![up_signal, down_signal])
    }
}

#[cfg(test)]
#[path = "tests/spread_capture_tests.rs"]
mod tests;
