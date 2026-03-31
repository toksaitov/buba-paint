use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::middleware;
use axum::routing::{get, post};
use clap::Parser;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

use buba_agent::api::{self, AppState};
use buba_agent::auth::{SharedSecret, require_secret};
use buba_agent::db_reader::DbReader;
use buba_agent::process_manager::{ChildProcessManager, NoopProcessManager, ProcessConfig};
use buba_agent::ws as agent_ws;

#[derive(Parser)]
#[command(name = "buba-agent", version)]
struct Cli {
    /// Path to the bot's `SQLite` database
    #[arg(long)]
    db_path: String,

    /// Port to listen on
    #[arg(long, default_value = "9090")]
    port: u16,

    /// Database polling interval in milliseconds
    #[arg(long, default_value = "2000")]
    poll_interval: u64,

    /// Shell command to start the bot (e.g. "cargo run --release -- live --db-path /data/bot.db")
    #[arg(long)]
    bot_cmd: Option<String>,

    /// Max log lines kept in memory
    #[arg(long, default_value = "10000")]
    log_buffer_size: usize,

    /// Auto-restart limit before giving up
    #[arg(long, default_value = "5")]
    max_restarts: u32,

    /// Delay between auto-restarts in milliseconds
    #[arg(long, default_value = "3000")]
    restart_delay: u64,

    /// Disable process control (monitoring-only mode)
    #[arg(long)]
    monitor_only: bool,

    /// Path to the bot's log file (used in monitor-only mode)
    #[arg(long)]
    log_path: Option<String>,
}

/// Starts the agent `HTTP` and `WebSocket` server for one bot database.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let secret = std::env::var("AGENT_SECRET").unwrap_or_else(|_| {
        tracing::warn!("AGENT_SECRET not set — using empty string (development only)");
        String::new()
    });

    let db = Arc::new(DbReader::new(&cli.db_path)?);

    let bot: Arc<dyn buba_agent::process_manager::ProcessManager> = if cli.monitor_only {
        Arc::new(NoopProcessManager::new(cli.log_path))
    } else if let Some(cmd) = &cli.bot_cmd {
        let command =
            shell_words::split(cmd).map_err(|e| anyhow::anyhow!("invalid --bot-cmd: {e}"))?;
        if command.is_empty() {
            anyhow::bail!("--bot-cmd must not be empty");
        }
        Arc::new(ChildProcessManager::new(ProcessConfig {
            command,
            max_restarts: cli.max_restarts,
            restart_delay: Duration::from_millis(cli.restart_delay),
            log_buffer_size: cli.log_buffer_size,
        }))
    } else {
        anyhow::bail!("either --bot-cmd or --monitor-only is required");
    };

    let (ws_tx, _) = broadcast::channel(256);

    agent_ws::spawn_poller(Arc::clone(&db), cli.poll_interval, ws_tx.clone());

    let state = AppState { db, bot, ws_tx };

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/api/status", get(api::get_status))
        .route("/api/trades", get(api::get_trades))
        .route("/api/balance", get(api::get_balance))
        .route("/api/signals", get(api::get_signals))
        .route("/api/stats", get(api::get_stats))
        .route("/api/bot/logs", get(api::get_logs))
        .route("/api/bot/start", post(api::bot_start))
        .route("/api/bot/stop", post(api::bot_stop))
        .route("/api/bot/restart", post(api::bot_restart))
        .route("/api/bot/status", get(api::bot_status))
        .route("/ws/live", get(api::ws_live))
        .layer(middleware::from_fn(require_secret))
        .layer(axum::Extension(SharedSecret(secret)))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cli.port);
    tracing::info!("buba-agent listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
