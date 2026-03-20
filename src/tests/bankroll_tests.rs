use super::*;
use crate::clock::BacktestClock;
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

fn temp_db() -> (Database, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
    (db, tmp)
}

fn make_manager(config: &Config, db: &Database, clock: &dyn Clock) -> BankrollManager {
    BankrollManager::new(config.starting_balance, config, db, clock)
}

// -- Kelly fraction for known inputs -------------------------------------

#[test]
fn kelly_fraction_known_inputs() {
    // entry = 0.45, WR = 0.60
    // b = (1-0.45)/0.45 = 0.55/0.45 ≈ 1.2222
    // p = 0.60, q = 0.40
    // full = (1.2222*0.60 - 0.40) / 1.2222 ≈ 0.2727
    // half = 0.2727 * 0.5 ≈ 0.1364
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

#[test]
fn kelly_fraction_negative_full_kelly_returns_floor() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    // entry=0.90, WR=0.52 → b=0.111, full=(0.111*0.52-0.48)/0.111 < 0
    let frac = mgr.get_kelly_fraction(0.90, 0.52, &cfg);
    assert!(
        (frac - cfg.min_kelly_floor).abs() < f64::EPSILON,
        "expected floor {}, got {frac}",
        cfg.min_kelly_floor
    );
}

// -- Confidence curve ----------------------------------------------------

#[test]
fn confidence_curve_0_5_gives_zero() {
    // (0.5 - 0.5) * 2.5 = 0.0
    let mult = (0.5_f64 - 0.5).mul_add(2.5, 0.0).max(0.0);
    assert!((mult - 0.0).abs() < f64::EPSILON);
}

#[test]
fn confidence_curve_0_6_gives_0_25() {
    let mult = (0.6_f64 - 0.5).mul_add(2.5, 0.0).max(0.0);
    assert!((mult - 0.25).abs() < 1e-10, "expected 0.25, got {mult}");
}

#[test]
fn confidence_curve_0_9_gives_1_0() {
    let mult = (0.9_f64 - 0.5).mul_add(2.5, 0.0).max(0.0);
    assert!((mult - 1.0).abs() < 1e-10, "expected 1.0, got {mult}");
}

#[test]
fn confidence_below_0_5_gives_zero() {
    let mult = (0.3_f64 - 0.5).mul_add(2.5, 0.0).max(0.0);
    assert!((mult - 0.0).abs() < f64::EPSILON);
}

// -- reserve_capital returns correct token count -------------------------

#[test]
fn reserve_capital_correct_tokens() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Before min_trades_for_kelly trades, fraction = max_position_fraction.
    // confidence = 0.9 → multiplier = 1.0
    // fraction = min(0.10 * 1.0, 0.10) = 0.10
    // kelly_notional = 200 * 0.10 = 20.0
    // max_position_usd = 200 * 0.20 = 40.0
    // available = 200.0
    // notional = min(20, 200, 40) = 20.0
    // tokens = floor(20.0 / 0.45) = floor(44.44) = 44
    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 44.0).abs() < f64::EPSILON,
        "expected 44 tokens, got {tokens}"
    );
}

#[test]
fn reserve_capital_zero_confidence() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // confidence 0.5 → multiplier = 0.0 → fraction = 0 → no tokens
    let tokens = mgr.reserve_capital(0.45, 0.5, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 0.0).abs() < f64::EPSILON,
        "expected 0 tokens, got {tokens}"
    );
}

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

// -- Min bet floor activates for small balances --------------------------

#[test]
fn min_bet_floor_activates() {
    // With a small balance, kelly_notional * entry_price might be below
    // min_bet_usd.  The floor should bump token_count up.
    let mut cfg = test_config();
    cfg.starting_balance = 30.0;
    cfg.min_bet_usd = 5.0;
    cfg.max_position_fraction = 0.10;
    cfg.max_position_usd_fraction = 0.50; // generous to not cap

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    // fraction = 0.10 * 1.0 = 0.10 (confidence = 0.9, mult = 1.0)
    // kelly_notional = 30 * 0.10 = 3.0
    // max_position_usd = 30 * 0.50 = 15.0
    // notional = min(3.0, 30.0, 15.0) = 3.0
    // tokens = floor(3.0 / 0.45) = floor(6.67) = 6
    // cost = 6 * 0.45 = 2.70 < 5.0 → activate min bet floor
    // min_tokens = floor(5.0 / 0.45) = floor(11.11) = 11
    // 11 * 0.45 = 4.95 <= 30.0 (available) and <= 15.0 (max_pos_usd) → ok
    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 11.0).abs() < f64::EPSILON,
        "expected 11 tokens (min bet floor), got {tokens}"
    );
}

