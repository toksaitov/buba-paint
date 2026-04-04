use super::*;
use crate::clock::BacktestClock;
use crate::portfolio::StrategyFamily;
use tempfile::NamedTempFile;

/// Helper: default config tweaked for predictable test results.
fn test_config() -> Config {
    Config {
        starting_balance: 200.0,
        max_position_fraction: 0.10,
        max_position_usd_fraction: 0.20,
        min_balance_threshold: 20.0,
        max_drawdown_pct: 0.50,
        kelly_fraction: 0.5,
        min_win_rate_for_kelly: 0.52,
        min_trades_for_kelly: 20,
        min_kelly_floor: 0.03,
        min_bet_usd: 5.0,
        kelly_rolling_window: 30,
        peak_dd_pause_pct: 0.30,
        peak_dd_pause_ms: 3_600_000,
        ..Config::default()
    }
}

/// Temp db.
fn temp_db() -> (Database, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
    (db, tmp)
}

/// Builds manager.
fn make_manager(config: &Config, db: &Database, clock: &dyn Clock) -> BankrollManager {
    BankrollManager::new(config.starting_balance, config, db, clock)
}

/// Verifies that kelly fraction known inputs.
#[test]
fn kelly_fraction_known_inputs() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    let frac = mgr.get_kelly_fraction(0.45, 0.60, &cfg);
    let b = 0.55 / 0.45;
    let expected = ((b * 0.60 - 0.40) / b) * 0.5;
    assert!(
        (frac - expected).abs() < 1e-10,
        "expected {expected}, got {frac}"
    );
}

/// Verifies that kelly fraction below min wr returns floor.
#[test]
fn kelly_fraction_below_min_wr_returns_floor() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    let frac = mgr.get_kelly_fraction(0.50, 0.51, &cfg);
    assert!(
        (frac - cfg.min_kelly_floor).abs() < f64::EPSILON,
        "expected floor {}, got {frac}",
        cfg.min_kelly_floor
    );
}

/// Verifies that kelly fraction negative full kelly returns floor.
#[test]
fn kelly_fraction_negative_full_kelly_returns_floor() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    let frac = mgr.get_kelly_fraction(0.90, 0.52, &cfg);
    assert!(
        (frac - cfg.min_kelly_floor).abs() < f64::EPSILON,
        "expected floor {}, got {frac}",
        cfg.min_kelly_floor
    );
}

/// Verifies that confidence curve 0 5 gives zero.
#[test]
fn confidence_curve_0_5_gives_zero() {
    let mult = (0.5_f64 - 0.5).mul_add(2.5, 0.0).max(0.0);
    assert!((mult - 0.0).abs() < f64::EPSILON);
}

/// Verifies that confidence curve 0 6 gives 0 25.
#[test]
fn confidence_curve_0_6_gives_0_25() {
    let mult = (0.6_f64 - 0.5).mul_add(2.5, 0.0).max(0.0);
    assert!((mult - 0.25).abs() < 1e-10, "expected 0.25, got {mult}");
}

/// Verifies that confidence curve 0 9 gives 1 0.
#[test]
fn confidence_curve_0_9_gives_1_0() {
    let mult = (0.9_f64 - 0.5).mul_add(2.5, 0.0).max(0.0);
    assert!((mult - 1.0).abs() < 1e-10, "expected 1.0, got {mult}");
}

/// Verifies that confidence below 0 5 gives zero.
#[test]
fn confidence_below_0_5_gives_zero() {
    let mult = (0.3_f64 - 0.5).mul_add(2.5, 0.0).max(0.0);
    assert!((mult - 0.0).abs() < f64::EPSILON);
}

/// Verifies that reserve capital correct tokens.
#[test]
fn reserve_capital_correct_tokens() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 44.0).abs() < f64::EPSILON,
        "expected 44 tokens, got {tokens}"
    );
}

/// Verifies that reserve capital zero confidence.
#[test]
fn reserve_capital_zero_confidence() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let tokens = mgr.reserve_capital(0.45, 0.5, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 0.0).abs() < f64::EPSILON,
        "expected 0 tokens, got {tokens}"
    );
}

/// Verifies that reserve capital invalid entry price.
#[test]
fn reserve_capital_invalid_entry_price() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    assert!((mgr.reserve_capital(0.0, 0.9, "x", &cfg, &clock) - 0.0).abs() < f64::EPSILON);
    assert!((mgr.reserve_capital(-0.1, 0.9, "x", &cfg, &clock) - 0.0).abs() < f64::EPSILON);
    assert!((mgr.reserve_capital(1.0, 0.9, "x", &cfg, &clock) - 0.0).abs() < f64::EPSILON);
    assert!((mgr.reserve_capital(1.5, 0.9, "x", &cfg, &clock) - 0.0).abs() < f64::EPSILON);
}

/// Verifies that min bet floor activates.
#[test]
fn min_bet_floor_activates() {
    let mut cfg = test_config();
    cfg.starting_balance = 30.0;
    cfg.min_bet_usd = 5.0;
    cfg.max_position_fraction = 0.20;
    cfg.max_position_usd_fraction = 0.50;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    let tokens = mgr.reserve_capital(0.45, 0.55, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 12.0).abs() < f64::EPSILON,
        "expected 12 tokens (min bet floor), got {tokens}"
    );
}

