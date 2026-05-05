use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::bankroll::BankrollStats;
use crate::clock::Clock;
use crate::config::Config;
use crate::db::database::Database;
use crate::live_sidecar::{LiveAccountState, LivePreflightResponse, LiveSidecarClient};
use crate::types::{ControlAuditEntry, LiveAccountSnapshot, LiveReconciliationEvent, LiveSession};

/// Poll cadence for real venue/account snapshots during readonly monitoring.
pub const READONLY_POLL_INTERVAL_SECS: u64 = 60;
/// Operator rollup cadence for combined venue and shadow-paper status logs.
pub const READONLY_ROLLUP_INTERVAL_SECS: u64 = 5 * 60;
const CASH_CHANGE_EPSILON_USD: f64 = 0.01;

#[derive(Debug, Default, Deserialize)]
struct AccountDetailsView {
    provider: Option<String>,
    user_stream_status: Option<String>,
    last_user_stream_connected_at_ms: Option<u64>,
    last_user_stream_event_at_ms: Option<u64>,
    last_successful_account_refresh_at_ms: Option<u64>,
    open_order_count: Option<u64>,
    open_buy_order_count: Option<u64>,
    open_sell_order_count: Option<u64>,
    position_count: Option<u64>,
    redeemable_position_count: Option<u64>,
    legal_order_min_usd: Option<f64>,
    account_refresh_error: Option<String>,
    relayer_api_key_present: Option<bool>,
}

#[derive(Debug, Clone)]
struct ReadonlySessionState {
    session_id: i64,
    status: String,
    preflight: LivePreflightResponse,
    account: Option<LiveAccountState>,
    soft_issues: Vec<String>,
}

/// Bootstrap output needed to start the shared shadow-paper runtime in readonly mode.
pub struct ReadonlyRuntimeBootstrap {
    /// Starting balance for the shadow-paper bankroll on this runtime boot.
    pub shadow_starting_balance: f64,
    /// Background readonly monitor that persists real venue/account state.
    pub monitor: ReadonlyMonitor,
}

/// Background readonly venue monitor that tracks session state and account snapshots.
pub struct ReadonlyMonitor {
    sidecar: LiveSidecarClient,
    session: ReadonlySessionState,
    previous_account: LiveAccountState,
    finished: bool,
}

impl ReadonlyMonitor {
    /// Fetch the latest remote account-state snapshot from the readonly sidecar.
    pub async fn fetch_account_state(&self) -> anyhow::Result<LiveAccountState> {
        self.sidecar.account_state().await
    }

    /// Persist one successfully fetched readonly account snapshot and refresh session status.
    pub fn apply_account_state(
        &mut self,
        db: &Database,
        config: &Config,
        account: LiveAccountState,
    ) -> anyhow::Result<()> {
        db.log_live_account_snapshot(&account_snapshot(self.session.session_id, &account))?;
        persist_reconciliation_events(
            db,
            readonly_reconciliation_events(
                self.session.session_id,
                Some(&self.previous_account),
                &account,
            ),
        )?;
        self.previous_account = account.clone();
        self.session.account = Some(account);
        self.session.soft_issues = readonly_soft_issues(
            &self.session.preflight,
            self.session.account.as_ref(),
            config,
        );
        update_session_status(db, &mut self.session, config)?;
        Ok(())
    }

    /// Record a non-fatal account refresh failure and degrade the readonly session.
    pub fn record_account_refresh_failure(
        &mut self,
        db: &Database,
        config: &Config,
        clock: &dyn Clock,
        error: &str,
    ) -> anyhow::Result<()> {
        let now_ms = clock.now();
        let event = LiveReconciliationEvent {
            id: None,
            session_id: self.session.session_id,
            timestamp_ms: now_ms,
            severity: "critical".to_string(),
            event_type: "account_state_refresh_failed".to_string(),
            local_value: None,
            remote_value: None,
            details_json: Some(json!({ "error": error }).to_string()),
        };
        persist_reconciliation_events(db, vec![event])?;
        self.session.soft_issues = vec![format!("Readonly account refresh failed: {error}")];
        update_session_status(db, &mut self.session, config)?;
        Ok(())
    }

