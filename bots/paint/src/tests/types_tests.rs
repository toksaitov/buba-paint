use super::*;
use crate::signal_features::SignalFeatureSnapshot;

/// Verifies that signal direction display up.
#[test]
fn signal_direction_display_up() {
    assert_eq!(SignalDirection::Up.to_string(), "UP");
}

/// Verifies that signal direction display down.
#[test]
fn signal_direction_display_down() {
    assert_eq!(SignalDirection::Down.to_string(), "DOWN");
}

/// Verifies that signal direction from str up.
#[test]
fn signal_direction_from_str_up() {
    assert_eq!(
        SignalDirection::from_str("UP").unwrap(),
        SignalDirection::Up
    );
}

/// Verifies that signal direction from str down.
#[test]
fn signal_direction_from_str_down() {
    assert_eq!(
        SignalDirection::from_str("DOWN").unwrap(),
        SignalDirection::Down
    );
}

/// Verifies that signal direction from str invalid.
#[test]
fn signal_direction_from_str_invalid() {
    let err = SignalDirection::from_str("up").unwrap_err();
    assert!(err.contains("invalid SignalDirection"));
}

/// Verifies that signal direction roundtrip.
#[test]
fn signal_direction_roundtrip() {
    for dir in [SignalDirection::Up, SignalDirection::Down] {
        let s = dir.to_string();
        assert_eq!(SignalDirection::from_str(&s).unwrap(), dir);
    }
}

/// Verifies that trade status display open.
#[test]
fn trade_status_display_open() {
    assert_eq!(TradeStatus::Open.to_string(), "open");
}

/// Verifies that trade status display closed.
#[test]
fn trade_status_display_closed() {
    assert_eq!(TradeStatus::Closed.to_string(), "closed");
}

/// Verifies that trade status display expired.
#[test]
fn trade_status_display_expired() {
    assert_eq!(TradeStatus::Expired.to_string(), "expired");
}

/// Verifies that trade status from str open.
#[test]
fn trade_status_from_str_open() {
    assert_eq!(TradeStatus::from_str("open").unwrap(), TradeStatus::Open);
}

/// Verifies that trade status from str closed.
#[test]
fn trade_status_from_str_closed() {
    assert_eq!(
        TradeStatus::from_str("closed").unwrap(),
        TradeStatus::Closed
    );
}

/// Verifies that trade status from str expired.
#[test]
fn trade_status_from_str_expired() {
    assert_eq!(
        TradeStatus::from_str("expired").unwrap(),
        TradeStatus::Expired
    );
}

/// Verifies that trade status from str invalid.
#[test]
fn trade_status_from_str_invalid() {
    let err = TradeStatus::from_str("OPEN").unwrap_err();
    assert!(err.contains("invalid TradeStatus"));
}

/// Verifies that trade status roundtrip.
#[test]
fn trade_status_roundtrip() {
    for status in [TradeStatus::Open, TradeStatus::Closed, TradeStatus::Expired] {
        let s = status.to_string();
        assert_eq!(TradeStatus::from_str(&s).unwrap(), status);
    }
}

/// Verifies that book state default is none.
#[test]
fn book_state_default_is_none() {
    let state = BookState::default();
    assert!(state.up.is_none());
    assert!(state.down.is_none());
}

/// Verifies that book state with one side.
#[test]
fn book_state_with_one_side() {
    let state = BookState {
        up: Some(TopOfBook {
            best_bid: 0.45,
            best_ask: 0.55,
            bid_size: 100.0,
            ask_size: 200.0,
            timestamp: 1_700_000_000_000,
            observed_at_ms: 1_700_000_000_000,
        }),
        down: None,
    };
    assert!(state.up.is_some());
    assert!(state.down.is_none());
    let up = state.up.unwrap();
    assert!((up.best_bid - 0.45).abs() < f64::EPSILON);
    assert!((up.best_ask - 0.55).abs() < f64::EPSILON);
}

/// Verifies that construct binance tick.
#[test]
fn construct_binance_tick() {
    let tick = BinanceTick {
        event_time: 1_700_000_000_000,
        price: 42_000.50,
        quantity: 0.001,
        trade_time: 1_700_000_000_001,
    };
    assert!((tick.price - 42_000.50).abs() < f64::EPSILON);
    assert_eq!(tick.event_time, 1_700_000_000_000);
}

