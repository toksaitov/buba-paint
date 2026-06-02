use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use super::FeedMessage;
use crate::config::{Config, FeedEventStorageProfile};
use crate::types::{BookState, TopOfBook};

/// Handle returned to the main loop so it can trigger resubscription when the
/// active market window changes.
#[derive(Clone)]
pub struct ClobFeedHandle {
    resubscribe_tx: mpsc::Sender<(String, String)>,
}

impl ClobFeedHandle {
    /// Request the `CLOB` feed to resubscribe to new token IDs.
    ///
    /// The feed task will disconnect the current `WebSocket`, clear its internal
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

use super::util::{
    FeedDisconnectCause, FeedDisconnectReport, backoff_delay, now_ms, now_us, should_reset_backoff,
};

/// Launch the `CLOB` `WebSocket` feed as a background tokio task.
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
    let connect_timeout_ms = config.websocket_connect_timeout_ms;
    let no_message_reconnect_ms = config.clob_no_message_reconnect_ms;
    let retain_payloads = config.feed_event_storage_profile == FeedEventStorageProfile::FullDebug;

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
            connect_timeout_ms,
            no_message_reconnect_ms,
            retain_payloads,
        )
        .await;
    });

    let feed_handle = ClobFeedHandle {
        resubscribe_tx: resub_tx,
    };
    (feed_handle, handle)
}

