use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::{get, post};
use rusqlite::Connection;
use tower::ServiceExt;

use crate::api::auth_routes::{self, AppState};
use crate::auth::{self, AuthState, hash_password};
use crate::db::DashboardDb;

/// Test app.
fn test_app() -> (Router, Arc<DashboardDb>) {
    let db = Arc::new(DashboardDb::from_connection(
        Connection::open_in_memory().unwrap(),
    ));

    let state = AppState {
        db: Arc::clone(&db),
        jwt_secret: "test-jwt-secret".to_string(),
        research_worker_token: None,
        research_work_root: None,
        agents: vec![],
    };

    let auth_state = AuthState {
        jwt_secret: "test-jwt-secret".to_string(),
        db: Arc::clone(&db),
    };

    let app = Router::new()
        .route("/api/auth/login", post(auth_routes::login))
        .route("/api/auth/me", get(auth_routes::me))
        .route("/api/users", post(auth_routes::create_user))
        .route("/api/users", get(auth_routes::list_users))
        .layer(middleware::from_fn(auth::require_auth))
        .layer(axum::Extension(auth_state))
        .with_state(state);

    (app, db)
}

/// Seed user.
async fn seed_user(db: &DashboardDb, username: &str, password: &str, role: &str) {
    let hash = hash_password(password).unwrap();
    db.create_user(username, &hash, role).await.unwrap();
}

/// Login body.
fn login_body(username: &str, password: &str) -> Body {
    Body::from(
        serde_json::to_string(&serde_json::json!({
            "username": username,
            "password": password,
        }))
        .unwrap(),
    )
}

/// Verifies that login success.
#[tokio::test]
async fn login_success() {
    let (app, db) = test_app();
    seed_user(&db, "admin", "pass123", "admin").await;

    let resp = app
        .oneshot(
            Request::post("/api/auth/login")
                .header("content-type", "application/json")
                .body(login_body("admin", "pass123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!json["token"].as_str().unwrap().is_empty());
    assert_eq!(json["user"]["username"], "admin");
    assert_eq!(json["user"]["role"], "admin");
}

/// Verifies that login wrong password.
#[tokio::test]
async fn login_wrong_password() {
    let (app, db) = test_app();
    seed_user(&db, "admin", "correct", "admin").await;

    let resp = app
        .oneshot(
            Request::post("/api/auth/login")
                .header("content-type", "application/json")
                .body(login_body("admin", "wrong"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that login nonexistent user.
#[tokio::test]
async fn login_nonexistent_user() {
    let (app, _db) = test_app();

    let resp = app
        .oneshot(
            Request::post("/api/auth/login")
                .header("content-type", "application/json")
                .body(login_body("ghost", "pass"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that me with valid token.
#[tokio::test]
async fn me_with_valid_token() {
    let (app, db) = test_app();
    seed_user(&db, "alice", "pass", "observer").await;

    let user = db.get_user_by_username("alice").await.unwrap().unwrap();
    let token = auth::create_jwt(&user.id, &user.role, "test-jwt-secret", 3600);

    let resp = app
        .oneshot(
            Request::get("/api/auth/me")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["username"], "alice");
}

/// Verifies that me without token.
#[tokio::test]
async fn me_without_token() {
    let (app, _db) = test_app();

    let resp = app
        .oneshot(Request::get("/api/auth/me").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that create user as admin.
#[tokio::test]
async fn create_user_as_admin() {
    let (app, db) = test_app();
    seed_user(&db, "admin", "pass", "admin").await;

    let user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let token = auth::create_jwt(&user.id, "admin", "test-jwt-secret", 3600);

    let resp = app
        .oneshot(
            Request::post("/api/users")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "username": "newuser",
                        "password": "newpass",
                        "role": "observer"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["username"], "newuser");
    assert_eq!(json["role"], "observer");
}

/// Verifies that create user as observer forbidden.
#[tokio::test]
async fn create_user_as_observer_forbidden() {
    let (app, db) = test_app();
    seed_user(&db, "viewer", "pass", "observer").await;

    let user = db.get_user_by_username("viewer").await.unwrap().unwrap();
    let token = auth::create_jwt(&user.id, "observer", "test-jwt-secret", 3600);

    let resp = app
        .oneshot(
            Request::post("/api/users")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "username": "hack",
                        "password": "hack",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// Verifies that list users as admin.
#[tokio::test]
async fn list_users_as_admin() {
    let (app, db) = test_app();
    seed_user(&db, "admin", "pass", "admin").await;
    seed_user(&db, "user1", "pass", "observer").await;

    let user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let token = auth::create_jwt(&user.id, "admin", "test-jwt-secret", 3600);

    let resp = app
        .oneshot(
            Request::get("/api/users")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 2);
}

/// Verifies that list users as observer forbidden.
#[tokio::test]
async fn list_users_as_observer_forbidden() {
    let (app, db) = test_app();
    seed_user(&db, "viewer", "pass", "observer").await;

    let user = db.get_user_by_username("viewer").await.unwrap().unwrap();
    let token = auth::create_jwt(&user.id, "observer", "test-jwt-secret", 3600);

    let resp = app
        .oneshot(
            Request::get("/api/users")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
