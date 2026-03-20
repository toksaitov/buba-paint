// CLI parsing and command dispatch.
//
// All logic lives here so it can be tested via `Cli::parse_from()` and
// `run()`.  The binary's `main()` is a thin shell: parse + run.

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};

use crate::backtest::runner::{BacktestOptions, TickSource};
use crate::backtest::sweep::{self, SweepDimension};
use crate::config::Config;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "buba-paint", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a single backtest
    Backtest {
        #[arg(long)]
        data: String,
        #[arg(long)]
        start: String,
        #[arg(long)]
        end: String,
        #[arg(long, default_value = "/tmp/buba-backtest.db")]
        output: String,
        #[arg(long, default_value = "200")]
        balance: f64,
        #[arg(long = "set")]
        sets: Vec<String>,
    },
    /// Run a parameter sweep
    Sweep {
        #[arg(long)]
        data: String,
        #[arg(long)]
        start: String,
        #[arg(long)]
        end: String,
        #[arg(long, default_value = "data/sweeps/output/sweep.csv")]
        output: String,
        #[arg(long, default_value = "200")]
        balance: f64,
        #[arg(long = "sweep")]
        sweeps: Vec<String>,
        #[arg(long = "set")]
        sets: Vec<String>,
    },
    /// Run the live paper trading bot
    Live {
        #[arg(long, default_value = "./data/buba-paint.db")]
        db_path: String,
        #[arg(long, default_value = "150")]
        balance: f64,
        #[arg(long = "set")]
        sets: Vec<String>,
    },
    /// Build merged market-data DB from run databases
    BuildData {
        /// Directory containing run subdirectories (e.g., runs/)
        #[arg(long, default_value = "runs")]
        runs_dir: String,
        /// Output path for the merged database
        #[arg(long, default_value = "data/market-data.db")]
        output: String,
    },
}

// ---------------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------------

/// Execute a parsed CLI command.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Backtest {
            data,
            start,
            end,
            output,
            balance,
            sets,
        } => {
            let start_time = parse_time(&start)?;
            let end_time = parse_time(&end)?;

            let mut config = Config {
                starting_balance: balance,
                log_level: "warn".to_string(),
                ..Config::default()
            };

            for set_str in &sets {
                apply_set_override(&mut config, set_str)?;
            }

            crate::backtest::runner::run_backtest(BacktestOptions {
                tick_source: TickSource::FromDb(data.clone()),
                data_db_path: data,
                results_db_path: output,
                start_time,
                end_time,
                starting_balance: balance,
                quiet: false,
                config,
            })?;
        }
        Commands::Sweep {
            data,
            start,
            end,
            output,
            balance,
            sweeps,
            sets,
        } => {
            let start_time = parse_time(&start)?;
            let end_time = parse_time(&end)?;

            let dimensions: Vec<SweepDimension> = sweeps
                .iter()
                .map(|s| sweep::parse_sweep_spec(s))
                .collect::<anyhow::Result<Vec<_>>>()?;

            let fixed_overrides: Vec<(String, String)> = sets
                .iter()
                .map(|s| parse_key_value(s))
                .collect::<anyhow::Result<Vec<_>>>()?;

            sweep::run_sweep(
                &data,
                &output,
                start_time,
                end_time,
                balance,
                &dimensions,
                &fixed_overrides,
            )?;
        }
        Commands::Live {
            db_path,
            balance,
            sets,
        } => {
            let mut config = Config::from_env();
            config.starting_balance = balance;
            config.db_path = db_path.clone();
            for set_str in &sets {
                apply_set_override(&mut config, set_str)?;
            }

            let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
            tracing_subscriber::fmt().with_env_filter(env_filter).init();

            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                let _ = shutdown_tx.send(());
            });

            crate::live::run_live(config, &db_path, balance, shutdown_rx).await?;
        }
        Commands::BuildData { runs_dir, output } => {
            crate::db::build_data::build_market_data(&runs_dir, &output)?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse an ISO-ish date string to UTC milliseconds since epoch.
pub fn parse_time(s: &str) -> anyhow::Result<u64> {
    // Append Z if no timezone indicator is present.
    // Check for actual timezone patterns: 'Z', '+HH:MM', or '-HH:MM' (but not
    // bare '-' in the date portion).  The old code used `ends_with("00:00")`
    // which falsely matched times like "T00:00:00".
    let has_tz = s.contains('Z') || s.contains('+') || s.rfind('-').is_some_and(|i| i > 10); // '-' after the date part = tz offset
    let normalized = if has_tz {
        s.to_string()
    } else {
        format!("{s}Z")
    };

    // Try RFC 3339 first (e.g., "2026-02-20T00:00:00Z").
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&normalized) {
        #[allow(clippy::cast_sign_loss)]
        return Ok(dt.timestamp_millis() as u64);
    }

    // Try "YYYY-MM-DDTHH:MM" format.
    if let Ok(dt) =
        chrono::NaiveDateTime::parse_from_str(normalized.trim_end_matches('Z'), "%Y-%m-%dT%H:%M")
    {
        #[allow(clippy::cast_sign_loss)]
        return Ok(dt.and_utc().timestamp_millis() as u64);
    }

    // Try "YYYY-MM-DD" format (midnight UTC).
    if let Ok(dt) = chrono::NaiveDate::parse_from_str(normalized.trim_end_matches('Z'), "%Y-%m-%d")
    {
        let naive_dt = dt
            .and_hms_opt(0, 0, 0)
            .context("invalid midnight for date")?;
        #[allow(clippy::cast_sign_loss)]
        return Ok(naive_dt.and_utc().timestamp_millis() as u64);
    }

    bail!("invalid date: {s}")
}

/// Apply a single `KEY=VALUE` override to the config.
pub fn apply_set_override(config: &mut Config, set_str: &str) -> anyhow::Result<()> {
    let eq_idx = set_str
        .find('=')
        .with_context(|| format!("invalid --set format: {set_str} (expected KEY=VALUE)"))?;
    let key = &set_str[..eq_idx];
    let value_str = &set_str[eq_idx + 1..];
    if let Ok(num) = value_str.parse::<f64>() {
        config.set_param(key, num);
    } else {
        eprintln!("Non-numeric --set value: {key}={value_str}");
    }
    Ok(())
}

/// Parse a `KEY=VALUE` string into a tuple.
pub fn parse_key_value(s: &str) -> anyhow::Result<(String, String)> {
    let eq_idx = s
        .find('=')
        .with_context(|| format!("invalid --set format: {s} (expected KEY=VALUE)"))?;
    Ok((s[..eq_idx].to_string(), s[eq_idx + 1..].to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/cli_tests.rs"]
mod tests;
