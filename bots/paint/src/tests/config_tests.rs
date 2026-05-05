use super::*;

/// Verifies that default values match typescript.
#[test]
fn default_values_match_typescript() {
    let cfg = Config::default();

    assert_eq!(
        cfg.binance_ws_url,
        "wss://stream.binance.com:9443/stream?streams=btcusdt@aggTrade/btcusdt@bookTicker/btcusdt@depth5@100ms&timeUnit=MICROSECOND"
    );
    assert_eq!(
        cfg.clob_ws_url,
        "wss://ws-subscriptions-clob.polymarket.com/ws/market"
    );
    assert_eq!(cfg.rtds_ws_url, "wss://ws-live-data.polymarket.com");
    assert_eq!(cfg.gamma_api_url, "https://gamma-api.polymarket.com");

    assert_eq!(cfg.gamma_poll_interval, 60_000);
    assert_eq!(cfg.tick_interval, 1_000);
    assert_eq!(cfg.clob_ping_interval, 10_000);
    assert_eq!(cfg.rtds_ping_interval, 5_000);
    assert_eq!(cfg.chainlink_stale_ms, 30_000);

    assert_eq!(cfg.reconnect_base_delay, 1_000);
    assert_eq!(cfg.reconnect_max_delay, 30_000);
    assert_eq!(cfg.reconnect_min_stable_ms, 5_000);
    assert_eq!(cfg.reconnect_max_failures, 20);
    assert_eq!(cfg.reconnect_pause_ms, 300_000);
    assert_eq!(cfg.websocket_connect_timeout_ms, 10_000);
    assert_eq!(cfg.binance_no_message_reconnect_ms, 5_000);
    assert_eq!(cfg.clob_no_message_reconnect_ms, 20_000);

    assert_eq!(cfg.db_path, "./data/paint.db");

    assert!((cfg.latency_arb_momentum_threshold - 0.0008).abs() < f64::EPSILON);
    assert!((cfg.latency_arb_max_ask - 0.60).abs() < f64::EPSILON);
    assert!((cfg.latency_arb_min_ask - 0.30).abs() < f64::EPSILON);
    assert_eq!(cfg.latency_arb_cooldown_ms, 60_000);

    assert!((cfg.spread_capture_threshold - 0.970).abs() < f64::EPSILON);
    assert!((cfg.spread_capture_min_ask - 0.15).abs() < f64::EPSILON);
    assert!((cfg.spread_capture_max_quote_churn_per_s - 8.0).abs() < f64::EPSILON);
    assert_eq!(cfg.spread_capture_max_position_fraction, Some(0.05));

    assert_eq!(cfg.momentum_window_ms, 30_000);

    assert!((cfg.starting_balance - 100.0).abs() < f64::EPSILON);
    assert!((cfg.max_position_fraction - 0.05).abs() < f64::EPSILON);
    assert!((cfg.min_balance_threshold - 20.0).abs() < f64::EPSILON);
    assert!((cfg.max_drawdown_pct - 0.50).abs() < f64::EPSILON);
    assert!((cfg.max_position_usd_fraction - 0.20).abs() < f64::EPSILON);

    assert!((cfg.kelly_fraction - 0.5).abs() < f64::EPSILON);
    assert!((cfg.min_win_rate_for_kelly - 0.52).abs() < f64::EPSILON);
    assert_eq!(cfg.min_trades_for_kelly, 20);
    assert!((cfg.min_kelly_floor - 0.03).abs() < f64::EPSILON);
    assert!((cfg.min_bet_usd - 5.0).abs() < f64::EPSILON);
    assert_eq!(cfg.kelly_rolling_window, 30);

    assert_eq!(cfg.max_open_positions, 5);
    assert_eq!(cfg.min_window_time_ms, 90_000);

    assert_eq!(cfg.circuit_breaker_losses, 3);
    assert_eq!(cfg.circuit_breaker_pause_ms, 900_000);

    assert!((cfg.peak_dd_pause_pct - 0.30).abs() < f64::EPSILON);
    assert_eq!(cfg.peak_dd_pause_ms, 3_600_000);
    assert!((cfg.dd_pause_recovery_pct - 0.05).abs() < f64::EPSILON);

    assert!(!cfg.trend_filter_enabled);
    assert!((cfg.trend_filter_threshold - 0.30).abs() < f64::EPSILON);
    assert_eq!(cfg.trend_filter_window, 10);
    assert!(!cfg.trend_filter_per_strategy);

    assert!(!cfg.regime_detection_enabled);
    assert!(cfg.latency_arb_enabled);
    assert!(!cfg.spread_capture_enabled);
    assert!(!cfg.calm_persistence_enabled);
    assert_eq!(cfg.latency_arb_max_position_fraction, Some(0.125));
    assert_eq!(cfg.calm_persistence_max_position_fraction, Some(0.05));
    assert_eq!(cfg.calm_persistence_min_window_time_ms, 30_000);
    assert_eq!(cfg.calm_persistence_max_window_time_ms, 90_000);
    assert!((cfg.calm_persistence_max_ask - 0.65).abs() < f64::EPSILON);
    assert!((cfg.calm_persistence_min_abs_distance_bps - 6.0).abs() < f64::EPSILON);
    assert!((cfg.calm_persistence_distance_vol_ratio_threshold - 1.0).abs() < f64::EPSILON);
    assert!((cfg.calm_persistence_max_realized_vol_15s_bps - 80.0).abs() < f64::EPSILON);
    assert_eq!(cfg.calm_persistence_max_open_crosses_30s, 1);
    assert!((cfg.calm_persistence_max_quote_churn_per_s - 100.0).abs() < f64::EPSILON);
    assert!((cfg.calm_persistence_min_alignment_fraction - 0.50).abs() < f64::EPSILON);
    assert!((cfg.calm_persistence_max_fair_bias - 0.35).abs() < f64::EPSILON);
    assert!((cfg.calm_persistence_min_expected_edge - 0.05).abs() < f64::EPSILON);

    assert_eq!(cfg.log_level, "info");

    assert_eq!(cfg.gamma_market_limit, 20);
    assert_eq!(
        cfg.feed_event_storage_profile,
        FeedEventStorageProfile::ReplayGrade
    );

    assert_eq!(cfg.resolution_poll_retries, 30);
    assert_eq!(cfg.resolution_initial_delay_ms, 30_000);
    assert_eq!(cfg.resolution_poll_delay_ms, 10_000);
    assert!((cfg.pending_settlement_family_reserve_fraction - 0.0).abs() < f64::EPSILON);
    assert!((cfg.pending_settlement_global_reserve_fraction - 0.25).abs() < f64::EPSILON);
    assert!(!cfg.pending_settlement_counts_as_open_position);
    assert_eq!(
        cfg.backtest_settlement_mode,
        BacktestSettlementMode::Immediate
    );
}

