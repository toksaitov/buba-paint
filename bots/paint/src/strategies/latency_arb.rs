use crate::config::Config;
use crate::strategies::{Strategy, StrategyResult};
use crate::types::{Signal, SignalDirection, SignalTelemetry, StrategyContext};

const MOMENTUM_BUFFER_SIZE: usize = 1800;

pub struct LatencyArbStrategy {
    last_signal_time: u64,
    momentum_buffer: Vec<f64>,
    momentum_timestamps: Vec<u64>,
    adaptive_threshold: f64,
    adaptive_window_ms: u64,
    last_threshold_calc: u64,
}

impl LatencyArbStrategy {
    /// Creates a new `LatencyArbStrategy`.
    #[must_use]
    pub fn new(initial_threshold: f64) -> Self {
        Self {
            last_signal_time: 0,
            momentum_buffer: Vec::with_capacity(MOMENTUM_BUFFER_SIZE),
            momentum_timestamps: Vec::with_capacity(MOMENTUM_BUFFER_SIZE),
            adaptive_threshold: initial_threshold,
            adaptive_window_ms: 1_800_000,
            last_threshold_calc: 0,
        }
    }

    /// Returns adaptive threshold.
    fn get_adaptive_threshold(&mut self, now: u64, base_threshold: f64) -> f64 {
        if now - self.last_threshold_calc < 10_000 {
            return self.adaptive_threshold;
        }
        self.last_threshold_calc = now;

        while self.momentum_timestamps.len() < self.momentum_buffer.len() {
            self.momentum_timestamps.push(now);
        }

        let first_in_window = self
            .momentum_timestamps
            .iter()
            .position(|ts| now.saturating_sub(*ts) <= self.adaptive_window_ms)
            .unwrap_or(self.momentum_buffer.len());

        let window = &self.momentum_buffer[first_in_window..];

        if window.len() < 60 {
            self.adaptive_threshold = base_threshold;
            return self.adaptive_threshold;
        }

        let mut sorted = window.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let p85_idx = (sorted.len() as f64 * 0.85).floor() as usize;
        let p85 = sorted[p85_idx];

        self.adaptive_threshold = base_threshold.max(p85);
        self.adaptive_threshold
    }

    /// Return whether the current feature snapshot is fresh enough to trade.
    fn features_are_fresh(ctx: &StrategyContext, config: &Config) -> bool {
        if ctx
            .features
            .binance_age_ms
            .is_some_and(|age| age > config.max_signal_feed_age_ms)
        {
            return false;
        }
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

    /// Estimate the external directional bias from the open drift and short returns.
    fn external_bias_score(ctx: &StrategyContext, threshold: f64) -> f64 {
        let short_return = ctx
            .features
            .return_250ms
            .or(ctx.features.return_500ms)
            .or(ctx.features.return_1000ms)
            .unwrap_or(ctx.binance_momentum);
        let open_drift = ctx.features.distance_from_open_bps.unwrap_or(0.0) / 10_000.0;
        let basis_adjustment = -ctx.features.chainlink_basis_bps.unwrap_or(0.0) / 10_000.0;
        let normalized_threshold = threshold.max(1e-6);
        (ctx.binance_momentum / normalized_threshold) * 0.45
            + (short_return / normalized_threshold) * 0.35
            + (open_drift / normalized_threshold) * 0.15
            + (basis_adjustment / normalized_threshold) * 0.05
    }

    /// Estimate the directional stale-price edge against the current market odds.
    fn stale_price_edge(
        ctx: &StrategyContext,
        direction: SignalDirection,
        threshold: f64,
    ) -> Option<f64> {
        let open_price = ctx.window_open_price?;
        let drift = (ctx.binance_price - open_price) / open_price.max(1.0);
        let directional_bias = (drift / threshold.max(1e-6)).clamp(-0.25, 0.25);
        let up_ask = ctx.book_state.up.as_ref()?.best_ask;
        let down_ask = ctx.book_state.down.as_ref()?.best_ask;
        match direction {
            SignalDirection::Up => Some((0.5 + directional_bias) - up_ask),
            SignalDirection::Down => Some((0.5 - directional_bias) - down_ask),
        }
    }

    /// Build the persisted telemetry snapshot for a generated signal.
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

    /// Record the latest momentum sample used by the adaptive threshold.
    fn record_momentum_sample(&mut self, momentum: f64, now: u64) {
        self.momentum_buffer.push(momentum.abs());
        self.momentum_timestamps.push(now);
        if self.momentum_buffer.len() > MOMENTUM_BUFFER_SIZE {
            self.momentum_buffer.remove(0);
            self.momentum_timestamps.remove(0);
        }
    }

    /// Pick the direction that clears the current ask and stale-price gates.
    fn select_direction(
        config: &Config,
        external_bias: f64,
        price_levels: &PriceLevels,
        up_edge: f64,
        down_edge: f64,
    ) -> Option<SignalDirection> {
        if external_bias > 1.0 && price_levels.up_ask < config.latency_arb_max_ask && up_edge > 0.0
        {
            return Some(SignalDirection::Up);
        }
        if external_bias < -1.0
            && price_levels.down_ask < config.latency_arb_max_ask
            && down_edge > 0.0
        {
            return Some(SignalDirection::Down);
        }
        None
    }

    /// Compute the expected fee, slippage, and net edge for the selected side.
    fn expected_trade_costs(
        ctx: &StrategyContext,
        direction: SignalDirection,
        up_edge: f64,
        down_edge: f64,
    ) -> (Option<f64>, Option<f64>, Option<f64>) {
        let expected_fee = match direction {
            SignalDirection::Up => ctx.features.expected_up_fee,
            SignalDirection::Down => ctx.features.expected_down_fee,
        };
        let expected_slippage = match direction {
            SignalDirection::Up => ctx.features.expected_up_slippage,
            SignalDirection::Down => ctx.features.expected_down_slippage,
        };
        let gross_edge = match direction {
            SignalDirection::Up => up_edge,
            SignalDirection::Down => down_edge,
        };

        (
            expected_fee,
            expected_slippage,
            Some(gross_edge - expected_fee.unwrap_or(0.0) - expected_slippage.unwrap_or(0.0)),
        )
    }

    /// Build the persisted signal record for the selected direction.
    fn build_signal(
        &self,
        ctx: &StrategyContext,
        now: u64,
        build: &SignalBuildContext<'_>,
    ) -> Signal {
        Signal {
            timestamp: now,
            strategy: self.name().to_string(),
            strategy_version: "v2".to_string(),
            feature_mode: ctx.features.feature_mode.to_string(),
            direction: build.direction,
            confidence: build.confidence,
            binance_price: ctx.binance_price,
            chainlink_price: ctx.chainlink_price.unwrap_or(0.0),
            up_ask: build.price_levels.up_ask,
            down_ask: build.price_levels.down_ask,
            up_bid: build.price_levels.up_bid,
            down_bid: build.price_levels.down_bid,
            expected_edge: build.expected_edge,
            metadata: serde_json::json!({
                "momentum": ctx.binance_momentum,
                "threshold": build.effective_threshold,
                "externalBias": build.external_bias,
                "upEdge": build.up_edge,
                "downEdge": build.down_edge,
                "expectedFee": build.expected_fee,
                "expectedSlippage": build.expected_slippage,
                "expectedEdge": build.expected_edge,
            }),
            telemetry: Some(Self::build_telemetry(
                ctx,
                now,
                build.expected_fee,
                build.expected_slippage,
                build.expected_edge,
            )),
        }
    }
}

/// Hold the selected latency-arb side and its expected execution context.
struct SignalBuildContext<'a> {
    direction: SignalDirection,
    confidence: f64,
    price_levels: &'a PriceLevels,
    effective_threshold: f64,
    external_bias: f64,
    up_edge: f64,
    down_edge: f64,
    expected_fee: Option<f64>,
    expected_slippage: Option<f64>,
    expected_edge: Option<f64>,
}

