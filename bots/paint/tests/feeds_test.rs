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
        clob_ping_interval: 60_000, // large so pings don't interfere
        rtds_ping_interval: 60_000,
        ..Config::default()
    }
}

// =========================================================================
// Binance feed tests
// =========================================================================

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

#[tokio::test]
async fn binance_feed_receives_agg_trade() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    // Wait for the FeedConnected message first.
    let _ = timeout(Duration::from_secs(5), rx.recv()).await;

    // Send a valid aggTrade.
    server
        .send(r#"{"e":"aggTrade","p":"42000.50","T":1700000000001}"#)
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for BinanceTick")
        .expect("channel closed");

    match msg {
        FeedMessage::BinanceTick { price, timestamp } => {
            assert!((price - 42_000.50).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_001);
        }
        other => panic!("expected BinanceTick, got {other:?}"),
    }
}

#[tokio::test]
async fn binance_feed_ignores_non_agg_trade() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    // Wait for the FeedConnected.
    let _ = timeout(Duration::from_secs(5), rx.recv()).await;

    // Send a non-aggTrade event.
    server
        .send(r#"{"e":"trade","p":"42000.50","T":1700000000001}"#)
        .await;

    // No BinanceTick should arrive; only a short timeout proves absence.
    let result = timeout(Duration::from_millis(500), rx.recv()).await;
    assert!(
        result.is_err(),
        "expected timeout (no message), but got a message"
    );
}

#[tokio::test]
async fn binance_feed_reconnects_after_disconnect() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    // Wait for the initial FeedConnected.
    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert!(matches!(msg, FeedMessage::FeedConnected(_)));

    // Close the connection from the server side.
    server.close().await;

    // Drain FeedDisconnected.
    // Then wait for a second FeedConnected (reconnection).
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

// =========================================================================
// CLOB feed tests
// =========================================================================

#[tokio::test]
async fn clob_feed_sends_subscription_after_resubscribe() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.clob_ws_url = server.url.clone();

    let (tx, mut _rx) = mpsc::channel::<FeedMessage>(64);
    let (clob_handle, _join) = buba_paint::feeds::clob_feed::run_clob_feed(&cfg, tx).await;

    // Trigger resubscription — this causes the feed to connect and subscribe.
    clob_handle
        .resubscribe("tok-up".to_string(), "tok-down".to_string())
        .await
        .unwrap();

    // Capture the subscription message sent by the client.
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

    // Wait for subscription message to arrive at server.
    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    // Drain FeedConnected from the rx channel.
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    // Send a book snapshot for the UP token.
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
        FeedMessage::ClobBook { book_state } => {
            let up = book_state.up.expect("up should be Some");
            assert!((up.best_bid - 0.45).abs() < f64::EPSILON);
            assert!((up.best_ask - 0.55).abs() < f64::EPSILON);
        }
        other => panic!("expected ClobBook, got {other:?}"),
    }
}

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

    // Wait for subscription to reach server.
    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    // Drain FeedConnected.
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    // Send a price_change event.
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
        FeedMessage::ClobPriceChange { book_state } => {
            let up = book_state.up.expect("up should be Some");
            assert!((up.best_bid - 0.46).abs() < f64::EPSILON);

            let down = book_state.down.expect("down should be Some");
            assert!((down.best_ask - 0.54).abs() < f64::EPSILON);
        }
        other => panic!("expected ClobPriceChange, got {other:?}"),
    }
}

#[tokio::test]
async fn clob_feed_resubscribes_on_new_tokens() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.clob_ws_url = server.url.clone();

    let (tx, mut _rx) = mpsc::channel::<FeedMessage>(64);
    let (clob_handle, _join) = buba_paint::feeds::clob_feed::run_clob_feed(&cfg, tx).await;

    // First subscription.
    clob_handle
        .resubscribe("tok-up-1".to_string(), "tok-down-1".to_string())
        .await
        .unwrap();

    // Wait for first subscription message.
    let sub1 = timeout(Duration::from_secs(5), server.recv())
        .await
        .expect("timed out")
        .expect("no message");
    let parsed1: serde_json::Value = serde_json::from_str(&sub1).unwrap();
    assert_eq!(parsed1["assets_ids"][0], "tok-up-1");

    // Trigger second subscription with new tokens.
    clob_handle
        .resubscribe("tok-up-2".to_string(), "tok-down-2".to_string())
        .await
        .unwrap();

    // Wait for the new subscription message (after reconnect).
    let sub2 = timeout(Duration::from_secs(5), server.recv())
        .await
        .expect("timed out waiting for second subscription")
        .expect("no second subscription");
    let parsed2: serde_json::Value = serde_json::from_str(&sub2).unwrap();
    assert_eq!(parsed2["assets_ids"][0], "tok-up-2");
    assert_eq!(parsed2["assets_ids"][1], "tok-down-2");
}

