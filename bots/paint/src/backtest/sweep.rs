use std::fs;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use rayon::prelude::*;

use crate::backtest::runner::{BacktestOptions, BacktestResult, TickSource};
use crate::backtest::tick_replay::{SharedTicks, TickReplay};
use crate::config::{Config, parse_boolish};
use crate::db::source_run_metrics::SourceRunMetrics;

pub struct SweepDimension {
    pub param: String,
    pub values: Vec<f64>,
}

/// Source-run calibration used to bias-adjust sweep `PnL`.
#[derive(Debug, Clone)]
pub(crate) struct SweepCalibration {
    source_baseline_pnl: f64,
    baseline_replay_pnl: f64,
    source_baseline_trades: u64,
    baseline_replay_trades: u64,
    source_baseline_signals: u64,
    baseline_replay_signals: u64,
    starting_balance: f64,
    confidence: &'static str,
}

impl SweepCalibration {
    /// Build calibration from one source run and one replay baseline.
    #[must_use]
    pub(crate) fn from_source_and_replay(
        source: &SourceRunMetrics,
        replay: &BacktestResult,
        starting_balance: f64,
    ) -> Self {
        let source_baseline_pnl = source.net_pnl.unwrap_or(0.0);
        let source_baseline_trades = source.trade_count.unwrap_or(0);
        let source_baseline_signals = source.signal_count.unwrap_or(0);
        let baseline_replay_trades = replay.trades;
        let baseline_replay_signals = replay.signals;
        let baseline_replay_pnl = replay.total_pnl;
        let pnl_delta = baseline_replay_pnl - source_baseline_pnl;
        let trade_delta = i64::try_from(baseline_replay_trades).unwrap_or(i64::MAX)
            - i64::try_from(source_baseline_trades).unwrap_or(i64::MAX);
        let signal_delta = i64::try_from(baseline_replay_signals).unwrap_or(i64::MAX)
            - i64::try_from(source_baseline_signals).unwrap_or(i64::MAX);
        let confidence =
            calibration_confidence(starting_balance, pnl_delta, trade_delta, signal_delta);
        Self {
            source_baseline_pnl,
            baseline_replay_pnl,
            source_baseline_trades,
            baseline_replay_trades,
            source_baseline_signals,
            baseline_replay_signals,
            starting_balance,
            confidence,
        }
    }

    /// Return replay minus source `PnL` for the baseline current-params replay.
    #[must_use]
    pub(crate) fn baseline_delta_pnl(&self) -> f64 {
        self.baseline_replay_pnl - self.source_baseline_pnl
    }

    /// Return replay minus source trade count for the baseline replay.
    #[must_use]
    pub(crate) fn baseline_trade_delta(&self) -> i64 {
        i64::try_from(self.baseline_replay_trades).unwrap_or(i64::MAX)
            - i64::try_from(self.source_baseline_trades).unwrap_or(i64::MAX)
    }

    /// Return replay minus source signal count for the baseline replay.
    #[must_use]
    pub(crate) fn baseline_signal_delta(&self) -> i64 {
        i64::try_from(self.baseline_replay_signals).unwrap_or(i64::MAX)
            - i64::try_from(self.source_baseline_signals).unwrap_or(i64::MAX)
    }

    /// Return one sweep row's baseline bias-adjusted `PnL`.
    #[must_use]
    pub(crate) fn calibrated_pnl(&self, replay_pnl: f64) -> f64 {
        replay_pnl - self.baseline_delta_pnl()
    }

    /// Return one sweep row's baseline bias-adjusted final balance.
    #[must_use]
    pub(crate) fn calibrated_final_balance(&self, replay_pnl: f64) -> f64 {
        self.starting_balance + self.calibrated_pnl(replay_pnl)
    }
}

