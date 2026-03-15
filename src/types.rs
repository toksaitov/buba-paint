// Data structures ported from the TypeScript bot.
//
// Conventions:
//   f64  — all prices, sizes, percentages
//   u64  — timestamps (ms since epoch)
//   i64  — database row IDs
//   String — free-form text fields

use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Binance aggregated trade tick
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceTick {
    pub event_time: u64,
    pub price: f64,
    pub quantity: f64,
    pub trade_time: u64,
}

// ---------------------------------------------------------------------------
// CLOB order book
// ---------------------------------------------------------------------------
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
    pub side: String, // "BUY" | "SELL"
}

// ---------------------------------------------------------------------------
// Chainlink price tick
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChainlinkTick {
    pub symbol: String,
    pub timestamp: u64,
    pub value: f64,
}

// ---------------------------------------------------------------------------
// Order book top-of-book / book state
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct TopOfBook {
    pub best_bid: f64,
    pub best_ask: f64,
    pub bid_size: f64,
    pub ask_size: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Default)]
pub struct BookState {
    pub up: Option<TopOfBook>,
    pub down: Option<TopOfBook>,
}

// ---------------------------------------------------------------------------
// Gamma (Polymarket) market metadata
// ---------------------------------------------------------------------------
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
    pub order_price_min_tick_size: f64,
    pub end_date: String,
    pub neg_risk: bool,
    #[serde(rename = "negRiskMarketID")]
    pub neg_risk_market_id: String,
}

// ---------------------------------------------------------------------------
// Resolved 5-minute market window
// ---------------------------------------------------------------------------
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
}

// ---------------------------------------------------------------------------
// Signal direction enum
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDirection {
    Up,
    Down,
}

impl fmt::Display for SignalDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Up => write!(f, "UP"),
            Self::Down => write!(f, "DOWN"),
        }
    }
}

impl FromStr for SignalDirection {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "UP" => Ok(Self::Up),
            "DOWN" => Ok(Self::Down),
            other => Err(format!("invalid SignalDirection: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Trading signal emitted by a strategy
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct Signal {
    pub timestamp: u64,
    pub strategy: String,
    pub direction: SignalDirection,
    pub confidence: f64,
    pub binance_price: f64,
    pub chainlink_price: f64,
    pub up_ask: f64,
    pub down_ask: f64,
    pub up_bid: f64,
    pub down_bid: f64,
    pub metadata: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Strategy evaluation context
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct StrategyContext {
    pub binance_price: f64,
    pub binance_momentum: f64,
    pub chainlink_price: Option<f64>,
    pub book_state: BookState,
    pub window_time_remaining_ms: u64,
}

// ---------------------------------------------------------------------------
// Trade status enum
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeStatus {
    Open,
    Closed,
    Expired,
}

impl fmt::Display for TradeStatus {
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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            "expired" => Ok(Self::Expired),
            other => Err(format!("invalid TradeStatus: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Simulated (paper) trade
// ---------------------------------------------------------------------------
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
}

// ---------------------------------------------------------------------------
// Trade result after settlement
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct TradeResult {
    pub trade_id: i64,
    pub exit_price: f64,
    pub settlement_price: f64,
    pub pnl_0pct: f64,
    pub pnl_1pct: f64,
    pub pnl_2pct: f64,
    pub pnl_3pct: f64,
}

// ---------------------------------------------------------------------------
// WebSocket feed connection status
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedStatus {
    Disconnected,
    Connecting,
    Connected,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/types_tests.rs"]
mod tests;
