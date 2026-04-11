use std::fmt;
use std::str::FromStr;

use crate::signal_features::SignalFeatureSnapshot;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceTick {
    pub event_time: u64,
    pub price: f64,
    pub quantity: f64,
    pub trade_time: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OrderLevel {
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClobBookSnapshot {
    pub asset_id: String,
    pub market: String,
    pub timestamp: u64,
    pub bids: Vec<OrderLevel>,
    pub asks: Vec<OrderLevel>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClobPriceChange {
    pub asset_id: String,
    pub market: String,
    pub timestamp: u64,
    pub changes: Vec<PriceChangeEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceChangeEntry {
    pub asset_id: String,
    pub price: f64,
    pub size: f64,
    pub side: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChainlinkTick {
    pub symbol: String,
    pub timestamp: u64,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct BinanceBookTicker {
    pub best_bid: f64,
    pub best_ask: f64,
    pub bid_size: f64,
    pub ask_size: f64,
    pub timestamp: u64,
    pub sequence_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BinanceDepthSnapshot {
    pub bid_levels: Vec<OrderLevel>,
    pub ask_levels: Vec<OrderLevel>,
    pub timestamp: u64,
    pub sequence_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TopOfBook {
    pub best_bid: f64,
    pub best_ask: f64,
    pub bid_size: f64,
    pub ask_size: f64,
    pub timestamp: u64,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct BookState {
    pub up: Option<TopOfBook>,
    pub down: Option<TopOfBook>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GammaMarket {
    pub id: String,
    pub question: String,
    pub condition_id: String,
    pub slug: String,
    pub active: bool,
    pub closed: bool,
    pub accepting_orders: bool,
    pub outcomes: Vec<String>,
    pub outcome_prices: Vec<f64>,
    pub clob_token_ids: Vec<String>,
    pub resolution_source: Option<String>,
    pub order_min_size: Option<f64>,
    pub order_price_min_tick_size: Option<f64>,
    pub maker_base_fee: Option<f64>,
    pub taker_base_fee: Option<f64>,
    pub rewards_min_size: Option<f64>,
    pub rewards_max_spread: Option<f64>,
    pub fees_enabled: Option<bool>,
    pub fee_schedule: Option<serde_json::Value>,
    pub accepting_orders_timestamp: Option<String>,
    pub clear_book_on_start: Option<bool>,
    pub end_date: String,
    pub neg_risk: bool,
    #[serde(rename = "negRiskMarketID")]
    pub neg_risk_market_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PolymarketFeeSchedule {
    pub exponent: Option<u32>,
    pub rate: Option<f64>,
    pub taker_only: Option<bool>,
    pub rebate_rate: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct MarketWindow {
    pub market_id: String,
    pub question: String,
    pub up_token_id: String,
    pub down_token_id: String,
    pub condition_id: String,
    pub start_time: u64,
    pub end_time: u64,
    pub slug: String,
    pub outcome: Option<String>,
    pub resolution_source: Option<String>,
    pub fee_profile: Option<String>,
    pub order_min_size: Option<f64>,
    pub order_price_min_tick_size: Option<f64>,
    pub maker_base_fee: Option<f64>,
    pub taker_base_fee: Option<f64>,
    pub rewards_min_size: Option<f64>,
    pub rewards_max_spread: Option<f64>,
    pub fees_enabled: Option<bool>,
    pub fee_schedule_json: Option<String>,
    pub token_fee_rates_json: Option<String>,
    pub accepting_orders: Option<bool>,
    pub accepting_orders_timestamp: Option<String>,
    pub clear_book_on_start: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDirection {
    Up,
    Down,
}

impl fmt::Display for SignalDirection {
    /// Formats the direction using the persisted `UP` or `DOWN` label.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Up => write!(f, "UP"),
            Self::Down => write!(f, "DOWN"),
        }
    }
}

impl FromStr for SignalDirection {
    type Err = String;

    /// Parses a persisted signal direction label.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "UP" => Ok(Self::Up),
            "DOWN" => Ok(Self::Down),
            other => Err(format!("invalid SignalDirection: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub timestamp: u64,
    pub strategy: String,
    pub strategy_version: String,
    pub feature_mode: String,
    pub direction: SignalDirection,
    pub confidence: f64,
    pub binance_price: f64,
    pub chainlink_price: f64,
    pub up_ask: f64,
    pub down_ask: f64,
    pub up_bid: f64,
    pub down_bid: f64,
    pub expected_edge: Option<f64>,
    pub metadata: serde_json::Value,
    pub telemetry: Option<SignalTelemetry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrategyRejectionReason {
    WindowTooLate,
    BookUnavailable,
    FeaturesStale,
    LegsOutOfSync,
    SpreadBudgetTooSmall,
    BlockedByRouter,
    CooldownActive,
    NonPositiveQuotes,
    DirectionNotSelected,
    EntryAskBelowMin,
    EntryAskAboveMax,
    CalmWindowInactive,
    DistanceBelowThreshold,
    VolatilityTooHigh,
    OpenCrossesTooHigh,
    OrderflowNotAligned,
    SpreadThresholdNotMet,
    LeggingRiskTooHigh,
    ExpectedEdgeNonPositive,
    ExpectedEdgeBelowMin,
    DuplicateOpenPosition,
    DuplicatePendingOrder,
}

impl StrategyRejectionReason {
    /// Returns the persisted rejection label used in logs and `SQLite` summaries.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WindowTooLate => "window_too_late",
            Self::BookUnavailable => "book_unavailable",
            Self::FeaturesStale => "features_stale",
            Self::LegsOutOfSync => "legs_out_of_sync",
            Self::SpreadBudgetTooSmall => "spread_budget_too_small",
            Self::BlockedByRouter => "blocked_by_router",
            Self::CooldownActive => "cooldown_active",
            Self::NonPositiveQuotes => "non_positive_quotes",
            Self::DirectionNotSelected => "direction_not_selected",
            Self::EntryAskBelowMin => "entry_ask_below_min",
            Self::EntryAskAboveMax => "entry_ask_above_max",
            Self::CalmWindowInactive => "calm_window_inactive",
            Self::DistanceBelowThreshold => "distance_below_threshold",
            Self::VolatilityTooHigh => "volatility_too_high",
            Self::OpenCrossesTooHigh => "open_crosses_too_high",
            Self::OrderflowNotAligned => "orderflow_not_aligned",
            Self::SpreadThresholdNotMet => "spread_threshold_not_met",
            Self::LeggingRiskTooHigh => "legging_risk_too_high",
            Self::ExpectedEdgeNonPositive => "expected_edge_non_positive",
            Self::ExpectedEdgeBelowMin => "expected_edge_below_min",
            Self::DuplicateOpenPosition => "duplicate_open_position",
            Self::DuplicatePendingOrder => "duplicate_pending_order",
        }
    }
}

impl fmt::Display for StrategyRejectionReason {
    /// Formats the rejection reason using the persisted lowercase label.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct StrategyRejectionSample {
    pub up_ask: Option<f64>,
    pub down_ask: Option<f64>,
    pub total_ask: Option<f64>,
    pub distance_from_open_bps: Option<f64>,
    pub realized_vol_15s_bps: Option<f64>,
    pub distance_vol_ratio: Option<f64>,
    pub open_crosses_30s: Option<u32>,
    pub alignment_fraction: Option<f64>,
    pub external_bias: Option<f64>,
    pub up_edge: Option<f64>,
    pub down_edge: Option<f64>,
    pub expected_fee: Option<f64>,
    pub expected_slippage: Option<f64>,
    pub expected_edge: Option<f64>,
    pub quote_age_ms: Option<u64>,
    pub book_staleness_ms: Option<u64>,
    pub inter_leg_skew_ms: Option<u64>,
    pub available_spread_budget: Option<f64>,
    pub required_pair_notional: Option<f64>,
    pub required_pair_units: Option<f64>,
    pub window_time_remaining_ms: Option<u64>,
    pub quote_churn_per_s: Option<f64>,
    pub move_velocity: Option<f64>,
}

impl StrategyRejectionSample {
    /// Builds a sample with the common quote and timing fields taken from one strategy context.
    #[must_use]
    pub fn from_ctx(ctx: &StrategyContext) -> Self {
        let up_ask = ctx
            .features
            .up_ask
            .or_else(|| ctx.book_state.up.as_ref().map(|book| book.best_ask));
        let down_ask = ctx
            .features
            .down_ask
            .or_else(|| ctx.book_state.down.as_ref().map(|book| book.best_ask));
        Self {
            up_ask,
            down_ask,
            total_ask: ctx.features.total_ask.or(match (up_ask, down_ask) {
                (Some(up), Some(down)) => Some(up + down),
                _ => None,
            }),
            distance_from_open_bps: ctx.features.distance_from_open_bps,
            realized_vol_15s_bps: ctx.features.realized_vol_15s_bps,
            distance_vol_ratio: None,
            open_crosses_30s: ctx.features.open_crosses_30s,
            alignment_fraction: None,
            external_bias: None,
            up_edge: None,
            down_edge: None,
            expected_fee: None,
            expected_slippage: None,
            expected_edge: None,
            quote_age_ms: ctx.features.quote_age_ms,
            book_staleness_ms: ctx.features.book_staleness_ms,
            inter_leg_skew_ms: ctx.features.inter_leg_skew_ms,
            available_spread_budget: None,
            required_pair_notional: None,
            required_pair_units: None,
            window_time_remaining_ms: Some(ctx.window_time_remaining_ms),
            quote_churn_per_s: ctx.features.polymarket_quote_churn_per_s,
            move_velocity: ctx
                .features
                .return_250ms
                .or(ctx.features.return_500ms)
                .or(ctx.features.return_1000ms)
                .or(Some(ctx.binance_momentum))
                .map(f64::abs),
        }
    }

    /// Returns a JSON representation used by aggregated rejection summaries.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "upAsk": self.up_ask,
            "downAsk": self.down_ask,
            "totalAsk": self.total_ask,
            "distanceFromOpenBps": self.distance_from_open_bps,
            "realizedVol15sBps": self.realized_vol_15s_bps,
            "distanceVolRatio": self.distance_vol_ratio,
            "openCrosses30s": self.open_crosses_30s,
            "alignmentFraction": self.alignment_fraction,
            "externalBias": self.external_bias,
            "upEdge": self.up_edge,
            "downEdge": self.down_edge,
            "expectedFee": self.expected_fee,
            "expectedSlippage": self.expected_slippage,
            "expectedEdge": self.expected_edge,
            "quoteAgeMs": self.quote_age_ms,
            "bookStalenessMs": self.book_staleness_ms,
            "interLegSkewMs": self.inter_leg_skew_ms,
            "availableSpreadBudget": self.available_spread_budget,
            "requiredPairNotional": self.required_pair_notional,
            "requiredPairUnits": self.required_pair_units,
            "windowTimeRemainingMs": self.window_time_remaining_ms,
            "quoteChurnPerS": self.quote_churn_per_s,
            "moveVelocity": self.move_velocity,
        })
    }
}

#[derive(Debug, Clone)]
pub struct StrategyRejection {
    pub strategy: String,
    pub reason: StrategyRejectionReason,
    pub sample: StrategyRejectionSample,
}

impl StrategyRejection {
    /// Builds one structured rejection observation for live aggregation.
    #[must_use]
    pub fn new(
        strategy: impl Into<String>,
        reason: StrategyRejectionReason,
        sample: StrategyRejectionSample,
    ) -> Self {
        Self {
            strategy: strategy.into(),
            reason,
            sample,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StrategyRejectionSummaryRecord {
    pub timestamp_ms: u64,
    pub market_id: String,
    pub strategy: String,
    pub reason: String,
    pub count: u64,
    pub details_json: String,
}

#[derive(Debug, Clone)]
pub struct StrategyContext {
    pub binance_price: f64,
    pub binance_momentum: f64,
    pub chainlink_price: Option<f64>,
    pub book_state: BookState,
    pub window_open_price: Option<f64>,
    pub window_time_remaining_ms: u64,
    pub now_us: Option<u64>,
    pub features: SignalFeatureSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeStatus {
    Open,
    Closed,
    Expired,
}

impl fmt::Display for TradeStatus {
    /// Formats the trade status using the persisted lowercase label.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Closed => write!(f, "closed"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

impl FromStr for TradeStatus {
    type Err = String;

    /// Parses a persisted trade status label.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            "expired" => Ok(Self::Expired),
            other => Err(format!("invalid TradeStatus: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimulatedTrade {
    pub id: Option<i64>,
    pub timestamp: u64,
    pub market_id: String,
    pub strategy: String,
    pub side: SignalDirection,
    pub token_id: String,
    pub entry_price: f64,
    pub size: f64,
    pub status: TradeStatus,
    pub signal_id: Option<i64>,
    pub requested_price: Option<f64>,
    pub requested_size: Option<f64>,
    pub filled_size: Option<f64>,
    pub avg_fill_price: Option<f64>,
    pub fill_status: Option<String>,
    pub fill_reason: Option<String>,
    pub fill_latency_ms: Option<u64>,
    pub execution_group_id: Option<String>,
    pub execution_fidelity: Option<String>,
    pub execution_mode: Option<String>,
    pub order_id: Option<String>,
    pub fill_price: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct TradeResult {
    pub trade_id: i64,
    pub exit_price: f64,
    pub settlement_price: f64,
    pub pnl_0pct: f64,
    pub pnl_1pct: f64,
    pub pnl_2pct: f64,
    pub pnl_3pct: f64,
    /// Actual dynamic fee amount computed from Polymarket's fee formula.
    pub fee_amount: f64,
    /// Profit/loss after the dynamic fee.
    pub pnl_net: f64,
    /// Settlement status: provisional (Binance), confirmed (matches Polymarket),
    /// or corrected (was provisional, Polymarket disagreed, adjusted).
    pub settlement_status: String,
    /// The original Binance-based profit/loss before any Polymarket correction
    /// (None for backtested trades or trades that were never provisional).
    pub provisional_pnl: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayFidelity {
    RawEvent,
    LegacySnapshot,
}

impl fmt::Display for ReplayFidelity {
    /// Formats the replay fidelity using the persisted storage label.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RawEvent => write!(f, "raw_event"),
            Self::LegacySnapshot => write!(f, "legacy_snapshot"),
        }
    }
}

impl FromStr for ReplayFidelity {
    type Err = String;

    /// Parses a persisted replay-fidelity label.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "raw_event" => Ok(Self::RawEvent),
            "legacy_snapshot" => Ok(Self::LegacySnapshot),
            other => Err(format!("invalid ReplayFidelity: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeedEvent {
    pub id: Option<i64>,
    pub received_at_ms: u64,
    pub event_at_ms: u64,
    pub received_at_us: Option<u64>,
    pub event_at_us: Option<u64>,
    pub source: String,
    pub event_type: String,
    pub source_topic: Option<String>,
    pub source_symbol: Option<String>,
    pub connection_id: Option<String>,
    pub sequence_key: Option<String>,
    pub market_id: Option<String>,
    pub asset_id: Option<String>,
    pub price: Option<f64>,
    pub trade_size: Option<f64>,
    pub signed_quantity: Option<f64>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
    pub depth_bid_notional: Option<f64>,
    pub depth_ask_notional: Option<f64>,
    pub depth_imbalance: Option<f64>,
    pub microprice: Option<f64>,
    pub payload_json: Option<String>,
    pub details_json: Option<String>,
    pub fidelity: ReplayFidelity,
}

#[derive(Debug, Clone)]
pub struct SignalTelemetry {
    pub generated_at_ms: u64,
    pub generated_at_us: Option<u64>,
    pub order_submitted_at_ms: Option<u64>,
    pub order_submitted_at_us: Option<u64>,
    pub expected_arrival_at_ms: Option<u64>,
    pub expected_arrival_at_us: Option<u64>,
    pub order_processed_at_ms: Option<u64>,
    pub order_processed_at_us: Option<u64>,
    pub effective_arrival_delay_ms: Option<u64>,
    pub binance_age_ms: Option<u64>,
    pub chainlink_age_ms: Option<u64>,
    pub clob_age_ms: Option<u64>,
    pub quote_age_ms: Option<u64>,
    pub book_staleness_ms: Option<u64>,
    pub expected_fee: Option<f64>,
    pub expected_slippage: Option<f64>,
    pub expected_edge: Option<f64>,
    pub available_feature_count: u32,
    pub decision_status: String,
    pub rejection_reason: Option<String>,
    pub features_json: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SignalMetricRecord {
    pub signal_id: i64,
    pub generated_at_ms: u64,
    pub generated_at_us: Option<u64>,
    pub order_submitted_at_ms: Option<u64>,
    pub order_submitted_at_us: Option<u64>,
    pub expected_arrival_at_ms: Option<u64>,
    pub expected_arrival_at_us: Option<u64>,
    pub order_processed_at_ms: Option<u64>,
    pub order_processed_at_us: Option<u64>,
    pub effective_arrival_delay_ms: Option<u64>,
    pub binance_age_ms: Option<u64>,
    pub chainlink_age_ms: Option<u64>,
    pub clob_age_ms: Option<u64>,
    pub quote_age_ms: Option<u64>,
    pub book_staleness_ms: Option<u64>,
    pub expected_fee: Option<f64>,
    pub expected_slippage: Option<f64>,
    pub expected_edge: Option<f64>,
    pub available_feature_count: u32,
    pub decision_status: String,
    pub rejection_reason: Option<String>,
    pub features_json: String,
}

#[derive(Debug, Clone)]
pub struct FeedHealthEvent {
    pub id: Option<i64>,
    pub timestamp_ms: u64,
    pub timestamp_us: Option<u64>,
    pub source: String,
    pub event_type: String,
    pub connection_id: Option<String>,
    pub market_id: Option<String>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveStrategyReadiness {
    LiveReadyV1,
    LiveSupportedButDisabled,
    NotLiveV1,
}

impl LiveStrategyReadiness {
    /// Return the stable label used in logs, docs, and JSON payloads.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveReadyV1 => "live_ready_v1",
            Self::LiveSupportedButDisabled => "live_supported_but_disabled",
            Self::NotLiveV1 => "not_live_v1",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveSession {
    pub id: Option<i64>,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub status: String,
    pub execution_mode: String,
    pub wallet_address: Option<String>,
    pub proxy_wallet: Option<String>,
    pub enabled_strategies_json: String,
    pub config_fingerprint: String,
    pub cash_cap_usd: f64,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveOrderIntent {
    pub id: Option<i64>,
    pub session_id: i64,
    pub signal_id: Option<i64>,
    pub market_id: String,
    pub strategy: String,
    pub side: String,
    pub order_type: String,
    pub status: String,
    pub created_at_ms: u64,
    pub requested_price: Option<f64>,
    pub requested_size: Option<f64>,
    pub limit_price: Option<f64>,
    pub fee_schedule_json: Option<String>,
    pub token_fee_rates_json: Option<String>,
    pub execution_group_id: Option<String>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveOrder {
    pub id: Option<i64>,
    pub session_id: i64,
    pub intent_id: i64,
    pub venue_order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub market_id: String,
    pub token_id: Option<String>,
    pub side: String,
    pub order_type: String,
    pub status: String,
    pub status_reason: Option<String>,
    pub created_at_ms: u64,
    pub acknowledged_at_ms: Option<u64>,
    pub updated_at_ms: u64,
    pub requested_price: Option<f64>,
    pub limit_price: Option<f64>,
    pub requested_size: Option<f64>,
    pub accepted_size: Option<f64>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveFill {
    pub id: Option<i64>,
    pub session_id: i64,
    pub intent_id: Option<i64>,
    pub live_order_id: Option<i64>,
    pub venue_trade_id: Option<String>,
    pub filled_at_ms: u64,
    pub price: f64,
    pub size: f64,
    pub fee_amount: Option<f64>,
    pub fee_rate: Option<f64>,
    pub liquidity_side: Option<String>,
    pub tx_hash: Option<String>,
    pub status: String,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveAccountSnapshot {
    pub id: Option<i64>,
    pub session_id: i64,
    pub timestamp_ms: u64,
    pub cash_available: f64,
    pub cash_reserved_for_orders: f64,
    pub inventory_mark_value: f64,
    pub redeemable_value: f64,
    pub pending_redeem_value: f64,
    pub total_equity: f64,
    pub allowance_available: Option<f64>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveRedemption {
    pub id: Option<i64>,
    pub session_id: i64,
    pub market_id: String,
    pub detected_redeemable_at_ms: u64,
    pub submitted_at_ms: Option<u64>,
    pub confirmed_at_ms: Option<u64>,
    pub cash_credit_observed_at_ms: Option<u64>,
    pub status: String,
    pub redeemable_value: f64,
    pub tx_hash: Option<String>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveReconciliationEvent {
    pub id: Option<i64>,
    pub session_id: i64,
    pub timestamp_ms: u64,
    pub severity: String,
    pub event_type: String,
    pub local_value: Option<f64>,
    pub remote_value: Option<f64>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ControlAuditEntry {
    pub id: Option<i64>,
    pub timestamp_ms: u64,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedStatus {
    Disconnected,
    Connecting,
    Connected,
}

#[cfg(test)]
#[path = "tests/types_tests.rs"]
mod tests;
