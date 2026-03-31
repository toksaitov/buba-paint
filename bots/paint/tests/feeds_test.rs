mod support;

use std::time::Duration;

use buba_paint::config::Config;
use buba_paint::feeds::FeedMessage;
use tokio::sync::mpsc;
use tokio::time::timeout;

use support::mock_ws::MockWsServer;

/// Build a `Config` with short timeouts and the mock server's URL plugged in
/// to the appropriate field.
fn test_config() -> Config {
    Config {
        reconnect_base_delay: 100,
        reconnect_max_delay: 500,
        chainlink_stale_ms: 1_000,
        clob_ping_interval: 60_000,
        rtds_ping_interval: 60_000,
        ..Config::default()
    }
}

/// Verifies that binance feed emits connected on start.
#[tokio::test]
async fn binance_feed_emits_connected_on_start() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for FeedConnected")
        .expect("channel closed");

    match msg {
        FeedMessage::FeedConnected(name) => assert_eq!(name, "binance"),
        other => panic!("expected FeedConnected(\"binance\"), got {other:?}"),
    }
}

/// Verifies that binance feed receives agg trade.
#[tokio::test]
async fn binance_feed_receives_agg_trade() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    let _ = timeout(Duration::from_secs(5), rx.recv()).await;

    server
        .send(r#"{"e":"aggTrade","p":"42000.50","T":1700000000001}"#)
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for BinanceTick")
        .expect("channel closed");

    match msg {
        FeedMessage::BinanceTick {
            price, timestamp, ..
        } => {
            assert!((price - 42_000.50).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_001);
        }
        other => panic!("expected BinanceTick, got {other:?}"),
    }
}

/// Verifies that binance feed ignores non agg trade.
#[tokio::test]
async fn binance_feed_ignores_non_agg_trade() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    let _ = timeout(Duration::from_secs(5), rx.recv()).await;

    server
        .send(r#"{"e":"trade","p":"42000.50","T":1700000000001}"#)
        .await;

    let result = timeout(Duration::from_millis(500), rx.recv()).await;
    assert!(
        result.is_err(),
        "expected timeout (no message), but got a message"
    );
}

/// Verifies that binance feed reconnects after disconnect.
#[tokio::test]
async fn binance_feed_reconnects_after_disconnect() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert!(matches!(msg, FeedMessage::FeedConnected(_)));

    server.close().await;

    let reconnected = timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Some(FeedMessage::FeedConnected(name)) if name == "binance" => return true,
                Some(_) => {}
                None => return false,
            }
        }
    })
    .await
    .expect("timed out waiting for reconnection");

    assert!(reconnected, "feed did not reconnect after disconnect");
}

/// Verifies that clob feed sends subscription after resubscribe.
#[tokio::test]
async fn clob_feed_sends_subscription_after_resubscribe() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.clob_ws_url = server.url.clone();

    let (tx, mut _rx) = mpsc::channel::<FeedMessage>(64);
    let (clob_handle, _join) = buba_paint::feeds::clob_feed::run_clob_feed(&cfg, tx).await;

    clob_handle
        .resubscribe("tok-up".to_string(), "tok-down".to_string())
        .await
        .unwrap();

    let sub_msg = timeout(Duration::from_secs(5), server.recv())
        .await
        .expect("timed out waiting for subscription message")
        .expect("no subscription message received");

    let parsed: serde_json::Value = serde_json::from_str(&sub_msg).unwrap();
    assert_eq!(parsed["type"], "market");

    let assets = parsed["assets_ids"].as_array().unwrap();
    assert_eq!(assets.len(), 2);
    assert_eq!(assets[0], "tok-up");
    assert_eq!(assets[1], "tok-down");
}

/// Verifies that clob feed receives book snapshot.
#[tokio::test]
async fn clob_feed_receives_book_snapshot() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.clob_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    let (clob_handle, _join) = buba_paint::feeds::clob_feed::run_clob_feed(&cfg, tx).await;

    clob_handle
        .resubscribe("tok-up".to_string(), "tok-down".to_string())
        .await
        .unwrap();

    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    server
        .send(
            &serde_json::json!({
                "asset_id": "tok-up",
                "timestamp": 1_700_000_000_000_u64,
                "bids": [{"price": "0.45", "size": "200"}],
                "asks": [{"price": "0.55", "size": "150"}]
            })
            .to_string(),
        )
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for ClobBook")
        .expect("channel closed");

    match msg {
        FeedMessage::ClobBook { book_state, .. } => {
            let up = book_state.up.expect("up should be Some");
            assert!((up.best_bid - 0.45).abs() < f64::EPSILON);
            assert!((up.best_ask - 0.55).abs() < f64::EPSILON);
        }
        other => panic!("expected ClobBook, got {other:?}"),
    }
}

