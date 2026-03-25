use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::db_reader::DbReader;
use crate::types::WsMessage;

/// Spawn the database polling loop that detects new rows and broadcasts them.
pub fn spawn_poller(db: Arc<DbReader>, poll_interval_ms: u64, tx: broadcast::Sender<WsMessage>) {
    tokio::spawn(async move {
        let mut last_trade_id = db.get_latest_trade_id().await.unwrap_or(0);
        let mut last_balance_id = db.get_latest_balance_id().await.unwrap_or(0);
        let mut last_signal_id = db.get_latest_signal_id().await.unwrap_or(0);

        let interval = Duration::from_millis(poll_interval_ms);

        loop {
            tokio::time::sleep(interval).await;

            // Check for new trades
            match db.get_trades_since(last_trade_id).await {
                Ok(trades) => {
                    for trade in trades {
                        last_trade_id = last_trade_id.max(trade.id);
                        let _ = tx.send(WsMessage::Trade(trade));
                    }
                }
                Err(e) => warn!("poll trades error: {e}"),
            }

            // Check for new balance entries
            match db.get_balance_since(last_balance_id).await {
                Ok(entries) => {
                    for entry in entries {
                        last_balance_id = last_balance_id.max(entry.id);
                        let _ = tx.send(WsMessage::Balance(entry));
                    }
                }
                Err(e) => warn!("poll balance error: {e}"),
            }

            // Check for new signals
            match db.get_signals_since(last_signal_id).await {
                Ok(signals) => {
                    for signal in signals {
                        last_signal_id = last_signal_id.max(signal.id);
                        let _ = tx.send(WsMessage::Signal(signal));
                    }
                }
                Err(e) => warn!("poll signals error: {e}"),
            }
        }
    });
}

/// Handle a single WebSocket connection: subscribe to broadcast and forward messages.
pub async fn handle_ws(mut socket: WebSocket, mut rx: broadcast::Receiver<WsMessage>) {
    debug!("WebSocket client connected");

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(ws_msg) => {
                        let json = match serde_json::to_string(&ws_msg) {
                            Ok(j) => j,
                            Err(e) => {
                                warn!("failed to serialize WS message: {e}");
                                continue;
                            }
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break; // client disconnected
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        debug!("WS client lagged by {n} messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    _ => {} // ignore text/binary from client
                }
            }
        }
    }

    debug!("WebSocket client disconnected");
}

#[cfg(test)]
#[path = "tests/ws_tests.rs"]
mod tests;
