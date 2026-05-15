use std::sync::Arc;
use std::time::Duration;

use axum::middleware;
use axum::routing::{get, post};
use axum::{Extension, Router};
use rusqlite::Connection;
use tokio::sync::broadcast;

use buba_agent::api::{self, AppState};
use buba_agent::auth::{SharedSecret, require_secret};
use buba_agent::db_reader::DbReader;
use buba_agent::process_manager::NoopProcessManager;
use buba_agent::types::WsMessage;
use buba_agent::ws;

/// Create a temp DB file with fixture data and return (path, tempdir).
fn fixture_db_file() -> (String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db").to_str().unwrap().to_string();
    let conn = Connection::open(&path).unwrap();
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
    drop(conn);
    (path, dir)
}

/// Spawns agent.
async fn spawn_agent(db_path: &str) -> String {
    let db = Arc::new(DbReader::new(db_path).unwrap());
    let bot: Arc<dyn buba_agent::process_manager::ProcessManager> =
        Arc::new(NoopProcessManager::new(None));
    let (ws_tx, _) = broadcast::channel::<WsMessage>(16);

    let machine = buba_agent::machine::MachineSampler::with_seeded_state(
        buba_agent::machine::HostIdentity {
            hostname: "integration-host".into(),
            os_name: "integration-os".into(),
            os_version: "1.0".into(),
            kernel_version: "5.0".into(),
            cpu_count: 1,
            total_ram_bytes: 1_024,
        },
        buba_agent::machine::MachineSamplerState::new(),
        0,
        std::path::PathBuf::from(db_path),
    );
    let state = AppState {
        db,
        bot,
        ws_tx: ws_tx.clone(),
        machine,
    };

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/api/status", get(api::get_status))
        .route("/api/trades", get(api::get_trades))
        .route("/api/balance", get(api::get_balance))
        .route("/api/signals", get(api::get_signals))
        .route("/api/stats", get(api::get_stats))
        .route("/api/bot/status", get(api::bot_status))
        .route("/api/bot/start", post(api::bot_start))
        .route("/api/bot/stop", post(api::bot_stop))
        .route("/api/bot/restart", post(api::bot_restart))
        .route(
            "/ws/live",
            get(move |ws_upgrade: axum::extract::WebSocketUpgrade| {
                let rx = ws_tx.subscribe();
                async move { ws_upgrade.on_upgrade(move |socket| ws::handle_ws(socket, rx)) }
            }),
        )
        .layer(middleware::from_fn(require_secret))
        .layer(Extension(SharedSecret("test-secret".to_string())))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    format!("http://127.0.0.1:{}", addr.port())
}

/// Authed get.
fn authed_get(base: &str, path: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header("Authorization", "Bearer test-secret")
}

/// Authed post.
fn authed_post(base: &str, path: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .post(format!("{base}{path}"))
        .header("Authorization", "Bearer test-secret")
}

/// Verifies that full api roundtrip.
#[tokio::test]
async fn full_api_roundtrip() {
    let (db_path, _dir) = fixture_db_file();
    let base = spawn_agent(&db_path).await;

    let resp = authed_get(&base, "/health").send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["ok"], true);

    let resp = authed_get(&base, "/api/status").send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("balance").is_some());

    let resp = authed_get(&base, "/api/trades?page=1&per_page=10")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["trades"].is_array());
    assert!(body["total"].as_u64().unwrap() >= 1);

    let resp = authed_get(&base, "/api/balance?since=0")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["entries"].as_array().unwrap().len() >= 2);

    let resp = authed_get(&base, "/api/signals?limit=50")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["signals"].as_array().unwrap().len() >= 1);

    let resp = authed_get(&base, "/api/stats").send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("by_strategy").is_some());

    let resp = authed_get(&base, "/api/bot/status").send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let resp = authed_post(&base, "/api/bot/start").send().await.unwrap();
    assert_eq!(resp.status(), 409);

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 401);
}

/// Verifies that ws endpoint accepts connection with auth.
#[tokio::test]
async fn ws_endpoint_accepts_connection_with_auth() {
    let (db_path, _dir) = fixture_db_file();
    let base = spawn_agent(&db_path).await;
    let ws_url = base.replace("http://", "ws://") + "/ws/live";

    let request = axum::http::Request::builder()
        .uri(&ws_url)
        .header(
            "Host",
            ws_url.replace("ws://", "").split('/').next().unwrap(),
        )
        .header("Authorization", "Bearer test-secret")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();

    let result = tokio::time::timeout(
        Duration::from_millis(3000),
        tokio_tungstenite::connect_async(request),
    )
    .await;

    assert!(result.is_ok(), "WS connection should not time out");
    assert!(
        result.unwrap().is_ok(),
        "WS connection should succeed with auth"
    );
}

/// Verifies that ws endpoint rejects without auth.
#[tokio::test]
async fn ws_endpoint_rejects_without_auth() {
    let (db_path, _dir) = fixture_db_file();
    let base = spawn_agent(&db_path).await;
    let ws_url = base.replace("http://", "ws://") + "/ws/live";

    let result = tokio_tungstenite::connect_async(&ws_url).await;
    assert!(
        result.is_err(),
        "WS should reject unauthenticated connection"
    );
}