    /// Emit one operator-facing rollup combining real venue status and shadow paper stats.
    pub fn log_shadow_rollup(&self, bankroll: &BankrollStats) {
        let account = self.session.account.as_ref();
        let details = account.map(parse_account_details).unwrap_or_default();
        info!(
            session_id = self.session.session_id,
            session_status = %self.session.status,
            cash_available = account.map(|value| value.cash_available),
            allowance_available = account.and_then(|value| value.allowance_available),
            user_stream_status = details.user_stream_status.as_deref().unwrap_or("unknown"),
            shadow_balance = bankroll.current_balance,
            shadow_total_pnl = bankroll.total_pnl,
            shadow_trades = bankroll.total_trades,
            "readonly shadow runtime rollup"
        );
    }

    /// Finish the readonly session cleanly after a normal runtime shutdown.
    pub fn finish_stopped(&mut self, db: &Database, clock: &dyn Clock) -> anyhow::Result<()> {
        self.finish_session(
            db,
            clock.now(),
            "readonly_stopped",
            "readonly_session_stopped",
            None,
        )
    }

    /// Finalize the current readonly session exactly once and persist the terminal audit entry.
    fn finish_session(
        &mut self,
        db: &Database,
        ended_at_ms: u64,
        status: &str,
        action: &str,
        extra_details_json: Option<String>,
    ) -> anyhow::Result<()> {
        if self.finished {
            return Ok(());
        }
        let mut details = serde_json::from_str::<serde_json::Value>(&session_details_json(
            Some(&self.session.preflight),
            self.session.account.as_ref(),
            &self.session.soft_issues,
            status,
        ))
        .unwrap_or_else(|_| json!({ "status": status }));
        if let Some(extra) = extra_details_json
            && let Ok(extra_value) = serde_json::from_str::<serde_json::Value>(&extra)
            && let Some(obj) = details.as_object_mut()
        {
            obj.insert("extra".to_string(), extra_value);
        }
        let details_json = details.to_string();
        db.finish_live_session(
            self.session.session_id,
            ended_at_ms,
            status,
            Some(&details_json),
        )?;
        db.log_control_audit(&ControlAuditEntry {
            id: None,
            timestamp_ms: ended_at_ms,
            actor: "system".to_string(),
            action: action.to_string(),
            target: Some(self.session.session_id.to_string()),
            details_json: Some(details_json),
        })?;
        self.finished = true;
        Ok(())
    }
}

