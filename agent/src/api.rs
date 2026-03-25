use std::sync::Arc;

use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Json};
use tokio::sync::broadcast;

use crate::db_reader::DbReader;
use crate::error::AgentError;
use crate::process_manager::ProcessManager;
use crate::types::WsMessage;
use crate::ws;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DbReader>,
    pub bot: Arc<dyn ProcessManager>,
    pub ws_tx: broadcast::Sender<WsMessage>,
}

// -- Health (no auth) --------------------------------------------------------

#[allow(clippy::unused_async)]
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

// -- Status ------------------------------------------------------------------

pub async fn get_status(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.db.get_status().await?;
    Ok(Json(status))
}

// -- Trades ------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
pub struct TradesQuery {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_per_page")]
    pub per_page: u64,
}

fn default_page() -> u64 {
    1
}
fn default_per_page() -> u64 {
    50
}

pub async fn get_trades(
    State(state): State<AppState>,
    Query(q): Query<TradesQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let trades = state.db.get_trades(q.page, q.per_page).await?;
    Ok(Json(trades))
}

// -- Balance -----------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
pub struct BalanceQuery {
    #[serde(default)]
    pub since: u64,
}

pub async fn get_balance(
    State(state): State<AppState>,
    Query(q): Query<BalanceQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let balance = state.db.get_balance_log(q.since).await?;
    Ok(Json(balance))
}

// -- Signals -----------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
pub struct SignalsQuery {
    #[serde(default = "default_signals_limit")]
    pub limit: u64,
}

fn default_signals_limit() -> u64 {
    100
}

pub async fn get_signals(
    State(state): State<AppState>,
    Query(q): Query<SignalsQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let signals = state.db.get_signals(q.limit).await?;
    Ok(Json(signals))
}

// -- Stats -------------------------------------------------------------------

pub async fn get_stats(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let stats = state.db.get_stats().await?;
    Ok(Json(stats))
}

// -- Bot logs ----------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
pub struct LogsQuery {
    #[serde(default = "default_log_lines")]
    pub lines: u64,
}

fn default_log_lines() -> u64 {
    200
}

pub async fn get_logs(
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let lines = state.bot.logs(q.lines).await?;
    Ok(Json(crate::types::LogsResponse { lines }))
}

// -- Bot control -------------------------------------------------------------

pub async fn bot_start(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.bot.start().await?;
    Ok(Json(serde_json::json!({ "ok": true, "status": status })))
}

pub async fn bot_stop(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.bot.stop().await?;
    Ok(Json(serde_json::json!({ "ok": true, "status": status })))
}

pub async fn bot_restart(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.bot.restart().await?;
    Ok(Json(serde_json::json!({ "ok": true, "status": status })))
}

pub async fn bot_status(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.bot.status().await?;
    Ok(Json(status))
}

// -- WebSocket ---------------------------------------------------------------

#[allow(clippy::unused_async)]
pub async fn ws_live(
    State(state): State<AppState>,
    ws_upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    let rx = state.ws_tx.subscribe();
    ws_upgrade.on_upgrade(move |socket| ws::handle_ws(socket, rx))
}

#[cfg(test)]
#[path = "tests/api_tests.rs"]
mod tests;
