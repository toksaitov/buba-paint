use rusqlite::Connection;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::db::DashboardDb;
use crate::research_artifacts::{ArtifactFileSpec, build_manifest, write_manifest_files};
use crate::research_backend::ResearchWorkBackend;
use crate::research_pipeline::{
    CommandCancellation, CommandExecutionFuture, CommandOutput, CommandSpec,
    ResearchCommandExecutor, ResearchPipelineConfig,
};

/// Build an in-memory dashboard DB.
fn test_db() -> DashboardDb {
    DashboardDb::from_connection(Connection::open_in_memory().unwrap())
}

/// Fake command executor used by worker tests.
struct FakeExecutor {
    outputs: Mutex<Vec<CommandOutput>>,
    commands: Mutex<Vec<CommandSpec>>,
}

/// Fake executor that marks the active job cancelled while the command runs.
struct CancellingExecutor {
    commands: Mutex<Vec<CommandSpec>>,
}

impl CancellingExecutor {
    /// Build a fake executor that records commands and cancels the active job.
    fn new() -> Self {
        Self {
            commands: Mutex::new(Vec::new()),
        }
    }
}

impl FakeExecutor {
    /// Build a fake executor that returns outputs in call order.
    fn new(outputs: Vec<CommandOutput>) -> Self {
        Self {
            outputs: Mutex::new(outputs.into_iter().rev().collect()),
            commands: Mutex::new(Vec::new()),
        }
    }

    /// Return the commands captured by this executor.
    fn commands(&self) -> Vec<CommandSpec> {
        self.commands.lock().unwrap().clone()
    }
}

impl ResearchCommandExecutor for FakeExecutor {
    /// Capture one command and return the next configured output.
    fn execute(&self, command: &CommandSpec) -> Result<CommandOutput, DashboardError> {
        self.commands.lock().unwrap().push(command.clone());
        self.outputs
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| DashboardError::Internal("fake executor output exhausted".to_string()))
    }
}

impl ResearchCommandExecutor for CancellingExecutor {
    /// Capture one command and return a cancelled command output.
    fn execute(&self, command: &CommandSpec) -> Result<CommandOutput, DashboardError> {
        self.commands.lock().unwrap().push(command.clone());
        Ok(cancelled_output("operator cancelled"))
    }

    /// Capture one supervised command and mark the owning job cancelled.
    fn execute_supervised<'a, B: ResearchWorkBackend>(
        &'a self,
        command: &'a CommandSpec,
        cancellation: CommandCancellation<'a, B>,
    ) -> CommandExecutionFuture<'a> {
        Box::pin(async move {
            self.commands.lock().unwrap().push(command.clone());
            cancellation
                .db
                .cancel_research_job(cancellation.job_id)
                .await?;
            Ok(cancelled_output("operator cancelled"))
        })
    }
}

/// Backend wrapper that cancels the owning job at the report-persist boundary.
struct CancelAtReportPersist {
    inner: DashboardDb,
    publish_calls: AtomicUsize,
}

impl CancelAtReportPersist {
    /// Wrap a local database to simulate a cancel landing after metadata persist.
    fn new(inner: DashboardDb) -> Self {
        Self {
            inner,
            publish_calls: AtomicUsize::new(0),
        }
    }

    /// Return how many times report documents were published.
    fn publish_calls(&self) -> usize {
        self.publish_calls.load(Ordering::SeqCst)
    }
}