/// Verifies that construct chainlink tick.
#[test]
fn construct_chainlink_tick() {
    let tick = ChainlinkTick {
        symbol: "BTC/USD".to_string(),
        timestamp: 1_700_000_000_000,
        value: 42_000.0,
    };
    assert_eq!(tick.symbol, "BTC/USD");
    assert!((tick.value - 42_000.0).abs() < f64::EPSILON);
}

/// Verifies that construct order level.
#[test]
fn construct_order_level() {
    let level = OrderLevel {
        price: 0.55,
        size: 1000.0,
    };
    assert!((level.price - 0.55).abs() < f64::EPSILON);
    assert!((level.size - 1000.0).abs() < f64::EPSILON);
}

/// Verifies that construct clob book snapshot.
#[test]
fn construct_clob_book_snapshot() {
    let snap = ClobBookSnapshot {
        asset_id: "token-123".to_string(),
        market: "market-abc".to_string(),
        timestamp: 1_700_000_000_000,
        bids: vec![OrderLevel {
            price: 0.45,
            size: 500.0,
        }],
        asks: vec![OrderLevel {
            price: 0.55,
            size: 300.0,
        }],
    };
    assert_eq!(snap.bids.len(), 1);
    assert_eq!(snap.asks.len(), 1);
}

/// Verifies that construct clob price change.
#[test]
fn construct_clob_price_change() {
    let change = ClobPriceChange {
        asset_id: "token-123".to_string(),
        market: "market-abc".to_string(),
        timestamp: 1_700_000_000_000,
        changes: vec![PriceChangeEntry {
            asset_id: "token-123".to_string(),
            price: 0.50,
            size: 100.0,
            side: "BUY".to_string(),
        }],
    };
    assert_eq!(change.changes.len(), 1);
    assert_eq!(change.changes[0].side, "BUY");
}

/// Verifies that construct gamma market.
#[test]
fn construct_gamma_market() {
    let market = GammaMarket {
        id: "mkt-1".to_string(),
        question: "Will BTC go up?".to_string(),
        condition_id: "cond-1".to_string(),
        slug: "btc-up-down".to_string(),
        active: true,
        closed: false,
        accepting_orders: true,
        outcomes: vec!["Up".to_string(), "Down".to_string()],
        outcome_prices: vec![0.55, 0.45],
        clob_token_ids: vec!["tok-up".to_string(), "tok-down".to_string()],
        resolution_source: Some("chainlink".to_string()),
        order_min_size: Some(5.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: Some(1000.0),
        taker_base_fee: Some(1000.0),
        rewards_min_size: Some(50.0),
        rewards_max_spread: Some(4.5),
        fees_enabled: Some(true),
        fee_schedule: Some(serde_json::json!({"exponent": 1, "rate": 0.072})),
        accepting_orders_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
        clear_book_on_start: Some(false),
        end_date: "2024-01-01T00:05:00Z".to_string(),
        neg_risk: false,
        neg_risk_market_id: String::new(),
    };
    assert!(market.active);
    assert!(!market.closed);
    assert_eq!(market.outcomes.len(), 2);
}

/// Verifies that construct market window.
#[test]
fn construct_market_window() {
    let window = MarketWindow {
        market_id: "mkt-1".to_string(),
        question: "Will BTC go up?".to_string(),
        up_token_id: "tok-up".to_string(),
        down_token_id: "tok-down".to_string(),
        condition_id: "cond-1".to_string(),
        start_time: 1_700_000_000_000,
        end_time: 1_700_000_300_000,
        slug: "btc-up-down".to_string(),
        outcome: None,
        resolution_source: Some("chainlink".to_string()),
        fee_profile: Some("crypto".to_string()),
        order_min_size: Some(5.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: Some(1000.0),
        taker_base_fee: Some(1000.0),
        rewards_min_size: Some(50.0),
        rewards_max_spread: Some(4.5),
        fees_enabled: Some(true),
        fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".to_string()),
        token_fee_rates_json: Some("{\"tok-up\":{\"base_fee\":1000}}".to_string()),
        accepting_orders: Some(true),
        accepting_orders_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
        clear_book_on_start: Some(false),
    };
    assert_eq!(window.end_time - window.start_time, 300_000);
}

/// Verifies that construct signal.
#[test]
fn construct_signal() {
    let signal = Signal {
        timestamp: 1_700_000_000_000,
        strategy: "latency-arb".to_string(),
        strategy_version: "v2".to_string(),
        feature_mode: "legacy_core".to_string(),
        direction: SignalDirection::Up,
        confidence: 0.72,
        binance_price: 42_000.0,
        chainlink_price: 41_999.0,
        up_ask: 0.52,
        down_ask: 0.50,
        up_bid: 0.48,
        down_bid: 0.46,
        expected_edge: None,
        metadata: serde_json::json!({"momentum": 0.0015}),
        telemetry: None,
    };
    assert_eq!(signal.direction, SignalDirection::Up);
    assert!((signal.confidence - 0.72).abs() < f64::EPSILON);
    assert!(signal.metadata.is_object());
}

/// Verifies that construct strategy context.
#[test]
fn construct_strategy_context() {
    let ctx = StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.001_5,
        chainlink_price: Some(41_999.0),
        book_state: BookState::default(),
        window_open_price: Some(41_500.0),
        window_time_remaining_ms: 120_000,
        now_us: Some(1_700_000_000_000_000),
        features: SignalFeatureSnapshot::default(),
    };
    assert!(ctx.chainlink_price.is_some());
    assert_eq!(ctx.window_time_remaining_ms, 120_000);
}

/// Verifies that construct strategy context no chainlink.
#[test]
fn construct_strategy_context_no_chainlink() {
    let ctx = StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.001_5,
        chainlink_price: None,
        book_state: BookState::default(),
        window_open_price: None,
        window_time_remaining_ms: 60_000,
        now_us: None,
        features: SignalFeatureSnapshot::default(),
    };
    assert!(ctx.chainlink_price.is_none());
}

