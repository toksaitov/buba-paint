use super::*;
use crate::clock::BacktestClock;
use tempfile::NamedTempFile;

/// Temp db.
fn temp_db() -> (Database, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
    (db, tmp)
}

/// Default config.
fn default_config() -> Config {
    Config::default()
}

/// Sample window.
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
        outcome: None,
        resolution_source: Some("chainlink".into()),
        fee_profile: Some("crypto".into()),
        order_min_size: Some(5.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: Some(1000.0),
        taker_base_fee: Some(1000.0),
        rewards_min_size: Some(50.0),
        rewards_max_spread: Some(4.5),
    }
}

/// Up signal.
fn up_signal() -> Signal {
    Signal {
        timestamp: 1_700_000_100_000,
        strategy: "latency-arb".into(),
        strategy_version: "v2".into(),
        feature_mode: "legacy_core".into(),
        direction: SignalDirection::Up,
        confidence: 0.72,
        binance_price: 42_000.0,
        chainlink_price: 41_999.0,
        up_ask: 0.45,
        down_ask: 0.50,
        up_bid: 0.40,
        down_bid: 0.46,
        expected_edge: None,
        metadata: serde_json::json!({}),
        telemetry: None,
    }
}

/// Down signal.
fn down_signal() -> Signal {
    Signal {
        timestamp: 1_700_000_100_000,
        strategy: "latency-arb".into(),
        strategy_version: "v2".into(),
        feature_mode: "legacy_core".into(),
        direction: SignalDirection::Down,
        confidence: 0.72,
        binance_price: 42_000.0,
        chainlink_price: 41_999.0,
        up_ask: 0.45,
        down_ask: 0.50,
        up_bid: 0.40,
        down_bid: 0.46,
        expected_edge: None,
        metadata: serde_json::json!({}),
        telemetry: None,
    }
}

/// Spread signals.
fn spread_signals() -> Vec<Signal> {
    vec![
        Signal {
            timestamp: 1_700_000_100_000,
            strategy: "spread-capture".into(),
            strategy_version: "v2".into(),
            feature_mode: "legacy_core".into(),
            direction: SignalDirection::Up,
            confidence: 0.60,
            binance_price: 42_000.0,
            chainlink_price: 41_999.0,
            up_ask: 0.45,
            down_ask: 0.50,
            up_bid: 0.40,
            down_bid: 0.46,
            expected_edge: None,
            metadata: serde_json::json!({}),
            telemetry: None,
        },
        Signal {
            timestamp: 1_700_000_100_000,
            strategy: "spread-capture".into(),
            strategy_version: "v2".into(),
            feature_mode: "legacy_core".into(),
            direction: SignalDirection::Down,
            confidence: 0.60,
            binance_price: 42_000.0,
            chainlink_price: 41_999.0,
            up_ask: 0.45,
            down_ask: 0.50,
            up_bid: 0.40,
            down_bid: 0.46,
            expected_edge: None,
            metadata: serde_json::json!({}),
            telemetry: None,
        },
    ]
}

/// Verifies that try open returns trade with correct fields.
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
        .try_open(
            &signal,
            &window,
            false,
            f64::MAX,
            &db,
            &mut bankroll,
            &config,
            &clock,
        )
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

/// Verifies that try open blocks at max positions.
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

    let signal = up_signal();
    let trade = pm.try_open(
        &signal,
        &window,
        false,
        f64::MAX,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade.is_some());
    assert_eq!(pm.open_count(), 1);

    let window2 = MarketWindow {
        market_id: "mkt-2".into(),
        question: "Another?".into(),
        condition_id: "cond-2".into(),
        slug: "btc-2".into(),
        up_token_id: "tok-up-2".into(),
        down_token_id: "tok-down-2".into(),
        start_time: 1_700_000_000_000,
        end_time: 1_700_000_300_000,
        outcome: None,
        resolution_source: Some("chainlink".into()),
        fee_profile: Some("crypto".into()),
        order_min_size: Some(5.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: Some(1000.0),
        taker_base_fee: Some(1000.0),
        rewards_min_size: Some(50.0),
        rewards_max_spread: Some(4.5),
    };
    db.upsert_market(&window2).unwrap();
    let trade2 = pm.try_open(
        &signal,
        &window2,
        false,
        f64::MAX,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade2.is_none());
    assert_eq!(pm.open_count(), 1);
}

