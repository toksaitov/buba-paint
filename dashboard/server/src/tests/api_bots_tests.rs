use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::{get, post};
use rusqlite::Connection;
use tower::ServiceExt;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::api::auth_routes::AppState;
use crate::api::bots;
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

/// Build a test app with one agent pointing to the given URL.
fn test_app_with_agent(agent_url: &str) -> (Router, Arc<DashboardDb>) {
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
        .route("/api/bots", get(bots::list_bots))
        .route("/api/bots/{id}/status", get(bots::bot_status))
        .route("/api/bots/{id}/trades", get(bots::bot_trades))
        .route("/api/bots/{id}/balance", get(bots::bot_balance))
        .route("/api/bots/{id}/signals", get(bots::bot_signals))
        .route("/api/bots/{id}/stats", get(bots::bot_stats))
        .route("/api/bots/{id}/live/status", get(bots::bot_live_status))
        .route("/api/bots/{id}/live/sessions", get(bots::bot_live_sessions))
        .route("/api/bots/{id}/live/orders", get(bots::bot_live_orders))
        .route("/api/bots/{id}/live/fills", get(bots::bot_live_fills))
        .route(
            "/api/bots/{id}/live/redemptions",
            get(bots::bot_live_redemptions),
        )
        .route(
            "/api/bots/{id}/live/reconciliation",
            get(bots::bot_live_reconciliation),
        )
        .route("/api/bots/{id}/logs", get(bots::bot_logs))
        .route("/api/bots/{id}/process", get(bots::bot_process_status))
        .route("/api/bots/{id}/start", post(bots::bot_start))
        .route("/api/bots/{id}/stop", post(bots::bot_stop))
        .route("/api/bots/{id}/restart", post(bots::bot_restart))
        .layer(middleware::from_fn(auth::require_auth))
        .layer(axum::Extension(auth_state))
        .with_state(state);

    (app, db)
}

/// Build a test app with no agents.
fn test_app_no_agents() -> (Router, Arc<DashboardDb>) {
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
        .route("/api/bots", get(bots::list_bots))
        .route("/api/bots/{id}/status", get(bots::bot_status))
        .layer(middleware::from_fn(auth::require_auth))
        .layer(axum::Extension(auth_state))
        .with_state(state);

    (app, db)
}

/// Admin token.
async fn admin_token(db: &DashboardDb) -> String {
    let hash = hash_password("pass").unwrap();
    db.create_user("admin", &hash, "admin").await.unwrap();
    let user = db.get_user_by_username("admin").await.unwrap().unwrap();
    auth::create_jwt(&user.id, "admin", "test-jwt-secret", 3600)
}

/// Auth get.
fn auth_get(path: &str, token: &str) -> Request<Body> {
    Request::get(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Auth post.
fn auth_post(path: &str, token: &str) -> Request<Body> {
    Request::post(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Json body.
async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Verifies that list bots returns configured agents.
#[tokio::test]
async fn list_bots_returns_configured_agents() {
    let server = MockServer::start().await;
    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app.oneshot(auth_get("/api/bots", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    let bots = body["bots"].as_array().unwrap();
    assert_eq!(bots.len(), 1);
    assert_eq!(bots[0]["id"], "paint");
    assert_eq!(bots[0]["name"], "Paint");
}

/// Verifies that list bots empty when no agents.
#[tokio::test]
async fn list_bots_empty_when_no_agents() {
    let (app, db) = test_app_no_agents();
    let token = admin_token(&db).await;

    let resp = app.oneshot(auth_get("/api/bots", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["bots"].as_array().unwrap().len(), 0);
}

/// Verifies that bot status proxies to agent.
#[tokio::test]
async fn bot_status_proxies_to_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "balance": 500.0,
            "open_trades": 2,
        })))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/paint/status", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["balance"], 500.0);
    assert_eq!(body["open_trades"], 2);
}

/// Verifies that bot status unknown id returns 404.
#[tokio::test]
async fn bot_status_unknown_id_returns_404() {
    let server = MockServer::start().await;
    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/unknown/status", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// Verifies that bot trades forwards query params.
#[tokio::test]
async fn bot_trades_forwards_query_params() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/trades"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "10"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"trades": [], "total": 0})),
        )
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get(
            "/api/bots/paint/trades?page=2&per_page=10",
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that bot balance forwards since param.
#[tokio::test]
async fn bot_balance_forwards_since_param() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/balance"))
        .and(query_param("since", "5000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"entries": []})))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/paint/balance?since=5000", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that bot signals forwards limit param.
#[tokio::test]
async fn bot_signals_forwards_limit_param() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/signals"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"signals": []})))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/paint/signals?limit=50", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that bot stats proxies to agent.
#[tokio::test]
async fn bot_stats_proxies_to_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/stats"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"strategies": []})),
        )
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/paint/stats", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that live status proxies to the agent.
#[tokio::test]
async fn bot_live_status_proxies_to_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/live/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "latest_session": { "id": 1 },
            "open_orders": 2,
            "pending_redemptions": 1,
            "critical_reconciliation_events": 1,
        })))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/paint/live/status", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["open_orders"], 2);
}