/// Runs sweep.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn run_sweep(
    data_path: &str,
    output_path: &str,
    start_time: u64,
    end_time: u64,
    starting_balance: f64,
    dimensions: &[SweepDimension],
    fixed_overrides: &[(String, String)],
    base_config: &Config,
) -> anyhow::Result<()> {
    let t0 = Instant::now();

    let readiness =
        crate::backtest::backtest_input::validate_input(data_path, start_time, end_time)?;
    println!(
        "Backtest input: {} | replay_quality={} | windows={} | dry_run_ticks={}\n",
        readiness.class.as_str(),
        readiness.replay_quality.class.as_str(),
        readiness.settled_windows,
        readiness.dry_run_ticks,
    );
    let live_fidelity =
        crate::db::live_fidelity::validate_live_sweep_input(data_path, start_time, end_time)?;
    println!("Live fidelity: {}\n", live_fidelity.class.as_str());

    println!("Loading tick data into memory...");
    let conn = rusqlite::Connection::open_with_flags(
        data_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("opening data DB: {data_path}"))?;
    if !crate::db::schema::has_replay_indexes(&conn)? {
        eprintln!(
            "warning: input DB is backtest-ready but not prepared for large sweeps; run `buba-paint prepare-backtest-input` for offline replay indexes"
        );
    }
    let cached_ticks = TickReplay::load_ticks(&conn, start_time, end_time)?;
    drop(conn);
    println!("Loaded {} ticks.\n", cached_ticks.len());

    let cached_ticks = Arc::new(cached_ticks);
    let calibration = build_sweep_calibration(
        data_path,
        start_time,
        end_time,
        starting_balance,
        Arc::clone(&cached_ticks),
        fixed_overrides,
        base_config,
    )?;

    let combinations = cartesian(dimensions);
    let total_runs = combinations.len();

    let dim_names: Vec<String> = dimensions.iter().map(|d| d.param.clone()).collect();
    let dim_desc: Vec<String> = dimensions
        .iter()
        .map(|d| format!("{}({})", d.param, d.values.len()))
        .collect();
    let rayon_threads = rayon::current_num_threads();
    println!(
        "Sweep: {} = {total_runs} combinations on {rayon_threads} Rayon threads\n",
        dim_desc.join(" x ")
    );

    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = combinations
        .par_iter()
        .enumerate()
        .map(|(i, combo)| {
            let mut config = base_config.clone();
            config.starting_balance = starting_balance;
            config.log_level = "error".to_string();

            for (param, value) in combo {
                config.set_param(param, *value);
            }
            apply_fixed_overrides(&mut config, fixed_overrides);

            let results_db_path = ":memory:".to_string();

            let label: String = combo
                .iter()
                .map(|(p, v)| format!("{p}={v}"))
                .collect::<Vec<_>>()
                .join(" ");

            let result = crate::backtest::runner::run_backtest(BacktestOptions {
                tick_source: TickSource::Cached(Arc::clone(&cached_ticks)),
                data_db_path: data_path.to_string(),
                results_db_path: results_db_path.clone(),
                start_time,
                end_time,
                starting_balance,
                quiet: true,
                config,
            });

            match result {
                Ok(r) => {
                    println!(
                        "[{}/{}] {} ... PnL=${:.0} WR={:.1}% Trades={} DD={:.1}% ({:.1}s)",
                        i + 1,
                        total_runs,
                        label,
                        r.total_pnl,
                        r.win_rate * 100.0,
                        r.trades,
                        r.max_drawdown_pct * 100.0,
                        r.elapsed_seconds,
                    );
                    let param_refs: Vec<(&str, f64)> =
                        combo.iter().map(|(p, v)| (p.as_str(), *v)).collect();
                    (param_refs, r)
                }
                Err(e) => {
                    eprintln!("[{}/{}] {} ... ERROR: {e}", i + 1, total_runs, label);
                    let param_refs: Vec<(&str, f64)> =
                        combo.iter().map(|(p, v)| (p.as_str(), *v)).collect();

                    (
                        param_refs,
                        BacktestResult {
                            start_time,
                            end_time,
                            duration_hours: 0.0,
                            elapsed_seconds: 0.0,
                            total_ticks: 0,
                            total_windows: 0,
                            signals: 0,
                            trades: 0,
                            wins: 0,
                            losses: 0,
                            win_rate: 0.0,
                            final_balance: starting_balance,
                            total_pnl: 0.0,
                            gross_pnl: 0.0,
                            max_drawdown_pct: 0.0,
                            high_water_mark: starting_balance,
                            total_fees: 0.0,
                            pnl_net: 0.0,
                            fill_rate: 0.0,
                            partial_fill_rate: 0.0,
                            no_fill_count: 0,
                            spread_legging_count: 0,
                            residual_position_count: 0,
                            avg_fill_latency_ms: 0.0,
                            avg_slippage: 0.0,
                            raw_event_batches: 0,
                            legacy_snapshot_batches: 0,
                            dislocation_regime_count: 0,
                            structural_pair_regime_count: 0,
                            calm_regime_count: 0,
                            dislocation_queued: 0,
                            structural_pair_queued: 0,
                            calm_queued: 0,
                            dislocation_filled: 0,
                            structural_pair_filled: 0,
                            calm_filled: 0,
                            dislocation_missed: 0,
                            structural_pair_missed: 0,
                            calm_missed: 0,
                            latency_arb_candidates: 0,
                            spread_capture_candidates: 0,
                            calm_persistence_candidates: 0,
                            router_blocked_count: 0,
                            capital_blocked_count: 0,
                            latency_spread_overlap_count: 0,
                            latency_calm_overlap_count: 0,
                            spread_calm_overlap_count: 0,
                        },
                    )
                }
            }
        })
        .collect();

    if let Some(parent) = std::path::Path::new(output_path).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory: {}", parent.display()))?;
    }

    let csv = build_csv_with_calibration(&dim_names, &results, calibration.as_ref());

    fs::write(output_path, &csv).with_context(|| format!("writing CSV to {output_path}"))?;

    let total_elapsed = t0.elapsed().as_secs_f64();
    println!("\nSweep complete: {total_runs} runs in {total_elapsed:.1}s");
    println!("Results: {output_path}");

    let top = top_n_by_calibrated_pnl(&results, calibration.as_ref(), 5);

    println!(
        "\nTop 5 by {}:",
        if calibration.is_some() {
            "calibrated PnL"
        } else {
            "PnL"
        }
    );
    for (i, &(combo, r)) in top.iter().enumerate() {
        let params: String = combo
            .iter()
            .map(|(p, v)| format!("{p}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        let displayed_pnl = calibration
            .as_ref()
            .map_or(r.total_pnl, |value| value.calibrated_pnl(r.total_pnl));
        println!(
            "  {}. {} -> PnL=${:.0} WR={:.1}% DD={:.1}%",
            i + 1,
            params,
            displayed_pnl,
            r.win_rate * 100.0,
            r.max_drawdown_pct * 100.0,
        );
    }

    Ok(())
}

/// Build sweep calibration from source-run metrics and a current-params replay.
fn build_sweep_calibration(
    data_path: &str,
    start_time: u64,
    end_time: u64,
    starting_balance: f64,
    cached_ticks: SharedTicks,
    fixed_overrides: &[(String, String)],
    base_config: &Config,
) -> anyhow::Result<Option<SweepCalibration>> {
    let Some(source) = crate::db::source_run_metrics::read_source_run_metrics(
        data_path,
        start_time,
        end_time,
        starting_balance,
    )?
    else {
        return Ok(None);
    };
    println!("Calibrating sweep against source-run current params baseline...");
    let mut config = base_config.clone();
    config.starting_balance = starting_balance;
    config.log_level = "error".to_string();
    apply_fixed_overrides(&mut config, fixed_overrides);
    config.validate()?;
    let replay = crate::backtest::runner::run_backtest(BacktestOptions {
        tick_source: TickSource::Cached(cached_ticks),
        data_db_path: data_path.to_string(),
        results_db_path: ":memory:".to_string(),
        start_time,
        end_time,
        starting_balance,
        quiet: true,
        config,
    })?;
    let calibration = SweepCalibration::from_source_and_replay(&source, &replay, starting_balance);
    println!(
        "Calibration: source_pnl={:.4} baseline_replay_pnl={:.4} delta={:.4} confidence={}",
        calibration.source_baseline_pnl,
        calibration.baseline_replay_pnl,
        calibration.baseline_delta_pnl(),
        calibration.confidence,
    );
    Ok(Some(calibration))
}

/// Apply fixed CLI overrides to one sweep config.
fn apply_fixed_overrides(config: &mut Config, fixed_overrides: &[(String, String)]) {
    for (param, value_str) in fixed_overrides {
        if let Ok(num) = value_str.parse::<f64>() {
            config.set_param(param, num);
        } else if let Some(value) = parse_boolish(value_str) {
            if !config.set_bool_param(param, value) {
                eprintln!("Boolean --set value ignored for non-boolean param: {param}={value_str}");
            }
        } else {
            eprintln!("Unsupported --set value ignored: {param}={value_str}");
        }
    }
}

/// Classify how much source/replay baseline mismatch affects sweep confidence.
fn calibration_confidence(
    starting_balance: f64,
    pnl_delta: f64,
    trade_delta: i64,
    signal_delta: i64,
) -> &'static str {
    if pnl_delta.abs() <= 0.01 && trade_delta == 0 && signal_delta == 0 {
        "high"
    } else if pnl_delta.abs() <= (starting_balance.abs() * 0.10).max(1.0)
        && trade_delta.abs() <= 2
        && signal_delta.abs() <= 2
    {
        "medium"
    } else {
        "low"
    }
}

