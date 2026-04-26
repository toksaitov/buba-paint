export interface User {
  id: string;
  username: string;
  role: "admin" | "observer";
}

export interface Bot {
  id: string;
  name: string;
}

export interface BotStatus {
  balance: number;
  starting_balance: number;
  execution_mode: string;
  live_session_status: string | null;
  total_trades: number;
  wins: number;
  losses: number;
  win_rate: number;
  total_pnl: number;
  max_drawdown_pct: number;
  high_water_mark: number;
  uptime_hours: number;
  current_window: {
    market_id: string;
    question: string;
    end_time: number;
  } | null;
  open_trades: number;
  last_tick_at: number | null;
}

export interface TradeRow {
  id: number;
  strategy: string;
  side: string;
  token_id: string;
  size: number;
  entry_price: number;
  timestamp: number;
  market_id: string;
  status: string;
  pnl: number | null;
  settlement_price: number | null;
  resolved_at: number | null;
  fill_status?: string | null;
  execution_group_id?: string | null;
  execution_fidelity?: string | null;
  filled_size?: number | null;
  avg_fill_price?: number | null;
}

export interface TradesResponse {
  trades: TradeRow[];
  total: number;
  page: number;
  per_page: number;
}

export interface BalanceEntry {
  id: number;
  timestamp: number;
  event: string;
  balance: number;
}

export interface BalanceResponse {
  entries: BalanceEntry[];
}

export interface EquitySeriesResponse {
  baseline: BalanceEntry | null;
  points: BalanceEntry[];
}

export interface SignalRow {
  id: number;
  timestamp: number;
  strategy: string;
  direction: string;
  binance_price: number | null;
  chainlink_price: number | null;
  up_ask: number | null;
  down_ask: number | null;
  metadata: string | null;
  market_id?: string | null;
  execution_fidelity?: string | null;
}

export interface SignalsResponse {
  signals: SignalRow[];
}

export interface SignalGroupRow {
  id: string;
  strategy: string;
  direction: string;
  market_id: string | null;
  start_timestamp: number;
  end_timestamp: number;
  count: number;
  first_signal_id: number;
  last_signal_id: number;
  binance_price: number | null;
  chainlink_price: number | null;
  up_ask: number | null;
  down_ask: number | null;
  execution_fidelity: string | null;
}

export interface SignalGroupsResponse {
  groups: SignalGroupRow[];
  raw_rows_scanned: number;
  quiet_gap_ms: number;
}

export interface LogsResponse {
  lines: string[];
}

export interface StrategyStats {
  trades: number;
  wins: number;
  losses: number;
  win_rate: number;
  total_pnl: number;
}

export interface StatsResponse {
  by_strategy: Record<string, StrategyStats>;
}

export interface BotProcessStatus {
  active: boolean;
  pid: number | null;
  uptime_secs: number | null;
  control_available?: boolean;
}

export interface LiveSessionRow {
  id: number;
  started_at_ms: number;
  ended_at_ms: number | null;
  status: string;
  execution_mode: string;
  wallet_address: string | null;
  proxy_wallet: string | null;
  enabled_strategies_json: string;
  config_fingerprint: string;
  cash_cap_usd: number;
  details_json: string | null;
}

export interface LiveAccountSnapshotRow {
  id: number;
  session_id: number;
  timestamp_ms: number;
  cash_available: number;
  cash_reserved_for_orders: number;
  inventory_mark_value: number;
  redeemable_value: number;
  pending_redeem_value: number;
  total_equity: number;
  allowance_available: number | null;
  details_json: string | null;
}

export interface LiveOrderRow {
  id: number;
  session_id: number;
  intent_id: number;
  venue_order_id: string | null;
  client_order_id: string | null;
  market_id: string;
  token_id: string | null;
  side: string;
  order_type: string;
  status: string;
  status_reason: string | null;
  created_at_ms: number;
  acknowledged_at_ms: number | null;
  updated_at_ms: number;
  requested_price: number | null;
  limit_price: number | null;
  requested_size: number | null;
  accepted_size: number | null;
  details_json: string | null;
}

export interface LiveFillRow {
  id: number;
  session_id: number;
  intent_id: number | null;
  live_order_id: number | null;
  venue_trade_id: string | null;
  filled_at_ms: number;
  price: number;
  size: number;
  fee_amount: number | null;
  fee_rate: number | null;
  liquidity_side: string | null;
  tx_hash: string | null;
  status: string;
  details_json: string | null;
}

export interface LiveRedemptionRow {
  id: number;
  session_id: number;
  market_id: string;
  detected_redeemable_at_ms: number;
  submitted_at_ms: number | null;
  confirmed_at_ms: number | null;
  cash_credit_observed_at_ms: number | null;
  status: string;
  redeemable_value: number;
  tx_hash: string | null;
  details_json: string | null;
}

export interface LiveReconciliationRow {
  id: number;
  session_id: number;
  timestamp_ms: number;
  severity: string;
  event_type: string;
  local_value: number | null;
  remote_value: number | null;
  details_json: string | null;
}

export interface LiveSessionsResponse {
  sessions: LiveSessionRow[];
}

export interface LiveOrdersResponse {
  orders: LiveOrderRow[];
}

export interface LiveFillsResponse {
  fills: LiveFillRow[];
}

export interface LiveRedemptionsResponse {
  redemptions: LiveRedemptionRow[];
}

export interface LiveReconciliationResponse {
  events: LiveReconciliationRow[];
}

export interface TradingHealth {
  state: string;
  label: string;
  detail: string | null;
}

export interface TradingControlCapability {
  enabled: boolean;
  reason: string;
}

export interface TradingCapabilities {
  preflight: TradingControlCapability;
  arm: TradingControlCapability;
  disarm: TradingControlCapability;
  cancel_all: TradingControlCapability;
  stop_after_flat: TradingControlCapability;
  redeem: TradingControlCapability;
  kill_switch: TradingControlCapability;
}

export interface TradingAlert {
  severity: string;
  title: string;
  detail: string;
}

export interface ShadowSummary {
  balance: number;
  starting_balance: number;
  total_pnl: number;
  total_trades: number;
  wins: number;
  losses: number;
  win_rate: number;
  open_trades: number;
  uptime_hours: number;
  high_water_mark: number;
  max_drawdown_pct: number;
  live_session_status: string | null;
  last_tick_at: number | null;
  current_window: BotStatus["current_window"];
}

export interface RealAccountSummary {
  available_cash: number | null;
  reserved_cash: number | null;
  inventory_mark_value: number | null;
  redeemable_value: number | null;
  pending_redeem_value: number | null;
  total_equity: number | null;
  allowance_available: number | null;
  latest_snapshot_at_ms: number | null;
  session_id: number | null;
  session_status: string | null;
  session_started_at_ms: number | null;
  wallet_address: string | null;
  proxy_wallet: string | null;
  cash_cap_usd: number | null;
  enabled_strategies: string[];
  provider: string | null;
  user_stream_status: string | null;
  last_user_stream_connected_at_ms: number | null;
  last_user_stream_event_at_ms: number | null;
  last_account_refresh_at_ms: number | null;
  open_orders: number;
  pending_redemptions: number;
  critical_reconciliation_events: number;
}

export interface TradingSummary {
  runtime_mode: string;
  trading_state: string;
  process_state: string;
  venue_health: TradingHealth;
  account_health: TradingHealth;
  reconciliation_health: TradingHealth;
  shadow_summary: ShadowSummary;
  real_account_summary: RealAccountSummary;
  capabilities: TradingCapabilities;
  alerts: TradingAlert[];
}
