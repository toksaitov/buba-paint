//! Long-running local process that executes dashboard research jobs.
//!
//! The binary polls the shared dashboard database, leases research steps, and
//! runs the local command-backed worker loop. It is intended for Compose-managed
//! deployments on research machines or for one-shot local verification runs.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;

use buba_dashboard::db::DashboardDb;
use buba_dashboard::research_pipeline::{
    BubaPaintCommand, ProcessCommandExecutor, ResearchPipelineConfig,
};
use buba_dashboard::research_transfer::{ArtifactTransferConfig, ArtifactTransferWorker};
use buba_dashboard::research_worker::LocalResearchWorker;

#[derive(Parser)]
#[command(name = "buba-research-worker", version)]
struct Cli {
    /// Dashboard `SQLite` database path shared with the dashboard backend.
    #[arg(
        long,
        env = "DASHBOARD_DB_PATH",
        default_value = "/runtime/dashboard.db"
    )]
    db_path: String,

    /// Research work root for artifacts, prepared DBs, reports, and scratch outputs.
    #[arg(long, env = "BUBA_RESEARCH_WORK_ROOT", default_value = "/research")]
    work_root: String,

    /// Repository root used as command working directory when cargo-backed execution is selected.
    #[arg(long, env = "BUBA_RESEARCH_REPO_ROOT", default_value = ".")]
    repo_root: String,

    /// Stable worker ID used in research step leases.
    #[arg(
        long,
        env = "BUBA_RESEARCH_WORKER_ID",
        default_value = "research-worker-local"
    )]
    worker_id: String,

    /// Lease duration in milliseconds for one claimed research step.
    #[arg(long, env = "BUBA_RESEARCH_LEASE_MS", default_value_t = 300_000)]
    lease_ms: u64,

    /// Idle poll interval in milliseconds.
    #[arg(long, env = "BUBA_RESEARCH_POLL_MS", default_value_t = 5_000)]
    poll_ms: u64,

    /// Maximum steps to process before returning to the idle poll loop.
    #[arg(long, env = "BUBA_RESEARCH_MAX_STEPS_PER_TICK", default_value_t = 1)]
    max_steps_per_tick: usize,

    /// Maximum artifact transfers to process before returning to the idle poll loop.
    #[arg(
        long,
        env = "BUBA_RESEARCH_MAX_TRANSFERS_PER_TICK",
        default_value_t = 1
    )]
    max_transfers_per_tick: usize,

    /// Optional direct buba-paint binary path. When omitted, cargo-backed local execution is used.
    #[arg(long, env = "BUBA_RESEARCH_PAINT_BIN")]
    paint_bin: Option<String>,

    /// Optional direct rsync binary path for remote artifact transfers.
    #[arg(long, env = "BUBA_RESEARCH_RSYNC_BIN", default_value = "rsync")]
    rsync_bin: String,

    /// Optional remote shell command passed to rsync with `-e`.
    #[arg(long, env = "BUBA_RESEARCH_RSYNC_SSH")]
    rsync_ssh: Option<String>,

    /// Whether this worker should claim queued artifact transfers.
    #[arg(long, env = "BUBA_RESEARCH_TRANSFERS_ENABLED", default_value_t = true)]
    transfers_enabled: bool,

    /// Running transfer age in milliseconds before restart recovery requeues it; zero disables recovery.
    #[arg(
        long,
        env = "BUBA_RESEARCH_TRANSFER_STALE_MS",
        default_value_t = 1_800_000
    )]
    transfer_stale_ms: u64,

    /// Process available work once and then exit.
    #[arg(long, env = "BUBA_RESEARCH_RUN_ONCE", default_value_t = false)]
    run_once: bool,

    /// Optional central dashboard URL for remote worker heartbeat reporting.
    #[arg(long, env = "BUBA_RESEARCH_CONTROLLER_URL")]
    controller_url: Option<String>,

    /// Machine ID updated by remote worker heartbeats.
    #[arg(long, env = "BUBA_RESEARCH_MACHINE_ID", default_value = "research")]
    machine_id: String,

    /// Shared token accepted by the central dashboard worker heartbeat endpoint.
    #[arg(long, env = "BUBA_RESEARCH_WORKER_TOKEN")]
    worker_token: Option<String>,

    /// Minimum interval in milliseconds between remote heartbeat posts.
    #[arg(long, env = "BUBA_RESEARCH_HEARTBEAT_MS", default_value_t = 30_000)]
    heartbeat_ms: u64,
}