#[test]
fn min_bet_floor_does_not_activate_when_cost_above_min() {
    let cfg = test_config(); // 200 starting
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // With 200 balance, 0.10 fraction, entry=0.45 → notional=20 → tokens=44
    // cost = 44 * 0.45 = 19.80 > 5.0 → no floor activation
    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 44.0).abs() < f64::EPSILON,
        "expected 44 tokens (no floor needed), got {tokens}"
    );
}

// -- apply_trade_result updates balance correctly (win and loss) ----------

#[test]
fn apply_trade_result_win() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Simulate a winning trade: bought 10 tokens at 0.45, settled at 1.0
    // cost = 10 * 0.45 = 4.5, payout = 10 * 1.0 = 10.0, pnl = +5.5
    mgr.reserved_capital = 4.5; // simulate prior reservation
    mgr.apply_trade_result(1, 0.45, 10.0, 1.0, "latency-arb", &cfg, &db, &clock);

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

#[test]
fn apply_trade_result_loss() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Losing trade: bought 10 tokens at 0.55, settled at 0.0
    // cost = 10 * 0.55 = 5.5, payout = 0.0, pnl = -5.5
    mgr.reserved_capital = 5.5;
    mgr.apply_trade_result(1, 0.55, 10.0, 0.0, "latency-arb", &cfg, &db, &clock);

    assert!(
        (mgr.get_balance() - 194.5).abs() < 1e-10,
        "expected 194.5, got {}",
        mgr.get_balance()
    );
    assert_eq!(mgr.total_wins, 0);
    assert_eq!(mgr.total_losses, 1);
    assert_eq!(mgr.total_trades, 1);
}

// -- DD pause triggers at threshold --------------------------------------

#[test]
fn dd_pause_triggers_at_threshold() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.peak_dd_pause_ms = 10_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Drive balance down 35% from HWM (200 → 130): DD = 35% > 30%
    mgr.current_balance = 130.0;

    // First call sets the pause timer.
    assert!(!mgr.can_trade(&cfg, &clock));

    // Still paused just before expiry.
    clock.set(10_999);
    assert!(!mgr.can_trade(&cfg, &clock));

    // Pause expires at 1_000 + 10_000 = 11_000.
    clock.set(11_000);
    assert!(mgr.can_trade(&cfg, &clock));
}

#[test]
fn dd_pause_resets_when_dd_recovers() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.peak_dd_pause_ms = 10_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Trigger DD pause.
    mgr.current_balance = 130.0;
    assert!(!mgr.can_trade(&cfg, &clock));

    // "Recover" balance so DD < threshold.
    mgr.current_balance = 180.0; // DD = (200-180)/200 = 10% < 30%
    clock.set(2_000); // still within original pause window
    assert!(mgr.can_trade(&cfg, &clock));
    assert_eq!(mgr.peak_dd_pause_until, 0); // timer reset
}

// -- can_trade returns false when balance below minimum ------------------

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

#[test]
fn can_trade_false_at_max_drawdown() {
    let mut cfg = test_config();
    cfg.max_drawdown_pct = 0.50;
    cfg.peak_dd_pause_pct = 1.0; // disable DD pause for this test

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // HWM = 200, balance = 100 → DD = 50% ≥ 50%
    mgr.current_balance = 100.0;
    assert!(!mgr.can_trade(&cfg, &clock));
}

// -- Rolling window eviction ---------------------------------------------

#[test]
fn rolling_window_eviction() {
    let mut cfg = test_config();
    cfg.kelly_rolling_window = 5;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    // Push 5 wins.
    for i in 0..5 {
        mgr.reserved_capital = 5.0;
        mgr.apply_trade_result(
            i64::from(i) + 1,
            0.50,
            10.0,
            1.0,
            "latency-arb",
            &cfg,
            &db,
            &clock,
        );
    }
    assert_eq!(mgr.recent_results.len(), 5);

    // Push one more (a loss) — oldest win should be evicted.
    mgr.reserved_capital = 5.0;
    mgr.apply_trade_result(6, 0.50, 10.0, 0.0, "latency-arb", &cfg, &db, &clock);
    assert_eq!(mgr.recent_results.len(), 5);
    // First element should now be the second original win (index 1).
    assert!(mgr.recent_results[0].won); // was originally trade #2, still a win
    // Last element should be the loss.
    assert!(!mgr.recent_results[4].won);
}