/// Verifies that construct simulated trade.
#[test]
fn construct_simulated_trade() {
    let trade = SimulatedTrade {
        id: None,
        timestamp: 1_700_000_000_000,
        market_id: "mkt-1".to_string(),
        strategy: "latency-arb".to_string(),
        side: SignalDirection::Up,
        token_id: "tok-up".to_string(),
        entry_price: 0.52,
        size: 50.0,
        status: TradeStatus::Open,
        signal_id: None,
        requested_price: None,
        requested_size: None,
        filled_size: None,
        avg_fill_price: None,
        fill_status: None,
        fill_reason: None,
        fill_latency_ms: None,
        execution_group_id: None,
        execution_fidelity: None,
        execution_mode: None,
        order_id: None,
        fill_price: None,
    };
    assert!(trade.id.is_none());
    assert_eq!(trade.status, TradeStatus::Open);
}

/// Verifies that construct simulated trade with id.
#[test]
fn construct_simulated_trade_with_id() {
    let trade = SimulatedTrade {
        id: Some(42),
        timestamp: 1_700_000_000_000,
        market_id: "mkt-1".to_string(),
        strategy: "spread-capture".to_string(),
        side: SignalDirection::Down,
        token_id: "tok-down".to_string(),
        entry_price: 0.48,
        size: 25.0,
        status: TradeStatus::Closed,
        signal_id: None,
        requested_price: None,
        requested_size: None,
        filled_size: None,
        avg_fill_price: None,
        fill_status: None,
        fill_reason: None,
        fill_latency_ms: None,
        execution_group_id: None,
        execution_fidelity: None,
        execution_mode: None,
        order_id: None,
        fill_price: None,
    };
    assert_eq!(trade.id, Some(42));
    assert_eq!(trade.side, SignalDirection::Down);
    assert_eq!(trade.status, TradeStatus::Closed);
}

/// Verifies that construct trade result.
#[test]
fn construct_trade_result() {
    let result = TradeResult {
        trade_id: 42,
        exit_price: 0.60,
        settlement_price: 1.0,
        pnl_0pct: 9.23,
        pnl_1pct: 8.73,
        pnl_2pct: 8.23,
        pnl_3pct: 7.73,
        fee_amount: 0.12,
        pnl_net: 9.11,
        settlement_status: "confirmed".to_string(),
        provisional_pnl: None,
    };
    assert_eq!(result.trade_id, 42);
    assert!((result.pnl_0pct - 9.23).abs() < f64::EPSILON);
    assert!((result.fee_amount - 0.12).abs() < f64::EPSILON);
    assert!((result.pnl_net - 9.11).abs() < f64::EPSILON);
    assert_eq!(result.settlement_status, "confirmed");
    assert!(result.provisional_pnl.is_none());
}

