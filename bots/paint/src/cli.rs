use std::str::FromStr;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};

use crate::backtest::runner::{BacktestOptions, TickSource};
use crate::backtest::sweep::{self, SweepDimension};
use crate::clock::{Clock, SystemClock};
use crate::config::{Config, parse_boolish};
use crate::live_control::{LiveControlAction, enqueue_live_control_command};
use crate::live_sidecar::LiveSidecarClient;

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
    /// Validate whether a database interval is safe for parameter sweeps
    ValidateReplayData {
        #[arg(long)]
        data: String,
        #[arg(long)]
        start: String,
        #[arg(long)]
        end: String,
    },
    /// Validate whether a live-trading interval is research-grade for sweeps
    ValidateLiveFidelity {
        #[arg(long)]
        db_path: String,
        #[arg(long)]
        start: String,
        #[arg(long)]
        end: String,
        #[arg(long)]
        output: Option<String>,
    },
    /// Run the long-lived runtime in paper, `live_readonly`, or disarmed `live_trading` mode
    Live {
        #[arg(long, default_value = "./data/paint.db")]
        db_path: String,
        #[arg(long, default_value = "100")]
        balance: f64,
        #[arg(long = "set")]
        sets: Vec<String>,
    },
    /// Queue a durable local live-trading control command
    LiveControl {
        #[arg(long)]
        db_path: String,
        action: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        reason: String,
    },
    /// Export a live-trading closeout package after halt or operator shutdown
    LiveCloseout {
        #[arg(long)]
        db_path: String,
        #[arg(long)]
        output_dir: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        reason: String,
    },
    /// Run live sidecar preflight for readonly or trading execution modes
    LivePreflight {
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
    /// Initialize an empty database (creates tables, no trading)
    InitDb {
        /// Path to the database file to create
        #[arg(long)]
        db_path: String,
        /// Starting balance to record in the init event
        #[arg(long, default_value = "200")]
        balance: f64,
    },
    /// Verify settlements against Polymarket's actual resolutions
    VerifySettlements {
        /// Path to the database to verify (market-data.db or a run DB)
        #[arg(long, default_value = "data/market-data.db")]
        db: String,
        /// Gamma API base URL
        #[arg(long, default_value = "https://gamma-api.polymarket.com")]
        gamma_url: String,
        /// Number of concurrent API requests
        #[arg(long, default_value = "10")]
        concurrency: usize,
    },
    /// Upgrade and backfill historical run DBs in place
    UpgradeHistory {
        #[arg(long, default_value = "runs")]
        runs_dir: String,
        #[arg(long, default_value = "4")]
        from_run: u32,
        #[arg(long, default_value = "9")]
        to_run: u32,
        #[arg(long, default_value_t = false)]
        rebuild_derived: bool,
        #[arg(long, default_value = "data/market-data.db")]
        output: String,
    },
    /// Probe current endpoint and feed latency from this host
    LatencyProbe {
        #[arg(long, default_value = "5000")]
        timeout_ms: u64,
        #[arg(long = "set")]
        sets: Vec<String>,
    },
    /// Print grouped database footprint and feed-event volume statistics
    DbFootprint {
        #[arg(long)]
        db_path: String,
    },
}

