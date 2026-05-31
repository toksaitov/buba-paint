//! Local research pipeline planning and command construction.
//!
//! This module turns durable dashboard jobs into concrete filesystem paths,
//! allowlisted `buba-paint` command invocations, report files, and scratch
//! archive operations. It deliberately keeps orchestration state in the
//! database layer and only owns local planning/execution primitives.

use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::process::ExitStatus;
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncReadExt;

use crate::db::{DashboardDb, ResearchArtifact, ResearchJob, ResearchJobStep};
use crate::error::DashboardError;
use crate::research_artifacts;
use crate::research_reports;

/// Static configuration required to plan and run local research jobs.
#[derive(Debug, Clone)]
pub struct ResearchPipelineConfig {
    /// Repository root used as the current directory for `buba-paint` commands.
    pub repo_root: PathBuf,
    /// Root directory for artifacts, job outputs, reports, and scratch files.
    pub work_root: PathBuf,
    /// Command launcher used for all allowlisted `buba-paint` invocations.
    pub buba_paint: BubaPaintCommand,
}

/// Configurable launcher prefix for running `buba-paint`.
#[derive(Debug, Clone)]
pub struct BubaPaintCommand {
    /// Program path, such as `cargo` or a direct `buba-paint` binary.
    pub program: PathBuf,
    /// Fixed arguments inserted before the per-step command name.
    pub fixed_args: Vec<String>,
}

/// Allowlisted `buba-paint` operations the research worker may execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubaPaintCommandKind {
    /// Validate that replay data is suitable for research use.
    ValidateReplayData,
    /// Validate that prepared inputs are usable by backtests.
    ValidateBacktestInput,
    /// Build a normalized backtest input database.
    PrepareBacktestInput,
    /// Run a single current-parameter backtest.
    RunBacktest,
    /// Run a sweep over one or more configured dimensions.
    RunSweep,
}

/// Serializable command specification passed to a command executor.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommandSpec {
    /// Program to execute.
    pub program: String,
    /// Ordered command-line arguments.
    pub args: Vec<String>,
    /// Current working directory for the process.
    pub cwd: String,
}

/// Captured output from a research command.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommandOutput {
    /// Whether the process exited successfully.
    pub success: bool,
    /// Process exit status code when one is available.
    pub status_code: Option<i32>,
    /// Captured standard output as lossy UTF-8 text.
    pub stdout: String,
    /// Captured standard error as lossy UTF-8 text.
    pub stderr: String,
    /// Whether the process was terminated after an operator cancellation.
    pub cancelled: bool,
}

/// Future returned by cancellable command executors.
pub type CommandExecutionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CommandOutput, DashboardError>> + Send + 'a>>;

/// Durable cancellation context for one running research command.
pub struct CommandCancellation<'a> {
    /// Dashboard database used to observe job and step state.
    pub db: &'a DashboardDb,
    /// Job currently owning the command-backed step.
    pub job_id: &'a str,
    /// Step currently owning the command process.
    pub step_id: &'a str,
    /// Poll interval while the child is still running.
    pub poll_interval: Duration,
}

impl CommandCancellation<'_> {
    /// Return true once either the job or active step has been cancelled.
    pub async fn is_cancelled(&self) -> Result<bool, DashboardError> {
        let Some(job) = self.db.get_research_job(self.job_id).await? else {
            return Ok(true);
        };
        if job.status == "cancelled" {
            return Ok(true);
        }
        let steps = self.db.get_research_job_steps(self.job_id).await?;
        Ok(steps
            .iter()
            .any(|step| step.id == self.step_id && step.status == "cancelled"))
    }
}

/// Pluggable executor for local research commands.
pub trait ResearchCommandExecutor: Sync {
    /// Execute one command specification and return captured process output.
    fn execute(&self, command: &CommandSpec) -> Result<CommandOutput, DashboardError>;

    /// Execute one command while checking for durable operator cancellation.
    fn execute_supervised<'a>(
        &'a self,
        command: &'a CommandSpec,
        _cancellation: CommandCancellation<'a>,
    ) -> CommandExecutionFuture<'a> {
        Box::pin(async move { self.execute(command) })
    }
}

/// Production executor that runs commands as child processes.
pub struct ProcessCommandExecutor;

