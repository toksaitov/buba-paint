use super::*;
use crate::clock::BacktestClock;
use tempfile::NamedTempFile;

// -- helpers --------------------------------------------------------------

fn temp_db() -> (Database, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
    (db, tmp)
}

fn default_config() -> Config {
    Config::default()
}

fn sample_window() -> MarketWindow {
    MarketWindow {
        market_id: "mkt-1".into(),
        question: "Will BTC go up?".into(),
        condition_id: "cond-1".into(),
        slug: "btc-up-down".into(),
        up_token_id: "tok-up".into(),
        down_token_id: "tok-down".into(),
        start_time: 1_700_000_000_000,
        end_time: 1_700_000_300_000,
    }
}

fn up_signal() -> Signal {
    Signal {
        timestamp: 1_700_000_100_000,
        strategy: "latency-arb".into(),
        direction: SignalDirection::Up,
        confidence: 0.72,
        binance_price: 42_000.0,
        chainlink_price: 41_999.0,
        up_ask: 0.45,
        down_ask: 0.50,
        up_bid: 0.40,
        down_bid: 0.46,
        metadata: serde_json::json!({}),
    }
}

fn down_signal() -> Signal {
    Signal {
        timestamp: 1_700_000_100_000,
        strategy: "latency-arb".into(),
        direction: SignalDirection::Down,
        confidence: 0.72,
        binance_price: 42_000.0,
        chainlink_price: 41_999.0,
        up_ask: 0.45,
        down_ask: 0.50,
        up_bid: 0.40,
        down_bid: 0.46,
        metadata: serde_json::json!({}),
    }
}

fn spread_signals() -> Vec<Signal> {
    vec![
        Signal {
            timestamp: 1_700_000_100_000,
            strategy: "spread-capture".into(),
            direction: SignalDirection::Up,
            confidence: 0.60,
            binance_price: 42_000.0,
            chainlink_price: 41_999.0,
            up_ask: 0.45,
            down_ask: 0.50,
            up_bid: 0.40,
            down_bid: 0.46,
            metadata: serde_json::json!({}),
        },
        Signal {
            timestamp: 1_700_000_100_000,
            strategy: "spread-capture".into(),
            direction: SignalDirection::Down,
            confidence: 0.60,
            binance_price: 42_000.0,
            chainlink_price: 41_999.0,
            up_ask: 0.45,
            down_ask: 0.50,
            up_bid: 0.40,
            down_bid: 0.46,
            metadata: serde_json::json!({}),
        },
    ]
}

// -- try_open returns a trade with correct fields -------------------------

#[test]
fn try_open_returns_trade_with_correct_fields() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = up_signal();
    let trade = pm
        .try_open(&signal, &window, false, &db, &mut bankroll, &config, &clock)
        .expect("should open trade");

    assert!(trade.id.is_some());
    assert_eq!(trade.market_id, "mkt-1");
    assert_eq!(trade.strategy, "latency-arb");
    assert_eq!(trade.side, SignalDirection::Up);
    assert_eq!(trade.token_id, "tok-up");
    assert!((trade.entry_price - 0.45).abs() < f64::EPSILON);
    assert!(trade.size > 0.0);
    assert_eq!(trade.status, TradeStatus::Open);
    assert_eq!(trade.timestamp, signal.timestamp);
    assert_eq!(pm.open_count(), 1);
}

// -- try_open blocks when max positions reached ---------------------------

#[test]
fn try_open_blocks_at_max_positions() {
    let (db, _tmp) = temp_db();
    let mut config = default_config();
    config.max_open_positions = 1;
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    // Open one trade to reach the limit.
    let signal = up_signal();
    let trade = pm.try_open(&signal, &window, false, &db, &mut bankroll, &config, &clock);
    assert!(trade.is_some());
    assert_eq!(pm.open_count(), 1);

    // Second open in a different market should be blocked.
    let window2 = MarketWindow {
        market_id: "mkt-2".into(),
        question: "Another?".into(),
        condition_id: "cond-2".into(),
        slug: "btc-2".into(),
        up_token_id: "tok-up-2".into(),
        down_token_id: "tok-down-2".into(),
        start_time: 1_700_000_000_000,
        end_time: 1_700_000_300_000,
    };
    db.upsert_market(&window2).unwrap();
    let trade2 = pm.try_open(
        &signal,
        &window2,
        false,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade2.is_none());
    assert_eq!(pm.open_count(), 1);
}