/// Verifies that the default pending-settlement policy matches the run-012 canary baseline.
#[test]
fn default_pending_settlement_policy_matches_run012_canary() {
    let cfg = Config::default();
    let policy = cfg.pending_settlement_policy().unwrap();

    assert_eq!(policy.mode, PendingSettlementReserveMode::Risky);
    assert!((policy.family_reserve_fraction - 0.0).abs() < f64::EPSILON);
    assert!((policy.global_reserve_fraction - 0.25).abs() < f64::EPSILON);
    assert!(!policy.counts_as_open_position);
}

/// Verifies that named pending-settlement policies classify correctly.
#[test]
fn pending_settlement_policy_classifies_named_modes() {
    let compatibility = PendingSettlementPolicy::classify(1.0, 1.0, true);
    assert_eq!(
        compatibility.mode,
        PendingSettlementReserveMode::Compatibility
    );

    let conservative = PendingSettlementPolicy::classify(0.0, 1.0, false);
    assert_eq!(
        conservative.mode,
        PendingSettlementReserveMode::Conservative
    );

    let risky = PendingSettlementPolicy::classify(0.0, 0.25, false);
    assert_eq!(risky.mode, PendingSettlementReserveMode::Risky);

    let custom = PendingSettlementPolicy::classify(0.4, 0.8, false);
    assert_eq!(custom.mode, PendingSettlementReserveMode::Custom);
}

/// Verifies that pending-settlement validation rejects invalid fractions.
#[test]
fn pending_settlement_policy_rejects_invalid_fraction() {
    let mut cfg = Config::default();
    cfg.pending_settlement_family_reserve_fraction = 1.25;
    assert!(cfg.validate().is_err());

    cfg.pending_settlement_family_reserve_fraction = 0.0;
    cfg.pending_settlement_global_reserve_fraction = -0.1;
    assert!(cfg.validate().is_err());
}

