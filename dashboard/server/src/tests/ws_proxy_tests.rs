use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use rusqlite::Connection;

use crate::api::auth_routes::AppState;
use crate::api::ws_proxy;
use crate::auth::{self, AuthState, hash_password};
use crate::config::AgentConfig;
use crate::db::DashboardDb;

/// Test agent.
fn test_agent(url: &str) -> AgentConfig {
    AgentConfig {
        id: "paint".into(),
        name: "Paint".into(),
        url: url.into(),
        secret: "agent-secret".into(),
    }
}

/// Spawns dashboard.
async fn spawn_dashboard(agent_url: &str) -> (String, Arc<DashboardDb>) {
    let db = Arc::new(DashboardDb::from_connection(
        Connection::open_in_memory().unwrap(),
    ));

    let state = AppState {
        db: Arc::clone(&db),
        jwt_secret: "test-jwt-secret".to_string(),
        agents: vec![test_agent(agent_url)],
    };

    let auth_state = AuthState {
        jwt_secret: "test-jwt-secret".to_string(),
        db: Arc::clone(&db),
    };

    let app = Router::new()
        .route("/ws/bots/{id}", get(ws_proxy::ws_proxy))
        .layer(axum::Extension(auth_state))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    (format!("127.0.0.1:{}", addr.port()), db)
}

/// Spawns dashboard no agents.
async fn spawn_dashboard_no_agents() -> (String, Arc<DashboardDb>) {
    let db = Arc::new(DashboardDb::from_connection(
        Connection::open_in_memory().unwrap(),
    ));

    let state = AppState {
        db: Arc::clone(&db),
        jwt_secret: "test-jwt-secret".to_string(),
        agents: vec![],
    };

    let auth_state = AuthState {
        jwt_secret: "test-jwt-secret".to_string(),
        db: Arc::clone(&db),
    };

    let app = Router::new()
        .route("/ws/bots/{id}", get(ws_proxy::ws_proxy))
        .layer(axum::Extension(auth_state))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    (format!("127.0.0.1:{}", addr.port()), db)
}

/// Admin token.
async fn admin_token(db: &DashboardDb) -> String {
    let hash = hash_password("pass").unwrap();
    db.create_user("admin", &hash, "admin").await.unwrap();
    let user = db.get_user_by_username("admin").await.unwrap().unwrap();
    auth::create_jwt(&user.id, "admin", "test-jwt-secret", 3600)
}

/// Verifies that ws proxy rejects missing token.
#[tokio::test]
async fn ws_proxy_rejects_missing_token() {
    let (addr, _db) = spawn_dashboard("http://127.0.0.1:1").await;

    let result = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/bots/paint")).await;
    assert!(result.is_err(), "should reject missing token");
}

/// Verifies that ws proxy rejects invalid jwt.
#[tokio::test]
async fn ws_proxy_rejects_invalid_jwt() {
    let (addr, _db) = spawn_dashboard("http://127.0.0.1:1").await;

    let result =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/bots/paint?token=bad-jwt")).await;
    assert!(result.is_err(), "should reject invalid JWT");
}

/// Verifies that ws proxy rejects unknown bot id.
#[tokio::test]
async fn ws_proxy_rejects_unknown_bot_id() {
    let (addr, db) = spawn_dashboard_no_agents().await;
    let token = admin_token(&db).await;

    let result =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/bots/unknown?token={token}"))
            .await;
    assert!(result.is_err(), "should reject unknown bot ID");
}

/// Spawn a mock agent WS endpoint using axum (same framework as the proxy uses to connect).
async fn spawn_mock_agent_axum() -> String {
    use axum::extract::ws::WebSocketUpgrade;
    use axum::response::Response;

    /// Ws handler.
    async fn ws_handler(ws: WebSocketUpgrade) -> Response {
        ws.on_upgrade(|mut socket| async move {
            use axum::extract::ws::Message;
            let _ = socket
                .send(Message::Text(
                    r#"{"type":"balance","data":{"balance":200}}"#.into(),
                ))
                .await;

            while let Some(Ok(msg)) = futures_util::StreamExt::next(&mut socket).await {
                if matches!(msg, Message::Close(_)) {
                    break;
                }
            }
        })
    }

    let app = Router::new().route("/ws/live", get(ws_handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    format!("http://127.0.0.1:{}", addr.port())
}

/// Verifies that ws proxy accepts valid connection.
#[tokio::test]
async fn ws_proxy_accepts_valid_connection() {
    let agent_url = spawn_mock_agent_axum().await;
    let (addr, db) = spawn_dashboard(&agent_url).await;
    let token = admin_token(&db).await;

    let result = tokio::time::timeout(
        Duration::from_millis(3000),
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/bots/paint?token={token}")),
    )
    .await;

    assert!(result.is_ok(), "WS connection should not time out");
    assert!(
        result.unwrap().is_ok(),
        "WS connection should succeed with valid token"
    );
}

/// Verifies that ws proxy forwards agent messages.
#[tokio::test]
async fn ws_proxy_forwards_agent_messages() {
    let agent_url = spawn_mock_agent_axum().await;
    let (addr, db) = spawn_dashboard(&agent_url).await;
    let token = admin_token(&db).await;

    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/bots/paint?token={token}"))
            .await
            .unwrap();

    use futures_util::StreamExt;
    let mut found = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_millis(3000), ws.next()).await {
            Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text)))) => {
                let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(parsed["type"], "balance");
                found = true;
                break;
            }
            Ok(Some(Ok(_))) => continue,
            _ => break,
        }
    }
    assert!(found, "expected a balance text message from proxy");
}

/// Verify that the proxy's connect_async can talk to a mock agent
/// (this isolates the proxy → agent connection from the client → proxy path).
#[tokio::test]
async fn proxy_connect_async_works_with_mock_agent() {
    let agent_url = spawn_mock_agent_axum().await;
    let ws_url = agent_url.replace("http://", "ws://") + "/ws/live";

    let uri: axum::http::Uri = ws_url.parse().unwrap();
    let host = format!("{}:{}", uri.host().unwrap(), uri.port_u16().unwrap());

    let request = axum::http::Request::builder()
        .uri(&ws_url)
        .header("Host", &host)
        .header("Authorization", "Bearer agent-secret")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();

    let result = tokio::time::timeout(
        Duration::from_millis(3000),
        tokio_tungstenite::connect_async(request),
    )
    .await;

    match result {
        Ok(Ok((mut ws, _))) => {
            use futures_util::StreamExt;
            if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) = ws.next().await {
                let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(parsed["type"], "balance");
            } else {
                panic!("expected text message from mock agent");
            }
        }
        Ok(Err(e)) => panic!("connect_async failed: {e}"),
        Err(_) => panic!("connect_async timed out"),
    }
}