/// Bootstrap the readonly venue monitor and return the starting shadow-paper bankroll.
#[allow(clippy::too_many_lines)]
pub async fn bootstrap_readonly_runtime(
    config: &Config,
    db: Database,
    clock: &dyn Clock,
) -> anyhow::Result<(Database, ReadonlyRuntimeBootstrap)> {
    let started_at_ms = clock.now();
    info!(
        execution_mode = config.execution_mode.as_str(),
        live_sidecar_url = %config.live_sidecar_url,
        cash_cap_usd = config.live_session_cash_cap_usd,
        "starting live readonly venue monitor"
    );
    let enabled_strategies = serde_json::to_string(&config.enabled_strategy_names())
        .context("serializing readonly enabled strategies")?;
    let config_fingerprint = readonly_config_fingerprint(config);
    let session_id = db.insert_live_session(&LiveSession {
        id: None,
        started_at_ms,
        ended_at_ms: None,
        status: "readonly_starting".to_string(),
        execution_mode: config.execution_mode.as_str().to_string(),
        wallet_address: None,
        proxy_wallet: None,
        enabled_strategies_json: enabled_strategies,
        config_fingerprint,
        cash_cap_usd: config.live_session_cash_cap_usd,
        details_json: Some(session_details_json(None, None, &[], "readonly_starting")),
    })?;
    db.log_control_audit(&ControlAuditEntry {
        id: None,
        timestamp_ms: started_at_ms,
        actor: "system".to_string(),
        action: "readonly_session_started".to_string(),
        target: Some(session_id.to_string()),
        details_json: Some(json!({ "execution_mode": config.execution_mode.as_str() }).to_string()),
    })?;

    let sidecar = LiveSidecarClient::new(&config.live_sidecar_url);
    let preflight = match sidecar.preflight(config).await {
        Ok(preflight) => preflight,
        Err(error) => {
            let details = json!({
                "startup_error": error.to_string(),
                "stage": "preflight",
            })
            .to_string();
            db.finish_live_session(session_id, clock.now(), "readonly_failed", Some(&details))?;
            bail!("readonly preflight failed: {error}");
        }
    };

    db.update_live_session_metadata(
        session_id,
        "readonly_starting",
        preflight.wallet_address.as_deref(),
        preflight.proxy_wallet.as_deref(),
        Some(&session_details_json(
            Some(&preflight),
            None,
            &[],
            "readonly_starting",
        )),
    )?;

    let hard_failures = readonly_hard_failures(&preflight);
    if !hard_failures.is_empty() {
        let details =
            session_details_json(Some(&preflight), None, &hard_failures, "readonly_failed");
        db.finish_live_session(session_id, clock.now(), "readonly_failed", Some(&details))?;
        bail!(
            "readonly startup blocked by hard preflight failures: {}",
            hard_failures.join("; ")
        );
    }

    let initial_account = match sidecar.account_state().await {
        Ok(account) => account,
        Err(error) => {
            let details = json!({
                "startup_error": error.to_string(),
                "stage": "account_state",
                "preflight": preflight_summary_json(&preflight),
            })
            .to_string();
            db.finish_live_session(session_id, clock.now(), "readonly_failed", Some(&details))?;
            bail!("readonly account-state bootstrap failed: {error}");
        }
    };

    let mut session = ReadonlySessionState {
        session_id,
        status: "readonly_starting".to_string(),
        soft_issues: readonly_soft_issues(&preflight, Some(&initial_account), config),
        preflight,
        account: Some(initial_account.clone()),
    };

    db.log_live_account_snapshot(&account_snapshot(session_id, &initial_account))?;
    persist_reconciliation_events(
        &db,
        readonly_reconciliation_events(session_id, None, &initial_account),
    )?;
    update_session_status(&db, &mut session, config)?;

    let shadow_starting_balance = match db.get_initial_balance()? {
        Some(balance) => balance,
        None => initial_account
            .cash_available
            .max(0.0)
            .min(config.live_session_cash_cap_usd),
    };
    info!(
        session_id,
        shadow_starting_balance,
        available_cash = initial_account.cash_available,
        cash_cap_usd = config.live_session_cash_cap_usd,
        "readonly shadow bankroll resolved"
    );

    Ok((
        db,
        ReadonlyRuntimeBootstrap {
            shadow_starting_balance,
            monitor: ReadonlyMonitor {
                sidecar,
                session,
                previous_account: initial_account,
                finished: false,
            },
        },
    ))
}

/// Update the mutable readonly session status and persisted details.
fn update_session_status(
    db: &Database,
    session: &mut ReadonlySessionState,
    config: &Config,
) -> anyhow::Result<()> {
    let next_status =
        readonly_session_status(&session.soft_issues, session.account.as_ref(), config);
    if session.status == next_status {
        db.update_live_session_metadata(
            session.session_id,
            &next_status,
            session.preflight.wallet_address.as_deref(),
            session.preflight.proxy_wallet.as_deref(),
            Some(&session_details_json(
                Some(&session.preflight),
                session.account.as_ref(),
                &session.soft_issues,
                &next_status,
            )),
        )?;
        return Ok(());
    }

    info!(
        session_id = session.session_id,
        previous_status = %session.status,
        next_status = %next_status,
        issue_count = session.soft_issues.len(),
        "readonly session status changed"
    );
    session.status.clone_from(&next_status);
    db.update_live_session_metadata(
        session.session_id,
        &next_status,
        session.preflight.wallet_address.as_deref(),
        session.preflight.proxy_wallet.as_deref(),
        Some(&session_details_json(
            Some(&session.preflight),
            session.account.as_ref(),
            &session.soft_issues,
            &next_status,
        )),
    )?;
    Ok(())
}

/// Build one deterministic config fingerprint for readonly sessions.
fn readonly_config_fingerprint(config: &Config) -> String {
    json!({
        "execution_mode": config.execution_mode.as_str(),
        "live_sidecar_url": config.live_sidecar_url,
        "enabled_strategies": config.enabled_strategy_names(),
        "cash_cap_usd": config.live_session_cash_cap_usd,
        "max_single_order_usd": config.live_max_single_order_usd,
        "max_open_notional_usd": config.live_max_open_notional_usd,
        "max_daily_loss_usd": config.live_max_daily_loss_usd,
        "max_session_drawdown_usd": config.live_max_session_drawdown_usd,
        "min_required_cash_usd": config.live_min_required_cash_usd,
    })
    .to_string()
}

