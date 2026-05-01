use std::str::FromStr;

use anyhow::{Context, bail};
use serde_json::json;

use crate::db::database::Database;
use crate::types::{ControlAuditEntry, LiveControlCommand, LiveControlState};

/// Durable live-control actions accepted by the local operator CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveControlAction {
    Preflight,
    Arm,
    Disarm,
    StopAfterFlat,
    KillSwitch,
    CancelAll,
    RedeemAll,
}

impl LiveControlAction {
    /// Return the stable command label stored in the audit ledger.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::Arm => "arm",
            Self::Disarm => "disarm",
            Self::StopAfterFlat => "stop-after-flat",
            Self::KillSwitch => "kill-switch",
            Self::CancelAll => "cancel-all",
            Self::RedeemAll => "redeem-all",
        }
    }
}

impl FromStr for LiveControlAction {
    type Err = anyhow::Error;

    /// Parse one operator action from the CLI spelling.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "preflight" => Ok(Self::Preflight),
            "arm" => Ok(Self::Arm),
            "disarm" => Ok(Self::Disarm),
            "stop-after-flat" => Ok(Self::StopAfterFlat),
            "kill-switch" => Ok(Self::KillSwitch),
            "cancel-all" => Ok(Self::CancelAll),
            "redeem-all" => Ok(Self::RedeemAll),
            _ => bail!(
                "invalid live-control action {value}; expected preflight, arm, disarm, stop-after-flat, kill-switch, cancel-all, or redeem-all"
            ),
        }
    }
}

/// Queue one live-control command against the active live-trading session.
pub fn enqueue_live_control_command(
    db: &Database,
    action: LiveControlAction,
    actor: &str,
    reason: &str,
    now_ms: u64,
) -> anyhow::Result<i64> {
    validate_operator_text("actor", actor)?;
    validate_operator_text("reason", reason)?;
    let session_id = latest_active_live_trading_session_id(db)?;
    let details_json = json!({
        "source": "cli",
        "action": action.as_str(),
    })
    .to_string();
    let command_id = db.insert_live_control_command(&LiveControlCommand {
        id: None,
        session_id,
        action: action.as_str().to_string(),
        actor: actor.to_string(),
        reason: reason.to_string(),
        requested_at_ms: now_ms,
        applied_at_ms: None,
        status: "pending".to_string(),
        details_json: Some(details_json),
    })?;
    db.log_control_audit(&ControlAuditEntry {
        id: None,
        timestamp_ms: now_ms,
        actor: actor.to_string(),
        action: "live_control_requested".to_string(),
        target: Some(format!("live_control_commands:{command_id}")),
        details_json: Some(
            json!({
                "session_id": session_id,
                "command_id": command_id,
                "action": action.as_str(),
                "reason": reason,
            })
            .to_string(),
        ),
    })?;
    Ok(command_id)
}

/// Persist one live-control state transition and its audit event.
pub fn record_live_control_state(
    db: &Database,
    session_id: i64,
    state: &str,
    actor: &str,
    reason: &str,
    now_ms: u64,
    details_json: Option<&str>,
) -> anyhow::Result<i64> {
    validate_operator_text("actor", actor)?;
    validate_operator_text("reason", reason)?;
    validate_operator_text("state", state)?;
    let state_id = db.insert_live_control_state(&LiveControlState {
        id: None,
        session_id,
        state: state.to_string(),
        updated_at_ms: now_ms,
        actor: actor.to_string(),
        reason: reason.to_string(),
        details_json: details_json.map(str::to_string),
    })?;
    db.log_control_audit(&ControlAuditEntry {
        id: None,
        timestamp_ms: now_ms,
        actor: actor.to_string(),
        action: "live_control_state_changed".to_string(),
        target: Some(format!("live_control_state:{state_id}")),
        details_json: Some(
            json!({
                "session_id": session_id,
                "state": state,
                "reason": reason,
                "details": details_json,
            })
            .to_string(),
        ),
    })?;
    Ok(state_id)
}

/// Return the active live-trading session ID or fail closed.
fn latest_active_live_trading_session_id(db: &Database) -> anyhow::Result<i64> {
    let session = db
        .latest_active_live_session()?
        .context("no active live session found for live-control command")?;
    if session.execution_mode != "live_trading" {
        bail!(
            "active live session is {}; live-control requires live_trading",
            session.execution_mode
        );
    }
    session
        .id
        .context("active live session is missing its database id")
}

/// Reject empty operator text before it reaches the audit ledger.
fn validate_operator_text(field: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty")
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/live_control_tests.rs"]
mod tests;
