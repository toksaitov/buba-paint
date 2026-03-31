use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum DashboardError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("agent error ({0}): {1}")]
    AgentError(u16, String),

    #[error("proxy error: {0}")]
    Proxy(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for DashboardError {
    /// Into response.
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::AgentError(code, msg) => {
                let status =
                    StatusCode::from_u16(*code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, msg.clone())
            }
            Self::Proxy(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
#[path = "tests/error_tests.rs"]
mod tests;
