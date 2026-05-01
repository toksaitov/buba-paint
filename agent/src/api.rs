use std::sync::Arc;

use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Json};
use tokio::sync::broadcast;

use crate::db_reader::DbReader;
use crate::error::AgentError;
use crate::process_manager::ProcessManager;
use crate::types::{
    RealAccountSummary, ShadowSummary, TradingAlert, TradingCapabilities, TradingControlCapability,
    TradingHealth, TradingSummary, WsMessage,
};
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

/// Returns chart-safe equity data after the requested timestamp.
pub async fn get_equity_series(
    State(state): State<AppState>,
    Query(q): Query<BalanceQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let series = state.db.get_equity_series(q.since).await?;
    Ok(Json(series))
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

#[derive(Debug, serde::Deserialize)]
pub struct SignalGroupsQuery {
    #[serde(default = "default_signal_groups_limit")]
    pub limit: u64,
    #[serde(default = "default_signal_groups_quiet_gap_ms")]
    pub quiet_gap_ms: u64,
    #[serde(default = "default_signal_groups_raw_limit")]
    pub raw_limit: u64,
}

/// Returns the default grouped signal count.
fn default_signal_groups_limit() -> u64 {
    50
}

/// Returns the default quiet gap for grouping adjacent raw signal rows.
fn default_signal_groups_quiet_gap_ms() -> u64 {
    5_000
}

/// Returns the default raw signal scan limit for grouping.
fn default_signal_groups_raw_limit() -> u64 {
    1_000
}

/// Returns recent signal bursts grouped for dashboard operator review.
pub async fn get_signal_groups(
    State(state): State<AppState>,
    Query(q): Query<SignalGroupsQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let groups = state
        .db
        .get_signal_groups(q.limit.min(500), q.quiet_gap_ms, q.raw_limit.min(10_000))
        .await?;
    Ok(Json(groups))
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

/// Returns the derived trading summary used by the dashboard IA.
pub async fn get_trading_summary(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AgentError> {
    let shadow_status = state.db.get_status().await?;
    let live_status = state.db.get_live_status().await?;
    let process = state.bot.status().await?;
    let session_details = parse_details(
        live_status
            .latest_session
            .as_ref()
            .and_then(|session| session.details_json.as_deref()),
    );
    let account_details = parse_details(
        live_status
            .latest_account_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.details_json.as_deref()),
    );
    let enabled_strategies = live_status
        .latest_session
        .as_ref()
        .map(|session| parse_enabled_strategies(&session.enabled_strategies_json))
        .unwrap_or_default();
    let provider = detail_string(account_details.as_ref(), "provider")
        .or_else(|| detail_string(session_details.as_ref(), "provider"));
    let user_stream_status = detail_string(account_details.as_ref(), "user_stream_status")
        .or_else(|| detail_string(session_details.as_ref(), "user_stream_status"));
    let last_user_stream_connected_at_ms =
        detail_u64(account_details.as_ref(), "last_user_stream_connected_at_ms")
            .or_else(|| detail_u64(session_details.as_ref(), "last_user_stream_connected_at_ms"));
    let last_user_stream_event_at_ms =
        detail_u64(account_details.as_ref(), "last_user_stream_event_at_ms")
            .or_else(|| detail_u64(session_details.as_ref(), "last_user_stream_event_at_ms"));
    let last_account_refresh_at_ms = detail_u64(
        account_details.as_ref(),
        "last_successful_account_refresh_at_ms",
    )
    .or_else(|| {
        detail_u64(
            session_details.as_ref(),
            "last_successful_account_refresh_at_ms",
        )
    });
    let process_state = derive_process_state(
        process.active,
        process.control_available,
        shadow_status.last_tick_at,
    );
    let trading_state = derive_trading_state(
        &shadow_status.execution_mode,
        shadow_status.live_session_status.as_deref(),
    );
    let venue_health = build_venue_health(
        &shadow_status.execution_mode,
        live_status.latest_session.is_some(),
        provider.as_deref(),
        user_stream_status.as_deref(),
    );
    let account_health = build_account_health(
        &shadow_status.execution_mode,
        live_status.latest_account_snapshot.is_some(),
        live_status
            .latest_account_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.allowance_available),
    );
    let reconciliation_health = build_reconciliation_health(
        &shadow_status.execution_mode,
        live_status.critical_reconciliation_events,
        live_status.open_orders,
        live_status.pending_redemptions,
    );
    let capabilities = build_trading_capabilities(
        &shadow_status.execution_mode,
        &trading_state,
        &process_state,
        &venue_health,
        &account_health,
        &reconciliation_health,
        &live_status,
    );
    let alerts = build_trading_alerts(
        &shadow_status.execution_mode,
        provider.as_deref(),
        user_stream_status.as_deref(),
        &live_status,
        &process_state,
    );
    let summary = TradingSummary {
        runtime_mode: shadow_status.execution_mode.clone(),
        trading_state,
        process_state,
        venue_health,
        account_health,
        reconciliation_health,
        shadow_summary: ShadowSummary {
            balance: shadow_status.balance,
            starting_balance: shadow_status.starting_balance,
            total_pnl: shadow_status.total_pnl,
            total_trades: shadow_status.total_trades,
            wins: shadow_status.wins,
            losses: shadow_status.losses,
            win_rate: shadow_status.win_rate,
            open_trades: shadow_status.open_trades,
            uptime_hours: shadow_status.uptime_hours,
            high_water_mark: shadow_status.high_water_mark,
            max_drawdown_pct: shadow_status.max_drawdown_pct,
            live_session_status: shadow_status.live_session_status,
            last_tick_at: shadow_status.last_tick_at,
            current_window: shadow_status.current_window,
        },
        real_account_summary: RealAccountSummary {
            available_cash: live_status
                .latest_account_snapshot
                .as_ref()
                .map(|snapshot| snapshot.cash_available),
            reserved_cash: live_status
                .latest_account_snapshot
                .as_ref()
                .map(|snapshot| snapshot.cash_reserved_for_orders),
            inventory_mark_value: live_status
                .latest_account_snapshot
                .as_ref()
                .map(|snapshot| snapshot.inventory_mark_value),
            redeemable_value: live_status
                .latest_account_snapshot
                .as_ref()
                .map(|snapshot| snapshot.redeemable_value),
            pending_redeem_value: live_status
                .latest_account_snapshot
                .as_ref()
                .map(|snapshot| snapshot.pending_redeem_value),
            total_equity: live_status
                .latest_account_snapshot
                .as_ref()
                .map(|snapshot| snapshot.total_equity),
            allowance_available: live_status
                .latest_account_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.allowance_available),
            latest_snapshot_at_ms: live_status
                .latest_account_snapshot
                .as_ref()
                .map(|snapshot| snapshot.timestamp_ms),
            session_id: live_status
                .latest_session
                .as_ref()
                .map(|session| session.id),
            session_status: live_status
                .latest_session
                .as_ref()
                .map(|session| session.status.clone()),
            session_started_at_ms: live_status
                .latest_session
                .as_ref()
                .map(|session| session.started_at_ms),
            wallet_address: live_status
                .latest_session
                .as_ref()
                .and_then(|session| session.wallet_address.clone()),
            proxy_wallet: live_status
                .latest_session
                .as_ref()
                .and_then(|session| session.proxy_wallet.clone()),
            cash_cap_usd: live_status
                .latest_session
                .as_ref()
                .map(|session| session.cash_cap_usd),
            enabled_strategies,
            provider,
            user_stream_status,
            last_user_stream_connected_at_ms,
            last_user_stream_event_at_ms,
            last_account_refresh_at_ms,
            open_orders: live_status.open_orders,
            pending_redemptions: live_status.pending_redemptions,
            critical_reconciliation_events: live_status.critical_reconciliation_events,
        },
        capabilities,
        alerts,
    };
    Ok(Json(summary))
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

/// Returns recent live-control audit events.
pub async fn get_live_control_audit(
    State(state): State<AppState>,
    Query(q): Query<LiveLimitQuery>,
) -> Result<impl IntoResponse, AgentError> {
    let entries = state.db.get_live_control_audit(q.limit).await?;
    Ok(Json(entries))
}

#[derive(Debug, serde::Deserialize)]
pub struct LiveControlRequest {
    pub action: String,
    pub actor: String,
    pub reason: String,
    pub confirmation: Option<String>,
    pub bot_id: String,
}

/// Queues one audited live-control command for the running bot.
#[allow(clippy::unused_async)]
pub async fn post_live_control(
    State(state): State<AppState>,
    Json(req): Json<LiveControlRequest>,
) -> Result<impl IntoResponse, AgentError> {
    let response = state.db.queue_live_control_command(
        &req.action,
        &req.actor,
        &req.reason,
        req.confirmation.as_deref(),
        &req.bot_id,
    )?;
    Ok(Json(response))
}

/// Parses a JSON object payload from a nullable string field.
fn parse_details(details_json: Option<&str>) -> Option<serde_json::Map<String, serde_json::Value>> {
    let raw = details_json?;
    match serde_json::from_str::<serde_json::Value>(raw).ok()? {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

/// Reads a string field from a parsed details object.
fn detail_string(
    details: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<String> {
    details?.get(key)?.as_str().map(ToString::to_string)
}

/// Reads an unsigned integer field from a parsed details object.
fn detail_u64(
    details: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<u64> {
    details?.get(key)?.as_u64()
}

/// Parses the enabled-strategy JSON list stored in the live session row.
fn parse_enabled_strategies(raw: &str) -> Vec<String> {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Array(values)) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect(),
        _ => vec![],
    }
}

/// Derives the process-state label used by the dashboard header.
fn derive_process_state(
    process_active: bool,
    control_available: bool,
    last_tick_at: Option<u64>,
) -> String {
    if process_active {
        return "running".to_string();
    }
    if !control_available && last_tick_at.is_some_and(is_tick_recent) {
        return "running".to_string();
    }
    if !control_available {
        return "monitoring".to_string();
    }
    "stopped".to_string()
}

/// Returns whether the latest bot tick is still recent enough to imply activity.
fn is_tick_recent(last_tick_at: u64) -> bool {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(last_tick_at);
    now_ms.saturating_sub(last_tick_at) < 15_000
}

/// Derives the high-level trading state from runtime mode and live-session status.
fn derive_trading_state(runtime_mode: &str, live_session_status: Option<&str>) -> String {
    match runtime_mode {
        "live_readonly" => match live_session_status {
            Some("readonly_degraded" | "readonly_failed") => "degraded".to_string(),
            _ => "readonly".to_string(),
        },
        "live_trading" => match live_session_status {
            Some("armed") => "armed".to_string(),
            Some("halted") => "halted".to_string(),
            Some("unknown_order") => "unknown_order".to_string(),
            Some("stop_after_flat") => "stop_after_flat".to_string(),
            Some("disarmed") | None => "disarmed".to_string(),
            Some(status) if status.contains("degraded") || status.contains("failed") => {
                "degraded".to_string()
            }
            Some(_) => "degraded".to_string(),
        },
        _ => "paper".to_string(),
    }
}

/// Builds the venue-health indicator for the trading summary.
fn build_venue_health(
    runtime_mode: &str,
    has_session: bool,
    provider: Option<&str>,
    user_stream_status: Option<&str>,
) -> TradingHealth {
    match runtime_mode {
        "paper" => TradingHealth {
            state: "idle".to_string(),
            label: "Paper only".to_string(),
            detail: Some("No real venue boundary is active in paper mode.".to_string()),
        },
        _ if !has_session => TradingHealth {
            state: "critical".to_string(),
            label: "No live session".to_string(),
            detail: Some(
                "The authenticated venue monitor has not produced a live session.".to_string(),
            ),
        },
        _ if provider == Some("stub") => TradingHealth {
            state: "warning".to_string(),
            label: "Stub provider".to_string(),
            detail: Some("Readonly contract checks are stubbed for this provider.".to_string()),
        },
        _ if user_stream_status == Some("ok") => TradingHealth {
            state: "healthy".to_string(),
            label: "Venue connected".to_string(),
            detail: Some(
                "Authenticated venue monitoring and user-stream health are green.".to_string(),
            ),
        },
        _ if user_stream_status.is_some() => TradingHealth {
            state: "warning".to_string(),
            label: "User stream degraded".to_string(),
            detail: Some(format!(
                "Latest user-stream state: {}.",
                user_stream_status.unwrap_or("unknown")
            )),
        },
        _ => TradingHealth {
            state: "warning".to_string(),
            label: "Venue state incomplete".to_string(),
            detail: Some("Session exists, but user-stream telemetry is incomplete.".to_string()),
        },
    }
}

/// Builds the account-health indicator for the trading summary.
fn build_account_health(
    runtime_mode: &str,
    has_snapshot: bool,
    allowance_available: Option<f64>,
) -> TradingHealth {
    match runtime_mode {
        "paper" => TradingHealth {
            state: "idle".to_string(),
            label: "Shadow only".to_string(),
            detail: Some("Paper mode has no real account snapshot.".to_string()),
        },
        _ if !has_snapshot => TradingHealth {
            state: "critical".to_string(),
            label: "No account snapshot".to_string(),
            detail: Some("The venue monitor has not persisted any real account state.".to_string()),
        },
        _ if allowance_available.is_none() => TradingHealth {
            state: "warning".to_string(),
            label: "Allowance unknown".to_string(),
            detail: Some(
                "Account snapshot is present, but allowance telemetry is missing.".to_string(),
            ),
        },
        _ => TradingHealth {
            state: "healthy".to_string(),
            label: "Account tracked".to_string(),
            detail: Some(
                "Real cash, allowance, and equity decomposition are available.".to_string(),
            ),
        },
    }
}

/// Builds the reconciliation-health indicator for the trading summary.
fn build_reconciliation_health(
    runtime_mode: &str,
    critical_reconciliation_events: u64,
    open_orders: u64,
    pending_redemptions: u64,
) -> TradingHealth {
    match runtime_mode {
        "paper" => TradingHealth {
            state: "idle".to_string(),
            label: "Paper only".to_string(),
            detail: Some("Paper mode has no real reconciliation surface.".to_string()),
        },
        _ if critical_reconciliation_events > 0 => TradingHealth {
            state: "critical".to_string(),
            label: "Critical reconciliation".to_string(),
            detail: Some(format!(
                "{critical_reconciliation_events} critical reconciliation events need attention."
            )),
        },
        _ if open_orders > 0 || pending_redemptions > 0 => TradingHealth {
            state: "warning".to_string(),
            label: "Pending activity".to_string(),
            detail: Some(format!(
                "{open_orders} open orders and {pending_redemptions} pending redemptions are still tracked."
            )),
        },
        _ => TradingHealth {
            state: "healthy".to_string(),
            label: "Reconciliation clean".to_string(),
            detail: Some("No critical reconciliation events are recorded.".to_string()),
        },
    }
}

/// Builds one disabled control capability entry with a reason.
fn disabled_capability(reason: &str) -> TradingControlCapability {
    capability(false, reason)
}

/// Builds one control capability entry.
fn capability(enabled: bool, reason: &str) -> TradingControlCapability {
    TradingControlCapability {
        enabled,
        reason: reason.to_string(),
    }
}

/// Builds the control-capability map for the dashboard trading surface.
fn build_trading_capabilities(
    runtime_mode: &str,
    trading_state: &str,
    process_state: &str,
    venue_health: &TradingHealth,
    account_health: &TradingHealth,
    reconciliation_health: &TradingHealth,
    live_status: &crate::types::LiveStatusResponse,
) -> TradingCapabilities {
    if runtime_mode == "paper" {
        let reason = "Paper mode has no live venue boundary.";
        return TradingCapabilities {
            preflight: disabled_capability(reason),
            arm: disabled_capability(reason),
            disarm: disabled_capability(reason),
            cancel_all: disabled_capability(reason),
            stop_after_flat: disabled_capability(reason),
            redeem: disabled_capability(reason),
            kill_switch: disabled_capability(reason),
        };
    }
    if runtime_mode == "live_readonly" {
        let reason =
            "live_readonly is observation-only; start a live_trading session to queue controls.";
        return TradingCapabilities {
            preflight: disabled_capability(reason),
            arm: disabled_capability(reason),
            disarm: disabled_capability(reason),
            cancel_all: disabled_capability(reason),
            stop_after_flat: disabled_capability(reason),
            redeem: disabled_capability(reason),
            kill_switch: disabled_capability(reason),
        };
    }

    let arm_ready = trading_state == "disarmed"
        && process_state == "running"
        && venue_health.state == "healthy"
        && account_health.state == "healthy"
        && reconciliation_health.state == "healthy";
    let arm_reason = if trading_state == "halted" {
        "This live session is halted. Export and analyze it before starting a new session."
    } else if trading_state == "unknown_order" {
        "Order state is unknown. Reconcile venue orders and fills before arming."
    } else if trading_state != "disarmed" {
        "Arming is only available from a disarmed live_trading session."
    } else if process_state != "running" {
        "The bot process must be running before arming."
    } else if venue_health.state != "healthy" {
        "Venue health is not green."
    } else if account_health.state != "healthy" {
        "Account health is not green."
    } else if reconciliation_health.state != "healthy" {
        "Reconciliation health is not green."
    } else {
        "All system gates are green. Admin confirmation is still required."
    };
    let open_orders = live_status.open_orders;
    let redeemable = live_status
        .latest_account_snapshot
        .as_ref()
        .map_or(0.0, |snapshot| snapshot.redeemable_value);
    TradingCapabilities {
        preflight: capability(true, "Queue an audited preflight refresh."),
        arm: capability(arm_ready, arm_reason),
        disarm: capability(
            matches!(trading_state, "armed" | "stop_after_flat"),
            if matches!(trading_state, "armed" | "stop_after_flat") {
                "Disarm live submissions without clearing unresolved critical states."
            } else {
                "Disarm is only available while armed or stopping after flat."
            },
        ),
        cancel_all: capability(
            open_orders > 0,
            if open_orders > 0 {
                "Cancel all tracked open venue orders."
            } else {
                "No open venue orders are currently tracked."
            },
        ),
        stop_after_flat: capability(
            trading_state == "armed",
            if trading_state == "armed" {
                "Stop submitting new orders after exposure is flat."
            } else {
                "Stop-after-flat is only available while armed."
            },
        ),
        redeem: capability(
            redeemable > 0.0,
            if redeemable > 0.0 {
                "Redeem all currently detected redeemable positions."
            } else {
                "No redeemable positions are currently detected."
            },
        ),
        kill_switch: capability(
            trading_state != "halted",
            if trading_state == "halted" {
                "The live session is already halted."
            } else {
                "Halt this session, attempt cancel-all, and block re-arming."
            },
        ),
    }
}

/// Builds the alert list shown in the header, overview, and trading pages.
fn build_trading_alerts(
    runtime_mode: &str,
    provider: Option<&str>,
    user_stream_status: Option<&str>,
    live_status: &crate::types::LiveStatusResponse,
    process_state: &str,
) -> Vec<TradingAlert> {
    let mut alerts = Vec::new();
    if process_state == "stopped" {
        alerts.push(TradingAlert {
            severity: "warning".to_string(),
            title: "Process stopped".to_string(),
            detail: "The bot process is not currently running.".to_string(),
        });
    }
    match runtime_mode {
        "live_trading" => match live_status
            .latest_session
            .as_ref()
            .map(|session| session.status.as_str())
        {
            Some("armed") => alerts.push(TradingAlert {
                severity: "critical".to_string(),
                title: "Live trading armed".to_string(),
                detail: "The bot is permitted to submit real venue orders through the local live-control state.".to_string(),
            }),
            Some("halted") => alerts.push(TradingAlert {
                severity: "critical".to_string(),
                title: "Live session halted".to_string(),
                detail: "The live session hit a terminal halt and requires analysis before a new session.".to_string(),
            }),
            Some("unknown_order") => alerts.push(TradingAlert {
                severity: "critical".to_string(),
                title: "Order state unknown".to_string(),
                detail: "Trading is blocked until venue order and fill state are reconciled.".to_string(),
            }),
            Some("stop_after_flat") => alerts.push(TradingAlert {
                severity: "warning".to_string(),
                title: "Stop after flat".to_string(),
                detail: "The bot should stop submitting new orders after current exposure is flat.".to_string(),
            }),
            Some("disarmed") => alerts.push(TradingAlert {
                severity: "info".to_string(),
                title: "Live trading disarmed".to_string(),
                detail: "Use the dashboard Execution controls or buba-paint live-control from the host after preflight gates pass.".to_string(),
            }),
            _ => {}
        },
        "live_readonly" => alerts.push(TradingAlert {
            severity: "info".to_string(),
            title: "Shadow and execution views differ".to_string(),
            detail: "Overview, equity, trades, signals, and strategies remain shadow-performance pages in live_readonly.".to_string(),
        }),
        _ => {}
    }
    if runtime_mode != "paper" && live_status.latest_session.is_none() {
        alerts.push(TradingAlert {
            severity: "critical".to_string(),
            title: "No live session".to_string(),
            detail: "The authenticated venue monitor has not recorded any session.".to_string(),
        });
    }
    if provider == Some("stub") {
        alerts.push(TradingAlert {
            severity: "warning".to_string(),
            title: "Stub provider".to_string(),
            detail: "Readonly venue checks are still stubbed for this provider.".to_string(),
        });
    }
    if user_stream_status.is_some() && user_stream_status != Some("ok") {
        alerts.push(TradingAlert {
            severity: "warning".to_string(),
            title: "User stream degraded".to_string(),
            detail: format!(
                "Latest user-stream state is {}.",
                user_stream_status.unwrap_or("unknown")
            ),
        });
    }
    if runtime_mode == "live_readonly" && live_status.open_orders > 0 {
        alerts.push(TradingAlert {
            severity: "critical".to_string(),
            title: "Unexpected remote open orders".to_string(),
            detail: format!(
                "{} real venue orders are open during readonly monitoring.",
                live_status.open_orders
            ),
        });
    }
    if runtime_mode != "paper" && live_status.latest_account_snapshot.is_none() {
        alerts.push(TradingAlert {
            severity: "warning".to_string(),
            title: "Account snapshot missing".to_string(),
            detail: "No real account snapshot has been recorded yet.".to_string(),
        });
    }
    if runtime_mode != "paper"
        && live_status
            .latest_account_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.allowance_available.is_none())
    {
        alerts.push(TradingAlert {
            severity: "warning".to_string(),
            title: "Allowance missing".to_string(),
            detail: "Live account telemetry does not currently include allowance.".to_string(),
        });
    }
    if live_status.critical_reconciliation_events > 0 {
        alerts.push(TradingAlert {
            severity: "critical".to_string(),
            title: "Critical reconciliation events".to_string(),
            detail: format!(
                "{} critical reconciliation events are recorded.",
                live_status.critical_reconciliation_events
            ),
        });
    }
    alerts
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
