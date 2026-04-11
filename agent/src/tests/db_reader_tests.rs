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
        );
        CREATE TABLE live_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at_ms INTEGER NOT NULL,
            ended_at_ms INTEGER,
            status TEXT NOT NULL,
            execution_mode TEXT NOT NULL,
            wallet_address TEXT,
            proxy_wallet TEXT,
            enabled_strategies_json TEXT NOT NULL,
            config_fingerprint TEXT NOT NULL,
            cash_cap_usd REAL NOT NULL,
            details_json TEXT
        );
        CREATE TABLE live_orders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            intent_id INTEGER NOT NULL,
            venue_order_id TEXT,
            client_order_id TEXT,
            market_id TEXT NOT NULL,
            token_id TEXT,
            side TEXT NOT NULL,
            order_type TEXT NOT NULL,
            status TEXT NOT NULL,
            status_reason TEXT,
            created_at_ms INTEGER NOT NULL,
            acknowledged_at_ms INTEGER,
            updated_at_ms INTEGER NOT NULL,
            requested_price REAL,
            limit_price REAL,
            requested_size REAL,
            accepted_size REAL,
            details_json TEXT
        );
        CREATE TABLE live_fills (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            intent_id INTEGER,
            live_order_id INTEGER,
            venue_trade_id TEXT,
            filled_at_ms INTEGER NOT NULL,
            price REAL NOT NULL,
            size REAL NOT NULL,
            fee_amount REAL,
            fee_rate REAL,
            liquidity_side TEXT,
            tx_hash TEXT,
            status TEXT NOT NULL,
            details_json TEXT
        );
        CREATE TABLE live_account_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            cash_available REAL NOT NULL,
            cash_reserved_for_orders REAL NOT NULL,
            inventory_mark_value REAL NOT NULL,
            redeemable_value REAL NOT NULL,
            pending_redeem_value REAL NOT NULL,
            total_equity REAL NOT NULL,
            allowance_available REAL,
            details_json TEXT
        );
        CREATE TABLE live_redemptions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            market_id TEXT NOT NULL,
            detected_redeemable_at_ms INTEGER NOT NULL,
            submitted_at_ms INTEGER,
            confirmed_at_ms INTEGER,
            cash_credit_observed_at_ms INTEGER,
            status TEXT NOT NULL,
            redeemable_value REAL NOT NULL,
            tx_hash TEXT,
            details_json TEXT
        );
        CREATE TABLE live_reconciliation_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            severity TEXT NOT NULL,
            event_type TEXT NOT NULL,
            local_value REAL,
            remote_value REAL,
            details_json TEXT
        );",
    )
    .unwrap();
    conn
}

