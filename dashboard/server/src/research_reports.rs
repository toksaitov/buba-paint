//! Research report analysis and report-file rendering.
//!
//! This module converts completed research job outputs into durable JSON and
//! CSV report files. It keeps the database schema unchanged by returning a
//! compact JSON summary for `research_reports.summary_json` while writing the
//! full analysis payload to `report.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

use crate::db::ResearchJobStep;
use crate::error::DashboardError;
use crate::research_pipeline::ResearchPipelinePlan;
use crate::research_util::path_to_string;

/// Rendered report artifacts ready for disk and metadata persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportDocuments {
    /// Compact metadata JSON stored in the dashboard DB.
    pub summary_json: String,
    /// Full report JSON written to the report file.
    pub full_json: String,
    /// Human-readable CSV written to the report CSV file.
    pub csv: String,
}

#[derive(Debug, Clone, Serialize)]
struct ResearchReportDocument {
    schema_version: u8,
    generated_at_ms: u64,
    provenance: ReportProvenance,
    metrics: ReportMetrics,
    source_comparison: Option<SourceRunComparison>,
    equity_curve: Vec<EquityPoint>,
    drawdown_curve: Vec<DrawdownPoint>,
    rejection_reasons: Vec<RejectionReason>,
    diagnostics: Vec<String>,
    sweep: Option<SweepAnalysis>,
    steps: Vec<StepSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct ResearchReportSummary {
    schema_version: u8,
    generated_at_ms: u64,
    provenance: ReportProvenance,
    metrics: ReportMetrics,
    source_comparison: Option<SourceRunComparison>,
    diagnostics: Vec<String>,
    sweep_summary: Option<SweepSummary>,
    steps: Vec<StepSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct ReportProvenance {
    job_id: String,
    job_type: String,
    artifact_id: Option<String>,
    start: String,
    end: String,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    balance: f64,
    sets: Vec<String>,
    sweeps: Vec<String>,
    data_db_path: String,
    prepared_db_output_path: String,
    backtest_output_path: String,
    sweep_output_path: String,
    report_json_path: String,
    report_csv_path: String,
    dashboard_image_ref: Option<String>,
    research_worker_image_ref: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct ReportMetrics {
    net_pnl: Option<f64>,
    gross_pnl: Option<f64>,
    total_fees: Option<f64>,
    final_balance: Option<f64>,
    trade_count: Option<u64>,
    wins: Option<u64>,
    losses: Option<u64>,
    win_rate: Option<f64>,
    max_drawdown: Option<f64>,
    max_drawdown_pct: Option<f64>,
    signal_count: Option<u64>,
    fill_count: Option<u64>,
    no_fill_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct SourceRunComparison {
    status: String,
    source: SourceRunMetrics,
    replay: SourceRunMetrics,
    delta: SourceRunMetricDelta,
}

#[derive(Debug, Clone, Default, Serialize)]
struct SourceRunMetrics {
    net_pnl: Option<f64>,
    gross_pnl: Option<f64>,
    total_fees: Option<f64>,
    final_balance: Option<f64>,
    trade_count: Option<u64>,
    signal_count: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct SourceRunMetricDelta {
    net_pnl: Option<f64>,
    final_balance: Option<f64>,
    trade_count: Option<i64>,
    signal_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct EquityPoint {
    ts: u64,
    equity: f64,
    event: String,
    amount: f64,
}

#[derive(Debug, Clone, Serialize)]
struct DrawdownPoint {
    ts: u64,
    equity: f64,
    high_water_mark: f64,
    drawdown: f64,
    drawdown_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
struct RejectionReason {
    reason: String,
    count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct SweepAnalysis {
    columns: Vec<String>,
    parameter_columns: Vec<String>,
    metric_columns: Vec<String>,
    ranked_by: String,
    rows: Vec<SweepRow>,
    top_rows: Vec<SweepRow>,
}

#[derive(Debug, Clone, Serialize)]
struct SweepSummary {
    row_count: usize,
    parameter_columns: Vec<String>,
    metric_columns: Vec<String>,
    ranked_by: String,
    top_row: Option<SweepRow>,
}

#[derive(Debug, Clone, Serialize)]
struct SweepRow {
    rank: Option<usize>,
    params: BTreeMap<String, Value>,
    metrics: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
struct StepSummary {
    index: i64,
    name: String,
    status: String,
    attempts: i64,
    error: Option<String>,
}

/// Build compact metadata, full JSON, and CSV text for one research report.
pub fn build_report_documents(
    plan: &ResearchPipelinePlan,
    steps: &[ResearchJobStep],
) -> Result<ReportDocuments, DashboardError> {
    let mut diagnostics = Vec::new();
    let mut metrics = ReportMetrics::default();
    let mut source_comparison = None;
    let mut equity_curve = Vec::new();
    let mut drawdown_curve = Vec::new();
    let mut rejection_reasons = Vec::new();
    let mut sweep = None;

    match plan.job_type.as_str() {
        "current_params" => analyze_current_params(
            plan,
            &mut metrics,
            &mut source_comparison,
            &mut equity_curve,
            &mut drawdown_curve,
            &mut rejection_reasons,
            &mut diagnostics,
        )?,
        "sweep" => {
            let analysis = analyze_sweep(plan, &mut metrics, &mut diagnostics)?;
            sweep = Some(analysis);
        }
        other => diagnostics.push(format!("analysis_unsupported_job_type={other}")),
    }

    let document = ResearchReportDocument {
        schema_version: 2,
        generated_at_ms: current_epoch_ms(),
        provenance: provenance(plan),
        metrics,
        source_comparison,
        equity_curve,
        drawdown_curve,
        rejection_reasons,
        diagnostics,
        sweep,
        steps: step_summaries(steps),
    };
    let summary = summary_from_document(&document);
    let full_json = serde_json::to_string_pretty(&document)
        .map_err(|error| DashboardError::Internal(format!("serializing report JSON: {error}")))?;
    let summary_json = serde_json::to_string_pretty(&summary).map_err(|error| {
        DashboardError::Internal(format!("serializing report summary: {error}"))
    })?;
    let csv = report_csv(&document);
    Ok(ReportDocuments {
        summary_json,
        full_json,
        csv,
    })
}

/// Insert a top-level field into an existing full report JSON file.
pub fn append_report_json_field<T: Serialize>(
    report_path: &Path,
    field_name: &str,
    field_value: &T,
) -> Result<(), DashboardError> {
    let text = std::fs::read_to_string(report_path)
        .map_err(|error| DashboardError::Internal(format!("reading report JSON: {error}")))?;
    let mut value: Value = serde_json::from_str(&text)
        .map_err(|error| DashboardError::Internal(format!("parsing report JSON: {error}")))?;
    value[field_name] = serde_json::to_value(field_value)
        .map_err(|error| DashboardError::Internal(format!("serializing report field: {error}")))?;
    let text = serde_json::to_string_pretty(&value)
        .map_err(|error| DashboardError::Internal(format!("serializing report JSON: {error}")))?;
    std::fs::write(report_path, text)
        .map_err(|error| DashboardError::Internal(format!("writing report JSON: {error}")))
}

/// Return true when the planned analysis source file exists.
pub fn report_analysis_source_exists(plan: &ResearchPipelinePlan) -> bool {
    match plan.job_type.as_str() {
        "current_params" => plan.backtest_output_path.exists(),
        "sweep" => plan.sweep_output_path.exists(),
        _ => false,
    }
}

/// Analyze a current-parameters backtest output database.
fn analyze_current_params(
    plan: &ResearchPipelinePlan,
    metrics: &mut ReportMetrics,
    source_comparison: &mut Option<SourceRunComparison>,
    equity_curve: &mut Vec<EquityPoint>,
    drawdown_curve: &mut Vec<DrawdownPoint>,
    rejection_reasons: &mut Vec<RejectionReason>,
    diagnostics: &mut Vec<String>,
) -> Result<(), DashboardError> {
    if !plan.backtest_output_path.exists() {
        diagnostics.push("analysis_missing_backtest_db".to_string());
        return Ok(());
    }
    let conn = Connection::open_with_flags(
        &plan.backtest_output_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| DashboardError::Internal(format!("opening backtest DB: {error}")))?;

    analyze_trade_metrics(&conn, plan.balance, metrics, diagnostics)?;
    *equity_curve = read_equity_curve(&conn, parse_time_ms(&plan.start))?;
    *drawdown_curve = drawdowns(equity_curve);
    apply_drawdown_metrics(metrics, drawdown_curve);
    metrics.signal_count = count_table_rows(&conn, "signals")?;
    apply_fill_metrics(&conn, metrics)?;
    *rejection_reasons = read_rejection_reasons(&conn)?;
    *source_comparison = compare_source_run(plan, metrics, diagnostics)?;
    if metrics.trade_count == Some(0) {
        diagnostics.push("no_trades".to_string());
    }
    if metrics.signal_count == Some(0) {
        diagnostics.push("no_signals".to_string());
    }
    if equity_curve.is_empty() {
        diagnostics.push("no_equity_curve".to_string());
    }
    if rejection_reasons.is_empty() {
        diagnostics.push("no_rejection_reasons".to_string());
    }
    Ok(())
}

/// Compare replay output against source-run results when the source DB has them.
fn compare_source_run(
    plan: &ResearchPipelinePlan,
    replay: &ReportMetrics,
    diagnostics: &mut Vec<String>,
) -> Result<Option<SourceRunComparison>, DashboardError> {
    if !plan.data_db_path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(
        &plan.data_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| DashboardError::Internal(format!("opening source DB: {error}")))?;
    let source = read_source_run_metrics(&conn, plan)?;
    if source.net_pnl.is_none() && source.trade_count.is_none() && source.signal_count.is_none() {
        return Ok(None);
    }
    let replay = SourceRunMetrics::from_report_metrics(replay);
    let delta = SourceRunMetricDelta {
        net_pnl: metric_delta(replay.net_pnl, source.net_pnl),
        final_balance: metric_delta(replay.final_balance, source.final_balance),
        trade_count: count_delta(replay.trade_count, source.trade_count),
        signal_count: count_delta(replay.signal_count, source.signal_count),
    };
    let status = if source_run_mismatch(&delta) {
        push_diagnostic(diagnostics, "source_replay_result_mismatch");
        "mismatch"
    } else {
        "matched"
    };
    Ok(Some(SourceRunComparison {
        status: status.to_string(),
        source,
        replay,
        delta,
    }))
}

/// Read source-run metrics from the artifact DB over the requested interval.
fn read_source_run_metrics(
    conn: &Connection,
    plan: &ResearchPipelinePlan,
) -> Result<SourceRunMetrics, DashboardError> {
    let start_ms = parse_time_ms(&plan.start);
    let end_ms = parse_time_ms(&plan.end);
    let (trade_count, net_pnl, total_fees) = read_source_trade_metrics(conn, start_ms, end_ms)?;
    let signal_count = read_time_window_count(conn, "signals", "timestamp", start_ms, end_ms)?;
    let gross_pnl = metric_sum(net_pnl, total_fees);
    let final_balance = net_pnl.map(|value| plan.balance + value);
    Ok(SourceRunMetrics {
        net_pnl,
        gross_pnl,
        total_fees,
        final_balance,
        trade_count,
        signal_count,
    })
}

/// Source trade aggregate columns: trade count, net `PnL` sum, and fee sum.
type SourceTradeMetrics = (Option<u64>, Option<f64>, Option<f64>);

/// Read trade metrics from a source DB, filtering by settlement time when possible.
fn read_source_trade_metrics(
    conn: &Connection,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<SourceTradeMetrics, DashboardError> {
    if !table_exists(conn, "trade_results")? || !column_exists(conn, "trade_results", "pnl_net")? {
        return Ok((None, None, None));
    }
    let has_fee = column_exists(conn, "trade_results", "fee_amount")?;
    let fee_expr = if has_fee {
        "COALESCE(SUM(fee_amount), 0.0)"
    } else {
        "0.0"
    };
    let predicate = source_time_predicate(conn, "trade_results", "resolved_at", start_ms, end_ms)?;
    let query = format!(
        "SELECT COUNT(*), COALESCE(SUM(pnl_net), 0.0), {fee_expr} FROM trade_results{predicate}",
    );
    let params = source_time_params(&predicate, start_ms, end_ms);
    let row = conn
        .query_row(&query, rusqlite::params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .map_err(|error| {
            DashboardError::Internal(format!("reading source trade metrics: {error}"))
        })?;
    Ok((
        Some(u64::try_from(row.0).unwrap_or(0)),
        Some(row.1),
        Some(row.2),
    ))
}

/// Count source rows inside the requested interval when the time column exists.
fn read_time_window_count(
    conn: &Connection,
    table: &str,
    column: &str,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<Option<u64>, DashboardError> {
    if !table_exists(conn, table)? {
        return Ok(None);
    }
    let predicate = source_time_predicate(conn, table, column, start_ms, end_ms)?;
    let query = format!("SELECT COUNT(*) FROM {}{predicate}", sqlite_ident(table));
    let params = source_time_params(&predicate, start_ms, end_ms);
    let count = conn
        .query_row(&query, rusqlite::params_from_iter(params.iter()), |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| DashboardError::Internal(format!("counting source rows: {error}")))?;
    Ok(Some(u64::try_from(count).unwrap_or(0)))
}

/// Return a trusted source DB time predicate when both bounds and column exist.
fn source_time_predicate(
    conn: &Connection,
    table: &str,
    column: &str,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<String, DashboardError> {
    if start_ms.is_some() && end_ms.is_some() && column_exists(conn, table, column)? {
        Ok(format!(" WHERE {} BETWEEN ?1 AND ?2", sqlite_ident(column)))
    } else {
        Ok(String::new())
    }
}

/// Return source time predicate parameters when both bounds are present.
fn source_time_params(predicate: &str, start_ms: Option<u64>, end_ms: Option<u64>) -> Vec<i64> {
    if predicate.is_empty() {
        return Vec::new();
    }
    let (Some(start_ms), Some(end_ms)) = (start_ms, end_ms) else {
        return Vec::new();
    };
    vec![
        i64::try_from(start_ms).unwrap_or(i64::MAX),
        i64::try_from(end_ms).unwrap_or(i64::MAX),
    ]
}

/// Return whether source-run and replay metrics differ enough to warn.
fn source_run_mismatch(delta: &SourceRunMetricDelta) -> bool {
    delta.net_pnl.is_some_and(|value| value.abs() > 0.01)
        || delta.final_balance.is_some_and(|value| value.abs() > 0.01)
        || delta.trade_count.is_some_and(|value| value != 0)
        || delta.signal_count.is_some_and(|value| value != 0)
}

/// Return an optional numeric difference.
fn metric_delta(replay: Option<f64>, source: Option<f64>) -> Option<f64> {
    Some(replay? - source?)
}

/// Return an optional count difference.
fn count_delta(replay: Option<u64>, source: Option<u64>) -> Option<i64> {
    let replay = i64::try_from(replay?).ok()?;
    let source = i64::try_from(source?).ok()?;
    Some(replay - source)
}

/// Return an optional numeric sum.
fn metric_sum(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    Some(left? + right?)
}

/// Push one diagnostic only once.
fn push_diagnostic(diagnostics: &mut Vec<String>, diagnostic: &str) {
    if !diagnostics.iter().any(|value| value == diagnostic) {
        diagnostics.push(diagnostic.to_string());
    }
}

impl SourceRunMetrics {
    /// Build replay comparison metrics from report metrics.
    fn from_report_metrics(metrics: &ReportMetrics) -> Self {
        Self {
            net_pnl: metrics.net_pnl,
            gross_pnl: metrics.gross_pnl,
            total_fees: metrics.total_fees,
            final_balance: metrics.final_balance,
            trade_count: metrics.trade_count,
            signal_count: metrics.signal_count,
        }
    }
}

/// Compute trade-level metrics from `trade_results`.
fn analyze_trade_metrics(
    conn: &Connection,
    starting_balance: f64,
    metrics: &mut ReportMetrics,
    diagnostics: &mut Vec<String>,
) -> Result<(), DashboardError> {
    if !table_exists(conn, "trade_results")? {
        diagnostics.push("missing_trade_results_table".to_string());
        metrics.trade_count = Some(0);
        metrics.final_balance = Some(starting_balance);
        return Ok(());
    }
    let row = conn
        .query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(pnl_net), 0.0),
                COALESCE(SUM(fee_amount), 0.0),
                COALESCE(SUM(CASE WHEN pnl_net > 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN pnl_net <= 0 THEN 1 ELSE 0 END), 0)
             FROM trade_results",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .map_err(|error| DashboardError::Internal(format!("reading trade metrics: {error}")))?;
    let trade_count = u64::try_from(row.0).unwrap_or(0);
    let wins = u64::try_from(row.3).unwrap_or(0);
    let losses = u64::try_from(row.4).unwrap_or(0);
    metrics.trade_count = Some(trade_count);
    metrics.net_pnl = Some(row.1);
    metrics.total_fees = Some(row.2);
    metrics.gross_pnl = Some(row.1 + row.2);
    metrics.wins = Some(wins);
    metrics.losses = Some(losses);
    metrics.win_rate = Some(if trade_count == 0 {
        0.0
    } else {
        wins as f64 / trade_count as f64
    });
    metrics.final_balance = Some(read_final_balance(conn)?.unwrap_or(starting_balance + row.1));
    Ok(())
}

/// Read the final balance from `balance_log`.
fn read_final_balance(conn: &Connection) -> Result<Option<f64>, DashboardError> {
    if !table_exists(conn, "balance_log")? {
        return Ok(None);
    }
    conn.query_row(
        "SELECT balance FROM balance_log ORDER BY timestamp DESC, id DESC LIMIT 1",
        [],
        |row| row.get::<_, f64>(0),
    )
    .optional()
    .map_err(|error| DashboardError::Internal(format!("reading final balance: {error}")))
}

/// Read the backtest balance log as an equity curve.
fn read_equity_curve(
    conn: &Connection,
    start_ms: Option<u64>,
) -> Result<Vec<EquityPoint>, DashboardError> {
    if !table_exists(conn, "balance_log")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, balance, event, amount
             FROM balance_log
             ORDER BY timestamp ASC, id ASC",
        )
        .map_err(|error| DashboardError::Internal(format!("preparing equity query: {error}")))?;
    let rows = stmt
        .query_map([], |row| {
            let raw_ts = row.get::<_, i64>(0)?;
            let event = row.get::<_, String>(2)?;
            let ts = if raw_ts <= 0 && event == "init" {
                start_ms.unwrap_or(0)
            } else {
                u64::try_from(raw_ts).unwrap_or(0)
            };
            Ok(EquityPoint {
                ts,
                equity: row.get(1)?,
                event,
                amount: row.get(3)?,
            })
        })
        .map_err(|error| DashboardError::Internal(format!("reading equity curve: {error}")))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|error| {
            DashboardError::Internal(format!("mapping equity curve row: {error}"))
        })?);
    }
    Ok(out)
}

/// Derive drawdown points from an equity curve.
fn drawdowns(equity_curve: &[EquityPoint]) -> Vec<DrawdownPoint> {
    let mut high_water_mark = f64::NEG_INFINITY;
    let mut out = Vec::with_capacity(equity_curve.len());
    for point in equity_curve {
        high_water_mark = high_water_mark.max(point.equity);
        let drawdown = point.equity - high_water_mark;
        let drawdown_pct = if high_water_mark > 0.0 {
            drawdown / high_water_mark
        } else {
            0.0
        };
        out.push(DrawdownPoint {
            ts: point.ts,
            equity: point.equity,
            high_water_mark,
            drawdown,
            drawdown_pct,
        });
    }
    out
}

/// Copy max drawdown values into the report metrics.
fn apply_drawdown_metrics(metrics: &mut ReportMetrics, drawdown_curve: &[DrawdownPoint]) {
    if let Some(worst) = drawdown_curve
        .iter()
        .min_by(|a, b| a.drawdown.total_cmp(&b.drawdown))
    {
        metrics.max_drawdown = Some(worst.drawdown);
        metrics.max_drawdown_pct = Some(worst.drawdown_pct);
    }
}

/// Compute fill and no-fill counts when simulated-trade columns are present.
fn apply_fill_metrics(
    conn: &Connection,
    metrics: &mut ReportMetrics,
) -> Result<(), DashboardError> {
    if !table_exists(conn, "simulated_trades")? {
        return Ok(());
    }
    if column_exists(conn, "simulated_trades", "fill_status")? {
        metrics.fill_count = Some(count_where(
            conn,
            "simulated_trades",
            "fill_status IN ('filled', 'partial')",
        )?);
        metrics.no_fill_count = Some(count_where(
            conn,
            "simulated_trades",
            "fill_status = 'no_fill'",
        )?);
    }
    Ok(())
}

/// Read the top rejection reasons from `strategy_rejection_summaries`.
fn read_rejection_reasons(conn: &Connection) -> Result<Vec<RejectionReason>, DashboardError> {
    if !table_exists(conn, "strategy_rejection_summaries")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT reason, SUM(count) AS total
             FROM strategy_rejection_summaries
             GROUP BY reason
             ORDER BY total DESC, reason ASC
             LIMIT 10",
        )
        .map_err(|error| {
            DashboardError::Internal(format!("preparing rejection summary query: {error}"))
        })?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RejectionReason {
                reason: row.get(0)?,
                count: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
            })
        })
        .map_err(|error| DashboardError::Internal(format!("reading rejection reasons: {error}")))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|error| {
            DashboardError::Internal(format!("mapping rejection reason: {error}"))
        })?);
    }
    Ok(out)
}

/// Analyze one sweep CSV output file.
fn analyze_sweep(
    plan: &ResearchPipelinePlan,
    metrics: &mut ReportMetrics,
    diagnostics: &mut Vec<String>,
) -> Result<SweepAnalysis, DashboardError> {
    if !plan.sweep_output_path.exists() {
        diagnostics.push("analysis_missing_sweep_csv".to_string());
        return Ok(empty_sweep_analysis());
    }
    let text = std::fs::read_to_string(&plan.sweep_output_path)
        .map_err(|error| DashboardError::Internal(format!("reading sweep CSV: {error}")))?;
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        diagnostics.push("sweep_csv_empty".to_string());
        return Ok(empty_sweep_analysis());
    };
    let columns = split_csv_line(header);
    if columns.is_empty() {
        diagnostics.push("sweep_csv_missing_header".to_string());
        return Ok(empty_sweep_analysis());
    }
    let metric_names = sweep_metric_names();
    let parameter_columns = columns
        .iter()
        .filter(|name| !metric_names.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let metric_columns = columns
        .iter()
        .filter(|name| metric_names.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !metric_columns.iter().any(|name| name == "pnl") {
        diagnostics.push("sweep_csv_missing_pnl_metric".to_string());
    }
    let mut rows = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let values = split_csv_line(line);
        if values.len() != columns.len() {
            diagnostics.push("sweep_csv_malformed_row".to_string());
            continue;
        }
        rows.push(sweep_row(
            &columns,
            &parameter_columns,
            &metric_columns,
            &values,
        ));
    }
    let ranked_by = if metric_columns.iter().any(|name| name == "calibrated_pnl") {
        push_diagnostic(diagnostics, "sweep_ranked_by_calibrated_pnl");
        "calibrated_pnl"
    } else {
        "pnl"
    };
    rows.sort_by(|a, b| numeric_metric(b, ranked_by).total_cmp(&numeric_metric(a, ranked_by)));
    for (index, row) in rows.iter_mut().enumerate() {
        row.rank = Some(index + 1);
    }
    if let Some(top) = rows.first() {
        apply_sweep_metrics(metrics, top, ranked_by);
        if string_metric(top, "calibration_confidence").as_deref() == Some("low") {
            push_diagnostic(diagnostics, "sweep_calibration_confidence_low");
        }
    }
    if rows.is_empty() {
        diagnostics.push("sweep_csv_no_rows".to_string());
    }
    Ok(SweepAnalysis {
        columns,
        parameter_columns,
        metric_columns,
        ranked_by: ranked_by.to_string(),
        top_rows: rows.iter().take(10).cloned().collect(),
        rows,
    })
}

/// Return an empty sweep analysis placeholder.
fn empty_sweep_analysis() -> SweepAnalysis {
    SweepAnalysis {
        columns: Vec::new(),
        parameter_columns: Vec::new(),
        metric_columns: Vec::new(),
        ranked_by: "pnl".to_string(),
        rows: Vec::new(),
        top_rows: Vec::new(),
    }
}

/// Build one typed sweep row.
fn sweep_row(
    columns: &[String],
    parameter_columns: &[String],
    metric_columns: &[String],
    values: &[String],
) -> SweepRow {
    let parameter_set = parameter_columns.iter().cloned().collect::<BTreeSet<_>>();
    let metric_set = metric_columns.iter().cloned().collect::<BTreeSet<_>>();
    let mut params = BTreeMap::new();
    let mut metrics = BTreeMap::new();
    for (name, raw) in columns.iter().zip(values) {
        let value = csv_value(raw);
        if parameter_set.contains(name) {
            params.insert(name.clone(), value);
        } else if metric_set.contains(name) {
            metrics.insert(name.clone(), value);
        }
    }
    SweepRow {
        rank: None,
        params,
        metrics,
    }
}

/// Apply top sweep-row values to summary metrics.
fn apply_sweep_metrics(metrics: &mut ReportMetrics, top: &SweepRow, ranked_by: &str) {
    metrics.net_pnl = metric_value(top, ranked_by).or_else(|| metric_value(top, "pnl"));
    metrics.gross_pnl = metric_value(top, "gross_pnl");
    metrics.total_fees = metric_value(top, "total_fees");
    metrics.final_balance = metric_value(top, "calibrated_final_balance")
        .or_else(|| metric_value(top, "final_balance"));
    metrics.trade_count = metric_value(top, "trades").and_then(f64_to_u64);
    metrics.wins = metric_value(top, "wins").and_then(f64_to_u64);
    metrics.losses = metric_value(top, "losses").and_then(f64_to_u64);
    metrics.win_rate = metric_value(top, "win_rate");
    metrics.max_drawdown_pct = metric_value(top, "max_dd").map(|value| -value.abs());
    metrics.signal_count = metric_value(top, "signals").and_then(f64_to_u64);
    metrics.fill_count = metric_value(top, "fill_rate").and_then(f64_to_u64);
    metrics.no_fill_count = metric_value(top, "no_fill_count").and_then(f64_to_u64);
}

/// Convert finite whole-ish numbers to `u64`.
fn f64_to_u64(value: f64) -> Option<u64> {
    if value.is_finite() && value >= 0.0 {
        Some(value.round() as u64)
    } else {
        None
    }
}

/// Return one numeric metric from a sweep row.
fn metric_value(row: &SweepRow, name: &str) -> Option<f64> {
    row.metrics.get(name).and_then(Value::as_f64)
}

/// Return one numeric metric or negative infinity for sorting.
fn numeric_metric(row: &SweepRow, name: &str) -> f64 {
    metric_value(row, name).unwrap_or(f64::NEG_INFINITY)
}

/// Return one string metric from a sweep row.
fn string_metric(row: &SweepRow, name: &str) -> Option<String> {
    row.metrics
        .get(name)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

/// Return known sweep metric column names.
fn sweep_metric_names() -> BTreeSet<&'static str> {
    [
        "pnl",
        "win_rate",
        "trades",
        "wins",
        "losses",
        "max_dd",
        "hwm",
        "final_balance",
        "signals",
        "fill_rate",
        "partial_fill_rate",
        "no_fill_count",
        "spread_legging_count",
        "residual_position_count",
        "avg_fill_latency_ms",
        "avg_slippage",
        "raw_event_batches",
        "legacy_snapshot_batches",
        "dislocation_regime_count",
        "structural_pair_regime_count",
        "calm_regime_count",
        "dislocation_queued",
        "structural_pair_queued",
        "calm_queued",
        "dislocation_filled",
        "structural_pair_filled",
        "calm_filled",
        "dislocation_missed",
        "structural_pair_missed",
        "calm_missed",
        "latency_arb_candidates",
        "spread_capture_candidates",
        "calm_persistence_candidates",
        "router_blocked_count",
        "capital_blocked_count",
        "latency_spread_overlap_count",
        "latency_calm_overlap_count",
        "spread_calm_overlap_count",
        "total_fees",
        "gross_pnl",
        "pnl_net",
        "elapsed_s",
        "calibrated_pnl",
        "calibrated_final_balance",
        "baseline_replay_delta_pnl",
        "source_baseline_pnl",
        "baseline_replay_pnl",
        "calibration_confidence",
        "source_baseline_trades",
        "baseline_replay_trades",
        "baseline_trade_delta",
        "source_baseline_signals",
        "baseline_replay_signals",
        "baseline_signal_delta",
    ]
    .into_iter()
    .collect()
}

/// Convert one CSV cell into JSON.
fn csv_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Value::Null
    } else if let Ok(value) = trimmed.parse::<f64>() {
        serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
    } else {
        Value::String(trimmed.to_string())
    }
}

