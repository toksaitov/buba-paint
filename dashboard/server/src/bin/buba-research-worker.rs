//! Long-running local process that executes dashboard research jobs.
//!
//! The binary polls the shared dashboard database, leases research steps, and
//! runs the local command-backed worker loop. It is intended for Compose-managed
//! deployments on research machines or for one-shot local verification runs.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use buba_machine_telemetry::MachineSampler;
use clap::Parser;

use buba_dashboard::db::{
    DashboardDb, ResearchMachineHeartbeatRecord, ResearchMachineTelemetryUpdate,
};
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
#[derive(Clone)]
struct RemoteHeartbeatConfig {
    controller_url: String,
    worker_token: String,
}

/// Local and optional remote heartbeat sender for research worker telemetry.
#[derive(Clone)]
struct RemoteHeartbeat {
    config: Option<RemoteHeartbeatConfig>,
    client: reqwest::Client,
    machine_id: String,
    worker_id: String,
    worker_version: String,
    interval: Duration,
}

/// Shared worker activity state reported with each telemetry heartbeat.
#[derive(Debug, Clone, serde::Serialize)]
struct WorkerActivity {
    /// Worker status reported to the machine row.
    status: String,
    /// Coarse worker phase for operator diagnostics.
    phase: String,
    /// Whether the configured machine is disabled for work.
    disabled: bool,
    /// Number of research job steps processed by the latest completed tick.
    processed_last_tick: usize,
    /// Number of artifact transfers processed by the latest completed tick.
    transfers_processed_last_tick: usize,
    /// Configured maximum job steps per worker tick.
    max_steps_per_tick: usize,
    /// Configured maximum transfers per worker tick.
    max_transfers_per_tick: usize,
    /// Whether transfer claiming is enabled for this worker.
    transfers_enabled: bool,
    /// Configured telemetry heartbeat interval.
    heartbeat_interval_ms: u64,
    /// Latest loop error, if the worker path failed.
    last_loop_error: Option<String>,
}

/// Runtime dependencies shared by the worker loop.
struct WorkerRuntime {
    db: Arc<DashboardDb>,
    sampler: Arc<MachineSampler>,
    pipeline: ResearchPipelineConfig,
    worker: LocalResearchWorker,
    transfer_worker: ArtifactTransferWorker,
    executor: ProcessCommandExecutor,
    heartbeat: RemoteHeartbeat,
    activity: Arc<Mutex<WorkerActivity>>,
}

/// Run the local research worker loop.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing();
    let runtime = WorkerRuntime::from_cli(&cli)?;
    send_heartbeat(
        &runtime.db,
        &runtime.heartbeat,
        &runtime.sampler,
        &activity_snapshot(&runtime.activity),
    )
    .await;
    let _heartbeat_task = (!cli.run_once).then(|| runtime.spawn_heartbeat_loop());
    run_worker_loop(&cli, &runtime).await
}

impl WorkerRuntime {
    /// Build worker runtime dependencies from CLI configuration.
    fn from_cli(cli: &Cli) -> anyhow::Result<Self> {
        let db = Arc::new(DashboardDb::new(&cli.db_path)?);
        let work_root = resolve_root(&cli.work_root)?;
        std::fs::create_dir_all(&work_root).context("creating research work root for telemetry")?;
        let sampler = MachineSampler::start(work_root);
        let pipeline = build_pipeline(cli)?;
        let worker = LocalResearchWorker::new(cli.worker_id.clone(), cli.lease_ms)?;
        let transfer_worker = build_transfer_worker(cli)?;
        let executor = ProcessCommandExecutor;
        let heartbeat = RemoteHeartbeat::from_cli(cli)?;
        let activity = Arc::new(Mutex::new(WorkerActivity::from_cli(cli)));
        Ok(Self {
            db,
            sampler,
            pipeline,
            worker,
            transfer_worker,
            executor,
            heartbeat,
            activity,
        })
    }

    /// Spawn periodic telemetry publishing independent of the work loop.
    fn spawn_heartbeat_loop(&self) -> tokio::task::JoinHandle<()> {
        spawn_heartbeat_loop(
            Arc::clone(&self.db),
            self.heartbeat.clone(),
            Arc::clone(&self.sampler),
            Arc::clone(&self.activity),
        )
    }
}

