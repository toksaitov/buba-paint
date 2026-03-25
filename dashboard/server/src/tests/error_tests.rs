use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::error::DashboardError;

/// Helper: extract status code and body JSON from a DashboardError response.
async fn error_parts(err: DashboardError) -> (StatusCode, serde_json::Value) {
    let response = err.into_response();
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    (status, json)
}

#[tokio::test]
async fn database_error_returns_500() {
    let err = DashboardError::Database(rusqlite::Error::QueryReturnedNoRows);
    let (status, _) = error_parts(err).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn unauthorized_returns_401() {
    let err = DashboardError::Unauthorized("bad token".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "bad token");
}

#[tokio::test]
async fn forbidden_returns_403() {
    let err = DashboardError::Forbidden("no access".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "no access");
}

#[tokio::test]
async fn not_found_returns_404() {
    let err = DashboardError::NotFound("missing".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "missing");
}

#[tokio::test]
async fn bad_request_returns_400() {
    let err = DashboardError::BadRequest("invalid".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid");
}

#[tokio::test]
async fn agent_error_preserves_status_code() {
    let err = DashboardError::AgentError(409, "conflict".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "conflict");
}

#[tokio::test]
async fn agent_error_invalid_code_falls_back_to_500() {
    // StatusCode::from_u16 accepts 100-999; values outside that range trigger the fallback.
    let err = DashboardError::AgentError(0, "weird".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "weird");
}

#[tokio::test]
async fn proxy_error_returns_502() {
    let err = DashboardError::Proxy("connection refused".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"], "connection refused");
}

#[tokio::test]
async fn internal_returns_500() {
    let err = DashboardError::Internal("something broke".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "something broke");
}

#[tokio::test]
async fn error_body_is_json_with_error_field() {
    // Every variant should produce a JSON body with an "error" key.
    let variants: Vec<DashboardError> = vec![
        DashboardError::Unauthorized("u".into()),
        DashboardError::Forbidden("f".into()),
        DashboardError::NotFound("n".into()),
        DashboardError::BadRequest("b".into()),
        DashboardError::AgentError(418, "teapot".into()),
        DashboardError::Proxy("p".into()),
        DashboardError::Internal("i".into()),
    ];

    for err in variants {
        let (_, body) = error_parts(err).await;
        assert!(
            body.get("error").is_some(),
            "expected 'error' key in response body: {body}"
        );
        assert!(
            body["error"].is_string(),
            "expected 'error' to be a string: {body}"
        );
    }
}