/// Return the hard readonly blockers extracted from one preflight response.
fn readonly_hard_failures(preflight: &LivePreflightResponse) -> Vec<String> {
    let mut failures = Vec::new();
    if matches!(
        preflight.geoblock_status,
        crate::live_sidecar::LiveCheckStatus::Failed
    ) {
        failures.push("Host geoblock check failed.".to_string());
    }
    if matches!(
        preflight.auth_status,
        crate::live_sidecar::LiveCheckStatus::Failed
    ) {
        failures.push("Polymarket authentication failed.".to_string());
    }
    if matches!(
        preflight.clock_status,
        crate::live_sidecar::LiveCheckStatus::Failed
    ) {
        failures.push("Clock drift exceeded the configured threshold.".to_string());
    }
    if matches!(
        preflight.user_stream_status,
        crate::live_sidecar::LiveCheckStatus::Failed
    ) {
        failures.push("Authenticated user stream could not connect.".to_string());
    }
    failures
}

/// Return the non-fatal readonly readiness issues from the latest state.
fn readonly_soft_issues(
    preflight: &LivePreflightResponse,
    account: Option<&LiveAccountState>,
    config: &Config,
) -> Vec<String> {
    let mut issues = Vec::new();
    if let Some(account) = account {
        let details = parse_account_details(account);
        if account.allowance_available.is_none() {
            issues.push("Allowance is unavailable for a future trading session.".to_string());
        }
        if account.cash_available + CASH_CHANGE_EPSILON_USD < config.live_min_required_cash_usd {
            issues.push(format!(
                "Available cash {:.2} is below LIVE_MIN_REQUIRED_CASH_USD {:.2}.",
                account.cash_available, config.live_min_required_cash_usd
            ));
        }
        let legal_order_min = details
            .legal_order_min_usd
            .or(preflight.legal_order_min_usd);
        if let Some(legal_order_min) = legal_order_min {
            if legal_order_min > config.live_max_single_order_usd + CASH_CHANGE_EPSILON_USD {
                issues.push(format!(
                    "Current legal order minimum {:.2} exceeds LIVE_MAX_SINGLE_ORDER_USD {:.2}.",
                    legal_order_min, config.live_max_single_order_usd
                ));
            }
            if account.cash_available + CASH_CHANGE_EPSILON_USD < legal_order_min {
                issues.push(format!(
                    "Cash available {:.2} is below the live venue minimum order size {:.2}.",
                    account.cash_available, legal_order_min
                ));
            }
        }
        if details.open_order_count.unwrap_or(0) > 0 {
            issues.push("Readonly session detected remote open orders.".to_string());
        }
        if details.user_stream_status.as_deref() == Some("failed") {
            issues.push("Readonly session user stream is unhealthy.".to_string());
        }
    } else {
        if matches!(
            preflight.allowance_status,
            crate::live_sidecar::LiveCheckStatus::Failed
        ) {
            issues.push("Allowance is unavailable for a future trading session.".to_string());
        }
        if let Some(cash_available) = preflight.available_cash_usd
            && cash_available + CASH_CHANGE_EPSILON_USD < config.live_min_required_cash_usd
        {
            issues.push(format!(
                "Available cash {:.2} is below LIVE_MIN_REQUIRED_CASH_USD {:.2}.",
                cash_available, config.live_min_required_cash_usd
            ));
        }
        if let Some(legal_order_min) = preflight.legal_order_min_usd {
            if legal_order_min > config.live_max_single_order_usd + CASH_CHANGE_EPSILON_USD {
                issues.push(format!(
                    "Current legal order minimum {:.2} exceeds LIVE_MAX_SINGLE_ORDER_USD {:.2}.",
                    legal_order_min, config.live_max_single_order_usd
                ));
            }
            if let Some(cash_available) = preflight.available_cash_usd
                && cash_available + CASH_CHANGE_EPSILON_USD < legal_order_min
            {
                issues.push(format!(
                    "Available cash {cash_available:.2} is below the live venue minimum order size {legal_order_min:.2}.",
                ));
            }
        }
    }
    issues.sort();
    issues.dedup();
    issues
}