// -- try_open blocks same strategy in same market (non-batch) -------------

#[test]
fn try_open_blocks_same_strategy_non_batch() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = up_signal();
    let trade = pm.try_open(&signal, &window, false, &db, &mut bankroll, &config, &clock);
    assert!(trade.is_some());

    // Same strategy, different direction, same market — should be blocked (non-batch).
    let signal2 = down_signal();
    let trade2 = pm.try_open(
        &signal2,
        &window,
        false,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade2.is_none());
    assert_eq!(pm.open_count(), 1);
}

// -- try_open allows same strategy different direction (batch mode) --------

#[test]
fn try_open_allows_same_strategy_different_direction_batch() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal_up = up_signal();
    let trade1 = pm.try_open(
        &signal_up,
        &window,
        true,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade1.is_some());

    // Same strategy, different direction, batch mode — should be allowed.
    let signal_down = down_signal();
    let trade2 = pm.try_open(
        &signal_down,
        &window,
        true,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade2.is_some());
    assert_eq!(pm.open_count(), 2);
}

// -- try_open_spread opens both legs --------------------------------------

#[test]
fn try_open_spread_opens_both_legs() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signals = spread_signals();
    let trades = pm.try_open_spread(&signals, &window, &db, &mut bankroll, &config, &clock);

    assert_eq!(trades.len(), 2);
    assert_eq!(pm.open_count(), 2);

    // One UP, one DOWN.
    let sides: Vec<_> = trades.iter().map(|t| t.side).collect();
    assert!(sides.contains(&SignalDirection::Up));
    assert!(sides.contains(&SignalDirection::Down));

    // Both should have IDs.
    assert!(trades[0].id.is_some());
    assert!(trades[1].id.is_some());

    // Strategy should be spread-capture.
    assert_eq!(trades[0].strategy, "spread-capture");
    assert_eq!(trades[1].strategy, "spread-capture");
}

// -- resolve_window correctly settles WIN (UP outcome) --------------------

#[test]
fn resolve_window_settles_win_up_outcome() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    // Open an UP trade.
    let signal = up_signal();
    let trade = pm
        .try_open(&signal, &window, false, &db, &mut bankroll, &config, &clock)
        .unwrap();
    let entry_price = trade.entry_price;
    let size = trade.size;
    assert_eq!(pm.open_count(), 1);

    // Resolve: close > open → UP wins.
    let results = pm.resolve_window(
        &window,
        42_000.0,
        42_100.0,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );

    assert_eq!(results.len(), 1);
    let (settled_trade, result) = &results[0];

    // Settlement price should be 1.0 (win).
    assert!((result.settlement_price - 1.0).abs() < f64::EPSILON);
    assert!((result.exit_price - 1.0).abs() < f64::EPSILON);

    // PnL checks.
    let gross = (1.0 - entry_price) * size;
    let entry_cost = entry_price * size;
    assert!((result.pnl_0pct - gross).abs() < 1e-10);
    assert!((result.pnl_1pct - (gross - entry_cost * 0.01)).abs() < 1e-10);
    assert!((result.pnl_2pct - (gross - entry_cost * 0.02)).abs() < 1e-10);
    assert!((result.pnl_3pct - (gross - entry_cost * 0.03)).abs() < 1e-10);

    assert_eq!(settled_trade.side, SignalDirection::Up);
}

// -- resolve_window correctly settles LOSS --------------------------------

#[test]
fn resolve_window_settles_loss() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    // Open an UP trade.
    let signal = up_signal();
    let trade = pm
        .try_open(&signal, &window, false, &db, &mut bankroll, &config, &clock)
        .unwrap();
    let entry_price = trade.entry_price;
    let size = trade.size;

    // Resolve: close < open → DOWN wins, UP trade loses.
    let results = pm.resolve_window(
        &window,
        42_000.0,
        41_900.0,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );

    assert_eq!(results.len(), 1);
    let (_settled_trade, result) = &results[0];

    // Settlement price should be 0.0 (loss).
    assert!((result.settlement_price - 0.0).abs() < f64::EPSILON);
    assert!((result.exit_price - 0.0).abs() < f64::EPSILON);

    // PnL should be negative.
    let gross = (0.0 - entry_price) * size;
    assert!((result.pnl_0pct - gross).abs() < 1e-10);
    assert!(result.pnl_0pct < 0.0);
}

