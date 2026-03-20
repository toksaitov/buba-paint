use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use super::FeedMessage;
use crate::config::Config;
use crate::types::{BookState, TopOfBook};

/// Handle returned to the main loop so it can trigger resubscription when the
/// active market window changes.
#[derive(Clone)]
pub struct ClobFeedHandle {
    resubscribe_tx: mpsc::Sender<(String, String)>,
}

impl ClobFeedHandle {
    /// Request the CLOB feed to resubscribe to new token IDs.
    ///
    /// The feed task will disconnect the current WebSocket, clear its internal
    /// book state, reconnect, and subscribe to the new market.
    pub async fn resubscribe(
        &self,
        up_token_id: String,
        down_token_id: String,
    ) -> anyhow::Result<()> {
        self.resubscribe_tx
            .send((up_token_id, down_token_id))
            .await
            .map_err(|_| anyhow::anyhow!("clob feed task gone"))
    }
}

use super::util::{backoff_delay, now_ms, should_reset_backoff};

/// Launch the CLOB WebSocket feed as a background tokio task.
///
/// Returns a `ClobFeedHandle` (for triggering resubscription) and a
/// `JoinHandle` for the spawned task.
#[allow(clippy::unused_async)]
pub async fn run_clob_feed(
    config: &Config,
    tx: mpsc::Sender<FeedMessage>,
) -> (ClobFeedHandle, tokio::task::JoinHandle<()>) {
    let (resub_tx, resub_rx) = mpsc::channel::<(String, String)>(4);

    let url = config.clob_ws_url.clone();
    let ping_interval_ms = config.clob_ping_interval;
    let base_delay = config.reconnect_base_delay;
    let max_delay = config.reconnect_max_delay;
    let min_stable_ms = config.reconnect_min_stable_ms;
    let max_failures = config.reconnect_max_failures;
    let feed_pause_ms = config.reconnect_pause_ms;

    let handle = tokio::spawn(async move {
        clob_feed_loop(
            url,
            tx,
            resub_rx,
            ping_interval_ms,
            base_delay,
            max_delay,
            min_stable_ms,
            max_failures,
            feed_pause_ms,
        )
        .await;
    });

    let feed_handle = ClobFeedHandle {
        resubscribe_tx: resub_tx,
    };
    (feed_handle, handle)
}

