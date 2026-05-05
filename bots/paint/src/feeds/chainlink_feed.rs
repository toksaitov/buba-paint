use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use super::FeedMessage;
use crate::config::{Config, FeedEventStorageProfile};

use super::util::{
    FeedDisconnectCause, FeedDisconnectReport, backoff_delay, now_ms, now_us, should_reset_backoff,
};

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
    let connect_timeout_ms = config.websocket_connect_timeout_ms;
    let mut attempt: u32 = 0;

    loop {
        info!(feed = "chainlink", "connecting to {url}");

        let disconnect = match tokio::time::timeout(
            Duration::from_millis(connect_timeout_ms),
            tokio_tungstenite::connect_async(url),
        )
        .await
        {
            Ok(Ok((ws_stream, _response))) => {
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
                    FeedDisconnectReport {
                        connection_id: Some(connection_id),
                        cause: FeedDisconnectCause::ConnectionFailed,
                        connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                        after_resubscribe: false,
                        error: Some(format!("failed to send subscription: {e}")),
                        timeout_ms: None,
                    }
                } else {
                    let mut ping_timer =
                        tokio::time::interval(Duration::from_millis(ping_interval_ms));
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
                                            Ok(false) => {}
                                            Err(e) => {
                                                warn!(feed = "chainlink", "failed to process message: {e}");
                                            }
                                        }
                                    }
                                    Some(Ok(Message::Ping(data))) => {
                                        if write.send(Message::Pong(data)).await.is_err() {
                                            warn!(feed = "chainlink", "failed to send pong");
                                            break FeedDisconnectReport {
                                                connection_id: Some(connection_id.clone()),
                                                cause: FeedDisconnectCause::PingFailure,
                                                connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                                after_resubscribe: false,
                                                error: Some("failed to send pong".to_string()),
                                                timeout_ms: None,
                                            };
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) => {
                                        info!(feed = "chainlink", "server sent close frame");
                                        break FeedDisconnectReport {
                                            connection_id: Some(connection_id.clone()),
                                            cause: FeedDisconnectCause::ConnectionFailed,
                                            connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                            after_resubscribe: false,
                                            error: Some("server sent close frame".to_string()),
                                            timeout_ms: None,
                                        };
                                    }
                                    Some(Err(e)) => {
                                        error!(feed = "chainlink", "websocket error: {e}");
                                        break FeedDisconnectReport {
                                            connection_id: Some(connection_id.clone()),
                                            cause: FeedDisconnectCause::WebsocketError,
                                            connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                            after_resubscribe: false,
                                            error: Some(e.to_string()),
                                            timeout_ms: None,
                                        };
                                    }
                                    None => {
                                        info!(feed = "chainlink", "stream ended");
                                        break FeedDisconnectReport {
                                            connection_id: Some(connection_id.clone()),
                                            cause: FeedDisconnectCause::ConnectionFailed,
                                            connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                            after_resubscribe: false,
                                            error: Some("stream ended".to_string()),
                                            timeout_ms: None,
                                        };
                                    }
                                    Some(Ok(_)) => {}
                                }
                            }
                            _ = ping_timer.tick() => {
                                if write.send(Message::Ping(Vec::new().into())).await.is_err() {
                                    warn!(feed = "chainlink", "failed to send ping");
                                    break FeedDisconnectReport {
                                        connection_id: Some(connection_id.clone()),
                                        cause: FeedDisconnectCause::PingFailure,
                                        connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                        after_resubscribe: false,
                                        error: Some("failed to send ping".to_string()),
                                        timeout_ms: None,
                                    };
                                }
                            }
                            () = &mut stale_sleep => {
                                warn!(feed = "chainlink", "no update in {stale_ms}ms — stale");
                                let stale_report = FeedDisconnectReport {
                                    connection_id: Some(connection_id.clone()),
                                    cause: FeedDisconnectCause::StaleTimeout,
                                    connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                    after_resubscribe: false,
                                    error: None,
                                    timeout_ms: Some(stale_ms),
                                };
                                let _ = tx
                                    .send(FeedMessage::ChainlinkStale {
                                        connection_id: Some(connection_id.clone()),
                                        details_json: stale_report.details_json(attempt.saturating_add(1), None),
                                    })
                                    .await;

                                break stale_report;
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                error!(feed = "chainlink", "connection failed: {e}");
                FeedDisconnectReport {
                    connection_id: None,
                    cause: FeedDisconnectCause::ConnectionFailed,
                    connection_lifetime_ms: None,
                    after_resubscribe: false,
                    error: Some(e.to_string()),
                    timeout_ms: None,
                }
            }
            Err(_) => {
                error!(
                    feed = "chainlink",
                    timeout_ms = connect_timeout_ms,
                    "websocket connect timed out"
                );
                FeedDisconnectReport {
                    connection_id: None,
                    cause: FeedDisconnectCause::ConnectTimeout,
                    connection_lifetime_ms: None,
                    after_resubscribe: false,
                    error: Some(format!(
                        "websocket connect timed out after {connect_timeout_ms}ms"
                    )),
                    timeout_ms: Some(connect_timeout_ms),
                }
            }
        };

        if let Some(connection_lifetime_ms) = disconnect.connection_lifetime_ms
            && should_reset_backoff(0, connection_lifetime_ms, min_stable_ms)
        {
            attempt = 0;
        }

        let reconnect_delay = if attempt >= max_failures {
            error!(
                feed = "chainlink",
                attempts = attempt,
                pause_ms = feed_pause_ms,
                "feed circuit breaker: pausing"
            );
            Duration::from_millis(feed_pause_ms)
        } else {
            let delay = backoff_delay(attempt, base_delay, max_delay);
            warn!(
                feed = "chainlink",
                "reconnecting in {}ms (attempt {})",
                delay.as_millis(),
                attempt + 1
            );
            delay
        };

        let details_json = disconnect.details_json(
            attempt.saturating_add(1),
            Some(reconnect_delay.as_millis() as u64),
        );
        let _ = tx
            .send(FeedMessage::FeedDisconnected {
                name: "chainlink".to_string(),
                connection_id: disconnect.connection_id,
                cause_class: disconnect.cause.as_str(),
                details_json,
            })
            .await;

        if attempt >= max_failures {
            tokio::time::sleep(reconnect_delay).await;
            attempt = 0;
        } else {
            tokio::time::sleep(reconnect_delay).await;
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

    if msg.get("topic").and_then(serde_json::Value::as_str) == Some("crypto_prices_chainlink")
        && let Some(payload) = msg.get("payload")
    {
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