/// Insert standard fixture data into the DB.
fn seed_fixtures(conn: &Connection) {
    conn.execute_batch(
        "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance)
         VALUES (1000, 'init', NULL, 0.0, 200.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance)
         VALUES (2000, 'settlement', 1, 50.0, 250.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance)
         VALUES (3000, 'settlement', 2, -30.0, 220.0);",
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO markets (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status)
         VALUES ('mkt-1', 'Will BTC go up?', 'cond-1', 'btc-updown-5m-100', 'tok-up', 'tok-down', 1000, 2000, 'resolved');
         INSERT INTO markets (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status)
         VALUES ('mkt-2', 'Will BTC go up?', 'cond-2', 'btc-updown-5m-200', 'tok-up-2', 'tok-down-2', 2000, 3000, 'active');",
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status)
         VALUES (1100, 'mkt-1', 'latency-arb', 'UP', 'tok-up', 0.45, 100.0, 'closed');
         INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status)
         VALUES (2100, 'mkt-2', 'spread-capture', 'DOWN', 'tok-down-2', 0.50, 80.0, 'closed');
         INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status)
         VALUES (2500, 'mkt-2', 'latency-arb', 'UP', 'tok-up-2', 0.42, 60.0, 'open');",
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO trade_results (trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at)
         VALUES (1, 1.0, 1.0, 55.0, 54.0, 53.0, 52.0, 1500);
         INSERT INTO trade_results (trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at)
         VALUES (2, 0.0, 0.0, -40.0, -40.8, -41.6, -42.4, 2500);",
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata)
         VALUES (1050, 'latency-arb', 'UP', 42000.0, 42001.0, 0.45, 0.55, '{\"momentum\": 0.003}');
         INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata)
         VALUES (2050, 'spread-capture', 'DOWN', 42100.0, 42099.0, 0.48, 0.50, '{\"spread\": 0.98}');",
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO tick_data (timestamp, source, price) VALUES (500, 'binance', 42000.0);
         INSERT INTO tick_data (timestamp, source, price) VALUES (3500, 'binance', 42100.0);",
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO live_sessions (started_at_ms, ended_at_ms, status, execution_mode, wallet_address, proxy_wallet, enabled_strategies_json, config_fingerprint, cash_cap_usd, details_json)
         VALUES (4000, NULL, 'readonly_ready', 'live_readonly', '0xwallet', '0xproxy', '[\"latency-arb\"]', 'fingerprint-1', 100.0, '{}');
         INSERT INTO live_account_snapshots (session_id, timestamp_ms, cash_available, cash_reserved_for_orders, inventory_mark_value, redeemable_value, pending_redeem_value, total_equity, allowance_available, details_json)
         VALUES (1, 4100, 96.0, 0.0, 2.0, 1.0, 0.0, 99.0, 96.0, '{}');
         INSERT INTO live_orders (session_id, intent_id, venue_order_id, client_order_id, market_id, token_id, side, order_type, status, status_reason, created_at_ms, acknowledged_at_ms, updated_at_ms, requested_price, limit_price, requested_size, accepted_size, details_json)
         VALUES (1, 11, 'venue-1', 'client-1', 'mkt-1', 'tok-up', 'BUY', 'FOK', 'open', NULL, 4200, 4201, 4201, 0.51, 0.51, 5.0, 5.0, '{}');
         INSERT INTO live_fills (session_id, intent_id, live_order_id, venue_trade_id, filled_at_ms, price, size, fee_amount, fee_rate, liquidity_side, tx_hash, status, details_json)
         VALUES (1, 11, 1, 'trade-1', 4300, 0.51, 5.0, 0.09, 0.072, 'taker', '0xtx', 'confirmed', '{}');
         INSERT INTO live_redemptions (session_id, market_id, detected_redeemable_at_ms, submitted_at_ms, confirmed_at_ms, cash_credit_observed_at_ms, status, redeemable_value, tx_hash, details_json)
         VALUES (1, 'mkt-1', 4400, 4500, NULL, NULL, 'submitted', 3.5, '0xredeem', '{}');
         INSERT INTO live_reconciliation_events (session_id, timestamp_ms, severity, event_type, local_value, remote_value, details_json)
         VALUES (1, 4600, 'critical', 'cash_drift', 96.0, 94.0, '{}');",
    )
    .unwrap();
}

/// Verifies that get status with fixture data.
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
    assert!((status.total_pnl - 15.0).abs() < 0.001);
    assert_eq!(status.high_water_mark, 250.0);
    assert_eq!(status.open_trades, 1);
}

/// Verifies that get status empty db.
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

/// Verifies that get status current window.
#[tokio::test]
async fn get_status_current_window() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    conn.execute("DELETE FROM tick_data WHERE timestamp = 3500", [])
        .unwrap();
    conn.execute(
        "INSERT INTO tick_data (timestamp, source, price) VALUES (2500, 'binance', 42050.0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status)
         VALUES ('mkt-3', 'Future BTC window', 'cond-3', 'btc-updown-5m-300', 'tok-up-3', 'tok-down-3', 4000, 5000, 'active')",
        [],
    )
    .unwrap();
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();
    let window = status.current_window.unwrap();
    assert_eq!(window.market_id, "mkt-2");
    assert_eq!(window.end_time, 3000);
}

/// Verifies that get status uptime hours.
#[tokio::test]
async fn get_status_uptime_hours() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();

    assert!(status.uptime_hours > 0.0);
    assert!(status.uptime_hours < 0.01);
}

/// Verifies that get status max drawdown.
#[tokio::test]
async fn get_status_max_drawdown() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();

    assert!(status.max_drawdown_pct >= 0.11);
    assert!(status.max_drawdown_pct <= 0.13);
}

/// Verifies that get trades first page.
#[tokio::test]
async fn get_trades_first_page() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_trades(1, 50).await.unwrap();
    assert_eq!(resp.total, 3);
    assert_eq!(resp.trades.len(), 3);
    assert_eq!(resp.page, 1);

    assert_eq!(resp.trades[0].id, 3);
    assert_eq!(resp.trades[0].status, "open");
    assert!(resp.trades[0].pnl.is_none());
}

