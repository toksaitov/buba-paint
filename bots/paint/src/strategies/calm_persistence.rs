use crate::config::Config;
use crate::portfolio::StrategyFamily;
use crate::strategies::{Strategy, StrategyResult};
use crate::types::{
    Signal, SignalDirection, SignalTelemetry, StrategyContext, StrategyRejection,
    StrategyRejectionReason, StrategyRejectionSample,
};

/// Stable persisted name for the calm-persistence strategy.
const STRATEGY_NAME: &str = "calm-persistence";
/// Minimum number of aligned microstructure signals required before trading.
const MIN_ALIGNMENT_SIGNALS: usize = 3;

/// Late-window calm-market strategy that bets on sign persistence into expiry.
pub struct CalmPersistenceStrategy;

impl CalmPersistenceStrategy {
    /// Create one calm-persistence strategy instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Build one explicit rejection result for live diagnostics.
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

    /// Build the persisted telemetry snapshot for one generated calm signal.
    fn build_telemetry(
        ctx: &StrategyContext,
        now: u64,
        expected_fee: f64,
        expected_slippage: f64,
        expected_edge: f64,
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
            expected_fee: Some(expected_fee),
            expected_slippage: Some(expected_slippage),
            expected_edge: Some(expected_edge),
            available_feature_count: ctx.features.available_feature_count(),
            decision_status: "generated".to_string(),
            rejection_reason: None,
            features_json: ctx.features.to_json(),
        }
    }

    /// Build one rejection sample that keeps expected execution costs in telemetry.
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

    /// Attach calm-specific diagnostics to one rejection sample.
    fn enrich_sample(
        sample: &mut StrategyRejectionSample,
        signed_distance_bps: Option<f64>,
        realized_vol_15s_bps: Option<f64>,
        distance_vol_ratio: Option<f64>,
        open_crosses_30s: Option<u32>,
        alignment_fraction: Option<f64>,
    ) {
        sample.distance_from_open_bps = signed_distance_bps.or(sample.distance_from_open_bps);
        sample.realized_vol_15s_bps = realized_vol_15s_bps.or(sample.realized_vol_15s_bps);
        sample.distance_vol_ratio = distance_vol_ratio;
        sample.open_crosses_30s = open_crosses_30s.or(sample.open_crosses_30s);
        sample.alignment_fraction = alignment_fraction;
    }

    /// Return the signed distance from the window open in basis points.
    fn signed_distance_from_open_bps(ctx: &StrategyContext) -> Option<f64> {
        ctx.features.distance_from_open_bps.or_else(|| {
            let open = ctx.window_open_price?;
            if open <= 0.0 {
                return None;
            }
            Some(((ctx.binance_price - open) / open) * 10_000.0)
        })
    }

    /// Measure how many supporting microstructure signals agree with the target direction.
    fn alignment_fraction(ctx: &StrategyContext, direction: SignalDirection) -> Option<f64> {
        let sign = match direction {
            SignalDirection::Up => 1.0,
            SignalDirection::Down => -1.0,
        };

        let mut aligned = 0_u32;
        let mut total = 0_u32;

        for value in [
            ctx.features.return_250ms.map(|v| v * sign),
            ctx.features.return_500ms.map(|v| v * sign),
            ctx.features.return_1000ms.map(|v| v * sign),
            ctx.features
                .binance_signed_trade_imbalance
                .map(|v| v * sign),
            ctx.features.binance_book_imbalance.map(|v| v * sign),
            ctx.features.polymarket_microprice_skew.map(|v| v * sign),
        ]
        .into_iter()
        .flatten()
        {
            total += 1;
            if value > 0.0 {
                aligned += 1;
            }
        }

        if total < MIN_ALIGNMENT_SIGNALS as u32 {
            return None;
        }

        Some(f64::from(aligned) / f64::from(total))
    }

    /// Convert the calm-state feature bundle into a simple fair-probability bias.
    fn fair_bias(
        distance_vol_ratio: f64,
        alignment_fraction: f64,
        open_crosses_30s: u32,
        config: &Config,
    ) -> f64 {
        let distance_term = 0.025 * distance_vol_ratio.max(0.0);
        let alignment_term = 0.12 * (alignment_fraction - 0.5).max(0.0);
        let cross_term = if open_crosses_30s == 0 { 0.02 } else { 0.0 };

        (distance_term + alignment_term + cross_term)
            .clamp(0.0, config.calm_persistence_max_fair_bias)
    }

    /// Convert the calm-state feature bundle into a bounded signal confidence.
    fn confidence(distance_vol_ratio: f64, alignment_fraction: f64, expected_edge: f64) -> f64 {
        (0.30
            + 0.15 * distance_vol_ratio.min(4.0) / 4.0
            + 0.35 * alignment_fraction
            + expected_edge.max(0.0) * 4.0)
            .clamp(0.0, 1.0)
    }
}