impl ResearchWorkBackend for CancelAtReportPersist {
    /// Lease the next step from the wrapped database.
    async fn lease_next_research_step(
        &self,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<Option<crate::db::ResearchStepLease>, DashboardError> {
        self.inner
            .lease_next_research_step(worker_id, lease_duration_ms)
            .await
    }

    /// Refresh a step lease in the wrapped database.
    async fn refresh_research_step_lease(
        &self,
        step_id: &str,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<crate::db::ResearchJobStep, DashboardError> {
        self.inner
            .refresh_research_step_lease(step_id, worker_id, lease_duration_ms)
            .await
    }

    /// Mark a step running in the wrapped database.
    async fn mark_research_step_running(
        &self,
        step_id: &str,
        worker_id: &str,
    ) -> Result<crate::db::ResearchJobStep, DashboardError> {
        self.inner
            .mark_research_step_running(step_id, worker_id)
            .await
    }

    /// Complete a step in the wrapped database.
    async fn complete_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        output_json: Option<&str>,
    ) -> Result<crate::db::ResearchJobStep, DashboardError> {
        self.inner
            .complete_research_step(step_id, worker_id, output_json)
            .await
    }

    /// Fail a step in the wrapped database.
    async fn fail_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        error: &str,
        retryable: bool,
    ) -> Result<crate::db::ResearchJobStep, DashboardError> {
        self.inner
            .fail_research_step(step_id, worker_id, error, retryable)
            .await
    }

    /// Block a step in the wrapped database.
    async fn block_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        reason: &str,
    ) -> Result<crate::db::ResearchJobStep, DashboardError> {
        self.inner
            .block_research_step(step_id, worker_id, reason)
            .await
    }

    /// Append a job event to the wrapped database.
    async fn append_research_job_event(
        &self,
        job_id: &str,
        step_id: Option<&str>,
        level: &str,
        message: &str,
        details_json: Option<&str>,
    ) -> Result<crate::db::ResearchJobEvent, DashboardError> {
        self.inner
            .append_research_job_event(job_id, step_id, level, message, details_json)
            .await
    }

    /// Fetch one job from the wrapped database.
    async fn get_research_job(
        &self,
        id: &str,
    ) -> Result<Option<crate::db::ResearchJob>, DashboardError> {
        self.inner.get_research_job(id).await
    }

    /// Cancel one job in the wrapped database.
    async fn cancel_research_job(
        &self,
        id: &str,
    ) -> Result<crate::db::ResearchJob, DashboardError> {
        self.inner.cancel_research_job(id).await
    }

    /// Fetch job steps from the wrapped database.
    async fn get_research_job_steps(
        &self,
        job_id: &str,
    ) -> Result<Vec<crate::db::ResearchJobStep>, DashboardError> {
        self.inner.get_research_job_steps(job_id).await
    }

    /// Fetch one artifact from the wrapped database.
    async fn get_research_artifact(
        &self,
        id: &str,
    ) -> Result<Option<crate::db::ResearchArtifact>, DashboardError> {
        self.inner.get_research_artifact(id).await
    }

    /// Upsert an artifact in the wrapped database.
    async fn upsert_research_artifact(
        &self,
        record: &crate::db::ResearchArtifactRecord<'_>,
    ) -> Result<crate::db::ResearchArtifact, DashboardError> {
        self.inner.upsert_research_artifact(record).await
    }

    /// Attach a produced artifact to its job in the wrapped database.
    async fn attach_research_job_artifact(
        &self,
        job_id: &str,
        artifact_id: &str,
    ) -> Result<crate::db::ResearchJob, DashboardError> {
        self.inner
            .attach_research_job_artifact(job_id, artifact_id)
            .await
    }

    /// Persist report metadata, then cancel the owning job to simulate a race.
    async fn create_or_update_research_report(
        &self,
        record: &crate::db::ResearchReportRecord<'_>,
    ) -> Result<crate::db::ResearchReport, DashboardError> {
        let report = self.inner.create_or_update_research_report(record).await?;
        self.inner.cancel_research_job(record.job_id).await?;
        Ok(report)
    }

    /// Count and forward report-document publishing to the wrapped database.
    async fn store_research_report_documents(
        &self,
        report_id: &str,
        report_json: &str,
        report_csv: &str,
    ) -> Result<(), DashboardError> {
        self.publish_calls.fetch_add(1, Ordering::SeqCst);
        self.inner
            .store_research_report_documents(report_id, report_json, report_csv)
            .await
    }

    /// Forward artifact-document publishing to the wrapped database.
    async fn store_research_artifact_documents(
        &self,
        artifact_id: &str,
        manifest_json: Option<&str>,
        checksums_text: Option<&str>,
    ) -> Result<(), DashboardError> {
        self.inner
            .store_research_artifact_documents(artifact_id, manifest_json, checksums_text)
            .await
    }

    /// Claim the next queued transfer from the wrapped database.
    async fn claim_next_artifact_transfer(
        &self,
        dest_machine_id: &str,
    ) -> Result<Option<crate::db::ArtifactTransfer>, DashboardError> {
        self.inner
            .claim_next_artifact_transfer(dest_machine_id)
            .await
    }

    /// Fetch one transfer from the wrapped database.
    async fn get_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<Option<crate::db::ArtifactTransfer>, DashboardError> {
        self.inner.get_artifact_transfer(id).await
    }

    /// Update transfer progress in the wrapped database.
    async fn update_artifact_transfer_progress(
        &self,
        id: &str,
        status: &str,
        bytes_done: Option<u64>,
        bytes_total: Option<u64>,
        checksum_status: Option<&str>,
        error: Option<&str>,
    ) -> Result<crate::db::ArtifactTransfer, DashboardError> {
        self.inner
            .update_artifact_transfer_progress(
                id,
                status,
                bytes_done,
                bytes_total,
                checksum_status,
                error,
            )
            .await
    }

    /// Recover stale running transfers in the wrapped database.
    async fn recover_stale_artifact_transfers(
        &self,
        dest_machine_id: &str,
        stale_after_ms: u64,
    ) -> Result<usize, DashboardError> {
        self.inner
            .recover_stale_artifact_transfers(dest_machine_id, stale_after_ms)
            .await
    }

    /// Fetch one machine from the wrapped database.
    async fn get_research_machine(
        &self,
        id: &str,
    ) -> Result<Option<crate::db::ResearchMachine>, DashboardError> {
        self.inner.get_research_machine(id).await
    }
}