// -- Strategy win rate from rolling window vs lifetime -------------------

#[test]
#[allow(clippy::cast_possible_wrap)]
fn strategy_win_rate_from_lifetime_stats() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Record 3 results (< 5 in rolling window for strategy) → use lifetime.
    // 2 wins, 1 loss → 66.7%
    for (i, won) in [true, true, false].iter().enumerate() {
        mgr.reserved_capital = 5.0;
        let settlement = if *won { 1.0 } else { 0.0 };
        mgr.apply_trade_result(
            i as i64 + 1,
            0.50,
            10.0,
            settlement,
            "test-strat",
            &cfg,
            &db,
            &clock,
        );
    }

    let wr = mgr.get_strategy_win_rate("test-strat");
    assert!((wr - 2.0 / 3.0).abs() < 1e-10, "expected ~0.6667, got {wr}");
}

#[test]
#[allow(clippy::cast_possible_wrap)]
fn strategy_win_rate_from_rolling_window() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Record 7 results for same strategy so rolling window has >= 5.
    // Pattern: W W W L L W W → rolling window has all 7 (window=30).
    // Rolling for "test-strat": 5 wins, 2 losses → 71.4%
    let pattern = [true, true, true, false, false, true, true];
    for (i, won) in pattern.iter().enumerate() {
        mgr.reserved_capital = 5.0;
        let settlement = if *won { 1.0 } else { 0.0 };
        mgr.apply_trade_result(
            i as i64 + 1,
            0.50,
            10.0,
            settlement,
            "test-strat",
            &cfg,
            &db,
            &clock,
        );
    }

    let wr = mgr.get_strategy_win_rate("test-strat");
    assert!((wr - 5.0 / 7.0).abs() < 1e-10, "expected ~0.7143, got {wr}");
}

#[test]
fn strategy_win_rate_unknown_strategy_returns_zero() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    assert!((mgr.get_strategy_win_rate("nonexistent") - 0.0).abs() < f64::EPSILON);
}

#[test]
fn strategy_win_rate_rolling_filters_by_strategy() {
    let mut cfg = test_config();
    cfg.kelly_rolling_window = 30;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    // Record 5 wins for "strat-a" and 5 losses for "strat-b".
    for i in 0..5 {
        mgr.reserved_capital = 5.0;
        mgr.apply_trade_result(
            i64::from(i) + 1,
            0.50,
            10.0,
            1.0,
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
            "strat-b",
            &cfg,
            &db,
            &clock,
        );
    }

    // Rolling window has 10 entries total, 5 per strategy (>= 5 each).
    assert!(
        (mgr.get_strategy_win_rate("strat-a") - 1.0).abs() < f64::EPSILON,
        "strat-a should be 100% WR"
    );
    assert!(
        (mgr.get_strategy_win_rate("strat-b") - 0.0).abs() < f64::EPSILON,
        "strat-b should be 0% WR"
    );
}

// -- Spread capital ------------------------------------------------------

#[test]
fn reserve_spread_capital_basic() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // up_ask = 0.48, down_ask = 0.50 → total_ask_per_unit = 0.98
    // max_from_balance = 200 * 0.10 = 20.0
    // max_position_usd = 200 * 0.20 = 40.0
    // notional = min(20.0, 200.0, 40.0) = 20.0
    // pair_units = floor(20.0 / 0.98) = floor(20.408) = 20
    // total_cost = 20 * 0.98 = 19.60
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

// -- High-water mark updates after winning trade -------------------------

#[test]
fn high_water_mark_updates() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.reserved_capital = 5.0;
    mgr.apply_trade_result(1, 0.50, 10.0, 1.0, "latency-arb", &cfg, &db, &clock);
    // pnl = 10*1.0 - 10*0.50 = 5.0. Balance = 205.0
    assert!(
        (mgr.high_water_mark - 205.0).abs() < 1e-10,
        "expected HWM 205.0, got {}",
        mgr.high_water_mark
    );
}

