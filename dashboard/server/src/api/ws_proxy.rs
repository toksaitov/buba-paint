use axum::Extension;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite;

use crate::auth::{AuthState, validate_jwt};
use crate::error::DashboardError;

use super::auth_routes::AppState;

#[derive(serde::Deserialize)]
pub struct WsQuery {
    token: Option<String>,
}

/// `GET /ws/bots/:id` — `WebSocket` proxy to agent.
///
/// Authenticates via `?token=<jwt>` query parameter, connects to the agent's
/// `/ws/live` endpoint, and bridges the two `WebSocket` connections.
#[allow(clippy::unused_async)]
pub async fn ws_proxy(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(query): Query<WsQuery>,
    Extension(auth_state): Extension<AuthState>,
    ws: WebSocketUpgrade,
) -> Result<Response, DashboardError> {
    let token = query
        .token
        .ok_or_else(|| DashboardError::Unauthorized("missing token".into()))?;
    validate_jwt(&token, &auth_state.jwt_secret)
        .map_err(|e| DashboardError::Unauthorized(e.clone()))?;

    let agent = state
        .agents
        .iter()
        .find(|a| a.id == bot_id)
        .ok_or_else(|| DashboardError::NotFound(format!("bot '{bot_id}' not found")))?;

    let agent_ws_url = agent
        .url
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let ws_url = format!("{agent_ws_url}/ws/live");
    let secret = agent.secret.clone();

    Ok(ws.on_upgrade(move |client_socket| proxy_websocket(client_socket, ws_url, secret)))
}

/// Proxy websocket.
async fn proxy_websocket(client_socket: WebSocket, agent_url: String, secret: String) {
    let host = agent_url
        .parse::<axum::http::Uri>()
        .ok()
        .and_then(|uri| {
            let h = uri.host()?.to_string();
            match uri.port_u16() {
                Some(p) => Some(format!("{h}:{p}")),
                None => Some(h),
            }
        })
        .unwrap_or_default();

    let request = match axum::http::Request::builder()
        .uri(&agent_url)
        .header("Host", &host)
        .header("Authorization", format!("Bearer {secret}"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .body(())
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("failed to build agent WS request: {e}");
            return;
        }
    };

    let (agent_ws, _) = match tokio_tungstenite::connect_async(request).await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::warn!("failed to connect to agent WebSocket at {agent_url}: {e}");
            return;
        }
    };

    let (mut agent_sink, mut agent_stream) = agent_ws.split();
    let (mut client_sink, mut client_stream) = client_socket.split();

    let agent_to_client = async {
        while let Some(msg) = agent_stream.next().await {
            match msg {
                Ok(tungstenite::Message::Text(text)) => {
                    if client_sink
                        .send(Message::Text(text.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(tungstenite::Message::Ping(data)) => {
                    let _ = client_sink.send(Message::Ping(data)).await;
                }
                Ok(tungstenite::Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
    };

    let client_to_agent = async {
        while let Some(msg) = client_stream.next().await {
            match msg {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(Message::Ping(data)) => {
                    let _ = agent_sink.send(tungstenite::Message::Ping(data)).await;
                }
                Ok(Message::Pong(data)) => {
                    let _ = agent_sink.send(tungstenite::Message::Pong(data)).await;
                }
                _ => {}
            }
        }
    };

    tokio::select! {
        () = agent_to_client => {},
        () = client_to_agent => {},
    }
}

#[cfg(test)]
#[path = "../tests/ws_proxy_tests.rs"]
mod tests;