/// Return one successful fake command output.
fn success_output(stdout: &str) -> CommandOutput {
    CommandOutput {
        success: true,
        status_code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
        cancelled: false,
    }
}

/// Return one failing fake command output.
fn failure_output(stderr: &str) -> CommandOutput {
    CommandOutput {
        success: false,
        status_code: Some(2),
        stdout: String::new(),
        stderr: stderr.to_string(),
        cancelled: false,
    }
}

/// Write one valid but empty `SQLite` database to the given path.
fn write_empty_sqlite_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch("CREATE TABLE _scratch (id INTEGER);")
        .unwrap();
}

/// Return one cancelled fake command output.
fn cancelled_output(stderr: &str) -> CommandOutput {
    CommandOutput {
        success: false,
        status_code: None,
        stdout: String::new(),
        stderr: stderr.to_string(),
        cancelled: true,
    }
}

/// Build a manifest-backed artifact fixture.
fn artifact_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("paint.db"), b"db-bytes").unwrap();
    let manifest = build_manifest(
        dir.path(),
        "artifact-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1_000,
        Some(1_000),
        Some(2_000),
        &[ArtifactFileSpec {
            logical_name: "runtime_db".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "paint.db".to_string(),
        }],
    )
    .unwrap();
    write_manifest_files(dir.path(), &manifest).unwrap();
    dir
}

/// Build a manifest-backed artifact fixture whose runtime DB is valid `SQLite`.
fn artifact_fixture_with_sqlite_db() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_empty_sqlite_db(&dir.path().join("paint.db"));
    let manifest = build_manifest(
        dir.path(),
        "artifact-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1_000,
        Some(1_000),
        Some(2_000),
        &[ArtifactFileSpec {
            logical_name: "runtime_db".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "paint.db".to_string(),
        }],
    )
    .unwrap();
    write_manifest_files(dir.path(), &manifest).unwrap();
    dir
}

/// Create one valid `SQLite` source DB for export tests.
fn sqlite_source(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("paint.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE ticks (id INTEGER PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO ticks (value) VALUES ('ok')", [])
        .unwrap();
    path
}

/// Return a local pipeline config for worker tests.
fn test_pipeline(work_root: &std::path::Path) -> ResearchPipelineConfig {
    ResearchPipelineConfig::new(std::env::current_dir().unwrap(), work_root).unwrap()
}