/// Verifies that feed reconnect validation rejects zero timeout/watchdog settings.
#[test]
fn feed_reconnect_validation_rejects_zero_thresholds() {
    let mut cfg = Config::default();
    cfg.websocket_connect_timeout_ms = 0;
    assert!(cfg.validate().is_err());

    cfg.websocket_connect_timeout_ms = 10_000;
    cfg.binance_no_message_reconnect_ms = 0;
    assert!(cfg.validate().is_err());

    cfg.binance_no_message_reconnect_ms = 5_000;
    cfg.clob_no_message_reconnect_ms = 0;
    assert!(cfg.validate().is_err());
}

/// Verifies that live-execution validation accepts a small bankroll envelope.
#[test]
fn live_execution_validation_allows_small_bankroll_caps() {
    let mut cfg = Config::default();
    cfg.execution_mode = ExecutionMode::LiveReadonly;
    cfg.live_session_cash_cap_usd = 90.0;
    cfg.live_max_single_order_usd = 7.5;
    cfg.live_max_open_notional_usd = 20.0;
    cfg.live_max_daily_loss_usd = 10.0;
    cfg.live_max_session_drawdown_usd = 12.0;
    cfg.live_min_required_cash_usd = 25.0;
    cfg.live_sidecar_url = "http://127.0.0.1:3210".to_string();
    cfg.clob_api_url = "https://clob.polymarket.com".to_string();
    cfg.gamma_api_url = "https://gamma-api.polymarket.com".to_string();

    assert!(cfg.validate().is_ok());
}

/// Verifies that live-execution validation rejects malformed venue URLs.
#[test]
fn live_execution_validation_rejects_invalid_urls() {
    let mut cfg = Config::default();
    cfg.execution_mode = ExecutionMode::LiveReadonly;
    cfg.live_sidecar_url = "not-a-url".to_string();
    assert!(cfg.validate().is_err());

    cfg.live_sidecar_url = "http://127.0.0.1:3210".to_string();
    cfg.clob_api_url = "/relative".to_string();
    assert!(cfg.validate().is_err());

    cfg.clob_api_url = "https://clob.polymarket.com".to_string();
    cfg.gamma_api_url = "gamma".to_string();
    assert!(cfg.validate().is_err());
}

/// Verifies that resolve str override.
#[test]
fn resolve_str_override() {
    assert_eq!(
        resolve_str(Some("/tmp/test.db"), "./data/paint.db"),
        "/tmp/test.db"
    );
}

/// Verifies that resolve str default.
#[test]
fn resolve_str_default() {
    assert_eq!(resolve_str(None, "./data/paint.db"), "./data/paint.db");
}

/// Verifies that resolve f64 override.
#[test]
fn resolve_f64_override() {
    let val = resolve_f64(Some("999.5"), 150.0);
    assert!((val - 999.5).abs() < f64::EPSILON);
}

/// Verifies that resolve f64 default.
#[test]
fn resolve_f64_default() {
    let val = resolve_f64(None, 150.0);
    assert!((val - 150.0).abs() < f64::EPSILON);
}

/// Verifies that resolve f64 invalid returns default.
#[test]
fn resolve_f64_invalid_returns_default() {
    let val = resolve_f64(Some("not_a_number"), 150.0);
    assert!((val - 150.0).abs() < f64::EPSILON);
}

/// Verifies that resolve u64 override.
#[test]
fn resolve_u64_override() {
    assert_eq!(resolve_u64(Some("2000"), 1_000), 2_000);
}

/// Verifies that resolve u64 default.
#[test]
fn resolve_u64_default() {
    assert_eq!(resolve_u64(None, 1_000), 1_000);
}

/// Verifies that resolve u64 invalid returns default.
#[test]
fn resolve_u64_invalid_returns_default() {
    assert_eq!(resolve_u64(Some("abc"), 1_000), 1_000);
}

/// Verifies that resolve bool true.
#[test]
fn resolve_bool_true() {
    assert!(resolve_bool(Some("true"), false));
}

/// Verifies that resolve bool false when missing.
#[test]
fn resolve_bool_false_when_missing() {
    assert!(!resolve_bool(None, false));
}

/// Verifies that resolve bool accepts common truthy forms.
#[test]
fn resolve_bool_truthy_variants() {
    assert!(resolve_bool(Some("yes"), false));
    assert!(resolve_bool(Some("1"), false));
    assert!(resolve_bool(Some("TRUE"), false));
    assert!(resolve_bool(Some("on"), false));
}

/// Verifies that resolve bool default true preserved.
#[test]
fn resolve_bool_default_true_preserved() {
    assert!(resolve_bool(None, true));
}