/// Execute a parsed CLI command.
#[allow(clippy::too_many_lines)]
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

            let mut config = Config::from_env();
            config.starting_balance = balance;
            config.log_level = "warn".to_string();

            for set_str in &sets {
                apply_set_override(&mut config, set_str)?;
            }
            config.validate()?;

            let quality =
                crate::backtest::replay_quality::analyze_path(&data, start_time, end_time)?;
            if !quality.is_sweep_grade() {
                eprintln!(
                    "warning: backtest input is not sweep-grade: replay_quality={}",
                    quality.class.as_str()
                );
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

            let mut base_config = Config::from_env();
            base_config.starting_balance = balance;
            base_config.log_level = "error".to_string();
            base_config.validate()?;

            sweep::run_sweep(
                &data,
                &output,
                start_time,
                end_time,
                balance,
                &dimensions,
                &fixed_overrides,
                &base_config,
            )?;
        }
        Commands::ValidateReplayData { data, start, end } => {
            let start_time = parse_time(&start)?;
            let end_time = parse_time(&end)?;
            let quality =
                crate::backtest::replay_quality::analyze_path(&data, start_time, end_time)?;
            println!(
                "{}",
                crate::backtest::replay_quality::format_report(&quality)
            );
            if !quality.is_sweep_grade() {
                bail!(
                    "{}",
                    crate::backtest::replay_quality::blocking_error(&quality)
                );
            }
        }
        Commands::ValidateLiveFidelity {
            db_path,
            start,
            end,
            output,
        } => {
            let start_time = parse_time(&start)?;
            let end_time = parse_time(&end)?;
            let report = crate::db::live_fidelity::analyze_path(&db_path, start_time, end_time)?;
            println!("{}", crate::db::live_fidelity::format_report(&report));
            if let Some(output_path) = output {
                write_live_fidelity_json(&output_path, &report)?;
            }
            if !report.is_research_grade_live() {
                bail!("{}", crate::db::live_fidelity::blocking_error(&report));
            }
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
            config.validate()?;

            let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
            tracing_subscriber::fmt()
                .with_ansi(true)
                .with_env_filter(env_filter)
                .init();

            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
            tokio::spawn(async move {
                tokio::select! {
                    () = async { let _ = tokio::signal::ctrl_c().await; } => {},
                    () = sigterm_future() => {},
                }
                let _ = shutdown_tx.send(());
            });

            crate::live::run_live(config, &db_path, balance, shutdown_rx).await?;
        }
        Commands::LiveControl {
            db_path,
            action,
            actor,
            reason,
        } => {
            let action = LiveControlAction::from_str(&action)?;
            let db = crate::db::database::Database::new(&db_path)?;
            let clock = SystemClock;
            let command_id =
                enqueue_live_control_command(&db, action, &actor, &reason, clock.now())?;
            db.close();
            println!(
                "queued live-control command {} ({})",
                command_id,
                action.as_str()
            );
        }
        Commands::LiveCloseout {
            db_path,
            output_dir,
            actor,
            reason,
        } => {
            let clock = SystemClock;
            let options = crate::live_closeout::LiveCloseoutOptions {
                db_path,
                output_dir,
                actor,
                reason,
                generated_at_ms: clock.now(),
            };
            crate::live_closeout::run_live_closeout(&options)?;
            println!("live closeout exported");
        }
        Commands::LivePreflight { sets } => {
            let mut config = Config::from_env();
            for set_str in &sets {
                apply_set_override(&mut config, set_str)?;
            }
            config.validate()?;
            if !config.is_live_execution() {
                bail!("live-preflight requires EXECUTION_MODE=live_readonly or live_trading");
            }

            let client = LiveSidecarClient::from_config(&config);
            let payload = client.preflight(&config).await?;
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        Commands::BuildData { runs_dir, output } => {
            crate::db::build_data::build_market_data(&runs_dir, &output)?;
        }
        Commands::InitDb { db_path, balance } => {
            let db = crate::db::database::Database::new(&db_path)?;
            let clock = crate::clock::BacktestClock::new();
            let config = Config::default();

            let _ = crate::bankroll::BankrollManager::new(balance, &config, &db, &clock);
            db.close();
            println!("Database initialized at {db_path} with balance ${balance}");
        }
        Commands::VerifySettlements {
            db,
            gamma_url,
            concurrency,
        } => {
            crate::verify::run_verify_settlements(&db, &gamma_url, concurrency).await?;
        }
        Commands::UpgradeHistory {
            runs_dir,
            from_run,
            to_run,
            rebuild_derived,
            output,
        } => {
            crate::db::upgrade_history::run_upgrade_history(
                crate::db::upgrade_history::UpgradeHistoryOptions {
                    runs_dir,
                    from_run,
                    to_run,
                    rebuild_derived,
                    output,
                },
            )
            .await?;
        }
        Commands::LatencyProbe { timeout_ms, sets } => {
            let mut config = Config::from_env();
            for set_str in &sets {
                apply_set_override(&mut config, set_str)?;
            }
            crate::latency_probe::run_latency_probe(&config, timeout_ms).await?;
        }
        Commands::DbFootprint { db_path } => {
            crate::db::database::print_db_footprint(&db_path)?;
        }
    }

    Ok(())
}

/// Parse an ISO-ish date string to UTC milliseconds since epoch.
pub fn parse_time(s: &str) -> anyhow::Result<u64> {
    let has_tz = s.contains('Z') || s.contains('+') || s.rfind('-').is_some_and(|i| i > 10);
    let normalized = if has_tz {
        s.to_string()
    } else {
        format!("{s}Z")
    };

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&normalized) {
        #[allow(clippy::cast_sign_loss)]
        return Ok(dt.timestamp_millis() as u64);
    }

    if let Ok(dt) =
        chrono::NaiveDateTime::parse_from_str(normalized.trim_end_matches('Z'), "%Y-%m-%dT%H:%M")
    {
        #[allow(clippy::cast_sign_loss)]
        return Ok(dt.and_utc().timestamp_millis() as u64);
    }

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
    } else if let Some(value) = parse_boolish(value_str) {
        if !config.set_bool_param(key, value) {
            eprintln!("Boolean --set value ignored for non-boolean param: {key}={value_str}");
        }
    } else if !config.set_string_param(key, value_str) {
        eprintln!("Unsupported --set value: {key}={value_str}");
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

/// Write one live-fidelity report as pretty JSON.
fn write_live_fidelity_json(
    output_path: &str,
    report: &crate::db::live_fidelity::LiveFidelityReport,
) -> anyhow::Result<()> {
    if let Some(parent) = std::path::Path::new(output_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory: {}", parent.display()))?;
    }
    std::fs::write(output_path, serde_json::to_string_pretty(report)?)
        .with_context(|| format!("writing live-fidelity JSON to {output_path}"))
}

/// Wait for SIGTERM (Unix only). On non-Unix platforms, this never resolves.
#[cfg(unix)]
async fn sigterm_future() {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler")
        .recv()
        .await;
}

/// Sigterm future.
#[cfg(not(unix))]
async fn sigterm_future() {
    std::future::pending::<()>().await
}

#[cfg(test)]
#[path = "tests/cli_tests.rs"]
mod tests;
