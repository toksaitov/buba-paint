use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use super::FeedMessage;
use crate::config::{Config, FeedEventStorageProfile};

use super::util::{backoff_delay, now_ms, now_us, should_reset_backoff};

/// Run the Chainlink (RTDS) price feed.
///
/// Connects to the Polymarket RTDS `WebSocket`, subscribes to BTC/USD Chainlink
/// prices, and sends `FeedMessage::ChainlinkPrice` updates through `tx`.
///
/// If no price update arrives within `config.chainlink_stale_ms`, sends a
/// `FeedMessage::ChainlinkStale` event and forces a reconnect.
///
/// This function runs forever (or until the channel is closed).
#[allow(clippy::too_many_lines)]
pub async fn run_chainlink_feed(
    config: &Config,
    tx: mpsc::Sender<FeedMessage>,
) -> anyhow::Result<()> {
    let url = &config.rtds_ws_url;
    let retain_payloads = config.feed_event_storage_profile == FeedEventStorageProfile::FullDebug;
    let ping_interval_ms = config.rtds_ping_interval;
    let stale_ms = config.chainlink_stale_ms;
    let base_delay = config.reconnect_base_delay;
    let max_delay = config.reconnect_max_delay;
    let min_stable_ms = config.reconnect_min_stable_ms;
    let max_failures = config.reconnect_max_failures;
    let feed_pause_ms = config.reconnect_pause_ms;
    let mut attempt: u32 = 0;

    loop {
        info!(feed = "chainlink", "connecting to {url}");

        match tokio_tungstenite::connect_async(url).await {
            Ok((ws_stream, _response)) => {
                let connected_at = now_ms();
                let connection_id = format!("chainlink-{}", now_us());
                if tx
                    .send(FeedMessage::FeedConnected {
                        name: "chainlink".to_string(),
                        connection_id: connection_id.clone(),
                    })
                    .await
                    .is_err()
                {
                    return Ok(());
                }

                let (mut write, mut read) = ws_stream.split();

                let sub_msg = serde_json::json!({
                    "action": "subscribe",
                    "subscriptions": [{
                        "topic": "crypto_prices_chainlink",
                        "type": "*",
                        "filters": "{\"symbol\":\"btc/usd\"}"
                    }]
                });
                if let Err(e) = write.send(Message::Text(sub_msg.to_string().into())).await {
                    error!(feed = "chainlink", "failed to send subscription: {e}");
                    let _ = tx
                        .send(FeedMessage::FeedDisconnected {
                            name: "chainlink".to_string(),
                            connection_id: Some(connection_id.clone()),
                        })
                        .await;
                    if attempt >= max_failures {
                        error!(
                            feed = "chainlink",
                            attempts = attempt,
                            pause_ms = feed_pause_ms,
                            "feed circuit breaker: pausing"
                        );
                        tokio::time::sleep(Duration::from_millis(feed_pause_ms)).await;
                        attempt = 0;
                    } else {
                        let delay = backoff_delay(attempt, base_delay, max_delay);
                        tokio::time::sleep(delay).await;
                        attempt = attempt.saturating_add(1);
                    }
                    continue;
                }

                let mut ping_timer = tokio::time::interval(Duration::from_millis(ping_interval_ms));
                let stale_duration = Duration::from_millis(stale_ms);
                let stale_sleep = tokio::time::sleep(stale_duration);
                tokio::pin!(stale_sleep);

                loop {
                    tokio::select! {
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    match process_chainlink_message(&text, &tx, &connection_id, retain_payloads).await {
                                        Ok(true) => {

                                            stale_sleep.as_mut().reset(
                                                tokio::time::Instant::now() + stale_duration
                                            );
                                        }
                                        Ok(false) => {

                                        }
                                        Err(e) => {
                                            warn!(feed = "chainlink", "failed to process message: {e}");
                                        }
                                    }
                                }
                                Some(Ok(Message::Ping(data))) => {
                                    if write.send(Message::Pong(data)).await.is_err() {
                                        warn!(feed = "chainlink", "failed to send pong");
                                        break;
                                    }
                                }
                                Some(Ok(Message::Close(_))) => {
                                    info!(feed = "chainlink", "server sent close frame");
                                    break;
                                }
                                Some(Err(e)) => {
                                    error!(feed = "chainlink", "websocket error: {e}");
                                    break;
                                }
                                None => {
                                    info!(feed = "chainlink", "stream ended");
                                    break;
                                }
                                Some(Ok(_)) => {}
                            }
                        }
                        _ = ping_timer.tick() => {
                            if write.send(Message::Ping(Vec::new().into())).await.is_err() {
                                warn!(feed = "chainlink", "failed to send ping");
                                break;
                            }
                        }
                        () = &mut stale_sleep => {
                            warn!(feed = "chainlink", "no update in {stale_ms}ms — stale");
                            let _ = tx
                                .send(FeedMessage::ChainlinkStale {
                                    connection_id: Some(connection_id.clone()),
                                })
                                .await;

                            break;
                        }
                    }
                }

                if should_reset_backoff(connected_at, now_ms(), min_stable_ms) {
                    attempt = 0;
                }
            }
            Err(e) => {
                error!(feed = "chainlink", "connection failed: {e}");
            }
        }

        let _ = tx
            .send(FeedMessage::FeedDisconnected {
                name: "chainlink".to_string(),
                connection_id: None,
            })
            .await;

        if attempt >= max_failures {
            error!(
                feed = "chainlink",
                attempts = attempt,
                pause_ms = feed_pause_ms,
                "feed circuit breaker: pausing"
            );
            tokio::time::sleep(Duration::from_millis(feed_pause_ms)).await;
            attempt = 0;
        } else {
            let delay = backoff_delay(attempt, base_delay, max_delay);
            warn!(
                feed = "chainlink",
                "reconnecting in {}ms (attempt {})",
                delay.as_millis(),
                attempt + 1
            );
            tokio::time::sleep(delay).await;
            attempt = attempt.saturating_add(1);
        }
    }
}

