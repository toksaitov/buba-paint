use std::env;

use anyhow::bail;
use reqwest::Url;

/// Env str.
fn env_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Env f64.
fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

/// Env u64.
fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

/// Env bool.
fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(raw) => parse_boolish(&raw).unwrap_or_else(|| {
            panic!("{key} must be one of true/false/1/0/yes/no/on/off, got {raw}")
        }),
        Err(_) => default,
    }
}

/// Parse one bool-like string accepted by env vars and CLI overrides.
pub(crate) fn parse_boolish(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Validate one configured URL that must be absolute in live-readiness modes.
fn validate_absolute_url(name: &str, raw: &str) -> anyhow::Result<()> {
    let parsed = Url::parse(raw).map_err(|error| {
        anyhow::anyhow!("{name} must be a valid absolute URL, got {raw:?}: {error}")
    })?;
    if parsed.scheme().is_empty() || parsed.host_str().is_none() {
        bail!("{name} must be a valid absolute URL, got {raw:?}");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedEventStorageProfile {
    Compact,
    ReplayGrade,
    FullDebug,
}

impl FeedEventStorageProfile {
    /// Return the persisted environment label for this storage profile.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::ReplayGrade => "replay_grade",
            Self::FullDebug => "full_debug",
        }
    }

    /// Parse one environment override into a supported storage profile.
    #[must_use]
    pub fn from_env_value(raw: Option<&str>) -> Self {
        match raw {
            Some("full_debug") => Self::FullDebug,
            Some("compact") => Self::Compact,
            _ => Self::ReplayGrade,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktestSettlementMode {
    Immediate,
    ObservedMarketResolution,
}

impl BacktestSettlementMode {
    /// Return the persisted environment label for this settlement mode.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::ObservedMarketResolution => "observed_market_resolution",
        }
    }

    /// Parse one environment override into a supported settlement mode.
    #[must_use]
    pub fn from_env_value(raw: Option<&str>) -> Self {
        match raw {
            Some("observed_market_resolution") => Self::ObservedMarketResolution,
            _ => Self::Immediate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Paper,
    LiveReadonly,
    LiveTrading,
}

impl ExecutionMode {
    /// Return the persisted environment label for this execution mode.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::LiveReadonly => "live_readonly",
            Self::LiveTrading => "live_trading",
        }
    }

    /// Parse one environment override into a supported execution mode.
    #[must_use]
    pub fn from_env_value(raw: Option<&str>) -> Self {
        match raw {
            Some("live_readonly") => Self::LiveReadonly,
            Some("live_trading") => Self::LiveTrading,
            _ => Self::Paper,
        }
    }

    /// Return whether this mode needs the live sidecar boundary.
    #[must_use]
    pub fn uses_live_sidecar(self) -> bool {
        matches!(self, Self::LiveReadonly | Self::LiveTrading)
    }
}

/// Named pending-settlement reserve modes used by live trading and exact-run parity replays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingSettlementReserveMode {
    Compatibility,
    Conservative,
    Risky,
    Custom,
}

impl PendingSettlementReserveMode {
    /// Return the stable label for this reserve mode.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compatibility => "compatibility",
            Self::Conservative => "conservative",
            Self::Risky => "risky",
            Self::Custom => "custom",
        }
    }
}

/// One resolved pending-settlement reserve policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingSettlementPolicy {
    pub mode: PendingSettlementReserveMode,
    pub family_reserve_fraction: f64,
    pub global_reserve_fraction: f64,
    pub counts_as_open_position: bool,
}

impl PendingSettlementPolicy {
    /// Build the compatibility policy.
    #[must_use]
    pub const fn compatibility() -> Self {
        Self {
            mode: PendingSettlementReserveMode::Compatibility,
            family_reserve_fraction: 1.0,
            global_reserve_fraction: 1.0,
            counts_as_open_position: true,
        }
    }

    /// Build the conservative policy.
    #[must_use]
    pub const fn conservative() -> Self {
        Self {
            mode: PendingSettlementReserveMode::Conservative,
            family_reserve_fraction: 0.0,
            global_reserve_fraction: 1.0,
            counts_as_open_position: false,
        }
    }

    /// Build the risky policy.
    #[must_use]
    pub const fn risky() -> Self {
        Self {
            mode: PendingSettlementReserveMode::Risky,
            family_reserve_fraction: 0.0,
            global_reserve_fraction: 0.25,
            counts_as_open_position: false,
        }
    }

    /// Classify one raw triple into a named or custom policy.
    #[must_use]
    pub fn classify(
        family_reserve_fraction: f64,
        global_reserve_fraction: f64,
        counts_as_open_position: bool,
    ) -> Self {
        let mode = if approx_f64(family_reserve_fraction, 1.0)
            && approx_f64(global_reserve_fraction, 1.0)
            && counts_as_open_position
        {
            PendingSettlementReserveMode::Compatibility
        } else if approx_f64(family_reserve_fraction, 0.0)
            && approx_f64(global_reserve_fraction, 1.0)
            && !counts_as_open_position
        {
            PendingSettlementReserveMode::Conservative
        } else if approx_f64(family_reserve_fraction, 0.0)
            && approx_f64(global_reserve_fraction, 0.25)
            && !counts_as_open_position
        {
            PendingSettlementReserveMode::Risky
        } else {
            PendingSettlementReserveMode::Custom
        };

        Self {
            mode,
            family_reserve_fraction,
            global_reserve_fraction,
            counts_as_open_position,
        }
    }

    /// Validate that the configured reserve fractions are sane.
    pub fn validate(self) -> anyhow::Result<Self> {
        if !(0.0..=1.0).contains(&self.family_reserve_fraction) {
            bail!(
                "PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION must be within [0.0, 1.0], got {}",
                self.family_reserve_fraction
            );
        }
        if !(0.0..=1.0).contains(&self.global_reserve_fraction) {
            bail!(
                "PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION must be within [0.0, 1.0], got {}",
                self.global_reserve_fraction
            );
        }
        Ok(self)
    }
}

/// Compare two floating-point config values with a tiny tolerance.
fn approx_f64(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9
}

