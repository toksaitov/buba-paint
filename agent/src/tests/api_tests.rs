use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::{get, post};
use axum::{Extension, Router};
use rusqlite::Connection;
use tokio::sync::broadcast;
use tower::ServiceExt as _;

use crate::api::{self, AppState};
use crate::auth::{SharedSecret, require_secret};
use crate::db_reader::DbReader;
use crate::process_manager::NoopProcessManager;
use crate::types::{LiveStatusResponse, WsMessage};

/// Create a fixture DB identical to the one in `db_reader_tests`.
fn fixture_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tick_data (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, source TEXT NOT NULL, price REAL, bid REAL, ask REAL, bid_size REAL, ask_size REAL);
         CREATE TABLE markets (id INTEGER PRIMARY KEY AUTOINCREMENT, market_id TEXT NOT NULL UNIQUE, question TEXT NOT NULL, condition_id TEXT NOT NULL, slug TEXT NOT NULL, up_token_id TEXT NOT NULL, down_token_id TEXT NOT NULL, start_time INTEGER NOT NULL, end_time INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'active');
         CREATE TABLE signals (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, strategy TEXT NOT NULL, direction TEXT NOT NULL, binance_price REAL, chainlink_price REAL, up_ask REAL, down_ask REAL, up_bid REAL, down_bid REAL, metadata TEXT);
         CREATE TABLE simulated_trades (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, market_id TEXT NOT NULL, strategy TEXT NOT NULL, side TEXT NOT NULL, token_id TEXT NOT NULL, entry_price REAL NOT NULL, size REAL NOT NULL, status TEXT NOT NULL DEFAULT 'open');
         CREATE TABLE trade_results (id INTEGER PRIMARY KEY AUTOINCREMENT, trade_id INTEGER NOT NULL UNIQUE, exit_price REAL, settlement_price REAL NOT NULL, pnl_0pct REAL NOT NULL, pnl_1pct REAL NOT NULL, pnl_2pct REAL NOT NULL, pnl_3pct REAL NOT NULL, resolved_at INTEGER NOT NULL);
         CREATE TABLE balance_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, event TEXT NOT NULL, trade_id INTEGER, amount REAL NOT NULL, balance REAL NOT NULL);
         CREATE TABLE live_sessions (id INTEGER PRIMARY KEY AUTOINCREMENT, started_at_ms INTEGER NOT NULL, ended_at_ms INTEGER, status TEXT NOT NULL, execution_mode TEXT NOT NULL, wallet_address TEXT, proxy_wallet TEXT, enabled_strategies_json TEXT NOT NULL, config_fingerprint TEXT NOT NULL, cash_cap_usd REAL NOT NULL, details_json TEXT);
         CREATE TABLE live_orders (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id INTEGER NOT NULL, intent_id INTEGER NOT NULL, venue_order_id TEXT, client_order_id TEXT, market_id TEXT NOT NULL, token_id TEXT, side TEXT NOT NULL, order_type TEXT NOT NULL, status TEXT NOT NULL, status_reason TEXT, created_at_ms INTEGER NOT NULL, acknowledged_at_ms INTEGER, updated_at_ms INTEGER NOT NULL, requested_price REAL, limit_price REAL, requested_size REAL, accepted_size REAL, details_json TEXT);
         CREATE TABLE live_fills (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id INTEGER NOT NULL, intent_id INTEGER, live_order_id INTEGER, venue_trade_id TEXT, filled_at_ms INTEGER NOT NULL, price REAL NOT NULL, size REAL NOT NULL, fee_amount REAL, fee_rate REAL, liquidity_side TEXT, tx_hash TEXT, status TEXT NOT NULL, details_json TEXT);
         CREATE TABLE live_account_snapshots (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id INTEGER NOT NULL, timestamp_ms INTEGER NOT NULL, cash_available REAL NOT NULL, cash_reserved_for_orders REAL NOT NULL, inventory_mark_value REAL NOT NULL, redeemable_value REAL NOT NULL, pending_redeem_value REAL NOT NULL, total_equity REAL NOT NULL, allowance_available REAL, details_json TEXT);
         CREATE TABLE live_redemptions (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id INTEGER NOT NULL, market_id TEXT NOT NULL, detected_redeemable_at_ms INTEGER NOT NULL, submitted_at_ms INTEGER, confirmed_at_ms INTEGER, cash_credit_observed_at_ms INTEGER, status TEXT NOT NULL, redeemable_value REAL NOT NULL, tx_hash TEXT, details_json TEXT);
         CREATE TABLE live_reconciliation_events (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id INTEGER NOT NULL, timestamp_ms INTEGER NOT NULL, severity TEXT NOT NULL, event_type TEXT NOT NULL, local_value REAL, remote_value REAL, details_json TEXT);",
    ).unwrap();

    conn.execute_batch(
        "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (1000, 'init', NULL, 0.0, 200.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (2000, 'settlement', 1, 50.0, 250.0);
         INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status) VALUES (1100, 'mkt-1', 'latency-arb', 'UP', 'tok-up', 0.45, 100.0, 'closed');
         INSERT INTO trade_results (trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at) VALUES (1, 1.0, 1.0, 55.0, 54.0, 53.0, 52.0, 1500);
         INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata) VALUES (1050, 'latency-arb', 'UP', 42000.0, 42001.0, 0.45, 0.55, '{}');
         INSERT INTO tick_data (timestamp, source, price) VALUES (500, 'binance', 42000.0);
         INSERT INTO tick_data (timestamp, source, price) VALUES (3000, 'binance', 42100.0);
         INSERT INTO live_sessions (started_at_ms, ended_at_ms, status, execution_mode, wallet_address, proxy_wallet, enabled_strategies_json, config_fingerprint, cash_cap_usd, details_json) VALUES (4000, NULL, 'readonly_ready', 'live_readonly', '0xwallet', '0xproxy', '[\"latency-arb\"]', 'fingerprint-1', 100.0, '{}');
         INSERT INTO live_account_snapshots (session_id, timestamp_ms, cash_available, cash_reserved_for_orders, inventory_mark_value, redeemable_value, pending_redeem_value, total_equity, allowance_available, details_json) VALUES (1, 4100, 96.0, 0.0, 2.0, 1.0, 0.0, 99.0, 96.0, '{}');
         INSERT INTO live_orders (session_id, intent_id, venue_order_id, client_order_id, market_id, token_id, side, order_type, status, status_reason, created_at_ms, acknowledged_at_ms, updated_at_ms, requested_price, limit_price, requested_size, accepted_size, details_json) VALUES (1, 11, 'venue-1', 'client-1', 'mkt-1', 'tok-up', 'BUY', 'FOK', 'open', NULL, 4200, 4201, 4201, 0.51, 0.51, 5.0, 5.0, '{}');
         INSERT INTO live_fills (session_id, intent_id, live_order_id, venue_trade_id, filled_at_ms, price, size, fee_amount, fee_rate, liquidity_side, tx_hash, status, details_json) VALUES (1, 11, 1, 'trade-1', 4300, 0.51, 5.0, 0.04, 0.072, 'taker', '0xfill', 'confirmed', '{}');
         INSERT INTO live_redemptions (session_id, market_id, detected_redeemable_at_ms, submitted_at_ms, confirmed_at_ms, cash_credit_observed_at_ms, status, redeemable_value, tx_hash, details_json) VALUES (1, 'mkt-1', 4400, 4500, NULL, NULL, 'submitted', 3.5, '0xredeem', '{}');
         INSERT INTO live_reconciliation_events (session_id, timestamp_ms, severity, event_type, local_value, remote_value, details_json) VALUES (1, 4600, 'critical', 'cash_drift', 96.0, 94.0, '{}');",
    ).unwrap();

    conn
}