/// Verifies that single-side reservation can size from the live ask while reserving at the limit.
#[test]
fn reserve_capital_with_reserve_price_honors_min_bet_at_entry_price() {
    let mut cfg = test_config();
    cfg.starting_balance = 30.0;
    cfg.min_bet_usd = 5.0;
    cfg.max_position_fraction = 0.20;
    cfg.max_position_usd_fraction = 0.50;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    let tokens = mgr.reserve_capital_with_reserve_price(
        0.56,
        0.65,
        0.9,
        "latency-arb",
        StrategyFamily::LatencyArb,
        &cfg,
        &clock,
    );
    assert!(
        (tokens - 9.0).abs() < f64::EPSILON,
        "expected 9 tokens (ceil min-bet floor), got {tokens}"
    );
    assert!(
        (mgr.reserved_capital - 5.85).abs() < 1e-10,
        "expected 5.85 reserved at limit price, got {}",
        mgr.reserved_capital
    );
}

/// Verifies that one calm reservation does not consume the latency-arb sleeve.
#[test]
fn strategy_sleeves_isolate_latency_and_calm_reservations() {
    let mut cfg = test_config();
    cfg.starting_balance = 200.0;
    cfg.max_position_fraction = 0.05;
    cfg.latency_arb_max_position_fraction = Some(0.05);
    cfg.calm_persistence_max_position_fraction = Some(0.05);

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    let calm_tokens = mgr.reserve_capital_with_reserve_price(
        0.50,
        0.50,
        1.0,
        "calm-persistence",
        StrategyFamily::CalmPersistence,
        &cfg,
        &clock,
    );
    let latency_tokens = mgr.reserve_capital_with_reserve_price(
        0.50,
        0.50,
        1.0,
        "latency-arb",
        StrategyFamily::LatencyArb,
        &cfg,
        &clock,
    );

    assert_eq!(calm_tokens, 20.0);
    assert_eq!(latency_tokens, 20.0);
    assert!((mgr.reserved_capital - 20.0).abs() < 1e-9);
}

/// Verifies that a tighter calm sleeve does not affect the latency-arb sleeve size.
#[test]
fn calm_sleeve_cap_is_independent_from_latency_sleeve() {
    let mut cfg = test_config();
    cfg.starting_balance = 200.0;
    cfg.max_position_fraction = 0.05;
    cfg.latency_arb_max_position_fraction = Some(0.05);
    cfg.calm_persistence_max_position_fraction = Some(0.02);

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    let calm_tokens = mgr.reserve_capital_with_reserve_price(
        0.50,
        0.50,
        1.0,
        "calm-persistence",
        StrategyFamily::CalmPersistence,
        &cfg,
        &clock,
    );
    let latency_tokens = mgr.reserve_capital_with_reserve_price(
        0.50,
        0.50,
        1.0,
        "latency-arb",
        StrategyFamily::LatencyArb,
        &cfg,
        &clock,
    );

    assert_eq!(calm_tokens, 8.0);
    assert_eq!(latency_tokens, 20.0);
}

/// Verifies that min bet floor does not activate when cost above min.
#[test]
fn min_bet_floor_does_not_activate_when_cost_above_min() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 44.0).abs() < f64::EPSILON,
        "expected 44 tokens (no floor needed), got {tokens}"
    );
}

/// Verifies that apply trade result win.
#[test]
fn apply_trade_result_win() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = 4.5;
    mgr.apply_trade_result(1, 0.45, 10.0, 1.0, 0.0, "latency-arb", &cfg, &db, &clock);

    assert!(
        (mgr.get_balance() - 205.5).abs() < 1e-10,
        "expected 205.5, got {}",
        mgr.get_balance()
    );
    assert_eq!(mgr.total_wins, 1);
    assert_eq!(mgr.total_losses, 0);
    assert_eq!(mgr.total_trades, 1);
    assert!((mgr.reserved_capital - 0.0).abs() < f64::EPSILON);
}

/// Verifies that apply trade result loss.
#[test]
fn apply_trade_result_loss() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = 5.5;
    mgr.apply_trade_result(1, 0.55, 10.0, 0.0, 0.0, "latency-arb", &cfg, &db, &clock);

    assert!(
        (mgr.get_balance() - 194.5).abs() < 1e-10,
        "expected 194.5, got {}",
        mgr.get_balance()
    );
    assert_eq!(mgr.total_wins, 0);
    assert_eq!(mgr.total_losses, 1);
    assert_eq!(mgr.total_trades, 1);
}

/// Verifies that dd pause triggers at threshold.
#[test]
fn dd_pause_triggers_at_threshold() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.peak_dd_pause_ms = 10_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0;

    assert!(!mgr.can_trade(&cfg, &clock));

    clock.set(10_999);
    assert!(!mgr.can_trade(&cfg, &clock));

    clock.set(11_000);
    assert!(mgr.can_trade(&cfg, &clock));
}