/// Fully resolved plan for one research job.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResearchPipelinePlan {
    /// Durable research job ID.
    pub job_id: String,
    /// Durable research job type.
    pub job_type: String,
    /// Artifact attached to this job, when one exists.
    pub artifact_id: Option<String>,
    /// Local artifact root used for input data.
    pub artifact_root: Option<PathBuf>,
    /// Job work directory under the configured work root.
    pub job_root: PathBuf,
    /// Input replay/backtest data database.
    pub data_db_path: PathBuf,
    /// Start time passed to the `buba-paint` CLI.
    pub start: String,
    /// End time passed to the `buba-paint` CLI.
    pub end: String,
    /// Prepared backtest database output path.
    pub prepared_db_output_path: PathBuf,
    /// Current-parameter backtest output path.
    pub backtest_output_path: PathBuf,
    /// Sweep output path.
    pub sweep_output_path: PathBuf,
    /// Generated report `JSON` path.
    pub report_json_path: PathBuf,
    /// Generated step-summary `CSV` path.
    pub report_csv_path: PathBuf,
    /// Starting balance passed to backtest commands.
    pub balance: f64,
    /// `--set` overrides passed through to `buba-paint`.
    pub sets: Vec<String>,
    /// `--sweep` dimensions passed through for sweep jobs.
    pub sweeps: Vec<String>,
    /// Whether scratch databases should be removed after report creation.
    pub archive_scratch: bool,
}

/// Summary of scratch database archive behavior.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArchiveSummary {
    /// Files removed from the job scratch directory.
    pub deleted_paths: Vec<String>,
    /// Expected scratch-family files that were already absent.
    pub skipped_paths: Vec<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawResearchJobParams {
    data_db_path: Option<String>,
    start: Option<String>,
    end: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    prepared_db_output_path: Option<String>,
    backtest_output_path: Option<String>,
    sweep_output_path: Option<String>,
    report_json_path: Option<String>,
    report_csv_path: Option<String>,
    balance: Option<f64>,
    #[serde(default, alias = "set", alias = "set_overrides")]
    sets: Vec<String>,
    #[serde(default, alias = "sweep", alias = "sweep_dimensions")]
    sweeps: Vec<String>,
    #[serde(default)]
    archive_scratch: bool,
}

impl ResearchPipelineConfig {
    /// Build a local-only pipeline config rooted in one repo and one work directory.
    pub fn new(
        repo_root: impl Into<PathBuf>,
        work_root: impl Into<PathBuf>,
    ) -> Result<Self, DashboardError> {
        let repo_root = repo_root.into();
        let work_root = work_root.into();
        if repo_root.as_os_str().is_empty() || work_root.as_os_str().is_empty() {
            return Err(DashboardError::BadRequest(
                "repo_root and work_root must not be empty".to_string(),
            ));
        }
        std::fs::create_dir_all(&work_root)
            .map_err(|e| DashboardError::Internal(format!("creating research work root: {e}")))?;
        Ok(Self {
            buba_paint: BubaPaintCommand::cargo_release(),
            repo_root: normalize_config_root(&repo_root)?,
            work_root: normalize_config_root(&work_root)?,
        })
    }

    /// Return a copy of this config with a custom command launcher.
    #[must_use]
    pub fn with_buba_paint_command(mut self, command: BubaPaintCommand) -> Self {
        self.buba_paint = command;
        self
    }