/// Verifies that get trades pagination.
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

/// Verifies that get trades with results.
#[tokio::test]
async fn get_trades_with_results() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_trades(1, 50).await.unwrap();

    let trade1 = resp.trades.iter().find(|t| t.id == 1).unwrap();
    assert_eq!(trade1.status, "closed");
    assert!((trade1.pnl.unwrap() - 55.0).abs() < 0.001);
    assert!((trade1.settlement_price.unwrap() - 1.0).abs() < 0.001);
}

/// Verifies that get status prefers pnl net when available.
#[tokio::test]
async fn get_status_prefers_pnl_net_when_available() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    conn.execute_batch(
        "ALTER TABLE trade_results ADD COLUMN pnl_net REAL;
         UPDATE trade_results SET pnl_net = 60.0 WHERE trade_id = 1;
         UPDATE trade_results SET pnl_net = -10.0 WHERE trade_id = 2;",
    )
    .unwrap();
    let reader = DbReader::from_connection(conn);

    let status = reader.get_status().await.unwrap();
    assert!((status.total_pnl - 50.0).abs() < 0.001);
    assert_eq!(status.wins, 1);
    assert_eq!(status.losses, 1);
}

/// Verifies that get trades reads optional execution columns when present.
#[tokio::test]
async fn get_trades_reads_optional_execution_columns_when_present() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    conn.execute_batch(
        "ALTER TABLE trade_results ADD COLUMN pnl_net REAL;
         ALTER TABLE simulated_trades ADD COLUMN fill_status TEXT;
         ALTER TABLE simulated_trades ADD COLUMN execution_group_id TEXT;
         ALTER TABLE simulated_trades ADD COLUMN execution_fidelity TEXT;
         ALTER TABLE simulated_trades ADD COLUMN filled_size REAL;
         ALTER TABLE simulated_trades ADD COLUMN avg_fill_price REAL;
         UPDATE trade_results SET pnl_net = 54.5 WHERE trade_id = 1;
         UPDATE simulated_trades
            SET fill_status = 'filled',
                execution_group_id = 'spread-1',
                execution_fidelity = 'legacy_snapshot',
                filled_size = 100.0,
                avg_fill_price = 0.45
          WHERE id = 1;",
    )
    .unwrap();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_trades(1, 10).await.unwrap();
    let trade = resp.trades.iter().find(|trade| trade.id == 1).unwrap();
    assert_eq!(trade.pnl, Some(54.5));
    assert_eq!(trade.fill_status.as_deref(), Some("filled"));
    assert_eq!(trade.execution_group_id.as_deref(), Some("spread-1"));
    assert_eq!(trade.execution_fidelity.as_deref(), Some("legacy_snapshot"));
    assert_eq!(trade.filled_size, Some(100.0));
    assert_eq!(trade.avg_fill_price, Some(0.45));
}

/// Verifies that get trades legacy schema leaves optional execution fields empty.
#[tokio::test]
async fn get_trades_legacy_schema_leaves_optional_execution_fields_empty() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_trades(1, 10).await.unwrap();
    let trade = resp.trades.iter().find(|trade| trade.id == 1).unwrap();
    assert!(trade.fill_status.is_none());
    assert!(trade.execution_group_id.is_none());
    assert!(trade.execution_fidelity.is_none());
    assert!(trade.filled_size.is_none());
    assert!(trade.avg_fill_price.is_none());
}

/// Verifies that get trades empty db.
#[tokio::test]
async fn get_trades_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_trades(1, 50).await.unwrap();
    assert_eq!(resp.total, 0);
    assert!(resp.trades.is_empty());
}

/// Verifies that get balance log all.
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

/// Verifies that get balance log since filter.
#[tokio::test]
async fn get_balance_log_since_filter() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_balance_log(2500).await.unwrap();
    assert_eq!(resp.entries.len(), 1);
    assert_eq!(resp.entries[0].timestamp, 3000);
}

/// Verifies that get balance log empty db.
#[tokio::test]
async fn get_balance_log_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_balance_log(0).await.unwrap();
    assert!(resp.entries.is_empty());
}

/// Verifies that get signals default limit.
#[tokio::test]
async fn get_signals_default_limit() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_signals(100).await.unwrap();
    assert_eq!(resp.signals.len(), 2);

    assert_eq!(resp.signals[0].strategy, "spread-capture");
}

