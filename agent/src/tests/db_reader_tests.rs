use rusqlite::Connection;

use super::*;

/// Create an in-memory DB with the bot's schema and fixture data.
fn fixture_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tick_data (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            source TEXT NOT NULL,
            price REAL, bid REAL, ask REAL, bid_size REAL, ask_size REAL
        );
        CREATE TABLE markets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            market_id TEXT NOT NULL UNIQUE,
            question TEXT NOT NULL,
            condition_id TEXT NOT NULL,
            slug TEXT NOT NULL,
            up_token_id TEXT NOT NULL,
            down_token_id TEXT NOT NULL,
            start_time INTEGER NOT NULL,
            end_time INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'active'
        );
        CREATE TABLE signals (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            strategy TEXT NOT NULL,
            direction TEXT NOT NULL,
            binance_price REAL,
            chainlink_price REAL,
            up_ask REAL, down_ask REAL,
            up_bid REAL, down_bid REAL,
            metadata TEXT
        );
        CREATE TABLE simulated_trades (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            market_id TEXT NOT NULL,
            strategy TEXT NOT NULL,
            side TEXT NOT NULL,
            token_id TEXT NOT NULL,
            entry_price REAL NOT NULL,
            size REAL NOT NULL,
            status TEXT NOT NULL DEFAULT 'open'
        );
        CREATE TABLE trade_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            trade_id INTEGER NOT NULL UNIQUE,
            exit_price REAL,
            settlement_price REAL NOT NULL,
            pnl_0pct REAL NOT NULL,
            pnl_1pct REAL NOT NULL,
            pnl_2pct REAL NOT NULL,
            pnl_3pct REAL NOT NULL,
            resolved_at INTEGER NOT NULL
        );
        CREATE TABLE balance_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            event TEXT NOT NULL,
            trade_id INTEGER,
            amount REAL NOT NULL,
            balance REAL NOT NULL
        );",
    )
    .unwrap();
    conn
}

/// Insert standard fixture data into the DB.
fn seed_fixtures(conn: &Connection) {
    // Balance log: init + 2 trades
    conn.execute_batch(
        "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance)
         VALUES (1000, 'init', NULL, 0.0, 200.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance)
         VALUES (2000, 'settlement', 1, 50.0, 250.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance)
         VALUES (3000, 'settlement', 2, -30.0, 220.0);",
    )
    .unwrap();

    // Markets
    conn.execute_batch(
        "INSERT INTO markets (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status)
         VALUES ('mkt-1', 'Will BTC go up?', 'cond-1', 'btc-updown-5m-100', 'tok-up', 'tok-down', 1000, 2000, 'resolved');
         INSERT INTO markets (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status)
         VALUES ('mkt-2', 'Will BTC go up?', 'cond-2', 'btc-updown-5m-200', 'tok-up-2', 'tok-down-2', 2000, 3000, 'active');",
    )
    .unwrap();

    // Trades
    conn.execute_batch(
        "INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status)
         VALUES (1100, 'mkt-1', 'latency-arb', 'UP', 'tok-up', 0.45, 100.0, 'closed');
         INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status)
         VALUES (2100, 'mkt-2', 'spread-capture', 'DOWN', 'tok-down-2', 0.50, 80.0, 'closed');
         INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status)
         VALUES (2500, 'mkt-2', 'latency-arb', 'UP', 'tok-up-2', 0.42, 60.0, 'open');",
    )
    .unwrap();

    // Trade results (for the 2 closed trades)
    conn.execute_batch(
        "INSERT INTO trade_results (trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at)
         VALUES (1, 1.0, 1.0, 55.0, 54.0, 53.0, 52.0, 1500);
         INSERT INTO trade_results (trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at)
         VALUES (2, 0.0, 0.0, -40.0, -40.8, -41.6, -42.4, 2500);",
    )
    .unwrap();

    // Signals
    conn.execute_batch(
        "INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata)
         VALUES (1050, 'latency-arb', 'UP', 42000.0, 42001.0, 0.45, 0.55, '{\"momentum\": 0.003}');
         INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata)
         VALUES (2050, 'spread-capture', 'DOWN', 42100.0, 42099.0, 0.48, 0.50, '{\"spread\": 0.98}');",
    )
    .unwrap();

    // Tick data
    conn.execute_batch(
        "INSERT INTO tick_data (timestamp, source, price) VALUES (500, 'binance', 42000.0);
         INSERT INTO tick_data (timestamp, source, price) VALUES (3500, 'binance', 42100.0);",
    )
    .unwrap();
}