/// Split one simple CSV line.
fn split_csv_line(line: &str) -> Vec<String> {
    line.split(',')
        .map(|value| value.trim().to_string())
        .collect()
}

/// Build compact summary from a full report document.
fn summary_from_document(document: &ResearchReportDocument) -> ResearchReportSummary {
    ResearchReportSummary {
        schema_version: document.schema_version,
        generated_at_ms: document.generated_at_ms,
        provenance: document.provenance.clone(),
        metrics: document.metrics.clone(),
        source_comparison: document.source_comparison.clone(),
        diagnostics: document.diagnostics.clone(),
        sweep_summary: document.sweep.as_ref().map(|sweep| SweepSummary {
            row_count: sweep.rows.len(),
            parameter_columns: sweep.parameter_columns.clone(),
            metric_columns: sweep.metric_columns.clone(),
            ranked_by: sweep.ranked_by.clone(),
            top_row: sweep.top_rows.first().cloned(),
        }),
        steps: document.steps.clone(),
    }
}

/// Build report provenance from the pipeline plan and environment.
fn provenance(plan: &ResearchPipelinePlan) -> ReportProvenance {
    ReportProvenance {
        job_id: plan.job_id.clone(),
        job_type: plan.job_type.clone(),
        artifact_id: plan.artifact_id.clone(),
        start: plan.start.clone(),
        end: plan.end.clone(),
        start_ms: parse_time_ms(&plan.start),
        end_ms: parse_time_ms(&plan.end),
        balance: plan.balance,
        sets: plan.sets.clone(),
        sweeps: plan.sweeps.clone(),
        data_db_path: path_to_string(&plan.data_db_path),
        prepared_db_output_path: path_to_string(&plan.prepared_db_output_path),
        backtest_output_path: path_to_string(&plan.backtest_output_path),
        sweep_output_path: path_to_string(&plan.sweep_output_path),
        report_json_path: path_to_string(&plan.report_json_path),
        report_csv_path: path_to_string(&plan.report_csv_path),
        dashboard_image_ref: optional_env("BUBA_DASHBOARD_IMAGE_REF")
            .or_else(|| optional_env("BUBA_DASHBOARD_IMAGE")),
        research_worker_image_ref: optional_env("BUBA_RESEARCH_WORKER_IMAGE_REF")
            .or_else(|| optional_env("BUBA_RESEARCH_WORKER_IMAGE")),
    }
}