/// Verifies that dd pause resets when dd recovers.
#[test]
fn dd_pause_resets_when_dd_recovers() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.peak_dd_pause_ms = 10_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0;
    assert!(!mgr.can_trade(&cfg, &clock));

    mgr.current_balance = 180.0;
    clock.set(2_000);
    assert!(mgr.can_trade(&cfg, &clock));
    assert_eq!(mgr.peak_dd_pause_until, 0);
}

/// Verifies that can trade false below min balance.
#[test]
fn can_trade_false_below_min_balance() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = cfg.min_balance_threshold - 1.0;
    assert!(!mgr.can_trade(&cfg, &clock));
}

/// Verifies that can trade false at max drawdown.
#[test]
fn can_trade_false_at_max_drawdown() {
    let mut cfg = test_config();
    cfg.max_drawdown_pct = 0.50;
    cfg.peak_dd_pause_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 100.0;
    assert!(!mgr.can_trade(&cfg, &clock));
}

/// Verifies that rolling window eviction.
#[test]
fn rolling_window_eviction() {
    let mut cfg = test_config();
    cfg.kelly_rolling_window = 5;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    for i in 0..5 {
        mgr.reserved_capital = 5.0;
        mgr.apply_trade_result(
            i64::from(i) + 1,
            0.50,
            10.0,
            1.0,
            0.0,
            "latency-arb",
            &cfg,
            &db,
            &clock,
        );
    }
    assert_eq!(mgr.recent_results.len(), 5);

    mgr.reserved_capital = 5.0;
    mgr.apply_trade_result(6, 0.50, 10.0, 0.0, 0.0, "latency-arb", &cfg, &db, &clock);
    assert_eq!(mgr.recent_results.len(), 5);

    assert!(mgr.recent_results[0].won);

    assert!(!mgr.recent_results[4].won);
}

/// Verifies that strategy win rate from lifetime stats.
#[test]
#[allow(clippy::cast_possible_wrap)]
fn strategy_win_rate_from_lifetime_stats() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    for (i, won) in [true, true, false].iter().enumerate() {
        mgr.reserved_capital = 5.0;
        let settlement = if *won { 1.0 } else { 0.0 };
        mgr.apply_trade_result(
            i as i64 + 1,
            0.50,
            10.0,
            settlement,
            0.0,
            "test-strat",
            &cfg,
            &db,
            &clock,
        );
    }

    let wr = mgr.get_strategy_win_rate("test-strat");
    assert!((wr - 2.0 / 3.0).abs() < 1e-10, "expected ~0.6667, got {wr}");
}

/// Verifies that strategy win rate from rolling window.
#[test]
#[allow(clippy::cast_possible_wrap)]
fn strategy_win_rate_from_rolling_window() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let pattern = [true, true, true, false, false, true, true];
    for (i, won) in pattern.iter().enumerate() {
        mgr.reserved_capital = 5.0;
        let settlement = if *won { 1.0 } else { 0.0 };
        mgr.apply_trade_result(
            i as i64 + 1,
            0.50,
            10.0,
            settlement,
            0.0,
            "test-strat",
            &cfg,
            &db,
            &clock,
        );
    }

    let wr = mgr.get_strategy_win_rate("test-strat");
    assert!((wr - 5.0 / 7.0).abs() < 1e-10, "expected ~0.7143, got {wr}");
}

/// Verifies that strategy win rate unknown strategy returns zero.
#[test]
fn strategy_win_rate_unknown_strategy_returns_zero() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    assert!((mgr.get_strategy_win_rate("nonexistent") - 0.0).abs() < f64::EPSILON);
}

/// Verifies that strategy win rate rolling filters by strategy.
#[test]
fn strategy_win_rate_rolling_filters_by_strategy() {
    let mut cfg = test_config();
    cfg.kelly_rolling_window = 30;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    for i in 0..5 {
        mgr.reserved_capital = 5.0;
        mgr.apply_trade_result(
            i64::from(i) + 1,
            0.50,
            10.0,
            1.0,
            0.0,
            "strat-a",
            &cfg,
            &db,
            &clock,
        );
    }
    for i in 0..5 {
        mgr.reserved_capital = 5.0;
        mgr.apply_trade_result(
            i64::from(i) + 6,
            0.50,
            10.0,
            0.0,
            0.0,
            "strat-b",
            &cfg,
            &db,
            &clock,
        );
    }

    assert!(
        (mgr.get_strategy_win_rate("strat-a") - 1.0).abs() < f64::EPSILON,
        "strat-a should be 100% WR"
    );
    assert!(
        (mgr.get_strategy_win_rate("strat-b") - 0.0).abs() < f64::EPSILON,
        "strat-b should be 0% WR"
    );
}

/// Verifies that reserve spread capital basic.
#[test]
fn reserve_spread_capital_basic() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let (up, down) = mgr.reserve_spread_capital(0.48, 0.50, 1.0, &cfg, &clock);
    assert!(
        (up - 20.0).abs() < f64::EPSILON,
        "expected 20 up tokens, got {up}"
    );
    assert!(
        (down - 20.0).abs() < f64::EPSILON,
        "expected 20 down tokens, got {down}"
    );
    assert!(
        (mgr.reserved_capital - 19.6).abs() < 1e-10,
        "expected 19.6 reserved, got {}",
        mgr.reserved_capital
    );
}

