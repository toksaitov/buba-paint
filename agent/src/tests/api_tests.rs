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
use crate::types::WsMessage;

/// Create a fixture DB identical to the one in `db_reader_tests`.
fn fixture_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tick_data (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, source TEXT NOT NULL, price REAL, bid REAL, ask REAL, bid_size REAL, ask_size REAL);
         CREATE TABLE markets (id INTEGER PRIMARY KEY AUTOINCREMENT, market_id TEXT NOT NULL UNIQUE, question TEXT NOT NULL, condition_id TEXT NOT NULL, slug TEXT NOT NULL, up_token_id TEXT NOT NULL, down_token_id TEXT NOT NULL, start_time INTEGER NOT NULL, end_time INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'active');
         CREATE TABLE signals (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, strategy TEXT NOT NULL, direction TEXT NOT NULL, binance_price REAL, chainlink_price REAL, up_ask REAL, down_ask REAL, up_bid REAL, down_bid REAL, metadata TEXT);
         CREATE TABLE simulated_trades (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, market_id TEXT NOT NULL, strategy TEXT NOT NULL, side TEXT NOT NULL, token_id TEXT NOT NULL, entry_price REAL NOT NULL, size REAL NOT NULL, status TEXT NOT NULL DEFAULT 'open');
         CREATE TABLE trade_results (id INTEGER PRIMARY KEY AUTOINCREMENT, trade_id INTEGER NOT NULL UNIQUE, exit_price REAL, settlement_price REAL NOT NULL, pnl_0pct REAL NOT NULL, pnl_1pct REAL NOT NULL, pnl_2pct REAL NOT NULL, pnl_3pct REAL NOT NULL, resolved_at INTEGER NOT NULL);
         CREATE TABLE balance_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, event TEXT NOT NULL, trade_id INTEGER, amount REAL NOT NULL, balance REAL NOT NULL);",
    ).unwrap();

    conn.execute_batch(
        "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (1000, 'init', NULL, 0.0, 200.0);
         INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (2000, 'settlement', 1, 50.0, 250.0);
         INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status) VALUES (1100, 'mkt-1', 'latency-arb', 'UP', 'tok-up', 0.45, 100.0, 'closed');
         INSERT INTO trade_results (trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at) VALUES (1, 1.0, 1.0, 55.0, 54.0, 53.0, 52.0, 1500);
         INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata) VALUES (1050, 'latency-arb', 'UP', 42000.0, 42001.0, 0.45, 0.55, '{}');
         INSERT INTO tick_data (timestamp, source, price) VALUES (500, 'binance', 42000.0);
         INSERT INTO tick_data (timestamp, source, price) VALUES (3000, 'binance', 42100.0);",
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
        .route("/api/signals", get(api::get_signals))
        .route("/api/stats", get(api::get_stats))
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
