use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::error::AgentError;

async fn error_parts(err: AgentError) -> (StatusCode, serde_json::Value) {
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
    let err = AgentError::Database(rusqlite::Error::QueryReturnedNoRows);
    let (status, _) = error_parts(err).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn unauthorized_returns_401() {
    let err = AgentError::Unauthorized("bad token".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "bad token");
}

#[tokio::test]
async fn bot_control_returns_500() {
    let err = AgentError::BotControl("spawn failed".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "spawn failed");
}

#[tokio::test]
async fn bot_control_unavailable_returns_409() {
    let err = AgentError::BotControlUnavailable("monitor-only mode".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "monitor-only mode");
}

#[tokio::test]
async fn internal_returns_500() {
    let err = AgentError::Internal("unexpected".into());
    let (status, body) = error_parts(err).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "unexpected");
}

#[tokio::test]
async fn error_body_is_json_with_error_field() {
    let variants: Vec<AgentError> = vec![
        AgentError::Unauthorized("u".into()),
        AgentError::BotControl("b".into()),
        AgentError::BotControlUnavailable("bu".into()),
        AgentError::Internal("i".into()),
    ];

    for err in variants {
        let (_, body) = error_parts(err).await;
        assert!(
            body.get("error").is_some(),
            "expected 'error' key in body: {body}"
        );
        assert!(body["error"].is_string());
    }
}