/// Verifies that invalid bool-like text falls back to the default in test helpers.
#[test]
fn resolve_bool_invalid_uses_default() {
    assert!(resolve_bool(Some("maybe"), true));
    assert!(!resolve_bool(Some("maybe"), false));
}

/// Verifies that config reports the currently enabled strategy set.
#[test]
fn enabled_strategy_names_match_flags() {
    let mut cfg = Config::default();
    assert_eq!(cfg.enabled_strategy_names(), vec!["latency-arb"]);

    cfg.calm_persistence_enabled = true;
    assert_eq!(
        cfg.enabled_strategy_names(),
        vec!["latency-arb", "calm-persistence"]
    );

    cfg.spread_capture_enabled = true;
    assert_eq!(
        cfg.enabled_strategy_names(),
        vec!["latency-arb", "spread-capture", "calm-persistence"]
    );
}

/// Verifies that boolean overrides can be applied without numeric coercion.
#[test]
fn set_bool_param_updates_known_flags() {
    let mut cfg = Config::default();
    assert!(cfg.set_bool_param("LATENCY_ARB_ENABLED", false));
    assert!(!cfg.latency_arb_enabled);
    assert!(cfg.set_bool_param("REGIME_DETECTION_ENABLED", true));
    assert!(cfg.regime_detection_enabled);
    assert!(cfg.set_bool_param("PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION", true));
    assert!(cfg.pending_settlement_counts_as_open_position);
    assert!(!cfg.set_bool_param("MAX_POSITION_FRACTION", true));
}

/// Verifies that live execution mode defaults to paper but parses explicit live strings.
#[test]
fn execution_mode_parses_known_labels() {
    assert_eq!(ExecutionMode::from_env_value(None), ExecutionMode::Paper);
    assert_eq!(
        ExecutionMode::from_env_value(Some("live_readonly")),
        ExecutionMode::LiveReadonly
    );
    assert_eq!(
        ExecutionMode::from_env_value(Some("live_trading")),
        ExecutionMode::LiveTrading
    );
}

/// Verifies that string config overrides can update live-mode surfaces.
#[test]
fn set_string_param_updates_live_fields() {
    let mut cfg = Config::default();
    assert!(cfg.set_string_param("EXECUTION_MODE", "live_readonly"));
    assert_eq!(cfg.execution_mode, ExecutionMode::LiveReadonly);
    assert!(cfg.set_string_param("LIVE_SIDECAR_URL", "http://127.0.0.1:4000"));
    assert_eq!(cfg.live_sidecar_url, "http://127.0.0.1:4000");
    assert!(cfg.set_string_param("CLOB_API_URL", "https://clob.example.test"));
    assert_eq!(cfg.clob_api_url, "https://clob.example.test");
}

/// Verifies that config is cloneable.
#[test]
fn config_is_cloneable() {
    let cfg = Config::default();
    let cfg2 = cfg.clone();
    assert_eq!(cfg.tick_interval, cfg2.tick_interval);
    assert!((cfg.starting_balance - cfg2.starting_balance).abs() < f64::EPSILON);
}

/// Verifies that set param momentum threshold.
#[test]
fn set_param_momentum_threshold() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("LATENCY_ARB_MOMENTUM_THRESHOLD", 0.0012));
    assert!((cfg.latency_arb_momentum_threshold - 0.0012).abs() < f64::EPSILON);
}

/// Verifies that set param max position fraction.
#[test]
fn set_param_max_position_fraction() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("MAX_POSITION_FRACTION", 0.125));
    assert!((cfg.max_position_fraction - 0.125).abs() < f64::EPSILON);
}

/// Verifies that set param peak dd pause pct.
#[test]
fn set_param_peak_dd_pause_pct() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("PEAK_DD_PAUSE_PCT", 1.0));
    assert!((cfg.peak_dd_pause_pct - 1.0).abs() < f64::EPSILON);
}

/// Verifies that set param dd pause recovery pct.
#[test]
fn set_param_dd_pause_recovery_pct() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("DD_PAUSE_RECOVERY_PCT", 0.10));
    assert!((cfg.dd_pause_recovery_pct - 0.10).abs() < f64::EPSILON);
}

/// Verifies that set param reconnect min stable ms.
#[test]
fn set_param_reconnect_min_stable_ms() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("RECONNECT_MIN_STABLE_MS", 10_000.0));
    assert_eq!(cfg.reconnect_min_stable_ms, 10_000);
}