/// Verifies that the worker rejects invalid construction inputs.
#[test]
fn local_worker_rejects_invalid_config() {
    assert!(LocalResearchWorker::new("", 1_000).is_err());
    assert!(LocalResearchWorker::new("worker", 0).is_err());
}

/// Verifies that known durable step names parse.
#[test]
fn research_step_kind_parses_allowlisted_names() {
    assert_eq!(
        ResearchStepKind::from_str("prepare_backtest_input").unwrap(),
        ResearchStepKind::PrepareBacktestInput
    );
    assert!(ResearchStepKind::from_str("rm_rf_runtime").is_err());
}

/// Verifies that the local worker can complete an export job end to end.
#[tokio::test]
async fn local_worker_runs_export_job_until_idle() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();

    let completed = worker.run_noop_until_idle(&db, 10).await.unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();
    let events = db.list_research_job_events(&job.id).await.unwrap();

    assert_eq!(completed, 4);
    assert_eq!(job.status, "completed");
    assert!(steps.iter().all(|step| step.status == "completed"));
    assert_eq!(events.len(), 8);
}

/// Verifies that the local worker returns idle when there are no runnable jobs.
#[tokio::test]
async fn local_worker_reports_idle_without_jobs() {
    let db = test_db();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();

    let lease = worker.run_one_noop(&db).await.unwrap();

    assert!(lease.is_none());
}

/// Verifies that the local artifact worker completes export placeholder steps.
#[tokio::test]
async fn local_artifact_worker_runs_export_placeholders_until_idle() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();

    let processed = worker.run_local_until_idle(&db, 10).await.unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();

    assert_eq!(processed, 4);
    assert_eq!(job.status, "completed");
    assert!(steps.iter().all(|step| {
        step.status == "completed"
            && step
                .output_json
                .as_deref()
                .is_some_and(|output| output.contains("local_artifact"))
    }));
}

/// Verifies that a cancelled job is not leased by the local worker.
#[tokio::test]
async fn local_worker_does_not_lease_cancelled_job() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    db.cancel_research_job(&job.id).await.unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();

    let lease = worker.run_one_noop(&db).await.unwrap();

    assert!(lease.is_none());
}

/// Verifies that the local worker verifies artifact manifests before blocking command steps.
#[tokio::test]
async fn local_worker_verifies_artifact_then_blocks_command_step() {
    let db = test_db();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("paint.db"), b"db-bytes").unwrap();
    let manifest = build_manifest(
        root,
        "artifact-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1_000,
        None,
        None,
        &[ArtifactFileSpec {
            logical_name: "runtime_db".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "paint.db".to_string(),
        }],
    )
    .unwrap();
    write_manifest_files(root, &manifest).unwrap();

    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(root.join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z"
    });
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();

    let verified = worker.run_one_local(&db).await.unwrap().unwrap();
    let blocked = worker.run_one_local(&db).await.unwrap().unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();

    assert_eq!(verified.step.name, "verify_artifact");
    assert_eq!(verified.step.status, "completed");
    assert!(verified.step.output_json.unwrap().contains("files_checked"));
    assert_eq!(blocked.step.name, "validate_replay_data");
    assert_eq!(blocked.step.status, "blocked");
    assert_eq!(job.status, "blocked");
}

/// Verifies that bad artifacts block verification and clear leases.
#[tokio::test]
async fn local_worker_blocks_bad_artifact_verification_and_clears_lease() {
    let db = test_db();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("manifest.json"), "{bad-json").unwrap();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z"
    });
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();

    let lease = worker.run_one_local(&db).await.unwrap().unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();

    assert_eq!(lease.step.name, "verify_artifact");
    assert_eq!(lease.step.status, "blocked");
    assert!(lease.step.lease_owner.is_none());
    assert!(lease.step.leased_until_ms.is_none());
    assert_eq!(job.status, "blocked");
}