/// Build the CSV string from sweep results.
///
/// Each row contains the parameter values followed by aggregate `PnL`, fill,
/// replay-fidelity, and timing metrics used by sweep analysis.
#[cfg(test)]
pub(crate) fn build_csv(
    dim_names: &[String],
    results: &[(Vec<(&str, f64)>, BacktestResult)],
) -> String {
    build_csv_with_calibration(dim_names, results, None)
}

/// Build the CSV string from sweep results with optional source-run calibration.
pub(crate) fn build_csv_with_calibration(
    dim_names: &[String],
    results: &[(Vec<(&str, f64)>, BacktestResult)],
    calibration: Option<&SweepCalibration>,
) -> String {
    let mut csv = String::new();
    csv.push_str(&sweep_csv_header(dim_names, calibration.is_some()));
    csv.push('\n');
    for (combo, r) in results {
        csv.push_str(&sweep_csv_row(combo, r, calibration));
        csv.push('\n');
    }
    csv
}

/// Build the sweep CSV header row, optionally with calibration columns.
fn sweep_csv_header(dim_names: &[String], with_calibration: bool) -> String {
    let mut headers: Vec<String> = dim_names.to_vec();
    headers.extend([
        "pnl".to_string(),
        "win_rate".to_string(),
        "trades".to_string(),
        "wins".to_string(),
        "losses".to_string(),
        "max_dd".to_string(),
        "hwm".to_string(),
        "final_balance".to_string(),
        "signals".to_string(),
        "fill_rate".to_string(),
        "partial_fill_rate".to_string(),
        "no_fill_count".to_string(),
        "spread_legging_count".to_string(),
        "residual_position_count".to_string(),
        "avg_fill_latency_ms".to_string(),
        "avg_slippage".to_string(),
        "raw_event_batches".to_string(),
        "legacy_snapshot_batches".to_string(),
        "dislocation_regime_count".to_string(),
        "structural_pair_regime_count".to_string(),
        "calm_regime_count".to_string(),
        "dislocation_queued".to_string(),
        "structural_pair_queued".to_string(),
        "calm_queued".to_string(),
        "dislocation_filled".to_string(),
        "structural_pair_filled".to_string(),
        "calm_filled".to_string(),
        "dislocation_missed".to_string(),
        "structural_pair_missed".to_string(),
        "calm_missed".to_string(),
        "latency_arb_candidates".to_string(),
        "spread_capture_candidates".to_string(),
        "calm_persistence_candidates".to_string(),
        "router_blocked_count".to_string(),
        "capital_blocked_count".to_string(),
        "latency_spread_overlap_count".to_string(),
        "latency_calm_overlap_count".to_string(),
        "spread_calm_overlap_count".to_string(),
        "total_fees".to_string(),
        "gross_pnl".to_string(),
        "pnl_net".to_string(),
        "elapsed_s".to_string(),
    ]);
    if with_calibration {
        headers.extend([
            "calibrated_pnl".to_string(),
            "calibrated_final_balance".to_string(),
            "baseline_replay_delta_pnl".to_string(),
            "source_baseline_pnl".to_string(),
            "baseline_replay_pnl".to_string(),
            "calibration_confidence".to_string(),
            "source_baseline_trades".to_string(),
            "baseline_replay_trades".to_string(),
            "baseline_trade_delta".to_string(),
            "source_baseline_signals".to_string(),
            "baseline_replay_signals".to_string(),
            "baseline_signal_delta".to_string(),
        ]);
    }
    headers.join(",")
}