/// Map the latest readonly issues into the persisted live-session status string.
fn readonly_session_status(
    soft_issues: &[String],
    account: Option<&LiveAccountState>,
    _config: &Config,
) -> String {
    if account.is_none() {
        return "readonly_starting".to_string();
    }
    if soft_issues.is_empty() {
        "readonly_ready".to_string()
    } else {
        "readonly_degraded".to_string()
    }
}

/// Convert one live sidecar account-state snapshot into a DB row.
fn account_snapshot(session_id: i64, account: &LiveAccountState) -> LiveAccountSnapshot {
    LiveAccountSnapshot {
        id: None,
        session_id,
        timestamp_ms: account.timestamp_ms,
        cash_available: account.cash_available,
        cash_reserved_for_orders: account.cash_reserved_for_orders,
        inventory_mark_value: account.inventory_mark_value,
        redeemable_value: account.redeemable_value,
        pending_redeem_value: account.pending_redeem_value,
        total_equity: account.total_equity,
        allowance_available: account.allowance_available,
        details_json: account.details_json.clone(),
    }
}

/// Build the operator-facing live-session details payload.
fn session_details_json(
    preflight: Option<&LivePreflightResponse>,
    account: Option<&LiveAccountState>,
    soft_issues: &[String],
    status: &str,
) -> String {
    let account_details = account.map(parse_account_details).unwrap_or_default();
    json!({
        "provider": account_details.provider.or_else(|| preflight_provider(preflight)),
        "status": status,
        "soft_issues": soft_issues,
        "preflight": preflight.map(preflight_summary_json),
        "account": account.map(account_summary_json),
        "user_stream_status": account_details.user_stream_status,
        "last_user_stream_connected_at_ms": account_details.last_user_stream_connected_at_ms,
        "last_user_stream_event_at_ms": account_details.last_user_stream_event_at_ms,
        "last_successful_account_refresh_at_ms": account_details.last_successful_account_refresh_at_ms,
        "open_order_count": account_details.open_order_count,
        "open_buy_order_count": account_details.open_buy_order_count,
        "open_sell_order_count": account_details.open_sell_order_count,
        "position_count": account_details.position_count,
        "redeemable_position_count": account_details.redeemable_position_count,
        "legal_order_min_usd": account_details.legal_order_min_usd.or_else(|| preflight.and_then(|value| value.legal_order_min_usd)),
        "relayer_api_key_present": account_details.relayer_api_key_present,
        "account_refresh_error": account_details.account_refresh_error,
    })
    .to_string()
}

/// Return the compact readonly preflight summary persisted in session details.
fn preflight_summary_json(preflight: &LivePreflightResponse) -> serde_json::Value {
    json!({
        "ok": preflight.ok,
        "mode": preflight.mode,
        "wallet_address": preflight.wallet_address,
        "proxy_wallet": preflight.proxy_wallet,
        "geoblock_status": status_label(preflight.geoblock_status),
        "geoblock_country_code": preflight.geoblock_country_code,
        "auth_status": status_label(preflight.auth_status),
        "clock_status": status_label(preflight.clock_status),
        "allowance_status": status_label(preflight.allowance_status),
        "user_stream_status": status_label(preflight.user_stream_status),
        "available_cash_usd": preflight.available_cash_usd,
        "legal_order_min_usd": preflight.legal_order_min_usd,
        "errors": preflight.errors,
        "details_json": preflight.details_json,
    })
}

/// Return the compact live account summary persisted in session details.
fn account_summary_json(account: &LiveAccountState) -> serde_json::Value {
    json!({
        "timestamp_ms": account.timestamp_ms,
        "wallet_address": account.wallet_address,
        "proxy_wallet": account.proxy_wallet,
        "cash_available": account.cash_available,
        "cash_reserved_for_orders": account.cash_reserved_for_orders,
        "inventory_mark_value": account.inventory_mark_value,
        "redeemable_value": account.redeemable_value,
        "pending_redeem_value": account.pending_redeem_value,
        "total_equity": account.total_equity,
        "allowance_available": account.allowance_available,
    })
}

/// Parse the sidecar account details payload into a strongly-typed view.
fn parse_account_details(account: &LiveAccountState) -> AccountDetailsView {
    account
        .details_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<AccountDetailsView>(value).ok())
        .unwrap_or_default()
}

/// Return the provider label embedded in the preflight details payload.
fn preflight_provider(preflight: Option<&LivePreflightResponse>) -> Option<String> {
    preflight
        .and_then(|value| value.details_json.as_deref())
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .and_then(|value| {
            value
                .get("provider")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        })
}