/// Verifies that the command-backed worker runs a current-params job to report creation.
#[tokio::test]
async fn local_worker_runs_current_params_pipeline_with_fake_commands() {
    let db = test_db();
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z",
        "balance": 150.0,
        "sets": ["PEAK_DD_PAUSE_PCT=1.0"]
    });
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(vec![
        success_output("replay_quality=sweep_grade"),
        success_output("backtest_input=backtest_ready"),
        success_output("prepared_backtest=ready"),
        success_output("backtest complete"),
    ]);

    let processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 10)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let reports = db.list_research_reports().await.unwrap();
    let commands = executor.commands();

    assert_eq!(processed, 6);
    assert_eq!(job.status, "completed");
    assert_eq!(reports.len(), 1);
    assert!(
        commands[0]
            .args
            .contains(&"validate-replay-data".to_string())
    );
    assert!(
        commands[1]
            .args
            .contains(&"validate-backtest-input".to_string())
    );
    assert!(
        commands[2]
            .args
            .contains(&"prepare-backtest-input".to_string())
    );
    assert!(commands[3].args.contains(&"backtest".to_string()));
}

/// Verifies a cancelled command leaves durable state cancelled, not blocked.
#[tokio::test]
async fn local_worker_preserves_cancelled_state_after_command_cancellation() {
    let db = test_db();
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z"
    });
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = CancellingExecutor::new();

    let verified = worker
        .run_one_local_with_pipeline(&db, &pipeline, &executor)
        .await
        .unwrap()
        .unwrap();
    let cancelled = worker
        .run_one_local_with_pipeline(&db, &pipeline, &executor)
        .await
        .unwrap()
        .unwrap();
    let idle = worker
        .run_one_local_with_pipeline(&db, &pipeline, &executor)
        .await
        .unwrap();
    let stored_job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let events = db.list_research_job_events(&job.id).await.unwrap();

    assert_eq!(verified.step.status, "completed");
    assert_eq!(cancelled.step.status, "cancelled");
    assert_eq!(stored_job.status, "cancelled");
    assert!(cancelled.step.error.is_none());
    assert!(idle.is_none());
    assert!(
        events
            .iter()
            .any(|event| { event.message == "research command terminated after cancellation" })
    );
    assert!(
        events
            .iter()
            .any(|event| { event.message == "worker observed cancellation" })
    );
}

/// Verifies that a failed validation blocks later backtest work.
#[tokio::test]
async fn local_worker_blocks_pipeline_after_failed_validation() {
    let db = test_db();
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z"
    });
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(vec![failure_output("not sweep-grade")]);

    let processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 10)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();
    let commands = executor.commands();

    assert_eq!(processed, 2);
    assert_eq!(job.status, "blocked");
    assert_eq!(commands.len(), 1);
    assert_eq!(steps[1].name, "validate_replay_data");
    assert_eq!(steps[1].status, "blocked");
    assert_eq!(steps[2].status, "queued");
}

/// Verifies that export jobs dry-run by default and do not create artifacts.
#[tokio::test]
async fn local_worker_export_job_dry_run_completes_without_artifact() {
    let db = test_db();
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let params = serde_json::json!({
        "source_db_path": source_db,
        "interval_start_ms": 1000,
        "interval_end_ms": 2000
    });
    let job = db
        .create_research_job("export", None, &user.id, 0, Some(&params.to_string()))
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(Vec::new());

    let processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 10)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();
    let artifacts = db.list_research_artifacts().await.unwrap();

    assert_eq!(processed, 4);
    assert_eq!(job.status, "completed");
    assert!(job.artifact_id.is_none());
    assert!(artifacts.is_empty());
    assert!(steps.iter().all(|step| step.status == "completed"));
    assert!(steps.iter().any(|step| {
        step.output_json
            .as_deref()
            .is_some_and(|json| json.contains("dry_run"))
    }));
}

