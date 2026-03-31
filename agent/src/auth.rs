use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

/// Shared-secret authentication middleware.
///
/// Compares the `Authorization: Bearer <token>` header against the configured secret.
/// Returns 401 if missing or mismatched. The `/health` endpoint is exempt.
pub async fn require_secret(
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let expected = request
        .extensions()
        .get::<SharedSecret>()
        .map(|s| s.0.clone())
        .unwrap_or_default();

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let provided = auth_header.strip_prefix("Bearer ").unwrap_or("");

    if provided.is_empty() || provided != expected {
        return Err((
            StatusCode::UNAUTHORIZED,
            r#"{"error":"unauthorized"}"#.to_string(),
        ));
    }

    Ok(next.run(request).await)
}

/// Wrapper type to store the shared secret in axum extensions.
#[derive(Clone, Debug)]
pub struct SharedSecret(pub String);

#[cfg(test)]
#[path = "tests/auth_tests.rs"]
mod tests;
