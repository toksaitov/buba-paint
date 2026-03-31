use std::fs;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use rayon::prelude::*;

use crate::backtest::runner::{BacktestOptions, BacktestResult, TickSource};
use crate::backtest::tick_replay::TickReplay;
use crate::config::Config;

pub struct SweepDimension {
    pub param: String,
    pub values: Vec<f64>,
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
) -> anyhow::Result<()> {
    let t0 = Instant::now();

    println!("Loading tick data into memory...");
    let conn = rusqlite::Connection::open_with_flags(
        data_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("opening data DB: {data_path}"))?;
    let cached_ticks = TickReplay::load_ticks(&conn, start_time, end_time)?;
    drop(conn);
    println!("Loaded {} ticks.\n", cached_ticks.len());

    let cached_ticks = Arc::new(cached_ticks);

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
            let mut config = Config {
                starting_balance,
                log_level: "error".to_string(),
                ..Config::default()
            };

            for (param, value) in combo {
                config.set_param(param, *value);
            }
            for (param, value_str) in fixed_overrides {
                if let Ok(num) = value_str.parse::<f64>() {
                    config.set_param(param, num);
                } else {
                    eprintln!("Non-numeric --set value ignored: {param}={value_str}");
                }
            }

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
                        },
                    )
                }
            }
        })
        .collect();

    if let Some(parent) = std::path::Path::new(output_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating output directory: {}", parent.display()))?;
        }
    }

    let csv = build_csv(&dim_names, &results);

    fs::write(output_path, &csv).with_context(|| format!("writing CSV to {output_path}"))?;

    let total_elapsed = t0.elapsed().as_secs_f64();
    println!("\nSweep complete: {total_runs} runs in {total_elapsed:.1}s");
    println!("Results: {output_path}");

    let top = top_n_by_pnl(&results, 5);

    println!("\nTop 5 by PnL:");
    for (i, &(combo, r)) in top.iter().enumerate() {
        let params: String = combo
            .iter()
            .map(|(p, v)| format!("{p}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "  {}. {} -> PnL=${:.0} WR={:.1}% DD={:.1}%",
            i + 1,
            params,
            r.total_pnl,
            r.win_rate * 100.0,
            r.max_drawdown_pct * 100.0,
        );
    }

    Ok(())
}

/// Build the CSV string from sweep results.
///
/// Each row contains the parameter values followed by aggregate `PnL`, fill,
/// replay-fidelity, and timing metrics used by sweep analysis.
pub(crate) fn build_csv(
    dim_names: &[String],
    results: &[(Vec<(&str, f64)>, BacktestResult)],
) -> String {
    let mut csv = String::new();

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
        "total_fees".to_string(),
        "gross_pnl".to_string(),
        "pnl_net".to_string(),
        "elapsed_s".to_string(),
    ]);
    csv.push_str(&headers.join(","));
    csv.push('\n');

    for (combo, r) in results {
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
        row.push(format!("{}", r.total_fees));
        row.push(format!("{}", r.gross_pnl));
        row.push(format!("{}", r.pnl_net));
        row.push(format!("{}", r.elapsed_seconds));
        csv.push_str(&row.join(","));
        csv.push('\n');
    }

    csv
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
/// - `PARAM=start:end:step` — range
/// - `PARAM=val1,val2,val3`  — explicit list
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
