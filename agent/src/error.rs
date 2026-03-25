use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("bot control failed: {0}")]
    BotControl(String),

    #[error("bot control unavailable: {0}")]
    BotControlUnavailable(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AgentError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::BotControl(msg) | Self::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg.clone())
            }
            Self::BotControlUnavailable(msg) => (StatusCode::CONFLICT, msg.clone()),
        };

        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
#[path = "tests/error_tests.rs"]
mod tests;
