use super::{
    ProbeResult, candidate_probe_slugs, clob_subscription_message, discover_probe_window,
    extract_source_timestamp_us, normalize_epoch_to_us, probe_gamma, probe_websocket,
    read_first_probe_message, rtds_subscription_message, run_latency_probe,
    summarize_probe_message,
};
use crate::config::Config;
use crate::market_discovery::parse_gamma_event_response;
use crate::types::MarketWindow;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build a valid Gamma response for one BTC market probe slug.
fn gamma_probe_body(slug: &str) -> serde_json::Value {
    serde_json::json!({
        "slug": slug,
        "markets": [{
            "id": format!("market-{slug}"),
            "question": "Will BTC close up?",
            "conditionId": format!("condition-{slug}"),
            "outcomes": ["Up", "Down"],
            "clobTokenIds": [format!("up-{slug}"), format!("down-{slug}")],
            "endDate": "2026-04-01T12:05:00Z"
        }]
    })
}

/// Spawn a single-use websocket probe server and capture the first subscription text.
async fn spawn_probe_ws_server(
    expected_subscription: Option<String>,
    outbound_messages: Vec<Message>,
) -> (String, Arc<Mutex<Vec<String>>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let received_texts = Arc::new(Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received_texts);
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        if expected_subscription.is_some()
            && let Some(Ok(Message::Text(text))) = socket.next().await
        {
            received_clone.lock().await.push(text.to_string());
        }
        for message in outbound_messages {
            socket.send(message).await.unwrap();
        }
    });
    (format!("ws://{addr}"), received_texts, handle)
}

/// Connect a real websocket client to the local test server.
async fn connect_probe_socket(
    url: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    connect_async(url).await.unwrap().0
}

/// Verifies that candidate probe slugs include current and next slots.
#[test]
fn candidate_probe_slugs_include_two_btc_windows() {
    let slugs = candidate_probe_slugs();

    assert_eq!(slugs.len(), 2);
    assert!(slugs.iter().all(|slug| slug.starts_with("btc-updown-5m-")));
    assert_ne!(slugs[0], slugs[1]);
}

/// Verifies that epoch normalization preserves microseconds.
#[test]
fn normalize_epoch_to_us_preserves_microsecond_values() {
    let value = 1_775_000_000_123_456_u64;

    assert_eq!(normalize_epoch_to_us(value), value);
}

/// Verifies that epoch normalization scales milliseconds to microseconds.
#[test]
fn normalize_epoch_to_us_scales_milliseconds() {
    assert_eq!(
        normalize_epoch_to_us(1_775_000_000_123),
        1_775_000_000_123_000
    );
}

/// Verifies that websocket timestamp extraction handles nested Binance envelopes.
#[test]
fn extract_source_timestamp_us_reads_nested_binance_event_time() {
    let value = serde_json::json!({
        "stream": "btcusdt@aggTrade",
        "data": {
            "E": 1_775_000_000_123_u64
        }
    });

    assert_eq!(
        extract_source_timestamp_us(&value),
        Some(1_775_000_000_123_000)
    );
}

/// Verifies that websocket timestamp extraction handles RTDS payload arrays.
#[test]
fn extract_source_timestamp_us_reads_rtds_payload_time() {
    let value = serde_json::json!({
        "payload": {
            "data": [
                {
                    "timestamp": 1_775_000_000_123_u64
                }
            ]
        }
    });

    assert_eq!(
        extract_source_timestamp_us(&value),
        Some(1_775_000_000_123_000)
    );
}

/// Verifies that RTDS probe subscriptions match the live feed payload.
#[test]
fn rtds_subscription_message_matches_live_feed_shape() {
    let payload = serde_json::from_str::<serde_json::Value>(&rtds_subscription_message()).unwrap();

    assert_eq!(payload["action"], "subscribe");
    assert_eq!(
        payload["subscriptions"][0]["topic"],
        "crypto_prices_chainlink"
    );
}

/// Verifies that `CLOB` probe subscriptions request custom market features.
#[test]
fn clob_subscription_message_requests_custom_features() {
    let window = MarketWindow {
        market_id: "market-1".to_string(),
        question: "Will BTC close up?".to_string(),
        up_token_id: "up-token".to_string(),
        down_token_id: "down-token".to_string(),
        condition_id: "condition-1".to_string(),
        start_time: 100,
        end_time: 200,
        slug: "btc-updown-5m-100".to_string(),
        outcome: None,
        resolution_source: None,
        fee_profile: Some("crypto".to_string()),
        order_min_size: None,
        order_price_min_tick_size: None,
        maker_base_fee: None,
        taker_base_fee: None,
        rewards_min_size: None,
        rewards_max_spread: None,
    };
    let payload =
        serde_json::from_str::<serde_json::Value>(&clob_subscription_message(&window)).unwrap();

    assert_eq!(payload["type"], "market");
    assert_eq!(payload["assets_ids"][0], "up-token");
    assert_eq!(payload["assets_ids"][1], "down-token");
    assert_eq!(payload["custom_feature_enabled"], true);
}

