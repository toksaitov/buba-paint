use serde::Serialize;

use crate::config::{Config, ExecutionMode};
use crate::db::database::Database;

/// Sanitized runtime configuration snapshot persisted at bot startup.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeConfigSnapshot {
    pub execution_mode: &'static str,
    pub runtime_name: Option<String>,
    pub process_start_time_ms: u64,
    pub db_path_label: String,
    pub config_fingerprint: String,
    pub git_sha: &'static str,
    pub package_version: &'static str,

    pub feed_event_storage_profile: &'static str,
    pub live_runtime_max_db_bytes: u64,

    pub latency_arb: LatencyArbParams,
    pub spread_capture: SpreadCaptureParams,
    pub calm_persistence: CalmPersistenceParams,

    pub risk: RiskKnobs,
    pub pending_settlement: PendingSettlementKnobs,
    pub fees: FeeKnobs,
    pub feed_freshness: FeedFreshnessKnobs,
    pub worker_budgets: WorkerBudgets,
}

/// Latency-arb strategy knobs.
#[derive(Debug, Clone, Serialize)]
pub struct LatencyArbParams {
    pub enabled: bool,
    pub momentum_threshold: f64,
    pub min_ask: f64,
    pub max_ask: f64,
    pub cooldown_ms: u64,
    pub adaptive_window_ms: u64,
    pub max_position_fraction: Option<f64>,
}

/// Spread-capture strategy knobs.
#[derive(Debug, Clone, Serialize)]
pub struct SpreadCaptureParams {
    pub enabled: bool,
    pub threshold: f64,
    pub min_ask: f64,
    pub max_leg_skew_ms: u64,
    pub max_quote_churn_per_s: f64,
    pub max_position_fraction: Option<f64>,
}

/// Calm-persistence strategy knobs.
#[derive(Debug, Clone, Serialize)]
pub struct CalmPersistenceParams {
    pub enabled: bool,
    pub min_window_time_ms: u64,
    pub max_window_time_ms: u64,
    pub max_ask: f64,
    pub min_abs_distance_bps: f64,
    pub distance_vol_ratio_threshold: f64,
    pub max_realized_vol_15s_bps: f64,
    pub max_open_crosses_30s: u32,
    pub max_quote_churn_per_s: f64,
    pub min_alignment_fraction: f64,
    pub max_fair_bias: f64,
    pub min_expected_edge: f64,
    pub max_position_fraction: Option<f64>,
}

/// Risk and bankroll knobs.
#[derive(Debug, Clone, Serialize)]
pub struct RiskKnobs {
    pub starting_balance: f64,
    pub max_position_fraction: f64,
    pub min_bet_usd: f64,
    pub max_drawdown_pct: f64,
    pub live_session_cash_cap_usd: f64,
    pub live_max_single_order_usd: f64,
    pub live_max_open_notional_usd: f64,
    pub live_max_daily_loss_usd: f64,
    pub live_max_session_drawdown_usd: f64,
    pub live_min_required_cash_usd: f64,
}

/// Pending settlement reserve knobs.
#[derive(Debug, Clone, Serialize)]
pub struct PendingSettlementKnobs {
    pub family_reserve_fraction: f64,
    pub global_reserve_fraction: f64,
    pub counts_as_open_position: bool,
}

/// Fee and paper-assumption knobs.
#[derive(Debug, Clone, Serialize)]
pub struct FeeKnobs {
    pub taker_fee_rate: f64,
    pub taker_fee_exponent: u32,
    pub sim_order_latency_ms: u64,
}

/// Feed freshness gates.
#[derive(Debug, Clone, Serialize)]
pub struct FeedFreshnessKnobs {
    pub max_book_staleness_ms: u64,
    pub max_signal_feed_age_ms: u64,
    pub max_quote_age_ms: u64,
    pub clob_no_message_reconnect_ms: u64,
    pub binance_no_message_reconnect_ms: u64,
}