/// Internal event loop for the CLOB feed.
///
/// Maintains the current subscription tokens and reconnects on disconnect.
/// When a resubscription request arrives the current connection is torn down,
/// book state is cleared, and a fresh connection with the new subscription is
/// established.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
async fn clob_feed_loop(
    url: String,
    tx: mpsc::Sender<FeedMessage>,
    mut resub_rx: mpsc::Receiver<(String, String)>,
    ping_interval_ms: u64,
    base_delay: u64,
    max_delay: u64,
    min_stable_ms: u64,
    max_failures: u32,
    feed_pause_ms: u64,
) {
    let mut up_token: Option<String> = None;
    let mut down_token: Option<String> = None;
    let mut book_state = BookState::default();
    let mut attempt: u32 = 0;
    let mut rapid_disconnect_count: u32 = 0;

    loop {
        // Wait for initial subscription tokens if we don't have any yet.
        if up_token.is_none() || down_token.is_none() {
            match resub_rx.recv().await {
                Some((up, down)) => {
                    up_token = Some(up);
                    down_token = Some(down);
                    book_state = BookState::default();
                }
                None => {
                    // Channel closed — shut down.
                    return;
                }
            }
        }

        let (up_id, down_id) = match (&up_token, &down_token) {
            (Some(u), Some(d)) => (u.clone(), d.clone()),
            _ => continue,
        };

        info!(feed = "clob", "connecting to {url}");

        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws_stream, _response)) => {
                let connected_at = now_ms();
                let _ = tx
                    .send(FeedMessage::FeedConnected("clob".to_string()))
                    .await;

                let (mut write, mut read) = ws_stream.split();

                // Subscribe to the market.
                let sub_msg = serde_json::json!({
                    "type": "market",
                    "assets_ids": [&up_id, &down_id],
                });
                if let Err(e) = write.send(Message::Text(sub_msg.to_string().into())).await {
                    error!(feed = "clob", "failed to send subscription: {e}");
                    let _ = tx
                        .send(FeedMessage::FeedDisconnected("clob".to_string()))
                        .await;
                    if attempt >= max_failures {
                        error!(
                            feed = "clob",
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

                let mut disconnected = false;

                loop {
                    tokio::select! {
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    if let Err(e) = process_clob_message(
                                        &text,
                                        &up_id,
                                        &down_id,
                                        &mut book_state,
                                        &tx,
                                    ).await {
                                        warn!(feed = "clob", "failed to process message: {e}");
                                    }
                                }
                                Some(Ok(Message::Ping(data))) => {
                                    if write.send(Message::Pong(data)).await.is_err() {
                                        warn!(feed = "clob", "failed to send pong");
                                        break;
                                    }
                                }
                                Some(Ok(Message::Close(_))) => {
                                    info!(feed = "clob", "server sent close frame");
                                    break;
                                }
                                Some(Err(e)) => {
                                    error!(feed = "clob", "websocket error: {e}");
                                    break;
                                }
                                None => {
                                    info!(feed = "clob", "stream ended");
                                    break;
                                }
                                Some(Ok(_)) => {}
                            }
                        }
                        _ = ping_timer.tick() => {
                            // Send keepalive ping.
                            if write.send(Message::Ping(Vec::new().into())).await.is_err() {
                                warn!(feed = "clob", "failed to send ping");
                                break;
                            }
                        }
                        resub = resub_rx.recv() => {
                            match resub {
                                Some((new_up, new_down)) => {
                                    info!(feed = "clob", "resubscribing to new tokens");
                                    up_token = Some(new_up);
                                    down_token = Some(new_down);
                                    book_state = BookState::default();
                                    // Close current connection — outer loop will reconnect.
                                    let _ = write.send(Message::Close(None)).await;
                                    disconnected = true;
                                    break;
                                }
                                None => {
                                    // Channel closed — shut down.
                                    return;
                                }
                            }
                        }
                    }
                }

                if disconnected {
                    // Deliberate resubscription -- always reset.
                    attempt = 0;
                    rapid_disconnect_count = 0;
                } else {
                    let was_stable = should_reset_backoff(connected_at, now_ms(), min_stable_ms);
                    if was_stable {
                        attempt = 0;
                        rapid_disconnect_count = 0;
                    } else {
                        rapid_disconnect_count += 1;
                        if rapid_disconnect_count >= 3 {
                            warn!(
                                feed = "clob",
                                count = rapid_disconnect_count,
                                "CLOB disconnecting immediately after subscribe — market tokens may be expired"
                            );
                        }
                    }
                    let _ = tx
                        .send(FeedMessage::FeedDisconnected("clob".to_string()))
                        .await;
                    if attempt >= max_failures {
                        error!(
                            feed = "clob",
                            attempts = attempt,
                            pause_ms = feed_pause_ms,
                            "feed circuit breaker: pausing"
                        );
                        tokio::time::sleep(Duration::from_millis(feed_pause_ms)).await;
                        attempt = 0;
                        rapid_disconnect_count = 0;
                    } else {
                        let delay = backoff_delay(attempt, base_delay, max_delay);
                        warn!(
                            feed = "clob",
                            "reconnecting in {}ms (attempt {})",
                            delay.as_millis(),
                            attempt + 1
                        );
                        tokio::time::sleep(delay).await;
                        attempt = attempt.saturating_add(1);
                    }
                }
            }
            Err(e) => {
                error!(feed = "clob", "connection failed: {e}");
                let _ = tx
                    .send(FeedMessage::FeedDisconnected("clob".to_string()))
                    .await;
                if attempt >= max_failures {
                    error!(
                        feed = "clob",
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
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pure parsing types and functions — no I/O, no channels.
// ---------------------------------------------------------------------------

/// A single price-change entry parsed from a CLOB `price_change` event.
#[derive(Debug, PartialEq)]
pub(crate) struct ClobPriceChangeEntry {
    pub asset_id: String,
    pub side: String,
    pub price: f64,
    pub size: f64,
}

/// Structured update parsed from a CLOB WebSocket message.
#[derive(Debug, PartialEq)]
pub(crate) enum ClobUpdate {
    /// Full order-book snapshot for a single asset.
    BookSnapshot {
        asset_id: String,
        best_bid: f64,
        best_ask: f64,
        bid_size: f64,
        ask_size: f64,
        timestamp: u64,
    },
    /// Incremental price changes with a shared timestamp.
    PriceChange {
        timestamp: u64,
        changes: Vec<ClobPriceChangeEntry>,
    },
    /// Message type we intentionally skip (e.g. `last_trade_price`).
    Ignored,
}

/// Parse a single CLOB WebSocket JSON value into a `ClobUpdate`.
///
/// Pure function -- no I/O, no channels.
#[allow(clippy::similar_names)] // side / size are the actual field names
pub(crate) fn parse_clob_event(v: &serde_json::Value) -> ClobUpdate {
    let event_type = v.get("event_type").and_then(|e| e.as_str()).unwrap_or("");

    match event_type {
        "last_trade_price" => ClobUpdate::Ignored,
        "price_change" => {
            let timestamp = v
                .get("timestamp")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);

            let mut changes = Vec::new();
            if let Some(arr) = v.get("price_changes").and_then(|c| c.as_array()) {
                for change in arr {
                    let asset_id = change
                        .get("asset_id")
                        .and_then(|a| a.as_str())
                        .unwrap_or("")
                        .to_string();
                    let side = change
                        .get("side")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let price = parse_f64_field(change, "price").unwrap_or(0.0);
                    let size = parse_f64_field(change, "size").unwrap_or(0.0);
                    changes.push(ClobPriceChangeEntry {
                        asset_id,
                        side,
                        price,
                        size,
                    });
                }
            }
            ClobUpdate::PriceChange { timestamp, changes }
        }
        _ => {
            // Check if this is a book snapshot (has asset_id + bids/asks).
            if v.get("asset_id").is_some() && (v.get("bids").is_some() || v.get("asks").is_some()) {
                let asset_id = v
                    .get("asset_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let timestamp = v
                    .get("timestamp")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);

                let best_bid = extract_best_level(v.get("bids"), true);
                let best_ask = extract_best_level(v.get("asks"), false);

                ClobUpdate::BookSnapshot {
                    asset_id,
                    best_bid: best_bid.0,
                    best_ask: best_ask.0,
                    bid_size: best_bid.1,
                    ask_size: best_ask.1,
                    timestamp,
                }
            } else {
                ClobUpdate::Ignored
            }
        }
    }
}

/// Parse a CLOB WebSocket text frame into a list of `ClobUpdate`s.
///
/// If the top-level JSON is an array, each element is parsed individually.
/// Pure function -- no I/O, no channels.
pub(crate) fn parse_clob_text(text: &str) -> Vec<ClobUpdate> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![];
    };

    if let Some(arr) = v.as_array() {
        arr.iter().map(parse_clob_event).collect()
    } else {
        vec![parse_clob_event(&v)]
    }
}