/// Verifies that message summaries prefer stream names when present.
#[test]
fn summarize_probe_message_prefers_stream_metadata() {
    let value = serde_json::json!({
        "stream": "btcusdt@aggTrade"
    });

    assert_eq!(
        summarize_probe_message(&value, 123),
        Some("stream=btcusdt@aggTrade bytes=123".to_string())
    );
}

/// Verifies that probe result equality remains stable for reporting tests.
#[test]
fn probe_result_equality_is_structural() {
    let left = ProbeResult {
        name: "binance",
        url: "wss://example.com".to_string(),
        connect_ms: 1.0,
        first_message_ms: Some(2.0),
        message_age_ms: Some(3.0),
        details: Some("stream=test".to_string()),
    };
    let right = left.clone();

    assert_eq!(left, right);
}

/// Verifies that websocket timestamp extraction handles direct string timestamps.
#[test]
fn extract_source_timestamp_us_reads_direct_string_timestamp() {
    let value = serde_json::json!({
        "timestamp": "1775000000123"
    });

    assert_eq!(
        extract_source_timestamp_us(&value),
        Some(1_775_000_000_123_000)
    );
}

/// Verifies that websocket timestamp extraction returns none when timestamps are absent.
#[test]
fn extract_source_timestamp_us_returns_none_without_timestamp() {
    let value = serde_json::json!({
        "topic": "no-time-here"
    });

    assert_eq!(extract_source_timestamp_us(&value), None);
}

/// Verifies that message summaries fall back to topics when streams are absent.
#[test]
fn summarize_probe_message_uses_topic_metadata() {
    let value = serde_json::json!({
        "topic": "crypto_prices_chainlink"
    });

    assert_eq!(
        summarize_probe_message(&value, 88),
        Some("topic=crypto_prices_chainlink bytes=88".to_string())
    );
}

/// Verifies that message summaries use event types when stream and topic are absent.
#[test]
fn summarize_probe_message_uses_event_type_metadata() {
    let value = serde_json::json!({
        "event_type": "best_bid_ask"
    });

    assert_eq!(
        summarize_probe_message(&value, 64),
        Some("event_type=best_bid_ask bytes=64".to_string())
    );
}

/// Verifies that message summaries fall back to raw byte counts.
#[test]
fn summarize_probe_message_falls_back_to_raw_length() {
    let value = serde_json::json!({
        "message": "opaque"
    });

    assert_eq!(
        summarize_probe_message(&value, 33),
        Some("bytes=33".to_string())
    );
}

/// Verifies that probe window discovery skips missing slugs and returns the first valid market.
#[tokio::test]
async fn discover_probe_window_returns_first_valid_market() {
    let server = MockServer::start().await;
    let slugs = candidate_probe_slugs();

    Mock::given(method("GET"))
        .and(path(format!("/events/slug/{}", slugs[0])))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/events/slug/{}", slugs[1])))
        .respond_with(ResponseTemplate::new(200).set_body_json(gamma_probe_body(&slugs[1])))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let window = discover_probe_window(&client, &server.uri(), Duration::from_secs(1))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(window.slug, slugs[1]);
    assert_eq!(window.up_token_id, format!("up-{}", slugs[1]));
}

