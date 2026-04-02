use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get;
use axum::{Extension, Router};
use tower::ServiceExt;

use super::*;

/// Test app.
fn test_app(secret: &str) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/status", get(|| async { "protected" }))
        .layer(middleware::from_fn(require_secret))
        .layer(Extension(SharedSecret(secret.to_string())))
}

/// Verifies that health accessible without auth.
#[tokio::test]
async fn health_accessible_without_auth() {
    let app = test_app("my-secret");
    let resp = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that protected route rejects missing auth.
#[tokio::test]
async fn protected_route_rejects_missing_auth() {
    let app = test_app("my-secret");
    let resp = app
        .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that protected route rejects wrong secret.
#[tokio::test]
async fn protected_route_rejects_wrong_secret() {
    let app = test_app("my-secret");
    let resp = app
        .oneshot(
            Request::get("/api/status")
                .header("authorization", "Bearer wrong-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that protected route accepts correct secret.
#[tokio::test]
async fn protected_route_accepts_correct_secret() {
    let app = test_app("my-secret");
    let resp = app
        .oneshot(
            Request::get("/api/status")
                .header("authorization", "Bearer my-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verifies that rejects bearer prefix missing.
#[tokio::test]
async fn rejects_bearer_prefix_missing() {
    let app = test_app("my-secret");
    let resp = app
        .oneshot(
            Request::get("/api/status")
                .header("authorization", "my-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that empty secret still requires matching.
#[tokio::test]
async fn empty_secret_still_requires_matching() {
    let app = test_app("");
    let resp = app
        .oneshot(
            Request::get("/api/status")
                .header("authorization", "Bearer ")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that missing agent secret is rejected by startup validation.
#[test]
fn required_shared_secret_rejects_missing_value() {
    let error = required_shared_secret(None).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("AGENT_SECRET must be set and non-empty")
    );
}

/// Verifies that blank agent secret is rejected by startup validation.
#[test]
fn required_shared_secret_rejects_blank_value() {
    let error = required_shared_secret(Some("   ".to_string())).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("AGENT_SECRET must be set and non-empty")
    );
}

/// Verifies that non-empty agent secrets are accepted unchanged.
#[test]
fn required_shared_secret_accepts_value() {
    let secret = required_shared_secret(Some("my-secret".to_string())).unwrap();
    assert_eq!(secret.0, "my-secret");
}
