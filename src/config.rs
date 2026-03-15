use std::env;

fn env_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key).ok().map_or(default, |v| v == "true")
}

// Testable parsing helpers that accept an `Option<&str>` instead of reading
// the environment directly.  The public `env_*` functions above delegate to
// `env::var`, but these let us exercise the same parsing logic in tests
// without mutating process-global state (which requires `unsafe` under
// Rust 2024 edition, and this crate forbids `unsafe_code`).

#[cfg(test)]
fn resolve_str(raw: Option<&str>, default: &str) -> String {
    raw.map_or_else(|| default.to_string(), ToString::to_string)
}

#[cfg(test)]
fn resolve_f64(raw: Option<&str>, default: f64) -> f64 {
    raw.and_then(|v| v.parse::<f64>().ok()).unwrap_or(default)
}

#[cfg(test)]
fn resolve_u64(raw: Option<&str>, default: u64) -> u64 {
    raw.and_then(|v| v.parse::<u64>().ok()).unwrap_or(default)
}

#[cfg(test)]
fn resolve_bool(raw: Option<&str>, default: bool) -> bool {
    raw.map_or(default, |v| v == "true")
}

#[derive(Debug, Clone)]
pub struct Config {
    // WebSocket / API URLs
    pub binance_ws_url: String,
    pub clob_ws_url: String,
    pub rtds_ws_url: String,
    pub gamma_api_url: String,

    // Polling & tick intervals
    pub gamma_poll_interval: u64,
    pub tick_interval: u64,
    pub clob_ping_interval: u64,
    pub rtds_ping_interval: u64,
    pub chainlink_stale_ms: u64,

    // Reconnection
    pub reconnect_base_delay: u64,
    pub reconnect_max_delay: u64,

    // Database
    pub db_path: String,

    // Latency-arb strategy
    pub latency_arb_momentum_threshold: f64,
    pub latency_arb_max_ask: f64,
    pub latency_arb_min_ask: f64,
    pub latency_arb_cooldown_ms: u64,

    // Spread-capture strategy
    pub spread_capture_threshold: f64,
    pub spread_capture_min_ask: f64,

    // Momentum
    pub momentum_window_ms: u64,

    // Bankroll
    pub starting_balance: f64,
    pub max_position_fraction: f64,
    pub min_balance_threshold: f64,
    pub max_drawdown_pct: f64,
    pub max_position_usd_fraction: f64,

    // Kelly criterion
    pub kelly_fraction: f64,
    pub min_win_rate_for_kelly: f64,
    pub min_trades_for_kelly: u64,
    pub min_kelly_floor: f64,
    pub min_bet_usd: f64,
    pub kelly_rolling_window: u64,

    // Position limits
    pub max_open_positions: u64,
    pub min_window_time_ms: u64,

    // Circuit breaker
    pub circuit_breaker_losses: u64,
    pub circuit_breaker_pause_ms: u64,

    // Peak drawdown pause
    pub peak_dd_pause_pct: f64,
    pub peak_dd_pause_ms: u64,

    // Trend filter
    pub trend_filter_enabled: bool,
    pub trend_filter_threshold: f64,
    pub trend_filter_window: u64,

    // Regime detection
    pub regime_detection_enabled: bool,

    // Logging
    pub log_level: String,

    // Gamma market discovery
    pub gamma_market_limit: u64,
}