/// Optional remote heartbeat configuration.
struct RemoteHeartbeatConfig {
    controller_url: String,
    machine_id: String,
    worker_id: String,
    worker_token: String,
    worker_version: String,
    interval: Duration,
}

/// Remote heartbeat sender for central dashboard machine status.
struct RemoteHeartbeat {
    config: Option<RemoteHeartbeatConfig>,
    client: reqwest::Client,
    last_sent: Option<Instant>,
}

/// Run the local research worker loop.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing();

    let db = Arc::new(DashboardDb::new(&cli.db_path)?);
    let pipeline = build_pipeline(&cli)?;
    let worker = LocalResearchWorker::new(cli.worker_id.clone(), cli.lease_ms)?;
    let transfer_worker = build_transfer_worker(&cli)?;
    let executor = ProcessCommandExecutor;
    let mut heartbeat = RemoteHeartbeat::from_cli(&cli)?;
    send_heartbeat(
        &mut heartbeat,
        "online",
        serde_json::json!({"phase":"startup"}),
        true,
    )
    .await;

    loop {
        if machine_work_disabled(&db, &cli.machine_id).await? {
            send_heartbeat(
                &mut heartbeat,
                "idle",
                serde_json::json!({"disabled":true,"processed_last_tick":0}),
                cli.run_once,
            )
            .await;
            if cli.run_once {
                tracing::info!("research worker run-once skipped because machine is disabled");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(cli.poll_ms)).await;
            continue;
        }
        let transfers_processed = if cli.transfers_enabled {
            transfer_worker
                .run_until_idle(&db, cli.max_transfers_per_tick)
                .await
                .context("running artifact transfer tick")?
        } else {
            0
        };
        let processed = worker
            .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, cli.max_steps_per_tick)
            .await
            .context("running research worker tick")?;
        let total_processed = transfers_processed + processed;
        let heartbeat_status = if total_processed == 0 { "idle" } else { "busy" };
        send_heartbeat(
            &mut heartbeat,
            heartbeat_status,
            serde_json::json!({
                "processed_last_tick": processed,
                "transfers_processed_last_tick": transfers_processed,
                "max_steps_per_tick": cli.max_steps_per_tick,
                "max_transfers_per_tick": cli.max_transfers_per_tick,
            }),
            cli.run_once,
        )
        .await;
        if cli.run_once {
            tracing::info!(
                processed,
                transfers_processed,
                "research worker run-once completed"
            );
            return Ok(());
        }
        if total_processed == 0 {
            tokio::time::sleep(Duration::from_millis(cli.poll_ms)).await;
        }
    }
}

/// Initialize process logging.
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

/// Build the research pipeline configuration from CLI and environment values.
fn build_pipeline(cli: &Cli) -> anyhow::Result<ResearchPipelineConfig> {
    let config =
        ResearchPipelineConfig::new(resolve_root(&cli.repo_root)?, resolve_root(&cli.work_root)?)?;
    Ok(match &cli.paint_bin {
        Some(path) => config.with_buba_paint_command(BubaPaintCommand {
            program: PathBuf::from(path),
            fixed_args: Vec::new(),
        }),
        None => config,
    })
}

/// Build the artifact transfer worker from CLI and environment values.
fn build_transfer_worker(cli: &Cli) -> anyhow::Result<ArtifactTransferWorker> {
    let config = ArtifactTransferConfig::new(resolve_root(&cli.work_root)?, &cli.machine_id)?
        .with_rsync_program(PathBuf::from(&cli.rsync_bin))
        .with_rsync_ssh(cli.rsync_ssh.clone())
        .with_stale_after_ms(if cli.transfer_stale_ms == 0 {
            None
        } else {
            Some(cli.transfer_stale_ms)
        });
    Ok(ArtifactTransferWorker::new(config))
}

/// Resolve a configured root against the current directory when it is relative.
fn resolve_root(path: &str) -> anyhow::Result<PathBuf> {
    let value = PathBuf::from(path);
    if value.is_absolute() {
        return Ok(value);
    }
    Ok(std::env::current_dir()?.join(value))
}

/// Return whether the configured machine is disabled for new work.
async fn machine_work_disabled(db: &DashboardDb, machine_id: &str) -> anyhow::Result<bool> {
    let machine = db
        .get_research_machine(machine_id)
        .await
        .with_context(|| format!("loading research machine '{machine_id}'"))?;
    Ok(machine.is_some_and(|machine| machine.status == "disabled"))
}