/// Internal event loop for the `CLOB` feed.
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
    connect_timeout_ms: u64,
    no_message_reconnect_ms: u64,
    retain_payloads: bool,
) {
    let mut up_token: Option<String> = None;
    let mut down_token: Option<String> = None;
    let mut book_state = BookState::default();
    let mut attempt: u32 = 0;
    let mut rapid_disconnect_count: u32 = 0;
    let mut connect_after_resubscribe = false;

    loop {
        if up_token.is_none() || down_token.is_none() {
            match resub_rx.recv().await {
                Some((up, down)) => {
                    up_token = Some(up);
                    down_token = Some(down);
                    book_state = BookState::default();
                }
                None => {
                    return;
                }
            }
        }
        connect_after_resubscribe |=
            apply_latest_resubscribe(&mut resub_rx, &mut up_token, &mut down_token);

        let (up_id, down_id) = match (&up_token, &down_token) {
            (Some(u), Some(d)) => (u.clone(), d.clone()),
            _ => continue,
        };

        info!(feed = "clob", "connecting to {url}");

        let disconnect = match tokio::time::timeout(
            Duration::from_millis(connect_timeout_ms),
            tokio_tungstenite::connect_async(&url),
        )
        .await
        {
            Ok(Ok((ws_stream, _response))) => {
                let connected_at = now_ms();
                let connection_id = format!("clob-{}", now_us());
                let _ = tx
                    .send(FeedMessage::FeedConnected {
                        name: "clob".to_string(),
                        connection_id: connection_id.clone(),
                    })
                    .await;

                let (mut write, mut read) = ws_stream.split();

                let sub_msg = serde_json::json!({
                    "type": "market",
                    "assets_ids": [&up_id, &down_id],
                    "custom_feature_enabled": true,
                });
                if let Err(e) = write.send(Message::Text(sub_msg.to_string().into())).await {
                    error!(feed = "clob", "failed to send subscription: {e}");
                    FeedDisconnectReport {
                        connection_id: Some(connection_id),
                        cause: FeedDisconnectCause::ConnectionFailed,
                        connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                        after_resubscribe: connect_after_resubscribe,
                        error: Some(format!("failed to send subscription: {e}")),
                        timeout_ms: None,
                    }
                } else {
                    let mut ping_timer =
                        tokio::time::interval(Duration::from_millis(ping_interval_ms));
                    let idle_duration = Duration::from_millis(no_message_reconnect_ms);
                    let idle_sleep = tokio::time::sleep(idle_duration);
                    tokio::pin!(idle_sleep);

                    loop {
                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        idle_sleep.as_mut().reset(tokio::time::Instant::now() + idle_duration);
                                        if let Err(e) = process_clob_message(
                                            &text,
                                            &up_id,
                                            &down_id,
                                            &mut book_state,
                                            &tx,
                                            &connection_id,
                                            retain_payloads,
                                        ).await {
                                            warn!(feed = "clob", "failed to process message: {e}");
                                        }
                                    }
                                    Some(Ok(Message::Ping(data))) => {
                                        if write.send(Message::Pong(data)).await.is_err() {
                                            warn!(feed = "clob", "failed to send pong");
                                            break FeedDisconnectReport {
                                                connection_id: Some(connection_id.clone()),
                                                cause: FeedDisconnectCause::PingFailure,
                                                connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                                after_resubscribe: connect_after_resubscribe,
                                                error: Some("failed to send pong".to_string()),
                                                timeout_ms: None,
                                            };
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) => {
                                        info!(feed = "clob", "server sent close frame");
                                        break FeedDisconnectReport {
                                            connection_id: Some(connection_id.clone()),
                                            cause: FeedDisconnectCause::ConnectionFailed,
                                            connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                            after_resubscribe: connect_after_resubscribe,
                                            error: Some("server sent close frame".to_string()),
                                            timeout_ms: None,
                                        };
                                    }
                                    Some(Err(e)) => {
                                        error!(feed = "clob", "websocket error: {e}");
                                        break FeedDisconnectReport {
                                            connection_id: Some(connection_id.clone()),
                                            cause: FeedDisconnectCause::WebsocketError,
                                            connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                            after_resubscribe: connect_after_resubscribe,
                                            error: Some(e.to_string()),
                                            timeout_ms: None,
                                        };
                                    }
                                    None => {
                                        info!(feed = "clob", "stream ended");
                                        break FeedDisconnectReport {
                                            connection_id: Some(connection_id.clone()),
                                            cause: FeedDisconnectCause::ConnectionFailed,
                                            connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                            after_resubscribe: connect_after_resubscribe,
                                            error: Some("stream ended".to_string()),
                                            timeout_ms: None,
                                        };
                                    }
                                    Some(Ok(_)) => {}
                                }
                            }
                            _ = ping_timer.tick() => {
                                if write.send(Message::Ping(Vec::new().into())).await.is_err() {
                                    warn!(feed = "clob", "failed to send ping");
                                    break FeedDisconnectReport {
                                        connection_id: Some(connection_id.clone()),
                                        cause: FeedDisconnectCause::PingFailure,
                                        connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                        after_resubscribe: connect_after_resubscribe,
                                        error: Some("failed to send ping".to_string()),
                                        timeout_ms: None,
                                    };
                                }
                            }
                            () = &mut idle_sleep => {
                                warn!(
                                    feed = "clob",
                                    timeout_ms = no_message_reconnect_ms,
                                    "no market data in {no_message_reconnect_ms}ms; forcing reconnect"
                                );
                                break FeedDisconnectReport {
                                    connection_id: Some(connection_id.clone()),
                                    cause: FeedDisconnectCause::IdleTimeout,
                                    connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                    after_resubscribe: connect_after_resubscribe,
                                    error: None,
                                    timeout_ms: Some(no_message_reconnect_ms),
                                };
                            }
                            resub = resub_rx.recv() => {
                                match resub {
                                    Some((new_up, new_down)) => {
                                        info!(feed = "clob", "resubscribing to new tokens");
                                        up_token = Some(new_up);
                                        down_token = Some(new_down);
                                        apply_latest_resubscribe(&mut resub_rx, &mut up_token, &mut down_token);
                                        book_state = BookState::default();
                                        connect_after_resubscribe = true;
                                        let _ = write.send(Message::Close(None)).await;
                                        break FeedDisconnectReport {
                                            connection_id: Some(connection_id.clone()),
                                            cause: FeedDisconnectCause::ConnectionFailed,
                                            connection_lifetime_ms: Some(now_ms().saturating_sub(connected_at)),
                                            after_resubscribe: true,
                                            error: Some("resubscribe_requested".to_string()),
                                            timeout_ms: None,
                                        };
                                    }
                                    None => return,
                                }
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                error!(feed = "clob", "connection failed: {e}");
                FeedDisconnectReport {
                    connection_id: None,
                    cause: FeedDisconnectCause::ConnectionFailed,
                    connection_lifetime_ms: None,
                    after_resubscribe: connect_after_resubscribe,
                    error: Some(e.to_string()),
                    timeout_ms: None,
                }
            }
            Err(_) => {
                error!(
                    feed = "clob",
                    timeout_ms = connect_timeout_ms,
                    "websocket connect timed out"
                );
                FeedDisconnectReport {
                    connection_id: None,
                    cause: FeedDisconnectCause::ConnectTimeout,
                    connection_lifetime_ms: None,
                    after_resubscribe: connect_after_resubscribe,
                    error: Some(format!(
                        "websocket connect timed out after {connect_timeout_ms}ms"
                    )),
                    timeout_ms: Some(connect_timeout_ms),
                }
            }
        };

        if disconnect.error.as_deref() == Some("resubscribe_requested") {
            attempt = 0;
            rapid_disconnect_count = 0;
            continue;
        }

        if let Some(connection_lifetime_ms) = disconnect.connection_lifetime_ms {
            if should_reset_backoff(0, connection_lifetime_ms, min_stable_ms) {
                attempt = 0;
                rapid_disconnect_count = 0;
            } else {
                rapid_disconnect_count += 1;
                if rapid_disconnect_count >= 3 {
                    warn!(
                        feed = "clob",
                        count = rapid_disconnect_count,
                        "CLOB disconnecting immediately after subscribe - market tokens may be expired"
                    );
                }
            }
        }

        let reconnect_delay = if attempt >= max_failures {
            error!(
                feed = "clob",
                attempts = attempt,
                pause_ms = feed_pause_ms,
                "feed circuit breaker: pausing"
            );
            rapid_disconnect_count = 0;
            Duration::from_millis(feed_pause_ms)
        } else {
            let delay = backoff_delay(attempt, base_delay, max_delay);
            warn!(
                feed = "clob",
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
                name: "clob".to_string(),
                connection_id: disconnect.connection_id,
                cause_class: disconnect.cause.as_str(),
                details_json,
            })
            .await;
        connect_after_resubscribe = false;

        tokio::time::sleep(reconnect_delay).await;
        if attempt >= max_failures {
            attempt = 0;
        } else {
            attempt = attempt.saturating_add(1);
        }
    }
}

/// Drain queued resubscribe requests so reconnects always use the newest token pair.
fn apply_latest_resubscribe(
    resub_rx: &mut mpsc::Receiver<(String, String)>,
    up_token: &mut Option<String>,
    down_token: &mut Option<String>,
) -> bool {
    let mut updated = false;
    while let Ok((up, down)) = resub_rx.try_recv() {
        *up_token = Some(up);
        *down_token = Some(down);
        updated = true;
    }
    updated
}

/// A single price-change entry parsed from a `CLOB` `price_change` event.
#[derive(Debug, PartialEq)]
pub(crate) struct ClobPriceChangeEntry {
    pub asset_id: String,
    pub side: String,
    pub price: f64,
    pub size: f64,
}

/// Structured update parsed from a `CLOB` `WebSocket` message.
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
        asset_id: Option<String>,
        timestamp: u64,
        timestamp_us: Option<u64>,
        changes: Vec<ClobPriceChangeEntry>,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
        bid_size: Option<f64>,
        ask_size: Option<f64>,
    },
    /// A direct best-bid-ask update for one asset.
    BestBidAsk {
        asset_id: String,
        best_bid: f64,
        best_ask: f64,
        bid_size: Option<f64>,
        ask_size: Option<f64>,
        timestamp: u64,
        timestamp_us: Option<u64>,
    },
    /// A non-book metadata event worth persisting but not trading on.
    MetaEvent {
        event_type: String,
        asset_id: Option<String>,
        timestamp: u64,
        timestamp_us: Option<u64>,
    },
    /// Message type we intentionally skip (e.g. `last_trade_price`).
    Ignored,
}

