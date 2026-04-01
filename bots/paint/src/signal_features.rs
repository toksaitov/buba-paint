use std::collections::VecDeque;
use std::fmt;

use crate::config::Config;
use crate::fees::{compute_taker_fee, resolve_fee_params};
use crate::types::{BinanceBookTicker, BinanceDepthSnapshot, BookState, MarketWindow, TopOfBook};

const TRADE_HISTORY_MS: u64 = 5_000;
const QUOTE_HISTORY_MS: u64 = 1_000;
const RETURN_HORIZONS_MS: [u64; 3] = [250, 500, 1_000];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureMode {
    LegacyCore,
    RawEventFull,
}

impl FeatureMode {
    /// Return the persisted feature-mode label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LegacyCore => "legacy_core",
            Self::RawEventFull => "raw_event_full",
        }
    }
}

impl fmt::Display for FeatureMode {
    /// Format the feature mode using the persisted label.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct TradeObservation {
    pub price: f64,
    pub quantity: f64,
    pub signed_quantity: Option<f64>,
    pub event_ms: u64,
    pub event_us: Option<u64>,
}

#[derive(Debug, Clone)]
struct QuoteObservation {
    midpoint: f64,
    event_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SignalState {
    pub binance_price: Option<f64>,
    pub binance_last_event_ms: Option<u64>,
    pub binance_last_event_us: Option<u64>,
    pub binance_trades: VecDeque<TradeObservation>,
    pub binance_book: Option<BinanceBookTicker>,
    pub binance_depth: Option<BinanceDepthSnapshot>,
    pub chainlink_price: Option<f64>,
    pub chainlink_last_event_ms: Option<u64>,
    pub chainlink_last_event_us: Option<u64>,
    pub book_state: BookState,
    pub clob_last_event_ms: Option<u64>,
    pub clob_last_event_us: Option<u64>,
    clob_midpoints: VecDeque<QuoteObservation>,
}

impl SignalState {
    /// Construct an empty signal-state container.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all tracked market state.
    pub fn reset(&mut self) {
        self.binance_price = None;
        self.binance_last_event_ms = None;
        self.binance_last_event_us = None;
        self.binance_trades.clear();
        self.binance_book = None;
        self.binance_depth = None;
        self.chainlink_price = None;
        self.chainlink_last_event_ms = None;
        self.chainlink_last_event_us = None;
        self.book_state = BookState::default();
        self.clob_last_event_ms = None;
        self.clob_last_event_us = None;
        self.clob_midpoints.clear();
    }

    /// Record a Binance trade update and prune stale trade history.
    pub fn update_binance_trade(
        &mut self,
        price: f64,
        quantity: f64,
        signed_quantity: Option<f64>,
        event_time_ms: u64,
        event_micros: Option<u64>,
    ) {
        self.binance_price = Some(price);
        self.binance_last_event_ms = Some(event_time_ms);
        self.binance_last_event_us = event_micros;
        self.binance_trades.push_back(TradeObservation {
            price,
            quantity,
            signed_quantity,
            event_ms: event_time_ms,
            event_us: event_micros,
        });
        self.prune(event_time_ms);
    }

    /// Record a Binance top-of-book update.
    pub fn update_binance_book(
        &mut self,
        best_bid: f64,
        best_ask: f64,
        bid_size: f64,
        ask_size: f64,
        event_ms: u64,
        sequence_key: Option<String>,
    ) {
        self.binance_last_event_ms = Some(event_ms);
        self.binance_book = Some(BinanceBookTicker {
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            timestamp: event_ms,
            sequence_key,
        });
    }

    /// Record a Binance shallow-depth update.
    pub fn update_binance_depth(
        &mut self,
        bid_levels: Vec<crate::types::OrderLevel>,
        ask_levels: Vec<crate::types::OrderLevel>,
        event_ms: u64,
        sequence_key: Option<String>,
    ) {
        self.binance_last_event_ms = Some(event_ms);
        self.binance_depth = Some(BinanceDepthSnapshot {
            bid_levels,
            ask_levels,
            timestamp: event_ms,
            sequence_key,
        });
    }