/// Parse an RFC3339 or millisecond string into epoch milliseconds.
fn parse_time_ms(value: &str) -> Option<u64> {
    if let Ok(ms) = value.parse::<u64>() {
        return Some(ms);
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .and_then(|dt| u64::try_from(dt.timestamp_millis()).ok())
}

/// Read an optional environment value.
fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}


/// Convert durable job steps into compact summaries.
fn step_summaries(steps: &[ResearchJobStep]) -> Vec<StepSummary> {
    steps
        .iter()
        .map(|step| StepSummary {
            index: step.step_index,
            name: step.name.clone(),
            status: step.status.clone(),
            attempts: step.attempts,
            error: step.error.clone(),
        })
        .collect()
}

/// Render a report CSV from the full document.
fn report_csv(document: &ResearchReportDocument) -> String {
    if let Some(sweep) = &document.sweep {
        return sweep_report_csv(sweep);
    }
    current_params_report_csv(document)
}

/// Render current-params metrics and equity into CSV text.
fn current_params_report_csv(document: &ResearchReportDocument) -> String {
    let mut csv = String::from("section,name,value\n");
    push_metric_row(&mut csv, "net_pnl", document.metrics.net_pnl);
    push_metric_row(&mut csv, "gross_pnl", document.metrics.gross_pnl);
    push_metric_row(&mut csv, "total_fees", document.metrics.total_fees);
    push_metric_row(&mut csv, "final_balance", document.metrics.final_balance);
    push_metric_row(
        &mut csv,
        "trade_count",
        document.metrics.trade_count.map(|value| value as f64),
    );
    push_metric_row(
        &mut csv,
        "signal_count",
        document.metrics.signal_count.map(|value| value as f64),
    );
    push_metric_row(&mut csv, "win_rate", document.metrics.win_rate);
    push_metric_row(&mut csv, "max_drawdown", document.metrics.max_drawdown);
    if let Some(comparison) = &document.source_comparison {
        push_metric_row(&mut csv, "source_net_pnl", comparison.source.net_pnl);
        push_metric_row(
            &mut csv,
            "source_final_balance",
            comparison.source.final_balance,
        );
        push_metric_row(
            &mut csv,
            "source_trade_count",
            comparison.source.trade_count.map(|value| value as f64),
        );
        push_metric_row(
            &mut csv,
            "source_signal_count",
            comparison.source.signal_count.map(|value| value as f64),
        );
        push_metric_row(&mut csv, "source_net_pnl_delta", comparison.delta.net_pnl);
        push_metric_row(
            &mut csv,
            "source_final_balance_delta",
            comparison.delta.final_balance,
        );
    }
    csv.push_str("section,timestamp,equity,drawdown,drawdown_pct\n");
    for (equity, drawdown) in document.equity_curve.iter().zip(&document.drawdown_curve) {
        let _ = writeln!(
            csv,
            "equity,{},{},{},{}",
            equity.ts, equity.equity, drawdown.drawdown, drawdown.drawdown_pct
        );
    }
    csv
}