/// Verifies that try open blocks same strategy non batch.
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
    let trade = pm.try_open(
        &signal,
        &window,
        false,
        f64::MAX,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade.is_some());

    let signal2 = down_signal();
    let trade2 = pm.try_open(
        &signal2,
        &window,
        false,
        f64::MAX,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade2.is_none());
    assert_eq!(pm.open_count(), 1);
}

/// Verifies that try open allows same strategy different direction batch.
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
        f64::MAX,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade1.is_some());

    let signal_down = down_signal();
    let trade2 = pm.try_open(
        &signal_down,
        &window,
        true,
        f64::MAX,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(trade2.is_some());
    assert_eq!(pm.open_count(), 2);
}

/// Verifies that try open spread opens both legs.
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

    let sides: Vec<_> = trades.iter().map(|t| t.side).collect();
    assert!(sides.contains(&SignalDirection::Up));
    assert!(sides.contains(&SignalDirection::Down));

    assert!(trades[0].id.is_some());
    assert!(trades[1].id.is_some());

    assert_eq!(trades[0].strategy, "spread-capture");
    assert_eq!(trades[1].strategy, "spread-capture");
}

/// Verifies that resolve window settles win up outcome.
#[test]
fn resolve_window_settles_win_up_outcome() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = up_signal();
    let trade = pm
        .try_open(
            &signal,
            &window,
            false,
            f64::MAX,
            &db,
            &mut bankroll,
            &config,
            &clock,
        )
        .unwrap();
    let entry_price = trade.entry_price;
    let size = trade.size;
    assert_eq!(pm.open_count(), 1);

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

    assert!((result.settlement_price - 1.0).abs() < f64::EPSILON);
    assert!((result.exit_price - 1.0).abs() < f64::EPSILON);

    let gross = (1.0 - entry_price) * size;
    let entry_cost = entry_price * size;
    assert!((result.pnl_0pct - gross).abs() < 1e-10);
    assert!((result.pnl_1pct - (gross - entry_cost * 0.01)).abs() < 1e-10);
    assert!((result.pnl_2pct - (gross - entry_cost * 0.02)).abs() < 1e-10);
    assert!((result.pnl_3pct - (gross - entry_cost * 0.03)).abs() < 1e-10);

    assert_eq!(settled_trade.side, SignalDirection::Up);
}

/// Verifies that resolve window settles loss.
#[test]
fn resolve_window_settles_loss() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = up_signal();
    let trade = pm
        .try_open(
            &signal,
            &window,
            false,
            f64::MAX,
            &db,
            &mut bankroll,
            &config,
            &clock,
        )
        .unwrap();
    let entry_price = trade.entry_price;
    let size = trade.size;

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

    assert!((result.settlement_price - 0.0).abs() < f64::EPSILON);
    assert!((result.exit_price - 0.0).abs() < f64::EPSILON);

    let gross = (0.0 - entry_price) * size;
    assert!((result.pnl_0pct - gross).abs() < 1e-10);
    assert!(result.pnl_0pct < 0.0);
}

/// Verifies that resolve window decrements open count.
#[test]
fn resolve_window_decrements_open_count() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal_up = up_signal();
    let signal_down = down_signal();
    pm.try_open(
        &signal_up,
        &window,
        true,
        f64::MAX,
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
        f64::MAX,
        &db,
        &mut bankroll,
        &config,
        &clock,
    )
    .unwrap();
    assert_eq!(pm.open_count(), 2);

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

/// Verifies that resolve window no trades resolves market.
#[test]
fn resolve_window_no_trades_resolves_market() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

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