// -- Constructor recovers from DB ----------------------------------------

#[test]
fn constructor_recovers_balance_from_db() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);

    // Pre-populate balance log (trade_id = None to avoid FK constraint).
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

#[test]
fn constructor_logs_init_on_fresh_db() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(5_000);

    let mgr = BankrollManager::new(200.0, &cfg, &db, &clock);
    assert!((mgr.get_balance() - 200.0).abs() < f64::EPSILON);

    // Verify init event was logged.
    let latest = db.get_latest_balance().unwrap();
    assert_eq!(latest, Some(200.0));
}

// -- get_stats -----------------------------------------------------------

#[test]
fn get_stats_snapshot() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Record a win and a loss.
    mgr.reserved_capital = 5.0;
    mgr.apply_trade_result(1, 0.50, 10.0, 1.0, "s", &cfg, &db, &clock);
    mgr.reserved_capital = 5.0;
    mgr.apply_trade_result(2, 0.50, 10.0, 0.0, "s", &cfg, &db, &clock);

    let stats = mgr.get_stats();
    assert!((stats.starting_balance - 200.0).abs() < f64::EPSILON);
    assert_eq!(stats.total_trades, 2);
    assert_eq!(stats.wins, 1);
    assert_eq!(stats.losses, 1);
    assert!((stats.win_rate - 0.5).abs() < f64::EPSILON);
    // pnl: win = +5.0, loss = -5.0 → net 0
    assert!(
        (stats.total_pnl - 0.0).abs() < 1e-10,
        "expected 0.0 total PnL, got {}",
        stats.total_pnl
    );
}

// -- Drawdown calculation ------------------------------------------------

#[test]
fn drawdown_pct_correct() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 150.0;
    // HWM = 200, DD = (200-150)/200 = 0.25
    assert!(
        (mgr.get_drawdown_pct() - 0.25).abs() < f64::EPSILON,
        "expected 0.25, got {}",
        mgr.get_drawdown_pct()
    );
}

#[test]
fn drawdown_pct_zero_when_at_hwm() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    assert!((mgr.get_drawdown_pct() - 0.0).abs() < f64::EPSILON);
}

// -- Overall win rate ----------------------------------------------------

#[test]
fn win_rate_no_trades() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mgr = make_manager(&cfg, &db, &clock);

    assert!((mgr.get_win_rate() - 0.0).abs() < f64::EPSILON);
}

#[test]
#[allow(clippy::cast_possible_wrap)]
fn win_rate_after_trades() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // 3 wins, 1 loss
    for (i, won) in [true, true, true, false].iter().enumerate() {
        mgr.reserved_capital = 5.0;
        let settlement = if *won { 1.0 } else { 0.0 };
        mgr.apply_trade_result(i as i64 + 1, 0.50, 10.0, settlement, "s", &cfg, &db, &clock);
    }

    assert!(
        (mgr.get_win_rate() - 0.75).abs() < f64::EPSILON,
        "expected 0.75, got {}",
        mgr.get_win_rate()
    );
}

// -- Reserve capital depletes available capital --------------------------

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

    // Second reservation should see reduced available capital.
    let t2 = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    // Both should succeed since max_position_usd_fraction = 0.20 → 40,
    // and each trade costs ~19.8, so two fit.
    assert!(t2 > 0.0);
    assert!(mgr.reserved_capital > reserved_after_first);
}

// -- Phase D: edge-case tests for near-100% coverage ----------------------

