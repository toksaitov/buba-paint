export interface StrategyReadiness {
  strategy: string;
  readiness: string;
  enabled: boolean;
}

export interface LiveBudgetLimits {
  cash_cap_usd: number;
  max_single_order_usd: number;
  max_open_notional_usd: number;
  max_daily_loss_usd: number;
  max_session_drawdown_usd: number;
  min_required_cash_usd: number;
}

export interface LivePreflightRequest {
  execution_mode: string;
  clob_api_url: string;
  gamma_api_url: string;
  strategy_readiness: StrategyReadiness[];
  budget_limits: LiveBudgetLimits;
}

export type LiveCheckStatus = "ok" | "failed";

export interface LivePreflightResponse {
  ok: boolean;
  mode: string;
  wallet_address: string | null;
  proxy_wallet: string | null;
  geoblock_status: LiveCheckStatus;
  geoblock_country_code: string | null;
  auth_status: LiveCheckStatus;
  clock_status: LiveCheckStatus;
  allowance_status: LiveCheckStatus;
  user_stream_status: LiveCheckStatus;
  available_cash_usd: number | null;
  legal_order_min_usd: number | null;
  details_json: string | null;
  errors: string[];
}

export interface LiveAccountState {
  timestamp_ms: number;
  wallet_address: string | null;
  proxy_wallet: string | null;
  cash_available: number;
  cash_reserved_for_orders: number;
  inventory_mark_value: number;
  redeemable_value: number;
  pending_redeem_value: number;
  total_equity: number;
  allowance_available: number | null;
  details_json: string | null;
}

export interface LiveOrderIntentRequest {
  session_id: number;
  intent_id: number;
  market_id: string;
  token_id: string;
  side: string;
  order_type: string;
  limit_price: number;
  size: number;
  amount_usd?: number | null;
  client_order_id: string;
  details_json: string | null;
}

export interface LiveOrderIntentResponse {
  ok: boolean;
  venue_order_id: string | null;
  client_order_id: string;
  status: string;
  status_reason: string | null;
  accepted_size: number | null;
  details_json: string | null;
}

export interface LiveCancellationResponse {
  ok: boolean;
  cancelled: number;
  details_json: string | null;
}

export interface LiveRedemptionResponse {
  ok: boolean;
  submitted: number;
  details_json: string | null;
}
