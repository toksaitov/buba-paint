use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;

use crate::api::auth_routes::AppState;
use crate::error::DashboardError;
use crate::proxy;

/// Build a query string from a `HashMap`.
#[allow(clippy::implicit_hasher)]
fn build_query(params: &HashMap<String, String>) -> Option<String> {
    if params.is_empty() {
        return None;
    }
    let qs: String = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    Some(qs)
}

/// `GET /api/bots` — list configured agents.
#[allow(clippy::unused_async)]
pub async fn list_bots(State(state): State<AppState>) -> impl IntoResponse {
    let bots: Vec<serde_json::Value> = state
        .agents
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "name": a.name,
            })
        })
        .collect();

    Json(serde_json::json!({ "bots": bots }))
}

/// `GET /api/bots/:id/status`
pub async fn bot_status(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let data = proxy::proxy_get(agent, "/api/status", None).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/trades`
#[allow(clippy::implicit_hasher)]
pub async fn bot_trades(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/trades", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/balance`
#[allow(clippy::implicit_hasher)]
pub async fn bot_balance(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/balance", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/equity/series`
#[allow(clippy::implicit_hasher)]
pub async fn bot_equity_series(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/equity/series", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/signals`
#[allow(clippy::implicit_hasher)]
pub async fn bot_signals(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/signals", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/signals/groups`
#[allow(clippy::implicit_hasher)]
pub async fn bot_signal_groups(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/signals/groups", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/stats`
pub async fn bot_stats(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let data = proxy::proxy_get(agent, "/api/stats", None).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/trading/summary`
pub async fn bot_trading_summary(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let data = proxy::proxy_get(agent, "/api/trading/summary", None).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/live/status`
pub async fn bot_live_status(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let data = proxy::proxy_get(agent, "/api/live/status", None).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/live/sessions`
#[allow(clippy::implicit_hasher)]
pub async fn bot_live_sessions(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/live/sessions", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/live/orders`
#[allow(clippy::implicit_hasher)]
pub async fn bot_live_orders(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/live/orders", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/live/fills`
#[allow(clippy::implicit_hasher)]
pub async fn bot_live_fills(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/live/fills", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/live/redemptions`
#[allow(clippy::implicit_hasher)]
pub async fn bot_live_redemptions(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/live/redemptions", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/live/reconciliation`
#[allow(clippy::implicit_hasher)]
pub async fn bot_live_reconciliation(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/live/reconciliation", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/logs`
#[allow(clippy::implicit_hasher)]
pub async fn bot_logs(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let qs = build_query(&params);
    let data = proxy::proxy_get(agent, "/api/bot/logs", qs.as_deref()).await?;
    Ok(Json(data))
}

/// `GET /api/bots/:id/process`
pub async fn bot_process_status(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let data = proxy::proxy_get(agent, "/api/bot/status", None).await?;
    Ok(Json(data))
}

/// `POST /api/bots/:id/start`
pub async fn bot_start(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let data = proxy::proxy_post(agent, "/api/bot/start").await?;
    Ok(Json(data))
}

/// `POST /api/bots/:id/stop`
pub async fn bot_stop(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let data = proxy::proxy_post(agent, "/api/bot/stop").await?;
    Ok(Json(data))
}

/// `POST /api/bots/:id/restart`
pub async fn bot_restart(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let agent = find_agent(&state, &bot_id)?;
    let data = proxy::proxy_post(agent, "/api/bot/restart").await?;
    Ok(Json(data))
}

/// Finds agent.
fn find_agent<'a>(
    state: &'a AppState,
    bot_id: &str,
) -> Result<&'a crate::config::AgentConfig, DashboardError> {
    state
        .agents
        .iter()
        .find(|a| a.id == bot_id)
        .ok_or_else(|| DashboardError::NotFound(format!("bot '{bot_id}' not found")))
}

#[cfg(test)]
#[path = "../tests/api_bots_tests.rs"]
mod tests;