/// Authed post.
fn authed_post(path: &str) -> Request<Body> {
    Request::post(path)
        .header("authorization", "Bearer test-secret")
        .body(Body::empty())
        .unwrap()
}

/// Test app.
fn test_app(conn: Connection) -> Router {
    test_app_with_bot(conn, None)
}

/// Test app with bot.
fn test_app_with_bot(conn: Connection, log_path: Option<&str>) -> Router {
    let db = Arc::new(DbReader::from_connection(conn));
    let bot: Arc<dyn crate::process_manager::ProcessManager> =
        Arc::new(NoopProcessManager::new(log_path.map(String::from)));
    let (ws_tx, _) = broadcast::channel::<WsMessage>(16);
    let state = AppState { db, bot, ws_tx };

    Router::new()
        .route("/health", get(api::health))
        .route("/api/status", get(api::get_status))
        .route("/api/trades", get(api::get_trades))
        .route("/api/balance", get(api::get_balance))
        .route("/api/equity/series", get(api::get_equity_series))
        .route("/api/signals", get(api::get_signals))
        .route("/api/signals/groups", get(api::get_signal_groups))
        .route("/api/stats", get(api::get_stats))
        .route("/api/trading/summary", get(api::get_trading_summary))
        .route("/api/live/status", get(api::get_live_status))
        .route("/api/live/sessions", get(api::get_live_sessions))
        .route("/api/live/orders", get(api::get_live_orders))
        .route("/api/live/fills", get(api::get_live_fills))
        .route("/api/live/redemptions", get(api::get_live_redemptions))
        .route(
            "/api/live/reconciliation",
            get(api::get_live_reconciliation),
        )
        .route("/api/bot/logs", get(api::get_logs))
        .route("/api/bot/status", get(api::bot_status))
        .route("/api/bot/start", post(api::bot_start))
        .route("/api/bot/stop", post(api::bot_stop))
        .route("/api/bot/restart", post(api::bot_restart))
        .layer(middleware::from_fn(require_secret))
        .layer(Extension(SharedSecret("test-secret".to_string())))
        .with_state(state)
}