/// Direct top-of-book fields carried by certain `CLOB` events.
struct DirectTopOfBookUpdate<'a> {
    asset_id: &'a str,
    timestamp_ms: u64,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
    bid_size: Option<f64>,
    ask_size: Option<f64>,
}

/// Parse a single `CLOB` `WebSocket` `JSON` value into a `ClobUpdate`.
///
/// Pure function -- no I/O, no channels.
#[allow(clippy::similar_names)]
pub(crate) fn parse_clob_event(v: &serde_json::Value) -> ClobUpdate {
    let event_type = v.get("event_type").and_then(|e| e.as_str()).unwrap_or("");
    let raw_timestamp = v
        .get("timestamp")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let (timestamp, timestamp_us) = normalize_source_timestamp(raw_timestamp);

    match event_type {
        "last_trade_price" | "tick_size_change" | "new_market" | "market_resolved" => {
            parse_meta_event(v, event_type, timestamp, timestamp_us)
        }
        "best_bid_ask" => parse_best_bid_ask_event(v, timestamp, timestamp_us),
        "price_change" => parse_price_change_event(v, timestamp, timestamp_us),
        _ => parse_book_snapshot_event(v).unwrap_or(ClobUpdate::Ignored),
    }
}

/// Parse a metadata-only `CLOB` event.
fn parse_meta_event(
    value: &serde_json::Value,
    event_type: &str,
    timestamp_ms: u64,
    source_micros: Option<u64>,
) -> ClobUpdate {
    ClobUpdate::MetaEvent {
        event_type: event_type.to_string(),
        asset_id: value
            .get("asset_id")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string),
        timestamp: timestamp_ms,
        timestamp_us: source_micros,
    }
}