impl RemoteHeartbeat {
    /// Build heartbeat state from CLI options.
    fn from_cli(cli: &Cli) -> anyhow::Result<Self> {
        let config = match optional_value(cli.controller_url.as_deref()) {
            Some(controller_url) => {
                if cli.heartbeat_ms == 0 {
                    anyhow::bail!("BUBA_RESEARCH_HEARTBEAT_MS must be positive");
                }
                let worker_token = optional_value(cli.worker_token.as_deref()).ok_or_else(|| {
                    anyhow::anyhow!(
                        "BUBA_RESEARCH_WORKER_TOKEN is required when BUBA_RESEARCH_CONTROLLER_URL is set"
                    )
                })?;
                Some(RemoteHeartbeatConfig {
                    controller_url: controller_url.trim_end_matches('/').to_string(),
                    machine_id: cli.machine_id.trim().to_string(),
                    worker_id: cli.worker_id.trim().to_string(),
                    worker_token,
                    worker_version: env!("CARGO_PKG_VERSION").to_string(),
                    interval: Duration::from_millis(cli.heartbeat_ms),
                })
            }
            None => None,
        };
        Ok(Self {
            config,
            client: reqwest::Client::new(),
            last_sent: None,
        })
    }

    /// Send a heartbeat when forced or when the configured interval elapsed.
    async fn send_if_due(
        &mut self,
        status: &str,
        details: serde_json::Value,
        force: bool,
    ) -> anyhow::Result<()> {
        let Some(config) = &self.config else {
            return Ok(());
        };
        let now = Instant::now();
        if !force
            && self
                .last_sent
                .is_some_and(|last_sent| now.duration_since(last_sent) < config.interval)
        {
            return Ok(());
        }
        let url = format!("{}/api/research/workers/heartbeat", config.controller_url);
        let response = self
            .client
            .post(url)
            .header("x-buba-research-worker-token", &config.worker_token)
            .json(&serde_json::json!({
                "machine_id": config.machine_id,
                "worker_id": config.worker_id,
                "worker_version": config.worker_version,
                "status": status,
                "details": details,
            }))
            .send()
            .await
            .context("sending research worker heartbeat")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("research worker heartbeat failed with {status}: {body}");
        }
        self.last_sent = Some(now);
        Ok(())
    }
}

/// Send a heartbeat and keep the worker alive if central status reporting fails.
async fn send_heartbeat(
    heartbeat: &mut RemoteHeartbeat,
    status: &str,
    details: serde_json::Value,
    force: bool,
) {
    if let Err(error) = heartbeat.send_if_due(status, details, force).await {
        tracing::warn!(?error, "research worker heartbeat failed");
    }
}

/// Normalize optional string settings.
fn optional_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Build a minimal CLI for heartbeat tests.
    fn test_cli() -> Cli {
        Cli {
            db_path: ":memory:".to_string(),
            work_root: "/tmp/research".to_string(),
            repo_root: ".".to_string(),
            worker_id: "worker-a".to_string(),
            lease_ms: 1_000,
            poll_ms: 1_000,
            max_steps_per_tick: 1,
            max_transfers_per_tick: 1,
            paint_bin: None,
            rsync_bin: "rsync".to_string(),
            rsync_ssh: None,
            transfers_enabled: true,
            transfer_stale_ms: 1_800_000,
            run_once: false,
            controller_url: None,
            machine_id: "research".to_string(),
            worker_token: None,
            heartbeat_ms: 30_000,
        }
    }

    /// Verifies heartbeat config is disabled without a controller URL.
    #[test]
    fn remote_heartbeat_config_is_optional() {
        let cli = test_cli();
        let heartbeat = RemoteHeartbeat::from_cli(&cli).unwrap();

        assert!(heartbeat.config.is_none());
    }

    /// Verifies controller URL requires a worker token.
    #[test]
    fn remote_heartbeat_config_requires_token_with_controller() {
        let mut cli = test_cli();
        cli.controller_url = Some("http://localhost:3001".to_string());

        let result = RemoteHeartbeat::from_cli(&cli);

        assert!(result.is_err());
    }

    /// Verifies a configured heartbeat posts to the central dashboard endpoint.
    #[tokio::test]
    async fn remote_heartbeat_posts_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/research/workers/heartbeat"))
            .and(header("x-buba-research-worker-token", "secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "machine": {"id": "research"}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let mut cli = test_cli();
        cli.controller_url = Some(server.uri());
        cli.worker_token = Some("secret".to_string());
        let mut heartbeat = RemoteHeartbeat::from_cli(&cli).unwrap();

        heartbeat
            .send_if_due("idle", serde_json::json!({"queue_depth": 0}), true)
            .await
            .unwrap();
    }
}