/// Parse a CLOB WebSocket message and update the internal book state.
///
/// The CLOB WS emits several message shapes:
///   - JSON arrays: each element is processed individually.
///   - `event_type: "price_change"`: top-of-book update from `price_changes`.
///   - Book snapshots (have `asset_id` + `bids`/`asks`): full level rebuild.
///   - `event_type: "last_trade_price"`: ignored.
async fn process_clob_message(
    text: &str,
    up_token: &str,
    down_token: &str,
    book_state: &mut BookState,
    tx: &mpsc::Sender<FeedMessage>,
) -> anyhow::Result<()> {
    let updates = parse_clob_text(text);
    if updates.is_empty() {
        anyhow::bail!("failed to parse CLOB message");
    }

    for update in updates {
        match update {
            ClobUpdate::Ignored => {}
            ClobUpdate::PriceChange { timestamp, changes } => {
                for entry in &changes {
                    let tob = if entry.asset_id == up_token {
                        book_state.up.get_or_insert(TopOfBook {
                            best_bid: 0.0,
                            best_ask: 0.0,
                            bid_size: 0.0,
                            ask_size: 0.0,
                            timestamp: 0,
                        })
                    } else if entry.asset_id == down_token {
                        book_state.down.get_or_insert(TopOfBook {
                            best_bid: 0.0,
                            best_ask: 0.0,
                            bid_size: 0.0,
                            ask_size: 0.0,
                            timestamp: 0,
                        })
                    } else {
                        continue;
                    };

                    match entry.side.as_str() {
                        "BUY" => {
                            tob.best_bid = entry.price;
                            tob.bid_size = entry.size;
                        }
                        "SELL" => {
                            tob.best_ask = entry.price;
                            tob.ask_size = entry.size;
                        }
                        _ => {}
                    }

                    tob.timestamp = timestamp;
                }

                tx.send(FeedMessage::ClobPriceChange {
                    book_state: book_state.clone(),
                })
                .await
                .map_err(|_| anyhow::anyhow!("channel closed"))?;
            }
            ClobUpdate::BookSnapshot {
                asset_id,
                best_bid,
                best_ask,
                bid_size,
                ask_size,
                timestamp,
            } => {
                let tob = TopOfBook {
                    best_bid,
                    best_ask,
                    bid_size,
                    ask_size,
                    timestamp,
                };

                if asset_id == up_token {
                    book_state.up = Some(tob);
                } else if asset_id == down_token {
                    book_state.down = Some(tob);
                }

                tx.send(FeedMessage::ClobBook {
                    book_state: book_state.clone(),
                })
                .await
                .map_err(|_| anyhow::anyhow!("channel closed"))?;
            }
        }
    }

    Ok(())
}