/// Hold the current best bid/ask pair for both market sides.
struct PriceLevels {
    up_ask: f64,
    down_ask: f64,
    up_bid: f64,
    down_bid: f64,
}

impl Strategy for LatencyArbStrategy {
    /// Returns the persisted name for the latency-arbitrage strategy.
    fn name(&self) -> &'static str {
        "latency-arb"
    }

    /// Evaluates the current book and momentum snapshot for a latency-arb signal.
    fn evaluate(&mut self, ctx: &StrategyContext, config: &Config, now: u64) -> StrategyResult {
        self.adaptive_window_ms = config.latency_arb_adaptive_window_ms;
        self.record_momentum_sample(ctx.binance_momentum, now);

        if ctx.window_time_remaining_ms < config.min_window_time_ms {
            return StrategyResult::None;
        }

        let (Some(up_book), Some(down_book)) = (&ctx.book_state.up, &ctx.book_state.down) else {
            return StrategyResult::None;
        };

        if !Self::features_are_fresh(ctx, config) {
            return StrategyResult::None;
        }

        if now - self.last_signal_time < config.latency_arb_cooldown_ms {
            return StrategyResult::None;
        }

        let price_levels = PriceLevels {
            up_ask: up_book.best_ask,
            down_ask: down_book.best_ask,
            up_bid: up_book.best_bid,
            down_bid: down_book.best_bid,
        };

        if price_levels.up_ask <= 0.0 || price_levels.down_ask <= 0.0 {
            return StrategyResult::None;
        }

        let effective_threshold =
            self.get_adaptive_threshold(now, config.latency_arb_momentum_threshold);
        let external_bias = Self::external_bias_score(ctx, effective_threshold);
        let up_edge = Self::stale_price_edge(ctx, SignalDirection::Up, effective_threshold)
            .unwrap_or_default();
        let down_edge = Self::stale_price_edge(ctx, SignalDirection::Down, effective_threshold)
            .unwrap_or_default();

        let direction =
            Self::select_direction(config, external_bias, &price_levels, up_edge, down_edge);

        let Some(direction) = direction else {
            return StrategyResult::None;
        };

        let entry_ask = match direction {
            SignalDirection::Up => price_levels.up_ask,
            SignalDirection::Down => price_levels.down_ask,
        };

        if entry_ask < config.latency_arb_min_ask {
            return StrategyResult::None;
        }

        let (expected_fee, expected_slippage, expected_edge) =
            Self::expected_trade_costs(ctx, direction, up_edge, down_edge);

        if expected_edge.unwrap_or_default() <= 0.0 {
            return StrategyResult::None;
        }

        let score_strength = match direction {
            SignalDirection::Up => external_bias,
            SignalDirection::Down => -external_bias,
        };
        let confidence = (0.35 + 0.20 * score_strength + expected_edge.unwrap_or_default() * 4.0)
            .clamp(0.35, 1.0);

        self.last_signal_time = now;
        let build = SignalBuildContext {
            direction,
            confidence,
            price_levels: &price_levels,
            effective_threshold,
            external_bias,
            up_edge,
            down_edge,
            expected_fee,
            expected_slippage,
            expected_edge,
        };

        StrategyResult::Single(Box::new(self.build_signal(ctx, now, &build)))
    }
}

#[cfg(test)]
#[path = "tests/latency_arb_tests.rs"]
mod tests;