/// Verifies that spread reservation falls back to the global position fraction when unset.
#[test]
fn reserve_spread_capital_falls_back_to_global_fraction() {
    let mut cfg = test_config();
    cfg.max_position_fraction = 0.05;
    cfg.spread_capture_max_position_fraction = None;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let (up, down) = mgr.reserve_spread_capital(0.48, 0.50, 1.0, &cfg, &clock);
    assert_eq!((up, down), (10.0, 10.0));
    assert!((mgr.reserved_capital - 9.8).abs() < 1e-10);
}

/// Verifies that spread reservation uses the spread-only position fraction when configured.
#[test]
fn reserve_spread_capital_uses_spread_specific_fraction() {
    let mut cfg = test_config();
    cfg.max_position_fraction = 0.05;
    cfg.spread_capture_max_position_fraction = Some(0.10);

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let (up, down) = mgr.reserve_spread_capital(0.48, 0.50, 1.0, &cfg, &clock);
    assert_eq!((up, down), (20.0, 20.0));
    assert!((mgr.reserved_capital - 19.6).abs() < 1e-10);
}

/// Verifies that live-like spread quotes fail the affordability precheck under the conservative cap.
#[test]
fn assess_spread_affordability_rejects_budget_too_small_setup() {
    let mut cfg = test_config();
    cfg.max_position_fraction = 0.05;
    cfg.spread_capture_max_position_fraction = Some(0.05);
    cfg.max_position_usd_fraction = 1.0;
    cfg.max_position_usd = 1_000.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mgr = make_manager(&cfg, &db, &clock);

    let assessment = mgr.assess_spread_affordability(0.46, 0.50, 1.0, &cfg);
    assert!(!assessment.queueable);
    assert_eq!(
        assessment.failure_reason,
        Some(SpreadAffordabilityFailureReason::SpreadBudgetTooSmall)
    );
    assert!((assessment.available_spread_budget - 10.0).abs() < 1e-10);
    assert!((assessment.required_pair_units - 11.0).abs() < f64::EPSILON);
    assert!((assessment.required_pair_notional - 10.56).abs() < 1e-10);
}

/// Verifies that the same spread becomes queueable once the spread cap is raised.
#[test]
fn assess_spread_affordability_passes_when_spread_cap_is_high_enough() {
    let mut cfg = test_config();
    cfg.max_position_fraction = 0.05;
    cfg.spread_capture_max_position_fraction = Some(0.10);
    cfg.max_position_usd_fraction = 1.0;
    cfg.max_position_usd = 1_000.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mgr = make_manager(&cfg, &db, &clock);

    let assessment = mgr.assess_spread_affordability(0.46, 0.50, 1.0, &cfg);
    assert!(assessment.queueable);
    assert_eq!(assessment.failure_reason, None);
    assert!((assessment.available_spread_budget - 20.0).abs() < 1e-10);
    assert!((assessment.required_pair_units - 11.0).abs() < f64::EPSILON);
    assert!((assessment.required_pair_notional - 10.56).abs() < 1e-10);
}

/// Verifies that spread reservation can return sub-minimum pair sizes for an explicit executor rejection.
#[test]
fn reserve_spread_capital_returns_affordable_pair_units_before_submit_guard() {
    let mut cfg = test_config();
    cfg.starting_balance = 20.0;
    cfg.max_position_fraction = 0.10;
    cfg.max_position_usd_fraction = 1.0;
    cfg.min_balance_threshold = 1.0;
    cfg.peak_dd_pause_pct = 1.0;
    cfg.max_drawdown_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    let (up, down) = mgr.reserve_spread_capital(0.64, 0.35, 1.0, &cfg, &clock);
    assert_eq!((up, down), (2.0, 2.0));
    assert!(
        (mgr.reserved_capital - 1.98).abs() < 1e-10,
        "expected 1.98 reserved, got {}",
        mgr.reserved_capital
    );
}

/// Verifies that spread reservation still respects the shared hard USD cap.
#[test]
fn reserve_spread_capital_respects_global_hard_usd_cap() {
    let mut cfg = test_config();
    cfg.max_position_fraction = 0.20;
    cfg.spread_capture_max_position_fraction = Some(0.20);
    cfg.max_position_usd_fraction = 1.0;
    cfg.max_position_usd = 8.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let (up, down) = mgr.reserve_spread_capital(0.48, 0.50, 1.0, &cfg, &clock);
    assert_eq!((up, down), (8.0, 8.0));
    assert!((mgr.reserved_capital - 7.84).abs() < 1e-10);
}

/// Verifies that reserve spread capital invalid asks.
#[test]
fn reserve_spread_capital_invalid_asks() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    assert_eq!(
        mgr.reserve_spread_capital(0.0, 0.50, 1.0, &cfg, &clock),
        (0.0, 0.0)
    );
    assert_eq!(
        mgr.reserve_spread_capital(0.50, 0.0, 1.0, &cfg, &clock),
        (0.0, 0.0)
    );
    assert_eq!(
        mgr.reserve_spread_capital(1.0, 0.50, 1.0, &cfg, &clock),
        (0.0, 0.0)
    );
    assert_eq!(
        mgr.reserve_spread_capital(0.50, 1.0, 1.0, &cfg, &clock),
        (0.0, 0.0)
    );
}

