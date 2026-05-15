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

export interface LiveControlAuditRow {
  id: number;
  timestamp_ms: number;
  actor: string;
  action: string;
  target: string | null;
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

export interface LiveControlAuditResponse {
  entries: LiveControlAuditRow[];
}

export type LiveControlAction =
  | "preflight"
  | "arm"
  | "disarm"
  | "stop_after_flat"
  | "kill_switch"
  | "cancel_all"
  | "redeem_all";

export interface LiveControlRequest {
  action: LiveControlAction;
  reason: string;
  confirmation?: string;
}

export interface LiveControlResponse {
  ok: boolean;
  command_id: number;
  action: string;
  status: string;
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

export interface TradingRiskSummary {
  halt_reason: string | null;
  halt_at_ms: number | null;
  high_water_mark: number | null;
  trough_equity: number | null;
  current_equity: number | null;
  daily_loss_usd: number | null;
  session_drawdown_usd: number | null;
  closeout_exported_at_ms: number | null;
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
  risk_summary?: TradingRiskSummary;
  capabilities: TradingCapabilities;
  alerts: TradingAlert[];
}

export interface RuntimeConfigLatencyArb {
  enabled: boolean;
  momentum_threshold: number;
  min_ask: number;
  max_ask: number;
  cooldown_ms: number;
  adaptive_window_ms: number;
  max_position_fraction: number | null;
}

export interface RuntimeConfigSpreadCapture {
  enabled: boolean;
  threshold: number;
  min_ask: number;
  max_leg_skew_ms: number;
  max_quote_churn_per_s: number;
  max_position_fraction: number | null;
}

export interface RuntimeConfigCalmPersistence {
  enabled: boolean;
  min_window_time_ms: number;
  max_window_time_ms: number;
  max_ask: number;
  min_abs_distance_bps: number;
  distance_vol_ratio_threshold: number;
  max_realized_vol_15s_bps: number;
  max_open_crosses_30s: number;
  max_quote_churn_per_s: number;
  min_alignment_fraction: number;
  max_fair_bias: number;
  min_expected_edge: number;
  max_position_fraction: number | null;
}

export interface RuntimeConfigRisk {
  starting_balance: number;
  max_position_fraction: number;
  min_bet_usd: number;
  max_drawdown_pct: number;
  live_session_cash_cap_usd: number;
  live_max_single_order_usd: number;
  live_max_open_notional_usd: number;
  live_max_daily_loss_usd: number;
  live_max_session_drawdown_usd: number;
  live_min_required_cash_usd: number;
}

export interface RuntimeConfigPendingSettlement {
  family_reserve_fraction: number;
  global_reserve_fraction: number;
  counts_as_open_position: boolean;
}

export interface RuntimeConfigFees {
  taker_fee_rate: number;
  taker_fee_exponent: number;
  sim_order_latency_ms: number;
}

export interface RuntimeConfigFeedFreshness {
  max_book_staleness_ms: number;
  max_signal_feed_age_ms: number;
  max_quote_age_ms: number;
  clob_no_message_reconnect_ms: number;
  binance_no_message_reconnect_ms: number;
}

export interface RuntimeConfigWorkerBudgets {
  feed_event_writer_queue_capacity: number;
  feed_event_writer_batch_size: number;
  feed_event_writer_flush_ms: number;
  feed_event_writer_max_lag_ms: number;
  clob_replay_block_max_rows: number;
  clob_replay_block_max_ms: number;
  clob_replay_block_zstd_level: number;
  live_decision_queue_capacity: number;
  live_decision_output_queue_capacity: number;
  live_runtime_persistence_queue_capacity: number;
  live_submission_queue_capacity: number;
  max_live_decision_age_ms: number;
  worker_shutdown_timeout_ms: number;
}

export interface RuntimeConfigSnapshot {
  execution_mode: string;
  runtime_name: string | null;
  process_start_time_ms: number;
  db_path_label: string;
  config_fingerprint: string;
  git_sha: string;
  package_version: string;
  feed_event_storage_profile: string;
  live_runtime_max_db_bytes: number;
  latency_arb: RuntimeConfigLatencyArb;
  spread_capture: RuntimeConfigSpreadCapture;
  calm_persistence: RuntimeConfigCalmPersistence;
  risk: RuntimeConfigRisk;
  pending_settlement: RuntimeConfigPendingSettlement;
  fees: RuntimeConfigFees;
  feed_freshness: RuntimeConfigFeedFreshness;
  worker_budgets: RuntimeConfigWorkerBudgets;
}

export interface RuntimeConfigResponse {
  snapshot: RuntimeConfigSnapshot | null;
  snapshot_recorded_at_ms: number | null;
  uptime_secs: number | null;
}

export interface MachineHostIdentity {
  hostname: string;
  os_name: string;
  os_version: string;
  kernel_version: string;
  cpu_count: number;
  total_ram_bytes: number;
}

export interface MachineSample {
  sampled_at_ms: number;
  cpu_percent: number;
  per_core_cpu: number[];
  load_one: number | null;
  load_five: number | null;
  load_fifteen: number | null;
  mem_used_bytes: number;
  mem_total_bytes: number;
  mem_available_bytes: number;
  swap_used_bytes: number;
  swap_total_bytes: number;
  disk_used_bytes: number;
  disk_total_bytes: number;
  disk_mount: string;
}

export interface MachineRuntimeDbFiles {
  db_path: string;
  db_bytes: number | null;
  wal_bytes: number | null;
  shm_bytes: number | null;
}

export interface MachineSamplerHealth {
  sample_interval_ms: number;
  samples_collected: number;
  last_error: string | null;
}

export interface MachineResponse {
  host: MachineHostIdentity;
  agent_started_at_ms: number;
  current: MachineSample | null;
  history: MachineSample[];
  runtime_db: MachineRuntimeDbFiles;
  sampler: MachineSamplerHealth;
}