/// Convert one serialized live check status back into its persisted string label.
fn status_label(status: crate::live_sidecar::LiveCheckStatus) -> &'static str {
    match status {
        crate::live_sidecar::LiveCheckStatus::Ok => "ok",
        crate::live_sidecar::LiveCheckStatus::Failed => "failed",
    }
}

/// Emit readonly reconciliation events for one newly-observed remote account state.
fn readonly_reconciliation_events(
    session_id: i64,
    previous: Option<&LiveAccountState>,
    current: &LiveAccountState,
) -> Vec<LiveReconciliationEvent> {
    let now_ms = current.timestamp_ms;
    let current_details = parse_account_details(current);
    let previous_details = previous.map(parse_account_details).unwrap_or_default();
    let mut events = Vec::new();

    let current_open_orders = current_details.open_order_count.unwrap_or(0);
    let previous_open_orders = previous_details.open_order_count.unwrap_or(0);
    if current_open_orders > 0 && current_open_orders != previous_open_orders {
        events.push(LiveReconciliationEvent {
            id: None,
            session_id,
            timestamp_ms: now_ms,
            severity: "critical".to_string(),
            event_type: "unexpected_remote_open_orders".to_string(),
            local_value: Some(previous_open_orders as f64),
            remote_value: Some(current_open_orders as f64),
            details_json: Some(
                json!({
                    "previous_open_order_count": previous_open_orders,
                    "current_open_order_count": current_open_orders,
                })
                .to_string(),
            ),
        });
    }

    if current_details.user_stream_status.as_deref() == Some("failed")
        && previous_details.user_stream_status.as_deref() != Some("failed")
    {
        events.push(LiveReconciliationEvent {
            id: None,
            session_id,
            timestamp_ms: now_ms,
            severity: "warning".to_string(),
            event_type: "user_stream_unhealthy".to_string(),
            local_value: None,
            remote_value: None,
            details_json: Some(
                json!({
                    "last_user_stream_connected_at_ms": current_details.last_user_stream_connected_at_ms,
                    "last_user_stream_event_at_ms": current_details.last_user_stream_event_at_ms,
                    "account_refresh_error": current_details.account_refresh_error,
                })
                .to_string(),
            ),
        });
    }

    if let Some(previous) = previous {
        if (current.cash_available - previous.cash_available).abs() > CASH_CHANGE_EPSILON_USD {
            events.push(LiveReconciliationEvent {
                id: None,
                session_id,
                timestamp_ms: now_ms,
                severity: "warning".to_string(),
                event_type: "unexpected_remote_balance_change".to_string(),
                local_value: Some(previous.cash_available),
                remote_value: Some(current.cash_available),
                details_json: None,
            });
        }

        let previous_position_value = previous.inventory_mark_value + previous.redeemable_value;
        let current_position_value = current.inventory_mark_value + current.redeemable_value;
        if (current_position_value - previous_position_value).abs() > CASH_CHANGE_EPSILON_USD {
            events.push(LiveReconciliationEvent {
                id: None,
                session_id,
                timestamp_ms: now_ms,
                severity: "warning".to_string(),
                event_type: "unexpected_remote_position_change".to_string(),
                local_value: Some(previous_position_value),
                remote_value: Some(current_position_value),
                details_json: Some(
                    json!({
                        "previous_position_count": previous_details.position_count,
                        "current_position_count": current_details.position_count,
                    })
                    .to_string(),
                ),
            });
        }
    }

    events
}