/// Worker queue and budget knobs.
#[derive(Debug, Clone, Serialize)]
pub struct WorkerBudgets {
    pub feed_event_writer_queue_capacity: u64,
    pub feed_event_writer_batch_size: u64,
    pub feed_event_writer_flush_ms: u64,
    pub feed_event_writer_max_lag_ms: u64,
    pub clob_replay_block_max_rows: u64,
    pub clob_replay_block_max_ms: u64,
    pub clob_replay_block_zstd_level: i32,
    pub live_decision_queue_capacity: u64,
    pub live_decision_output_queue_capacity: u64,
    pub live_runtime_persistence_queue_capacity: u64,
    pub live_submission_queue_capacity: u64,
    pub max_live_decision_age_ms: u64,
    pub worker_shutdown_timeout_ms: u64,
}

/// Build the sanitized snapshot from runtime configuration.
#[must_use]
pub fn build_runtime_config_snapshot(config: &Config, now_ms: u64) -> RuntimeConfigSnapshot {
    RuntimeConfigSnapshot {
        execution_mode: config.execution_mode.as_str(),
        runtime_name: resolve_runtime_name(),
        process_start_time_ms: now_ms,
        db_path_label: config.db_path.clone(),
        config_fingerprint: snapshot_config_fingerprint(config),
        git_sha: option_env!("BUBA_GIT_SHA").unwrap_or("unknown"),
        package_version: env!("CARGO_PKG_VERSION"),

        feed_event_storage_profile: config.feed_event_storage_profile.as_str(),
        live_runtime_max_db_bytes: config.live_runtime_max_db_bytes,

        latency_arb: LatencyArbParams {
            enabled: config.latency_arb_enabled,
            momentum_threshold: config.latency_arb_momentum_threshold,
            min_ask: config.latency_arb_min_ask,
            max_ask: config.latency_arb_max_ask,
            cooldown_ms: config.latency_arb_cooldown_ms,
            adaptive_window_ms: config.latency_arb_adaptive_window_ms,
            max_position_fraction: config.latency_arb_max_position_fraction,
        },
        spread_capture: SpreadCaptureParams {
            enabled: config.spread_capture_enabled,
            threshold: config.spread_capture_threshold,
            min_ask: config.spread_capture_min_ask,
            max_leg_skew_ms: config.spread_capture_max_leg_skew_ms,
            max_quote_churn_per_s: config.spread_capture_max_quote_churn_per_s,
            max_position_fraction: config.spread_capture_max_position_fraction,
        },
        calm_persistence: CalmPersistenceParams {
            enabled: config.calm_persistence_enabled,
            min_window_time_ms: config.calm_persistence_min_window_time_ms,
            max_window_time_ms: config.calm_persistence_max_window_time_ms,
            max_ask: config.calm_persistence_max_ask,
            min_abs_distance_bps: config.calm_persistence_min_abs_distance_bps,
            distance_vol_ratio_threshold: config.calm_persistence_distance_vol_ratio_threshold,
            max_realized_vol_15s_bps: config.calm_persistence_max_realized_vol_15s_bps,
            max_open_crosses_30s: config.calm_persistence_max_open_crosses_30s,
            max_quote_churn_per_s: config.calm_persistence_max_quote_churn_per_s,
            min_alignment_fraction: config.calm_persistence_min_alignment_fraction,
            max_fair_bias: config.calm_persistence_max_fair_bias,
            min_expected_edge: config.calm_persistence_min_expected_edge,
            max_position_fraction: config.calm_persistence_max_position_fraction,
        },

        risk: RiskKnobs {
            starting_balance: config.starting_balance,
            max_position_fraction: config.max_position_fraction,
            min_bet_usd: config.min_bet_usd,
            max_drawdown_pct: config.max_drawdown_pct,
            live_session_cash_cap_usd: config.live_session_cash_cap_usd,
            live_max_single_order_usd: config.live_max_single_order_usd,
            live_max_open_notional_usd: config.live_max_open_notional_usd,
            live_max_daily_loss_usd: config.live_max_daily_loss_usd,
            live_max_session_drawdown_usd: config.live_max_session_drawdown_usd,
            live_min_required_cash_usd: config.live_min_required_cash_usd,
        },
        pending_settlement: PendingSettlementKnobs {
            family_reserve_fraction: config.pending_settlement_family_reserve_fraction,
            global_reserve_fraction: config.pending_settlement_global_reserve_fraction,
            counts_as_open_position: config.pending_settlement_counts_as_open_position,
        },
        fees: FeeKnobs {
            taker_fee_rate: config.taker_fee_rate,
            taker_fee_exponent: config.taker_fee_exponent,
            sim_order_latency_ms: config.sim_order_latency_ms,
        },
        feed_freshness: FeedFreshnessKnobs {
            max_book_staleness_ms: config.max_book_staleness_ms,
            max_signal_feed_age_ms: config.max_signal_feed_age_ms,
            max_quote_age_ms: config.max_quote_age_ms,
            clob_no_message_reconnect_ms: config.clob_no_message_reconnect_ms,
            binance_no_message_reconnect_ms: config.binance_no_message_reconnect_ms,
        },
        worker_budgets: WorkerBudgets {
            feed_event_writer_queue_capacity: config.feed_event_writer_queue_capacity as u64,
            feed_event_writer_batch_size: config.feed_event_writer_batch_size as u64,
            feed_event_writer_flush_ms: config.feed_event_writer_flush_ms,
            feed_event_writer_max_lag_ms: config.feed_event_writer_max_lag_ms,
            clob_replay_block_max_rows: config.clob_replay_block_max_rows as u64,
            clob_replay_block_max_ms: config.clob_replay_block_max_ms,
            clob_replay_block_zstd_level: config.clob_replay_block_zstd_level,
            live_decision_queue_capacity: config.live_decision_queue_capacity as u64,
            live_decision_output_queue_capacity: config.live_decision_output_queue_capacity as u64,
            live_runtime_persistence_queue_capacity: config.live_runtime_persistence_queue_capacity
                as u64,
            live_submission_queue_capacity: config.live_submission_queue_capacity as u64,
            max_live_decision_age_ms: config.max_live_decision_age_ms,
            worker_shutdown_timeout_ms: config.worker_shutdown_timeout_ms,
        },
    }
}

