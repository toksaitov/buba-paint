use serde::Serialize;

/// Bot status summary — aggregated from `balance_log`, `simulated_trades`, `trade_results`, `markets`.
#[derive(Debug, Clone, Serialize)]
pub struct BotStatus {
    pub balance: f64,
    pub starting_balance: f64,
    pub total_trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub max_drawdown_pct: f64,
    pub high_water_mark: f64,
    pub uptime_hours: f64,
    pub open_trades: u64,
    pub last_tick_at: Option<u64>,
    pub current_window: Option<WindowInfo>,
}

/// Minimal market window info for the status response.
#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    pub market_id: String,
    pub question: String,
    pub end_time: u64,
}

/// A single trade row joined with its result (if settled).
#[derive(Debug, Clone, Serialize)]
pub struct TradeRow {
    pub id: i64,
    pub timestamp: u64,
    pub market_id: String,
    pub strategy: String,
    pub side: String,
    pub entry_price: f64,
    pub size: f64,
    pub status: String,
    pub pnl: Option<f64>,
    pub settlement_price: Option<f64>,
    pub resolved_at: Option<u64>,
    pub fill_status: Option<String>,
    pub execution_group_id: Option<String>,
    pub execution_fidelity: Option<String>,
    pub filled_size: Option<f64>,
    pub avg_fill_price: Option<f64>,
}

/// Paginated trade response.
#[derive(Debug, Clone, Serialize)]
pub struct TradesResponse {
    pub trades: Vec<TradeRow>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

/// A single balance log entry.
#[derive(Debug, Clone, Serialize)]
pub struct BalanceEntry {
    pub id: i64,
    pub timestamp: u64,
    pub event: String,
    pub trade_id: Option<i64>,
    pub amount: f64,
    pub balance: f64,
}

/// Balance log response.
#[derive(Debug, Clone, Serialize)]
pub struct BalanceResponse {
    pub entries: Vec<BalanceEntry>,
}

/// A single signal row.
#[derive(Debug, Clone, Serialize)]
pub struct SignalRow {
    pub id: i64,
    pub timestamp: u64,
    pub strategy: String,
    pub direction: String,
    pub binance_price: Option<f64>,
    pub chainlink_price: Option<f64>,
    pub up_ask: Option<f64>,
    pub down_ask: Option<f64>,
    pub metadata: Option<String>,
    pub market_id: Option<String>,
    pub execution_fidelity: Option<String>,
}

/// Signals response.
#[derive(Debug, Clone, Serialize)]
pub struct SignalsResponse {
    pub signals: Vec<SignalRow>,
}

/// Per-strategy stats.
#[derive(Debug, Clone, Serialize)]
pub struct StrategyStats {
    pub trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub total_pnl: f64,
    pub win_rate: f64,
}

/// Aggregated stats response.
#[derive(Debug, Clone, Serialize)]
pub struct StatsResponse {
    pub by_strategy: std::collections::HashMap<String, StrategyStats>,
}

/// Bot process status.
#[derive(Debug, Clone, Serialize)]
pub struct BotProcessStatus {
    pub active: bool,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
}

/// Bot log response.
#[derive(Debug, Clone, Serialize)]
pub struct LogsResponse {
    pub lines: Vec<String>,
}

/// `WebSocket` push message.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum WsMessage {
    #[serde(rename = "trade")]
    Trade(TradeRow),
    #[serde(rename = "balance")]
    Balance(BalanceEntry),
    #[serde(rename = "signal")]
    Signal(SignalRow),
    #[serde(rename = "status")]
    Status(BotStatus),
}