/// Run the main polling loop until run-once completion or fatal worker error.
async fn run_worker_loop(cli: &Cli, runtime: &WorkerRuntime) -> anyhow::Result<()> {
    loop {
        if machine_work_disabled(&runtime.db, &cli.machine_id).await? {
            if handle_disabled_tick(cli, runtime).await {
                return Ok(());
            }
            continue;
        }
        update_activity(&runtime.activity, |activity| {
            activity.status = "busy".to_string();
            activity.phase = "processing".to_string();
            activity.disabled = false;
            activity.last_loop_error = None;
        });
        let (processed, transfers_processed) = match run_work_tick(cli, runtime).await {
            Ok(result) => result,
            Err(error) => {
                update_activity(&runtime.activity, |activity| {
                    activity.status = "error".to_string();
                    activity.phase = "error".to_string();
                    activity.last_loop_error = Some(error.to_string());
                });
                send_heartbeat(
                    &runtime.db,
                    &runtime.heartbeat,
                    &runtime.sampler,
                    &activity_snapshot(&runtime.activity),
                )
                .await;
                return Err(error);
            }
        };
        let total_processed = transfers_processed + processed;
        update_activity(&runtime.activity, |activity| {
            activity.status = if total_processed == 0 {
                "idle".to_string()
            } else {
                "busy".to_string()
            };
            activity.phase = if total_processed == 0 {
                "idle".to_string()
            } else {
                "processed".to_string()
            };
            activity.disabled = false;
            activity.processed_last_tick = processed;
            activity.transfers_processed_last_tick = transfers_processed;
            activity.last_loop_error = None;
        });
        if cli.run_once {
            send_heartbeat(
                &runtime.db,
                &runtime.heartbeat,
                &runtime.sampler,
                &activity_snapshot(&runtime.activity),
            )
            .await;
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

/// Handle one disabled-machine loop tick and return true when the worker should exit.
async fn handle_disabled_tick(cli: &Cli, runtime: &WorkerRuntime) -> bool {
    update_activity(&runtime.activity, |activity| {
        activity.status = "idle".to_string();
        activity.phase = "disabled".to_string();
        activity.disabled = true;
        activity.processed_last_tick = 0;
        activity.transfers_processed_last_tick = 0;
        activity.last_loop_error = None;
    });
    if cli.run_once {
        send_heartbeat(
            &runtime.db,
            &runtime.heartbeat,
            &runtime.sampler,
            &activity_snapshot(&runtime.activity),
        )
        .await;
        tracing::info!("research worker run-once skipped because machine is disabled");
        return true;
    }
    tokio::time::sleep(Duration::from_millis(cli.poll_ms)).await;
    false
}

/// Process one transfer and job work tick.
async fn run_work_tick(cli: &Cli, runtime: &WorkerRuntime) -> anyhow::Result<(usize, usize)> {
    let transfers_processed = if cli.transfers_enabled {
        runtime
            .transfer_worker
            .run_until_idle(&runtime.db, cli.max_transfers_per_tick)
            .await
            .context("running artifact transfer tick")?
    } else {
        0
    };
    let processed = runtime
        .worker
        .run_local_with_pipeline_until_idle(
            &runtime.db,
            &runtime.pipeline,
            &runtime.executor,
            cli.max_steps_per_tick,
        )
        .await
        .context("running research worker tick")?;
    Ok((processed, transfers_processed))
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

impl WorkerActivity {
    /// Build the initial activity payload from CLI configuration.
    fn from_cli(cli: &Cli) -> Self {
        Self {
            status: "online".to_string(),
            phase: "startup".to_string(),
            disabled: false,
            processed_last_tick: 0,
            transfers_processed_last_tick: 0,
            max_steps_per_tick: cli.max_steps_per_tick,
            max_transfers_per_tick: cli.max_transfers_per_tick,
            transfers_enabled: cli.transfers_enabled,
            heartbeat_interval_ms: cli.heartbeat_ms,
            last_loop_error: None,
        }
    }
}

/// Mutate shared worker activity without panicking on a poisoned mutex.
fn update_activity(
    activity: &Arc<Mutex<WorkerActivity>>,
    update: impl FnOnce(&mut WorkerActivity),
) {
    let mut guard = match activity.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    update(&mut guard);
}

/// Clone shared worker activity without panicking on a poisoned mutex.
fn activity_snapshot(activity: &Arc<Mutex<WorkerActivity>>) -> WorkerActivity {
    match activity.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// Spawn periodic telemetry publishing independent of the work loop.
fn spawn_heartbeat_loop(
    db: Arc<DashboardDb>,
    heartbeat: RemoteHeartbeat,
    sampler: Arc<MachineSampler>,
    activity: Arc<Mutex<WorkerActivity>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(heartbeat.interval()).await;
            let activity = activity_snapshot(&activity);
            send_heartbeat(&db, &heartbeat, &sampler, &activity).await;
        }
    })
}

impl RemoteHeartbeat {
    /// Build heartbeat state from CLI options.
    fn from_cli(cli: &Cli) -> anyhow::Result<Self> {
        if cli.heartbeat_ms == 0 {
            anyhow::bail!("BUBA_RESEARCH_HEARTBEAT_MS must be positive");
        }
        let config = match optional_value(cli.controller_url.as_deref()) {
            Some(controller_url) => {
                let worker_token = optional_value(cli.worker_token.as_deref()).ok_or_else(|| {
                    anyhow::anyhow!(
                        "BUBA_RESEARCH_WORKER_TOKEN is required when BUBA_RESEARCH_CONTROLLER_URL is set"
                    )
                })?;
                Some(RemoteHeartbeatConfig {
                    controller_url: controller_url.trim_end_matches('/').to_string(),
                    worker_token,
                })
            }
            None => None,
        };
        Ok(Self {
            config,
            client: reqwest::Client::new(),
            machine_id: cli.machine_id.trim().to_string(),
            worker_id: cli.worker_id.trim().to_string(),
            worker_version: env!("CARGO_PKG_VERSION").to_string(),
            interval: Duration::from_millis(cli.heartbeat_ms),
        })
    }

    /// Return the configured telemetry publishing interval.
    fn interval(&self) -> Duration {
        self.interval
    }

    /// Post one heartbeat payload to the central dashboard when configured.
    async fn post_to_controller(
        &self,
        status: &str,
        details: &serde_json::Value,
        sampler: &MachineSampler,
        sampler_health: &buba_machine_telemetry::MachineSamplerHealth,
        samples: &[buba_machine_telemetry::MachineSample],
    ) -> anyhow::Result<()> {
        let Some(config) = &self.config else {
            return Ok(());
        };
        let url = format!("{}/api/research/workers/heartbeat", config.controller_url);
        let response = self
            .client
            .post(url)
            .header("x-buba-research-worker-token", &config.worker_token)
            .json(&serde_json::json!({
                "machine_id": &self.machine_id,
                "worker_id": &self.worker_id,
                "worker_version": &self.worker_version,
                "status": status,
                "details": details,
                "host": sampler.host(),
                "sampler": sampler_health,
                "samples": samples,
                "activity": details,
            }))
            .send()
            .await
            .context("sending research worker heartbeat")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("research worker heartbeat failed with {status}: {body}");
        }
        Ok(())
    }
}

/// Publish one heartbeat to the local DB and optionally to a central dashboard.
async fn publish_heartbeat_once(
    db: &DashboardDb,
    heartbeat: &RemoteHeartbeat,
    sampler: &MachineSampler,
    activity: &WorkerActivity,
) -> anyhow::Result<()> {
    let details = serde_json::to_value(activity).context("serializing research worker activity")?;
    let snapshot = sampler.snapshot();
    let sampler_health = snapshot.health();
    let telemetry = ResearchMachineTelemetryUpdate {
        host: Some(sampler.host()),
        sampler: Some(&sampler_health),
        samples: &snapshot.history,
        activity: Some(&details),
    };
    let record = ResearchMachineHeartbeatRecord {
        machine_id: &heartbeat.machine_id,
        worker_id: &heartbeat.worker_id,
        worker_version: Some(&heartbeat.worker_version),
        status: &activity.status,
        details: Some(&details),
        telemetry,
    };
    db.record_research_machine_heartbeat_with_telemetry(&record)
        .await
        .context("recording local research worker heartbeat")?;
    heartbeat
        .post_to_controller(
            &activity.status,
            &details,
            sampler,
            &sampler_health,
            &snapshot.history,
        )
        .await?;
    Ok(())
}

/// Send a heartbeat and keep the worker alive if telemetry reporting fails.
async fn send_heartbeat(
    db: &DashboardDb,
    heartbeat: &RemoteHeartbeat,
    sampler: &MachineSampler,
    activity: &WorkerActivity,
) {
    if let Err(error) = publish_heartbeat_once(db, heartbeat, sampler, activity).await {
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
    use buba_machine_telemetry::{HostIdentity, MachineSample, MachineSamplerState};
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

    /// Build a seeded sampler for deterministic heartbeat tests.
    fn test_sampler() -> Arc<MachineSampler> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(10_000);
        let mut state = MachineSamplerState::new();
        state.push(MachineSample {
            sampled_at_ms: now_ms,
            cpu_percent: 12.5,
            per_core_cpu: vec![12.5, 8.0],
            load_one: Some(0.5),
            load_five: Some(0.4),
            load_fifteen: Some(0.3),
            mem_used_bytes: 512,
            mem_total_bytes: 1_024,
            mem_available_bytes: 512,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
            disk_used_bytes: 2_048,
            disk_total_bytes: 4_096,
            disk_mount: "/tmp".to_string(),
        });
        MachineSampler::with_seeded_state(
            HostIdentity {
                hostname: "testing".to_string(),
                os_name: "Linux".to_string(),
                os_version: "test".to_string(),
                kernel_version: "test".to_string(),
                cpu_count: 2,
                total_ram_bytes: 1_024,
            },
            state,
            900,
            PathBuf::from("/tmp/research"),
        )
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
        let heartbeat = RemoteHeartbeat::from_cli(&cli).unwrap();
        let db = DashboardDb::new(":memory:").unwrap();
        let sampler = test_sampler();
        let mut activity = WorkerActivity::from_cli(&cli);
        activity.status = "idle".to_string();
        activity.phase = "idle".to_string();

        publish_heartbeat_once(&db, &heartbeat, &sampler, &activity)
            .await
            .unwrap();
    }

    /// Verifies local telemetry is recorded when no controller URL is configured.
    #[tokio::test]
    async fn heartbeat_without_controller_records_local_telemetry() {
        let cli = test_cli();
        let heartbeat = RemoteHeartbeat::from_cli(&cli).unwrap();
        let db = DashboardDb::new(":memory:").unwrap();
        let sampler = test_sampler();
        let mut activity = WorkerActivity::from_cli(&cli);
        activity.status = "idle".to_string();
        activity.phase = "idle".to_string();

        publish_heartbeat_once(&db, &heartbeat, &sampler, &activity)
            .await
            .unwrap();

        let telemetry = db
            .get_research_machine_telemetry("research", None, None)
            .await
            .unwrap();
        assert_eq!(telemetry.state.unwrap().worker_id, "worker-a");
        assert_eq!(telemetry.samples.len(), 1);
    }

    /// Verifies controller failures do not prevent local telemetry persistence.
    #[tokio::test]
    async fn failed_controller_heartbeat_is_nonfatal() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/research/workers/heartbeat"))
            .and(header("x-buba-research-worker-token", "secret"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .expect(1)
            .mount(&server)
            .await;

        let mut cli = test_cli();
        cli.controller_url = Some(server.uri());
        cli.worker_token = Some("secret".to_string());
        let heartbeat = RemoteHeartbeat::from_cli(&cli).unwrap();
        let db = DashboardDb::new(":memory:").unwrap();
        let sampler = test_sampler();
        let mut activity = WorkerActivity::from_cli(&cli);
        activity.status = "idle".to_string();
        activity.phase = "idle".to_string();

        send_heartbeat(&db, &heartbeat, &sampler, &activity).await;

        let telemetry = db
            .get_research_machine_telemetry("research", None, None)
            .await
            .unwrap();
        assert_eq!(telemetry.state.unwrap().worker_status, "idle");
    }
}