/// Verifies that try open returns none with tiny balance.
#[test]
fn try_open_returns_none_with_tiny_balance() {
    let (db, _tmp) = temp_db();
    let mut config = default_config();
    config.min_balance_threshold = 20.0;
    let clock = BacktestClock::new();

    let mut bankroll = BankrollManager::new(5.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = up_signal();
    let result = pm.try_open(
        &signal,
        &window,
        false,
        f64::MAX,
        &db,
        &mut bankroll,
        &config,
        &clock,
    );
    assert!(
        result.is_none(),
        "should return None when balance is below min threshold"
    );
    assert_eq!(pm.open_count(), 0);
}

/// Verifies that try open spread returns empty with tiny balance.
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

/// Verifies that resolve window mixed win and loss.
#[test]
fn resolve_window_mixed_win_and_loss() {
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
            f64::MAX,
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
            f64::MAX,
            &db,
            &mut bankroll,
            &config,
            &clock,
        )
        .unwrap();
    assert_eq!(pm.open_count(), 2);

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

    let up_result = results.iter().find(|(t, _)| t.side == SignalDirection::Up);
    let down_result = results
        .iter()
        .find(|(t, _)| t.side == SignalDirection::Down);
    assert!(up_result.is_some());
    assert!(down_result.is_some());

    let (_, up_tr) = up_result.unwrap();
    let (_, down_tr) = down_result.unwrap();

    assert!((up_tr.settlement_price - 1.0).abs() < f64::EPSILON);
    assert!(up_tr.pnl_0pct > 0.0);

    assert!((down_tr.settlement_price - 0.0).abs() < f64::EPSILON);
    assert!(down_tr.pnl_0pct < 0.0);

    let gross_up = (1.0 - trade_up.entry_price) * trade_up.size;
    assert!((up_tr.pnl_0pct - gross_up).abs() < 1e-10);

    let gross_down = (0.0 - trade_down.entry_price) * trade_down.size;
    assert!((down_tr.pnl_0pct - gross_down).abs() < 1e-10);
}

/// Verifies that try open down direction uses correct token and price.
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
        .try_open(
            &signal,
            &window,
            false,
            f64::MAX,
            &db,
            &mut bankroll,
            &config,
            &clock,
        )
        .expect("should open DOWN trade");

    assert_eq!(trade.side, SignalDirection::Down);
    assert_eq!(trade.token_id, "tok-down");
    assert!((trade.entry_price - signal.down_ask).abs() < f64::EPSILON);
}

/// Verifies that try open spread blocks duplicate legs.
#[test]
fn try_open_spread_blocks_duplicate_legs() {
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

    let trades2 = pm.try_open_spread(&signals, &window, &db, &mut bankroll, &config, &clock);
    assert!(trades2.is_empty(), "duplicate spread should be blocked");
    assert_eq!(pm.open_count(), 2);
}

/// Verifies that try open spread blocks when max positions insufficient.
#[test]
fn try_open_spread_blocks_when_max_positions_insufficient() {
    let (db, _tmp) = temp_db();
    let mut config = default_config();
    config.max_open_positions = 1;
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

/// Verifies that try open spread missing direction returns empty.
#[test]
fn try_open_spread_missing_direction_returns_empty() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signals = vec![spread_signals().remove(0)];
    let trades = pm.try_open_spread(&signals, &window, &db, &mut bankroll, &config, &clock);
    assert!(trades.is_empty(), "should require both UP and DOWN signals");
}

/// Verifies that resolve window down outcome.
#[test]
fn resolve_window_down_outcome() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();
    let window = sample_window();
    db.upsert_market(&window).unwrap();

    let signal = down_signal();
    let _trade = pm
        .try_open(
            &signal,
            &window,
            false,
            f64::MAX,
            &db,
            &mut bankroll,
            &config,
            &clock,
        )
        .unwrap();

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

/// Verifies that default impl creates position manager.
#[test]
fn default_impl_creates_position_manager() {
    let pm = PositionManager::default();
    assert_eq!(pm.open_count(), 0);
}

/// Verifies that resolve window no matching trades returns empty.
#[test]
fn resolve_window_no_matching_trades_returns_empty() {
    let (db, _tmp) = temp_db();
    let config = default_config();
    let clock = BacktestClock::new();
    let mut bankroll = BankrollManager::new(200.0, &config, &db, &clock);
    let mut pm = PositionManager::new();

    let window = MarketWindow {
        market_id: "mkt-nonexistent".into(),
        question: "Does not exist".into(),
        condition_id: "cond-none".into(),
        slug: "btc-none".into(),
        up_token_id: "tok-up-none".into(),
        down_token_id: "tok-down-none".into(),
        start_time: 2_000_000_000_000,
        end_time: 2_000_000_300_000,
        outcome: None,
        resolution_source: Some("chainlink".into()),
        fee_profile: Some("crypto".into()),
        order_min_size: Some(5.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: Some(1000.0),
        taker_base_fee: Some(1000.0),
        rewards_min_size: Some(50.0),
        rewards_max_spread: Some(4.5),
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