/// Verifies that set param resolution initial delay ms.
#[test]
fn set_param_resolution_initial_delay_ms() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("RESOLUTION_INITIAL_DELAY_MS", 2_500.0));
    assert_eq!(cfg.resolution_initial_delay_ms, 2_500);
}

/// Verifies that set param cooldown ms u64 cast.
#[test]
fn set_param_cooldown_ms_u64_cast() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("LATENCY_ARB_COOLDOWN_MS", 30_000.0));
    assert_eq!(cfg.latency_arb_cooldown_ms, 30_000);
}

/// Verifies that set param starting balance.
#[test]
fn set_param_starting_balance() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("STARTING_BALANCE", 500.0));
    assert!((cfg.starting_balance - 500.0).abs() < f64::EPSILON);
}

/// Verifies that set param unknown returns false.
#[test]
fn set_param_unknown_returns_false() {
    let mut cfg = Config::default();
    assert!(!cfg.set_param("NONEXISTENT_PARAM", 42.0));
}

/// Verifies that set param returns bool.
#[test]
fn set_param_returns_bool() {
    let mut cfg = Config::default();
    let result: bool = cfg.set_param("STARTING_BALANCE", 200.0);
    assert!(result);
}

/// Verifies that set param all f64 params.
#[test]
fn set_param_all_f64_params() {
    let mut cfg = Config::default();

    let f64_params = [
        "LATENCY_ARB_MOMENTUM_THRESHOLD",
        "LATENCY_ARB_MAX_ASK",
        "LATENCY_ARB_MIN_ASK",
        "MAX_POSITION_FRACTION",
        "SPREAD_CAPTURE_THRESHOLD",
        "SPREAD_CAPTURE_MIN_ASK",
        "PEAK_DD_PAUSE_PCT",
        "DD_PAUSE_RECOVERY_PCT",
        "STARTING_BALANCE",
        "MAX_POSITION_USD_FRACTION",
        "MIN_BALANCE_THRESHOLD",
        "MAX_DRAWDOWN_PCT",
        "KELLY_FRACTION",
        "MIN_WIN_RATE_FOR_KELLY",
        "MIN_KELLY_FLOOR",
        "MIN_BET_USD",
        "TREND_FILTER_THRESHOLD",
    ];
    for param in f64_params {
        assert!(
            cfg.set_param(param, 0.42),
            "set_param({param}) should return true"
        );
    }
}

/// Verifies that set param all u64 params.
#[test]
fn set_param_all_u64_params() {
    let mut cfg = Config::default();
    let u64_params = [
        "LATENCY_ARB_COOLDOWN_MS",
        "PEAK_DD_PAUSE_MS",
        "MOMENTUM_WINDOW_MS",
        "MIN_TRADES_FOR_KELLY",
        "KELLY_ROLLING_WINDOW",
        "MAX_OPEN_POSITIONS",
        "MIN_WINDOW_TIME_MS",
        "CIRCUIT_BREAKER_LOSSES",
        "CIRCUIT_BREAKER_PAUSE_MS",
        "TREND_FILTER_WINDOW",
        "RECONNECT_MIN_STABLE_MS",
        "RECONNECT_MAX_FAILURES",
        "RECONNECT_PAUSE_MS",
        "WEBSOCKET_CONNECT_TIMEOUT_MS",
        "BINANCE_NO_MESSAGE_RECONNECT_MS",
        "CLOB_NO_MESSAGE_RECONNECT_MS",
    ];
    for param in u64_params {
        assert!(
            cfg.set_param(param, 100.0),
            "set_param({param}) should return true"
        );
    }
    assert_eq!(cfg.latency_arb_cooldown_ms, 100);
    assert_eq!(cfg.peak_dd_pause_ms, 100);
    assert_eq!(cfg.circuit_breaker_losses, 100);
    assert_eq!(cfg.websocket_connect_timeout_ms, 100);
    assert_eq!(cfg.binance_no_message_reconnect_ms, 100);
    assert_eq!(cfg.clob_no_message_reconnect_ms, 100);
}