/// Verifies that a confirmed export writes and verifies a local artifact.
#[tokio::test]
async fn local_worker_export_job_writes_and_verifies_artifact() {
    let db = test_db();
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let log_path = source_dir.path().join("runtime.log");
    std::fs::write(&log_path, b"log").unwrap();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let params = serde_json::json!({
        "source_db_path": source_db,
        "log_paths": [log_path],
        "interval_start_ms": 1000,
        "interval_end_ms": 2000,
        "dry_run": false,
        "confirm_export": true
    });
    let job = db
        .create_research_job("export", None, &user.id, 0, Some(&params.to_string()))
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(Vec::new());

    let processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 10)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();
    let artifacts = db.list_research_artifacts().await.unwrap();

    assert_eq!(processed, 4);
    assert_eq!(job.status, "completed");
    assert!(job.artifact_id.is_some());
    assert_eq!(artifacts.len(), 1);
    assert!(
        artifacts[0]
            .manifest_path
            .as_deref()
            .is_some_and(|path| path.ends_with("manifest.json"))
    );
    assert_eq!(steps[3].name, "verify_artifact");
    assert_eq!(steps[3].status, "completed");
    assert!(
        steps[3]
            .output_json
            .as_deref()
            .unwrap()
            .contains("files_checked")
    );
}

/// Verifies that the command worker honors the max step limit.
#[tokio::test]
async fn local_worker_pipeline_max_steps_limits_processing() {
    let db = test_db();
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let params = serde_json::json!({
        "source_db_path": source_db,
        "interval_start_ms": 1000,
        "interval_end_ms": 2000
    });
    let job = db
        .create_research_job("export", None, &user.id, 0, Some(&params.to_string()))
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(Vec::new());

    let processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 2)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();

    assert_eq!(processed, 2);
    assert_eq!(job.status, "running");
    assert_eq!(steps[0].status, "completed");
    assert_eq!(steps[1].status, "completed");
    assert_eq!(steps[2].status, "queued");
}

/// Verifies that unconfirmed real exports block without creating artifacts.
#[tokio::test]
async fn local_worker_blocks_unconfirmed_real_export_without_artifact() {
    let db = test_db();
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let params = serde_json::json!({
        "source_db_path": source_db,
        "dry_run": false
    });
    let job = db
        .create_research_job("export", None, &user.id, 0, Some(&params.to_string()))
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(Vec::new());

    let processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 10)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();
    let artifacts = db.list_research_artifacts().await.unwrap();

    assert_eq!(processed, 1);
    assert_eq!(job.status, "blocked");
    assert!(job.artifact_id.is_none());
    assert!(artifacts.is_empty());
    assert_eq!(steps[0].status, "blocked");
    assert!(steps[0].lease_owner.is_none());
    assert!(steps[0].leased_until_ms.is_none());
}

/// Verifies retry resumes blocked commands without rerunning completed steps.
#[tokio::test]
async fn local_worker_retry_resumes_blocked_command_without_rerunning_completed_steps() {
    let db = test_db();
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z"
    });
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let failing_executor = FakeExecutor::new(vec![failure_output("temporary replay failure")]);

    let first_processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &failing_executor, 10)
        .await
        .unwrap();
    let blocked_steps = db.get_research_job_steps(&job.id).await.unwrap();
    assert_eq!(first_processed, 2);
    assert_eq!(blocked_steps[0].status, "completed");
    assert_eq!(blocked_steps[0].attempts, 1);
    assert_eq!(blocked_steps[1].status, "blocked");
    assert_eq!(blocked_steps[1].attempts, 1);
    assert!(blocked_steps[1].lease_owner.is_none());
    assert!(blocked_steps[1].leased_until_ms.is_none());

    db.retry_research_job(&job.id).await.unwrap();
    let recovery_executor = FakeExecutor::new(vec![
        success_output("replay_quality=sweep_grade"),
        success_output("backtest_input=backtest_ready"),
        success_output("prepared_backtest=ready"),
        success_output("backtest complete"),
    ]);
    let second_processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &recovery_executor, 10)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let recovered_steps = db.get_research_job_steps(&job.id).await.unwrap();

    assert_eq!(second_processed, 5);
    assert_eq!(job.status, "completed");
    assert_eq!(recovered_steps[0].attempts, 1);
    assert_eq!(recovered_steps[1].attempts, 2);
    assert!(
        recovered_steps
            .iter()
            .all(|step| step.status == "completed")
    );
}

