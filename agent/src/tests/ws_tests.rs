use tokio::sync::broadcast;

use crate::types::{BalanceEntry, SignalRow, TradeRow, WsMessage};

/// Verifies that ws message serializes trade.
#[tokio::test]
async fn ws_message_serializes_trade() {
    let trade = TradeRow {
        id: 1,
        timestamp: 1000,
        market_id: "mkt-1".to_string(),
        strategy: "latency-arb".to_string(),
        side: "UP".to_string(),
        entry_price: 0.45,
        size: 100.0,
        status: "closed".to_string(),
        pnl: Some(55.0),
        settlement_price: Some(1.0),
        resolved_at: Some(1500),
        fill_status: None,
        execution_group_id: None,
        execution_fidelity: None,
        filled_size: None,
        avg_fill_price: None,
    };
    let msg = WsMessage::Trade(trade);
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "trade");
    assert_eq!(parsed["data"]["strategy"], "latency-arb");
    assert_eq!(parsed["data"]["pnl"], 55.0);
}

/// Verifies that ws message serializes balance.
#[tokio::test]
async fn ws_message_serializes_balance() {
    let entry = BalanceEntry {
        id: 1,
        timestamp: 1000,
        event: "init".to_string(),
        trade_id: None,
        amount: 0.0,
        balance: 200.0,
    };
    let msg = WsMessage::Balance(entry);
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "balance");
    assert_eq!(parsed["data"]["balance"], 200.0);
    assert!(parsed["data"]["trade_id"].is_null());
}

/// Verifies that ws message serializes signal.
#[tokio::test]
async fn ws_message_serializes_signal() {
    let signal = SignalRow {
        id: 1,
        timestamp: 1000,
        strategy: "spread-capture".to_string(),
        direction: "DOWN".to_string(),
        binance_price: Some(42000.0),
        chainlink_price: Some(42001.0),
        up_ask: Some(0.48),
        down_ask: Some(0.50),
        metadata: Some("{}".to_string()),
        market_id: Some("mkt-1".to_string()),
        execution_fidelity: Some("legacy_snapshot".to_string()),
    };
    let msg = WsMessage::Signal(signal);
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "signal");
    assert_eq!(parsed["data"]["direction"], "DOWN");
}

/// Verifies that broadcast channel delivers messages.
#[tokio::test]
async fn broadcast_channel_delivers_messages() {
    let (tx, mut rx1) = broadcast::channel::<WsMessage>(16);
    let mut rx2 = tx.subscribe();

    let trade = TradeRow {
        id: 1,
        timestamp: 1000,
        market_id: "mkt-1".to_string(),
        strategy: "latency-arb".to_string(),
        side: "UP".to_string(),
        entry_price: 0.45,
        size: 100.0,
        status: "open".to_string(),
        pnl: None,
        settlement_price: None,
        resolved_at: None,
        fill_status: None,
        execution_group_id: None,
        execution_fidelity: None,
        filled_size: None,
        avg_fill_price: None,
    };

    tx.send(WsMessage::Trade(trade)).unwrap();

    let msg1 = rx1.recv().await.unwrap();
    let msg2 = rx2.recv().await.unwrap();

    let json1 = serde_json::to_string(&msg1).unwrap();
    let json2 = serde_json::to_string(&msg2).unwrap();
    assert_eq!(json1, json2);
    assert!(json1.contains("latency-arb"));
}