/// Parse a direct best-bid-ask `CLOB` event.
fn parse_best_bid_ask_event(
    value: &serde_json::Value,
    timestamp_ms: u64,
    source_micros: Option<u64>,
) -> ClobUpdate {
    ClobUpdate::BestBidAsk {
        asset_id: value
            .get("asset_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        best_bid: parse_f64_field(value, "best_bid").unwrap_or(0.0),
        best_ask: parse_f64_field(value, "best_ask").unwrap_or(0.0),
        bid_size: parse_f64_field(value, "bid_size")
            .or_else(|| parse_f64_field(value, "best_bid_size")),
        ask_size: parse_f64_field(value, "ask_size")
            .or_else(|| parse_f64_field(value, "best_ask_size")),
        timestamp: timestamp_ms,
        timestamp_us: source_micros,
    }
}

/// Parse an incremental `price_change` event.
fn parse_price_change_event(
    value: &serde_json::Value,
    timestamp_ms: u64,
    source_micros: Option<u64>,
) -> ClobUpdate {
    let changes = value
        .get("price_changes")
        .and_then(serde_json::Value::as_array)
        .map_or_else(Vec::new, |entries| parse_price_change_entries(entries));
    ClobUpdate::PriceChange {
        asset_id: value
            .get("asset_id")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string),
        timestamp: timestamp_ms,
        timestamp_us: source_micros,
        changes,
        best_bid: parse_f64_field(value, "best_bid"),
        best_ask: parse_f64_field(value, "best_ask"),
        bid_size: parse_f64_field(value, "bid_size")
            .or_else(|| parse_f64_field(value, "best_bid_size")),
        ask_size: parse_f64_field(value, "ask_size")
            .or_else(|| parse_f64_field(value, "best_ask_size")),
    }
}

/// Parse the per-level changes inside a `price_change` event.
fn parse_price_change_entries(changes: &[serde_json::Value]) -> Vec<ClobPriceChangeEntry> {
    changes
        .iter()
        .map(|change| ClobPriceChangeEntry {
            asset_id: change
                .get("asset_id")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            side: change
                .get("side")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            price: parse_f64_field(change, "price").unwrap_or(0.0),
            size: parse_f64_field(change, "size").unwrap_or(0.0),
        })
        .collect()
}