/// Extract the best (highest bid or lowest ask) price and size from a levels array.
///
/// Returns `(price, size)`.  If the array is empty or missing, returns `(0.0, 0.0)`.
// Sentinel comparisons (f64::MIN / f64::MAX) are exact bit patterns, not computed floats.
#[allow(clippy::float_cmp)]
pub(crate) fn extract_best_level(levels: Option<&serde_json::Value>, is_bid: bool) -> (f64, f64) {
    let arr = match levels.and_then(|l| l.as_array()) {
        Some(a) if !a.is_empty() => a,
        _ => return (0.0, 0.0),
    };

    let mut best_price: f64 = if is_bid { f64::MIN } else { f64::MAX };
    let mut best_size: f64 = 0.0;

    for level in arr {
        let price = parse_f64_field(level, "price").unwrap_or(0.0);
        let size = parse_f64_field(level, "size").unwrap_or(0.0);

        if is_bid {
            if price > best_price {
                best_price = price;
                best_size = size;
            }
        } else if price < best_price {
            best_price = price;
            best_size = size;
        }
    }

    if (is_bid && best_price == f64::MIN) || (!is_bid && best_price == f64::MAX) {
        return (0.0, 0.0);
    }

    (best_price, best_size)
}

/// Parse a JSON field that may be either a number or a string-encoded number.
pub(crate) fn parse_f64_field(v: &serde_json::Value, field: &str) -> Option<f64> {
    v.get(field).and_then(|val| {
        val.as_f64()
            .or_else(|| val.as_str().and_then(|s| s.parse::<f64>().ok()))
    })
}

#[cfg(test)]
#[path = "tests/clob_feed_tests.rs"]
mod tests;