/// Append one metric CSV row when a value exists.
fn push_metric_row(csv: &mut String, name: &str, value: Option<f64>) {
    if let Some(value) = value {
        let _ = writeln!(csv, "metric,{name},{value}");
    }
}

/// Render sweep rows into ranked CSV text.
fn sweep_report_csv(sweep: &SweepAnalysis) -> String {
    let mut columns = vec!["rank".to_string()];
    columns.extend(sweep.parameter_columns.clone());
    columns.extend(sweep.metric_columns.clone());
    let mut csv = String::new();
    csv.push_str(&columns.join(","));
    csv.push('\n');
    for row in &sweep.rows {
        let mut values = vec![row.rank.map_or(String::new(), |rank| rank.to_string())];
        values.extend(sweep.parameter_columns.iter().map(|name| {
            row.params
                .get(name)
                .map_or_else(String::new, json_value_to_csv_cell)
        }));
        values.extend(sweep.metric_columns.iter().map(|name| {
            row.metrics
                .get(name)
                .map_or_else(String::new, json_value_to_csv_cell)
        }));
        csv.push_str(&values.join(","));
        csv.push('\n');
    }
    csv
}

/// Convert a JSON scalar to one simple CSV cell.
fn json_value_to_csv_cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

/// Return whether a `SQLite` table exists.
fn table_exists(conn: &Connection, table: &str) -> Result<bool, DashboardError> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .optional()
    .map(|value| value.is_some())
    .map_err(|error| DashboardError::Internal(format!("checking table existence: {error}")))
}