/// Persist the sanitized snapshot under the `runtime_config_snapshot` `run_metadata` key.
pub fn persist_runtime_config_snapshot(
    db: &Database,
    config: &Config,
    now_ms: u64,
) -> anyhow::Result<()> {
    let snapshot = build_runtime_config_snapshot(config, now_ms);
    let value = serde_json::to_string(&snapshot)?;
    db.set_run_metadata("runtime_config_snapshot", &value, now_ms)?;
    Ok(())
}

/// Resolve the human runtime label from environment, falling back to the runtime dir basename.
fn resolve_runtime_name() -> Option<String> {
    if let Ok(name) = std::env::var("BUBA_RUNTIME_NAME") {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Ok(dir) = std::env::var("BUBA_RUNTIME_DIR") {
        let trimmed = dir.trim_end_matches('/');
        if let Some(idx) = trimmed.rfind('/') {
            let basename = &trimmed[idx + 1..];
            if !basename.is_empty() {
                return Some(basename.to_string());
            }
        } else if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Compute the snapshot's `config_fingerprint`, matching the value `live_sessions.config_fingerprint` carries for the current mode.
fn snapshot_config_fingerprint(config: &Config) -> String {
    match config.execution_mode {
        ExecutionMode::LiveTrading => crate::live::live_trading_config_fingerprint(config),
        _ => crate::live_readonly::readonly_config_fingerprint(config),
    }
}

#[cfg(test)]
#[path = "tests/runtime_config_snapshot_tests.rs"]
mod tests;
