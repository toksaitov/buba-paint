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

  // Reconnect backoff
  RECONNECT_BASE_DELAY: 1_000,
  RECONNECT_MAX_DELAY: 30_000,

  // Database
  DB_PATH: env("DB_PATH", "./data/buba-paint.db"),

  // Strategy A: Latency Arb
  LATENCY_ARB_MOMENTUM_THRESHOLD: envFloat("LATENCY_ARB_MOMENTUM_THRESHOLD", 0.003),
  LATENCY_ARB_MAX_ASK: envFloat("LATENCY_ARB_MAX_ASK", 0.55),

  // Strategy B: Spread Capture
  SPREAD_CAPTURE_THRESHOLD: envFloat("SPREAD_CAPTURE_THRESHOLD", 0.98),

  // Momentum calculation
  MOMENTUM_WINDOW_MS: envInt("MOMENTUM_WINDOW_MS", 10_000),

  // Simulated trading
  POSITION_SIZE: envFloat("POSITION_SIZE", 100),
  MAX_OPEN_POSITIONS: envInt("MAX_OPEN_POSITIONS", 5),

  // Minimum time remaining in window to enter a trade (ms)
  MIN_WINDOW_TIME_MS: envInt("MIN_WINDOW_TIME_MS", 30_000),

  // Logging
  LOG_LEVEL: env("LOG_LEVEL", "info") as "debug" | "info" | "warn" | "error",

  // Gamma API search
  GAMMA_MARKET_LIMIT: 20,
} as const;