/// Verifies that high water mark updates.
#[test]
fn high_water_mark_updates() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = 5.0;
    mgr.apply_trade_result(1, 0.50, 10.0, 1.0, 0.0, "latency-arb", &cfg, &db, &clock);

    assert!(
        (mgr.high_water_mark - 205.0).abs() < 1e-10,
        "expected HWM 205.0, got {}",
        mgr.high_water_mark
    );
}

/// Verifies that constructor recovers balance from db.
#[test]
fn constructor_recovers_balance_from_db() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);

    db.log_balance_event(1_000, "init", None, 0.0, 200.0)
        .unwrap();
    db.log_balance_event(2_000, "trade_close", None, 50.0, 250.0)
        .unwrap();

    let mgr = BankrollManager::new(200.0, &cfg, &db, &clock);
    assert!(
        (mgr.get_balance() - 250.0).abs() < f64::EPSILON,
        "expected recovered balance 250.0, got {}",
        mgr.get_balance()
    );
    assert!(
        (mgr.high_water_mark - 250.0).abs() < f64::EPSILON,
        "expected HWM max(200, 250) = 250.0, got {}",
        mgr.high_water_mark
    );
}

/// Verifies that constructor logs init on fresh db.
#[test]
fn constructor_logs_init_on_fresh_db() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(5_000);

    let mgr = BankrollManager::new(200.0, &cfg, &db, &clock);
    assert!((mgr.get_balance() - 200.0).abs() < f64::EPSILON);

    let latest = db.get_latest_balance().unwrap();
    assert_eq!(latest, Some(200.0));
}

/// Verifies that get stats snapshot.
#[test]
fn get_stats_snapshot() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = 5.0;
    mgr.apply_trade_result(1, 0.50, 10.0, 1.0, 0.0, "s", &cfg, &db, &clock);
    mgr.reserved_capital = 5.0;
    mgr.apply_trade_result(2, 0.50, 10.0, 0.0, 0.0, "s", &cfg, &db, &clock);

    let stats = mgr.get_stats();
    assert!((stats.starting_balance - 200.0).abs() < f64::EPSILON);
    assert_eq!(stats.total_trades, 2);
    assert_eq!(stats.wins, 1);
    assert_eq!(stats.losses, 1);
    assert!((stats.win_rate - 0.5).abs() < f64::EPSILON);

    assert!(
        (stats.total_pnl - 0.0).abs() < 1e-10,
        "expected 0.0 total PnL, got {}",
        stats.total_pnl
    );
}

/// Verifies that drawdown pct correct.
#[test]
fn drawdown_pct_correct() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 150.0;

    assert!(
        (mgr.get_drawdown_pct() - 0.25).abs() < f64::EPSILON,
        "expected 0.25, got {}",
        mgr.get_drawdown_pct()
    );
}

/// Verifies that drawdown pct zero when at hwm.
#[test]
fn drawdown_pct_zero_when_at_hwm() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    assert!((mgr.get_drawdown_pct() - 0.0).abs() < f64::EPSILON);
}

/// Verifies that win rate no trades.
#[test]
fn win_rate_no_trades() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    assert!((mgr.get_win_rate() - 0.0).abs() < f64::EPSILON);
}

/// Verifies that win rate after trades.
#[test]
#[allow(clippy::cast_possible_wrap)]
fn win_rate_after_trades() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    for (i, won) in [true, true, true, false].iter().enumerate() {
        mgr.reserved_capital = 5.0;
        let settlement = if *won { 1.0 } else { 0.0 };
        mgr.apply_trade_result(
            i as i64 + 1,
            0.50,
            10.0,
            settlement,
            0.0,
            "s",
            &cfg,
            &db,
            &clock,
        );
    }

    assert!(
        (mgr.get_win_rate() - 0.75).abs() < f64::EPSILON,
        "expected 0.75, got {}",
        mgr.get_win_rate()
    );
}

/// Verifies that reserve capital depletes available.
#[test]
fn reserve_capital_depletes_available() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let t1 = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(t1 > 0.0);
    let reserved_after_first = mgr.reserved_capital;
    assert!(reserved_after_first > 0.0);

    let t2 = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);

    assert!(
        t2 > 0.0,
        "legacy global sizing should allow another reservation"
    );
    assert!(mgr.reserved_capital > reserved_after_first);
}

/// Verifies that explicit family sleeves clamp repeated reservations for that strategy.
#[test]
fn explicit_latency_sleeve_blocks_second_same_family_reservation() {
    let mut cfg = test_config();
    cfg.max_position_fraction = 0.05;
    cfg.latency_arb_max_position_fraction = Some(0.05);

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let t1 = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(t1 > 0.0);
    let reserved_after_first = mgr.reserved_capital;

    let t2 = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);

    assert!((t2 - 0.0).abs() < f64::EPSILON);
    assert!((mgr.reserved_capital - reserved_after_first).abs() < f64::EPSILON);
}