/// Verifies that probe window discovery returns none when no candidate response is usable.
#[tokio::test]
async fn discover_probe_window_returns_none_for_invalid_bodies() {
    let server = MockServer::start().await;
    let slugs = candidate_probe_slugs();
    for slug in &slugs {
        Mock::given(method("GET"))
            .and(path(format!("/events/slug/{slug}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;
    }

    let client = reqwest::Client::new();
    let window = discover_probe_window(&client, &server.uri(), Duration::from_secs(1))
        .await
        .unwrap();

    assert!(window.is_none());
}

/// Verifies that the Gamma probe records the request path and response status.
#[tokio::test]
async fn probe_gamma_records_status_and_slug() {
    let server = MockServer::start().await;
    let slug = candidate_probe_slugs().into_iter().next().unwrap();
    Mock::given(method("GET"))
        .and(path(format!("/events/slug/{slug}")))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let result = probe_gamma(&client, &server.uri(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(result.name, "gamma");
    assert!(result.url.ends_with(&format!("/events/slug/{slug}")));
    assert!(result.connect_ms >= 0.0);
    assert!(result.first_message_ms.is_none());
    assert!(result.message_age_ms.is_none());
    assert!(
        result
            .details
            .as_deref()
            .is_some_and(|details| details.contains("status=200 OK") && details.contains(&slug))
    );
}

/// Verifies that websocket probes send subscriptions and measure the first text frame.
#[tokio::test]
async fn probe_websocket_sends_subscription_and_reads_first_message() {
    let subscription = rtds_subscription_message();
    let event_ts_ms = crate::feeds::util::now_us() / 1_000;
    let payload = serde_json::json!({
        "topic": "crypto_prices_chainlink",
        "payload": {
            "data": [{
                "timestamp": event_ts_ms
            }]
        }
    });
    let (url, received_texts, handle) = spawn_probe_ws_server(
        Some(subscription.clone()),
        vec![Message::Text(payload.to_string().into())],
    )
    .await;

    let result = probe_websocket("rtds", &url, Some(subscription), Duration::from_secs(1))
        .await
        .unwrap();

    handle.await.unwrap();
    let seen = received_texts.lock().await.clone();
    assert_eq!(seen, vec![rtds_subscription_message()]);
    assert_eq!(result.name, "rtds");
    assert!(result.first_message_ms.is_some());
    assert!(result.message_age_ms.is_some());
    assert!(
        result
            .details
            .as_deref()
            .is_some_and(|details| details.contains("topic=crypto_prices_chainlink"))
    );
}

/// Verifies that websocket probes surface close reasons when no text frame arrives.
#[tokio::test]
async fn probe_websocket_returns_close_reason_without_text() {
    let (url, _received_texts, handle) = spawn_probe_ws_server(
        None,
        vec![Message::Close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "bye".into(),
        }))],
    )
    .await;

    let result = probe_websocket("binance", &url, None, Duration::from_secs(1))
        .await
        .unwrap();

    handle.await.unwrap();
    assert!(result.first_message_ms.is_none());
    assert!(result.message_age_ms.is_none());
    assert_eq!(result.details.as_deref(), Some("bye"));
}

/// Verifies that first-message reads skip pings and parse the next text payload.
#[tokio::test]
async fn read_first_probe_message_skips_ping_frames() {
    let payload = serde_json::json!({
        "event_type": "best_bid_ask",
        "timestamp": "1775000000123"
    });
    let (url, _received_texts, handle) = spawn_probe_ws_server(
        None,
        vec![
            Message::Ping(vec![1, 2, 3].into()),
            Message::Text(payload.to_string().into()),
        ],
    )
    .await;
    let mut socket = connect_probe_socket(&url).await;
    let started = Instant::now();

    let (first_message_ms, message_age_ms, details) =
        read_first_probe_message("clob", &mut socket, Duration::from_secs(1), started)
            .await
            .unwrap();

    handle.await.unwrap();
    assert!(first_message_ms.is_some());
    assert!(message_age_ms.is_some());
    assert!(
        details
            .as_deref()
            .is_some_and(|details| details.starts_with("event_type=best_bid_ask bytes="))
    );
}

/// Verifies that the full latency probe runs against local HTTP and websocket endpoints.
#[tokio::test]
async fn run_latency_probe_completes_against_local_endpoints() {
    let server = MockServer::start().await;
    let slug = candidate_probe_slugs().into_iter().next().unwrap();
    Mock::given(method("GET"))
        .and(path(format!("/events/slug/{slug}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(gamma_probe_body(&slug)))
        .mount(&server)
        .await;

    let binance_payload = serde_json::json!({
        "stream": "btcusdt@aggTrade",
        "data": { "E": crate::feeds::util::now_us() / 1_000 }
    });
    let rtds_payload = serde_json::json!({
        "topic": "crypto_prices_chainlink",
        "payload": { "data": [{ "timestamp": crate::feeds::util::now_us() / 1_000 }] }
    });
    let clob_payload = serde_json::json!({
        "event_type": "best_bid_ask",
        "timestamp": crate::feeds::util::now_us() / 1_000
    });

    let (binance_url, _binance_received, binance_handle) = spawn_probe_ws_server(
        None,
        vec![Message::Text(binance_payload.to_string().into())],
    )
    .await;
    let (rtds_url, _rtds_received, rtds_handle) = spawn_probe_ws_server(
        Some(rtds_subscription_message()),
        vec![Message::Text(rtds_payload.to_string().into())],
    )
    .await;
    let window = parse_gamma_event_response(&gamma_probe_body(&slug)).unwrap();
    let (clob_url, _clob_received, clob_handle) = spawn_probe_ws_server(
        Some(clob_subscription_message(&window)),
        vec![Message::Text(clob_payload.to_string().into())],
    )
    .await;

    let mut config = Config::default();
    config.gamma_api_url = server.uri();
    config.binance_ws_url = binance_url;
    config.rtds_ws_url = rtds_url;
    config.clob_ws_url = clob_url;

    run_latency_probe(&config, 1_000).await.unwrap();

    binance_handle.await.unwrap();
    rtds_handle.await.unwrap();
    clob_handle.await.unwrap();
}