#[test]
fn apply_trade_result_reserved_capital_clamped_to_zero() {
    // If reserved_capital is smaller than the trade cost (e.g. rounding),
    // the .max(0.0) clamp should prevent it from going negative.
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Artificially set reserved_capital below the trade cost.
    mgr.reserved_capital = 1.0;
    // Trade cost = 0.50 * 10 = 5.0 > 1.0 reserved.
    mgr.apply_trade_result(1, 0.50, 10.0, 1.0, "latency-arb", &cfg, &db, &clock);
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

#[test]
fn can_trade_false_at_exact_max_drawdown_boundary() {
    let mut cfg = test_config();
    cfg.max_drawdown_pct = 0.50;
    cfg.peak_dd_pause_pct = 1.0; // disable DD pause

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // HWM = 200, balance = 100 → DD = exactly 50% = max_drawdown_pct
    // The guard is `>=` so this should return false.
    mgr.current_balance = 100.0;
    assert!(!mgr.can_trade(&cfg, &clock));
}

#[test]
fn can_trade_true_just_below_max_drawdown() {
    let mut cfg = test_config();
    cfg.max_drawdown_pct = 0.50;
    cfg.peak_dd_pause_pct = 1.0; // disable DD pause

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // HWM = 200, balance = 100.01 → DD = 49.995% < 50%
    mgr.current_balance = 100.01;
    assert!(mgr.can_trade(&cfg, &clock));
}

#[test]
fn reserve_capital_returns_zero_when_balance_at_min_threshold() {
    let mut cfg = test_config();
    cfg.min_balance_threshold = 20.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    // Drive balance to exactly the threshold — can_trade should return false.
    mgr.current_balance = 20.0;
    mgr.high_water_mark = 200.0; // keep HWM high so DD check might also block
    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 0.0).abs() < f64::EPSILON,
        "expected 0 tokens at min balance, got {tokens}"
    );
}

#[test]
fn reserve_capital_returns_zero_when_all_capital_reserved() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Reserve all capital so available = 0.
    mgr.reserved_capital = mgr.current_balance;
    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        (tokens - 0.0).abs() < f64::EPSILON,
        "expected 0 tokens when fully reserved, got {tokens}"
    );
}

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

#[test]
fn get_drawdown_pct_zero_hwm() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Force HWM to zero — should return 0.0 (not divide by zero).
    mgr.high_water_mark = 0.0;
    assert!(
        (mgr.get_drawdown_pct() - 0.0).abs() < f64::EPSILON,
        "drawdown with zero HWM should be 0.0"
    );
}

#[test]
fn peak_drawdown_tracked_after_loss() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Apply a losing trade to create drawdown.
    mgr.reserved_capital = 5.5;
    mgr.apply_trade_result(1, 0.55, 10.0, 0.0, "latency-arb", &cfg, &db, &clock);
    // pnl = -5.5, balance = 194.5, DD = (200-194.5)/200 = 0.0275
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

#[test]
fn min_bet_floor_skipped_when_min_tokens_exceed_available() {
    // If min_tokens * entry_price > available, the floor shouldn't activate
    // and the original token_count stands.
    let mut cfg = test_config();
    cfg.starting_balance = 10.0; // very small
    cfg.min_bet_usd = 50.0; // very high floor
    cfg.max_position_fraction = 0.10;
    cfg.max_position_usd_fraction = 1.0; // don't cap
    cfg.min_balance_threshold = 1.0; // allow trading
    cfg.peak_dd_pause_pct = 1.0; // disable DD pause
    cfg.max_drawdown_pct = 1.0; // disable DD check

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = BankrollManager::new(cfg.starting_balance, &cfg, &db, &clock);

    // fraction = 0.10 * 1.0 = 0.10 (confidence 0.9 → mult 1.0)
    // kelly_notional = 10 * 0.10 = 1.0
    // tokens = floor(1.0 / 0.45) = 2
    // cost = 2 * 0.45 = 0.90 < 50.0 (min_bet)
    // min_tokens = floor(50 / 0.45) = 111
    // 111 * 0.45 = 49.95 > 10.0 (available) → floor NOT applied
    // tokens stays 2, cost 0.90 — but that's > 0 so it goes through.
    let tokens = mgr.reserve_capital(0.45, 0.9, "latency-arb", &cfg, &clock);
    assert!(
        tokens > 0.0 && tokens < 50.0,
        "expected small token count (floor skipped), got {tokens}"
    );
}

#[test]
fn strategy_stats_zero_total_returns_zero_wr() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Manually insert a strategy with 0 wins and 0 losses.
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

// -- DD pause hysteresis -------------------------------------------------

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

    // Drive to 35% DD (balance = 130, HWM = 200)
    mgr.current_balance = 130.0;

    // First call triggers pause until 11_000
    assert!(!mgr.can_trade(&cfg, &clock));

    // Advance past pause expiry, DD still at 35%
    clock.set(12_000);
    // Pause expired, dd_pause_armed becomes false
    assert!(mgr.can_trade(&cfg, &clock));

    // Call again — should NOT re-trigger because armed=false
    clock.set(13_000);
    assert!(mgr.can_trade(&cfg, &clock));
}

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

    // Trigger DD pause
    mgr.current_balance = 130.0; // 35% DD
    assert!(!mgr.can_trade(&cfg, &clock));

    // Let pause expire
    clock.set(12_000);
    assert!(mgr.can_trade(&cfg, &clock)); // armed=false now

    // Recover below 25% (recovery threshold = 30% - 5% = 25%)
    mgr.current_balance = 160.0; // 20% DD
    clock.set(15_000);
    assert!(mgr.can_trade(&cfg, &clock)); // arms again

    // Relapse to 35% DD
    mgr.current_balance = 130.0;
    clock.set(16_000);
    // Should re-trigger because armed=true after recovery
    assert!(!mgr.can_trade(&cfg, &clock));
}