impl Config {
    /// Set a config parameter by name (used by the sweep engine).
    ///
    /// Returns `true` if the parameter was recognised, `false` otherwise.
    #[allow(clippy::cast_possible_wrap)]
    pub fn set_param(&mut self, name: &str, value: f64) -> bool {
        match name {
            "LATENCY_ARB_MOMENTUM_THRESHOLD" => self.latency_arb_momentum_threshold = value,
            "LATENCY_ARB_MAX_ASK" => self.latency_arb_max_ask = value,
            "LATENCY_ARB_MIN_ASK" => self.latency_arb_min_ask = value,
            "LATENCY_ARB_COOLDOWN_MS" => self.latency_arb_cooldown_ms = value as u64,
            "MAX_POSITION_FRACTION" => self.max_position_fraction = value,
            "SPREAD_CAPTURE_THRESHOLD" => self.spread_capture_threshold = value,
            "SPREAD_CAPTURE_MIN_ASK" => self.spread_capture_min_ask = value,
            "PEAK_DD_PAUSE_PCT" => self.peak_dd_pause_pct = value,
            "PEAK_DD_PAUSE_MS" => self.peak_dd_pause_ms = value as u64,
            "STARTING_BALANCE" => self.starting_balance = value,
            "MOMENTUM_WINDOW_MS" => self.momentum_window_ms = value as u64,
            "MAX_POSITION_USD_FRACTION" => self.max_position_usd_fraction = value,
            "MIN_BALANCE_THRESHOLD" => self.min_balance_threshold = value,
            "MAX_DRAWDOWN_PCT" => self.max_drawdown_pct = value,
            "KELLY_FRACTION" => self.kelly_fraction = value,
            "MIN_WIN_RATE_FOR_KELLY" => self.min_win_rate_for_kelly = value,
            "MIN_TRADES_FOR_KELLY" => self.min_trades_for_kelly = value as u64,
            "MIN_KELLY_FLOOR" => self.min_kelly_floor = value,
            "MIN_BET_USD" => self.min_bet_usd = value,
            "KELLY_ROLLING_WINDOW" => self.kelly_rolling_window = value as u64,
            "MAX_OPEN_POSITIONS" => self.max_open_positions = value as u64,
            "MIN_WINDOW_TIME_MS" => self.min_window_time_ms = value as u64,
            "CIRCUIT_BREAKER_LOSSES" => self.circuit_breaker_losses = value as u64,
            "CIRCUIT_BREAKER_PAUSE_MS" => self.circuit_breaker_pause_ms = value as u64,
            "TREND_FILTER_THRESHOLD" => self.trend_filter_threshold = value,
            "TREND_FILTER_WINDOW" => self.trend_filter_window = value as u64,
            _ => {
                eprintln!("Unknown sweep param: {name}");
                return false;
            }
        }
        true
    }