/// Verifies that apply trade result reserved capital clamped to zero.
#[test]
fn apply_trade_result_reserved_capital_clamped_to_zero() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = 1.0;

    mgr.apply_trade_result(1, 0.50, 10.0, 1.0, 0.0, "latency-arb", &cfg, &db, &clock);
    assert!(
        mgr.reserved_capital >= 0.0,
        "reserved_capital should never go negative, got {}",
        mgr.reserved_capital
    );
    assert!(
        (mgr.reserved_capital - 0.0).abs() < f64::EPSILON,
        "expected clamped to 0.0, got {}",
        mgr.reserved_capital
    );
}

/// Verifies that can trade false at exact max drawdown boundary.
#[test]
fn can_trade_false_at_exact_max_drawdown_boundary() {
    let mut cfg = test_config();
    cfg.max_drawdown_pct = 0.50;
    cfg.peak_dd_pause_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 100.0;
    assert!(!mgr.can_trade(&cfg, &clock));
}

/// Verifies that can trade true just below max drawdown.
#[test]
fn can_trade_true_just_below_max_drawdown() {
    let mut cfg = test_config();
    cfg.max_drawdown_pct = 0.50;
    cfg.peak_dd_pause_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 100.01;
    assert!(mgr.can_trade(&cfg, &clock));
}

/// Verifies that reserve capital returns zero when balance at min threshold.
#[test]
fn reserve_capital_returns_zero_when_balance_at_min_threshold() {
    let mut cfg = test_config();
    cfg.min_balance_threshold = 20.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    mgr.current_balance = 20.0;
    mgr.high_water_mark = 200.0;
    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 0.0).abs() < f64::EPSILON,
        "expected 0 tokens at min balance, got {tokens}"
    );
}

/// Verifies that reserve capital returns zero when all capital reserved.
#[test]
fn reserve_capital_returns_zero_when_all_capital_reserved() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = mgr.current_balance;
    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 0.0).abs() < f64::EPSILON,
        "expected 0 tokens when fully reserved, got {tokens}"
    );
}

/// Verifies that reserve spread capital returns zero when all reserved.
#[test]
fn reserve_spread_capital_returns_zero_when_all_reserved() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = mgr.current_balance;
    let (up, down) = mgr.reserve_spread_capital(0.48, 0.50, 1.0, &cfg, &clock);
    assert!(
        (up - 0.0).abs() < f64::EPSILON && (down - 0.0).abs() < f64::EPSILON,
        "expected (0, 0) when fully reserved"
    );
}

/// Verifies that get drawdown pct zero hwm.
#[test]
fn get_drawdown_pct_zero_hwm() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.high_water_mark = 0.0;
    assert!(
        (mgr.get_drawdown_pct() - 0.0).abs() < f64::EPSILON,
        "drawdown with zero HWM should be 0.0"
    );
}

/// Verifies that peak drawdown tracked after loss.
#[test]
fn peak_drawdown_tracked_after_loss() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = 5.5;
    mgr.apply_trade_result(1, 0.55, 10.0, 0.0, 0.0, "latency-arb", &cfg, &db, &clock);

    assert!(
        mgr.peak_drawdown_pct > 0.0,
        "peak DD should be tracked after a loss"
    );
    let expected_dd = (200.0 - 194.5) / 200.0;
    assert!(
        (mgr.peak_drawdown_pct - expected_dd).abs() < 1e-10,
        "expected {expected_dd}, got {}",
        mgr.peak_drawdown_pct
    );
}

/// Verifies that reserve capital negative ask returns zero.
#[test]
fn reserve_capital_negative_ask_returns_zero() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let (up, down) = mgr.reserve_spread_capital(-0.1, 0.50, 1.0, &cfg, &clock);
    assert!(
        (up - 0.0).abs() < f64::EPSILON && (down - 0.0).abs() < f64::EPSILON,
        "negative up_ask should return (0, 0)"
    );
}

/// Verifies that min bet floor skipped when min tokens exceed available.
#[test]
fn min_bet_floor_skipped_when_min_tokens_exceed_available() {
    let mut cfg = test_config();
    cfg.starting_balance = 10.0;
    cfg.min_bet_usd = 50.0;
    cfg.max_position_fraction = 0.10;
    cfg.max_position_usd_fraction = 1.0;
    cfg.min_balance_threshold = 1.0;
    cfg.peak_dd_pause_pct = 1.0;
    cfg.max_drawdown_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        tokens > 0.0 && tokens < 50.0,
        "expected small token count (floor skipped), got {tokens}"
    );
}

/// Verifies that strategy stats zero total returns zero wr.
#[test]
fn strategy_stats_zero_total_returns_zero_wr() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.strategy_stats.insert(
        "empty-strat".to_string(),
        StrategyRecord { wins: 0, losses: 0 },
    );
    let wr = mgr.get_strategy_win_rate("empty-strat");
    assert!(
        (wr - 0.0).abs() < f64::EPSILON,
        "expected 0.0 WR for empty strategy, got {wr}"
    );
}

