use crate::config::Config;
use crate::fees::{resolve_fee_params, spread_net_edge};
use crate::strategies::{Strategy, StrategyResult};
use crate::types::{
    Signal, SignalDirection, SignalTelemetry, StrategyContext, StrategyRejection,
    StrategyRejectionReason, StrategyRejectionSample,
};

const STRATEGY_NAME: &str = "spread-capture";

pub struct SpreadCaptureStrategy;

/// Hold the current best bid and ask for both spread legs.
#[derive(Clone, Copy)]
struct SpreadBookSnapshot {
    up_ask: f64,
    down_ask: f64,
    up_bid: f64,
    down_bid: f64,
}

/// Hold the net-edge and confidence derived from one spread quote snapshot.
struct SpreadDecision {
    raw_edge: f64,
    expected_fee: f64,
    expected_slippage: f64,
    expected_edge: f64,
    confidence: f64,
}

impl SpreadCaptureStrategy {
    /// Creates a new `SpreadCaptureStrategy`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Builds one explicit rejected result for live diagnostics.
    fn reject(reason: StrategyRejectionReason, sample: StrategyRejectionSample) -> StrategyResult {
        StrategyResult::Rejected(Box::new(StrategyRejection::new(
            STRATEGY_NAME,
            reason,
            sample,
        )))
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
        move_velocity <= config.latency_arb_momentum_threshold * 2.0
            && quote_churn <= config.spread_capture_max_quote_churn_per_s
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
            order_processed_at_ms: None,
            order_processed_at_us: None,
            effective_arrival_delay_ms: None,
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

    /// Builds one rejection sample from the current quote snapshot and optional cost fields.
    fn rejection_sample(
        ctx: &StrategyContext,
        expected_fee: Option<f64>,
        expected_slippage: Option<f64>,
        expected_edge: Option<f64>,
    ) -> StrategyRejectionSample {
        let mut sample = StrategyRejectionSample::from_ctx(ctx);
        sample.expected_fee = expected_fee;
        sample.expected_slippage = expected_slippage;
        sample.expected_edge = expected_edge;
        sample
    }

    /// Validates the current book snapshot and extracts the best quotes for both legs.
    fn snapshot_for_evaluation(
        ctx: &StrategyContext,
        config: &Config,
    ) -> Result<SpreadBookSnapshot, StrategyResult> {
        let (Some(up_book), Some(down_book)) = (&ctx.book_state.up, &ctx.book_state.down) else {
            return Err(Self::reject(
                StrategyRejectionReason::BookUnavailable,
                StrategyRejectionSample::from_ctx(ctx),
            ));
        };

        if !Self::quotes_are_fresh(ctx, config) {
            return Err(Self::reject(
                StrategyRejectionReason::FeaturesStale,
                StrategyRejectionSample::from_ctx(ctx),
            ));
        }

        if ctx
            .features
            .inter_leg_skew_ms
            .is_some_and(|skew| skew > config.spread_capture_max_leg_skew_ms)
        {
            return Err(Self::reject(
                StrategyRejectionReason::LegsOutOfSync,
                StrategyRejectionSample::from_ctx(ctx),
            ));
        }

        if !Self::legging_risk_is_acceptable(ctx, config) {
            return Err(Self::reject(
                StrategyRejectionReason::LeggingRiskTooHigh,
                StrategyRejectionSample::from_ctx(ctx),
            ));
        }

        let snapshot = SpreadBookSnapshot {
            up_ask: up_book.best_ask,
            down_ask: down_book.best_ask,
            up_bid: up_book.best_bid,
            down_bid: down_book.best_bid,
        };

        if snapshot.up_ask <= 0.0 || snapshot.down_ask <= 0.0 {
            return Err(Self::reject(
                StrategyRejectionReason::NonPositiveQuotes,
                StrategyRejectionSample::from_ctx(ctx),
            ));
        }

        if snapshot.up_ask < config.spread_capture_min_ask
            || snapshot.down_ask < config.spread_capture_min_ask
        {
            return Err(Self::reject(
                StrategyRejectionReason::EntryAskBelowMin,
                StrategyRejectionSample::from_ctx(ctx),
            ));
        }

        Ok(snapshot)
    }

    /// Converts one validated spread quote snapshot into a tradeable edge decision.
    fn build_decision(
        ctx: &StrategyContext,
        config: &Config,
        now: u64,
        snapshot: SpreadBookSnapshot,
    ) -> Result<SpreadDecision, StrategyResult> {
        let total_ask = snapshot.up_ask + snapshot.down_ask;
        if total_ask >= config.spread_capture_threshold {
            return Err(Self::reject(
                StrategyRejectionReason::SpreadThresholdNotMet,
                StrategyRejectionSample::from_ctx(ctx),
            ));
        }

        let raw_edge = ctx.features.summed_ask_edge.unwrap_or(1.0 - total_ask);
        let fee_params = resolve_fee_params(config, None, now);
        let expected_fee = ctx.features.expected_up_fee.unwrap_or(0.0)
            + ctx.features.expected_down_fee.unwrap_or(0.0);
        let expected_slippage = ctx.features.expected_up_slippage.unwrap_or(0.0)
            + ctx.features.expected_down_slippage.unwrap_or(0.0);
        let expected_edge = spread_net_edge(
            snapshot.up_ask,
            snapshot.down_ask,
            1.0,
            fee_params.fee_rate,
            fee_params.exponent,
        ) - expected_slippage;
        if expected_edge <= 0.0 {
            return Err(Self::reject(
                StrategyRejectionReason::ExpectedEdgeNonPositive,
                Self::rejection_sample(
                    ctx,
                    Some(expected_fee),
                    Some(expected_slippage),
                    Some(expected_edge),
                ),
            ));
        }

        let max_edge = (1.0 - config.spread_capture_threshold).max(1e-6);
        let confidence = (0.5 + 0.5 * (expected_edge / max_edge)).clamp(0.5, 1.0);

        Ok(SpreadDecision {
            raw_edge,
            expected_fee,
            expected_slippage,
            expected_edge,
            confidence,
        })
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
        STRATEGY_NAME
    }

    /// Evaluates the current book snapshot for a two-leg spread-capture setup.
    fn evaluate(&mut self, ctx: &StrategyContext, config: &Config, now: u64) -> StrategyResult {
        let snapshot = match Self::snapshot_for_evaluation(ctx, config) {
            Ok(snapshot) => snapshot,
            Err(rejection) => return rejection,
        };
        let decision = match Self::build_decision(ctx, config, now, snapshot) {
            Ok(decision) => decision,
            Err(rejection) => return rejection,
        };

        let metadata = serde_json::json!({
            "totalAsk": snapshot.up_ask + snapshot.down_ask,
            "threshold": config.spread_capture_threshold,
            "spreadEdge": decision.raw_edge,
            "expectedFee": decision.expected_fee,
            "expectedSlippage": decision.expected_slippage,
            "expectedEdge": decision.expected_edge,
            "quoteChurnPerS": ctx.features.polymarket_quote_churn_per_s,
            "micropriceSkew": ctx.features.polymarket_microprice_skew,
        });

        let chainlink_price = ctx.chainlink_price.unwrap_or(0.0);
        let telemetry = Some(Self::build_telemetry(
            ctx,
            now,
            Some(decision.expected_fee),
            Some(decision.expected_slippage),
            Some(decision.expected_edge),
        ));

        let up_signal = Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            strategy_version: "v2".to_string(),
            feature_mode: ctx.features.feature_mode.to_string(),
            direction: SignalDirection::Up,
            confidence: decision.confidence,
            binance_price: ctx.binance_price,
            chainlink_price,
            up_ask: snapshot.up_ask,
            down_ask: snapshot.down_ask,
            up_bid: snapshot.up_bid,
            down_bid: snapshot.down_bid,
            expected_edge: Some(decision.expected_edge),
            metadata: metadata.clone(),
            telemetry: telemetry.clone(),
        };

        let down_signal = Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            strategy_version: "v2".to_string(),
            feature_mode: ctx.features.feature_mode.to_string(),
            direction: SignalDirection::Down,
            confidence: decision.confidence,
            binance_price: ctx.binance_price,
            chainlink_price,
            up_ask: snapshot.up_ask,
            down_ask: snapshot.down_ask,
            up_bid: snapshot.up_bid,
            down_bid: snapshot.down_bid,
            expected_edge: Some(decision.expected_edge),
            metadata,
            telemetry,
        };

        StrategyResult::Batch(vec![up_signal, down_signal])
    }
}

#[cfg(test)]
#[path = "tests/spread_capture_tests.rs"]
mod tests;