/// Parse a full-book snapshot event when the payload does not carry `event_type`.
fn parse_book_snapshot_event(value: &serde_json::Value) -> Option<ClobUpdate> {
    if value.get("asset_id").is_none()
        || (value.get("bids").is_none() && value.get("asks").is_none())
    {
        return None;
    }

    let asset_id = value
        .get("asset_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let (timestamp_ms, _timestamp_us) = value
        .get("timestamp")
        .and_then(serde_json::Value::as_u64)
        .map_or((0, None), normalize_source_timestamp);
    let best_bid = extract_best_level(value.get("bids"), true);
    let best_ask = extract_best_level(value.get("asks"), false);

    Some(ClobUpdate::BookSnapshot {
        asset_id,
        best_bid: best_bid.0,
        best_ask: best_ask.0,
        bid_size: best_bid.1,
        ask_size: best_ask.1,
        timestamp: timestamp_ms,
    })
}

/// Parse a `CLOB` `WebSocket` text frame into a list of `ClobUpdate`s.
///
/// If the top-level `JSON` is an array, each element is parsed individually.
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

/// Parse a `CLOB` `WebSocket` message and update the internal book state.
///
/// The `CLOB` WS emits several message shapes:
///   - `JSON` arrays: each element is processed individually.
///   - `event_type: "price_change"`: top-of-book update from `price_changes`.
///   - Book snapshots (have `asset_id` + `bids`/`asks`): full level rebuild.
///   - `event_type: "last_trade_price"`: ignored.
async fn process_clob_message(
    text: &str,
    up_token: &str,
    down_token: &str,
    book_state: &mut BookState,
    tx: &mpsc::Sender<FeedMessage>,
    connection_id: &str,
    retain_payloads: bool,
) -> anyhow::Result<()> {
    let updates = parse_clob_text(text);
    if updates.is_empty() {
        anyhow::bail!("failed to parse CLOB message");
    }

    for update in updates {
        let mut context = ClobDispatchContext {
            up_token,
            down_token,
            book_state,
            tx,
            connection_id,
            raw_text: text,
            retain_payloads,
        };
        handle_clob_update(update, &mut context).await?;
    }

    Ok(())
}

/// Bundle the shared inputs needed to route one parsed `CLOB` update.
struct ClobDispatchContext<'a> {
    up_token: &'a str,
    down_token: &'a str,
    book_state: &'a mut BookState,
    tx: &'a mpsc::Sender<FeedMessage>,
    connection_id: &'a str,
    raw_text: &'a str,
    retain_payloads: bool,
}

/// Bundle the shared transport settings for one emitted `CLOB` message.
struct ClobEmitContext<'a> {
    tx: &'a mpsc::Sender<FeedMessage>,
    connection_id: &'a str,
    raw_text: &'a str,
    retain_payloads: bool,
}