    /// Resolve one job into concrete paths and command options.
    pub fn plan_for_job(
        &self,
        job: &ResearchJob,
        artifact: Option<&ResearchArtifact>,
    ) -> Result<ResearchPipelinePlan, DashboardError> {
        let raw = parse_job_params(job.params_json.as_deref())?;
        let job_root = resolve_output_path(&self.work_root, &format!("jobs/{}", job.id))?;
        std::fs::create_dir_all(&job_root)
            .map_err(|e| DashboardError::Internal(format!("creating research job root: {e}")))?;
        let artifact_root = artifact.map(resolve_artifact_root).transpose()?;
        let data_db_path =
            resolve_data_db_path(raw.data_db_path.as_deref(), artifact_root.as_deref())?;
        let start = resolve_time(
            raw.start.as_deref(),
            raw.start_ms,
            artifact.and_then(|a| a.interval_start_ms),
            "start",
        )?;
        let end = resolve_time(
            raw.end.as_deref(),
            raw.end_ms,
            artifact.and_then(|a| a.interval_end_ms),
            "end",
        )?;
        let balance = raw.balance.unwrap_or(200.0);
        if !balance.is_finite() || balance <= 0.0 {
            return Err(DashboardError::BadRequest(
                "balance must be a positive finite number".to_string(),
            ));
        }
        validate_string_list("set override", &raw.sets)?;
        validate_string_list("sweep dimension", &raw.sweeps)?;
        if job.job_type == "sweep" && raw.sweeps.is_empty() {
            return Err(DashboardError::BadRequest(
                "sweep jobs require at least one sweep dimension".to_string(),
            ));
        }

        Ok(ResearchPipelinePlan {
            job_id: job.id.clone(),
            job_type: job.job_type.clone(),
            artifact_id: job.artifact_id.clone(),
            artifact_root,
            data_db_path,
            prepared_db_output_path: resolve_named_output(
                &job_root,
                raw.prepared_db_output_path.as_deref(),
                "prepared-backtest.db",
            )?,
            backtest_output_path: resolve_named_output(
                &job_root,
                raw.backtest_output_path.as_deref(),
                "backtest.db",
            )?,
            sweep_output_path: resolve_named_output(
                &job_root,
                raw.sweep_output_path.as_deref(),
                "sweep.csv",
            )?,
            report_json_path: resolve_named_output(
                &job_root,
                raw.report_json_path.as_deref(),
                "report.json",
            )?,
            report_csv_path: resolve_named_output(
                &job_root,
                raw.report_csv_path.as_deref(),
                "report.csv",
            )?,
            job_root,
            start,
            end,
            balance,
            sets: raw.sets,
            sweeps: raw.sweeps,
            archive_scratch: raw.archive_scratch,
        })
    }

    /// Build the allowlisted `buba-paint` command for one local research step.
    pub fn command_for_step(
        &self,
        kind: BubaPaintCommandKind,
        plan: &ResearchPipelinePlan,
    ) -> CommandSpec {
        let mut args = self.buba_paint.fixed_args.clone();
        match kind {
            BubaPaintCommandKind::ValidateReplayData => push_data_interval_args(
                &mut args,
                "validate-replay-data",
                &plan.data_db_path,
                &plan.start,
                &plan.end,
            ),
            BubaPaintCommandKind::ValidateBacktestInput => push_data_interval_args(
                &mut args,
                "validate-backtest-input",
                &plan.data_db_path,
                &plan.start,
                &plan.end,
            ),
            BubaPaintCommandKind::PrepareBacktestInput => {
                push_data_interval_args(
                    &mut args,
                    "prepare-backtest-input",
                    &plan.data_db_path,
                    &plan.start,
                    &plan.end,
                );
                args.push("--output".to_string());
                args.push(path_to_string(&plan.prepared_db_output_path));
            }
            BubaPaintCommandKind::RunBacktest => {
                push_data_interval_args(
                    &mut args,
                    "backtest",
                    &plan.prepared_db_output_path,
                    &plan.start,
                    &plan.end,
                );
                push_balance_and_sets(&mut args, plan.balance, &plan.sets);
                args.push("--output".to_string());
                args.push(path_to_string(&plan.backtest_output_path));
            }
            BubaPaintCommandKind::RunSweep => {
                push_data_interval_args(
                    &mut args,
                    "sweep",
                    &plan.prepared_db_output_path,
                    &plan.start,
                    &plan.end,
                );
                push_balance_and_sets(&mut args, plan.balance, &plan.sets);
                for sweep in &plan.sweeps {
                    args.push("--sweep".to_string());
                    args.push(sweep.clone());
                }
                args.push("--output".to_string());
                args.push(path_to_string(&plan.sweep_output_path));
            }
        }
        CommandSpec {
            program: path_to_string(&self.buba_paint.program),
            args,
            cwd: path_to_string(&self.repo_root),
        }
    }
}

impl BubaPaintCommand {
    /// Return the default cargo-backed `buba-paint` launcher.
    pub fn cargo_release() -> Self {
        Self {
            program: PathBuf::from("cargo"),
            fixed_args: vec![
                "run".to_string(),
                "-p".to_string(),
                "buba-paint".to_string(),
                "--release".to_string(),
                "--".to_string(),
            ],
        }
    }
}

impl ResearchCommandExecutor for ProcessCommandExecutor {
    /// Execute one local allowlisted command and capture its output.
    fn execute(&self, command: &CommandSpec) -> Result<CommandOutput, DashboardError> {
        let output = std::process::Command::new(&command.program)
            .args(&command.args)
            .current_dir(&command.cwd)
            .output()
            .map_err(|e| DashboardError::Internal(format!("executing research command: {e}")))?;
        Ok(CommandOutput {
            success: output.status.success(),
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            cancelled: false,
        })
    }