// -- resolve_window decrements open_count ---------------------------------

#[test]
fn resolve_window_decrements_open_count() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    // Open two trades (batch mode to allow same strategy different direction).
    let signal_up = up_signal();
    let signal_down = down_signal();
    pm.try_open(
        &signal_up,
        &window,
        true,
        &db,
        &mut bankroll,
        &config,
        &clock,
    )
    .unwrap();
    pm.try_open(
        &signal_down,
        &window,
        true,
        &db,
        &mut bankroll,
        &config,
        &clock,
    )
    .unwrap();
    assert_eq!(pm.open_count(), 2);

    // Resolve all.
    let results = pm.resolve_window(
        &window,
        42_000.0,
        42_100.0,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );

    assert_eq!(results.len(), 2);
    assert_eq!(pm.open_count(), 0);
}

// -- resolve_window with no trades just resolves market --------------------

#[test]
fn resolve_window_no_trades_resolves_market() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    // Resolve with no open trades.
    let results = pm.resolve_window(
        &window,
        42_000.0,
        42_100.0,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );

    assert!(results.is_empty());
    assert_eq!(pm.open_count(), 0);
}

// -- Phase D: edge-case tests for near-100% coverage ----------------------

#[test]
fn try_open_returns_none_with_tiny_balance() {
    let (db, _tmp) = temp_db();
    let mut config = default_config();
    config.min_balance_threshold = 20.0;
    let clock = BacktestClock::new();
    // Starting balance below min_balance_threshold → bankroll rejects.
    let mut bankroll = BankrollManager::new(5.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = up_signal();
    let result = pm.try_open(&signal, &window, false, &db, &mut bankroll, &config, &clock);
    assert!(
        result.is_none(),
        "should return None when balance is below min threshold"
    );
    assert_eq!(pm.open_count(), 0);
}

#[test]
fn try_open_spread_returns_empty_with_tiny_balance() {
    let (db, _tmp) = temp_db();
    let mut config = default_config();
    config.min_balance_threshold = 20.0;
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(5.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signals = spread_signals();
    let trades = pm.try_open_spread(&signals, &window, &db, &mut bankroll, &config, &clock);
    assert!(
        trades.is_empty(),
        "should return empty vec when balance is below min threshold"
    );
    assert_eq!(pm.open_count(), 0);
}

#[test]
fn resolve_window_mixed_win_and_loss() {
    // Open UP + DOWN trades (batch), then resolve with UP outcome.
    // UP trade wins, DOWN trade loses.
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal_up = up_signal();
    let signal_down = down_signal();
    let trade_up = pm
        .try_open(
            &signal_up,
            &window,
            true,
            &db,
            &mut bankroll,
            &config,
            &clock,
        )
        .unwrap();
    let trade_down = pm
        .try_open(
            &signal_down,
            &window,
            true,
            &db,
            &mut bankroll,
            &config,
            &clock,
        )
        .unwrap();
    assert_eq!(pm.open_count(), 2);

    // Resolve: close > open → UP wins.
    let results = pm.resolve_window(
        &window,
        42_000.0,
        42_100.0,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );

    assert_eq!(results.len(), 2);
    assert_eq!(pm.open_count(), 0);

    // Find the UP and DOWN results.
    let up_result = results.iter().find(|(t, _)| t.side == SignalDirection::Up);
    let down_result = results
        .iter()
        .find(|(t, _)| t.side == SignalDirection::Down);
    assert!(up_result.is_some());
    assert!(down_result.is_some());

    let (_, up_tr) = up_result.unwrap();
    let (_, down_tr) = down_result.unwrap();

    // UP trade wins: settlement=1.0, pnl>0
    assert!((up_tr.settlement_price - 1.0).abs() < f64::EPSILON);
    assert!(up_tr.pnl_0pct > 0.0);

    // DOWN trade loses: settlement=0.0, pnl<0
    assert!((down_tr.settlement_price - 0.0).abs() < f64::EPSILON);
    assert!(down_tr.pnl_0pct < 0.0);

    // Verify PnL arithmetic for the UP trade.
    let gross_up = (1.0 - trade_up.entry_price) * trade_up.size;
    assert!((up_tr.pnl_0pct - gross_up).abs() < 1e-10);

    // Verify PnL arithmetic for the DOWN trade.
    let gross_down = (0.0 - trade_down.entry_price) * trade_down.size;
    assert!((down_tr.pnl_0pct - gross_down).abs() < 1e-10);
}

#[test]
fn try_open_down_direction_uses_correct_token_and_price() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = down_signal();
    let trade = pm
        .try_open(&signal, &window, false, &db, &mut bankroll, &config, &clock)
        .expect("should open DOWN trade");

    assert_eq!(trade.side, SignalDirection::Down);
    assert_eq!(trade.token_id, "tok-down");
    assert!((trade.entry_price - signal.down_ask).abs() < f64::EPSILON);
}

#[test]
fn try_open_spread_blocks_duplicate_legs() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    // Open a spread (UP + DOWN).
    let signals = spread_signals();
    let trades = pm.try_open_spread(&signals, &window, &db, &mut bankroll, &config, &clock);
    assert_eq!(trades.len(), 2);
    assert_eq!(pm.open_count(), 2);

    // Try to open the same spread again — should be blocked by duplicate check.
    let trades2 = pm.try_open_spread(&signals, &window, &db, &mut bankroll, &config, &clock);
    assert!(trades2.is_empty(), "duplicate spread should be blocked");
    assert_eq!(pm.open_count(), 2); // still 2 from the first spread
}