/// Route one parsed `CLOB` update to the matching state mutation and channel send.
#[allow(clippy::too_many_lines)]
async fn handle_clob_update(
    update: ClobUpdate,
    context: &mut ClobDispatchContext<'_>,
) -> anyhow::Result<()> {
    match update {
        ClobUpdate::Ignored => Ok(()),
        ClobUpdate::MetaEvent {
            event_type,
            asset_id,
            timestamp,
            timestamp_us,
        } => {
            send_clob_meta_event(
                &ClobEmitContext {
                    tx: context.tx,
                    connection_id: context.connection_id,
                    raw_text: context.raw_text,
                    retain_payloads: context.retain_payloads,
                },
                ClobMetaEvent {
                    event_type,
                    asset_id,
                    timestamp_ms: timestamp,
                    source_micros: timestamp_us,
                },
            )
            .await
        }
        ClobUpdate::PriceChange {
            asset_id,
            timestamp,
            timestamp_us,
            changes,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
        } => {
            apply_price_change_entries(
                context.book_state,
                context.up_token,
                context.down_token,
                &changes,
                timestamp,
            );
            if let Some(asset_id) = asset_id.as_deref() {
                apply_direct_tob(
                    context.book_state,
                    context.up_token,
                    context.down_token,
                    &DirectTopOfBookUpdate {
                        asset_id,
                        timestamp_ms: timestamp,
                        best_bid,
                        best_ask,
                        bid_size,
                        ask_size,
                    },
                );
            }
            send_clob_price_change(
                context.tx,
                context.connection_id,
                context.raw_text,
                context.book_state,
                asset_id,
                timestamp,
                timestamp_us,
                best_bid,
                best_ask,
                changes.len(),
                context.retain_payloads,
            )
            .await
        }
        ClobUpdate::BestBidAsk {
            asset_id,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            timestamp,
            timestamp_us,
        } => {
            apply_direct_tob(
                context.book_state,
                context.up_token,
                context.down_token,
                &DirectTopOfBookUpdate {
                    asset_id: &asset_id,
                    timestamp_ms: timestamp,
                    best_bid: Some(best_bid),
                    best_ask: Some(best_ask),
                    bid_size,
                    ask_size,
                },
            );
            let (merged_bid_size, merged_ask_size) = merged_side_sizes(
                context.book_state,
                context.up_token,
                context.down_token,
                &asset_id,
            );
            send_clob_best_bid_ask(
                context.tx,
                context.connection_id,
                context.raw_text,
                context.book_state,
                asset_id,
                timestamp,
                timestamp_us,
                best_bid,
                best_ask,
                merged_bid_size,
                merged_ask_size,
                context.retain_payloads,
            )
            .await
        }
        ClobUpdate::BookSnapshot {
            asset_id,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            timestamp,
        } => {
            apply_book_snapshot(
                context.book_state,
                context.up_token,
                context.down_token,
                &asset_id,
                best_bid,
                best_ask,
                bid_size,
                ask_size,
                timestamp,
            );
            send_clob_book_snapshot(
                context.tx,
                context.connection_id,
                context.raw_text,
                context.book_state,
                asset_id,
                best_bid,
                best_ask,
                timestamp,
                context.retain_payloads,
            )
            .await
        }
    }
}

/// Carry the durable fields for one emitted `CLOB` metadata event.
struct ClobMetaEvent {
    event_type: String,
    asset_id: Option<String>,
    timestamp_ms: u64,
    source_micros: Option<u64>,
}

/// Emit one metadata-only `CLOB` message.
async fn send_clob_meta_event(
    context: &ClobEmitContext<'_>,
    event: ClobMetaEvent,
) -> anyhow::Result<()> {
    context
        .tx
        .send(FeedMessage::ClobMetaEvent {
            event_type: event.event_type,
            timestamp_ms: event.timestamp_ms,
            timestamp_us: event.source_micros,
            asset_id: event.asset_id,
            source_topic: Some("market".to_string()),
            connection_id: context.connection_id.to_string(),
            payload_json: context
                .retain_payloads
                .then(|| context.raw_text.to_string()),
            details_json: Some(
                serde_json::json!({
                    "topic": "market",
                    "timestamp": event.timestamp_ms,
                })
                .to_string(),
            ),
        })
        .await
        .map_err(|_| anyhow::anyhow!("channel closed"))
}

/// Apply one `price_change` batch to the in-memory binary book.
fn apply_price_change_entries(
    book_state: &mut BookState,
    up_token: &str,
    down_token: &str,
    changes: &[ClobPriceChangeEntry],
    timestamp_ms: u64,
) {
    let observed_at_ms = now_ms();
    for entry in changes {
        let top_of_book = resolve_top_of_book(book_state, up_token, down_token, &entry.asset_id);
        let Some(top_of_book) = top_of_book else {
            continue;
        };

        match entry.side.as_str() {
            "BUY" => {
                top_of_book.best_bid = entry.price;
                top_of_book.bid_size = entry.size;
            }
            "SELL" => {
                top_of_book.best_ask = entry.price;
                top_of_book.ask_size = entry.size;
            }
            _ => {}
        }

        top_of_book.timestamp = timestamp_ms;
        top_of_book.observed_at_ms = observed_at_ms;
    }
}