    /// Execute one command as a supervised child and terminate it on cancellation.
    fn execute_supervised<'a>(
        &'a self,
        command: &'a CommandSpec,
        cancellation: CommandCancellation<'a>,
    ) -> CommandExecutionFuture<'a> {
        Box::pin(async move {
            let mut process = tokio::process::Command::new(&command.program);
            process
                .args(&command.args)
                .current_dir(&command.cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            #[cfg(unix)]
            process.process_group(0);

            let mut child = process.spawn().map_err(|e| {
                DashboardError::Internal(format!("executing research command: {e}"))
            })?;

            let stdout = child.stdout.take().ok_or_else(|| {
                DashboardError::Internal("capturing research command stdout".to_string())
            })?;
            let stderr = child.stderr.take().ok_or_else(|| {
                DashboardError::Internal("capturing research command stderr".to_string())
            })?;
            let stdout_task = tokio::spawn(read_child_output(stdout));
            let stderr_task = tokio::spawn(read_child_output(stderr));

            let mut cancelled = false;
            let status = loop {
                if let Some(status) = child.try_wait().map_err(|e| {
                    DashboardError::Internal(format!("waiting for research command: {e}"))
                })? {
                    break status;
                }
                if cancellation.is_cancelled().await? {
                    cancelled = true;
                    break terminate_cancelled_child(&mut child).await?;
                }
                tokio::time::sleep(cancellation.poll_interval).await;
            };

            let stdout = join_child_output(stdout_task).await?;
            let stderr = join_child_output(stderr_task).await?;
            Ok(CommandOutput {
                success: status.success() && !cancelled,
                status_code: status.code(),
                stdout,
                stderr,
                cancelled,
            })
        })
    }
}

/// Terminate a running research child process after durable cancellation.
async fn terminate_cancelled_child(
    child: &mut tokio::process::Child,
) -> Result<ExitStatus, DashboardError> {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        let _ = send_process_group_signal(pid, "TERM");
        for _ in 0..10 {
            if let Some(status) = child.try_wait().map_err(|e| {
                DashboardError::Internal(format!("waiting for cancelled research command: {e}"))
            })? {
                return Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let _ = send_process_group_signal(pid, "KILL");
    }

    child.kill().await.map_err(|e| {
        DashboardError::Internal(format!("terminating cancelled research command: {e}"))
    })?;
    child.wait().await.map_err(|e| {
        DashboardError::Internal(format!("waiting for cancelled research command: {e}"))
    })
}

/// Send one signal to a Unix process group by child process ID.
#[cfg(unix)]
fn send_process_group_signal(pid: u32, signal: &str) -> Result<(), DashboardError> {
    let status = std::process::Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(format!("-{pid}"))
        .status()
        .map_err(|e| DashboardError::Internal(format!("signalling process group: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(DashboardError::Internal(format!(
            "signalling process group failed with status {:?}",
            status.code()
        )))
    }
}

/// Read one child output stream into a lossy UTF-8 string.
async fn read_child_output<R>(mut reader: R) -> Result<String, std::io::Error>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Join one child output task and convert stream or join failures.
async fn join_child_output(
    task: tokio::task::JoinHandle<Result<String, std::io::Error>>,
) -> Result<String, DashboardError> {
    task.await
        .map_err(|e| DashboardError::Internal(format!("joining command output task: {e}")))?
        .map_err(|e| DashboardError::Internal(format!("reading research command output: {e}")))
}

/// Write a JSON report and step-summary CSV for one completed research job.
pub fn write_report_files(
    plan: &ResearchPipelinePlan,
    steps: &[ResearchJobStep],
) -> Result<String, DashboardError> {
    create_parent_dir(&plan.report_json_path)?;
    create_parent_dir(&plan.report_csv_path)?;
    let documents = research_reports::build_report_documents(plan, steps)?;
    std::fs::write(&plan.report_json_path, &documents.full_json)
        .map_err(|e| DashboardError::Internal(format!("writing research report JSON: {e}")))?;
    std::fs::write(&plan.report_csv_path, &documents.csv)
        .map_err(|e| DashboardError::Internal(format!("writing research report CSV: {e}")))?;
    Ok(documents.summary_json)
}