/// Build the default Binance stream URL from the configured stream names.
fn default_binance_ws_url(
    trade_stream: &str,
    book_ticker_stream: &str,
    depth_stream: &str,
    use_microseconds: bool,
) -> String {
    let base = format!(
        "wss://stream.binance.com:9443/stream?streams={trade_stream}/{book_ticker_stream}/{depth_stream}"
    );
    if use_microseconds {
        format!("{base}&timeUnit=MICROSECOND")
    } else {
        base
    }
}

/// Resolves str.
#[cfg(test)]
fn resolve_str(raw: Option<&str>, default: &str) -> String {
    raw.map_or_else(|| default.to_string(), ToString::to_string)
}

/// Resolves f64.
#[cfg(test)]
fn resolve_f64(raw: Option<&str>, default: f64) -> f64 {
    raw.and_then(|v| v.parse::<f64>().ok()).unwrap_or(default)
}

/// Resolves u64.
#[cfg(test)]
fn resolve_u64(raw: Option<&str>, default: u64) -> u64 {
    raw.and_then(|v| v.parse::<u64>().ok()).unwrap_or(default)
}

/// Resolves bool.
#[cfg(test)]
fn resolve_bool(raw: Option<&str>, default: bool) -> bool {
    raw.and_then(parse_boolish).unwrap_or(default)
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    pub binance_ws_url: String,
    pub binance_trade_stream: String,
    pub binance_book_ticker_stream: String,
    pub binance_depth_stream: String,
    pub clob_ws_url: String,
    pub clob_api_url: String,
    pub rtds_ws_url: String,
    pub gamma_api_url: String,

    pub gamma_poll_interval: u64,
    pub tick_interval: u64,
    pub tick_data_logging_enabled: bool,
    pub clob_ping_interval: u64,
    pub rtds_ping_interval: u64,
    pub chainlink_stale_ms: u64,
    pub websocket_connect_timeout_ms: u64,
    pub binance_no_message_reconnect_ms: u64,
    pub clob_no_message_reconnect_ms: u64,

    pub reconnect_base_delay: u64,
    pub reconnect_max_delay: u64,
    pub reconnect_min_stable_ms: u64,
    pub reconnect_max_failures: u32,
    pub reconnect_pause_ms: u64,

    pub db_path: String,

    pub latency_arb_momentum_threshold: f64,
    pub latency_arb_max_ask: f64,
    pub latency_arb_min_ask: f64,
    pub latency_arb_cooldown_ms: u64,
    pub latency_arb_adaptive_window_ms: u64,
    pub latency_arb_enabled: bool,
    pub latency_arb_max_position_fraction: Option<f64>,

    pub spread_capture_threshold: f64,
    pub spread_capture_min_ask: f64,
    pub spread_capture_max_leg_skew_ms: u64,
    pub spread_capture_max_quote_churn_per_s: f64,
    pub spread_capture_enabled: bool,
    pub spread_capture_max_position_fraction: Option<f64>,

    pub calm_persistence_enabled: bool,
    pub calm_persistence_min_window_time_ms: u64,
    pub calm_persistence_max_window_time_ms: u64,
    pub calm_persistence_max_ask: f64,
    pub calm_persistence_min_abs_distance_bps: f64,
    pub calm_persistence_distance_vol_ratio_threshold: f64,
    pub calm_persistence_max_realized_vol_15s_bps: f64,
    pub calm_persistence_max_open_crosses_30s: u32,
    pub calm_persistence_max_quote_churn_per_s: f64,
    pub calm_persistence_min_alignment_fraction: f64,
    pub calm_persistence_max_fair_bias: f64,
    pub calm_persistence_min_expected_edge: f64,
    pub calm_persistence_max_position_fraction: Option<f64>,

    pub momentum_window_ms: u64,

    pub starting_balance: f64,
    pub max_position_fraction: f64,
    pub min_balance_threshold: f64,
    pub max_drawdown_pct: f64,
    pub max_position_usd_fraction: f64,

    pub kelly_fraction: f64,
    pub min_win_rate_for_kelly: f64,
    pub min_trades_for_kelly: u64,
    pub min_kelly_floor: f64,
    pub min_bet_usd: f64,
    pub kelly_rolling_window: u64,

    pub max_open_positions: u64,
    pub min_window_time_ms: u64,

    pub circuit_breaker_losses: u64,
    pub circuit_breaker_pause_ms: u64,

    pub peak_dd_pause_pct: f64,
    pub peak_dd_pause_ms: u64,
    pub dd_pause_recovery_pct: f64,

    pub trend_filter_enabled: bool,
    pub trend_filter_threshold: f64,
    pub trend_filter_window: u64,
    pub trend_filter_per_strategy: bool,

    pub regime_detection_enabled: bool,

    pub log_level: String,

    pub gamma_market_limit: u64,

    pub max_position_usd: f64,

    pub taker_fee_rate: f64,
    pub taker_fee_exponent: u32,
    pub taker_fee_override_explicit: bool,

    pub execution_mode: ExecutionMode,
    pub live_sidecar_url: String,
    pub live_sidecar_request_timeout_ms: u64,
    pub live_sidecar_emergency_timeout_ms: u64,
    pub live_session_cash_cap_usd: f64,
    pub live_max_single_order_usd: f64,
    pub live_max_open_notional_usd: f64,
    pub live_max_daily_loss_usd: f64,
    pub live_max_session_drawdown_usd: f64,
    pub live_min_required_cash_usd: f64,
    pub live_expected_signature_type: Option<u32>,
    pub live_allow_deposit_wallet: bool,
    pub live_expected_egress_ip: Option<String>,
    pub enforce_open_exposure_caps: bool,
    pub live_onchain_reconcile: bool,
    pub live_onchain_reconcile_grace_ms: u64,
    pub live_onchain_reconcile_retry_interval_ms: u64,
    pub live_onchain_reconcile_max_attempts: u32,
    pub live_dry_run: bool,
    pub live_max_session_orders: u32,
    pub live_max_session_fills: u32,
    pub feed_event_storage_profile: FeedEventStorageProfile,
    pub feed_event_writer_queue_capacity: usize,
    pub feed_event_writer_batch_size: usize,
    pub feed_event_writer_flush_ms: u64,
    pub feed_event_writer_max_lag_ms: u64,
    pub clob_replay_block_max_rows: usize,
    pub clob_replay_block_max_ms: u64,
    pub clob_replay_block_zstd_level: i32,
    pub live_runtime_max_db_bytes: u64,
    pub live_feed_batch_max_messages: usize,
    pub live_decision_queue_capacity: usize,
    pub live_decision_output_queue_capacity: usize,
    pub live_runtime_persistence_queue_capacity: usize,
    pub live_submission_queue_capacity: usize,
    pub max_live_decision_age_ms: u64,
    pub worker_shutdown_timeout_ms: u64,
    pub sim_order_latency_ms: u64,
    pub max_book_staleness_ms: u64,
    pub max_signal_feed_age_ms: u64,
    pub max_quote_age_ms: u64,

    pub resolution_poll_retries: u32,
    pub resolution_initial_delay_ms: u64,
    pub resolution_poll_delay_ms: u64,
    pub pending_settlement_family_reserve_fraction: f64,
    pub pending_settlement_global_reserve_fraction: f64,
    pub pending_settlement_counts_as_open_position: bool,
    pub backtest_settlement_mode: BacktestSettlementMode,
}