/// Verifies that dd pause does not retrigger at threshold.
#[test]
fn dd_pause_does_not_retrigger_at_threshold() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.dd_pause_recovery_pct = 0.05;
    cfg.peak_dd_pause_ms = 10_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0;

    assert!(!mgr.can_trade(&cfg, &clock));

    clock.set(12_000);

    assert!(mgr.can_trade(&cfg, &clock));

    clock.set(13_000);
    assert!(mgr.can_trade(&cfg, &clock));
}

/// Verifies that dd pause retriggers after full recovery.
#[test]
fn dd_pause_retriggers_after_full_recovery() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.dd_pause_recovery_pct = 0.05;
    cfg.peak_dd_pause_ms = 10_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0;
    assert!(!mgr.can_trade(&cfg, &clock));

    clock.set(12_000);
    assert!(mgr.can_trade(&cfg, &clock));

    mgr.current_balance = 160.0;
    clock.set(15_000);
    assert!(mgr.can_trade(&cfg, &clock));

    mgr.current_balance = 130.0;
    clock.set(16_000);

    assert!(!mgr.can_trade(&cfg, &clock));
}

/// Verifies that dd pause zero recovery pct still breaks loop.
#[test]
fn dd_pause_zero_recovery_pct_still_breaks_loop() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.dd_pause_recovery_pct = 0.0;
    cfg.peak_dd_pause_ms = 10_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0;
    assert!(!mgr.can_trade(&cfg, &clock));

    clock.set(12_000);

    assert!(mgr.can_trade(&cfg, &clock));

    clock.set(13_000);
    assert!(mgr.can_trade(&cfg, &clock));
}

/// Verifies that dd pause disabled at 100 pct.
#[test]
fn dd_pause_disabled_at_100_pct() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 1.0;
    cfg.dd_pause_recovery_pct = 0.05;
    cfg.peak_dd_pause_ms = 10_000;
    cfg.max_drawdown_pct = 1.0;
    cfg.min_balance_threshold = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 10.0;

    assert!(mgr.can_trade(&cfg, &clock));
}

/// Verifies that get balance returns current balance.
#[test]
fn get_balance_returns_current_balance() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    assert!(
        (mgr.get_balance() - cfg.starting_balance).abs() < f64::EPSILON,
        "expected get_balance() = {}, got {}",
        cfg.starting_balance,
        mgr.get_balance()
    );
}

/// Verifies that get drawdown pct after loss.
#[test]
fn get_drawdown_pct_after_loss() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = 10.0;
    mgr.apply_trade_result(1, 0.50, 20.0, 0.0, 0.0, "latency-arb", &cfg, &db, &clock);

    let dd = mgr.get_drawdown_pct();
    assert!(dd > 0.0, "drawdown should be > 0 after a loss, got {dd}");
    let expected = (200.0 - 190.0) / 200.0;
    assert!(
        (dd - expected).abs() < 1e-10,
        "expected drawdown {expected}, got {dd}"
    );
}

/// Verifies that get win rate after trades.
#[test]
#[allow(clippy::cast_possible_wrap)]
fn get_win_rate_after_trades() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let pattern = [true, true, false];
    for (i, won) in pattern.iter().enumerate() {
        mgr.reserved_capital = 5.0;
        let settlement = if *won { 1.0 } else { 0.0 };
        mgr.apply_trade_result(
            i as i64 + 1,
            0.50,
            10.0,
            settlement,
            0.0,
            "latency-arb",
            &cfg,
            &db,
            &clock,
        );
    }

    let wr = mgr.get_win_rate();
    let expected = 2.0 / 3.0;
    assert!(
        (wr - expected).abs() < 1e-10,
        "expected win rate {expected}, got {wr}"
    );
}

/// Verifies that reserve spread capital asks sum exactly to balance.
#[test]
fn reserve_spread_capital_asks_sum_exactly_to_balance() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let (up, down) = mgr.reserve_spread_capital(0.50, 0.50, 1.0, &cfg, &clock);
    assert!(up > 0.0, "up tokens should be non-zero, got {up}");
    assert!(
        (up - down).abs() < f64::EPSILON,
        "up and down tokens should be equal, got up={up}, down={down}"
    );

    assert!(
        (up - 20.0).abs() < f64::EPSILON,
        "expected 20 tokens, got {up}"
    );
}

/// Verifies that confidence above one clamped by max fraction.
#[test]
fn confidence_above_one_clamped_by_max_fraction() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    let tokens = mgr.reserve_capital(0.45, 1.0, "latency-arb", &cfg, &clock);
    assert!(tokens > 0.0, "should get some tokens");

    let cost = tokens * 0.45;
    let max_notional = cfg.starting_balance * cfg.max_position_fraction;
    assert!(
        cost <= max_notional + 1e-10,
        "cost {cost} should not exceed max_notional {max_notional}"
    );
}

/// Verifies that log blocked updates timestamp.
#[test]
fn log_blocked_updates_timestamp() {
    let mut cfg = test_config();
    cfg.min_balance_threshold = 500.0;
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 1_000);
}