    /// Record a Chainlink price update.
    pub fn update_chainlink(&mut self, price: f64, event_time_ms: u64, event_micros: Option<u64>) {
        self.chainlink_price = Some(price);
        self.chainlink_last_event_ms = Some(event_time_ms);
        self.chainlink_last_event_us = event_micros;
    }

    /// Record a Polymarket top-of-book update and quote-churn history.
    pub fn update_clob(
        &mut self,
        book_state: BookState,
        event_time_ms: u64,
        event_micros: Option<u64>,
    ) {
        self.book_state = book_state;
        self.clob_last_event_ms = Some(event_time_ms);
        self.clob_last_event_us = event_micros;
        if let Some(midpoint) = current_summed_midpoint(&self.book_state) {
            let should_record = self
                .clob_midpoints
                .back()
                .is_none_or(|last| (last.midpoint - midpoint).abs() > f64::EPSILON);
            if should_record {
                self.clob_midpoints.push_back(QuoteObservation {
                    midpoint,
                    event_ms: event_time_ms,
                });
            }
        }
        self.prune(event_time_ms);
    }

    /// Return the relative Binance move for the requested horizon.
    #[must_use]
    pub fn return_over_ms(&self, now_ms: u64, horizon_ms: u64) -> Option<f64> {
        let latest = self.binance_price?;
        let cutoff = now_ms.saturating_sub(horizon_ms);
        let base = self
            .binance_trades
            .iter()
            .find(|trade| trade.event_ms >= cutoff)
            .or_else(|| self.binance_trades.front())?;
        if base.price <= 0.0 {
            return None;
        }
        Some((latest - base.price) / base.price)
    }

    /// Return the signed-trade imbalance over the requested horizon.
    #[must_use]
    pub fn trade_imbalance_over_ms(&self, now_ms: u64, horizon_ms: u64) -> Option<f64> {
        let cutoff = now_ms.saturating_sub(horizon_ms);
        let mut total_abs = 0.0;
        let mut total_signed = 0.0;
        for trade in self
            .binance_trades
            .iter()
            .filter(|trade| trade.event_ms >= cutoff)
        {
            let signed = trade.signed_quantity?;
            total_signed += signed;
            total_abs += signed.abs();
        }
        if total_abs <= 0.0 {
            return None;
        }
        Some(total_signed / total_abs)
    }

    /// Return the current Binance book imbalance from the latest top-of-book.
    #[must_use]
    pub fn book_imbalance(&self) -> Option<f64> {
        let book = self.binance_book.as_ref()?;
        let total = book.bid_size + book.ask_size;
        if total <= 0.0 {
            return None;
        }
        Some((book.bid_size - book.ask_size) / total)
    }

    /// Return an estimated sweep cost from the current shallow depth snapshot.
    #[must_use]
    pub fn depth_sweep_cost(&self) -> Option<f64> {
        let depth = self.binance_depth.as_ref()?;
        let book = self.binance_book.as_ref()?;
        let ask_cost = weighted_average(&depth.ask_levels)?;
        let bid_cost = weighted_average(&depth.bid_levels)?;
        let mid = f64::midpoint(book.best_bid, book.best_ask);
        if mid <= 0.0 {
            return None;
        }
        Some(((ask_cost - bid_cost).abs() / 2.0) / mid)
    }

    /// Return the current Polymarket quote-churn rate over the requested window.
    #[must_use]
    pub fn clob_quote_churn_per_s(&self, now_ms: u64, horizon_ms: u64) -> Option<f64> {
        let cutoff = now_ms.saturating_sub(horizon_ms);
        let count = self
            .clob_midpoints
            .iter()
            .filter(|obs| obs.event_ms >= cutoff)
            .count();
        if count == 0 || horizon_ms == 0 {
            return None;
        }
        Some(count as f64 / (horizon_ms as f64 / 1_000.0))
    }