/// Build one sweep CSV data row, optionally with calibration columns.
fn sweep_csv_row(
    combo: &[(&str, f64)],
    r: &BacktestResult,
    calibration: Option<&SweepCalibration>,
) -> String {
    {
        let mut row: Vec<String> = combo.iter().map(|(_, v)| format!("{v}")).collect();
        row.push(format!("{}", r.total_pnl));
        row.push(format!("{}", r.win_rate));
        row.push(format!("{}", r.trades));
        row.push(format!("{}", r.wins));
        row.push(format!("{}", r.losses));
        row.push(format!("{}", r.max_drawdown_pct));
        row.push(format!("{}", r.high_water_mark));
        row.push(format!("{}", r.final_balance));
        row.push(format!("{}", r.signals));
        row.push(format!("{}", r.fill_rate));
        row.push(format!("{}", r.partial_fill_rate));
        row.push(format!("{}", r.no_fill_count));
        row.push(format!("{}", r.spread_legging_count));
        row.push(format!("{}", r.residual_position_count));
        row.push(format!("{}", r.avg_fill_latency_ms));
        row.push(format!("{}", r.avg_slippage));
        row.push(format!("{}", r.raw_event_batches));
        row.push(format!("{}", r.legacy_snapshot_batches));
        row.push(format!("{}", r.dislocation_regime_count));
        row.push(format!("{}", r.structural_pair_regime_count));
        row.push(format!("{}", r.calm_regime_count));
        row.push(format!("{}", r.dislocation_queued));
        row.push(format!("{}", r.structural_pair_queued));
        row.push(format!("{}", r.calm_queued));
        row.push(format!("{}", r.dislocation_filled));
        row.push(format!("{}", r.structural_pair_filled));
        row.push(format!("{}", r.calm_filled));
        row.push(format!("{}", r.dislocation_missed));
        row.push(format!("{}", r.structural_pair_missed));
        row.push(format!("{}", r.calm_missed));
        row.push(format!("{}", r.latency_arb_candidates));
        row.push(format!("{}", r.spread_capture_candidates));
        row.push(format!("{}", r.calm_persistence_candidates));
        row.push(format!("{}", r.router_blocked_count));
        row.push(format!("{}", r.capital_blocked_count));
        row.push(format!("{}", r.latency_spread_overlap_count));
        row.push(format!("{}", r.latency_calm_overlap_count));
        row.push(format!("{}", r.spread_calm_overlap_count));
        row.push(format!("{}", r.total_fees));
        row.push(format!("{}", r.gross_pnl));
        row.push(format!("{}", r.pnl_net));
        row.push(format!("{}", r.elapsed_seconds));
        if let Some(calibration) = calibration {
            row.push(format!("{}", calibration.calibrated_pnl(r.total_pnl)));
            row.push(format!(
                "{}",
                calibration.calibrated_final_balance(r.total_pnl)
            ));
            row.push(format!("{}", calibration.baseline_delta_pnl()));
            row.push(format!("{}", calibration.source_baseline_pnl));
            row.push(format!("{}", calibration.baseline_replay_pnl));
            row.push(calibration.confidence.to_string());
            row.push(format!("{}", calibration.source_baseline_trades));
            row.push(format!("{}", calibration.baseline_replay_trades));
            row.push(format!("{}", calibration.baseline_trade_delta()));
            row.push(format!("{}", calibration.source_baseline_signals));
            row.push(format!("{}", calibration.baseline_replay_signals));
            row.push(format!("{}", calibration.baseline_signal_delta()));
        }
        row.join(",")
    }
}