/// Delete scratch `SQLite` outputs after reports are preserved.
pub fn archive_scratch_dbs(plan: &ResearchPipelinePlan) -> Result<ArchiveSummary, DashboardError> {
    let mut summary = ArchiveSummary {
        deleted_paths: Vec::new(),
        skipped_paths: Vec::new(),
    };
    for path in [&plan.prepared_db_output_path, &plan.backtest_output_path] {
        archive_db_family(&plan.job_root, path, &mut summary)?;
    }
    Ok(summary)
}

/// Parse optional research job params JSON.
fn parse_job_params(params_json: Option<&str>) -> Result<RawResearchJobParams, DashboardError> {
    match params_json {
        Some(value) if !value.trim().is_empty() => serde_json::from_str(value)
            .map_err(|e| DashboardError::BadRequest(format!("invalid research params_json: {e}"))),
        _ => Ok(RawResearchJobParams::default()),
    }
}

/// Resolve a configured root without allowing traversal syntax.
fn normalize_config_root(path: &Path) -> Result<PathBuf, DashboardError> {
    normalize_path(path)
}

/// Resolve an artifact root from stored artifact metadata.
fn resolve_artifact_root(artifact: &ResearchArtifact) -> Result<PathBuf, DashboardError> {
    if let Some(root) = artifact.artifact_root.as_deref() {
        return normalize_config_root(Path::new(root));
    }
    if let Some(manifest_path) = artifact.manifest_path.as_deref()
        && let Some(parent) = Path::new(manifest_path).parent()
    {
        return normalize_config_root(parent);
    }
    Err(DashboardError::BadRequest(format!(
        "artifact '{}' has no artifact_root or manifest_path",
        artifact.id
    )))
}

/// Resolve the input data DB path from params or the artifact manifest.
fn resolve_data_db_path(
    configured: Option<&str>,
    artifact_root: Option<&Path>,
) -> Result<PathBuf, DashboardError> {
    let root = artifact_root.ok_or_else(|| {
        DashboardError::BadRequest("research jobs require a local artifact root".to_string())
    })?;
    if let Some(path) = configured {
        return resolve_existing_artifact_path(root, path);
    }
    let manifest = research_artifacts::read_manifest(root)?;
    let file = manifest
        .files
        .iter()
        .find(|file| file.logical_name == "runtime_db")
        .or_else(|| manifest.files.iter().find(|file| file.kind == "sqlite"))
        .or_else(|| {
            manifest.files.iter().find(|file| {
                Path::new(&file.relative_path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("db"))
            })
        })
        .ok_or_else(|| {
            DashboardError::BadRequest(
                "artifact manifest does not include a runtime DB".to_string(),
            )
        })?;
    resolve_existing_artifact_path(root, &file.relative_path)
}

/// Resolve one existing path that must remain inside the artifact root.
fn resolve_existing_artifact_path(
    artifact_root: &Path,
    path: &str,
) -> Result<PathBuf, DashboardError> {
    let root = normalize_config_root(artifact_root)?;
    let resolved = resolve_under_root(&root, path)?;
    if !resolved.exists() {
        return Err(DashboardError::BadRequest(format!(
            "artifact input path does not exist: {}",
            path_to_string(&resolved)
        )));
    }
    Ok(resolved)
}

/// Resolve one output path under the job work root.
fn resolve_named_output(
    job_root: &Path,
    configured: Option<&str>,
    default_name: &str,
) -> Result<PathBuf, DashboardError> {
    resolve_output_path(job_root, configured.unwrap_or(default_name))
}

/// Resolve one path under a root, creating its parent for file outputs when needed.
fn resolve_output_path(root: &Path, path: &str) -> Result<PathBuf, DashboardError> {
    let root = normalize_config_root(root)?;
    let resolved = resolve_under_root(&root, path)?;
    if resolved == root {
        return Err(DashboardError::BadRequest(
            "output path must name a file or child directory".to_string(),
        ));
    }
    Ok(resolved)
}

/// Resolve one string path under a root with absolute paths still checked against the root.
fn resolve_under_root(root: &Path, path: &str) -> Result<PathBuf, DashboardError> {
    if path.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "research path must not be empty".to_string(),
        ));
    }
    let candidate = if Path::new(path).is_absolute() {
        normalize_path(Path::new(path))?
    } else {
        normalize_path(&root.join(path))?
    };
    if !candidate.starts_with(root) {
        return Err(DashboardError::BadRequest(format!(
            "research path escapes configured root: {}",
            path_to_string(&candidate)
        )));
    }
    Ok(candidate)
}