    /// Return the current Polymarket microprice skew.
    #[must_use]
    pub fn clob_microprice_skew(&self) -> Option<f64> {
        let up = microprice(self.book_state.up.as_ref()?)?;
        let down = microprice(self.book_state.down.as_ref()?)?;
        Some((up + down) - 1.0)
    }

    /// Prune short-horizon trade and quote history.
    pub fn prune(&mut self, now_ms: u64) {
        let trade_cutoff = now_ms.saturating_sub(TRADE_HISTORY_MS);
        while self
            .binance_trades
            .front()
            .is_some_and(|trade| trade.event_ms < trade_cutoff)
        {
            self.binance_trades.pop_front();
        }

        let quote_cutoff = now_ms.saturating_sub(QUOTE_HISTORY_MS);
        while self
            .clob_midpoints
            .front()
            .is_some_and(|obs| obs.event_ms < quote_cutoff)
        {
            self.clob_midpoints.pop_front();
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignalFeatureSnapshot {
    pub feature_mode: FeatureMode,
    pub momentum_30s: f64,
    pub distance_from_open_bps: Option<f64>,
    pub return_250ms: Option<f64>,
    pub return_500ms: Option<f64>,
    pub return_1000ms: Option<f64>,
    pub chainlink_basis_bps: Option<f64>,
    pub summed_ask_edge: Option<f64>,
    pub summed_mid_edge: Option<f64>,
    pub binance_age_ms: Option<u64>,
    pub chainlink_age_ms: Option<u64>,
    pub clob_age_ms: Option<u64>,
    pub quote_age_ms: Option<u64>,
    pub book_staleness_ms: Option<u64>,
    pub binance_signed_trade_imbalance: Option<f64>,
    pub binance_book_imbalance: Option<f64>,
    pub binance_depth_sweep_cost: Option<f64>,
    pub polymarket_quote_churn_per_s: Option<f64>,
    pub polymarket_microprice_skew: Option<f64>,
    pub event_to_decision_lag_us: Option<u64>,
    pub expected_up_fee: Option<f64>,
    pub expected_down_fee: Option<f64>,
    pub expected_up_slippage: Option<f64>,
    pub expected_down_slippage: Option<f64>,
}

impl Default for SignalFeatureSnapshot {
    /// Build an empty legacy-core feature snapshot for tests and placeholder contexts.
    fn default() -> Self {
        Self {
            feature_mode: FeatureMode::LegacyCore,
            momentum_30s: 0.0,
            distance_from_open_bps: None,
            return_250ms: None,
            return_500ms: None,
            return_1000ms: None,
            chainlink_basis_bps: None,
            summed_ask_edge: None,
            summed_mid_edge: None,
            binance_age_ms: None,
            chainlink_age_ms: None,
            clob_age_ms: None,
            quote_age_ms: None,
            book_staleness_ms: None,
            binance_signed_trade_imbalance: None,
            binance_book_imbalance: None,
            binance_depth_sweep_cost: None,
            polymarket_quote_churn_per_s: None,
            polymarket_microprice_skew: None,
            event_to_decision_lag_us: None,
            expected_up_fee: None,
            expected_down_fee: None,
            expected_up_slippage: None,
            expected_down_slippage: None,
        }
    }
}

impl SignalFeatureSnapshot {
    /// Count how many optional features are populated.
    #[must_use]
    pub fn available_feature_count(&self) -> u32 {
        let values = [
            self.distance_from_open_bps.map(|_| 1_u32),
            self.return_250ms.map(|_| 1),
            self.return_500ms.map(|_| 1),
            self.return_1000ms.map(|_| 1),
            self.chainlink_basis_bps.map(|_| 1),
            self.summed_ask_edge.map(|_| 1),
            self.summed_mid_edge.map(|_| 1),
            self.binance_age_ms.map(|_| 1),
            self.chainlink_age_ms.map(|_| 1),
            self.clob_age_ms.map(|_| 1),
            self.quote_age_ms.map(|_| 1),
            self.book_staleness_ms.map(|_| 1),
            self.binance_signed_trade_imbalance.map(|_| 1),
            self.binance_book_imbalance.map(|_| 1),
            self.binance_depth_sweep_cost.map(|_| 1),
            self.polymarket_quote_churn_per_s.map(|_| 1),
            self.polymarket_microprice_skew.map(|_| 1),
            self.event_to_decision_lag_us.map(|_| 1),
            self.expected_up_fee.map(|_| 1),
            self.expected_down_fee.map(|_| 1),
            self.expected_up_slippage.map(|_| 1),
            self.expected_down_slippage.map(|_| 1),
        ];
        values.into_iter().flatten().sum()
    }

    /// Serialize the feature snapshot into a JSON payload for persistence.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "featureMode": self.feature_mode.as_str(),
            "momentum30s": self.momentum_30s,
            "distanceFromOpenBps": self.distance_from_open_bps,
            "return250ms": self.return_250ms,
            "return500ms": self.return_500ms,
            "return1000ms": self.return_1000ms,
            "chainlinkBasisBps": self.chainlink_basis_bps,
            "summedAskEdge": self.summed_ask_edge,
            "summedMidEdge": self.summed_mid_edge,
            "binanceAgeMs": self.binance_age_ms,
            "chainlinkAgeMs": self.chainlink_age_ms,
            "clobAgeMs": self.clob_age_ms,
            "quoteAgeMs": self.quote_age_ms,
            "bookStalenessMs": self.book_staleness_ms,
            "binanceSignedTradeImbalance": self.binance_signed_trade_imbalance,
            "binanceBookImbalance": self.binance_book_imbalance,
            "binanceDepthSweepCost": self.binance_depth_sweep_cost,
            "polymarketQuoteChurnPerS": self.polymarket_quote_churn_per_s,
            "polymarketMicropriceSkew": self.polymarket_microprice_skew,
            "eventToDecisionLagUs": self.event_to_decision_lag_us,
            "expectedUpFee": self.expected_up_fee,
            "expectedDownFee": self.expected_down_fee,
            "expectedUpSlippage": self.expected_up_slippage,
            "expectedDownSlippage": self.expected_down_slippage,
        })
    }
}

pub struct SignalFeatureEngine;

struct ExpectedTradeCosts {
    up_fee: Option<f64>,
    down_fee: Option<f64>,
    up_slippage: Option<f64>,
    down_slippage: Option<f64>,
}

impl SignalFeatureEngine {
    /// Build a feature snapshot from the current shared market state.
    #[must_use]
    pub fn compute(
        state: &mut SignalState,
        window: Option<&MarketWindow>,
        window_open_price: Option<f64>,
        binance_momentum: f64,
        now_ms: u64,
        decision_time_us: Option<u64>,
        config: &Config,
    ) -> SignalFeatureSnapshot {
        state.prune(now_ms);

        let distance_from_open_bps =
            compute_distance_from_open_bps(window_open_price, state.binance_price);
        let (return_250ms, return_500ms, return_1000ms) = compute_short_returns(state, now_ms);
        let chainlink_basis_bps =
            compute_chainlink_basis_bps(state.binance_price, state.chainlink_price);
        let summed_ask_edge = current_summed_ask_edge(&state.book_state);
        let summed_mid_edge = current_summed_mid_edge(&state.book_state);
        let quote_age_ms = quote_age_ms(&state.book_state, now_ms);
        let book_staleness_ms = quote_age_ms;
        let binance_age_ms = event_age_ms(state.binance_last_event_ms, now_ms);
        let chainlink_age_ms = event_age_ms(state.chainlink_last_event_ms, now_ms);
        let clob_age_ms = event_age_ms(state.clob_last_event_ms, now_ms);
        let binance_signed_trade_imbalance = state.trade_imbalance_over_ms(now_ms, 1_000);
        let binance_book_imbalance = state.book_imbalance();
        let binance_depth_sweep_cost = state.depth_sweep_cost();
        let polymarket_quote_churn_per_s = state.clob_quote_churn_per_s(now_ms, 1_000);
        let polymarket_microprice_skew = state.clob_microprice_skew();
        let event_to_decision_lag_us = compute_event_to_decision_lag_us(state, decision_time_us);
        let trade_costs = compute_expected_trade_costs(state, window, now_ms, quote_age_ms, config);
        let raw_only_available = [
            binance_signed_trade_imbalance.map(|_| 1_u8),
            binance_book_imbalance.map(|_| 1),
            binance_depth_sweep_cost.map(|_| 1),
            polymarket_quote_churn_per_s.map(|_| 1),
            polymarket_microprice_skew.map(|_| 1),
            event_to_decision_lag_us.map(|_| 1),
        ]
        .into_iter()
        .flatten()
        .count()
            >= 5;

        SignalFeatureSnapshot {
            feature_mode: if raw_only_available {
                FeatureMode::RawEventFull
            } else {
                FeatureMode::LegacyCore
            },
            momentum_30s: binance_momentum,
            distance_from_open_bps,
            return_250ms,
            return_500ms,
            return_1000ms,
            chainlink_basis_bps,
            summed_ask_edge,
            summed_mid_edge,
            binance_age_ms,
            chainlink_age_ms,
            clob_age_ms,
            quote_age_ms,
            book_staleness_ms,
            binance_signed_trade_imbalance,
            binance_book_imbalance,
            binance_depth_sweep_cost,
            polymarket_quote_churn_per_s,
            polymarket_microprice_skew,
            event_to_decision_lag_us,
            expected_up_fee: trade_costs.up_fee,
            expected_down_fee: trade_costs.down_fee,
            expected_up_slippage: trade_costs.up_slippage,
            expected_down_slippage: trade_costs.down_slippage,
        }
    }
}

/// Compute the current basis-points distance from the captured window open.
fn compute_distance_from_open_bps(
    window_open_price: Option<f64>,
    latest_price: Option<f64>,
) -> Option<f64> {
    match (window_open_price, latest_price) {
        (Some(open), Some(latest)) if open > 0.0 => Some(((latest - open) / open) * 10_000.0),
        _ => None,
    }
}

/// Compute the short-horizon return set used by both strategies.
fn compute_short_returns(
    state: &SignalState,
    now_ms: u64,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    (
        state.return_over_ms(now_ms, RETURN_HORIZONS_MS[0]),
        state.return_over_ms(now_ms, RETURN_HORIZONS_MS[1]),
        state.return_over_ms(now_ms, RETURN_HORIZONS_MS[2]),
    )
}

/// Compute the current Chainlink-versus-Binance basis in basis points.
fn compute_chainlink_basis_bps(
    binance_price: Option<f64>,
    chainlink_price: Option<f64>,
) -> Option<f64> {
    match (binance_price, chainlink_price) {
        (Some(binance), Some(chainlink)) if chainlink > 0.0 => {
            Some(((binance - chainlink) / chainlink) * 10_000.0)
        }
        _ => None,
    }
}

/// Convert an optional event timestamp into current age in milliseconds.
fn event_age_ms(last_event_ms: Option<u64>, now_ms: u64) -> Option<u64> {
    last_event_ms.map(|timestamp| now_ms.saturating_sub(timestamp))
}

/// Compute the latest event-to-decision lag when microsecond timestamps exist.
fn compute_event_to_decision_lag_us(
    state: &SignalState,
    decision_time_us: Option<u64>,
) -> Option<u64> {
    let latest_event_us = [
        state.binance_last_event_us,
        state.chainlink_last_event_us,
        state.clob_last_event_us,
    ]
    .into_iter()
    .flatten()
    .max();

    match (decision_time_us, latest_event_us) {
        (Some(decision_us), Some(event_us)) if decision_us >= event_us => {
            Some(decision_us - event_us)
        }
        _ => None,
    }
}

/// Compute the expected taker fee and slippage for both binary legs.
fn compute_expected_trade_costs(
    state: &SignalState,
    window: Option<&MarketWindow>,
    now_ms: u64,
    quote_age_ms: Option<u64>,
    config: &Config,
) -> ExpectedTradeCosts {
    let fee_params = resolve_fee_params(config, window, now_ms);
    let (up_fee, down_fee) = match (&state.book_state.up, &state.book_state.down) {
        (Some(up), Some(down)) => (
            Some(compute_taker_fee(
                up.best_ask,
                1.0,
                fee_params.fee_rate,
                fee_params.exponent,
            )),
            Some(compute_taker_fee(
                down.best_ask,
                1.0,
                fee_params.fee_rate,
                fee_params.exponent,
            )),
        ),
        _ => (None, None),
    };

    ExpectedTradeCosts {
        up_fee,
        down_fee,
        up_slippage: state
            .book_state
            .up
            .as_ref()
            .map(|book| estimate_slippage(book, config.sim_order_latency_ms, quote_age_ms)),
        down_slippage: state
            .book_state
            .down
            .as_ref()
            .map(|book| estimate_slippage(book, config.sim_order_latency_ms, quote_age_ms)),
    }
}

/// Return the current summed ask edge from the binary-market books.
fn current_summed_ask_edge(book_state: &BookState) -> Option<f64> {
    let up = book_state.up.as_ref()?;
    let down = book_state.down.as_ref()?;
    Some(1.0 - up.best_ask - down.best_ask)
}

/// Return the current summed midpoint edge from the binary-market books.
fn current_summed_mid_edge(book_state: &BookState) -> Option<f64> {
    let midpoint = current_summed_midpoint(book_state)?;
    Some(1.0 - midpoint)
}

/// Return the summed midpoint across the up and down books.
fn current_summed_midpoint(book_state: &BookState) -> Option<f64> {
    let up = book_state.up.as_ref()?;
    let down = book_state.down.as_ref()?;
    Some(f64::midpoint(up.best_bid, up.best_ask) + f64::midpoint(down.best_bid, down.best_ask))
}

/// Return the age of the freshest complete binary quote snapshot.
fn quote_age_ms(book_state: &BookState, now_ms: u64) -> Option<u64> {
    let up = book_state.up.as_ref()?;
    let down = book_state.down.as_ref()?;
    Some(now_ms.saturating_sub(up.timestamp.max(down.timestamp)))
}

/// Compute the weighted-average execution price for a set of depth levels.
fn weighted_average(levels: &[crate::types::OrderLevel]) -> Option<f64> {
    let mut total_qty = 0.0;
    let mut total_notional = 0.0;
    for level in levels {
        if level.size <= 0.0 {
            continue;
        }
        total_qty += level.size;
        total_notional += level.price * level.size;
    }
    if total_qty <= 0.0 {
        return None;
    }
    Some(total_notional / total_qty)
}

/// Compute a microprice from a single top-of-book snapshot.
fn microprice(book: &TopOfBook) -> Option<f64> {
    let total_size = book.bid_size + book.ask_size;
    if total_size <= 0.0 {
        return None;
    }
    Some(((book.best_ask * book.bid_size) + (book.best_bid * book.ask_size)) / total_size)
}

/// Estimate one-leg slippage from spread, quote age, and configured latency.
fn estimate_slippage(
    book: &TopOfBook,
    sim_order_latency_ms: u64,
    quote_age_ms: Option<u64>,
) -> f64 {
    let half_spread = ((book.best_ask - book.best_bid) / 2.0).max(0.0);
    let latency_penalty = sim_order_latency_ms as f64 / 200_000.0;
    let age_penalty = quote_age_ms.unwrap_or(0) as f64 / 400_000.0;
    half_spread + latency_penalty + age_penalty
}

#[cfg(test)]
#[path = "tests/signal_features_tests.rs"]
mod tests;