#[tokio::test]
async fn get_status_with_fixture_data() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();
    assert_eq!(status.balance, 220.0);
    assert_eq!(status.starting_balance, 200.0);
    assert_eq!(status.total_trades, 2);
    assert_eq!(status.wins, 1);
    assert_eq!(status.losses, 1);
    assert!((status.win_rate - 0.5).abs() < 0.001);
    assert!((status.total_pnl - 15.0).abs() < 0.001); // 55 + (-40)
    assert_eq!(status.high_water_mark, 250.0);
    assert_eq!(status.open_trades, 1);
}

#[tokio::test]
async fn get_status_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();
    assert_eq!(status.balance, 0.0);
    assert_eq!(status.total_trades, 0);
    assert_eq!(status.open_trades, 0);
    assert!(status.current_window.is_none());
}

#[tokio::test]
async fn get_status_current_window() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();
    let window = status.current_window.unwrap();
    assert_eq!(window.market_id, "mkt-2");
    assert_eq!(window.end_time, 3000);
}

#[tokio::test]
async fn get_status_uptime_hours() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();
    // 3500 - 500 = 3000ms = 0.000833... hours
    assert!(status.uptime_hours > 0.0);
    assert!(status.uptime_hours < 0.01);
}

#[tokio::test]
async fn get_status_max_drawdown() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();
    // HWM = 250, lowest after = 220, DD = 30/250 = 0.12
    assert!(status.max_drawdown_pct >= 0.11);
    assert!(status.max_drawdown_pct <= 0.13);
}

#[tokio::test]
async fn get_trades_first_page() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_trades(1, 50).await.unwrap();
    assert_eq!(resp.total, 3);
    assert_eq!(resp.trades.len(), 3);
    assert_eq!(resp.page, 1);
    // Newest first
    assert_eq!(resp.trades[0].id, 3);
    assert_eq!(resp.trades[0].status, "open");
    assert!(resp.trades[0].pnl.is_none());
}

#[tokio::test]
async fn get_trades_pagination() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let page1 = reader.get_trades(1, 2).await.unwrap();
    assert_eq!(page1.trades.len(), 2);
    assert_eq!(page1.total, 3);

    let page2 = reader.get_trades(2, 2).await.unwrap();
    assert_eq!(page2.trades.len(), 1);
}

#[tokio::test]
async fn get_trades_with_results() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_trades(1, 50).await.unwrap();
    // Trade 1 (closed, winner)
    let trade1 = resp.trades.iter().find(|t| t.id == 1).unwrap();
    assert_eq!(trade1.status, "closed");
    assert!((trade1.pnl.unwrap() - 55.0).abs() < 0.001);
    assert!((trade1.settlement_price.unwrap() - 1.0).abs() < 0.001);
}

#[tokio::test]
async fn get_trades_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_trades(1, 50).await.unwrap();
    assert_eq!(resp.total, 0);
    assert!(resp.trades.is_empty());
}

#[tokio::test]
async fn get_balance_log_all() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_balance_log(0).await.unwrap();
    assert_eq!(resp.entries.len(), 3);
    assert_eq!(resp.entries[0].event, "init");
    assert_eq!(resp.entries[0].balance, 200.0);
    assert_eq!(resp.entries[2].balance, 220.0);
}

#[tokio::test]
async fn get_balance_log_since_filter() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_balance_log(2500).await.unwrap();
    assert_eq!(resp.entries.len(), 1);
    assert_eq!(resp.entries[0].timestamp, 3000);
}

#[tokio::test]
async fn get_balance_log_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_balance_log(0).await.unwrap();
    assert!(resp.entries.is_empty());
}

#[tokio::test]
async fn get_signals_default_limit() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_signals(100).await.unwrap();
    assert_eq!(resp.signals.len(), 2);
    // Newest first
    assert_eq!(resp.signals[0].strategy, "spread-capture");
}

#[tokio::test]
async fn get_signals_limited() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_signals(1).await.unwrap();
    assert_eq!(resp.signals.len(), 1);
    assert_eq!(resp.signals[0].strategy, "spread-capture");
}

#[tokio::test]
async fn get_signals_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_signals(100).await.unwrap();
    assert!(resp.signals.is_empty());
}

#[tokio::test]
async fn get_stats_by_strategy() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_stats().await.unwrap();
    assert_eq!(resp.by_strategy.len(), 2);

    let la = resp.by_strategy.get("latency-arb").unwrap();
    assert_eq!(la.trades, 1);
    assert_eq!(la.wins, 1);
    assert!((la.total_pnl - 55.0).abs() < 0.001);

    let sc = resp.by_strategy.get("spread-capture").unwrap();
    assert_eq!(sc.trades, 1);
    assert_eq!(sc.losses, 1);
    assert!((sc.total_pnl - (-40.0)).abs() < 0.001);
}

#[tokio::test]
async fn get_stats_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_stats().await.unwrap();
    assert!(resp.by_strategy.is_empty());
}