/// Resolve or create the mutable book side for a given asset id.
fn resolve_top_of_book<'a>(
    book_state: &'a mut BookState,
    up_token: &str,
    down_token: &str,
    asset_id: &str,
) -> Option<&'a mut TopOfBook> {
    if asset_id == up_token {
        return Some(book_state.up.get_or_insert(TopOfBook {
            best_bid: 0.0,
            best_ask: 0.0,
            bid_size: 0.0,
            ask_size: 0.0,
            timestamp: 0,
            observed_at_ms: 0,
        }));
    }
    if asset_id == down_token {
        return Some(book_state.down.get_or_insert(TopOfBook {
            best_bid: 0.0,
            best_ask: 0.0,
            bid_size: 0.0,
            ask_size: 0.0,
            timestamp: 0,
            observed_at_ms: 0,
        }));
    }
    None
}

/// Emit one incremental `price_change` message after state mutation.
#[allow(clippy::too_many_arguments)]
async fn send_clob_price_change(
    tx: &mpsc::Sender<FeedMessage>,
    connection_id: &str,
    raw_text: &str,
    book_state: &BookState,
    asset_id: Option<String>,
    timestamp_ms: u64,
    source_micros: Option<u64>,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
    change_count: usize,
    retain_payloads: bool,
) -> anyhow::Result<()> {
    tx.send(FeedMessage::ClobPriceChange {
        book_state: book_state.clone(),
        timestamp_ms,
        timestamp_us: source_micros,
        asset_id,
        source_topic: Some("market".to_string()),
        connection_id: connection_id.to_string(),
        payload_json: retain_payloads.then(|| raw_text.to_string()),
        details_json: retain_payloads.then(|| {
            serde_json::json!({
                "changeCount": change_count,
                "bestBid": best_bid,
                "bestAsk": best_ask,
            })
            .to_string()
        }),
    })
    .await
    .map_err(|_| anyhow::anyhow!("channel closed"))
}

/// Emit one direct best-bid-ask message after state mutation.
#[allow(clippy::too_many_arguments)]
async fn send_clob_best_bid_ask(
    tx: &mpsc::Sender<FeedMessage>,
    connection_id: &str,
    raw_text: &str,
    book_state: &BookState,
    asset_id: String,
    timestamp_ms: u64,
    source_micros: Option<u64>,
    best_bid: f64,
    best_ask: f64,
    bid_size: f64,
    ask_size: f64,
    retain_payloads: bool,
) -> anyhow::Result<()> {
    tx.send(FeedMessage::ClobBestBidAsk {
        book_state: book_state.clone(),
        timestamp_ms,
        timestamp_us: source_micros,
        asset_id: Some(asset_id),
        source_topic: Some("best_bid_ask".to_string()),
        connection_id: connection_id.to_string(),
        payload_json: retain_payloads.then(|| raw_text.to_string()),
        details_json: retain_payloads.then(|| {
            serde_json::json!({
                "bestBid": best_bid,
                "bestAsk": best_ask,
                "bidSize": bid_size,
                "askSize": ask_size,
            })
            .to_string()
        }),
    })
    .await
    .map_err(|_| anyhow::anyhow!("channel closed"))
}

/// Apply one full-book snapshot to the binary book state.
#[allow(clippy::too_many_arguments)]
fn apply_book_snapshot(
    book_state: &mut BookState,
    up_token: &str,
    down_token: &str,
    asset_id: &str,
    best_bid: f64,
    best_ask: f64,
    bid_size: f64,
    ask_size: f64,
    timestamp_ms: u64,
) {
    let observed_at_ms = now_ms();
    let top_of_book = TopOfBook {
        best_bid,
        best_ask,
        bid_size,
        ask_size,
        timestamp: timestamp_ms,
        observed_at_ms,
    };

    if asset_id == up_token {
        book_state.up = Some(top_of_book);
    } else if asset_id == down_token {
        book_state.down = Some(top_of_book);
    }
}