    /// Build config by reading environment variables (with `.env` file support).
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self {
            binance_ws_url: env_str(
                "BINANCE_WS_URL",
                "wss://stream.binance.com:9443/ws/btcusdt@aggTrade",
            ),
            clob_ws_url: env_str(
                "CLOB_WS_URL",
                "wss://ws-subscriptions-clob.polymarket.com/ws/market",
            ),
            rtds_ws_url: env_str("RTDS_WS_URL", "wss://ws-live-data.polymarket.com"),
            gamma_api_url: env_str("GAMMA_API_URL", "https://gamma-api.polymarket.com"),

            gamma_poll_interval: env_u64("GAMMA_POLL_INTERVAL", 60_000),
            tick_interval: env_u64("TICK_INTERVAL", 1_000),
            clob_ping_interval: 10_000,
            rtds_ping_interval: 5_000,
            chainlink_stale_ms: env_u64("CHAINLINK_STALE_MS", 30_000),

            reconnect_base_delay: 1_000,
            reconnect_max_delay: 30_000,

            db_path: env_str("DB_PATH", "./data/buba-paint.db"),

            latency_arb_momentum_threshold: env_f64("LATENCY_ARB_MOMENTUM_THRESHOLD", 0.0015),
            latency_arb_max_ask: env_f64("LATENCY_ARB_MAX_ASK", 0.55),
            latency_arb_min_ask: env_f64("LATENCY_ARB_MIN_ASK", 0.30),
            latency_arb_cooldown_ms: env_u64("LATENCY_ARB_COOLDOWN_MS", 60_000),

            spread_capture_threshold: env_f64("SPREAD_CAPTURE_THRESHOLD", 0.998),
            spread_capture_min_ask: env_f64("SPREAD_CAPTURE_MIN_ASK", 0.15),

            momentum_window_ms: env_u64("MOMENTUM_WINDOW_MS", 30_000),

            starting_balance: env_f64("STARTING_BALANCE", 150.0),
            max_position_fraction: env_f64("MAX_POSITION_FRACTION", 0.10),
            min_balance_threshold: env_f64("MIN_BALANCE_THRESHOLD", 20.0),
            max_drawdown_pct: env_f64("MAX_DRAWDOWN_PCT", 0.50),
            max_position_usd_fraction: env_f64("MAX_POSITION_USD_FRACTION", 0.20),

            kelly_fraction: env_f64("KELLY_FRACTION", 0.5),
            min_win_rate_for_kelly: env_f64("MIN_WIN_RATE_FOR_KELLY", 0.52),
            min_trades_for_kelly: env_u64("MIN_TRADES_FOR_KELLY", 20),
            min_kelly_floor: env_f64("MIN_KELLY_FLOOR", 0.03),
            min_bet_usd: env_f64("MIN_BET_USD", 5.0),
            kelly_rolling_window: env_u64("KELLY_ROLLING_WINDOW", 30),

            max_open_positions: env_u64("MAX_OPEN_POSITIONS", 5),
            min_window_time_ms: env_u64("MIN_WINDOW_TIME_MS", 90_000),

            circuit_breaker_losses: env_u64("CIRCUIT_BREAKER_LOSSES", 3),
            circuit_breaker_pause_ms: env_u64("CIRCUIT_BREAKER_PAUSE_MS", 900_000),

            peak_dd_pause_pct: env_f64("PEAK_DD_PAUSE_PCT", 0.30),
            peak_dd_pause_ms: env_u64("PEAK_DD_PAUSE_MS", 3_600_000),

            trend_filter_enabled: env_bool("TREND_FILTER_ENABLED", false),
            trend_filter_threshold: env_f64("TREND_FILTER_THRESHOLD", 0.30),
            trend_filter_window: env_u64("TREND_FILTER_WINDOW", 10),

            regime_detection_enabled: env_bool("REGIME_DETECTION_ENABLED", false),

            log_level: env_str("LOG_LEVEL", "info"),

            gamma_market_limit: 20,
        }
    }
}

impl Default for Config {
    /// Returns defaults without reading environment variables (useful for tests).
    fn default() -> Self {
        Self {
            binance_ws_url: "wss://stream.binance.com:9443/ws/btcusdt@aggTrade".to_string(),
            clob_ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/market".to_string(),
            rtds_ws_url: "wss://ws-live-data.polymarket.com".to_string(),
            gamma_api_url: "https://gamma-api.polymarket.com".to_string(),

            gamma_poll_interval: 60_000,
            tick_interval: 1_000,
            clob_ping_interval: 10_000,
            rtds_ping_interval: 5_000,
            chainlink_stale_ms: 30_000,

            reconnect_base_delay: 1_000,
            reconnect_max_delay: 30_000,

            db_path: "./data/buba-paint.db".to_string(),

            latency_arb_momentum_threshold: 0.0015,
            latency_arb_max_ask: 0.55,
            latency_arb_min_ask: 0.30,
            latency_arb_cooldown_ms: 60_000,

            spread_capture_threshold: 0.998,
            spread_capture_min_ask: 0.15,

            momentum_window_ms: 30_000,

            starting_balance: 150.0,
            max_position_fraction: 0.10,
            min_balance_threshold: 20.0,
            max_drawdown_pct: 0.50,
            max_position_usd_fraction: 0.20,

            kelly_fraction: 0.5,
            min_win_rate_for_kelly: 0.52,
            min_trades_for_kelly: 20,
            min_kelly_floor: 0.03,
            min_bet_usd: 5.0,
            kelly_rolling_window: 30,

            max_open_positions: 5,
            min_window_time_ms: 90_000,

            circuit_breaker_losses: 3,
            circuit_breaker_pause_ms: 900_000,

            peak_dd_pause_pct: 0.30,
            peak_dd_pause_ms: 3_600_000,

            trend_filter_enabled: false,
            trend_filter_threshold: 0.30,
            trend_filter_window: 10,

            regime_detection_enabled: false,

            log_level: "info".to_string(),

            gamma_market_limit: 20,
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