#[tokio::test]
async fn get_latest_ids() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    assert_eq!(reader.get_latest_trade_id().await.unwrap(), 3);
    assert_eq!(reader.get_latest_balance_id().await.unwrap(), 3);
    assert_eq!(reader.get_latest_signal_id().await.unwrap(), 2);
}

#[tokio::test]
async fn get_latest_ids_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    assert_eq!(reader.get_latest_trade_id().await.unwrap(), 0);
    assert_eq!(reader.get_latest_balance_id().await.unwrap(), 0);
    assert_eq!(reader.get_latest_signal_id().await.unwrap(), 0);
}

#[tokio::test]
async fn get_trades_since() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let trades = reader.get_trades_since(1).await.unwrap();
    assert_eq!(trades.len(), 2);
    assert_eq!(trades[0].id, 2);
    assert_eq!(trades[1].id, 3);
}

#[tokio::test]
async fn get_balance_since() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let entries = reader.get_balance_since(2).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, 3);
}

#[tokio::test]
async fn get_signals_since() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let signals = reader.get_signals_since(1).await.unwrap();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].strategy, "spread-capture");
}

// -- Edge case tests ----------------------------------------------------------

#[tokio::test]
async fn new_opens_existing_db_file() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    // Create a DB with the expected schema.
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE tick_data (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, source TEXT NOT NULL, price REAL, bid REAL, ask REAL, bid_size REAL, ask_size REAL);
         CREATE TABLE markets (id INTEGER PRIMARY KEY AUTOINCREMENT, market_id TEXT NOT NULL UNIQUE, question TEXT NOT NULL, condition_id TEXT NOT NULL, slug TEXT NOT NULL, up_token_id TEXT NOT NULL, down_token_id TEXT NOT NULL, start_time INTEGER NOT NULL, end_time INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'active');
         CREATE TABLE signals (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, strategy TEXT NOT NULL, direction TEXT NOT NULL, binance_price REAL, chainlink_price REAL, up_ask REAL, down_ask REAL, up_bid REAL, down_bid REAL, metadata TEXT);
         CREATE TABLE simulated_trades (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, market_id TEXT NOT NULL, strategy TEXT NOT NULL, side TEXT NOT NULL, token_id TEXT NOT NULL, entry_price REAL NOT NULL, size REAL NOT NULL, status TEXT NOT NULL DEFAULT 'open');
         CREATE TABLE trade_results (id INTEGER PRIMARY KEY AUTOINCREMENT, trade_id INTEGER NOT NULL UNIQUE, exit_price REAL, settlement_price REAL NOT NULL, pnl_0pct REAL NOT NULL, pnl_1pct REAL NOT NULL, pnl_2pct REAL NOT NULL, pnl_3pct REAL NOT NULL, resolved_at INTEGER NOT NULL);
         CREATE TABLE balance_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, event TEXT NOT NULL, trade_id INTEGER, amount REAL NOT NULL, balance REAL NOT NULL);",
    ).unwrap();
    drop(conn);

    let reader = DbReader::new(db_path.to_str().unwrap()).unwrap();
    let status = reader.get_status().await.unwrap();
    assert_eq!(status.total_trades, 0);
}

#[tokio::test]
async fn new_nonexistent_path_returns_error() {
    let result = DbReader::new("/nonexistent/path/db.sqlite");
    assert!(result.is_err());
}

#[tokio::test]
async fn max_drawdown_monotone_up_returns_zero() {
    let conn = fixture_db();
    conn.execute_batch(
        "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (1000, 'init', NULL, 0.0, 100.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (2000, 'win', 1, 50.0, 200.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (3000, 'win', 2, 100.0, 300.0);",
    ).unwrap();

    let reader = DbReader::from_connection(conn);
    let status = reader.get_status().await.unwrap();
    assert!(
        status.max_drawdown_pct.abs() < f64::EPSILON,
        "monotonically increasing balance should have 0% DD, got: {}",
        status.max_drawdown_pct
    );
}

#[tokio::test]
async fn max_drawdown_single_dip() {
    let conn = fixture_db();
    conn.execute_batch(
        "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (1000, 'init', NULL, 0.0, 200.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (2000, 'win', 1, 100.0, 300.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (3000, 'loss', 2, -150.0, 150.0);",
    ).unwrap();

    let reader = DbReader::from_connection(conn);
    let status = reader.get_status().await.unwrap();
    // DD = (300 - 150) / 300 = 0.5
    assert!(
        (status.max_drawdown_pct - 0.5).abs() < 0.001,
        "expected ~50% DD, got: {}",
        status.max_drawdown_pct
    );
}

#[tokio::test]
async fn max_drawdown_empty_returns_zero() {
    let conn = fixture_db();
    // No balance_log entries.
    let reader = DbReader::from_connection(conn);
    let status = reader.get_status().await.unwrap();
    assert!(
        status.max_drawdown_pct.abs() < f64::EPSILON,
        "empty balance log should have 0% DD"
    );
}