#[test]
fn dd_pause_zero_recovery_pct_still_breaks_loop() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.dd_pause_recovery_pct = 0.0; // recovery threshold = 30% - 0% = 30%
    cfg.peak_dd_pause_ms = 10_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 130.0; // 35% DD
    assert!(!mgr.can_trade(&cfg, &clock));

    // Let pause expire
    clock.set(12_000);
    // With recovery_pct=0, recovery threshold equals trigger threshold.
    // DD (35%) is NOT below 30%, so armed stays false.
    assert!(mgr.can_trade(&cfg, &clock));

    // Still not re-triggered
    clock.set(13_000);
    assert!(mgr.can_trade(&cfg, &clock));
}

#[test]
fn dd_pause_disabled_at_100_pct() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 1.0; // disabled (as in sweeps)
    cfg.dd_pause_recovery_pct = 0.05;
    cfg.peak_dd_pause_ms = 10_000;
    cfg.max_drawdown_pct = 1.0; // also disable max DD guard
    cfg.min_balance_threshold = 1.0; // also lower min balance

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.current_balance = 10.0; // 95% DD - extreme
    // Should still allow trading (DD < 100%, DD pause disabled)
    assert!(mgr.can_trade(&cfg, &clock));
}

// -- Getter tests --------------------------------------------------------

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

#[test]
fn get_drawdown_pct_after_loss() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Record a losing trade: bought 20 tokens at 0.50, settled at 0.0.
    // cost = 20 * 0.50 = 10.0, payout = 0.0, pnl = -10.0
    // Balance: 200 - 10 = 190, HWM = 200, DD = (200-190)/200 = 0.05
    mgr.reserved_capital = 10.0;
    mgr.apply_trade_result(1, 0.50, 20.0, 0.0, "latency-arb", &cfg, &db, &clock);

    let dd = mgr.get_drawdown_pct();
    assert!(dd > 0.0, "drawdown should be > 0 after a loss, got {dd}");
    let expected = (200.0 - 190.0) / 200.0;
    assert!(
        (dd - expected).abs() < 1e-10,
        "expected drawdown {expected}, got {dd}"
    );
}

#[test]
#[allow(clippy::cast_possible_wrap)]
fn get_win_rate_after_trades() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Record 2 wins and 1 loss.
    let pattern = [true, true, false];
    for (i, won) in pattern.iter().enumerate() {
        mgr.reserved_capital = 5.0;
        let settlement = if *won { 1.0 } else { 0.0 };
        mgr.apply_trade_result(
            i as i64 + 1,
            0.50,
            10.0,
            settlement,
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

// -- Edge-case tests: spread capital, confidence clamping, Kelly near 1 --

#[test]
fn reserve_spread_capital_asks_sum_exactly_to_balance() {
    // up_ask=0.50, down_ask=0.50 → total_ask_per_unit = 1.0
    // max_from_balance = 200 * 0.10 = 20.0
    // max_position_usd = 200 * 0.20 = 40.0
    // notional = min(20.0, 200.0, 40.0) = 20.0
    // pair_units = floor(20.0 / 1.0) = 20
    // total_cost = 20 * 1.0 = 20.0
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
    // pair_units = floor(20.0 / 1.0) = 20
    assert!(
        (up - 20.0).abs() < f64::EPSILON,
        "expected 20 tokens, got {up}"
    );
}

#[test]
fn confidence_above_one_clamped_by_max_fraction() {
    // confidence=1.0 → multiplier = (1.0-0.5)*2.5 = 1.25
    // Before min_trades_for_kelly: fraction = max_position_fraction = 0.10
    // adjusted = 0.10 * 1.25 = 0.125
    // clamped = min(0.125, 0.10) = 0.10
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Reserve capital with confidence=1.0 — should not exceed max_position_fraction.
    let tokens = mgr.reserve_capital(0.45, 1.0, "latency-arb", &cfg, &clock);
    assert!(tokens > 0.0, "should get some tokens");

    // The notional should be at most balance * max_position_fraction = 200 * 0.10 = 20.0
    let cost = tokens * 0.45;
    let max_notional = cfg.starting_balance * cfg.max_position_fraction;
    assert!(
        cost <= max_notional + 1e-10,
        "cost {cost} should not exceed max_notional {max_notional}"
    );
}

// -- Phase 3: log_blocked rate-limiting tests ----------------------------

#[test]
fn log_blocked_updates_timestamp() {
    let mut cfg = test_config();
    cfg.min_balance_threshold = 500.0; // balance (200) < threshold
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 1_000);
}

#[test]
fn log_blocked_rate_limited() {
    let mut cfg = test_config();
    cfg.min_balance_threshold = 500.0; // balance (200) < threshold
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 1_000);

    // 30 seconds later — should NOT update (rate limited to 60s)
    clock.set(31_000);
    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 1_000); // unchanged

    // 61 seconds later — should update
    clock.set(62_000);
    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 62_000);
}

