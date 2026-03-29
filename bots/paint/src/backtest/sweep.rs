// Parameter sweep — runs the backtester with many parameter combinations.
//
// Direct port of the TypeScript sweep.  Uses rayon for parallelism.

use std::fs;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use rayon::prelude::*;

use crate::backtest::runner::{BacktestOptions, BacktestResult, TickSource};
use crate::backtest::tick_replay::TickReplay;
use crate::config::Config;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct SweepDimension {
    pub param: String,
    pub values: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

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

    // Pre-load ticks once (biggest perf win).
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

    // Generate Cartesian product.
    let combinations = cartesian(dimensions);
    let total_runs = combinations.len();

    let dim_names: Vec<String> = dimensions.iter().map(|d| d.param.clone()).collect();
    let dim_desc: Vec<String> = dimensions
        .iter()
        .map(|d| format!("{}({})", d.param, d.values.len()))
        .collect();
    println!(
        "Sweep: {} = {total_runs} combinations\n",
        dim_desc.join(" x ")
    );

    // Run in parallel.
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = combinations
        .par_iter()
        .enumerate()
        .map(|(i, combo)| {
            // Clone config and apply overrides.
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

            // Use a unique temp path per run — includes process ID to avoid
            // stale-DB contamination when multiple sweeps run in the same process.
            let pid = std::process::id();
            let results_db_path = format!("/tmp/buba-sweep-{pid}-{i:04}.db");

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

            // Clean up temp DB after reading results.
            for suffix in ["", "-shm", "-wal"] {
                let f = format!("{results_db_path}{suffix}");
                let _ = fs::remove_file(&f);
            }

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
                    // Return a zeroed result on error.
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
                            max_drawdown_pct: 0.0,
                            high_water_mark: starting_balance,
                            total_fees: 0.0,
                            pnl_net: 0.0,
                        },
                    )
                }
            }
        })
        .collect();

    // Write CSV.
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

    // Print top 5 by PnL.
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the CSV string from sweep results.
///
/// Each row contains the parameter values followed by the standard metric
/// columns: `pnl`, `win_rate`, `trades`, `wins`, `losses`, `max_dd`, `hwm`,
/// `final_balance`, `signals`, `elapsed_s`.
pub(crate) fn build_csv(
    dim_names: &[String],
    results: &[(Vec<(&str, f64)>, BacktestResult)],
) -> String {
    let mut csv = String::new();

    // Header.
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
        "total_fees".to_string(),
        "pnl_net".to_string(),
        "elapsed_s".to_string(),
    ]);
    csv.push_str(&headers.join(","));
    csv.push('\n');

    // Rows.
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
        row.push(format!("{}", r.total_fees));
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
            // Match JS toPrecision(10) — clean up float drift.
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