/// Emit one full-book snapshot message after state mutation.
#[allow(clippy::too_many_arguments)]
async fn send_clob_book_snapshot(
    tx: &mpsc::Sender<FeedMessage>,
    connection_id: &str,
    raw_text: &str,
    book_state: &BookState,
    asset_id: String,
    best_bid: f64,
    best_ask: f64,
    timestamp_ms: u64,
    retain_payloads: bool,
) -> anyhow::Result<()> {
    tx.send(FeedMessage::ClobBook {
        book_state: book_state.clone(),
        timestamp_ms,
        timestamp_us: None,
        asset_id: Some(asset_id),
        source_topic: Some("market".to_string()),
        connection_id: connection_id.to_string(),
        payload_json: retain_payloads.then(|| raw_text.to_string()),
        details_json: retain_payloads.then(|| {
            serde_json::json!({
                "bestBid": best_bid,
                "bestAsk": best_ask,
            })
            .to_string()
        }),
    })
    .await
    .map_err(|_| anyhow::anyhow!("channel closed"))
}

/// Apply a direct best-bid-ask update onto the current binary book state.
fn apply_direct_tob(
    book_state: &mut BookState,
    up_token: &str,
    down_token: &str,
    update: &DirectTopOfBookUpdate<'_>,
) {
    let observed_at_ms = now_ms();
    let target = if update.asset_id == up_token {
        &mut book_state.up
    } else if update.asset_id == down_token {
        &mut book_state.down
    } else {
        return;
    };

    let book = target.get_or_insert(TopOfBook {
        best_bid: 0.0,
        best_ask: 0.0,
        bid_size: 0.0,
        ask_size: 0.0,
        timestamp: update.timestamp_ms,
        observed_at_ms,
    });

    if let Some(best_bid) = update.best_bid {
        book.best_bid = best_bid;
        if best_bid <= 0.0 {
            book.bid_size = 0.0;
        }
    }
    if let Some(best_ask) = update.best_ask {
        book.best_ask = best_ask;
        if best_ask <= 0.0 {
            book.ask_size = 0.0;
        }
    }
    if let Some(bid_size) = update.bid_size {
        book.bid_size = bid_size;
    }
    if let Some(ask_size) = update.ask_size {
        book.ask_size = ask_size;
    }
    book.timestamp = update.timestamp_ms;
    book.observed_at_ms = observed_at_ms;
}

/// Return the merged live sizes for the asset targeted by one direct update.
fn merged_side_sizes(
    book_state: &BookState,
    up_token: &str,
    down_token: &str,
    asset_id: &str,
) -> (f64, f64) {
    let book = if asset_id == up_token {
        book_state.up.as_ref()
    } else if asset_id == down_token {
        book_state.down.as_ref()
    } else {
        None
    };
    book.map_or((0.0, 0.0), |book| (book.bid_size, book.ask_size))
}

/// Normalize a `CLOB` source timestamp into millisecond and optional
/// microsecond representations.
fn normalize_source_timestamp(raw: u64) -> (u64, Option<u64>) {
    if raw >= 10_000_000_000_000 {
        (raw / 1_000, Some(raw))
    } else {
        (raw, None)
    }
}

/// Extract the best (highest bid or lowest ask) price and size from a levels array.
///
/// Returns `(price, size)`.  If the array is empty or missing, returns `(0.0, 0.0)`.
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

/// Parse a `JSON` field that may be either a number or a string-encoded number.
pub(crate) fn parse_f64_field(v: &serde_json::Value, field: &str) -> Option<f64> {
    v.get(field).and_then(|val| {
        val.as_f64()
            .or_else(|| val.as_str().and_then(|s| s.parse::<f64>().ok()))
    })
}

#[cfg(test)]
#[path = "tests/clob_feed_tests.rs"]
mod tests;