/// Return the top `n` results sorted by calibrated `PnL` when available.
pub(crate) fn top_n_by_calibrated_pnl<'a>(
    results: &'a [(Vec<(&'a str, f64)>, BacktestResult)],
    calibration: Option<&SweepCalibration>,
    n: usize,
) -> Vec<&'a (Vec<(&'a str, f64)>, BacktestResult)> {
    let Some(calibration) = calibration else {
        return top_n_by_pnl(results, n);
    };
    let mut sorted: Vec<_> = results.iter().collect();
    sorted.sort_by(|a, b| {
        calibration
            .calibrated_pnl(b.1.total_pnl)
            .partial_cmp(&calibration.calibrated_pnl(a.1.total_pnl))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(n);
    sorted
}

/// Return the top `n` results sorted by descending `PnL`.
///
/// If there are fewer than `n` results, all are returned.  Ordering is
/// stable when `PnL` values are equal (preserves the original order).
pub(crate) fn top_n_by_pnl<'a>(
    results: &'a [(Vec<(&'a str, f64)>, BacktestResult)],
    n: usize,
) -> Vec<&'a (Vec<(&'a str, f64)>, BacktestResult)> {
    let mut sorted: Vec<_> = results.iter().collect();
    sorted.sort_by(|a, b| {
        b.1.total_pnl
            .partial_cmp(&a.1.total_pnl)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(n);
    sorted
}

/// Generate all combinations (Cartesian product) of sweep dimensions.
pub(crate) fn cartesian(dims: &[SweepDimension]) -> Vec<Vec<(String, f64)>> {
    if dims.is_empty() {
        return vec![vec![]];
    }

    let first = &dims[0];
    let rest = cartesian(&dims[1..]);

    let mut result = Vec::new();
    for value in &first.values {
        for combo in &rest {
            let mut new_combo = vec![(first.param.clone(), *value)];
            new_combo.extend(combo.iter().cloned());
            result.push(new_combo);
        }
    }
    result
}

/// Parse a sweep specification string into a `SweepDimension`.
///
/// Formats:
/// - `PARAM=start:end:step` - range
/// - `PARAM=val1,val2,val3`  - explicit list
pub fn parse_sweep_spec(spec: &str) -> anyhow::Result<SweepDimension> {
    let eq_idx = spec.find('=').ok_or_else(|| {
        anyhow::anyhow!("invalid sweep format: {spec} (expected PARAM=start:end:step)")
    })?;
    let param = spec[..eq_idx].to_string();
    let range = &spec[eq_idx + 1..];

    let values = if range.contains(',') {
        range
            .split(',')
            .map(|s| {
                s.parse::<f64>()
                    .with_context(|| format!("invalid number in sweep list: {s}"))
            })
            .collect::<anyhow::Result<Vec<f64>>>()?
    } else {
        let parts: Vec<&str> = range.split(':').collect();
        if parts.len() != 3 {
            anyhow::bail!("invalid range: {range} (expected start:end:step)");
        }
        let start: f64 = parts[0].parse().context("invalid range start")?;
        let end: f64 = parts[1].parse().context("invalid range end")?;
        let step: f64 = parts[2].parse().context("invalid range step")?;

        if step <= 0.0 {
            anyhow::bail!("step must be positive: {step}");
        }

        let mut values = Vec::new();
        let mut v = start;
        while v <= end + step * 0.001 {
            let s = format!("{v:.10e}");
            let cleaned: f64 = s.parse().unwrap_or(v);
            values.push(cleaned);
            v += step;
        }
        values
    };

    Ok(SweepDimension { param, values })
}

#[cfg(test)]
#[path = "tests/sweep_tests.rs"]
mod tests;
