use std::sync::Arc;

use axum::Extension;
use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;

use crate::auth::{Claims, create_jwt, hash_password, verify_password};
use crate::db::DashboardDb;
use crate::error::DashboardError;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DashboardDb>,
    pub jwt_secret: String,
    pub agents: Vec<crate::config::AgentConfig>,
}

#[derive(serde::Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(serde::Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(serde::Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: String,
}

/// `POST /api/auth/login`
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    let user = state
        .db
        .get_user_by_username(&req.username)
        .await?
        .ok_or_else(|| DashboardError::Unauthorized("invalid credentials".to_string()))?;

    if !verify_password(&req.password, &user.password_hash) {
        return Err(DashboardError::Unauthorized(
            "invalid credentials".to_string(),
        ));
    }

    let token = create_jwt(&user.id, &user.role, &state.jwt_secret, 86400);

    Ok(Json(LoginResponse {
        token,
        user: UserInfo {
            id: user.id,
            username: user.username,
            role: user.role,
        },
    }))
}

/// `GET /api/auth/me`
pub async fn me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, DashboardError> {
    let user = state
        .db
        .get_user_by_id(&claims.sub)
        .await?
        .ok_or_else(|| DashboardError::NotFound("user not found".to_string()))?;

    Ok(Json(UserInfo {
        id: user.id,
        username: user.username,
        role: user.role,
    }))
}

/// `POST /api/users` (admin only)
#[derive(serde::Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    #[serde(default = "default_role")]
    pub role: String,
}

/// Default role.
fn default_role() -> String {
    "observer".to_string()
}

/// Creates user.
pub async fn create_user(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    if claims.role != "admin" {
        return Err(DashboardError::Forbidden("admin role required".to_string()));
    }

    if req.role != "admin" && req.role != "observer" {
        return Err(DashboardError::BadRequest(
            "role must be 'admin' or 'observer'".to_string(),
        ));
    }

    let password_hash =
        hash_password(&req.password).map_err(|e| DashboardError::Internal(e.clone()))?;

    let user = state
        .db
        .create_user(&req.username, &password_hash, &req.role)
        .await?;

    Ok(Json(UserInfo {
        id: user.id,
        username: user.username,
        role: user.role,
    }))
}

/// `GET /api/users` (admin only)
pub async fn list_users(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, DashboardError> {
    if claims.role != "admin" {
        return Err(DashboardError::Forbidden("admin role required".to_string()));
    }

    let users = state.db.list_users().await?;
    let infos: Vec<UserInfo> = users
        .into_iter()
        .map(|u| UserInfo {
            id: u.id,
            username: u.username,
            role: u.role,
        })
        .collect();

    Ok(Json(infos))
}

#[cfg(test)]
#[path = "../tests/api_auth_tests.rs"]
mod tests;