/// Verifies that feed status equality.
#[test]
fn feed_status_equality() {
    assert_eq!(FeedStatus::Disconnected, FeedStatus::Disconnected);
    assert_eq!(FeedStatus::Connecting, FeedStatus::Connecting);
    assert_eq!(FeedStatus::Connected, FeedStatus::Connected);
    assert_ne!(FeedStatus::Disconnected, FeedStatus::Connected);
}

/// Verifies that feed status copy.
#[test]
fn feed_status_copy() {
    let a = FeedStatus::Connected;
    let b = a;
    assert_eq!(a, b);
}

/// Verifies that top of book clone.
#[test]
fn top_of_book_clone() {
    let original = TopOfBook {
        best_bid: 0.45,
        best_ask: 0.55,
        bid_size: 100.0,
        ask_size: 200.0,
        timestamp: 1_700_000_000_000,
        observed_at_ms: 1_700_000_000_000,
    };
    let cloned = original.clone();
    assert!((cloned.best_bid - 0.45).abs() < f64::EPSILON);
    assert_eq!(cloned.timestamp, 1_700_000_000_000);
}

/// Verifies that signal direction debug.
#[test]
fn signal_direction_debug() {
    let debug_str = format!("{:?}", SignalDirection::Up);
    assert_eq!(debug_str, "Up");
}

/// Verifies that trade status debug.
#[test]
fn trade_status_debug() {
    let debug_str = format!("{:?}", TradeStatus::Expired);
    assert_eq!(debug_str, "Expired");
}

/// Verifies that deserialize binance tick.
#[test]
fn deserialize_binance_tick() {
    let json = r#"{
        "eventTime": 1700000000000,
        "price": 42000.5,
        "quantity": 0.001,
        "tradeTime": 1700000000001
    }"#;
    let tick: BinanceTick = serde_json::from_str(json).unwrap();
    assert_eq!(tick.event_time, 1_700_000_000_000);
    assert!((tick.price - 42_000.5).abs() < f64::EPSILON);
}

/// Verifies that deserialize chainlink tick.
#[test]
fn deserialize_chainlink_tick() {
    let json = r#"{
        "symbol": "BTC/USD",
        "timestamp": 1700000000000,
        "value": 42000.0
    }"#;
    let tick: ChainlinkTick = serde_json::from_str(json).unwrap();
    assert_eq!(tick.symbol, "BTC/USD");
}

/// Verifies that deserialize clob book snapshot.
#[test]
fn deserialize_clob_book_snapshot() {
    let json = r#"{
        "assetId": "tok-1",
        "market": "mkt-1",
        "timestamp": 1700000000000,
        "bids": [{"price": 0.45, "size": 500}],
        "asks": [{"price": 0.55, "size": 300}]
    }"#;
    let snap: ClobBookSnapshot = serde_json::from_str(json).unwrap();
    assert_eq!(snap.asset_id, "tok-1");
    assert_eq!(snap.bids.len(), 1);
}

/// Verifies that deserialize price change entry.
#[test]
fn deserialize_price_change_entry() {
    let json = r#"{
        "assetId": "tok-1",
        "price": 0.50,
        "size": 100,
        "side": "BUY"
    }"#;
    let entry: PriceChangeEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.side, "BUY");
    assert_eq!(entry.asset_id, "tok-1");
}

/// Verifies that deserialize gamma market.
#[test]
fn deserialize_gamma_market() {
    let json = r#"{
        "id": "mkt-1",
        "question": "Will BTC go up?",
        "conditionId": "cond-1",
        "slug": "btc-up",
        "active": true,
        "closed": false,
        "acceptingOrders": true,
        "outcomes": ["Up", "Down"],
        "outcomePrices": [0.55, 0.45],
        "clobTokenIds": ["tok-up", "tok-down"],
        "orderPriceMinTickSize": 0.01,
        "endDate": "2024-01-01T00:05:00Z",
        "negRisk": false,
        "negRiskMarketID": ""
    }"#;
    let market: GammaMarket = serde_json::from_str(json).unwrap();
    assert_eq!(market.id, "mkt-1");
    assert!(market.active);
    assert_eq!(market.clob_token_ids.len(), 2);
}
