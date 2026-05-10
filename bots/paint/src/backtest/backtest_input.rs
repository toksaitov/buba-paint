use anyhow::{Context, bail};
use rusqlite::OpenFlags;

use crate::backtest::replay_quality::{ReplayQualityReport, blocking_error};
use crate::backtest::tick_replay::TickReplay;
use crate::backtest::window_manager::WindowManager;

const DRY_RUN_MS: u64 = 60_000;

/// Classification for an interval's end-to-end backtest readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktestInputClass {
    /// The interval has replay-grade feed rows and loadable settled windows.
    BacktestReady,
    /// The interval is missing at least one requirement for backtesting.
    NotReady,
}

impl BacktestInputClass {
    /// Return the stable report label for this class.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BacktestReady => "backtest_ready",
            Self::NotReady => "not_backtest_ready",
        }
    }
}

/// End-to-end readiness report for using one DB interval in a backtest or sweep.
#[derive(Debug, Clone)]
pub struct BacktestInputReport {
    /// Start of the validated interval in milliseconds.
    pub start_time: u64,
    /// End of the validated interval in milliseconds.
    pub end_time: u64,
    /// Final end-to-end readiness class.
    pub class: BacktestInputClass,
    /// Public replay-quality report for the same interval.
    pub replay_quality: ReplayQualityReport,
    /// Count of settled windows loaded by the backtester.
    pub settled_windows: usize,
    /// Settled windows whose open price could not be stored or derived.
    pub missing_open_prices: usize,
    /// Settled windows whose close price could not be stored or derived.
    pub missing_close_prices: usize,
    /// Ticks loaded by the bounded dry-run replay.
    pub dry_run_ticks: usize,
    /// Actionable blockers that prevent backtesting.
    pub blockers: Vec<String>,
}

impl BacktestInputReport {
    /// Return whether the interval is ready for trusted backtesting.
    #[must_use]
    pub fn is_backtest_ready(&self) -> bool {
        self.class == BacktestInputClass::BacktestReady
    }
}

/// Analyze one open `SQLite` connection for end-to-end backtest readiness.
pub fn analyze_connection(
    conn: &rusqlite::Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<BacktestInputReport> {
    let replay_quality =
        crate::backtest::replay_quality::analyze_connection(conn, start_time, end_time)?;
    let mut blockers = Vec::new();
    if !replay_quality.is_sweep_grade() {
        blockers.push(blocking_error(&replay_quality));
    }

    let mut settled_windows = 0;
    let mut missing_open_prices = 0;
    let mut missing_close_prices = 0;
    match WindowManager::new(conn, start_time, end_time) {
        Ok(window_manager) => {
            settled_windows = window_manager.total_windows();
            missing_open_prices = window_manager
                .settlements()
                .iter()
                .filter(|window| window.open_price <= 0.0)
                .count();
            missing_close_prices = window_manager
                .settlements()
                .iter()
                .filter(|window| window.close_price <= 0.0)
                .count();
        }
        Err(error) => blockers.push(format!("window_load_failed={error:#}")),
    }

    if settled_windows == 0 {
        blockers.push("no_settled_windows".to_string());
    }
    if missing_open_prices > 0 {
        blockers.push(format!("missing_open_prices={missing_open_prices}"));
    }
    if missing_close_prices > 0 {
        blockers.push(format!("missing_close_prices={missing_close_prices}"));
    }

    let dry_run_ticks = dry_run_tick_count(conn, start_time, end_time)?;
    if dry_run_ticks == 0 {
        blockers.push("dry_run_no_ticks".to_string());
    }

    let class = if blockers.is_empty() {
        BacktestInputClass::BacktestReady
    } else {
        BacktestInputClass::NotReady
    };

    Ok(BacktestInputReport {
        start_time,
        end_time,
        class,
        replay_quality,
        settled_windows,
        missing_open_prices,
        missing_close_prices,
        dry_run_ticks,
        blockers,
    })
}

/// Analyze one `SQLite` DB path for end-to-end backtest readiness.
pub fn analyze_path(
    data_path: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<BacktestInputReport> {
    let conn = rusqlite::Connection::open_with_flags(data_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening data DB: {data_path}"))?;
    analyze_connection(&conn, start_time, end_time)
}

/// Fail unless the database interval is ready for trusted backtesting.
pub fn validate_input(
    data_path: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<BacktestInputReport> {
    let report = analyze_path(data_path, start_time, end_time)?;
    if report.is_backtest_ready() {
        return Ok(report);
    }
    bail!("{}", blocking_error_for_backtest(&report));
}

/// Return a printable multiline readiness report.
#[must_use]
pub fn format_report(report: &BacktestInputReport) -> String {
    let mut lines = vec![
        format!("backtest_input={}", report.class.as_str()),
        format!("start_time={}", report.start_time),
        format!("end_time={}", report.end_time),
        format!("replay_quality={}", report.replay_quality.class.as_str()),
        format!("feed_event_rows={}", report.replay_quality.feed_event_rows),
        format!(
            "legacy_tick_rows={}",
            report.replay_quality.legacy_tick_rows
        ),
        format!("settled_windows={}", report.settled_windows),
        format!("missing_open_prices={}", report.missing_open_prices),
        format!("missing_close_prices={}", report.missing_close_prices),
        format!("dry_run_ticks={}", report.dry_run_ticks),
    ];
    for blocker in &report.blockers {
        lines.push(format!("blocker={blocker}"));
    }
    lines.join("\n")
}

/// Return a concise blocking error for non-ready inputs.
#[must_use]
pub fn blocking_error_for_backtest(report: &BacktestInputReport) -> String {
    if report.blockers.is_empty() {
        return format!("input DB is not backtest-ready: {}", report.class.as_str());
    }
    format!(
        "input DB is not backtest-ready: {}. Run `buba-paint validate-backtest-input` for details.",
        report.blockers.join(",")
    )
}

/// Load a bounded replay slice to prove that ticks can be materialized.
fn dry_run_tick_count(
    conn: &rusqlite::Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<usize> {
    let Some(first_tick) = TickReplay::first_tick_timestamp(conn, start_time, end_time)? else {
        return Ok(0);
    };
    let dry_run_end = first_tick.saturating_add(DRY_RUN_MS).min(end_time);
    let dry_run = TickReplay::from_db(conn, first_tick, dry_run_end)?;
    Ok(dry_run.total_ticks())
}

#[cfg(test)]
#[path = "tests/backtest_input_tests.rs"]
mod tests;