/// Verifies that from env smoke test.
#[test]
fn from_env_smoke_test() {
    let cfg = Config::from_env();
    let default = Config::default();

    assert_eq!(cfg.tick_interval, default.tick_interval);
    assert_eq!(cfg.gamma_poll_interval, default.gamma_poll_interval);
    assert_eq!(cfg.chainlink_stale_ms, default.chainlink_stale_ms);

    assert!((cfg.starting_balance - default.starting_balance).abs() < f64::EPSILON);
    assert!(
        (cfg.latency_arb_momentum_threshold - default.latency_arb_momentum_threshold).abs()
            < f64::EPSILON
    );
    assert!((cfg.latency_arb_max_ask - default.latency_arb_max_ask).abs() < f64::EPSILON);
    assert!((cfg.latency_arb_min_ask - default.latency_arb_min_ask).abs() < f64::EPSILON);
    assert!((cfg.spread_capture_threshold - default.spread_capture_threshold).abs() < f64::EPSILON);
    assert!((cfg.spread_capture_min_ask - default.spread_capture_min_ask).abs() < f64::EPSILON);
    assert!((cfg.max_position_fraction - default.max_position_fraction).abs() < f64::EPSILON);
    assert!((cfg.min_balance_threshold - default.min_balance_threshold).abs() < f64::EPSILON);
    assert!((cfg.max_drawdown_pct - default.max_drawdown_pct).abs() < f64::EPSILON);
    assert!(
        (cfg.max_position_usd_fraction - default.max_position_usd_fraction).abs() < f64::EPSILON
    );
    assert!((cfg.kelly_fraction - default.kelly_fraction).abs() < f64::EPSILON);
    assert!((cfg.min_win_rate_for_kelly - default.min_win_rate_for_kelly).abs() < f64::EPSILON);
    assert!((cfg.min_kelly_floor - default.min_kelly_floor).abs() < f64::EPSILON);
    assert!((cfg.min_bet_usd - default.min_bet_usd).abs() < f64::EPSILON);
    assert!((cfg.peak_dd_pause_pct - default.peak_dd_pause_pct).abs() < f64::EPSILON);
    assert!((cfg.dd_pause_recovery_pct - default.dd_pause_recovery_pct).abs() < f64::EPSILON);
    assert!((cfg.trend_filter_threshold - default.trend_filter_threshold).abs() < f64::EPSILON);

    assert_eq!(cfg.latency_arb_cooldown_ms, default.latency_arb_cooldown_ms);
    assert_eq!(cfg.momentum_window_ms, default.momentum_window_ms);
    assert_eq!(cfg.min_trades_for_kelly, default.min_trades_for_kelly);
    assert_eq!(cfg.kelly_rolling_window, default.kelly_rolling_window);
    assert_eq!(cfg.max_open_positions, default.max_open_positions);
    assert_eq!(cfg.min_window_time_ms, default.min_window_time_ms);
    assert_eq!(cfg.circuit_breaker_losses, default.circuit_breaker_losses);
    assert_eq!(
        cfg.circuit_breaker_pause_ms,
        default.circuit_breaker_pause_ms
    );
    assert_eq!(cfg.peak_dd_pause_ms, default.peak_dd_pause_ms);
    assert_eq!(cfg.trend_filter_window, default.trend_filter_window);

    assert_eq!(cfg.trend_filter_enabled, default.trend_filter_enabled);
    assert_eq!(
        cfg.regime_detection_enabled,
        default.regime_detection_enabled
    );

    assert_eq!(cfg.db_path, default.db_path);
    assert_eq!(cfg.log_level, default.log_level);
    assert_eq!(cfg.binance_ws_url, default.binance_ws_url);
    assert_eq!(cfg.clob_ws_url, default.clob_ws_url);
    assert_eq!(cfg.rtds_ws_url, default.rtds_ws_url);
    assert_eq!(cfg.gamma_api_url, default.gamma_api_url);

    assert_eq!(cfg.clob_ping_interval, default.clob_ping_interval);
    assert_eq!(cfg.rtds_ping_interval, default.rtds_ping_interval);
    assert_eq!(cfg.reconnect_base_delay, default.reconnect_base_delay);
    assert_eq!(cfg.reconnect_max_delay, default.reconnect_max_delay);
    assert_eq!(cfg.reconnect_min_stable_ms, default.reconnect_min_stable_ms);
    assert_eq!(
        cfg.websocket_connect_timeout_ms,
        default.websocket_connect_timeout_ms
    );
    assert_eq!(
        cfg.binance_no_message_reconnect_ms,
        default.binance_no_message_reconnect_ms
    );
    assert_eq!(
        cfg.clob_no_message_reconnect_ms,
        default.clob_no_message_reconnect_ms
    );
    assert_eq!(cfg.gamma_market_limit, default.gamma_market_limit);
    assert_eq!(
        cfg.feed_event_storage_profile,
        default.feed_event_storage_profile
    );
}