/// Verifies that broadcast channel handles lagged receiver.
#[tokio::test]
async fn broadcast_channel_handles_lagged_receiver() {
    let (tx, _rx_unused) = broadcast::channel::<WsMessage>(2);
    let mut rx = tx.subscribe();

    for i in 0..3 {
        let entry = BalanceEntry {
            id: i,
            timestamp: 1000 + i as u64,
            event: "test".to_string(),
            trade_id: None,
            amount: 0.0,
            balance: 200.0,
        };
        let _ = tx.send(WsMessage::Balance(entry));
    }

    match rx.recv().await {
        Err(broadcast::error::RecvError::Lagged(_)) | Ok(_) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
}

use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;

use crate::db_reader::DbReader;

/// Poller fixture db.
fn poller_fixture_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tick_data (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, source TEXT NOT NULL, price REAL, bid REAL, ask REAL, bid_size REAL, ask_size REAL);
         CREATE TABLE markets (id INTEGER PRIMARY KEY AUTOINCREMENT, market_id TEXT NOT NULL UNIQUE, question TEXT NOT NULL, condition_id TEXT NOT NULL, slug TEXT NOT NULL, up_token_id TEXT NOT NULL, down_token_id TEXT NOT NULL, start_time INTEGER NOT NULL, end_time INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'active');
         CREATE TABLE signals (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, strategy TEXT NOT NULL, direction TEXT NOT NULL, binance_price REAL, chainlink_price REAL, up_ask REAL, down_ask REAL, up_bid REAL, down_bid REAL, metadata TEXT);
         CREATE TABLE simulated_trades (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, market_id TEXT NOT NULL, strategy TEXT NOT NULL, side TEXT NOT NULL, token_id TEXT NOT NULL, entry_price REAL NOT NULL, size REAL NOT NULL, status TEXT NOT NULL DEFAULT 'open');
         CREATE TABLE trade_results (id INTEGER PRIMARY KEY AUTOINCREMENT, trade_id INTEGER NOT NULL UNIQUE, exit_price REAL, settlement_price REAL NOT NULL, pnl_0pct REAL NOT NULL, pnl_1pct REAL NOT NULL, pnl_2pct REAL NOT NULL, pnl_3pct REAL NOT NULL, resolved_at INTEGER NOT NULL);
         CREATE TABLE balance_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, event TEXT NOT NULL, trade_id INTEGER, amount REAL NOT NULL, balance REAL NOT NULL);",
    ).unwrap();
    conn
}

/// Create a shared-memory DB so both the test code and the `DbReader` see the same rows.
/// `SQLite` in-memory DBs with `file::memory:?cache=shared` require a unique name per test.
fn shared_poller_db(name: &str) -> (Connection, Arc<DbReader>) {
    let uri = format!("file:{name}?mode=memory&cache=shared");
    let writer = Connection::open(&uri).unwrap();
    writer
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS tick_data (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, source TEXT NOT NULL, price REAL, bid REAL, ask REAL, bid_size REAL, ask_size REAL);
             CREATE TABLE IF NOT EXISTS markets (id INTEGER PRIMARY KEY AUTOINCREMENT, market_id TEXT NOT NULL UNIQUE, question TEXT NOT NULL, condition_id TEXT NOT NULL, slug TEXT NOT NULL, up_token_id TEXT NOT NULL, down_token_id TEXT NOT NULL, start_time INTEGER NOT NULL, end_time INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'active');
             CREATE TABLE IF NOT EXISTS signals (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, strategy TEXT NOT NULL, direction TEXT NOT NULL, binance_price REAL, chainlink_price REAL, up_ask REAL, down_ask REAL, up_bid REAL, down_bid REAL, metadata TEXT);
             CREATE TABLE IF NOT EXISTS simulated_trades (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, market_id TEXT NOT NULL, strategy TEXT NOT NULL, side TEXT NOT NULL, token_id TEXT NOT NULL, entry_price REAL NOT NULL, size REAL NOT NULL, status TEXT NOT NULL DEFAULT 'open');
             CREATE TABLE IF NOT EXISTS trade_results (id INTEGER PRIMARY KEY AUTOINCREMENT, trade_id INTEGER NOT NULL UNIQUE, exit_price REAL, settlement_price REAL NOT NULL, pnl_0pct REAL NOT NULL, pnl_1pct REAL NOT NULL, pnl_2pct REAL NOT NULL, pnl_3pct REAL NOT NULL, resolved_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS balance_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, event TEXT NOT NULL, trade_id INTEGER, amount REAL NOT NULL, balance REAL NOT NULL);",
        )
        .unwrap();

    let reader_conn = Connection::open(&uri).unwrap();
    let db = Arc::new(DbReader::from_connection(reader_conn));
    (writer, db)
}