/// Verifies that live list proxies forward the limit query parameter.
#[tokio::test]
async fn bot_live_list_proxies_forward_limit() {
    let server = MockServer::start().await;
    for live_path in [
        "/api/live/sessions",
        "/api/live/orders",
        "/api/live/fills",
        "/api/live/redemptions",
        "/api/live/reconciliation",
    ] {
        Mock::given(method("GET"))
            .and(path(live_path))
            .and(query_param("limit", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;
    }

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    for path in [
        "/api/bots/paint/live/sessions?limit=10",
        "/api/bots/paint/live/orders?limit=10",
        "/api/bots/paint/live/fills?limit=10",
        "/api/bots/paint/live/redemptions?limit=10",
        "/api/bots/paint/live/reconciliation?limit=10",
    ] {
        let resp = app.clone().oneshot(auth_get(path, &token)).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "unexpected status for {path}"
        );
    }
}

/// Verifies that bot logs forwards lines param.
#[tokio::test]
async fn bot_logs_forwards_lines_param() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/bot/logs"))
        .and(query_param("lines", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"lines": []})))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/paint/logs?lines=100", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that bot process status proxies to agent.
#[tokio::test]
async fn bot_process_status_proxies_to_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/bot/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "active": true,
            "pid": 1234,
            "uptime_secs": 300,
        })))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/paint/process", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["active"], true);
    assert_eq!(body["pid"], 1234);
}

/// Verifies that bot start posts to agent.
#[tokio::test]
async fn bot_start_posts_to_agent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/bot/start"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "active": true, "pid": 42, "uptime_secs": 0,
        })))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_post("/api/bots/paint/start", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that bot stop posts to agent.
#[tokio::test]
async fn bot_stop_posts_to_agent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/bot/stop"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "active": false, "pid": null, "uptime_secs": null,
        })))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_post("/api/bots/paint/stop", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that bot restart posts to agent.
#[tokio::test]
async fn bot_restart_posts_to_agent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/bot/restart"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "active": true, "pid": 99, "uptime_secs": 0,
        })))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_post("/api/bots/paint/restart", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that bot handler propagates agent 409.
#[tokio::test]
async fn bot_handler_propagates_agent_409() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/bot/start"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": "monitor-only mode"
        })))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_post("/api/bots/paint/start", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

/// Verifies that bot handler propagates agent 500.
#[tokio::test]
async fn bot_handler_propagates_agent_500() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": "db failure"
        })))
        .mount(&server)
        .await;

    let (app, db) = test_app_with_agent(&server.uri());
    let token = admin_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/bots/paint/status", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

/// Verifies that bot handler requires auth.
#[tokio::test]
async fn bot_handler_requires_auth() {
    let server = MockServer::start().await;
    let (app, _db) = test_app_with_agent(&server.uri());

    let resp = app
        .oneshot(
            Request::get("/api/bots/paint/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that build query empty params.
#[test]
fn build_query_empty_params() {
    let params = HashMap::new();
    assert!(super::build_query(&params).is_none());
}

/// Verifies that build query single param.
#[test]
fn build_query_single_param() {
    let mut params = HashMap::new();
    params.insert("page".to_string(), "1".to_string());
    let qs = super::build_query(&params).unwrap();
    assert_eq!(qs, "page=1");
}

/// Verifies that build query multiple params.
#[test]
fn build_query_multiple_params() {
    let mut params = HashMap::new();
    params.insert("page".to_string(), "2".to_string());
    params.insert("per_page".to_string(), "50".to_string());
    let qs = super::build_query(&params).unwrap();

    assert!(qs.contains("page=2"));
    assert!(qs.contains("per_page=50"));
    assert!(qs.contains('&'));
}