/// Verifies that storage profile env parsing accepts full debug mode.
#[test]
fn storage_profile_env_parses_full_debug() {
    assert_eq!(
        FeedEventStorageProfile::from_env_value(Some("full_debug")),
        FeedEventStorageProfile::FullDebug
    );
}

/// Verifies that storage profile env parsing accepts compact mode.
#[test]
fn storage_profile_env_parses_compact() {
    assert_eq!(
        FeedEventStorageProfile::from_env_value(Some("compact")),
        FeedEventStorageProfile::Compact
    );
}

/// Verifies that storage profile env parsing defaults to replay-grade mode.
#[test]
fn storage_profile_env_defaults_to_replay_grade() {
    assert_eq!(
        FeedEventStorageProfile::from_env_value(Some("unexpected")),
        FeedEventStorageProfile::ReplayGrade
    );
    assert_eq!(
        FeedEventStorageProfile::from_env_value(None),
        FeedEventStorageProfile::ReplayGrade
    );
}

/// Verifies that resolve f64 empty string returns default.
#[test]
fn resolve_f64_empty_string_returns_default() {
    let val = resolve_f64(Some(""), 42.0);
    assert!((val - 42.0).abs() < f64::EPSILON);
}

/// Verifies that resolve u64 negative returns default.
#[test]
fn resolve_u64_negative_returns_default() {
    assert_eq!(resolve_u64(Some("-1"), 999), 999);
}

/// Verifies that resolve u64 float string returns default.
#[test]
fn resolve_u64_float_string_returns_default() {
    assert_eq!(resolve_u64(Some("3.14"), 1_000), 1_000);
}

/// Verifies that resolve bool false string is not true.
#[test]
fn resolve_bool_false_string_is_not_true() {
    assert!(!resolve_bool(Some("false"), false));
}

/// Verifies that resolve str empty string override.
#[test]
fn resolve_str_empty_string_override() {
    assert_eq!(resolve_str(Some(""), "default_value"), "");
}

/// Verifies that set param verifies every u64 field value.
#[test]
fn set_param_verifies_every_u64_field_value() {
    let mut cfg = Config::default();
    cfg.set_param("MOMENTUM_WINDOW_MS", 5_000.0);
    assert_eq!(cfg.momentum_window_ms, 5_000);

    cfg.set_param("MIN_TRADES_FOR_KELLY", 50.0);
    assert_eq!(cfg.min_trades_for_kelly, 50);

    cfg.set_param("KELLY_ROLLING_WINDOW", 15.0);
    assert_eq!(cfg.kelly_rolling_window, 15);

    cfg.set_param("MAX_OPEN_POSITIONS", 10.0);
    assert_eq!(cfg.max_open_positions, 10);

    cfg.set_param("MIN_WINDOW_TIME_MS", 120_000.0);
    assert_eq!(cfg.min_window_time_ms, 120_000);

    cfg.set_param("TREND_FILTER_WINDOW", 20.0);
    assert_eq!(cfg.trend_filter_window, 20);
}

/// Verifies that from env returns valid config.
#[test]
fn from_env_returns_valid_config() {
    let cfg = Config::from_env();
    assert!(
        cfg.starting_balance > 0.0,
        "starting_balance should be positive, got {}",
        cfg.starting_balance
    );
    assert!(
        cfg.latency_arb_momentum_threshold > 0.0,
        "latency_arb_momentum_threshold should be positive, got {}",
        cfg.latency_arb_momentum_threshold
    );
    assert!(
        cfg.max_position_fraction > 0.0,
        "max_position_fraction should be positive, got {}",
        cfg.max_position_fraction
    );
    assert!(
        cfg.kelly_fraction > 0.0,
        "kelly_fraction should be positive, got {}",
        cfg.kelly_fraction
    );
    assert!(
        cfg.tick_interval > 0,
        "tick_interval should be positive, got {}",
        cfg.tick_interval
    );
}

/// Verifies that set param with infinity.
#[test]
fn set_param_with_infinity() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("STARTING_BALANCE", f64::INFINITY));
    assert!(cfg.starting_balance.is_infinite());
}

/// Verifies that set param with nan.
#[test]
fn set_param_with_nan() {
    let mut cfg = Config::default();
    assert!(cfg.set_param("STARTING_BALANCE", f64::NAN));
    assert!(cfg.starting_balance.is_nan());
}