/// Verifies that spawn poller detects new trade.
#[tokio::test]
async fn spawn_poller_detects_new_trade() {
    let (writer, db) = shared_poller_db("poller_trade");
    let (tx, mut rx) = broadcast::channel::<WsMessage>(16);

    super::spawn_poller(Arc::clone(&db), 50, tx);

    tokio::time::sleep(Duration::from_millis(30)).await;
    writer
        .execute(
            "INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status) VALUES (1100, 'mkt-1', 'latency-arb', 'UP', 'tok-up', 0.45, 100.0, 'open')",
            [],
        )
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timed out waiting for trade broadcast")
        .unwrap();
    assert!(matches!(msg, WsMessage::Trade(_)));
}

/// Verifies that spawn poller detects new balance.
#[tokio::test]
async fn spawn_poller_detects_new_balance() {
    let (writer, db) = shared_poller_db("poller_balance");
    let (tx, mut rx) = broadcast::channel::<WsMessage>(16);

    super::spawn_poller(Arc::clone(&db), 50, tx);

    tokio::time::sleep(Duration::from_millis(30)).await;
    writer
        .execute(
            "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (1000, 'init', NULL, 0.0, 200.0)",
            [],
        )
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timed out waiting for balance broadcast")
        .unwrap();
    assert!(matches!(msg, WsMessage::Balance(_)));
}

/// Verifies that spawn poller detects new signal.
#[tokio::test]
async fn spawn_poller_detects_new_signal() {
    let (writer, db) = shared_poller_db("poller_signal");
    let (tx, mut rx) = broadcast::channel::<WsMessage>(16);

    super::spawn_poller(Arc::clone(&db), 50, tx);

    tokio::time::sleep(Duration::from_millis(30)).await;
    writer
        .execute(
            "INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, metadata) VALUES (1050, 'latency-arb', 'UP', 42000.0, 42001.0, 0.45, 0.55, '{}')",
            [],
        )
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timed out waiting for signal broadcast")
        .unwrap();
    assert!(matches!(msg, WsMessage::Signal(_)));
}

/// Verifies that spawn poller handles empty db.
#[tokio::test]
async fn spawn_poller_handles_empty_db() {
    let conn = poller_fixture_db();
    let db = Arc::new(DbReader::from_connection(conn));
    let (tx, mut rx) = broadcast::channel::<WsMessage>(16);

    super::spawn_poller(db, 50, tx);

    let result = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await;
    assert!(
        result.is_err(),
        "expected timeout (no messages from empty DB)"
    );
}

use axum::Router;
use axum::extract::WebSocketUpgrade;
use axum::routing::get;

/// Build a minimal WS server and return (url, broadcast_tx).
async fn spawn_ws_server() -> (String, broadcast::Sender<WsMessage>) {
    let (tx, _) = broadcast::channel::<WsMessage>(64);
    let tx2 = tx.clone();

    let app = Router::new().route(
        "/ws",
        get(move |ws: WebSocketUpgrade| {
            let rx = tx2.subscribe();
            async move { ws.on_upgrade(move |socket| super::handle_ws(socket, rx)) }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    (format!("ws://127.0.0.1:{}/ws", addr.port()), tx)
}

/// Verifies that handle ws forwards broadcast to client.
#[tokio::test]
async fn handle_ws_forwards_broadcast_to_client() {
    let (url, tx) = spawn_ws_server().await;

    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let entry = BalanceEntry {
        id: 1,
        timestamp: 1000,
        event: "init".to_string(),
        trade_id: None,
        amount: 0.0,
        balance: 200.0,
    };
    tx.send(WsMessage::Balance(entry)).unwrap();

    use futures_util::StreamExt;
    let msg = tokio::time::timeout(Duration::from_millis(1000), ws.next())
        .await
        .expect("timed out waiting for WS message")
        .unwrap()
        .unwrap();

    match msg {
        tokio_tungstenite::tungstenite::Message::Text(text) => {
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(parsed["type"], "balance");
        }
        other => panic!("expected Text message, got: {other:?}"),
    }
}

/// Verifies that handle ws exits on client close.
#[tokio::test]
async fn handle_ws_exits_on_client_close() {
    let (url, _tx) = spawn_ws_server().await;

    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    ws.close(None).await.unwrap();
}
