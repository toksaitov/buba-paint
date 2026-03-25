use std::sync::Arc;

use axum::middleware;
use axum::routing::{get, post};
use axum::{Extension, Router};

use buba_dashboard::api::auth_routes::{self, AppState};
use buba_dashboard::api::bots;
use buba_dashboard::auth::{self, AuthState, hash_password};
use buba_dashboard::config::AgentConfig;
use buba_dashboard::db::DashboardDb;

fn test_agent(url: &str) -> AgentConfig {
    AgentConfig {
        id: "paint".into(),
        name: "Paint".into(),
        url: url.into(),
        secret: "agent-secret".into(),
    }
}

async fn spawn_dashboard(agent_url: &str) -> (String, Arc<DashboardDb>) {
    let db = Arc::new(DashboardDb::new(":memory:").unwrap());

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
        .route("/api/auth/login", post(auth_routes::login))
        .route("/api/auth/me", get(auth_routes::me))
        .route("/api/users", post(auth_routes::create_user))
        .route("/api/bots", get(bots::list_bots))
        .route("/api/bots/{id}/status", get(bots::bot_status))
        .layer(middleware::from_fn(auth::require_auth))
        .layer(Extension(auth_state))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    (format!("http://127.0.0.1:{}", addr.port()), db)
}

async fn seed_user(db: &DashboardDb, username: &str, password: &str, role: &str) {
    let hash = hash_password(password).unwrap();
    db.create_user(username, &hash, role).await.unwrap();
}

#[tokio::test]
async fn login_then_list_and_proxy_bots() {
    let agent_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/status"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"balance": 500.0, "open_trades": 1})),
        )
        .mount(&agent_server)
        .await;

    let (base, db) = spawn_dashboard(&agent_server.uri()).await;
    seed_user(&db, "admin", "pass123", "admin").await;

    let client = reqwest::Client::new();

    // Login
    let resp = client
        .post(format!("{base}/api/auth/login"))
        .json(&serde_json::json!({"username": "admin", "password": "pass123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let token = body["token"].as_str().unwrap().to_string();

    // Get me
    let resp = client
        .get(format!("{base}/api/auth/me"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["username"], "admin");

    // List bots
    let resp = client
        .get(format!("{base}/api/bots"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["bots"][0]["id"], "paint");

    // Proxy bot status
    let resp = client
        .get(format!("{base}/api/bots/paint/status"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["balance"], 500.0);
}

#[tokio::test]
async fn observer_cannot_create_users() {
    let (base, db) = spawn_dashboard("http://127.0.0.1:1").await;
    seed_user(&db, "observer", "pass123", "observer").await;

    let client = reqwest::Client::new();

    // Login as observer
    let resp = client
        .post(format!("{base}/api/auth/login"))
        .json(&serde_json::json!({"username": "observer", "password": "pass123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let token = body["token"].as_str().unwrap().to_string();

    // Attempt to create a user — should be forbidden
    let resp = client
        .post(format!("{base}/api/users"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({"username": "new-user", "password": "pass", "role": "admin"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}