/// Verifies that set param verifies every f64 field value.
#[test]
fn set_param_verifies_every_f64_field_value() {
    let mut cfg = Config::default();
    cfg.set_param("LATENCY_ARB_MIN_ASK", 0.25);
    assert!((cfg.latency_arb_min_ask - 0.25).abs() < f64::EPSILON);

    cfg.set_param("SPREAD_CAPTURE_THRESHOLD", 0.95);
    assert!((cfg.spread_capture_threshold - 0.95).abs() < f64::EPSILON);

    cfg.set_param("SPREAD_CAPTURE_MIN_ASK", 0.20);
    assert!((cfg.spread_capture_min_ask - 0.20).abs() < f64::EPSILON);

    cfg.set_param("SPREAD_CAPTURE_MAX_QUOTE_CHURN_PER_S", 50.0);
    assert!((cfg.spread_capture_max_quote_churn_per_s - 50.0).abs() < f64::EPSILON);

    cfg.set_param("SPREAD_CAPTURE_MAX_POSITION_FRACTION", 0.15);
    assert_eq!(cfg.spread_capture_max_position_fraction, Some(0.15));

    cfg.set_param("MAX_POSITION_USD_FRACTION", 0.15);
    assert!((cfg.max_position_usd_fraction - 0.15).abs() < f64::EPSILON);

    cfg.set_param("MIN_BALANCE_THRESHOLD", 50.0);
    assert!((cfg.min_balance_threshold - 50.0).abs() < f64::EPSILON);

    cfg.set_param("MAX_DRAWDOWN_PCT", 0.40);
    assert!((cfg.max_drawdown_pct - 0.40).abs() < f64::EPSILON);

    cfg.set_param("KELLY_FRACTION", 0.25);
    assert!((cfg.kelly_fraction - 0.25).abs() < f64::EPSILON);

    cfg.set_param("MIN_WIN_RATE_FOR_KELLY", 0.55);
    assert!((cfg.min_win_rate_for_kelly - 0.55).abs() < f64::EPSILON);

    cfg.set_param("MIN_KELLY_FLOOR", 0.05);
    assert!((cfg.min_kelly_floor - 0.05).abs() < f64::EPSILON);

    cfg.set_param("MIN_BET_USD", 10.0);
    assert!((cfg.min_bet_usd - 10.0).abs() < f64::EPSILON);

    cfg.set_param("TREND_FILTER_THRESHOLD", 0.50);
    assert!((cfg.trend_filter_threshold - 0.50).abs() < f64::EPSILON);
}

/// Verifies that calm and router fields are wired into set-param.
#[test]
fn set_param_supports_calm_and_router_fields() {
    let mut cfg = Config::default();

    assert!(cfg.set_param("LATENCY_ARB_ENABLED", 0.0));
    assert!(!cfg.latency_arb_enabled);
    assert!(cfg.set_param("SPREAD_CAPTURE_ENABLED", 0.0));
    assert!(!cfg.spread_capture_enabled);
    assert!(cfg.set_param("CALM_PERSISTENCE_ENABLED", 1.0));
    assert!(cfg.calm_persistence_enabled);
    assert!(cfg.set_param("REGIME_DETECTION_ENABLED", 1.0));
    assert!(cfg.regime_detection_enabled);
    assert!(cfg.set_param("TREND_FILTER_PER_STRATEGY", 1.0));
    assert!(cfg.trend_filter_per_strategy);

    assert!(cfg.set_param("LATENCY_ARB_MAX_POSITION_FRACTION", 0.04));
    assert_eq!(cfg.latency_arb_max_position_fraction, Some(0.04));
    assert!(cfg.set_param("CALM_PERSISTENCE_MAX_POSITION_FRACTION", 0.03));
    assert_eq!(cfg.calm_persistence_max_position_fraction, Some(0.03));
    assert!(cfg.set_param("CALM_PERSISTENCE_MAX_WINDOW_TIME_MS", 75_000.0));
    assert_eq!(cfg.calm_persistence_max_window_time_ms, 75_000);
    assert!(cfg.set_param("CALM_PERSISTENCE_MAX_OPEN_CROSSES_30S", 2.0));
    assert_eq!(cfg.calm_persistence_max_open_crosses_30s, 2);
    assert!(cfg.set_param("CALM_PERSISTENCE_MIN_EXPECTED_EDGE", 0.05));
    assert!((cfg.calm_persistence_min_expected_edge - 0.05).abs() < f64::EPSILON);
}
