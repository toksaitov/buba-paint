use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use super::FeedMessage;
use crate::config::{Config, FeedEventStorageProfile};
use crate::types::OrderLevel;

use super::util::{
    FeedDisconnectCause, FeedDisconnectReport, backoff_delay, now_ms, now_us, should_reset_backoff,
};

/// Run the shared Binance market-data feed.
///
/// Connects to the configured Binance `WebSocket` endpoint, parses trade,
/// book-ticker, and shallow-depth messages, and forwards them through `tx`.
/// On disconnect the feed waits with exponential backoff and reconnects
/// automatically.
#[allow(clippy::too_many_lines)]
pub async fn run_binance_feed(
    config: &Config,
    tx: mpsc::Sender<FeedMessage>,
) -> anyhow::Result<()> {
    let url = &config.binance_ws_url;
    let retain_payloads = config.feed_event_storage_profile == FeedEventStorageProfile::FullDebug;
    let base_delay = config.reconnect_base_delay;
    let max_delay = config.reconnect_max_delay;
    let min_stable_ms = config.reconnect_min_stable_ms;
    let max_failures = config.reconnect_max_failures;
    let feed_pause_ms = config.reconnect_pause_ms;
    let connect_timeout_ms = config.websocket_connect_timeout_ms;
    let no_message_reconnect_ms = config.binance_no_message_reconnect_ms;
    let mut attempt: u32 = 0;

    loop {
        info!(feed = "binance", "connecting to {url}");

        let disconnect = match tokio::time::timeout(
            Duration::from_millis(connect_timeout_ms),
            tokio_tungstenite::connect_async(url),
        )
        .await
        {
            Ok(Ok((ws_stream, _response))) => {
                let connected_at = now_ms();
                let connection_id = format!("binance-{}", now_us());
                if tx
                    .send(FeedMessage::FeedConnected {
                        name: "binance".to_string(),
                        connection_id: connection_id.clone(),
                    })
                    .await
                    .is_err()
                {
                    return Ok(());
                }

                let (mut write, mut read) = ws_stream.split();
                let idle_duration = Duration::from_millis(no_message_reconnect_ms);
                let idle_sleep = tokio::time::sleep(idle_duration);
                tokio::pin!(idle_sleep);

                loop {
                    tokio::select! {
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    idle_sleep.as_mut().reset(tokio::time::Instant::now() + idle_duration);
                                    if let Err(error) =
                                        process_binance_message(&text, &tx, &connection_id, retain_payloads)
                                            .await
                                    {
                                        warn!(feed = "binance", "failed to process message: {error}");
                                    }
                                }
                                Some(Ok(Message::Ping(data))) => {
                                    if write.send(Message::Pong(data)).await.is_err() {
                                        warn!(feed = "binance", "failed to send pong");
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
                                    info!(feed = "binance", "server sent close frame");
                                    break FeedDisconnectReport {
                                        connection_id: Some(connection_id.clone()),
                                        cause: FeedDisconnectCause::ConnectionFailed,
                                        connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                        after_resubscribe: false,
                                        error: Some("server sent close frame".to_string()),
                                        timeout_ms: None,
                                    };
                                }
                                Some(Err(error)) => {
                                    error!(feed = "binance", "websocket error: {error}");
                                    break FeedDisconnectReport {
                                        connection_id: Some(connection_id.clone()),
                                        cause: FeedDisconnectCause::WebsocketError,
                                        connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                        after_resubscribe: false,
                                        error: Some(error.to_string()),
                                        timeout_ms: None,
                                    };
                                }
                                None => {
                                    info!(feed = "binance", "stream ended");
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
                        () = &mut idle_sleep => {
                            warn!(
                                feed = "binance",
                                timeout_ms = no_message_reconnect_ms,
                                "no market data in {no_message_reconnect_ms}ms; forcing reconnect"
                            );
                            break FeedDisconnectReport {
                                connection_id: Some(connection_id.clone()),
                                cause: FeedDisconnectCause::IdleTimeout,
                                connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                after_resubscribe: false,
                                error: None,
                                timeout_ms: Some(no_message_reconnect_ms),
                            };
                        }
                    }
                }
            }
            Ok(Err(error)) => {
                error!(feed = "binance", "connection failed: {error}");
                FeedDisconnectReport {
                    connection_id: None,
                    cause: FeedDisconnectCause::ConnectionFailed,
                    connection_lifetime_ms: None,
                    after_resubscribe: false,
                    error: Some(error.to_string()),
                    timeout_ms: None,
                }
            }
            Err(_) => {
                error!(
                    feed = "binance",
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

        if let Some(connection_lifetime_ms) = disconnect.connection_lifetime_ms {
            if should_reset_backoff(0, connection_lifetime_ms, min_stable_ms) {
                attempt = 0;
            }
        }

        let reconnect_delay = if attempt >= max_failures {
            error!(
                feed = "binance",
                attempts = attempt,
                pause_ms = feed_pause_ms,
                "feed circuit breaker: pausing"
            );
            Duration::from_millis(feed_pause_ms)
        } else {
            let delay = backoff_delay(attempt, base_delay, max_delay);
            warn!(
                feed = "binance",
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
                name: "binance".to_string(),
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

#[derive(Debug, Clone)]
struct BinanceEnvelope {
    topic: Option<String>,
    payload: serde_json::Value,
}

/// Parse a raw Binance text frame into a combined-stream envelope.
fn parse_envelope(text: &str) -> Option<BinanceEnvelope> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if let Some(payload) = value.get("data") {
        return Some(BinanceEnvelope {
            topic: value
                .get("stream")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            payload: payload.clone(),
        });
    }
    Some(BinanceEnvelope {
        topic: None,
        payload: value,
    })
}

/// Normalize a Binance source timestamp into millisecond and optional
/// microsecond representations.
fn normalize_source_timestamp(raw: u64) -> (u64, Option<u64>) {
    if raw >= 10_000_000_000_000 {
        (raw / 1_000, Some(raw))
    } else {
        (raw, None)
    }
}

/// Parse a string or numeric field into `f64`.
fn parse_f64_field(value: &serde_json::Value, field: &str) -> Option<f64> {
    value.get(field).and_then(|entry| {
        entry
            .as_f64()
            .or_else(|| entry.as_str().and_then(|raw| raw.parse::<f64>().ok()))
    })
}

/// Extract a symbol from a Binance payload or topic string.
fn extract_symbol(value: &serde_json::Value, topic: Option<&str>) -> Option<String> {
    value
        .get("s")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| topic.and_then(|raw| raw.split('@').next().map(str::to_uppercase)))
}

/// Parse a Binance depth-side array into order levels.
fn parse_levels(value: &serde_json::Value) -> Vec<OrderLevel> {
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };

    entries
        .iter()
        .filter_map(|entry| {
            let level = entry.as_array()?;
            let price = level.first()?.as_str()?.parse::<f64>().ok()?;
            let size = level.get(1)?.as_str()?.parse::<f64>().ok()?;
            Some(OrderLevel { price, size })
        })
        .collect()
}

/// Parse a trade payload into a `FeedMessage`.
fn parse_trade(
    payload: &serde_json::Value,
    topic: Option<&str>,
    connection_id: &str,
    raw_text: &str,
    retain_payloads: bool,
) -> Option<FeedMessage> {
    let event = payload.get("e").and_then(serde_json::Value::as_str)?;
    if event != "aggTrade" {
        return None;
    }

    let price = parse_f64_field(payload, "p")?;
    let quantity = parse_f64_field(payload, "q")?;
    let raw_timestamp = payload
        .get("T")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| payload.get("E").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);
    let (timestamp_ms, source_time_us) = normalize_source_timestamp(raw_timestamp);
    let is_buyer_maker = payload
        .get("m")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let signed_quantity = Some(if is_buyer_maker { -quantity } else { quantity });

    Some(FeedMessage::BinanceTrade {
        price,
        quantity,
        signed_quantity,
        timestamp_ms,
        timestamp_us: source_time_us,
        source_topic: topic.map(ToString::to_string),
        source_symbol: extract_symbol(payload, topic),
        sequence_key: payload
            .get("a")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value.to_string()),
        connection_id: connection_id.to_string(),
        payload_json: retain_payloads.then(|| raw_text.to_string()),
        details_json: retain_payloads.then(|| {
            serde_json::json!({
                "eventType": event,
                "buyerMaker": is_buyer_maker,
                "eventTime": payload.get("E"),
                "tradeTime": payload.get("T"),
            })
            .to_string()
        }),
    })
}

/// Parse a top-of-book payload into a `FeedMessage`.
fn parse_book_ticker(
    payload: &serde_json::Value,
    topic: Option<&str>,
    connection_id: &str,
    raw_text: &str,
    retain_payloads: bool,
) -> Option<FeedMessage> {
    let event = payload
        .get("e")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if event != "bookTicker" && topic.is_none_or(|raw| !raw.contains("bookTicker")) {
        return None;
    }

    let best_bid = parse_f64_field(payload, "b")?;
    let best_ask = parse_f64_field(payload, "a")?;
    let bid_size = parse_f64_field(payload, "B")?;
    let ask_size = parse_f64_field(payload, "A")?;
    let raw_timestamp = payload
        .get("E")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let (timestamp_ms, source_time_us) = normalize_source_timestamp(raw_timestamp);

    Some(FeedMessage::BinanceBookTicker {
        best_bid,
        best_ask,
        bid_size,
        ask_size,
        timestamp_ms,
        timestamp_us: source_time_us,
        source_topic: topic.map(ToString::to_string),
        source_symbol: extract_symbol(payload, topic),
        sequence_key: payload
            .get("u")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value.to_string()),
        connection_id: connection_id.to_string(),
        payload_json: retain_payloads.then(|| raw_text.to_string()),
        details_json: retain_payloads.then(|| {
            serde_json::json!({
                "eventType": event,
                "updateId": payload.get("u"),
            })
            .to_string()
        }),
    })
}

/// Parse a shallow-depth payload into a `FeedMessage`.
fn parse_depth(
    payload: &serde_json::Value,
    topic: Option<&str>,
    connection_id: &str,
    raw_text: &str,
    retain_payloads: bool,
) -> Option<FeedMessage> {
    let bid_levels = payload
        .get("bids")
        .map(parse_levels)
        .or_else(|| payload.get("b").map(parse_levels))?;
    let ask_levels = payload
        .get("asks")
        .map(parse_levels)
        .or_else(|| payload.get("a").map(parse_levels))?;
    let raw_timestamp = payload
        .get("E")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let (timestamp_ms, source_time_us) = normalize_source_timestamp(raw_timestamp);
    let sequence_key = payload
        .get("lastUpdateId")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| payload.get("u").and_then(serde_json::Value::as_u64))
        .map(|value| value.to_string());

    Some(FeedMessage::BinanceDepth {
        bid_levels,
        ask_levels,
        timestamp_ms,
        timestamp_us: source_time_us,
        source_topic: topic.map(ToString::to_string),
        source_symbol: extract_symbol(payload, topic),
        sequence_key,
        connection_id: connection_id.to_string(),
        payload_json: retain_payloads.then(|| raw_text.to_string()),
        details_json: retain_payloads.then(|| {
            serde_json::json!({
                "lastUpdateId": payload.get("lastUpdateId"),
                "eventTime": payload.get("E"),
            })
            .to_string()
        }),
    })
}

/// Parse a raw Binance text frame into one feed message.
pub(crate) fn process_binance_text(
    text: &str,
    connection_id: &str,
    retain_payloads: bool,
) -> Option<FeedMessage> {
    let envelope = parse_envelope(text)?;
    let topic = envelope.topic.as_deref();
    parse_trade(
        &envelope.payload,
        topic,
        connection_id,
        text,
        retain_payloads,
    )
    .or_else(|| {
        parse_book_ticker(
            &envelope.payload,
            topic,
            connection_id,
            text,
            retain_payloads,
        )
    })
    .or_else(|| {
        parse_depth(
            &envelope.payload,
            topic,
            connection_id,
            text,
            retain_payloads,
        )
    })
}

/// Parse one Binance text frame and forward the resulting feed message.
async fn process_binance_message(
    text: &str,
    tx: &mpsc::Sender<FeedMessage>,
    connection_id: &str,
    retain_payloads: bool,
) -> anyhow::Result<()> {
    if let Some(message) = process_binance_text(text, connection_id, retain_payloads) {
        tx.send(message)
            .await
            .map_err(|_| anyhow::anyhow!("channel closed"))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/binance_feed_tests.rs"]
mod tests;
