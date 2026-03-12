function env(key: string, fallback: string): string {
  return process.env[key] ?? fallback;
}

function envInt(key: string, fallback: number): number {
  const v = process.env[key];
  return v ? parseInt(v, 10) : fallback;
}

function envFloat(key: string, fallback: number): number {
  const v = process.env[key];
  return v ? parseFloat(v) : fallback;
}

export const CONFIG = {
  // WebSocket URLs
  BINANCE_WS_URL: "wss://stream.binance.com:9443/ws/btcusdt@aggTrade",
  CLOB_WS_URL: "wss://ws-subscriptions-clob.polymarket.com/ws/market",
  RTDS_WS_URL: "wss://ws-live-data.polymarket.com",
  GAMMA_API_URL: "https://gamma-api.polymarket.com",

  // Polling / sampling intervals (ms)
  GAMMA_POLL_INTERVAL: envInt("GAMMA_POLL_INTERVAL", 60_000),
  TICK_INTERVAL: envInt("TICK_INTERVAL", 1_000),

  // WebSocket ping intervals (ms)
  CLOB_PING_INTERVAL: 10_000,
  RTDS_PING_INTERVAL: 5_000,

  // Chainlink staleness detection (ms) — if no update arrives within this
  // window, getPrice() returns null (falls back to Binance) and the feed
  // force-reconnects to get a fresh subscription.
  CHAINLINK_STALE_MS: envInt("CHAINLINK_STALE_MS", 30_000),

  // Reconnect backoff
  RECONNECT_BASE_DELAY: 1_000,
  RECONNECT_MAX_DELAY: 30_000,

  // Database
  DB_PATH: env("DB_PATH", "./data/buba-paint.db"),

  // Strategy A: Latency Arb
  LATENCY_ARB_MOMENTUM_THRESHOLD: envFloat("LATENCY_ARB_MOMENTUM_THRESHOLD", 0.0015),
  LATENCY_ARB_MAX_ASK: envFloat("LATENCY_ARB_MAX_ASK", 0.55),
  LATENCY_ARB_MIN_ASK: envFloat("LATENCY_ARB_MIN_ASK", 0.30),
  LATENCY_ARB_COOLDOWN_MS: envInt("LATENCY_ARB_COOLDOWN_MS", 60_000),

  // Strategy B: Spread Capture
  SPREAD_CAPTURE_THRESHOLD: envFloat("SPREAD_CAPTURE_THRESHOLD", 0.998),
  SPREAD_CAPTURE_MIN_ASK: envFloat("SPREAD_CAPTURE_MIN_ASK", 0.15),

  // Momentum calculation
  MOMENTUM_WINDOW_MS: envInt("MOMENTUM_WINDOW_MS", 30_000),

  // Bankroll management
  STARTING_BALANCE: envFloat("STARTING_BALANCE", 150),
  MAX_POSITION_FRACTION: envFloat("MAX_POSITION_FRACTION", 0.10),
  MIN_BALANCE_THRESHOLD: envFloat("MIN_BALANCE_THRESHOLD", 20),
  MAX_DRAWDOWN_PCT: envFloat("MAX_DRAWDOWN_PCT", 0.50),
  MAX_POSITION_USD_FRACTION: envFloat("MAX_POSITION_USD_FRACTION", 0.20),

  // Kelly criterion
  KELLY_FRACTION: envFloat("KELLY_FRACTION", 0.5),
  MIN_WIN_RATE_FOR_KELLY: envFloat("MIN_WIN_RATE_FOR_KELLY", 0.52),
  MIN_TRADES_FOR_KELLY: envInt("MIN_TRADES_FOR_KELLY", 20),
  MIN_KELLY_FLOOR: envFloat("MIN_KELLY_FLOOR", 0.03),
  MIN_BET_USD: envFloat("MIN_BET_USD", 5),

  // Position limits
  MAX_OPEN_POSITIONS: envInt("MAX_OPEN_POSITIONS", 5),

  // Minimum time remaining in window to enter a trade (ms)
  MIN_WINDOW_TIME_MS: envInt("MIN_WINDOW_TIME_MS", 90_000),

  // Circuit breaker — pause trading after consecutive losses
  CIRCUIT_BREAKER_LOSSES: envInt("CIRCUIT_BREAKER_LOSSES", 3),
  CIRCUIT_BREAKER_PAUSE_MS: envInt("CIRCUIT_BREAKER_PAUSE_MS", 900_000),

  // Peak drawdown pause — stop trading when balance drops ≥30% from all-time high
  PEAK_DD_PAUSE_PCT: envFloat("PEAK_DD_PAUSE_PCT", 0.30),
  PEAK_DD_PAUSE_MS: envInt("PEAK_DD_PAUSE_MS", 3_600_000),

  // Kelly rolling window — use recent N trades instead of lifetime
  KELLY_ROLLING_WINDOW: envInt("KELLY_ROLLING_WINDOW", 30),

  // Trend filter (experimental, off by default)
  TREND_FILTER_ENABLED: env("TREND_FILTER_ENABLED", "false") === "true",
  TREND_FILTER_THRESHOLD: envFloat("TREND_FILTER_THRESHOLD", 0.30),
  TREND_FILTER_WINDOW: envInt("TREND_FILTER_WINDOW", 10),

  // Regime detection (experimental, off by default)
  REGIME_DETECTION_ENABLED: env("REGIME_DETECTION_ENABLED", "false") === "true",

  // Logging
  LOG_LEVEL: env("LOG_LEVEL", "info") as "debug" | "info" | "warn" | "error",

  // Gamma API search
  GAMMA_MARKET_LIMIT: 20,
} as const;