/// Verifies that clob feed receives price change.
#[tokio::test]
async fn clob_feed_receives_price_change() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.clob_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    let (clob_handle, _join) = buba_paint::feeds::clob_feed::run_clob_feed(&cfg, tx).await;

    clob_handle
        .resubscribe("tok-up".to_string(), "tok-down".to_string())
        .await
        .unwrap();

    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    server
        .send(
            &serde_json::json!({
                "event_type": "price_change",
                "timestamp": 1_700_000_000_000_u64,
                "price_changes": [
                    {"asset_id": "tok-up", "side": "BUY", "price": "0.46", "size": "100"},
                    {"asset_id": "tok-down", "side": "SELL", "price": "0.54", "size": "80"}
                ]
            })
            .to_string(),
        )
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for ClobPriceChange")
        .expect("channel closed");

    match msg {
        FeedMessage::ClobPriceChange { book_state, .. } => {
            let up = book_state.up.expect("up should be Some");
            assert!((up.best_bid - 0.46).abs() < f64::EPSILON);

            let down = book_state.down.expect("down should be Some");
            assert!((down.best_ask - 0.54).abs() < f64::EPSILON);
        }
        other => panic!("expected ClobPriceChange, got {other:?}"),
    }
}

/// Verifies that clob feed resubscribes on new tokens.
#[tokio::test]
async fn clob_feed_resubscribes_on_new_tokens() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.clob_ws_url = server.url.clone();

    let (tx, mut _rx) = mpsc::channel::<FeedMessage>(64);
    let (clob_handle, _join) = buba_paint::feeds::clob_feed::run_clob_feed(&cfg, tx).await;

    clob_handle
        .resubscribe("tok-up-1".to_string(), "tok-down-1".to_string())
        .await
        .unwrap();

    let sub1 = timeout(Duration::from_secs(5), server.recv())
        .await
        .expect("timed out")
        .expect("no message");
    let parsed1: serde_json::Value = serde_json::from_str(&sub1).unwrap();
    assert_eq!(parsed1["assets_ids"][0], "tok-up-1");

    clob_handle
        .resubscribe("tok-up-2".to_string(), "tok-down-2".to_string())
        .await
        .unwrap();

    let sub2 = timeout(Duration::from_secs(5), server.recv())
        .await
        .expect("timed out waiting for second subscription")
        .expect("no second subscription");
    let parsed2: serde_json::Value = serde_json::from_str(&sub2).unwrap();
    assert_eq!(parsed2["assets_ids"][0], "tok-up-2");
    assert_eq!(parsed2["assets_ids"][1], "tok-down-2");
}

/// Verifies that binance feed handles invalid text gracefully.
#[tokio::test]
async fn binance_feed_handles_invalid_text_gracefully() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for FeedConnected")
        .expect("channel closed");
    assert!(matches!(msg, FeedMessage::FeedConnected(_)));

    server.send("not valid json at all {{{{").await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    server
        .send(r#"{"e":"aggTrade","p":"42000.50","T":1700000000001}"#)
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for BinanceTick after invalid text")
        .expect("channel closed");

    match msg {
        FeedMessage::BinanceTick {
            price, timestamp, ..
        } => {
            assert!((price - 42_000.50).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_001);
        }
        other => panic!("expected BinanceTick, got {other:?}"),
    }
}

/// Verifies that chainlink feed sends subscription on connect.
#[tokio::test]
async fn chainlink_feed_sends_subscription_on_connect() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.rtds_ws_url = server.url.clone();

    let (tx, mut _rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::chainlink_feed::run_chainlink_feed(&cfg, tx).await;
    });

    let sub_msg = timeout(Duration::from_secs(5), server.recv())
        .await
        .expect("timed out waiting for subscription")
        .expect("no subscription received");

    let parsed: serde_json::Value = serde_json::from_str(&sub_msg).unwrap();
    assert_eq!(parsed["action"], "subscribe");

    let subs = parsed["subscriptions"].as_array().unwrap();
    assert!(!subs.is_empty());
    assert_eq!(subs[0]["topic"], "crypto_prices_chainlink");
}