impl Config {
    /// Return the stable names of every enabled strategy family.
    #[must_use]
    pub fn enabled_strategy_names(&self) -> Vec<&'static str> {
        let mut strategies = Vec::new();
        if self.latency_arb_enabled {
            strategies.push("latency-arb");
        }
        if self.spread_capture_enabled {
            strategies.push("spread-capture");
        }
        if self.calm_persistence_enabled {
            strategies.push("calm-persistence");
        }
        strategies
    }

    /// Return the resolved pending-settlement policy after validation.
    pub fn pending_settlement_policy(&self) -> anyhow::Result<PendingSettlementPolicy> {
        PendingSettlementPolicy::classify(
            self.pending_settlement_family_reserve_fraction,
            self.pending_settlement_global_reserve_fraction,
            self.pending_settlement_counts_as_open_position,
        )
        .validate()
    }

    /// Return the resolved pending-settlement policy without re-validating it.
    #[must_use]
    pub fn pending_settlement_policy_unchecked(&self) -> PendingSettlementPolicy {
        PendingSettlementPolicy::classify(
            self.pending_settlement_family_reserve_fraction,
            self.pending_settlement_global_reserve_fraction,
            self.pending_settlement_counts_as_open_position,
        )
    }

    /// Return whether this config targets one live venue mode.
    #[must_use]
    pub fn is_live_execution(&self) -> bool {
        self.execution_mode.uses_live_sidecar()
    }

    /// Return the hard open-exposure ceiling in USD, or None when the caps are not enforced.
    ///
    /// The ceiling is the tighter of the open-notional and session-cash caps and is applied to
    /// total committed open exposure at reservation time. It is active in live execution modes and,
    /// for live-fidelity backtests, whenever `enforce_open_exposure_caps` is set; otherwise it is
    /// None so research backtests size exactly as before.
    #[must_use]
    pub fn open_exposure_ceiling_usd(&self) -> Option<f64> {
        if self.is_live_execution() || self.enforce_open_exposure_caps {
            Some(
                self.live_max_open_notional_usd
                    .min(self.live_session_cash_cap_usd),
            )
        } else {
            None
        }
    }

    /// Return whether post-fill on-chain CTF reconciliation should run.
    ///
    /// Verification only ever runs in live execution modes. `live_onchain_reconcile`
    /// is an operator kill-switch (default on) to disable it without changing mode,
    /// for example during a known RPC outage. It can never enable verification in
    /// paper or research backtests, which never reach a real fill.
    #[must_use]
    pub fn live_onchain_reconcile_enabled(&self) -> bool {
        self.is_live_execution() && self.live_onchain_reconcile
    }

    /// Validate config invariants that should fail fast at startup.
    pub fn validate(&self) -> anyhow::Result<()> {
        let _ = self.pending_settlement_policy()?;
        if self.websocket_connect_timeout_ms == 0 {
            bail!("WEBSOCKET_CONNECT_TIMEOUT_MS must be > 0");
        }
        if self.binance_no_message_reconnect_ms == 0 {
            bail!("BINANCE_NO_MESSAGE_RECONNECT_MS must be > 0");
        }
        if self.clob_no_message_reconnect_ms == 0 {
            bail!("CLOB_NO_MESSAGE_RECONNECT_MS must be > 0");
        }
        if self.open_exposure_ceiling_usd().is_some() {
            if !(self.live_max_open_notional_usd.is_finite()
                && self.live_max_open_notional_usd > 0.0)
            {
                bail!(
                    "LIVE_MAX_OPEN_NOTIONAL_USD must be a positive finite number when the open-exposure ceiling is enforced"
                );
            }
            if !(self.live_session_cash_cap_usd.is_finite() && self.live_session_cash_cap_usd > 0.0)
            {
                bail!(
                    "LIVE_SESSION_CASH_CAP_USD must be a positive finite number when the open-exposure ceiling is enforced"
                );
            }
        }
        if self.is_live_execution() {
            if self.live_session_cash_cap_usd <= 0.0 {
                bail!("LIVE_SESSION_CASH_CAP_USD must be > 0 for live execution modes");
            }
            if self.live_max_single_order_usd <= 0.0 {
                bail!("LIVE_MAX_SINGLE_ORDER_USD must be > 0 for live execution modes");
            }
            if self.live_max_open_notional_usd <= 0.0 {
                bail!("LIVE_MAX_OPEN_NOTIONAL_USD must be > 0 for live execution modes");
            }
            if self.live_max_daily_loss_usd <= 0.0 {
                bail!("LIVE_MAX_DAILY_LOSS_USD must be > 0 for live execution modes");
            }
            if self.live_max_session_drawdown_usd <= 0.0 {
                bail!("LIVE_MAX_SESSION_DRAWDOWN_USD must be > 0 for live execution modes");
            }
            if self.live_min_required_cash_usd <= 0.0 {
                bail!("LIVE_MIN_REQUIRED_CASH_USD must be > 0 for live execution modes");
            }
            if self.live_min_required_cash_usd > self.live_session_cash_cap_usd {
                bail!("LIVE_MIN_REQUIRED_CASH_USD cannot exceed LIVE_SESSION_CASH_CAP_USD");
            }
            if self.live_max_single_order_usd > self.live_max_open_notional_usd {
                bail!("LIVE_MAX_SINGLE_ORDER_USD cannot exceed LIVE_MAX_OPEN_NOTIONAL_USD");
            }
            if self.live_sidecar_url.trim().is_empty() {
                bail!("LIVE_SIDECAR_URL must be set for live execution modes");
            }
            if self.live_sidecar_request_timeout_ms == 0 {
                bail!("LIVE_SIDECAR_REQUEST_TIMEOUT_MS must be > 0 for live execution modes");
            }
            if self.live_sidecar_emergency_timeout_ms == 0 {
                bail!("LIVE_SIDECAR_EMERGENCY_TIMEOUT_MS must be > 0 for live execution modes");
            }
            if let Some(expected) = self.live_expected_signature_type
                && expected > 3
            {
                bail!("LIVE_EXPECTED_SIGNATURE_TYPE must be one of 0, 1, 2, 3");
            }
            if let Some(ip) = &self.live_expected_egress_ip
                && ip.parse::<std::net::IpAddr>().is_err()
            {
                bail!("LIVE_EXPECTED_EGRESS_IP must be a valid IP address, got {ip:?}");
            }
            validate_absolute_url("LIVE_SIDECAR_URL", &self.live_sidecar_url)?;
            validate_absolute_url("CLOB_API_URL", &self.clob_api_url)?;
            validate_absolute_url("GAMMA_API_URL", &self.gamma_api_url)?;
        }
        Ok(())
    }

    /// Set a config parameter by name (used by the sweep engine).
    ///
    /// Returns `true` if the parameter was recognised, `false` otherwise.
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::too_many_lines)]
    pub fn set_param(&mut self, name: &str, value: f64) -> bool {
        match name {
            "LATENCY_ARB_MOMENTUM_THRESHOLD" => self.latency_arb_momentum_threshold = value,
            "LATENCY_ARB_MAX_ASK" => self.latency_arb_max_ask = value,
            "LATENCY_ARB_MIN_ASK" => self.latency_arb_min_ask = value,
            "LATENCY_ARB_COOLDOWN_MS" => self.latency_arb_cooldown_ms = value as u64,
            "LATENCY_ARB_ADAPTIVE_WINDOW_MS" => self.latency_arb_adaptive_window_ms = value as u64,
            "LATENCY_ARB_ENABLED" => self.latency_arb_enabled = value != 0.0,
            "LATENCY_ARB_MAX_POSITION_FRACTION" => {
                self.latency_arb_max_position_fraction = Some(value);
            }
            "MAX_POSITION_FRACTION" => self.max_position_fraction = value,
            "MAX_POSITION_USD" => self.max_position_usd = value,
            "SPREAD_CAPTURE_THRESHOLD" => self.spread_capture_threshold = value,
            "SPREAD_CAPTURE_MIN_ASK" => self.spread_capture_min_ask = value,
            "SPREAD_CAPTURE_MAX_LEG_SKEW_MS" => self.spread_capture_max_leg_skew_ms = value as u64,
            "SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S" => {
                self.spread_capture_max_quote_churn_per_s = value;
            }
            "SPREAD_CAPTURE_ENABLED" => self.spread_capture_enabled = value != 0.0,
            "SPREAD_CAPTURE_MAX_POSITION_FRACTION" => {
                self.spread_capture_max_position_fraction = Some(value);
            }
            "CALM_PERSISTENCE_ENABLED" => self.calm_persistence_enabled = value != 0.0,
            "CALM_PERSISTENCE_MIN_WINDOW_TIME_MS" => {
                self.calm_persistence_min_window_time_ms = value as u64;
            }
            "CALM_PERSISTENCE_MAX_WINDOW_TIME_MS" => {
                self.calm_persistence_max_window_time_ms = value as u64;
            }
            "CALM_PERSISTENCE_MAX_ASK" => self.calm_persistence_max_ask = value,
            "CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS" => {
                self.calm_persistence_min_abs_distance_bps = value;
            }
            "CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD" => {
                self.calm_persistence_distance_vol_ratio_threshold = value;
            }
            "CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS" => {
                self.calm_persistence_max_realized_vol_15s_bps = value;
            }
            "CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S" => {
                self.calm_persistence_max_open_crosses_30s = value as u32;
            }
            "CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S" => {
                self.calm_persistence_max_quote_churn_per_s = value;
            }
            "CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION" => {
                self.calm_persistence_min_alignment_fraction = value;
            }
            "CALM_PERSISTENCE_MAX_FAIR_BIAS" => {
                self.calm_persistence_max_fair_bias = value;
            }
            "CALM_PERSISTENCE_MIN_EXPECTED_EDGE" => {
                self.calm_persistence_min_expected_edge = value;
            }
            "CALM_PERSISTENCE_MAX_POSITION_FRACTION" => {
                self.calm_persistence_max_position_fraction = Some(value);
            }
            "PEAK_DD_PAUSE_PCT" => self.peak_dd_pause_pct = value,
            "PEAK_DD_PAUSE_MS" => self.peak_dd_pause_ms = value as u64,
            "DD_PAUSE_RECOVERY_PCT" => self.dd_pause_recovery_pct = value,
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
            "RECONNECT_MIN_STABLE_MS" => self.reconnect_min_stable_ms = value as u64,
            "RECONNECT_MAX_FAILURES" => self.reconnect_max_failures = value as u32,
            "RECONNECT_PAUSE_MS" => self.reconnect_pause_ms = value as u64,
            "WEBSOCKET_CONNECT_TIMEOUT_MS" => self.websocket_connect_timeout_ms = value as u64,
            "BINANCE_NO_MESSAGE_RECONNECT_MS" => {
                self.binance_no_message_reconnect_ms = value as u64;
            }
            "CLOB_NO_MESSAGE_RECONNECT_MS" => {
                self.clob_no_message_reconnect_ms = value as u64;
            }
            "TREND_FILTER_THRESHOLD" => self.trend_filter_threshold = value,
            "TREND_FILTER_WINDOW" => self.trend_filter_window = value as u64,
            "TREND_FILTER_PER_STRATEGY" => self.trend_filter_per_strategy = value != 0.0,
            "REGIME_DETECTION_ENABLED" => self.regime_detection_enabled = value != 0.0,
            "RESOLUTION_POLL_RETRIES" => self.resolution_poll_retries = value as u32,
            "RESOLUTION_INITIAL_DELAY_MS" => self.resolution_initial_delay_ms = value as u64,
            "RESOLUTION_POLL_DELAY_MS" => self.resolution_poll_delay_ms = value as u64,
            "PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION" => {
                self.pending_settlement_family_reserve_fraction = value;
            }
            "PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION" => {
                self.pending_settlement_global_reserve_fraction = value;
            }
            "PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION" => {
                self.pending_settlement_counts_as_open_position = value != 0.0;
            }
            "TAKER_FEE_RATE" => {
                self.taker_fee_rate = value;
                self.taker_fee_override_explicit = true;
            }
            "TAKER_FEE_EXPONENT" => {
                self.taker_fee_exponent = value as u32;
                self.taker_fee_override_explicit = true;
            }
            "SIM_ORDER_LATENCY_MS" => self.sim_order_latency_ms = value as u64,
            "MAX_BOOK_STALENESS_MS" => self.max_book_staleness_ms = value as u64,
            "MAX_SIGNAL_FEED_AGE_MS" => self.max_signal_feed_age_ms = value as u64,
            "MAX_QUOTE_AGE_MS" => self.max_quote_age_ms = value as u64,
            "CLOB_REPLAY_BLOCK_MAX_ROWS" => self.clob_replay_block_max_rows = value as usize,
            "CLOB_REPLAY_BLOCK_MAX_MS" => self.clob_replay_block_max_ms = value as u64,
            "CLOB_REPLAY_BLOCK_ZSTD_LEVEL" => {
                self.clob_replay_block_zstd_level = value as i32;
            }
            _ => {
                eprintln!("Unknown sweep param: {name}");
                return false;
            }
        }
        true
    }

    /// Set one boolean config parameter from a resolved bool value.
    pub fn set_bool_param(&mut self, name: &str, value: bool) -> bool {
        match name {
            "LATENCY_ARB_ENABLED" => self.latency_arb_enabled = value,
            "SPREAD_CAPTURE_ENABLED" => self.spread_capture_enabled = value,
            "CALM_PERSISTENCE_ENABLED" => self.calm_persistence_enabled = value,
            "TREND_FILTER_ENABLED" => self.trend_filter_enabled = value,
            "TREND_FILTER_PER_STRATEGY" => self.trend_filter_per_strategy = value,
            "REGIME_DETECTION_ENABLED" => self.regime_detection_enabled = value,
            "PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION" => {
                self.pending_settlement_counts_as_open_position = value;
            }
            _ => return false,
        }
        true
    }

    /// Set one string-like config parameter used by CLI overrides.
    pub fn set_string_param(&mut self, name: &str, value: &str) -> bool {
        match name {
            "EXECUTION_MODE" => {
                self.execution_mode = ExecutionMode::from_env_value(Some(value));
            }
            "LIVE_SIDECAR_URL" => self.live_sidecar_url = value.to_string(),
            "CLOB_API_URL" => self.clob_api_url = value.to_string(),
            "GAMMA_API_URL" => self.gamma_api_url = value.to_string(),
            "DB_PATH" => self.db_path = value.to_string(),
            "LOG_LEVEL" => self.log_level = value.to_string(),
            "FEED_EVENT_STORAGE_PROFILE" => {
                self.feed_event_storage_profile =
                    FeedEventStorageProfile::from_env_value(Some(value));
            }
            _ => return false,
        }
        true
    }

    /// Build config by reading environment variables (with `.env` file support).
    #[allow(clippy::too_many_lines)]
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        let taker_fee_rate_override = env::var("TAKER_FEE_RATE").ok();
        let taker_fee_exponent_override = env::var("TAKER_FEE_EXPONENT").ok();
        let trade_stream = env_str("BINANCE_TRADE_STREAM", "btcusdt@aggTrade");
        let book_ticker_stream = env_str("BINANCE_BOOK_TICKER_STREAM", "btcusdt@bookTicker");
        let depth_stream = env_str("BINANCE_DEPTH_STREAM", "btcusdt@depth5@100ms");
        let request_microseconds = env_bool("BINANCE_REQUEST_MICROSECONDS", true);
        let legacy_binance_ws_url = env::var("BINANCE_WS_URL").ok();
        let taker_fee_override_explicit =
            taker_fee_rate_override.is_some() || taker_fee_exponent_override.is_some();

        Self {
            binance_ws_url: legacy_binance_ws_url.unwrap_or_else(|| {
                default_binance_ws_url(
                    &trade_stream,
                    &book_ticker_stream,
                    &depth_stream,
                    request_microseconds,
                )
            }),
            binance_trade_stream: trade_stream,
            binance_book_ticker_stream: book_ticker_stream,
            binance_depth_stream: depth_stream,
            clob_ws_url: env_str(
                "CLOB_WS_URL",
                "wss://ws-subscriptions-clob.polymarket.com/ws/market",
            ),
            clob_api_url: env_str("CLOB_API_URL", "https://clob.polymarket.com"),
            rtds_ws_url: env_str("RTDS_WS_URL", "wss://ws-live-data.polymarket.com"),
            gamma_api_url: env_str("GAMMA_API_URL", "https://gamma-api.polymarket.com"),

            gamma_poll_interval: env_u64("GAMMA_POLL_INTERVAL", 60_000),
            tick_interval: env_u64("TICK_INTERVAL", 1_000),
            tick_data_logging_enabled: env_bool("TICK_DATA_LOGGING_ENABLED", false),
            clob_ping_interval: 10_000,
            rtds_ping_interval: 5_000,
            chainlink_stale_ms: env_u64("CHAINLINK_STALE_MS", 30_000),
            websocket_connect_timeout_ms: env_u64("WEBSOCKET_CONNECT_TIMEOUT_MS", 10_000),
            binance_no_message_reconnect_ms: env_u64("BINANCE_NO_MESSAGE_RECONNECT_MS", 5_000),
            clob_no_message_reconnect_ms: env_u64("CLOB_NO_MESSAGE_RECONNECT_MS", 20_000),

            reconnect_base_delay: 1_000,
            reconnect_max_delay: 30_000,
            reconnect_min_stable_ms: env_u64("RECONNECT_MIN_STABLE_MS", 5_000),
            reconnect_max_failures: env_u64("RECONNECT_MAX_FAILURES", 20) as u32,
            reconnect_pause_ms: env_u64("RECONNECT_PAUSE_MS", 300_000),

            db_path: env_str("DB_PATH", "./data/paint.db"),

            latency_arb_momentum_threshold: env_f64("LATENCY_ARB_MOMENTUM_THRESHOLD", 0.0008),
            latency_arb_max_ask: env_f64("LATENCY_ARB_MAX_ASK", 0.60),
            latency_arb_min_ask: env_f64("LATENCY_ARB_MIN_ASK", 0.30),
            latency_arb_cooldown_ms: env_u64("LATENCY_ARB_COOLDOWN_MS", 60_000),
            latency_arb_adaptive_window_ms: env_u64("LATENCY_ARB_ADAPTIVE_WINDOW_MS", 1_800_000),
            latency_arb_enabled: env_bool("LATENCY_ARB_ENABLED", true),
            latency_arb_max_position_fraction: env::var("LATENCY_ARB_MAX_POSITION_FRACTION")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .or(Some(0.125)),

            spread_capture_threshold: env_f64("SPREAD_CAPTURE_THRESHOLD", 0.970),
            spread_capture_min_ask: env_f64("SPREAD_CAPTURE_MIN_ASK", 0.15),
            spread_capture_max_leg_skew_ms: env_u64("SPREAD_CAPTURE_MAX_LEG_SKEW_MS", 25),
            spread_capture_max_quote_churn_per_s: env_f64(
                "SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S",
                8.0,
            ),
            spread_capture_enabled: env_bool("SPREAD_CAPTURE_ENABLED", false),
            spread_capture_max_position_fraction: env::var("SPREAD_CAPTURE_MAX_POSITION_FRACTION")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .or(Some(0.05)),

            calm_persistence_enabled: env_bool("CALM_PERSISTENCE_ENABLED", false),
            calm_persistence_min_window_time_ms: env_u64(
                "CALM_PERSISTENCE_MIN_WINDOW_TIME_MS",
                30_000,
            ),
            calm_persistence_max_window_time_ms: env_u64(
                "CALM_PERSISTENCE_MAX_WINDOW_TIME_MS",
                90_000,
            ),
            calm_persistence_max_ask: env_f64("CALM_PERSISTENCE_MAX_ASK", 0.65),
            calm_persistence_min_abs_distance_bps: env_f64(
                "CALM_PERSISTENCE_MIN_ABS_DISTANCE_BPS",
                6.0,
            ),
            calm_persistence_distance_vol_ratio_threshold: env_f64(
                "CALM_PERSISTENCE_DISTANCE_VOL_RATIO_THRESHOLD",
                1.0,
            ),
            calm_persistence_max_realized_vol_15s_bps: env_f64(
                "CALM_PERSISTENCE_MAX_REALIZED_VOL_15S_BPS",
                80.0,
            ),
            calm_persistence_max_open_crosses_30s: env_u64(
                "CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S",
                1,
            ) as u32,
            calm_persistence_max_quote_churn_per_s: env_f64(
                "CALM_PERSISTENCE_MAX_QUOTE_CHURN_PER_S",
                100.0,
            ),
            calm_persistence_min_alignment_fraction: env_f64(
                "CALM_PERSISTENCE_MIN_ALIGNMENT_FRACTION",
                0.50,
            ),
            calm_persistence_max_fair_bias: env_f64("CALM_PERSISTENCE_MAX_FAIR_BIAS", 0.35),
            calm_persistence_min_expected_edge: env_f64("CALM_PERSISTENCE_MIN_EXPECTED_EDGE", 0.05),
            calm_persistence_max_position_fraction: env::var(
                "CALM_PERSISTENCE_MAX_POSITION_FRACTION",
            )
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .or(Some(0.05)),

            momentum_window_ms: env_u64("MOMENTUM_WINDOW_MS", 30_000),

            starting_balance: env_f64("STARTING_BALANCE", 100.0),
            max_position_fraction: env_f64("MAX_POSITION_FRACTION", 0.05),
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
            dd_pause_recovery_pct: env_f64("DD_PAUSE_RECOVERY_PCT", 0.05),

            trend_filter_enabled: env_bool("TREND_FILTER_ENABLED", false),
            trend_filter_threshold: env_f64("TREND_FILTER_THRESHOLD", 0.30),
            trend_filter_window: env_u64("TREND_FILTER_WINDOW", 10),
            trend_filter_per_strategy: env_bool("TREND_FILTER_PER_STRATEGY", false),

            regime_detection_enabled: env_bool("REGIME_DETECTION_ENABLED", false),

            log_level: env_str("LOG_LEVEL", "info"),

            gamma_market_limit: 20,

            max_position_usd: env_f64("MAX_POSITION_USD", 500.0),

            taker_fee_rate: taker_fee_rate_override
                .as_deref()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.07),
            taker_fee_exponent: taker_fee_exponent_override
                .as_deref()
                .and_then(|v| v.parse::<f64>().ok())
                .map_or(1, |v| v as u32),
            taker_fee_override_explicit,

            execution_mode: ExecutionMode::from_env_value(
                env::var("EXECUTION_MODE").ok().as_deref(),
            ),
            live_sidecar_url: env_str("LIVE_SIDECAR_URL", "http://127.0.0.1:3210"),
            live_sidecar_request_timeout_ms: env_u64("LIVE_SIDECAR_REQUEST_TIMEOUT_MS", 5_000),
            live_sidecar_emergency_timeout_ms: env_u64("LIVE_SIDECAR_EMERGENCY_TIMEOUT_MS", 2_000),
            live_session_cash_cap_usd: env_f64("LIVE_SESSION_CASH_CAP_USD", 100.0),
            live_max_single_order_usd: env_f64("LIVE_MAX_SINGLE_ORDER_USD", 10.0),
            live_max_open_notional_usd: env_f64("LIVE_MAX_OPEN_NOTIONAL_USD", 25.0),
            live_max_daily_loss_usd: env_f64("LIVE_MAX_DAILY_LOSS_USD", 15.0),
            live_max_session_drawdown_usd: env_f64("LIVE_MAX_SESSION_DRAWDOWN_USD", 20.0),
            live_min_required_cash_usd: env_f64("LIVE_MIN_REQUIRED_CASH_USD", 25.0),
            live_expected_signature_type: env::var("LIVE_EXPECTED_SIGNATURE_TYPE")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .map(|v| v.parse::<u32>().unwrap_or(u32::MAX)),
            live_allow_deposit_wallet: env_bool("LIVE_ALLOW_DEPOSIT_WALLET", false),
            live_expected_egress_ip: env::var("LIVE_EXPECTED_EGRESS_IP")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            enforce_open_exposure_caps: env_bool("ENFORCE_OPEN_EXPOSURE_CAPS", false),
            live_onchain_reconcile: env_bool("LIVE_ONCHAIN_RECONCILE", true),
            live_onchain_reconcile_grace_ms: env_u64("LIVE_ONCHAIN_RECONCILE_GRACE_MS", 6_000),
            live_onchain_reconcile_retry_interval_ms: env_u64(
                "LIVE_ONCHAIN_RECONCILE_RETRY_INTERVAL_MS",
                3_000,
            ),
            live_onchain_reconcile_max_attempts: env_u64("LIVE_ONCHAIN_RECONCILE_MAX_ATTEMPTS", 5)
                .clamp(1, u64::from(u32::MAX))
                as u32,
            live_dry_run: env_bool("LIVE_DRY_RUN", false),
            live_max_session_orders: env_u64("LIVE_MAX_SESSION_ORDERS", 0).min(u64::from(u32::MAX))
                as u32,
            live_max_session_fills: env_u64("LIVE_MAX_SESSION_FILLS", 0).min(u64::from(u32::MAX))
                as u32,
            feed_event_storage_profile: FeedEventStorageProfile::from_env_value(
                env::var("FEED_EVENT_STORAGE_PROFILE").ok().as_deref(),
            ),
            feed_event_writer_queue_capacity: env_u64("FEED_EVENT_WRITER_QUEUE_CAPACITY", 50_000)
                as usize,
            feed_event_writer_batch_size: env_u64("FEED_EVENT_WRITER_BATCH_SIZE", 500) as usize,
            feed_event_writer_flush_ms: env_u64("FEED_EVENT_WRITER_FLUSH_MS", 100),
            feed_event_writer_max_lag_ms: env_u64("FEED_EVENT_WRITER_MAX_LAG_MS", 2_000),
            clob_replay_block_max_rows: env_u64("CLOB_REPLAY_BLOCK_MAX_ROWS", 10_000) as usize,
            clob_replay_block_max_ms: env_u64("CLOB_REPLAY_BLOCK_MAX_MS", 1_000),
            clob_replay_block_zstd_level: env_u64("CLOB_REPLAY_BLOCK_ZSTD_LEVEL", 3) as i32,
            live_runtime_max_db_bytes: env_u64("LIVE_RUNTIME_MAX_DB_BYTES", 50_000_000_000),
            live_feed_batch_max_messages: env_u64("LIVE_FEED_BATCH_MAX_MESSAGES", 64) as usize,
            live_decision_queue_capacity: env_u64("LIVE_DECISION_QUEUE_CAPACITY", 128) as usize,
            live_decision_output_queue_capacity: env_u64(
                "LIVE_DECISION_OUTPUT_QUEUE_CAPACITY",
                1_024,
            ) as usize,
            live_runtime_persistence_queue_capacity: env_u64(
                "LIVE_RUNTIME_PERSISTENCE_QUEUE_CAPACITY",
                10_000,
            ) as usize,
            live_submission_queue_capacity: env_u64("LIVE_SUBMISSION_QUEUE_CAPACITY", 1_024)
                as usize,
            max_live_decision_age_ms: env_u64("MAX_LIVE_DECISION_AGE_MS", 250),
            worker_shutdown_timeout_ms: env_u64("WORKER_SHUTDOWN_TIMEOUT_MS", 5_000),
            sim_order_latency_ms: env_u64("SIM_ORDER_LATENCY_MS", 250),
            max_book_staleness_ms: env_u64("MAX_BOOK_STALENESS_MS", 1_000),
            max_signal_feed_age_ms: env_u64("MAX_SIGNAL_FEED_AGE_MS", 1_000),
            max_quote_age_ms: env_u64("MAX_QUOTE_AGE_MS", 750),

            resolution_poll_retries: 30,
            resolution_initial_delay_ms: env_u64("RESOLUTION_INITIAL_DELAY_MS", 30_000),
            resolution_poll_delay_ms: 10_000,
            pending_settlement_family_reserve_fraction: env_f64(
                "PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION",
                0.0,
            ),
            pending_settlement_global_reserve_fraction: env_f64(
                "PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION",
                0.25,
            ),
            pending_settlement_counts_as_open_position: env_bool(
                "PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION",
                false,
            ),
            backtest_settlement_mode: BacktestSettlementMode::from_env_value(
                env::var("BACKTEST_SETTLEMENT_MODE").ok().as_deref(),
            ),
        }
    }
}

