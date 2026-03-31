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
}