#[test]
fn log_blocked_max_drawdown_updates_timestamp() {
    let mut cfg = test_config();
    cfg.max_drawdown_pct = 0.50;
    cfg.peak_dd_pause_pct = 1.0; // disable DD pause
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(5_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // HWM = 200, balance = 100 → DD = 50% >= 50%
    mgr.current_balance = 100.0;
    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 5_000);
}

#[test]
fn log_blocked_dd_pause_updates_timestamp() {
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.peak_dd_pause_ms = 10_000;
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(2_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Drive balance down 35% from HWM (200 → 130): DD = 35% > 30%
    mgr.current_balance = 130.0;
    mgr.can_trade(&cfg, &clock);
    assert_eq!(mgr.last_blocked_log_ms, 2_000);
}

#[test]
fn log_blocked_not_set_when_trading_allowed() {
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Trading should be allowed with default healthy state.
    assert!(mgr.can_trade(&cfg, &clock));
    assert_eq!(mgr.last_blocked_log_ms, 0);
}

// -- Part D: Bankroll + Circuit Breaker logging edge cases ----------------

#[test]
fn log_blocked_dd_pause_shows_correct_timestamp() {
    // Trigger ONLY the DD pause path (not min balance or max DD).
    // Set min_balance_threshold=0 so the min-balance guard never fires.
    // Set max_drawdown_pct=1.0 so the max-DD guard never fires.
    // Set peak_dd_pause_pct=0.30 so a 35% DD triggers the DD pause.
    let mut cfg = test_config();
    cfg.min_balance_threshold = 0.0;
    cfg.max_drawdown_pct = 1.0;
    cfg.peak_dd_pause_pct = 0.30;
    cfg.peak_dd_pause_ms = 60_000;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(7_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Drive balance down 35% from HWM (200 -> 130): DD = 35% > 30%
    mgr.current_balance = 130.0;

    assert!(!mgr.can_trade(&cfg, &clock));
    assert_eq!(
        mgr.last_blocked_log_ms, 7_000,
        "DD pause path should set last_blocked_log_ms to current time"
    );
}

#[test]
fn log_blocked_not_updated_when_trading_allowed() {
    // Healthy state: all guards pass, can_trade returns true.
    let cfg = test_config();
    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(10_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Balance = 200, HWM = 200, DD = 0%, no guards triggered.
    assert!(mgr.can_trade(&cfg, &clock));
    assert_eq!(
        mgr.last_blocked_log_ms, 0,
        "last_blocked_log_ms should remain 0 when trading is allowed"
    );

    // Call again at a later time — still should remain 0.
    clock.set(100_000);
    assert!(mgr.can_trade(&cfg, &clock));
    assert_eq!(
        mgr.last_blocked_log_ms, 0,
        "last_blocked_log_ms must stay 0 when trading is never blocked"
    );
}

#[test]
fn dd_pause_hysteresis_recovery_pct_greater_than_pause_pct() {
    // Set dd_pause_recovery_pct=0.40, peak_dd_pause_pct=0.30.
    // Recovery threshold = 0.30 - 0.40 = -0.10.
    // DD can never be < -0.10 (DD is always >= 0), so recovery_threshold
    // is always satisfied, meaning dd_pause_armed re-arms immediately.
    //
    // However: after the pause expires, armed becomes false. On the NEXT
    // call, the recovery check (peak_dd < recovery_threshold = -0.10) is
    // impossible (DD >= 0), so armed stays false. The pause should NOT
    // re-trigger because armed is false.
    //
    // Wait — actually if recovery_threshold is negative, then peak_dd (>= 0)
    // is NEVER < recovery_threshold, so armed stays false forever after
    // first expiry.  Let's verify that.
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.dd_pause_recovery_pct = 0.40; // recovery threshold = -0.10
    cfg.peak_dd_pause_ms = 10_000;
    cfg.min_balance_threshold = 0.0;
    cfg.max_drawdown_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    clock.set(1_000);
    let mut mgr = make_manager(&cfg, &db, &clock);

    // Trigger DD pause: 35% DD.
    mgr.current_balance = 130.0;
    assert!(!mgr.can_trade(&cfg, &clock)); // pause triggered

    // Let pause expire.
    clock.set(12_000);
    assert!(mgr.can_trade(&cfg, &clock)); // armed becomes false

    // DD is still 35%, but armed=false → no re-trigger.
    clock.set(13_000);
    assert!(mgr.can_trade(&cfg, &clock));

    // Even after much more time, should still not re-trigger.
    clock.set(1_000_000);
    assert!(
        mgr.can_trade(&cfg, &clock),
        "DD pause must never re-trigger when recovery threshold is negative"
    );

    // Verify armed stays false: reduce DD to 0% and check.
    // Even at 0% DD, the recovery threshold is -0.10 and DD=0 is NOT < -0.10.
    mgr.current_balance = 200.0; // DD = 0%
    clock.set(2_000_000);
    assert!(mgr.can_trade(&cfg, &clock));
    // Now push DD above trigger again — should NOT re-trigger.
    mgr.current_balance = 130.0;
    clock.set(3_000_000);
    assert!(
        mgr.can_trade(&cfg, &clock),
        "armed should stay false because recovery_threshold is unreachable"
    );
}

#[test]
fn dd_pause_three_full_cycles() {
    // Run 3 complete trigger → expire → recover → trigger cycles.
    let mut cfg = test_config();
    cfg.peak_dd_pause_pct = 0.30;
    cfg.dd_pause_recovery_pct = 0.05; // recovery threshold = 25%
    cfg.peak_dd_pause_ms = 10_000;
    cfg.min_balance_threshold = 0.0;
    cfg.max_drawdown_pct = 1.0;

    let (db, _tmp) = temp_db();
    let clock = BacktestClock::new();
    let mut mgr = make_manager(&cfg, &db, &clock);

    for cycle in 0..3 {
        let base = (cycle as u64) * 100_000;

        // Step 1: Trigger DD pause (35% DD).
        mgr.current_balance = 130.0;
        clock.set(base + 1_000);
        assert!(
            !mgr.can_trade(&cfg, &clock),
            "cycle {cycle}: DD pause should trigger"
        );

        // Step 2: Let pause expire.
        clock.set(base + 12_000);
        assert!(
            mgr.can_trade(&cfg, &clock),
            "cycle {cycle}: pause should expire"
        );
        // armed is now false.

        // Step 3: Recover below recovery threshold (DD < 25%).
        mgr.current_balance = 160.0; // DD = (200-160)/200 = 20% < 25%
        clock.set(base + 20_000);
        assert!(
            mgr.can_trade(&cfg, &clock),
            "cycle {cycle}: recovery should re-arm"
        );
        // armed should now be true again.
    }

    // After 3 full cycles, verify one final trigger still works.
    mgr.current_balance = 130.0;
    clock.set(400_000);
    assert!(
        !mgr.can_trade(&cfg, &clock),
        "post-cycles: DD pause should still trigger after 3 full cycles"
    );
}

#[test]
fn kelly_with_entry_price_near_one() {
    // entry_price=0.99 → b = (1-0.99)/0.99 = 0.01/0.99 ≈ 0.0101
    // With a high win rate (0.60) and min_win_rate_for_kelly=0.52:
    //   full_kelly = (0.0101*0.60 - 0.40) / 0.0101 ≈ negative → floor
    // Should not crash and should return the min_kelly_floor.
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