impl Strategy for CalmPersistenceStrategy {
    /// Return the persisted strategy name.
    fn name(&self) -> &'static str {
        STRATEGY_NAME
    }

    /// Return the portfolio family used for routing and sleeve attribution.
    fn family(&self) -> StrategyFamily {
        StrategyFamily::CalmPersistence
    }

    /// Evaluate one late-window snapshot for a calm sign-persistence opportunity.
    #[allow(clippy::too_many_lines)]
    fn evaluate(&mut self, ctx: &StrategyContext, config: &Config, now: u64) -> StrategyResult {
        if ctx.window_time_remaining_ms < config.calm_persistence_min_window_time_ms
            || ctx.window_time_remaining_ms > config.calm_persistence_max_window_time_ms
        {
            return Self::reject(
                StrategyRejectionReason::CalmWindowInactive,
                StrategyRejectionSample::from_ctx(ctx),
            );
        }

        let (Some(up_book), Some(down_book)) = (&ctx.book_state.up, &ctx.book_state.down) else {
            return Self::reject(
                StrategyRejectionReason::BookUnavailable,
                StrategyRejectionSample::from_ctx(ctx),
            );
        };

        if !Self::quotes_are_fresh(ctx, config) {
            return Self::reject(
                StrategyRejectionReason::FeaturesStale,
                StrategyRejectionSample::from_ctx(ctx),
            );
        }

        let signed_distance_bps = Self::signed_distance_from_open_bps(ctx).unwrap_or_default();
        if signed_distance_bps.abs() < config.calm_persistence_min_abs_distance_bps {
            let mut sample = StrategyRejectionSample::from_ctx(ctx);
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                ctx.features.realized_vol_15s_bps,
                None,
                ctx.features.open_crosses_30s,
                None,
            );
            return Self::reject(StrategyRejectionReason::DistanceBelowThreshold, sample);
        }

        let direction = if signed_distance_bps > 0.0 {
            SignalDirection::Up
        } else {
            SignalDirection::Down
        };

        let entry_ask = match direction {
            SignalDirection::Up => up_book.best_ask,
            SignalDirection::Down => down_book.best_ask,
        };
        if entry_ask <= 0.0 {
            let mut sample = StrategyRejectionSample::from_ctx(ctx);
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                ctx.features.realized_vol_15s_bps,
                None,
                ctx.features.open_crosses_30s,
                None,
            );
            return Self::reject(StrategyRejectionReason::NonPositiveQuotes, sample);
        }
        if entry_ask > config.calm_persistence_max_ask {
            let mut sample = StrategyRejectionSample::from_ctx(ctx);
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                ctx.features.realized_vol_15s_bps,
                None,
                ctx.features.open_crosses_30s,
                None,
            );
            return Self::reject(StrategyRejectionReason::EntryAskAboveMax, sample);
        }

        let realized_vol_15s_bps = ctx.features.realized_vol_15s_bps.unwrap_or(f64::INFINITY);
        let quote_churn_per_s = ctx.features.polymarket_quote_churn_per_s.unwrap_or(0.0);
        if realized_vol_15s_bps > config.calm_persistence_max_realized_vol_15s_bps
            || quote_churn_per_s > config.calm_persistence_max_quote_churn_per_s
        {
            let mut sample = StrategyRejectionSample::from_ctx(ctx);
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                Some(realized_vol_15s_bps),
                None,
                ctx.features.open_crosses_30s,
                None,
            );
            return Self::reject(StrategyRejectionReason::VolatilityTooHigh, sample);
        }

        let open_crosses_30s = ctx.features.open_crosses_30s.unwrap_or(u32::MAX);
        if open_crosses_30s > config.calm_persistence_max_open_crosses_30s {
            let mut sample = StrategyRejectionSample::from_ctx(ctx);
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                Some(realized_vol_15s_bps),
                None,
                Some(open_crosses_30s),
                None,
            );
            return Self::reject(StrategyRejectionReason::OpenCrossesTooHigh, sample);
        }

        let vol_denominator = realized_vol_15s_bps.max(1.0);
        let distance_vol_ratio = signed_distance_bps.abs() / vol_denominator;
        if distance_vol_ratio < config.calm_persistence_distance_vol_ratio_threshold {
            let mut sample = StrategyRejectionSample::from_ctx(ctx);
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                Some(realized_vol_15s_bps),
                Some(distance_vol_ratio),
                Some(open_crosses_30s),
                None,
            );
            return Self::reject(StrategyRejectionReason::DistanceBelowThreshold, sample);
        }

        let Some(alignment_fraction) = Self::alignment_fraction(ctx, direction) else {
            let mut sample = StrategyRejectionSample::from_ctx(ctx);
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                Some(realized_vol_15s_bps),
                Some(distance_vol_ratio),
                Some(open_crosses_30s),
                None,
            );
            return Self::reject(StrategyRejectionReason::OrderflowNotAligned, sample);
        };
        if alignment_fraction < config.calm_persistence_min_alignment_fraction {
            let mut sample = StrategyRejectionSample::from_ctx(ctx);
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                Some(realized_vol_15s_bps),
                Some(distance_vol_ratio),
                Some(open_crosses_30s),
                Some(alignment_fraction),
            );
            return Self::reject(StrategyRejectionReason::OrderflowNotAligned, sample);
        }

        let expected_fee = match direction {
            SignalDirection::Up => ctx.features.expected_up_fee.unwrap_or(0.0),
            SignalDirection::Down => ctx.features.expected_down_fee.unwrap_or(0.0),
        };
        let expected_slippage = match direction {
            SignalDirection::Up => ctx.features.expected_up_slippage.unwrap_or(0.0),
            SignalDirection::Down => ctx.features.expected_down_slippage.unwrap_or(0.0),
        };
        let fair_bias = Self::fair_bias(
            distance_vol_ratio,
            alignment_fraction,
            open_crosses_30s,
            config,
        );
        let fair_probability = match direction {
            SignalDirection::Up | SignalDirection::Down => 0.5 + fair_bias,
        };
        let expected_edge = fair_probability - entry_ask - expected_fee - expected_slippage;
        if expected_edge <= 0.0 {
            let mut sample = Self::rejection_sample(
                ctx,
                Some(expected_fee),
                Some(expected_slippage),
                Some(expected_edge),
            );
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                Some(realized_vol_15s_bps),
                Some(distance_vol_ratio),
                Some(open_crosses_30s),
                Some(alignment_fraction),
            );
            return Self::reject(StrategyRejectionReason::ExpectedEdgeNonPositive, sample);
        }
        if expected_edge <= config.calm_persistence_min_expected_edge {
            let mut sample = Self::rejection_sample(
                ctx,
                Some(expected_fee),
                Some(expected_slippage),
                Some(expected_edge),
            );
            Self::enrich_sample(
                &mut sample,
                Some(signed_distance_bps),
                Some(realized_vol_15s_bps),
                Some(distance_vol_ratio),
                Some(open_crosses_30s),
                Some(alignment_fraction),
            );
            return Self::reject(StrategyRejectionReason::ExpectedEdgeBelowMin, sample);
        }

        let confidence = Self::confidence(distance_vol_ratio, alignment_fraction, expected_edge);
        let metadata = serde_json::json!({
            "distanceFromOpenBps": signed_distance_bps,
            "realizedVol15sBps": realized_vol_15s_bps,
            "distanceVolRatio": distance_vol_ratio,
            "openCrosses30s": open_crosses_30s,
            "alignmentFraction": alignment_fraction,
            "fairBias": fair_bias,
            "fairProbability": fair_probability,
            "expectedFee": expected_fee,
            "expectedSlippage": expected_slippage,
            "expectedEdge": expected_edge,
            "quoteChurnPerS": quote_churn_per_s,
        });

        StrategyResult::Single(Box::new(Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            strategy_version: "v1".to_string(),
            feature_mode: ctx.features.feature_mode.to_string(),
            direction,
            confidence,
            binance_price: ctx.binance_price,
            chainlink_price: ctx.chainlink_price.unwrap_or(0.0),
            up_ask: up_book.best_ask,
            down_ask: down_book.best_ask,
            up_bid: up_book.best_bid,
            down_bid: down_book.best_bid,
            expected_edge: Some(expected_edge),
            metadata,
            telemetry: Some(Self::build_telemetry(
                ctx,
                now,
                expected_fee,
                expected_slippage,
                expected_edge,
            )),
        }))
    }
}

impl Default for CalmPersistenceStrategy {
    /// Create the default calm-persistence strategy instance.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/calm_persistence_tests.rs"]
mod tests;