/// Verifies report writing includes archive summaries when requested.
#[tokio::test]
async fn local_worker_write_report_includes_archive_summary_when_requested() {
    let db = test_db();
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z",
        "archive_scratch": true
    });
    let _job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(vec![
        success_output("replay_quality=sweep_grade"),
        success_output("backtest_input=backtest_ready"),
        success_output("prepared_backtest=ready"),
        success_output("backtest complete"),
    ]);

    worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 10)
        .await
        .unwrap();
    let report = db.list_research_reports().await.unwrap().remove(0);
    let summary: serde_json::Value = serde_json::from_str(&report.summary_json.unwrap()).unwrap();

    assert_eq!(
        summary["archive"]["deleted_paths"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert!(
        summary["archive"]["skipped_paths"]
            .as_array()
            .unwrap()
            .len()
            >= 6
    );
}

/// Verifies archive failures preserve report files and report metadata.
#[tokio::test]
async fn local_worker_preserves_report_when_archive_fails() {
    let db = test_db();
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z",
        "prepared_db_output_path": "prepared.txt",
        "archive_scratch": true
    });
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(vec![
        success_output("replay_quality=sweep_grade"),
        success_output("backtest_input=backtest_ready"),
        success_output("prepared_backtest=ready"),
        success_output("backtest complete"),
    ]);

    let processed = worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 10)
        .await
        .unwrap();
    let stored_job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();
    let report = db.list_research_reports().await.unwrap().remove(0);
    let summary: serde_json::Value = serde_json::from_str(&report.summary_json.unwrap()).unwrap();
    let report_path = std::path::Path::new(report.report_path.as_deref().unwrap());
    let csv_path = std::path::Path::new(report.csv_path.as_deref().unwrap());

    assert_eq!(processed, 6);
    assert_eq!(stored_job.status, "blocked");
    assert_eq!(
        steps
            .iter()
            .find(|step| step.name == "write_report")
            .unwrap()
            .status,
        "blocked"
    );
    assert!(report_path.exists());
    assert!(csv_path.exists());
    assert_eq!(summary["archive_error"]["status"], "failed");
    assert!(
        summary["archive_error"]["error"]
            .as_str()
            .unwrap()
            .contains("not a SQLite DB")
    );
}

/// Verifies a cancel after metadata persist skips publish and scratch deletion.
#[tokio::test]
async fn local_worker_skips_publish_and_archive_when_cancelled_after_persist() {
    let inner = test_db();
    let artifact_dir = artifact_fixture_with_sqlite_db();
    let work_dir = tempfile::tempdir().unwrap();
    let user = inner
        .create_user("researcher", "hash", "admin")
        .await
        .unwrap();
    let artifact = inner
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z",
        "archive_scratch": true
    });
    let job = inner
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();

    let job_root = work_dir.path().join("jobs").join(&job.id);
    std::fs::create_dir_all(&job_root).unwrap();
    let prepared_db = job_root.join("prepared-backtest.db");
    let backtest_db = job_root.join("backtest.db");
    write_empty_sqlite_db(&prepared_db);
    write_empty_sqlite_db(&backtest_db);

    let db = CancelAtReportPersist::new(inner);
    let worker = LocalResearchWorker::new("local-worker", 1_000).unwrap();
    let pipeline = test_pipeline(work_dir.path());
    let executor = FakeExecutor::new(vec![
        success_output("replay_quality=sweep_grade"),
        success_output("backtest_input=backtest_ready"),
        success_output("prepared_backtest=ready"),
        success_output("backtest complete"),
    ]);

    worker
        .run_local_with_pipeline_until_idle(&db, &pipeline, &executor, 10)
        .await
        .unwrap();

    let stored_job = db.get_research_job(&job.id).await.unwrap().unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();
    let write_report = steps
        .iter()
        .find(|step| step.name == "write_report")
        .unwrap();

    assert_eq!(stored_job.status, "cancelled");
    assert_eq!(write_report.status, "cancelled");
    assert_eq!(db.publish_calls(), 0);
    assert!(prepared_db.exists());
    assert!(backtest_db.exists());
}