/// Authed get.
fn authed_get(path: &str) -> Request<Body> {
    Request::get(path)
        .header("authorization", "Bearer test-secret")
        .body(Body::empty())
        .unwrap()
}

/// Decode one JSON response body into a `serde_json::Value`.
async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Verifies that health returns ok.
#[tokio::test]
async fn health_returns_ok() {
    let app = test_app(fixture_db());
    let resp = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
}

/// Verifies that status returns balance.
#[tokio::test]
async fn status_returns_balance() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_get("/api/status")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["balance"], 250.0);
    assert_eq!(json["starting_balance"], 200.0);
    assert_eq!(json["total_trades"], 1);
    assert_eq!(json["wins"], 1);
}

/// Verifies that status requires auth.
#[tokio::test]
async fn status_requires_auth() {
    let app = test_app(fixture_db());
    let resp = app
        .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that trades returns list.
#[tokio::test]
async fn trades_returns_list() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_get("/api/trades")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["trades"][0]["strategy"], "latency-arb");
}

/// Verifies that trades pagination.
#[tokio::test]
async fn trades_pagination() {
    let app = test_app(fixture_db());
    let resp = app
        .oneshot(authed_get("/api/trades?page=1&per_page=1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["per_page"], 1);
    assert_eq!(json["trades"].as_array().unwrap().len(), 1);
}

/// Verifies that balance returns entries.
#[tokio::test]
async fn balance_returns_entries() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_get("/api/balance")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["entries"].as_array().unwrap().len(), 2);
}

