use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use super::FeedMessage;
use crate::config::Config;

use super::util::{backoff_delay, now_ms, should_reset_backoff};

/// Run the Binance aggTrade `WebSocket` feed.
///
/// Connects to the Binance aggTrade stream, parses trade messages, and sends
/// `FeedMessage::BinanceTick` updates through `tx`. On disconnect the feed
/// waits with exponential backoff and reconnects automatically.
///
/// This function runs forever (or until the channel is closed).
pub async fn run_binance_feed(
    config: &Config,
    tx: mpsc::Sender<FeedMessage>,
) -> anyhow::Result<()> {
    let url = &config.binance_ws_url;
    let base_delay = config.reconnect_base_delay;
    let max_delay = config.reconnect_max_delay;
    let min_stable_ms = config.reconnect_min_stable_ms;
    let max_failures = config.reconnect_max_failures;
    let feed_pause_ms = config.reconnect_pause_ms;
    let mut attempt: u32 = 0;

    loop {
        info!(feed = "binance", "connecting to {url}");

        match tokio_tungstenite::connect_async(url).await {
            Ok((ws_stream, _response)) => {
                let connected_at = now_ms();
                if tx
                    .send(FeedMessage::FeedConnected("binance".to_string()))
                    .await
                    .is_err()
                {
                    return Ok(());
                }

                let (mut write, mut read) = ws_stream.split();

                loop {
                    match read.next().await {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = process_binance_message(&text, &tx).await {
                                warn!(feed = "binance", "failed to process message: {e}");
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            if write.send(Message::Pong(data)).await.is_err() {
                                warn!(feed = "binance", "failed to send pong");
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!(feed = "binance", "server sent close frame");
                            break;
                        }
                        Some(Err(e)) => {
                            error!(feed = "binance", "websocket error: {e}");
                            break;
                        }
                        None => {
                            info!(feed = "binance", "stream ended");
                            break;
                        }

                        Some(Ok(_)) => {}
                    }
                }

                if should_reset_backoff(connected_at, now_ms(), min_stable_ms) {
                    attempt = 0;
                }
            }
            Err(e) => {
                error!(feed = "binance", "connection failed: {e}");
            }
        }

        let _ = tx
            .send(FeedMessage::FeedDisconnected("binance".to_string()))
            .await;

        if attempt >= max_failures {
            error!(
                feed = "binance",
                attempts = attempt,
                pause_ms = feed_pause_ms,
                "feed circuit breaker: pausing"
            );
            tokio::time::sleep(std::time::Duration::from_millis(feed_pause_ms)).await;
            attempt = 0;
        } else {
            let delay = backoff_delay(attempt, base_delay, max_delay);
            warn!(
                feed = "binance",
                "reconnecting in {}ms (attempt {})",
                delay.as_millis(),
                attempt + 1
            );
            tokio::time::sleep(delay).await;
            attempt = attempt.saturating_add(1);
        }
    }
}

/// Parse a Binance aggTrade `JSON` value into (price, timestamp).
///
/// Returns `None` if the event type is not `aggTrade` or required fields are
/// missing.
pub(crate) fn parse_agg_trade(raw: &serde_json::Value) -> Option<(f64, u64)> {
    if raw.get("e").and_then(|e| e.as_str()) != Some("aggTrade") {
        return None;
    }

    let price: f64 = raw
        .get("p")
        .and_then(|p| p.as_str())
        .and_then(|s| s.parse().ok())?;

    let timestamp = raw
        .get("T")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| raw.get("E").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);

    Some((price, timestamp))
}

/// Parse a raw `WebSocket` text frame into a `FeedMessage`.
///
/// Pure function -- no I/O, no channels.
pub(crate) fn process_binance_text(text: &str) -> Option<super::FeedMessage> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let (price, timestamp) = parse_agg_trade(&v)?;
    Some(super::FeedMessage::BinanceTick {
        price,
        timestamp,
        payload_json: Some(text.to_string()),
    })
}

/// Parse a single Binance aggTrade `JSON` message and send it on the channel.
async fn process_binance_message(text: &str, tx: &mpsc::Sender<FeedMessage>) -> anyhow::Result<()> {
    if let Some(FeedMessage::BinanceTick {
        price,
        timestamp,
        payload_json,
    }) = process_binance_text(text)
    {
        tx.send(FeedMessage::BinanceTick {
            price,
            timestamp,
            payload_json,
        })
        .await
        .map_err(|_| anyhow::anyhow!("channel closed"))?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/binance_feed_tests.rs"]
mod tests;
