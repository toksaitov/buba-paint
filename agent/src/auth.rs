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

/// Validates the configured shared secret required by the agent API.
pub fn required_shared_secret(secret: Option<String>) -> anyhow::Result<SharedSecret> {
    let secret = secret.unwrap_or_default();
    if secret.trim().is_empty() {
        anyhow::bail!("AGENT_SECRET must be set and non-empty");
    }
    Ok(SharedSecret(secret))
}

#[cfg(test)]
#[path = "tests/auth_tests.rs"]
mod tests;
