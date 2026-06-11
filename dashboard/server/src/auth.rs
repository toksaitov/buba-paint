use std::sync::Arc;

use argon2::Argon2;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};

use crate::db::DashboardDb;

/// `JWT` claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: String,
    pub exp: u64,
    pub iat: u64,
}

/// Hash a password with argon2.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("failed to hash password: {e}"))?;
    Ok(hash.to_string())
}

/// Verify a password against a hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Create a `JWT` token.
pub fn create_jwt(user_id: &str, role: &str, secret: &str, duration_secs: u64) -> String {
    let now = jsonwebtoken::get_current_timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.to_string(),
        exp: now + duration_secs,
        iat: now,
    };
    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("JWT encoding should not fail")
}

/// Validate a `JWT` token and return claims.
pub fn validate_jwt(token: &str, secret: &str) -> Result<Claims, String> {
    let mut validation = Validation::default();
    validation.leeway = 0;
    jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| format!("invalid token: {e}"))
}

/// Shared state for the auth middleware.
#[derive(Clone)]
pub struct AuthState {
    pub jwt_secret: String,
    pub db: Arc<DashboardDb>,
}

/// `JWT` auth middleware - extracts and validates the Bearer token.
/// Stores `Claims` in request extensions on success.
pub async fn require_auth(mut request: Request, next: Next) -> Result<Response, StatusCode> {
    let path = request.uri().path();
    if path == "/api/auth/login"
        || path == "/health"
        || path.starts_with("/api/research/workers/")
        || path.starts_with("/ws/")
    {
        return Ok(next.run(request).await);
    }

    let auth_state = request
        .extensions()
        .get::<AuthState>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let claims =
        validate_jwt(token, &auth_state.jwt_secret).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let now = jsonwebtoken::get_current_timestamp();
    if claims.exp < now {
        return Err(StatusCode::UNAUTHORIZED);
    }

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

#[cfg(test)]
#[path = "tests/auth_tests.rs"]
mod tests;