/// Parse a Chainlink/RTDS `JSON` message into a list of (price, timestamp) pairs.
///
/// Handles two formats:
/// - **Regular update**: `{topic: "crypto_prices_chainlink", payload: {value, timestamp}}`
/// - **Initial data dump**: `{payload: {data: [{value, timestamp}, ...]}}`
///
/// Returns an empty vec for unrecognised messages.
pub(crate) fn parse_chainlink_payload(msg: &serde_json::Value) -> Vec<(f64, u64)> {
    let mut results = Vec::new();

    if msg.get("topic").and_then(serde_json::Value::as_str) == Some("crypto_prices_chainlink") {
        if let Some(payload) = msg.get("payload") {
            let price = parse_f64_field(payload, "value");
            let timestamp = payload
                .get("timestamp")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);

            if let Some(p) = price {
                results.push((p, timestamp));
                return results;
            }
        }
    }

    if let Some(data) = msg
        .get("payload")
        .and_then(|p| p.get("data"))
        .and_then(serde_json::Value::as_array)
    {
        for entry in data {
            let price = parse_f64_field(entry, "value");
            let timestamp = entry
                .get("timestamp")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);

            if let Some(p) = price {
                results.push((p, timestamp));
            }
        }
    }

    results
}

/// Parse a raw `WebSocket` text frame into price updates.
///
/// Pure function -- no I/O, no channels.
/// If the top-level value is a `JSON` array, each element is processed.
/// Otherwise delegates to `parse_chainlink_payload`.
pub(crate) fn process_chainlink_text(text: &str) -> Vec<(f64, u64)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![];
    };

    if let Some(arr) = v.as_array() {
        let mut results = Vec::new();
        for item in arr {
            results.extend(parse_chainlink_payload(item));
        }
        results
    } else {
        parse_chainlink_payload(&v)
    }
}

/// Process a single Chainlink/RTDS message.
///
/// Returns `Ok(true)` if a price was extracted and sent (caller should reset
/// the staleness timer), `Ok(false)` for non-price messages, and `Err` on
/// parse failures.
async fn process_chainlink_message(
    text: &str,
    tx: &mpsc::Sender<FeedMessage>,
    connection_id: &str,
    retain_payloads: bool,
) -> anyhow::Result<bool> {
    let pairs = process_chainlink_text(text);

    if pairs.is_empty() {
        return Ok(false);
    }

    for (price, timestamp) in &pairs {
        let (timestamp_ms, source_time_us) = if *timestamp >= 10_000_000_000_000 {
            (*timestamp / 1_000, Some(*timestamp))
        } else {
            (*timestamp, None)
        };
        tx.send(FeedMessage::ChainlinkPrice {
            price: *price,
            timestamp_ms,
            timestamp_us: source_time_us,
            source_topic: Some("crypto_prices_chainlink".to_string()),
            source_symbol: Some("BTC/USD".to_string()),
            connection_id: connection_id.to_string(),
            payload_json: retain_payloads.then(|| text.to_string()),
            details_json: retain_payloads.then(|| {
                serde_json::json!({
                    "topic": "crypto_prices_chainlink",
                    "timestamp": timestamp_ms,
                })
                .to_string()
            }),
        })
        .await
        .map_err(|_| anyhow::anyhow!("channel closed"))?;
    }

    Ok(true)
}

/// Parse a `JSON` field that may be either a number or a string-encoded number.
fn parse_f64_field(v: &serde_json::Value, field: &str) -> Option<f64> {
    v.get(field).and_then(|val| {
        val.as_f64()
            .or_else(|| val.as_str().and_then(|s| s.parse::<f64>().ok()))
    })
}

#[cfg(test)]
#[path = "tests/chainlink_feed_tests.rs"]
mod tests;