impl Default for Config {
    /// Returns defaults without reading environment variables (useful for tests).
    #[allow(clippy::too_many_lines)]
    fn default() -> Self {
        let trade_stream = "btcusdt@aggTrade".to_string();
        let book_ticker_stream = "btcusdt@bookTicker".to_string();
        let depth_stream = "btcusdt@depth5@100ms".to_string();
        Self {
            binance_ws_url: default_binance_ws_url(
                &trade_stream,
                &book_ticker_stream,
                &depth_stream,
                true,
            ),
            binance_trade_stream: trade_stream,
            binance_book_ticker_stream: book_ticker_stream,
            binance_depth_stream: depth_stream,
            clob_ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/market".to_string(),
            clob_api_url: "https://clob.polymarket.com".to_string(),
            rtds_ws_url: "wss://ws-live-data.polymarket.com".to_string(),
            gamma_api_url: "https://gamma-api.polymarket.com".to_string(),

            gamma_poll_interval: 60_000,
            tick_interval: 1_000,
            tick_data_logging_enabled: false,
            clob_ping_interval: 10_000,
            rtds_ping_interval: 5_000,
            chainlink_stale_ms: 30_000,
            websocket_connect_timeout_ms: 10_000,
            binance_no_message_reconnect_ms: 5_000,
            clob_no_message_reconnect_ms: 20_000,

            reconnect_base_delay: 1_000,
            reconnect_max_delay: 30_000,
            reconnect_min_stable_ms: 5_000,
            reconnect_max_failures: 20,
            reconnect_pause_ms: 300_000,

            db_path: "./data/paint.db".to_string(),

            latency_arb_momentum_threshold: 0.0008,
            latency_arb_max_ask: 0.60,
            latency_arb_min_ask: 0.30,
            latency_arb_cooldown_ms: 60_000,
            latency_arb_adaptive_window_ms: 1_800_000,
            latency_arb_enabled: true,
            latency_arb_max_position_fraction: Some(0.125),

            spread_capture_threshold: 0.970,
            spread_capture_min_ask: 0.15,
            spread_capture_max_leg_skew_ms: 25,
            spread_capture_max_quote_churn_per_s: 8.0,
            spread_capture_enabled: false,
            spread_capture_max_position_fraction: Some(0.05),

            calm_persistence_enabled: false,
            calm_persistence_min_window_time_ms: 30_000,
            calm_persistence_max_window_time_ms: 90_000,
            calm_persistence_max_ask: 0.65,
            calm_persistence_min_abs_distance_bps: 6.0,
            calm_persistence_distance_vol_ratio_threshold: 1.0,
            calm_persistence_max_realized_vol_15s_bps: 80.0,
            calm_persistence_max_open_crosses_30s: 1,
            calm_persistence_max_quote_churn_per_s: 100.0,
            calm_persistence_min_alignment_fraction: 0.50,
            calm_persistence_max_fair_bias: 0.35,
            calm_persistence_min_expected_edge: 0.05,
            calm_persistence_max_position_fraction: Some(0.05),

            momentum_window_ms: 30_000,

            starting_balance: 100.0,
            max_position_fraction: 0.05,
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
            dd_pause_recovery_pct: 0.05,

            trend_filter_enabled: false,
            trend_filter_threshold: 0.30,
            trend_filter_window: 10,
            trend_filter_per_strategy: false,

            regime_detection_enabled: false,

            log_level: "info".to_string(),

            gamma_market_limit: 20,

            max_position_usd: 500.0,

            taker_fee_rate: 0.07,
            taker_fee_exponent: 1,
            taker_fee_override_explicit: false,

            execution_mode: ExecutionMode::Paper,
            live_sidecar_url: "http://127.0.0.1:3210".to_string(),
            live_sidecar_request_timeout_ms: 5_000,
            live_sidecar_emergency_timeout_ms: 2_000,
            live_session_cash_cap_usd: 100.0,
            live_max_single_order_usd: 10.0,
            live_max_open_notional_usd: 25.0,
            live_max_daily_loss_usd: 15.0,
            live_max_session_drawdown_usd: 20.0,
            live_min_required_cash_usd: 25.0,
            live_expected_signature_type: None,
            live_allow_deposit_wallet: false,
            live_expected_egress_ip: None,
            enforce_open_exposure_caps: false,
            live_onchain_reconcile: true,
            live_onchain_reconcile_grace_ms: 6_000,
            live_onchain_reconcile_retry_interval_ms: 3_000,
            live_onchain_reconcile_max_attempts: 5,
            live_dry_run: false,
            live_max_session_orders: 0,
            live_max_session_fills: 0,
            feed_event_storage_profile: FeedEventStorageProfile::ReplayGrade,
            feed_event_writer_queue_capacity: 50_000,
            feed_event_writer_batch_size: 500,
            feed_event_writer_flush_ms: 100,
            feed_event_writer_max_lag_ms: 2_000,
            clob_replay_block_max_rows: 10_000,
            clob_replay_block_max_ms: 1_000,
            clob_replay_block_zstd_level: 3,
            live_runtime_max_db_bytes: 50_000_000_000,
            live_feed_batch_max_messages: 64,
            live_decision_queue_capacity: 128,
            live_decision_output_queue_capacity: 1_024,
            live_runtime_persistence_queue_capacity: 10_000,
            live_submission_queue_capacity: 1_024,
            max_live_decision_age_ms: 250,
            worker_shutdown_timeout_ms: 5_000,
            sim_order_latency_ms: 250,
            max_book_staleness_ms: 1_000,
            max_signal_feed_age_ms: 1_000,
            max_quote_age_ms: 750,

            resolution_poll_retries: 30,
            resolution_initial_delay_ms: 30_000,
            resolution_poll_delay_ms: 10_000,
            pending_settlement_family_reserve_fraction: 0.0,
            pending_settlement_global_reserve_fraction: 0.25,
            pending_settlement_counts_as_open_position: false,
            backtest_settlement_mode: BacktestSettlementMode::Immediate,
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