/// Normalize a path lexically and reject parent traversal.
fn normalize_path(path: &Path) -> Result<PathBuf, DashboardError> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                out.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(DashboardError::BadRequest(format!(
                    "research paths must not contain parent traversal: {}",
                    path_to_string(path)
                )));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(DashboardError::BadRequest(
            "research path must not be empty".to_string(),
        ));
    }
    Ok(out)
}

/// Resolve one CLI time value.
fn resolve_time(
    configured: Option<&str>,
    configured_ms: Option<u64>,
    artifact_ms: Option<u64>,
    name: &str,
) -> Result<String, DashboardError> {
    if let Some(value) = configured {
        if value.trim().is_empty() {
            return Err(DashboardError::BadRequest(format!(
                "{name} must not be empty"
            )));
        }
        return Ok(value.to_string());
    }
    if let Some(ms) = configured_ms.or(artifact_ms) {
        return format_millis_rfc3339(ms);
    }
    Err(DashboardError::BadRequest(format!(
        "{name} is required in research params or artifact interval metadata"
    )))
}

/// Format milliseconds since epoch as RFC3339 UTC.
fn format_millis_rfc3339(ms: u64) -> Result<String, DashboardError> {
    let seconds = i64::try_from(ms / 1_000)
        .map_err(|e| DashboardError::BadRequest(format!("timestamp is too large: {e}")))?;
    let nanos = u32::try_from((ms % 1_000) * 1_000_000)
        .map_err(|e| DashboardError::BadRequest(format!("timestamp millis invalid: {e}")))?;
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanos)
        .ok_or_else(|| DashboardError::BadRequest(format!("timestamp is out of range: {ms}")))?;
    Ok(dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// Validate a list of CLI pass-through strings.
fn validate_string_list(name: &str, values: &[String]) -> Result<(), DashboardError> {
    for value in values {
        if value.trim().is_empty() {
            return Err(DashboardError::BadRequest(format!(
                "{name} values must not be empty"
            )));
        }
    }
    Ok(())
}

/// Push common data interval CLI arguments.
fn push_data_interval_args(
    args: &mut Vec<String>,
    command_name: &str,
    data_path: &Path,
    start: &str,
    end: &str,
) {
    args.push(command_name.to_string());
    args.push("--data".to_string());
    args.push(path_to_string(data_path));
    args.push("--start".to_string());
    args.push(start.to_string());
    args.push("--end".to_string());
    args.push(end.to_string());
}

/// Push shared balance and set override CLI arguments.
fn push_balance_and_sets(args: &mut Vec<String>, balance: f64, sets: &[String]) {
    args.push("--balance".to_string());
    args.push(balance.to_string());
    for set in sets {
        args.push("--set".to_string());
        args.push(set.clone());
    }
}

/// Create the parent directory for one output file.
fn create_parent_dir(path: &Path) -> Result<(), DashboardError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DashboardError::Internal(format!("creating report directory: {e}")))?;
    }
    Ok(())
}

/// Archive one `SQLite` file and its sidecars after path safety checks.
fn archive_db_family(
    job_root: &Path,
    path: &Path,
    summary: &mut ArchiveSummary,
) -> Result<(), DashboardError> {
    let normalized = normalize_path(path)?;
    if !normalized.starts_with(job_root) {
        return Err(DashboardError::BadRequest(format!(
            "archive path escapes job root: {}",
            path_to_string(&normalized)
        )));
    }
    if normalized.extension().and_then(std::ffi::OsStr::to_str) != Some("db") {
        return Err(DashboardError::BadRequest(format!(
            "archive path is not a SQLite DB: {}",
            path_to_string(&normalized)
        )));
    }
    for candidate in sqlite_family_paths(&normalized) {
        if candidate.exists() {
            std::fs::remove_file(&candidate)
                .map_err(|e| DashboardError::Internal(format!("archiving scratch DB: {e}")))?;
            summary.deleted_paths.push(path_to_string(&candidate));
        } else {
            summary.skipped_paths.push(path_to_string(&candidate));
        }
    }
    Ok(())
}

/// Return the `SQLite` main file plus WAL and shared-memory sidecars.
fn sqlite_family_paths(path: &Path) -> Vec<PathBuf> {
    vec![
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path_to_string(path))),
        PathBuf::from(format!("{}-shm", path_to_string(path))),
    ]
}

/// Convert a path to a UTF-8-ish display string.
fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
#[path = "tests/research_pipeline_tests.rs"]
mod tests;