/// Verifies that chainlink feed receives price update.
#[tokio::test]
async fn chainlink_feed_receives_price_update() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.rtds_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::chainlink_feed::run_chainlink_feed(&cfg, tx).await;
    });

    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    server
        .send(
            &serde_json::json!({
                "topic": "crypto_prices_chainlink",
                "payload": {
                    "value": 42000,
                    "timestamp": 1_700_000_000_000_u64
                }
            })
            .to_string(),
        )
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for ChainlinkPrice")
        .expect("channel closed");

    match msg {
        FeedMessage::ChainlinkPrice {
            price, timestamp, ..
        } => {
            assert!((price - 42_000.0).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_000);
        }
        other => panic!("expected ChainlinkPrice, got {other:?}"),
    }
}

/// Verifies that chainlink feed detects staleness.
#[tokio::test]
async fn chainlink_feed_detects_staleness() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.rtds_ws_url = server.url.clone();

    cfg.chainlink_stale_ms = 500;

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::chainlink_feed::run_chainlink_feed(&cfg, tx).await;
    });

    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    server
        .send(
            &serde_json::json!({
                "topic": "crypto_prices_chainlink",
                "payload": {"value": 42000, "timestamp": 100}
            })
            .to_string(),
        )
        .await;

    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    let stale_found = timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Some(FeedMessage::ChainlinkStale) => return true,
                Some(_) => {}
                None => return false,
            }
        }
    })
    .await
    .expect("timed out waiting for ChainlinkStale");

    assert!(stale_found, "expected ChainlinkStale event");
}

/// Verifies that chainlink feed reconnects after stale.
#[tokio::test]
async fn chainlink_feed_reconnects_after_stale() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.rtds_ws_url = server.url.clone();
    cfg.chainlink_stale_ms = 500;

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::chainlink_feed::run_chainlink_feed(&cfg, tx).await;
    });

    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    let reconnected = timeout(Duration::from_secs(10), async {
        let mut saw_stale = false;
        loop {
            match rx.recv().await {
                Some(FeedMessage::ChainlinkStale) => {
                    saw_stale = true;
                }
                Some(FeedMessage::FeedConnected(name)) if name == "chainlink" && saw_stale => {
                    return true;
                }
                Some(_) => {}
                None => return false,
            }
        }
    })
    .await
    .expect("timed out waiting for reconnect after stale");

    assert!(reconnected, "feed did not reconnect after staleness");
}

/// Verifies that chainlink feed handles invalid text gracefully.
#[tokio::test]
async fn chainlink_feed_handles_invalid_text_gracefully() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.rtds_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::chainlink_feed::run_chainlink_feed(&cfg, tx).await;
    });

    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for FeedConnected")
        .expect("channel closed");
    assert!(matches!(msg, FeedMessage::FeedConnected(_)));

    server.send("not valid json at all {{{{").await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    server
        .send(
            &serde_json::json!({
                "topic": "crypto_prices_chainlink",
                "payload": {
                    "value": 43000,
                    "timestamp": 1_700_000_000_100_u64
                }
            })
            .to_string(),
        )
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for ChainlinkPrice after invalid text")
        .expect("channel closed");

    match msg {
        FeedMessage::ChainlinkPrice {
            price, timestamp, ..
        } => {
            assert!((price - 43_000.0).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_100);
        }
        other => panic!("expected ChainlinkPrice, got {other:?}"),
    }
}

/// Verifies that clob feed handles invalid text gracefully.
#[tokio::test]
async fn clob_feed_handles_invalid_text_gracefully() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.clob_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    let (clob_handle, _join) = buba_paint::feeds::clob_feed::run_clob_feed(&cfg, tx).await;

    clob_handle
        .resubscribe("tok-up".to_string(), "tok-down".to_string())
        .await
        .unwrap();

    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    server.send("not valid json at all {{{{").await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    server
        .send(
            &serde_json::json!({
                "asset_id": "tok-up",
                "timestamp": 1_700_000_000_000_u64,
                "bids": [{"price": "0.45", "size": "200"}],
                "asks": [{"price": "0.55", "size": "150"}]
            })
            .to_string(),
        )
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for ClobBook after invalid text")
        .expect("channel closed");

    match msg {
        FeedMessage::ClobBook { book_state, .. } => {
            let up = book_state.up.expect("up should be Some");
            assert!((up.best_bid - 0.45).abs() < f64::EPSILON);
            assert!((up.best_ask - 0.55).abs() < f64::EPSILON);
        }
        other => panic!("expected ClobBook, got {other:?}"),
    }
}