/// Verifies that balance since filter.
#[tokio::test]
async fn balance_since_filter() {
    let app = test_app(fixture_db());
    let resp = app
        .oneshot(authed_get("/api/balance?since=1500"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["entries"].as_array().unwrap().len(), 1);
}

/// Verifies that equity series separates timestamp-zero baseline rows.
#[tokio::test]
async fn equity_series_separates_baseline_from_points() {
    let conn = fixture_db();
    conn.execute(
        "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance)
         VALUES (0, 'init', NULL, 0.0, 180.0)",
        [],
    )
    .unwrap();
    let app = test_app(conn);
    let resp = app.oneshot(authed_get("/api/equity/series")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = json_body(resp).await;
    assert_eq!(json["baseline"]["timestamp"], 0);
    assert_eq!(json["baseline"]["balance"], 180.0);
    assert_eq!(json["points"].as_array().unwrap().len(), 2);
    assert!(
        json["points"]
            .as_array()
            .unwrap()
            .iter()
            .all(|point| point["timestamp"].as_u64().unwrap() > 0)
    );
}

/// Verifies that signals returns list.
#[tokio::test]
async fn signals_returns_list() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_get("/api/signals")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["signals"].as_array().unwrap().len(), 1);
}

/// Verifies that signals limit.
#[tokio::test]
async fn signals_limit() {
    let app = test_app(fixture_db());
    let resp = app
        .oneshot(authed_get("/api/signals?limit=0"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["signals"].as_array().unwrap().is_empty());
}

/// Verifies that grouped signals collapse adjacent duplicate bursts.
#[tokio::test]
async fn signal_groups_collapse_adjacent_duplicates() {
    let conn = fixture_db();
    conn.execute_batch(
        "INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata)
         VALUES (5000, 'calm-persistence', 'DOWN', 42010.0, 42009.0, 0.31, 0.70, '{}');
         INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata)
         VALUES (4990, 'calm-persistence', 'DOWN', 42010.0, 42009.0, 0.31, 0.70, '{}');
         INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata)
         VALUES (4980, 'calm-persistence', 'DOWN', 42010.0, 42009.0, 0.31, 0.70, '{}');
         INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata)
         VALUES (100, 'calm-persistence', 'DOWN', 42010.0, 42009.0, 0.31, 0.70, '{}');",
    )
    .unwrap();
    let app = test_app(conn);
    let resp = app
        .oneshot(authed_get("/api/signals/groups?limit=2&quiet_gap_ms=100"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = json_body(resp).await;
    let groups = json["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["strategy"], "calm-persistence");
    assert_eq!(groups[0]["direction"], "DOWN");
    assert_eq!(groups[0]["count"], 3);
    assert_eq!(groups[0]["start_timestamp"], 4980);
    assert_eq!(groups[0]["end_timestamp"], 5000);
    assert_eq!(json["quiet_gap_ms"], 100);
}

/// Verifies that stats returns by strategy.
#[tokio::test]
async fn stats_returns_by_strategy() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_get("/api/stats")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let la = &json["by_strategy"]["latency-arb"];
    assert_eq!(la["trades"], 1);
    assert_eq!(la["wins"], 1);
}

/// Verifies that live status returns the current live summary.
#[tokio::test]
async fn live_status_returns_summary() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_get("/api/live/status")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["open_orders"], 1);
    assert_eq!(json["pending_redemptions"], 1);
    assert_eq!(json["critical_reconciliation_events"], 1);
}