// =========================================================================
// Binance feed — invalid text handling
// =========================================================================

#[tokio::test]
async fn binance_feed_handles_invalid_text_gracefully() {
    let server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.binance_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::binance_feed::run_binance_feed(&cfg, tx).await;
    });

    // Wait for FeedConnected.
    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for FeedConnected")
        .expect("channel closed");
    assert!(matches!(msg, FeedMessage::FeedConnected(_)));

    // Send invalid text — the feed should warn but not crash.
    server.send("not valid json at all {{{{").await;

    // Small delay to let the feed process the invalid message.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now send a valid aggTrade — the feed should still be alive.
    server
        .send(r#"{"e":"aggTrade","p":"42000.50","T":1700000000001}"#)
        .await;

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for BinanceTick after invalid text")
        .expect("channel closed");

    match msg {
        FeedMessage::BinanceTick { price, timestamp } => {
            assert!((price - 42_000.50).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_001);
        }
        other => panic!("expected BinanceTick, got {other:?}"),
    }
}

// =========================================================================
// Chainlink feed tests
// =========================================================================

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

#[tokio::test]
async fn chainlink_feed_receives_price_update() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.rtds_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::chainlink_feed::run_chainlink_feed(&cfg, tx).await;
    });

    // Wait for subscription to arrive at server.
    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    // Drain FeedConnected.
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    // Send a Chainlink price update.
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
        FeedMessage::ChainlinkPrice { price, timestamp } => {
            assert!((price - 42_000.0).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_000);
        }
        other => panic!("expected ChainlinkPrice, got {other:?}"),
    }
}

#[tokio::test]
async fn chainlink_feed_detects_staleness() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.rtds_ws_url = server.url.clone();
    // Use a very short stale timeout for the test.
    cfg.chainlink_stale_ms = 500;

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::chainlink_feed::run_chainlink_feed(&cfg, tx).await;
    });

    // Wait for subscription.
    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    // Drain FeedConnected.
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    // Send one price so the feed is "alive", then go silent.
    server
        .send(
            &serde_json::json!({
                "topic": "crypto_prices_chainlink",
                "payload": {"value": 42000, "timestamp": 100}
            })
            .to_string(),
        )
        .await;

    // Drain the ChainlinkPrice message.
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    // Now wait for the stale event (should arrive after ~500ms).
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

    // Wait for first subscription.
    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    // Drain FeedConnected.
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    // Let the feed go stale (no messages sent).
    // Wait for ChainlinkStale, FeedDisconnected, then a new FeedConnected.
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

#[tokio::test]
async fn chainlink_feed_handles_invalid_text_gracefully() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.rtds_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    tokio::spawn(async move {
        let _ = buba_paint::feeds::chainlink_feed::run_chainlink_feed(&cfg, tx).await;
    });

    // Wait for subscription message to arrive at server.
    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    // Drain FeedConnected.
    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for FeedConnected")
        .expect("channel closed");
    assert!(matches!(msg, FeedMessage::FeedConnected(_)));

    // Send invalid text — the feed should warn but not crash.
    server.send("not valid json at all {{{{").await;

    // Small delay to let the feed process the invalid message.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now send a valid Chainlink price update — the feed should still be alive.
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
        FeedMessage::ChainlinkPrice { price, timestamp } => {
            assert!((price - 43_000.0).abs() < f64::EPSILON);
            assert_eq!(timestamp, 1_700_000_000_100);
        }
        other => panic!("expected ChainlinkPrice, got {other:?}"),
    }
}

#[tokio::test]
async fn clob_feed_handles_invalid_text_gracefully() {
    let mut server = MockWsServer::start().await;
    let mut cfg = test_config();
    cfg.clob_ws_url = server.url.clone();

    let (tx, mut rx) = mpsc::channel::<FeedMessage>(64);
    let (clob_handle, _join) = buba_paint::feeds::clob_feed::run_clob_feed(&cfg, tx).await;

    // Trigger resubscription so the feed connects and subscribes.
    clob_handle
        .resubscribe("tok-up".to_string(), "tok-down".to_string())
        .await
        .unwrap();

    // Wait for subscription message to arrive at server.
    let _ = timeout(Duration::from_secs(5), server.recv()).await;

    // Drain FeedConnected.
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    // Send invalid text — the feed should warn but not crash.
    server.send("not valid json at all {{{{").await;

    // Small delay to let the feed process the invalid message.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now send a valid book snapshot — the feed should still be alive.
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
        FeedMessage::ClobBook { book_state } => {
            let up = book_state.up.expect("up should be Some");
            assert!((up.best_bid - 0.45).abs() < f64::EPSILON);
            assert!((up.best_ask - 0.55).abs() < f64::EPSILON);
        }
        other => panic!("expected ClobBook, got {other:?}"),
    }
}