#[test]
fn try_open_spread_blocks_when_max_positions_insufficient() {
    let (db, _tmp) = temp_db();
    let mut config = default_config();
    config.max_open_positions = 1; // only room for 1, spread needs 2
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signals = spread_signals();
    let trades = pm.try_open_spread(&signals, &window, &db, &mut bankroll, &config, &clock);
    assert!(
        trades.is_empty(),
        "spread needs 2 slots but max_open_positions = 1"
    );
}

#[test]
fn try_open_spread_missing_direction_returns_empty() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    // Only an UP signal, no DOWN.
    let signals = vec![spread_signals().remove(0)];
    let trades = pm.try_open_spread(&signals, &window, &db, &mut bankroll, &config, &clock);
    assert!(trades.is_empty(), "should require both UP and DOWN signals");
}

#[test]
fn resolve_window_down_outcome() {
    // Open a DOWN trade, resolve with DOWN outcome (close < open) → wins.
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = down_signal();
    let _trade = pm
        .try_open(&signal, &window, false, &db, &mut bankroll, &config, &clock)
        .unwrap();

    // Resolve: close < open → DOWN wins.
    let results = pm.resolve_window(
        &window,
        42_000.0,
        41_900.0,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );

    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];
    assert!(
        (result.settlement_price - 1.0).abs() < f64::EPSILON,
        "DOWN trade should win"
    );
    assert!(result.pnl_0pct > 0.0);
}

#[test]
fn default_impl_creates_position_manager() {
    let pm = PositionManager::default();
    assert_eq!(pm.open_count(), 0);
}

#[test]
fn resolve_window_no_matching_trades_returns_empty() {
    // Call resolve_window with a market_id that was never upserted to the DB
    // and has no open trades.  This exercises the empty-trades early-return
    // path where resolve_market is called on a nonexistent row (a no-op).
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();

    // Use a different market window that has NOT been upserted.
    let window = MarketWindow {
        market_id: "mkt-nonexistent".into(),
        question: "Does not exist".into(),
        condition_id: "cond-none".into(),
        slug: "btc-none".into(),
        up_token_id: "tok-up-none".into(),
        down_token_id: "tok-down-none".into(),
        start_time: 2_000_000_000_000,
        end_time: 2_000_000_300_000,
    };

    let results = pm.resolve_window(
        &window,
        42_000.0,
        42_100.0,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );

    assert!(
        results.is_empty(),
        "should return empty vec for nonexistent market"
    );
    assert_eq!(pm.open_count(), 0);
}
