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

/// Returns a simple liveness payload for probes and smoke tests.
#[allow(clippy::unused_async)]
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

/// Returns the latest bot status snapshot from the run database.
pub async fn get_status(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.db.get_status().await?;
    Ok(Json(status))
}

#[derive(Debug, serde::Deserialize)]
pub struct TradesQuery {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_per_page")]
    pub per_page: u64,
}

/// Returns the default page number for trade pagination.
fn default_page() -> u64 {
    1
}

/// Returns the default page size for trade pagination.
fn default_per_page() -> u64 {
    50
}

/// Returns one page of recorded trades.
pub async fn get_trades(
    State(state): State<AppState>,
    Query(q): Query<TradesQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let trades = state.db.get_trades(q.page, q.per_page).await?;
    Ok(Json(trades))
}

#[derive(Debug, serde::Deserialize)]
pub struct BalanceQuery {
    #[serde(default)]
    pub since: u64,
}

/// Returns the balance history after the requested timestamp.
pub async fn get_balance(
    State(state): State<AppState>,
    Query(q): Query<BalanceQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let balance = state.db.get_balance_log(q.since).await?;
    Ok(Json(balance))
}

#[derive(Debug, serde::Deserialize)]
pub struct SignalsQuery {
    #[serde(default = "default_signals_limit")]
    pub limit: u64,
}

/// Returns the default signal fetch limit.
fn default_signals_limit() -> u64 {
    100
}

/// Returns the most recent recorded signals.
pub async fn get_signals(
    State(state): State<AppState>,
    Query(q): Query<SignalsQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let signals = state.db.get_signals(q.limit).await?;
    Ok(Json(signals))
}

/// Returns aggregate strategy statistics for the current run.
pub async fn get_stats(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let stats = state.db.get_stats().await?;
    Ok(Json(stats))
}

/// Returns the current live-trading readiness/status summary from the additive live tables.
pub async fn get_live_status(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AgentError> {
    let status = state.db.get_live_status().await?;
    Ok(Json(status))
}

#[derive(Debug, serde::Deserialize)]
pub struct LiveLimitQuery {
    #[serde(default = "default_live_limit")]
    pub limit: u64,
}

/// Returns the default row limit for live-table queries.
fn default_live_limit() -> u64 {
    50
}

/// Returns recent live sessions.
pub async fn get_live_sessions(
    State(state): State<AppState>,
    Query(q): Query<LiveLimitQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let sessions = state.db.get_live_sessions(q.limit).await?;
    Ok(Json(sessions))
}

/// Returns recent live orders.
pub async fn get_live_orders(
    State(state): State<AppState>,
    Query(q): Query<LiveLimitQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let orders = state.db.get_live_orders(q.limit).await?;
    Ok(Json(orders))
}

/// Returns recent live fills.
pub async fn get_live_fills(
    State(state): State<AppState>,
    Query(q): Query<LiveLimitQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let fills = state.db.get_live_fills(q.limit).await?;
    Ok(Json(fills))
}

/// Returns recent live redemptions.
pub async fn get_live_redemptions(
    State(state): State<AppState>,
    Query(q): Query<LiveLimitQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let redemptions = state.db.get_live_redemptions(q.limit).await?;
    Ok(Json(redemptions))
}

/// Returns recent live reconciliation events.
pub async fn get_live_reconciliation(
    State(state): State<AppState>,
    Query(q): Query<LiveLimitQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let events = state.db.get_live_reconciliation(q.limit).await?;
    Ok(Json(events))
}

#[derive(Debug, serde::Deserialize)]
pub struct LogsQuery {
    #[serde(default = "default_log_lines")]
    pub lines: u64,
}

/// Returns the default log tail length.
fn default_log_lines() -> u64 {
    200
}

/// Returns the most recent bot log lines from the process manager.
pub async fn get_logs(
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let lines = state.bot.logs(q.lines).await?;
    Ok(Json(crate::types::LogsResponse { lines }))
}

/// Starts the managed bot process and returns its updated status.
pub async fn bot_start(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.bot.start().await?;
    Ok(Json(serde_json::json!({ "ok": true, "status": status })))
}

/// Stops the managed bot process and returns its updated status.
pub async fn bot_stop(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.bot.stop().await?;
    Ok(Json(serde_json::json!({ "ok": true, "status": status })))
}

/// Restarts the managed bot process and returns its updated status.
pub async fn bot_restart(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.bot.restart().await?;
    Ok(Json(serde_json::json!({ "ok": true, "status": status })))
}

/// Returns the current process-manager view of the bot status.
pub async fn bot_status(State(state): State<AppState>) -> Result<impl IntoResponse, AgentError> {
    let status = state.bot.status().await?;
    Ok(Json(status))
}

/// Upgrades the client connection and streams live agent updates over `WebSocket`.
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