/// Verifies that trading summary returns the derived dashboard model.
#[tokio::test]
async fn trading_summary_returns_derived_model() {
    let app = test_app(fixture_db());
    let resp = app
        .oneshot(authed_get("/api/trading/summary"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["runtime_mode"], "live_readonly");
    assert_eq!(body["trading_state"], "readonly");
    assert_eq!(body["process_state"], "monitoring");
    assert_eq!(body["shadow_summary"]["balance"], 250.0);
    assert_eq!(body["real_account_summary"]["available_cash"], 96.0);
    assert_eq!(body["venue_health"]["label"], "Venue state incomplete");
    assert_eq!(
        body["capabilities"]["arm"]["enabled"],
        serde_json::Value::Bool(false)
    );
    assert!(
        body["alerts"]
            .as_array()
            .is_some_and(|alerts| !alerts.is_empty())
    );
}

/// Verifies that live table endpoints return seeded rows and honor the query limit.
#[tokio::test]
async fn live_table_endpoints_return_rows() {
    let app = test_app(fixture_db());

    for path in [
        "/api/live/sessions",
        "/api/live/orders?limit=1",
        "/api/live/fills?limit=1",
        "/api/live/redemptions?limit=1",
        "/api/live/reconciliation?limit=1",
    ] {
        let resp = app.clone().oneshot(authed_get(path)).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "unexpected status for {path}"
        );
    }

    let sessions = json_body(
        app.clone()
            .oneshot(authed_get("/api/live/sessions"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(sessions["sessions"].as_array().unwrap().len(), 1);

    let orders = json_body(
        app.clone()
            .oneshot(authed_get("/api/live/orders?limit=1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(orders["orders"].as_array().unwrap().len(), 1);

    let fills = json_body(
        app.clone()
            .oneshot(authed_get("/api/live/fills?limit=1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(fills["fills"].as_array().unwrap().len(), 1);

    let redemptions = json_body(
        app.clone()
            .oneshot(authed_get("/api/live/redemptions?limit=1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(redemptions["redemptions"].as_array().unwrap().len(), 1);

    let reconciliation = json_body(
        app.oneshot(authed_get("/api/live/reconciliation?limit=1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(reconciliation["events"].as_array().unwrap().len(), 1);
}

/// Verifies that empty db returns defaults.
#[tokio::test]
async fn empty_db_returns_defaults() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tick_data (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, source TEXT NOT NULL, price REAL, bid REAL, ask REAL, bid_size REAL, ask_size REAL);
         CREATE TABLE markets (id INTEGER PRIMARY KEY AUTOINCREMENT, market_id TEXT NOT NULL UNIQUE, question TEXT NOT NULL, condition_id TEXT NOT NULL, slug TEXT NOT NULL, up_token_id TEXT NOT NULL, down_token_id TEXT NOT NULL, start_time INTEGER NOT NULL, end_time INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'active');
         CREATE TABLE signals (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, strategy TEXT NOT NULL, direction TEXT NOT NULL, binance_price REAL, chainlink_price REAL, up_ask REAL, down_ask REAL, up_bid REAL, down_bid REAL, metadata TEXT);
         CREATE TABLE simulated_trades (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, market_id TEXT NOT NULL, strategy TEXT NOT NULL, side TEXT NOT NULL, token_id TEXT NOT NULL, entry_price REAL NOT NULL, size REAL NOT NULL, status TEXT NOT NULL DEFAULT 'open');
         CREATE TABLE trade_results (id INTEGER PRIMARY KEY AUTOINCREMENT, trade_id INTEGER NOT NULL UNIQUE, exit_price REAL, settlement_price REAL NOT NULL, pnl_0pct REAL NOT NULL, pnl_1pct REAL NOT NULL, pnl_2pct REAL NOT NULL, pnl_3pct REAL NOT NULL, resolved_at INTEGER NOT NULL);
         CREATE TABLE balance_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, event TEXT NOT NULL, trade_id INTEGER, amount REAL NOT NULL, balance REAL NOT NULL);",
    ).unwrap();

    let app = test_app(conn);
    let resp = app.oneshot(authed_get("/api/status")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["balance"], 0.0);
    assert_eq!(json["total_trades"], 0);
}

/// Verifies that bot start noop returns 409.
#[tokio::test]
async fn bot_start_noop_returns_409() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_post("/api/bot/start")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("monitor-only"));
}

/// Verifies that bot stop noop returns 409.
#[tokio::test]
async fn bot_stop_noop_returns_409() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_post("/api/bot/stop")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("monitor-only"));
}

/// Verifies that bot restart noop returns 409.
#[tokio::test]
async fn bot_restart_noop_returns_409() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_post("/api/bot/restart")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("monitor-only"));
}

/// Verifies that bot status noop returns inactive.
#[tokio::test]
async fn bot_status_noop_returns_inactive() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_get("/api/bot/status")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["active"], false);
    assert_eq!(json["control_available"], false);
    assert!(json["pid"].is_null());
}

/// Verifies that get logs returns lines from log file.
#[tokio::test]
async fn get_logs_returns_lines_from_log_file() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("bot.log");
    std::fs::write(&log_path, "line1\nline2\nline3\n").unwrap();

    let app = test_app_with_bot(fixture_db(), Some(log_path.to_str().unwrap()));
    let resp = app
        .oneshot(authed_get("/api/bot/logs?lines=10"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let lines = json["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "line1");
}

/// Verifies that get logs without log path.
#[tokio::test]
async fn get_logs_without_log_path() {
    let app = test_app(fixture_db());
    let resp = app.oneshot(authed_get("/api/bot/logs")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let lines = json["lines"].as_array().unwrap();

    assert!(!lines.is_empty());
}

/// Builds an empty live-status payload for helper tests.
fn empty_live_status() -> LiveStatusResponse {
    LiveStatusResponse {
        latest_session: None,
        latest_account_snapshot: None,
        open_orders: 0,
        pending_redemptions: 0,
        critical_reconciliation_events: 0,
    }
}

/// Verifies that details helpers parse object payloads and ignore invalid shapes.
#[test]
fn details_helpers_parse_expected_shapes() {
    let parsed = super::parse_details(Some(
        r#"{"provider":"stub","last_user_stream_connected_at_ms":1234,"ignored":[1,2]}"#,
    ))
    .unwrap();
    assert_eq!(
        super::detail_string(Some(&parsed), "provider").as_deref(),
        Some("stub")
    );
    assert_eq!(
        super::detail_u64(Some(&parsed), "last_user_stream_connected_at_ms"),
        Some(1234)
    );
    assert!(super::detail_string(Some(&parsed), "missing").is_none());
    assert!(super::parse_details(Some("[]")).is_none());
    assert!(super::parse_details(Some("not-json")).is_none());
    assert!(super::parse_details(None).is_none());
}

/// Verifies that enabled strategies parsing filters out invalid entries.
#[test]
fn parse_enabled_strategies_filters_non_strings() {
    assert_eq!(
        super::parse_enabled_strategies(r#"["latency-arb",7,"calm-persistence",null]"#),
        vec!["latency-arb".to_string(), "calm-persistence".to_string()]
    );
    assert!(super::parse_enabled_strategies(r#"{"not":"an-array"}"#).is_empty());
}

/// Verifies that process-state derivation distinguishes running, monitoring, and stopped.
#[test]
fn derive_process_state_covers_runtime_modes() {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert_eq!(super::derive_process_state(true, true, None), "running");
    assert_eq!(
        super::derive_process_state(false, false, Some(now_ms)),
        "running"
    );
    assert_eq!(
        super::derive_process_state(false, false, Some(1)),
        "monitoring"
    );
    assert_eq!(super::derive_process_state(false, true, None), "stopped");
}

/// Verifies that trading-state derivation handles readonly degradation and gating.
#[test]
fn derive_trading_state_covers_modes() {
    assert_eq!(
        super::derive_trading_state("live_readonly", Some("readonly_failed")),
        "degraded"
    );
    assert_eq!(
        super::derive_trading_state("live_readonly", Some("readonly_ready")),
        "readonly"
    );
    assert_eq!(super::derive_trading_state("live_trading", None), "gated");
    assert_eq!(super::derive_trading_state("paper", None), "paper");
}

/// Verifies that the trading-health helpers cover paper, missing, degraded, and healthy states.
#[test]
fn trading_health_helpers_cover_branches() {
    let venue_paper = super::build_venue_health("paper", false, None, None);
    assert_eq!(venue_paper.state, "idle");

    let venue_missing = super::build_venue_health("live_readonly", false, None, None);
    assert_eq!(venue_missing.state, "critical");

    let venue_stub = super::build_venue_health("live_readonly", true, Some("stub"), None);
    assert_eq!(venue_stub.label, "Stub provider");

    let venue_ok = super::build_venue_health("live_readonly", true, Some("polymarket"), Some("ok"));
    assert_eq!(venue_ok.state, "healthy");

    let venue_degraded =
        super::build_venue_health("live_readonly", true, Some("polymarket"), Some("down"));
    assert_eq!(venue_degraded.label, "User stream degraded");

    let venue_incomplete =
        super::build_venue_health("live_readonly", true, Some("polymarket"), None);
    assert_eq!(venue_incomplete.label, "Venue state incomplete");

    let account_paper = super::build_account_health("paper", false, None);
    assert_eq!(account_paper.state, "idle");

    let account_missing = super::build_account_health("live_readonly", false, None);
    assert_eq!(account_missing.label, "No account snapshot");

    let account_unknown = super::build_account_health("live_readonly", true, None);
    assert_eq!(account_unknown.label, "Allowance unknown");

    let account_ok = super::build_account_health("live_readonly", true, Some(10.0));
    assert_eq!(account_ok.state, "healthy");

    let recon_paper = super::build_reconciliation_health("paper", 0, 0, 0);
    assert_eq!(recon_paper.state, "idle");

    let recon_critical = super::build_reconciliation_health("live_readonly", 2, 0, 0);
    assert_eq!(recon_critical.state, "critical");

    let recon_pending = super::build_reconciliation_health("live_readonly", 0, 1, 2);
    assert_eq!(recon_pending.label, "Pending activity");

    let recon_ok = super::build_reconciliation_health("live_readonly", 0, 0, 0);
    assert_eq!(recon_ok.state, "healthy");
}

/// Verifies that capability and alert helpers expose the expected gating reasons.
#[test]
fn trading_capabilities_and_alerts_cover_branches() {
    let paper_capabilities = super::build_trading_capabilities("paper");
    assert!(paper_capabilities.preflight.reason.contains("Paper mode"));
    assert!(!paper_capabilities.arm.enabled);

    let live_capabilities = super::build_trading_capabilities("live_readonly");
    assert!(live_capabilities.preflight.reason.contains("not wired"));
    assert!(
        live_capabilities
            .kill_switch
            .reason
            .contains("no dashboard action endpoint")
    );

    let mut missing_allowance = empty_live_status();
    missing_allowance.latest_account_snapshot = Some(crate::types::LiveAccountSnapshotRow {
        id: 1,
        session_id: 1,
        timestamp_ms: 1000,
        cash_available: 10.0,
        cash_reserved_for_orders: 0.0,
        inventory_mark_value: 0.0,
        redeemable_value: 0.0,
        pending_redeem_value: 0.0,
        total_equity: 10.0,
        allowance_available: None,
        details_json: None,
    });

    let readonly_alerts = super::build_trading_alerts(
        "live_readonly",
        Some("stub"),
        Some("down"),
        &LiveStatusResponse {
            open_orders: 2,
            critical_reconciliation_events: 3,
            ..missing_allowance.clone()
        },
        "stopped",
    );
    let readonly_titles = readonly_alerts
        .iter()
        .map(|alert| alert.title.as_str())
        .collect::<Vec<_>>();
    assert!(readonly_titles.contains(&"Process stopped"));
    assert!(readonly_titles.contains(&"Shadow and execution views differ"));
    assert!(readonly_titles.contains(&"No live session"));
    assert!(readonly_titles.contains(&"Stub provider"));
    assert!(readonly_titles.contains(&"User stream degraded"));
    assert!(readonly_titles.contains(&"Unexpected remote open orders"));
    assert!(readonly_titles.contains(&"Allowance missing"));
    assert!(readonly_titles.contains(&"Critical reconciliation events"));

    let live_trading_alerts =
        super::build_trading_alerts("live_trading", None, None, &empty_live_status(), "running");
    assert!(
        live_trading_alerts
            .iter()
            .any(|alert| alert.title == "Live trading gated")
    );

    let paper_alerts =
        super::build_trading_alerts("paper", None, None, &empty_live_status(), "running");
    assert!(paper_alerts.is_empty());
}
