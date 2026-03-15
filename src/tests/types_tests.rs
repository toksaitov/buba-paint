use super::*;

// -- SignalDirection Display / FromStr ----------------------------------

#[test]
fn signal_direction_display_up() {
    assert_eq!(SignalDirection::Up.to_string(), "UP");
}

#[test]
fn signal_direction_display_down() {
    assert_eq!(SignalDirection::Down.to_string(), "DOWN");
}

#[test]
fn signal_direction_from_str_up() {
    assert_eq!(
        SignalDirection::from_str("UP").unwrap(),
        SignalDirection::Up
    );
}

#[test]
fn signal_direction_from_str_down() {
    assert_eq!(
        SignalDirection::from_str("DOWN").unwrap(),
        SignalDirection::Down
    );
}

#[test]
fn signal_direction_from_str_invalid() {
    let err = SignalDirection::from_str("up").unwrap_err();
    assert!(err.contains("invalid SignalDirection"));
}

#[test]
fn signal_direction_roundtrip() {
    for dir in [SignalDirection::Up, SignalDirection::Down] {
        let s = dir.to_string();
        assert_eq!(SignalDirection::from_str(&s).unwrap(), dir);
    }
}

// -- TradeStatus Display / FromStr -------------------------------------

#[test]
fn trade_status_display_open() {
    assert_eq!(TradeStatus::Open.to_string(), "open");
}

#[test]
fn trade_status_display_closed() {
    assert_eq!(TradeStatus::Closed.to_string(), "closed");
}

#[test]
fn trade_status_display_expired() {
    assert_eq!(TradeStatus::Expired.to_string(), "expired");
}

#[test]
fn trade_status_from_str_open() {
    assert_eq!(TradeStatus::from_str("open").unwrap(), TradeStatus::Open);
}

#[test]
fn trade_status_from_str_closed() {
    assert_eq!(
        TradeStatus::from_str("closed").unwrap(),
        TradeStatus::Closed
    );
}

#[test]
fn trade_status_from_str_expired() {
    assert_eq!(
        TradeStatus::from_str("expired").unwrap(),
        TradeStatus::Expired
    );
}

#[test]
fn trade_status_from_str_invalid() {
    let err = TradeStatus::from_str("OPEN").unwrap_err();
    assert!(err.contains("invalid TradeStatus"));
}

#[test]
fn trade_status_roundtrip() {
    for status in [TradeStatus::Open, TradeStatus::Closed, TradeStatus::Expired] {
        let s = status.to_string();
        assert_eq!(TradeStatus::from_str(&s).unwrap(), status);
    }
}

// -- BookState default -------------------------------------------------

#[test]
fn book_state_default_is_none() {
    let state = BookState::default();
    assert!(state.up.is_none());
    assert!(state.down.is_none());
}

#[test]
fn book_state_with_one_side() {
    let state = BookState {
        up: Some(TopOfBook {
            best_bid: 0.45,
            best_ask: 0.55,
            bid_size: 100.0,
            ask_size: 200.0,
            timestamp: 1_700_000_000_000,
        }),
        down: None,
    };
    assert!(state.up.is_some());
    assert!(state.down.is_none());
    let up = state.up.unwrap();
    assert!((up.best_bid - 0.45).abs() < f64::EPSILON);
    assert!((up.best_ask - 0.55).abs() < f64::EPSILON);
}

// -- Struct construction -----------------------------------------------

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

#[test]
fn construct_order_level() {
    let level = OrderLevel {
        price: 0.55,
        size: 1000.0,
    };
    assert!((level.price - 0.55).abs() < f64::EPSILON);
    assert!((level.size - 1000.0).abs() < f64::EPSILON);
}

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
        order_price_min_tick_size: 0.01,
        end_date: "2024-01-01T00:05:00Z".to_string(),
        neg_risk: false,
        neg_risk_market_id: String::new(),
    };
    assert!(market.active);
    assert!(!market.closed);
    assert_eq!(market.outcomes.len(), 2);
}

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
    };
    assert_eq!(window.end_time - window.start_time, 300_000);
}

#[test]
fn construct_signal() {
    let signal = Signal {
        timestamp: 1_700_000_000_000,
        strategy: "latency-arb".to_string(),
        direction: SignalDirection::Up,
        confidence: 0.72,
        binance_price: 42_000.0,
        chainlink_price: 41_999.0,
        up_ask: 0.52,
        down_ask: 0.50,
        up_bid: 0.48,
        down_bid: 0.46,
        metadata: serde_json::json!({"momentum": 0.0015}),
    };
    assert_eq!(signal.direction, SignalDirection::Up);
    assert!((signal.confidence - 0.72).abs() < f64::EPSILON);
    assert!(signal.metadata.is_object());
}

#[test]
fn construct_strategy_context() {
    let ctx = StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.001_5,
        chainlink_price: Some(41_999.0),
        book_state: BookState::default(),
        window_time_remaining_ms: 120_000,
    };
    assert!(ctx.chainlink_price.is_some());
    assert_eq!(ctx.window_time_remaining_ms, 120_000);
}

#[test]
fn construct_strategy_context_no_chainlink() {
    let ctx = StrategyContext {
        binance_price: 42_000.0,
        binance_momentum: 0.001_5,
        chainlink_price: None,
        book_state: BookState::default(),
        window_time_remaining_ms: 60_000,
    };
    assert!(ctx.chainlink_price.is_none());
}

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
    };
    assert!(trade.id.is_none());
    assert_eq!(trade.status, TradeStatus::Open);
}

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
    };
    assert_eq!(trade.id, Some(42));
    assert_eq!(trade.side, SignalDirection::Down);
    assert_eq!(trade.status, TradeStatus::Closed);
}

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
    };
    assert_eq!(result.trade_id, 42);
    assert!((result.pnl_0pct - 9.23).abs() < f64::EPSILON);
}

// -- FeedStatus --------------------------------------------------------

#[test]
fn feed_status_equality() {
    assert_eq!(FeedStatus::Disconnected, FeedStatus::Disconnected);
    assert_eq!(FeedStatus::Connecting, FeedStatus::Connecting);
    assert_eq!(FeedStatus::Connected, FeedStatus::Connected);
    assert_ne!(FeedStatus::Disconnected, FeedStatus::Connected);
}

#[test]
fn feed_status_copy() {
    let a = FeedStatus::Connected;
    let b = a; // Copy
    assert_eq!(a, b);
}

// -- Clone / Debug smoke tests -----------------------------------------

#[test]
fn top_of_book_clone() {
    let original = TopOfBook {
        best_bid: 0.45,
        best_ask: 0.55,
        bid_size: 100.0,
        ask_size: 200.0,
        timestamp: 1_700_000_000_000,
    };
    let cloned = original.clone();
    assert!((cloned.best_bid - 0.45).abs() < f64::EPSILON);
    assert_eq!(cloned.timestamp, 1_700_000_000_000);
}

#[test]
fn signal_direction_debug() {
    let debug_str = format!("{:?}", SignalDirection::Up);
    assert_eq!(debug_str, "Up");
}

#[test]
fn trade_status_debug() {
    let debug_str = format!("{:?}", TradeStatus::Expired);
    assert_eq!(debug_str, "Expired");
}

// -- Serde deserialization ---------------------------------------------

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