/// Return whether a `SQLite` table has a column.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, DashboardError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", sqlite_ident(table)))
        .map_err(|error| DashboardError::Internal(format!("reading table info: {error}")))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| DashboardError::Internal(format!("reading columns: {error}")))?;
    for row in rows {
        if row.map_err(|error| DashboardError::Internal(format!("mapping column: {error}")))?
            == column
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Count rows in one table when it exists.
fn count_table_rows(conn: &Connection, table: &str) -> Result<Option<u64>, DashboardError> {
    if !table_exists(conn, table)? {
        return Ok(None);
    }
    let count = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", sqlite_ident(table)),
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| DashboardError::Internal(format!("counting table rows: {error}")))?;
    Ok(Some(u64::try_from(count).unwrap_or(0)))
}

/// Count rows in one table matching a trusted predicate.
fn count_where(conn: &Connection, table: &str, predicate: &str) -> Result<u64, DashboardError> {
    let count = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE {predicate}",
                sqlite_ident(table)
            ),
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| DashboardError::Internal(format!("counting filtered rows: {error}")))?;
    Ok(u64::try_from(count).unwrap_or(0))
}

/// Quote a trusted `SQLite` identifier.
fn sqlite_ident(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Return the current epoch milliseconds.
fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Build a minimal test pipeline plan.
    fn test_plan(root: &Path, job_type: &str) -> ResearchPipelinePlan {
        ResearchPipelinePlan {
            job_id: "job-1".to_string(),
            job_type: job_type.to_string(),
            artifact_id: Some("artifact-1".to_string()),
            artifact_root: None,
            job_root: root.to_path_buf(),
            data_db_path: root.join("data.db"),
            start: "1970-01-01T00:00:01.000Z".to_string(),
            end: "1970-01-01T00:00:02.000Z".to_string(),
            prepared_db_output_path: root.join("prepared.db"),
            backtest_output_path: root.join("backtest.db"),
            sweep_output_path: root.join("sweep.csv"),
            report_json_path: root.join("report.json"),
            report_csv_path: root.join("report.csv"),
            balance: 200.0,
            sets: vec!["EDGE=1.0".to_string()],
            sweeps: vec!["EDGE=1.0,2.0".to_string()],
            archive_scratch: false,
        }
    }

    /// Return an empty step list for report tests.
    fn no_steps() -> Vec<ResearchJobStep> {
        Vec::new()
    }

    /// Verify current-params reports include metrics and curves from SQLite.
    #[test]
    fn current_params_report_reads_backtest_db() {
        let dir = tempdir().unwrap();
        let plan = test_plan(dir.path(), "current_params");
        let conn = Connection::open(&plan.backtest_output_path).unwrap();
        conn.execute(
            "CREATE TABLE trade_results (pnl_net REAL, fee_amount REAL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO trade_results (pnl_net, fee_amount) VALUES (10.0, 1.0), (-3.0, 0.5)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE balance_log (id INTEGER PRIMARY KEY, timestamp INTEGER, event TEXT, trade_id INTEGER, amount REAL, balance REAL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (0, 'init', NULL, 0.0, 200.0), (2, 'trade', 1, 10.0, 210.0), (3, 'trade', 2, -3.0, 207.0)",
            [],
        )
        .unwrap();
        conn.execute("CREATE TABLE signals (id INTEGER PRIMARY KEY)", [])
            .unwrap();
        conn.execute("INSERT INTO signals (id) VALUES (1)", [])
            .unwrap();
        conn.execute(
            "CREATE TABLE strategy_rejection_summaries (reason TEXT, count INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO strategy_rejection_summaries (reason, count) VALUES ('late', 3)",
            [],
        )
        .unwrap();
        drop(conn);

        let docs = build_report_documents(&plan, &no_steps()).unwrap();
        let full: Value = serde_json::from_str(&docs.full_json).unwrap();
        let summary: Value = serde_json::from_str(&docs.summary_json).unwrap();

        assert_eq!(full["schema_version"], 2);
        assert_eq!(full["metrics"]["net_pnl"], 7.0);
        assert_eq!(full["metrics"]["trade_count"], 2);
        assert_eq!(full["metrics"]["wins"], 1);
        assert_eq!(full["metrics"]["losses"], 1);
        assert_eq!(full["equity_curve"].as_array().unwrap().len(), 3);
        assert_eq!(full["equity_curve"][0]["ts"], 1000);
        assert_eq!(full["rejection_reasons"][0]["reason"], "late");
        assert!(summary["equity_curve"].is_null());
    }

    /// Verify reports warn when source-run metrics differ from replay output.
    #[test]
    fn current_params_report_compares_source_run_metrics() {
        let dir = tempdir().unwrap();
        let plan = test_plan(dir.path(), "current_params");
        let replay = Connection::open(&plan.backtest_output_path).unwrap();
        replay
            .execute(
                "CREATE TABLE trade_results (pnl_net REAL, fee_amount REAL)",
                [],
            )
            .unwrap();
        replay
            .execute(
                "INSERT INTO trade_results (pnl_net, fee_amount) VALUES (45.0, 2.0)",
                [],
            )
            .unwrap();
        replay
            .execute(
                "CREATE TABLE balance_log (id INTEGER PRIMARY KEY, timestamp INTEGER, event TEXT, trade_id INTEGER, amount REAL, balance REAL)",
                [],
            )
            .unwrap();
        replay
            .execute(
                "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (0, 'init', NULL, 0.0, 200.0), (1500, 'trade', 1, 45.0, 245.0)",
                [],
            )
            .unwrap();
        replay
            .execute("CREATE TABLE signals (id INTEGER PRIMARY KEY)", [])
            .unwrap();
        replay
            .execute("INSERT INTO signals (id) VALUES (1), (2)", [])
            .unwrap();
        drop(replay);

        let source = Connection::open(&plan.data_db_path).unwrap();
        source
            .execute(
                "CREATE TABLE trade_results (resolved_at INTEGER, pnl_net REAL, fee_amount REAL)",
                [],
            )
            .unwrap();
        source
            .execute(
                "INSERT INTO trade_results (resolved_at, pnl_net, fee_amount) VALUES (1500, 35.0, 1.5)",
                [],
            )
            .unwrap();
        source
            .execute("CREATE TABLE signals (timestamp INTEGER)", [])
            .unwrap();
        source
            .execute("INSERT INTO signals (timestamp) VALUES (1500)", [])
            .unwrap();
        drop(source);

        let docs = build_report_documents(&plan, &no_steps()).unwrap();
        let full: Value = serde_json::from_str(&docs.full_json).unwrap();
        let summary: Value = serde_json::from_str(&docs.summary_json).unwrap();

        assert_eq!(full["source_comparison"]["status"], "mismatch");
        assert_eq!(full["source_comparison"]["source"]["net_pnl"], 35.0);
        assert_eq!(full["source_comparison"]["replay"]["net_pnl"], 45.0);
        assert_eq!(full["source_comparison"]["delta"]["net_pnl"], 10.0);
        assert_eq!(full["source_comparison"]["delta"]["signal_count"], 1);
        assert_eq!(summary["source_comparison"]["status"], "mismatch");
        assert!(
            full["diagnostics"]
                .as_array()
                .unwrap()
                .contains(&Value::String("source_replay_result_mismatch".to_string()))
        );
    }

    /// Verify no-trade reports state the condition explicitly.
    #[test]
    fn current_params_report_marks_no_trades_and_no_signals() {
        let dir = tempdir().unwrap();
        let plan = test_plan(dir.path(), "current_params");
        let conn = Connection::open(&plan.backtest_output_path).unwrap();
        conn.execute(
            "CREATE TABLE trade_results (pnl_net REAL, fee_amount REAL)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE balance_log (id INTEGER PRIMARY KEY, timestamp INTEGER, event TEXT, trade_id INTEGER, amount REAL, balance REAL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance) VALUES (1, 'init', NULL, 0.0, 200.0)",
            [],
        )
        .unwrap();
        conn.execute("CREATE TABLE signals (id INTEGER PRIMARY KEY)", [])
            .unwrap();
        drop(conn);

        let docs = build_report_documents(&plan, &no_steps()).unwrap();
        let full: Value = serde_json::from_str(&docs.full_json).unwrap();

        assert_eq!(full["metrics"]["trade_count"], 0);
        assert_eq!(full["metrics"]["signal_count"], 0);
        assert!(
            full["diagnostics"]
                .as_array()
                .unwrap()
                .contains(&Value::String("no_trades".to_string()))
        );
        assert!(
            full["diagnostics"]
                .as_array()
                .unwrap()
                .contains(&Value::String("no_signals".to_string()))
        );
    }

    /// Verify sweep reports parse and rank rows by PnL.
    #[test]
    fn sweep_report_ranks_rows_by_pnl() {
        let dir = tempdir().unwrap();
        let plan = test_plan(dir.path(), "sweep");
        std::fs::write(
            &plan.sweep_output_path,
            "EDGE,pnl,win_rate,trades,max_dd,final_balance\n1,5,0.5,2,0.01,205\n2,9,1,1,0,209\n",
        )
        .unwrap();

        let docs = build_report_documents(&plan, &no_steps()).unwrap();
        let full: Value = serde_json::from_str(&docs.full_json).unwrap();

        assert_eq!(full["metrics"]["net_pnl"], 9.0);
        assert_eq!(full["sweep"]["parameter_columns"][0], "EDGE");
        assert_eq!(full["sweep"]["rows"][0]["rank"], 1);
        assert_eq!(full["sweep"]["rows"][0]["params"]["EDGE"], 2.0);
        assert!(docs.csv.starts_with("rank,EDGE,pnl"));
    }

    /// Verify sweep reports prefer calibrated PnL when available.
    #[test]
    fn sweep_report_ranks_rows_by_calibrated_pnl() {
        let dir = tempdir().unwrap();
        let plan = test_plan(dir.path(), "sweep");
        std::fs::write(
            &plan.sweep_output_path,
            "EDGE,pnl,calibrated_pnl,calibrated_final_balance,calibration_confidence,trades\n1,50,30,130,medium,2\n2,45,35,135,medium,2\n",
        )
        .unwrap();

        let docs = build_report_documents(&plan, &no_steps()).unwrap();
        let full: Value = serde_json::from_str(&docs.full_json).unwrap();
        let summary: Value = serde_json::from_str(&docs.summary_json).unwrap();

        assert_eq!(full["metrics"]["net_pnl"], 35.0);
        assert_eq!(full["metrics"]["final_balance"], 135.0);
        assert_eq!(full["sweep"]["ranked_by"], "calibrated_pnl");
        assert_eq!(full["sweep"]["rows"][0]["params"]["EDGE"], 2.0);
        assert_eq!(summary["sweep_summary"]["ranked_by"], "calibrated_pnl");
        assert!(
            full["diagnostics"]
                .as_array()
                .unwrap()
                .contains(&Value::String("sweep_ranked_by_calibrated_pnl".to_string()))
        );
    }

    /// Verify malformed sweep rows are skipped and diagnosed.
    #[test]
    fn sweep_report_skips_malformed_rows() {
        let dir = tempdir().unwrap();
        let plan = test_plan(dir.path(), "sweep");
        std::fs::write(&plan.sweep_output_path, "EDGE,pnl\n1,5\nbad\n").unwrap();

        let docs = build_report_documents(&plan, &no_steps()).unwrap();
        let full: Value = serde_json::from_str(&docs.full_json).unwrap();

        assert_eq!(full["sweep"]["rows"].as_array().unwrap().len(), 1);
        assert!(
            full["diagnostics"]
                .as_array()
                .unwrap()
                .contains(&Value::String("sweep_csv_malformed_row".to_string()))
        );
    }
}
