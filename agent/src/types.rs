use serde::Serialize;

/// Bot status summary — aggregated from `balance_log`, `simulated_trades`, `trade_results`, `markets`.
#[derive(Debug, Clone, Serialize)]
pub struct BotStatus {
    pub balance: f64,
    pub starting_balance: f64,
    pub execution_mode: String,
    pub live_session_status: Option<String>,
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

/// Chart-safe equity response with invalid baseline rows separated from plotted points.
#[derive(Debug, Clone, Serialize)]
pub struct EquitySeriesResponse {
    pub baseline: Option<BalanceEntry>,
    pub points: Vec<BalanceEntry>,
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

/// One operator-level signal burst collapsed from consecutive raw signal rows.
#[derive(Debug, Clone, Serialize)]
pub struct SignalGroupRow {
    pub id: String,
    pub strategy: String,
    pub direction: String,
    pub market_id: Option<String>,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub count: u64,
    pub first_signal_id: i64,
    pub last_signal_id: i64,
    pub binance_price: Option<f64>,
    pub chainlink_price: Option<f64>,
    pub up_ask: Option<f64>,
    pub down_ask: Option<f64>,
    pub execution_fidelity: Option<String>,
}

/// Grouped signal response used by the dashboard default Signals view.
#[derive(Debug, Clone, Serialize)]
pub struct SignalGroupsResponse {
    pub groups: Vec<SignalGroupRow>,
    pub raw_rows_scanned: u64,
    pub quiet_gap_ms: u64,
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
    pub control_available: bool,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveSessionRow {
    pub id: i64,
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

#[derive(Debug, Clone, Serialize)]
pub struct LiveAccountSnapshotRow {
    pub id: i64,
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

#[derive(Debug, Clone, Serialize)]
pub struct LiveOrderRow {
    pub id: i64,
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

#[derive(Debug, Clone, Serialize)]
pub struct LiveFillRow {
    pub id: i64,
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

#[derive(Debug, Clone, Serialize)]
pub struct LiveRedemptionRow {
    pub id: i64,
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

#[derive(Debug, Clone, Serialize)]
pub struct LiveReconciliationRow {
    pub id: i64,
    pub session_id: i64,
    pub timestamp_ms: u64,
    pub severity: String,
    pub event_type: String,
    pub local_value: Option<f64>,
    pub remote_value: Option<f64>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveStatusResponse {
    pub latest_session: Option<LiveSessionRow>,
    pub latest_account_snapshot: Option<LiveAccountSnapshotRow>,
    pub open_orders: u64,
    pub pending_redemptions: u64,
    pub critical_reconciliation_events: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveSessionsResponse {
    pub sessions: Vec<LiveSessionRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveOrdersResponse {
    pub orders: Vec<LiveOrderRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveFillsResponse {
    pub fills: Vec<LiveFillRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveRedemptionsResponse {
    pub redemptions: Vec<LiveRedemptionRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveReconciliationResponse {
    pub events: Vec<LiveReconciliationRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveControlAuditRow {
    pub id: i64,
    pub timestamp_ms: u64,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveControlAuditResponse {
    pub entries: Vec<LiveControlAuditRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveControlCommandResponse {
    pub ok: bool,
    pub command_id: i64,
    pub action: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradingHealth {
    pub state: String,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradingControlCapability {
    pub enabled: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradingCapabilities {
    pub preflight: TradingControlCapability,
    pub arm: TradingControlCapability,
    pub disarm: TradingControlCapability,
    pub cancel_all: TradingControlCapability,
    pub stop_after_flat: TradingControlCapability,
    pub redeem: TradingControlCapability,
    pub kill_switch: TradingControlCapability,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradingAlert {
    pub severity: String,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShadowSummary {
    pub balance: f64,
    pub starting_balance: f64,
    pub total_pnl: f64,
    pub total_trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub win_rate: f64,
    pub open_trades: u64,
    pub uptime_hours: f64,
    pub high_water_mark: f64,
    pub max_drawdown_pct: f64,
    pub live_session_status: Option<String>,
    pub last_tick_at: Option<u64>,
    pub current_window: Option<WindowInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RealAccountSummary {
    pub available_cash: Option<f64>,
    pub reserved_cash: Option<f64>,
    pub inventory_mark_value: Option<f64>,
    pub redeemable_value: Option<f64>,
    pub pending_redeem_value: Option<f64>,
    pub total_equity: Option<f64>,
    pub allowance_available: Option<f64>,
    pub latest_snapshot_at_ms: Option<u64>,
    pub session_id: Option<i64>,
    pub session_status: Option<String>,
    pub session_started_at_ms: Option<u64>,
    pub wallet_address: Option<String>,
    pub proxy_wallet: Option<String>,
    pub cash_cap_usd: Option<f64>,
    pub enabled_strategies: Vec<String>,
    pub provider: Option<String>,
    pub user_stream_status: Option<String>,
    pub last_user_stream_connected_at_ms: Option<u64>,
    pub last_user_stream_event_at_ms: Option<u64>,
    pub last_account_refresh_at_ms: Option<u64>,
    pub open_orders: u64,
    pub pending_redemptions: u64,
    pub critical_reconciliation_events: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradingRiskSummary {
    pub halt_reason: Option<String>,
    pub halt_at_ms: Option<u64>,
    pub high_water_mark: Option<f64>,
    pub trough_equity: Option<f64>,
    pub current_equity: Option<f64>,
    pub daily_loss_usd: Option<f64>,
    pub session_drawdown_usd: Option<f64>,
    pub closeout_exported_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradingSummary {
    pub runtime_mode: String,
    pub trading_state: String,
    pub process_state: String,
    pub venue_health: TradingHealth,
    pub account_health: TradingHealth,
    pub reconciliation_health: TradingHealth,
    pub shadow_summary: ShadowSummary,
    pub real_account_summary: RealAccountSummary,
    pub risk_summary: TradingRiskSummary,
    pub capabilities: TradingCapabilities,
    pub alerts: Vec<TradingAlert>,
}

/// One `run_metadata` row returned from the bot DB.
#[derive(Debug, Clone)]
pub struct RunMetadataRow {
    pub value: String,
    pub recorded_at_ms: i64,
}

/// Sanitized runtime-config snapshot plus observed runtime context.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeConfigResponse {
    pub snapshot: Option<serde_json::Value>,
    pub snapshot_recorded_at_ms: Option<i64>,
    pub uptime_secs: Option<u64>,
}

/// Bot runtime DB / WAL / SHM file sizes, stat'd per-request by the handler.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeDbFiles {
    pub db_path: String,
    pub db_bytes: Option<u64>,
    pub wal_bytes: Option<u64>,
    pub shm_bytes: Option<u64>,
}

/// Sampler health metadata returned alongside the machine snapshot.
pub type SamplerHealth = crate::machine::MachineSamplerHealth;

/// Top-level response shape for `GET /api/machine`.
#[derive(Debug, Clone, Serialize)]
pub struct MachineResponse {
    pub host: crate::machine::HostIdentity,
    pub agent_started_at_ms: i64,
    pub current: Option<crate::machine::MachineSample>,
    pub history: Vec<crate::machine::MachineSample>,
    pub runtime_db: RuntimeDbFiles,
    pub sampler: SamplerHealth,
}