/// Verifies that get signals limited.
#[tokio::test]
async fn get_signals_limited() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_signals(1).await.unwrap();
    assert_eq!(resp.signals.len(), 1);
    assert_eq!(resp.signals[0].strategy, "spread-capture");
}

/// Verifies that get signals empty db.
#[tokio::test]
async fn get_signals_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_signals(100).await.unwrap();
    assert!(resp.signals.is_empty());
}

/// Verifies that get stats by strategy.
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

/// Verifies that get stats empty db.
#[tokio::test]
async fn get_stats_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    let resp = reader.get_stats().await.unwrap();
    assert!(resp.by_strategy.is_empty());
}

/// Verifies that get latest ids.
#[tokio::test]
async fn get_latest_ids() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    assert_eq!(reader.get_latest_trade_id().await.unwrap(), 3);
    assert_eq!(reader.get_latest_balance_id().await.unwrap(), 3);
    assert_eq!(reader.get_latest_signal_id().await.unwrap(), 2);
}

/// Verifies that get latest ids empty db.
#[tokio::test]
async fn get_latest_ids_empty_db() {
    let conn = fixture_db();
    let reader = DbReader::from_connection(conn);

    assert_eq!(reader.get_latest_trade_id().await.unwrap(), 0);
    assert_eq!(reader.get_latest_balance_id().await.unwrap(), 0);
    assert_eq!(reader.get_latest_signal_id().await.unwrap(), 0);
}

/// Verifies that get trades since.
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

/// Verifies that get balance since.
#[tokio::test]
async fn get_balance_since() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let entries = reader.get_balance_since(2).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, 3);
}

/// Verifies that get signals since.
#[tokio::test]
async fn get_signals_since() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let signals = reader.get_signals_since(1).await.unwrap();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].strategy, "spread-capture");
}

/// Verifies that live status summarizes the additive live tables.
#[tokio::test]
async fn get_live_status_with_fixture_data() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    let status = reader.get_live_status().await.unwrap();
    assert_eq!(status.open_orders, 1);
    assert_eq!(status.pending_redemptions, 1);
    assert_eq!(status.critical_reconciliation_events, 1);
    assert_eq!(
        status
            .latest_session
            .as_ref()
            .map(|session| session.execution_mode.as_str()),
        Some("live_readonly")
    );
    assert_eq!(
        status
            .latest_account_snapshot
            .as_ref()
            .map(|snapshot| snapshot.cash_available),
        Some(96.0)
    );
}

/// Verifies that live table readers return recent rows.
#[tokio::test]
async fn live_table_queries_return_rows() {
    let conn = fixture_db();
    seed_fixtures(&conn);
    let reader = DbReader::from_connection(conn);

    assert_eq!(
        reader.get_live_sessions(10).await.unwrap().sessions.len(),
        1
    );
    assert_eq!(reader.get_live_orders(10).await.unwrap().orders.len(), 1);
    assert_eq!(reader.get_live_fills(10).await.unwrap().fills.len(), 1);
    assert_eq!(
        reader
            .get_live_redemptions(10)
            .await
            .unwrap()
            .redemptions
            .len(),
        1
    );
    assert_eq!(
        reader
            .get_live_reconciliation(10)
            .await
            .unwrap()
            .events
            .len(),
        1
    );
}

/// Verifies that new opens existing db file.
#[tokio::test]
async fn new_opens_existing_db_file() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

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

/// Verifies that new nonexistent path returns error.
#[tokio::test]
async fn new_nonexistent_path_returns_error() {
    let result = DbReader::new("/nonexistent/path/db.sqlite");
    assert!(result.is_err());
}

/// Verifies that max drawdown monotone up returns zero.
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

/// Verifies that max drawdown single dip.
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

    assert!(
        (status.max_drawdown_pct - 0.5).abs() < 0.001,
        "expected ~50% DD, got: {}",
        status.max_drawdown_pct
    );
}

/// Verifies that max drawdown empty returns zero.
#[tokio::test]
async fn max_drawdown_empty_returns_zero() {
    let conn = fixture_db();

    let reader = DbReader::from_connection(conn);
    let status = reader.get_status().await.unwrap();
    assert!(
        status.max_drawdown_pct.abs() < f64::EPSILON,
        "empty balance log should have 0% DD"
    );
}