/// Verifies that log blocked rate limited.
#[test]
fn log_blocked_rate_limited() {
    let mut cfg = test_config();
    cfg.min_balance_threshold = 500.0;
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 1_000);

    clock.set(31_000);
    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 1_000);

    clock.set(62_000);
    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 62_000);
}

/// Verifies that log blocked max drawdown updates timestamp.
#[test]
fn log_blocked_max_drawdown_updates_timestamp() {
    let mut cfg = test_config();
    cfg.max_drawdown_pct = 0.50;
    cfg.peak_dd_pause_pct = 1.0;
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(5_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 100.0;
    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 5_000);
}

/// Verifies that log blocked dd pause updates timestamp.
#[test]
fn log_blocked_dd_pause_updates_timestamp() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.peak_dd_pause_ms = 10_000;
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(2_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0;
    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 2_000);
}

/// Verifies that log blocked not set when trading allowed.
#[test]
fn log_blocked_not_set_when_trading_allowed() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    assert!(mgr.can_trade(&cfg, &clock));
    assert_eq!(mgr.last_blocked_log_ms, 0);
}

/// Verifies that log blocked dd pause shows correct timestamp.
#[test]
fn log_blocked_dd_pause_shows_correct_timestamp() {
    let mut cfg = test_config();
    cfg.min_balance_threshold = 0.0;
    cfg.max_drawdown_pct = 1.0;
    cfg.peak_dd_pause_pct = 0.30;
    cfg.peak_dd_pause_ms = 60_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(7_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0;

    assert!(!mgr.can_trade(&cfg, &clock));
    assert_eq!(
        mgr.last_blocked_log_ms, 7_000,
        "DD pause path should set last_blocked_log_ms to current time"
    );
}

/// Verifies that log blocked not updated when trading allowed.
#[test]
fn log_blocked_not_updated_when_trading_allowed() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(10_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    assert!(mgr.can_trade(&cfg, &clock));
    assert_eq!(
        mgr.last_blocked_log_ms, 0,
        "last_blocked_log_ms should remain 0 when trading is allowed"
    );

    clock.set(100_000);
    assert!(mgr.can_trade(&cfg, &clock));
    assert_eq!(
        mgr.last_blocked_log_ms, 0,
        "last_blocked_log_ms must stay 0 when trading is never blocked"
    );
}

/// Verifies that dd pause hysteresis recovery pct greater than pause pct.
#[test]
fn dd_pause_hysteresis_recovery_pct_greater_than_pause_pct() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.dd_pause_recovery_pct = 0.40;
    cfg.peak_dd_pause_ms = 10_000;
    cfg.min_balance_threshold = 0.0;
    cfg.max_drawdown_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0;
    assert!(!mgr.can_trade(&cfg, &clock));

    clock.set(12_000);
    assert!(mgr.can_trade(&cfg, &clock));

    clock.set(13_000);
    assert!(mgr.can_trade(&cfg, &clock));

    clock.set(1_000_000);
    assert!(
        mgr.can_trade(&cfg, &clock),
        "DD pause must never re-trigger when recovery threshold is negative"
    );

    mgr.current_balance = 200.0;
    clock.set(2_000_000);
    assert!(mgr.can_trade(&cfg, &clock));

    mgr.current_balance = 130.0;
    clock.set(3_000_000);
    assert!(
        mgr.can_trade(&cfg, &clock),
        "armed should stay false because recovery_threshold is unreachable"
    );
}

/// Verifies that dd pause three full cycles.
#[test]
fn dd_pause_three_full_cycles() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.dd_pause_recovery_pct = 0.05;
    cfg.peak_dd_pause_ms = 10_000;
    cfg.min_balance_threshold = 0.0;
    cfg.max_drawdown_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mut mgr = make_manager(&cfg, &db, &clock);

    for cycle in 0..3 {
        let base = (cycle as u64) * 100_000;

        mgr.current_balance = 130.0;
        clock.set(base + 1_000);
        assert!(
            !mgr.can_trade(&cfg, &clock),
            "cycle {cycle}: DD pause should trigger"
        );

        clock.set(base + 12_000);
        assert!(
            mgr.can_trade(&cfg, &clock),
            "cycle {cycle}: pause should expire"
        );

        mgr.current_balance = 160.0;
        clock.set(base + 20_000);
        assert!(
            mgr.can_trade(&cfg, &clock),
            "cycle {cycle}: recovery should re-arm"
        );
    }

    mgr.current_balance = 130.0;
    clock.set(400_000);
    assert!(
        !mgr.can_trade(&cfg, &clock),
        "post-cycles: DD pause should still trigger after 3 full cycles"
    );
}

/// Verifies that kelly with entry price near one.
#[test]
fn kelly_with_entry_price_near_one() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    let frac = mgr.get_kelly_fraction(0.99, 0.60, &cfg);
    assert!(
        frac.is_finite(),
        "kelly fraction should be finite, got {frac}"
    );
    assert!(
        (frac - cfg.min_kelly_floor).abs() < f64::EPSILON,
        "expected floor {}, got {frac} (b is very small so full_kelly is negative)",
        cfg.min_kelly_floor
    );
}