/// Persist one batch of reconciliation events while preserving call-site ordering semantics.
fn persist_reconciliation_events(
    db: &Database,
    events: Vec<LiveReconciliationEvent>,
) -> anyhow::Result<()> {
    for event in events {
        if event.severity.as_str() == "critical" {
            warn!(
                session_id = event.session_id,
                event_type = %event.event_type,
                local_value = event.local_value,
                remote_value = event.remote_value,
                details_json = event.details_json.as_deref(),
                "readonly reconciliation event"
            );
        } else {
            info!(
                session_id = event.session_id,
                event_type = %event.event_type,
                local_value = event.local_value,
                remote_value = event.remote_value,
                details_json = event.details_json.as_deref(),
                "readonly reconciliation event"
            );
        }
        db.log_live_reconciliation_event(&event)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::clock::SystemClock;
    use crate::config::ExecutionMode;
    use crate::db::database::Database;

    /// Return one temporary sqlite path suitable for readonly runtime tests.
    fn temp_db_path() -> (NamedTempFile, String) {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        (tmp, path)
    }

    /// Return one successful readonly preflight payload.
    fn ok_preflight_body() -> serde_json::Value {
        json!({
            "ok": true,
            "mode": "live_readonly",
            "wallet_address": "0xwallet",
            "proxy_wallet": "0xproxy",
            "geoblock_status": "ok",
            "geoblock_country_code": "IE",
            "auth_status": "ok",
            "clock_status": "ok",
            "allowance_status": "ok",
            "user_stream_status": "ok",
            "available_cash_usd": 92.0,
            "legal_order_min_usd": 5.0,
            "details_json": "{\"provider\":\"polymarket\"}",
            "errors": []
        })
    }

    /// Return one readonly account payload with healthy stream metadata.
    fn ok_account_body() -> serde_json::Value {
        json!({
            "timestamp_ms": 1_700_000_000_000u64,
            "wallet_address": "0xwallet",
            "proxy_wallet": "0xproxy",
            "cash_available": 92.0,
            "cash_reserved_for_orders": 0.0,
            "inventory_mark_value": 0.0,
            "redeemable_value": 0.0,
            "pending_redeem_value": 0.0,
            "total_equity": 92.0,
            "allowance_available": 92.0,
            "details_json": "{\"provider\":\"polymarket\",\"user_stream_status\":\"ok\",\"open_order_count\":0,\"position_count\":0,\"redeemable_position_count\":0,\"legal_order_min_usd\":5.0}"
        })
    }

    /// Verifies that readonly bootstrap creates a ready session and account snapshot.
    #[tokio::test]
    async fn bootstrap_readonly_runtime_creates_ready_session() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/preflight"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_preflight_body()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_account_body()))
            .mount(&server)
            .await;

        let (_tmp, db_path) = temp_db_path();
        let db = Database::new(&db_path).unwrap();
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveReadonly;
        config.live_sidecar_url = server.uri();
        let clock = SystemClock;

        let (db, mut bootstrap) = bootstrap_readonly_runtime(&config, db, &clock)
            .await
            .unwrap();
        bootstrap.monitor.finish_stopped(&db, &clock).unwrap();

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM live_sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let execution_mode: String = conn
            .query_row(
                "SELECT execution_mode FROM live_sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let snapshot_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM live_account_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(status, "readonly_stopped");
        assert_eq!(execution_mode, "live_readonly");
        assert_eq!(snapshot_count, 1);
        assert!((bootstrap.shadow_starting_balance - 92.0).abs() < f64::EPSILON);
    }

    /// Verifies that readonly bootstrap preserves the original run init balance on restart.
    #[tokio::test]
    async fn bootstrap_readonly_runtime_preserves_existing_init_balance() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/preflight"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_preflight_body()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_account_body()))
            .mount(&server)
            .await;

        let (_tmp, db_path) = temp_db_path();
        let db = Database::new(&db_path).unwrap();
        db.log_balance_event(1_000, "init", None, 0.0, 77.0)
            .unwrap();
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveReadonly;
        config.live_sidecar_url = server.uri();
        let clock = SystemClock;

        let (_db, bootstrap) = bootstrap_readonly_runtime(&config, db, &clock)
            .await
            .unwrap();

        assert!((bootstrap.shadow_starting_balance - 77.0).abs() < f64::EPSILON);
    }

    /// Verifies that account refresh failures degrade the readonly session without crashing it.
    #[tokio::test]
    async fn refresh_account_state_marks_session_degraded_on_refresh_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/preflight"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_preflight_body()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_account_body()))
            .mount(&server)
            .await;

        let (_tmp, db_path) = temp_db_path();
        let db = Database::new(&db_path).unwrap();
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveReadonly;
        config.live_sidecar_url = server.uri();
        let clock = SystemClock;

        let (db, mut bootstrap) = bootstrap_readonly_runtime(&config, db, &clock)
            .await
            .unwrap();
        bootstrap
            .monitor
            .record_account_refresh_failure(&db, &config, &clock, "boom")
            .unwrap();

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM live_sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let event_type: String = conn
            .query_row(
                "SELECT event_type FROM live_reconciliation_events ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(status, "readonly_degraded");
        assert_eq!(event_type, "account_state_refresh_failed");
    }

    /// Verifies that current account readiness clears stale startup degradation.
    #[tokio::test]
    async fn apply_account_state_uses_current_account_readiness() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/preflight"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": false,
                "mode": "live_readonly",
                "wallet_address": "0xwallet",
                "proxy_wallet": "0xproxy",
                "geoblock_status": "ok",
                "geoblock_country_code": "IE",
                "auth_status": "ok",
                "clock_status": "ok",
                "allowance_status": "ok",
                "user_stream_status": "ok",
                "available_cash_usd": 2.0,
                "legal_order_min_usd": 5.0,
                "details_json": "{\"provider\":\"polymarket\"}",
                "errors": ["low cash"]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "timestamp_ms": 1_700_000_000_000u64,
                "wallet_address": "0xwallet",
                "proxy_wallet": "0xproxy",
                "cash_available": 2.0,
                "cash_reserved_for_orders": 0.0,
                "inventory_mark_value": 0.0,
                "redeemable_value": 0.0,
                "pending_redeem_value": 0.0,
                "total_equity": 2.0,
                "allowance_available": 2.0,
                "details_json": "{\"provider\":\"polymarket\",\"user_stream_status\":\"ok\",\"open_order_count\":0,\"position_count\":0,\"redeemable_position_count\":0,\"legal_order_min_usd\":5.0}"
            })))
            .mount(&server)
            .await;

        let (_tmp, db_path) = temp_db_path();
        let db = Database::new(&db_path).unwrap();
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveReadonly;
        config.live_sidecar_url = server.uri();
        config.live_min_required_cash_usd = 1.0;
        let clock = SystemClock;

        let (db, mut bootstrap) = bootstrap_readonly_runtime(&config, db, &clock)
            .await
            .unwrap();
        let degraded_status: String = rusqlite::Connection::open(&db_path)
            .unwrap()
            .query_row(
                "SELECT status FROM live_sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(degraded_status, "readonly_degraded");

        bootstrap
            .monitor
            .apply_account_state(
                &db,
                &config,
                LiveAccountState {
                    timestamp_ms: 1_700_000_000_100u64,
                    wallet_address: Some("0xwallet".to_string()),
                    proxy_wallet: Some("0xproxy".to_string()),
                    cash_available: 92.0,
                    cash_reserved_for_orders: 0.0,
                    inventory_mark_value: 0.0,
                    redeemable_value: 0.0,
                    pending_redeem_value: 0.0,
                    total_equity: 92.0,
                    allowance_available: Some(92.0),
                    details_json: Some("{\"provider\":\"polymarket\",\"user_stream_status\":\"ok\",\"open_order_count\":0,\"position_count\":0,\"redeemable_position_count\":0,\"legal_order_min_usd\":5.0}".to_string()),
                },
            )
            .unwrap();

        let ready_status: String = rusqlite::Connection::open(&db_path)
            .unwrap()
            .query_row(
                "SELECT status FROM live_sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ready_status, "readonly_ready");
    }

    /// Verifies that hard readonly preflight failures abort startup cleanly.
    #[tokio::test]
    async fn bootstrap_readonly_runtime_fails_on_hard_preflight_blockers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/preflight"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": false,
                "mode": "live_readonly",
                "wallet_address": "0xwallet",
                "proxy_wallet": "0xproxy",
                "geoblock_status": "failed",
                "geoblock_country_code": "GB",
                "auth_status": "ok",
                "clock_status": "ok",
                "allowance_status": "ok",
                "user_stream_status": "ok",
                "available_cash_usd": 92.0,
                "legal_order_min_usd": 5.0,
                "details_json": "{\"provider\":\"polymarket\"}",
                "errors": ["blocked"]
            })))
            .mount(&server)
            .await;

        let (_tmp, db_path) = temp_db_path();
        let db = Database::new(&db_path).unwrap();
        let mut config = Config::default();
        config.execution_mode = ExecutionMode::LiveReadonly;
        config.live_sidecar_url = server.uri();
        let clock = SystemClock;

        let error = bootstrap_readonly_runtime(&config, db, &clock)
            .await
            .err()
            .expect("expected hard preflight failure")
            .to_string();

        assert!(error.contains("hard preflight failures"));
    }
}
