//! Research worker implementation.
//!
//! The worker leases durable job steps through a `ResearchWorkBackend`, which is
//! either the local `SQLite` `DashboardDb` or a remote `ResearchControllerClient`
//! over HTTP, executes the allowlisted action for each step, and records progress
//! back into job steps and events. The backend is selected at startup from the
//! worker configuration: a controller URL plus a worker token routes all work
//! through the controller, otherwise the worker uses the local database.

use std::str::FromStr;
use std::time::Duration;

use crate::db::{ResearchArtifactRecord, ResearchReportRecord, ResearchStepLease};
use crate::error::DashboardError;
use crate::research_artifacts;
use crate::research_backend::ResearchWorkBackend;
use crate::research_export;
use crate::research_pipeline::{
    BubaPaintCommandKind, CommandCancellation, CommandOutput, CommandSpec, ResearchCommandExecutor,
    ResearchPipelineConfig, ResearchPipelinePlan, archive_scratch_dbs, write_report_files,
};
use crate::research_reports::append_report_json_field;
use crate::research_util::research_artifact_for_job_id;

/// Durable research step names understood by the local worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchStepKind {
    /// Validate an export request and report safety status.
    PlanExport,
    /// Copy or snapshot runtime inputs into an artifact directory.
    SnapshotOrCopyRuntime,
    /// Write the artifact manifest and attach it to the job.
    WriteArtifactManifest,
    /// Verify an artifact manifest and file checksums.
    VerifyArtifact,
    /// Run replay-data validation.
    ValidateReplayData,
    /// Run backtest-input validation.
    ValidateBacktestInput,
    /// Prepare the backtest input database.
    PrepareBacktestInput,
    /// Run a current-parameter backtest.
    RunBacktest,
    /// Run a parameter sweep.
    RunSweep,
    /// Write final report files and optional archive metadata.
    WriteReport,
}

/// Local worker that leases and executes research steps for one worker ID.
pub struct LocalResearchWorker {
    worker_id: String,
    lease_duration_ms: u64,
}

impl LocalResearchWorker {
    /// Create a local worker that executes allowlisted no-op research steps.
    pub fn new(
        worker_id: impl Into<String>,
        lease_duration_ms: u64,
    ) -> Result<Self, DashboardError> {
        let worker_id = worker_id.into();
        if worker_id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "worker_id must not be empty".to_string(),
            ));
        }
        if lease_duration_ms == 0 {
            return Err(DashboardError::BadRequest(
                "lease_duration_ms must be positive".to_string(),
            ));
        }
        Ok(Self {
            worker_id,
            lease_duration_ms,
        })
    }

    /// Lease and run one available step with the phase-3 local no-op executor.
    pub async fn run_one_noop(
        &self,
        db: &impl ResearchWorkBackend,
    ) -> Result<Option<ResearchStepLease>, DashboardError> {
        let Some(lease) = db
            .lease_next_research_step(&self.worker_id, self.lease_duration_ms)
            .await?
        else {
            return Ok(None);
        };

        let step_id = lease.step.id.clone();
        let job_id = lease.job.id.clone();
        let step_name = lease.step.name.clone();

        let step_kind = match ResearchStepKind::from_str(&step_name) {
            Ok(kind) => kind,
            Err(error) => {
                db.append_research_job_event(
                    &job_id,
                    Some(&step_id),
                    "error",
                    &error,
                    Some(r#"{"executor":"local_noop"}"#),
                )
                .await?;
                let step = db
                    .fail_research_step(&step_id, &self.worker_id, &error, false)
                    .await?;
                return Ok(Some(ResearchStepLease {
                    job: lease.job,
                    step,
                }));
            }
        };

        db.mark_research_step_running(&step_id, &self.worker_id)
            .await?;
        db.append_research_job_event(
            &job_id,
            Some(&step_id),
            "info",
            "worker started step",
            Some(&format!(
                r#"{{"executor":"local_noop","step_kind":"{}"}}"#,
                step_kind.as_str()
            )),
        )
        .await?;

        let output = format!(
            r#"{{"executor":"local_noop","step_kind":"{}","status":"completed"}}"#,
            step_kind.as_str()
        );
        let step = db
            .complete_research_step(&step_id, &self.worker_id, Some(&output))
            .await?;
        db.append_research_job_event(
            &job_id,
            Some(&step_id),
            "info",
            "worker completed step",
            Some(&output),
        )
        .await?;

        Ok(Some(ResearchStepLease {
            job: lease.job,
            step,
        }))
    }

    /// Run available no-op steps up to a bounded limit.
    pub async fn run_noop_until_idle(
        &self,
        db: &impl ResearchWorkBackend,
        max_steps: usize,
    ) -> Result<usize, DashboardError> {
        let mut completed = 0;
        for _ in 0..max_steps {
            if self.run_one_noop(db).await?.is_none() {
                break;
            }
            completed += 1;
        }
        Ok(completed)
    }

    /// Lease and run one available step with local artifact-aware behavior.
    pub async fn run_one_local(
        &self,
        db: &impl ResearchWorkBackend,
    ) -> Result<Option<ResearchStepLease>, DashboardError> {
        let Some(lease) = db
            .lease_next_research_step(&self.worker_id, self.lease_duration_ms)
            .await?
        else {
            return Ok(None);
        };

        let step_id = lease.step.id.clone();
        let job_id = lease.job.id.clone();
        let step_kind = match ResearchStepKind::from_str(&lease.step.name) {
            Ok(kind) => kind,
            Err(error) => {
                let step = db
                    .fail_research_step(&step_id, &self.worker_id, &error, false)
                    .await?;
                return Ok(Some(ResearchStepLease {
                    job: lease.job,
                    step,
                }));
            }
        };

        db.mark_research_step_running(&step_id, &self.worker_id)
            .await?;

        match step_kind {
            ResearchStepKind::VerifyArtifact => {
                let result = self.verify_artifact_step(db, &lease).await;
                complete_or_block_step(db, &self.worker_id, &lease, result).await
            }
            ResearchStepKind::ValidateReplayData
            | ResearchStepKind::ValidateBacktestInput
            | ResearchStepKind::PrepareBacktestInput
            | ResearchStepKind::RunBacktest
            | ResearchStepKind::RunSweep => {
                let reason = format!(
                    "{} requires the buba-paint command executor, which is deferred until the artifact pipeline is wired to real local DB fixtures",
                    step_kind.as_str()
                );
                db.append_research_job_event(
                    &job_id,
                    Some(&step_id),
                    "warn",
                    &reason,
                    Some(r#"{"executor":"local_artifact","status":"blocked"}"#),
                )
                .await?;
                let step = db
                    .block_research_step(&step_id, &self.worker_id, &reason)
                    .await?;
                Ok(Some(ResearchStepLease {
                    job: lease.job,
                    step,
                }))
            }
            ResearchStepKind::PlanExport
            | ResearchStepKind::SnapshotOrCopyRuntime
            | ResearchStepKind::WriteArtifactManifest
            | ResearchStepKind::WriteReport => {
                let output = format!(
                    r#"{{"executor":"local_artifact","step_kind":"{}","status":"completed"}}"#,
                    step_kind.as_str()
                );
                db.append_research_job_event(
                    &job_id,
                    Some(&step_id),
                    "info",
                    "worker completed placeholder step",
                    Some(&output),
                )
                .await?;
                let step = db
                    .complete_research_step(&step_id, &self.worker_id, Some(&output))
                    .await?;
                Ok(Some(ResearchStepLease {
                    job: lease.job,
                    step,
                }))
            }
        }
    }

    /// Run artifact-aware local steps up to a bounded limit.
    pub async fn run_local_until_idle(
        &self,
        db: &impl ResearchWorkBackend,
        max_steps: usize,
    ) -> Result<usize, DashboardError> {
        let mut processed = 0;
        for _ in 0..max_steps {
            if self.run_one_local(db).await?.is_none() {
                break;
            }
            processed += 1;
        }
        Ok(processed)
    }

    /// Lease and run one available step with command-backed local behavior.
    pub async fn run_one_local_with_pipeline<E: ResearchCommandExecutor>(
        &self,
        db: &impl ResearchWorkBackend,
        pipeline: &ResearchPipelineConfig,
        executor: &E,
    ) -> Result<Option<ResearchStepLease>, DashboardError> {
        let Some(lease) = db
            .lease_next_research_step(&self.worker_id, self.lease_duration_ms)
            .await?
        else {
            return Ok(None);
        };

        let step_id = lease.step.id.clone();
        let step_kind = match ResearchStepKind::from_str(&lease.step.name) {
            Ok(kind) => kind,
            Err(error) => {
                let step = db
                    .fail_research_step(&step_id, &self.worker_id, &error, false)
                    .await?;
                return Ok(Some(ResearchStepLease {
                    job: lease.job,
                    step,
                }));
            }
        };

        db.mark_research_step_running(&step_id, &self.worker_id)
            .await?;

        match step_kind {
            ResearchStepKind::PlanExport => {
                let result = Self::plan_export_step(pipeline, &lease);
                complete_or_block_pipeline_step(db, &self.worker_id, &lease, result).await
            }
            ResearchStepKind::SnapshotOrCopyRuntime => {
                let result = Self::snapshot_or_copy_runtime_step(pipeline, &lease);
                complete_or_block_pipeline_step(db, &self.worker_id, &lease, result).await
            }
            ResearchStepKind::WriteArtifactManifest => {
                let result = self
                    .write_artifact_manifest_step(db, pipeline, &lease)
                    .await;
                complete_or_block_pipeline_step(db, &self.worker_id, &lease, result).await
            }
            ResearchStepKind::VerifyArtifact => {
                let result = self.verify_artifact_step(db, &lease).await;
                complete_or_block_step(db, &self.worker_id, &lease, result).await
            }
            ResearchStepKind::ValidateReplayData
            | ResearchStepKind::ValidateBacktestInput
            | ResearchStepKind::PrepareBacktestInput
            | ResearchStepKind::RunBacktest
            | ResearchStepKind::RunSweep => {
                let result = self
                    .run_command_step(db, pipeline, executor, &lease, step_kind)
                    .await;
                complete_or_block_pipeline_step(db, &self.worker_id, &lease, result).await
            }
            ResearchStepKind::WriteReport => {
                let result = self.write_report_step(db, pipeline, &lease).await;
                complete_or_block_pipeline_step(db, &self.worker_id, &lease, result).await
            }
        }
    }

    /// Run command-backed local steps up to a bounded limit.
    pub async fn run_local_with_pipeline_until_idle<E: ResearchCommandExecutor>(
        &self,
        db: &impl ResearchWorkBackend,
        pipeline: &ResearchPipelineConfig,
        executor: &E,
        max_steps: usize,
    ) -> Result<usize, DashboardError> {
        let mut processed = 0;
        for _ in 0..max_steps {
            if self
                .run_one_local_with_pipeline(db, pipeline, executor)
                .await?
                .is_none()
            {
                break;
            }
            processed += 1;
        }
        Ok(processed)
    }

    /// Verify the artifact referenced by the leased job.
    async fn verify_artifact_step(
        &self,
        db: &impl ResearchWorkBackend,
        lease: &ResearchStepLease,
    ) -> Result<String, DashboardError> {
        let Some(artifact_id) = lease.job.artifact_id.as_deref() else {
            return Ok(
                r#"{"executor":"local_artifact","artifact":"none","status":"skipped"}"#.to_string(),
            );
        };
        let artifact = db
            .get_research_artifact(artifact_id)
            .await?
            .ok_or_else(|| {
                DashboardError::NotFound(format!("artifact '{artifact_id}' not found"))
            })?;
        let root = artifact
            .artifact_root
            .as_deref()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                artifact.manifest_path.as_deref().and_then(|path| {
                    std::path::Path::new(path)
                        .parent()
                        .map(std::path::Path::to_path_buf)
                })
            })
            .ok_or_else(|| {
                DashboardError::BadRequest(format!(
                    "artifact '{artifact_id}' has no artifact_root or manifest_path"
                ))
            })?;
        let verification = research_artifacts::verify_artifact(&root)?;
        serde_json::to_string(&verification)
            .map_err(|e| DashboardError::Internal(format!("serializing verification: {e}")))
    }

    /// Execute one local `buba-paint` command-backed pipeline step.
    async fn run_command_step<E: ResearchCommandExecutor>(
        &self,
        db: &impl ResearchWorkBackend,
        pipeline: &ResearchPipelineConfig,
        executor: &E,
        lease: &ResearchStepLease,
        step_kind: ResearchStepKind,
    ) -> Result<String, DashboardError> {
        let artifact = research_artifact_for_job_id(db, lease.job.artifact_id.as_deref()).await?;
        let plan = pipeline.plan_for_job(&lease.job, artifact.as_ref())?;
        let command_kind = step_kind.command_kind().ok_or_else(|| {
            DashboardError::BadRequest(format!(
                "{} is not a command-backed step",
                step_kind.as_str()
            ))
        })?;
        let command = pipeline.command_for_step(command_kind, &plan);
        let command_json = serde_json::to_string(&command)
            .map_err(|e| DashboardError::Internal(format!("serializing command: {e}")))?;
        db.append_research_job_event(
            &lease.job.id,
            Some(&lease.step.id),
            "info",
            "research command started",
            Some(&command_json),
        )
        .await?;

        let command_output = executor
            .execute_supervised(
                &command,
                CommandCancellation {
                    db,
                    job_id: &lease.job.id,
                    step_id: &lease.step.id,
                    worker_id: &self.worker_id,
                    poll_interval: Duration::from_secs(3),
                    lease_duration_ms: self.lease_duration_ms,
                },
            )
            .await?;
        let output_json = command_step_output(step_kind, &command, &command_output)?;
        if command_output.cancelled {
            if let Some(job) = db.get_research_job(&lease.job.id).await?
                && job.status != "cancelled"
            {
                match db.cancel_research_job(&lease.job.id).await {
                    Ok(_) | Err(DashboardError::NotFound(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            db.append_research_job_event(
                &lease.job.id,
                Some(&lease.step.id),
                "info",
                "research command terminated after cancellation",
                Some(&output_json),
            )
            .await?;
            return Err(DashboardError::BadRequest(format!(
                "{} command cancelled",
                step_kind.as_str()
            )));
        }
        if !command_output.success {
            db.append_research_job_event(
                &lease.job.id,
                Some(&lease.step.id),
                "warn",
                "research command failed",
                Some(&output_json),
            )
            .await?;
            return Err(DashboardError::BadRequest(format!(
                "{} command failed with status {:?}",
                step_kind.as_str(),
                command_output.status_code
            )));
        }
        Ok(output_json)
    }

    /// Write report files, persist report metadata, and optionally archive scratch DB outputs.
    ///
    /// Cancellation is re-checked at entry and again immediately before the
    /// destructive scratch archival and before publishing report documents, so
    /// an operator cancel during report generation neither deletes scratch
    /// inputs nor publishes a report.
    async fn write_report_step(
        &self,
        db: &impl ResearchWorkBackend,
        pipeline: &ResearchPipelineConfig,
        lease: &ResearchStepLease,
    ) -> Result<String, DashboardError> {
        if report_step_is_cancelled(db, lease).await? {
            return Err(DashboardError::BadRequest(
                "write_report cancelled before report generation".to_string(),
            ));
        }
        let artifact = research_artifact_for_job_id(db, lease.job.artifact_id.as_deref()).await?;
        let plan = pipeline.plan_for_job(&lease.job, artifact.as_ref())?;
        let mut steps = db.get_research_job_steps(&lease.job.id).await?;
        for step in &mut steps {
            if step.id == lease.step.id {
                step.status = "completed".to_string();
                step.error = None;
            }
        }
        let mut summary_json = write_report_files(&plan, &steps)?;
        let mut report_id =
            persist_research_report_metadata(db, lease, &plan, &summary_json).await?;
        let archive_summary = if plan.archive_scratch {
            if report_step_is_cancelled(db, lease).await? {
                return Err(DashboardError::BadRequest(
                    "write_report cancelled before scratch archival".to_string(),
                ));
            }
            match archive_scratch_dbs(&plan) {
                Ok(summary) => {
                    summary_json = report_summary_with_field(&summary_json, "archive", &summary)?;
                    append_report_json_field(&plan.report_json_path, "archive", &summary)?;
                    report_id =
                        persist_research_report_metadata(db, lease, &plan, &summary_json).await?;
                    Some(summary)
                }
                Err(error) => {
                    let reason = error.to_string();
                    let archive_error = serde_json::json!({
                        "status": "failed",
                        "error": reason,
                    });
                    summary_json =
                        report_summary_with_field(&summary_json, "archive_error", &archive_error)?;
                    append_report_json_field(
                        &plan.report_json_path,
                        "archive_error",
                        &archive_error,
                    )?;
                    persist_research_report_metadata(db, lease, &plan, &summary_json).await?;
                    return Err(error);
                }
            }
        } else {
            None
        };
        if report_step_is_cancelled(db, lease).await? {
            return Err(DashboardError::BadRequest(
                "write_report cancelled before publishing report documents".to_string(),
            ));
        }
        publish_report_documents(db, &report_id, &plan).await?;
        serde_json::to_string(&serde_json::json!({
            "executor": "local_command",
            "step_kind": ResearchStepKind::WriteReport.as_str(),
            "status": "completed",
            "report_id": report_id,
            "report_path": plan.report_json_path.to_string_lossy(),
            "csv_path": plan.report_csv_path.to_string_lossy(),
            "archive": archive_summary,
        }))
        .map_err(|e| DashboardError::Internal(format!("serializing report output: {e}")))
    }

    /// Build and validate a local export plan.
    fn plan_export_step(
        pipeline: &ResearchPipelineConfig,
        lease: &ResearchStepLease,
    ) -> Result<String, DashboardError> {
        let plan = research_export::plan_export(
            pipeline,
            &lease.job.id,
            lease.job.params_json.as_deref(),
        )?;
        if plan.safety_status == "blocked" {
            return Err(DashboardError::BadRequest(format!(
                "export plan is blocked: {}",
                plan.safety_reasons.join("; ")
            )));
        }
        export_step_output(ResearchStepKind::PlanExport, "planned", &plan)
    }

    /// Snapshot or copy runtime files for a confirmed export.
    fn snapshot_or_copy_runtime_step(
        pipeline: &ResearchPipelineConfig,
        lease: &ResearchStepLease,
    ) -> Result<String, DashboardError> {
        let plan = research_export::plan_export(
            pipeline,
            &lease.job.id,
            lease.job.params_json.as_deref(),
        )?;
        if research_export::is_dry_run(&plan) {
            return export_step_output(
                ResearchStepKind::SnapshotOrCopyRuntime,
                "dry_run_skipped",
                &plan,
            );
        }
        let result = research_export::export_runtime_files(&plan)?;
        export_step_output(
            ResearchStepKind::SnapshotOrCopyRuntime,
            "completed",
            &result,
        )
    }

    /// Write an exported artifact manifest and attach it to the job.
    async fn write_artifact_manifest_step(
        &self,
        db: &impl ResearchWorkBackend,
        pipeline: &ResearchPipelineConfig,
        lease: &ResearchStepLease,
    ) -> Result<String, DashboardError> {
        let plan = research_export::plan_export(
            pipeline,
            &lease.job.id,
            lease.job.params_json.as_deref(),
        )?;
        if research_export::is_dry_run(&plan) {
            return export_step_output(
                ResearchStepKind::WriteArtifactManifest,
                "dry_run_skipped",
                &plan,
            );
        }
        let result = research_export::write_export_manifest(&plan)?;
        let artifact_root = result.artifact_root.to_string_lossy().to_string();
        let manifest_path = result.manifest_path.to_string_lossy().to_string();
        let source_db_path = plan.source_db_path.to_string_lossy().to_string();
        db.upsert_research_artifact(&ResearchArtifactRecord {
            id: &result.artifact_id,
            source_machine_id: Some("live"),
            kind: &plan.artifact_kind,
            status: "available",
            run_mode: Some(&plan.run_mode),
            artifact_root: Some(&artifact_root),
            manifest_path: Some(&manifest_path),
            bundle_path: None,
            source_db_path: Some(&source_db_path),
            interval_start_ms: plan.interval_start_ms,
            interval_end_ms: plan.interval_end_ms,
            bytes: Some(result.bytes),
            checksum: Some(&result.checksum),
            replay_quality_class: None,
            backtest_ready_class: None,
            live_fidelity_class: None,
        })
        .await?;
        db.attach_research_job_artifact(&lease.job.id, &result.artifact_id)
            .await?;
        let manifest_json = std::fs::read_to_string(&result.manifest_path).ok();
        let checksums_text =
            std::fs::read_to_string(result.artifact_root.join("checksums.sha256")).ok();
        db.store_research_artifact_documents(
            &result.artifact_id,
            manifest_json.as_deref(),
            checksums_text.as_deref(),
        )
        .await?;
        export_step_output(
            ResearchStepKind::WriteArtifactManifest,
            "completed",
            &result,
        )
    }
}

/// Upload generated report documents to the work source after metadata persists.
async fn publish_report_documents(
    db: &impl ResearchWorkBackend,
    report_id: &str,
    plan: &ResearchPipelinePlan,
) -> Result<(), DashboardError> {
    let report_json = std::fs::read_to_string(&plan.report_json_path).map_err(|error| {
        DashboardError::Internal(format!("reading generated report JSON: {error}"))
    })?;
    let report_csv = std::fs::read_to_string(&plan.report_csv_path).map_err(|error| {
        DashboardError::Internal(format!("reading generated report CSV: {error}"))
    })?;
    db.store_research_report_documents(report_id, &report_json, &report_csv)
        .await
}

/// Persist the durable report metadata row for a generated research report.
async fn persist_research_report_metadata(
    db: &impl ResearchWorkBackend,
    lease: &ResearchStepLease,
    plan: &ResearchPipelinePlan,
    summary_json: &str,
) -> Result<String, DashboardError> {
    let title = research_report_title(&lease.job);
    let report_path = plan.report_json_path.to_string_lossy().to_string();
    let csv_path = plan.report_csv_path.to_string_lossy().to_string();
    let report = db
        .create_or_update_research_report(&ResearchReportRecord {
            job_id: &lease.job.id,
            artifact_id: lease.job.artifact_id.as_deref(),
            title: &title,
            status: "available",
            summary_json: Some(summary_json),
            report_path: Some(&report_path),
            csv_path: Some(&csv_path),
        })
        .await?;
    Ok(report.id)
}

/// Build an operator-facing report title from job type and interval.
fn research_report_title(job: &crate::db::ResearchJob) -> String {
    let type_label = match job.job_type.as_str() {
        "current_params" => "Backtest",
        "sweep" => "Sweep",
        "export" => "Export",
        other => other,
    };
    let params: serde_json::Value = job
        .params_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(serde_json::Value::Null);
    let start_ms = params.get("start_ms").and_then(serde_json::Value::as_i64);
    let end_ms = params.get("end_ms").and_then(serde_json::Value::as_i64);
    if let (Some(start_ms), Some(end_ms)) = (start_ms, end_ms) {
        let start = chrono::DateTime::from_timestamp_millis(start_ms);
        let end = chrono::DateTime::from_timestamp_millis(end_ms);
        if let (Some(start), Some(end)) = (start, end) {
            return format!(
                "{type_label} {} to {} UTC",
                start.format("%Y-%m-%d %H:%M"),
                end.format("%Y-%m-%d %H:%M")
            );
        }
    }
    let short_id: String = job.id.chars().take(8).collect();
    format!("{type_label} {short_id}")
}

/// Return a report JSON document with one top-level field inserted.
fn report_summary_with_field<T: serde::Serialize>(
    summary_json: &str,
    field_name: &str,
    field_value: &T,
) -> Result<String, DashboardError> {
    let mut value: serde_json::Value = serde_json::from_str(summary_json)
        .map_err(|e| DashboardError::Internal(format!("parsing generated research report: {e}")))?;
    value[field_name] = serde_json::to_value(field_value)
        .map_err(|e| DashboardError::Internal(format!("serializing report field: {e}")))?;
    serde_json::to_string_pretty(&value)
        .map_err(|e| DashboardError::Internal(format!("serializing research report: {e}")))
}

/// Complete a step when a local action succeeds, otherwise block it.
async fn complete_or_block_step(
    db: &impl ResearchWorkBackend,
    worker_id: &str,
    lease: &ResearchStepLease,
    result: Result<String, DashboardError>,
) -> Result<Option<ResearchStepLease>, DashboardError> {
    match result {
        Ok(output) => {
            db.append_research_job_event(
                &lease.job.id,
                Some(&lease.step.id),
                "info",
                "worker completed step",
                Some(&output),
            )
            .await?;
            let step = db
                .complete_research_step(&lease.step.id, worker_id, Some(&output))
                .await?;
            Ok(Some(ResearchStepLease {
                job: lease.job.clone(),
                step,
            }))
        }
        Err(error) => {
            let reason = error.to_string();
            db.append_research_job_event(
                &lease.job.id,
                Some(&lease.step.id),
                "warn",
                &reason,
                Some(r#"{"executor":"local_artifact","status":"blocked"}"#),
            )
            .await?;
            let step = db
                .block_research_step(&lease.step.id, worker_id, &reason)
                .await?;
            Ok(Some(ResearchStepLease {
                job: lease.job.clone(),
                step,
            }))
        }
    }
}

/// Complete a command-backed step when it succeeds, otherwise block it.
async fn complete_or_block_pipeline_step(
    db: &impl ResearchWorkBackend,
    worker_id: &str,
    lease: &ResearchStepLease,
    result: Result<String, DashboardError>,
) -> Result<Option<ResearchStepLease>, DashboardError> {
    match result {
        Ok(output) => {
            db.append_research_job_event(
                &lease.job.id,
                Some(&lease.step.id),
                "info",
                "worker completed step",
                Some(&output),
            )
            .await?;
            let step = db
                .complete_research_step(&lease.step.id, worker_id, Some(&output))
                .await?;
            Ok(Some(ResearchStepLease {
                job: lease.job.clone(),
                step,
            }))
        }
        Err(error) => {
            if let Some(cancelled) = cancelled_step_lease(db, lease).await? {
                let output = serde_json::json!({
                    "executor": "local_command",
                    "status": "cancelled",
                    "error": error.to_string(),
                })
                .to_string();
                db.append_research_job_event(
                    &lease.job.id,
                    Some(&lease.step.id),
                    "info",
                    "worker observed cancellation",
                    Some(&output),
                )
                .await?;
                return Ok(Some(cancelled));
            }
            let reason = error.to_string();
            db.append_research_job_event(
                &lease.job.id,
                Some(&lease.step.id),
                "warn",
                &reason,
                Some(r#"{"executor":"local_command","status":"blocked"}"#),
            )
            .await?;
            let step = db
                .block_research_step(&lease.step.id, worker_id, &reason)
                .await?;
            Ok(Some(ResearchStepLease {
                job: lease.job.clone(),
                step,
            }))
        }
    }
}

/// Return the fresh cancelled lease when the operator cancelled the active step.
async fn cancelled_step_lease(
    db: &impl ResearchWorkBackend,
    lease: &ResearchStepLease,
) -> Result<Option<ResearchStepLease>, DashboardError> {
    let Some(job) = db.get_research_job(&lease.job.id).await? else {
        return Ok(None);
    };
    if job.status != "cancelled" {
        return Ok(None);
    }
    let steps = db.get_research_job_steps(&lease.job.id).await?;
    let Some(step) = steps.into_iter().find(|step| step.id == lease.step.id) else {
        return Ok(None);
    };
    if step.status != "cancelled" {
        return Ok(None);
    }
    Ok(Some(ResearchStepLease { job, step }))
}

/// Return true when the leased job or its active step has been cancelled.
async fn report_step_is_cancelled(
    db: &impl ResearchWorkBackend,
    lease: &ResearchStepLease,
) -> Result<bool, DashboardError> {
    let Some(job) = db.get_research_job(&lease.job.id).await? else {
        return Ok(true);
    };
    if job.status == "cancelled" {
        return Ok(true);
    }
    let steps = db.get_research_job_steps(&lease.job.id).await?;
    Ok(steps
        .iter()
        .any(|step| step.id == lease.step.id && step.status == "cancelled"))
}

/// Return serialized command output for a completed command-backed step.
fn command_step_output(
    step_kind: ResearchStepKind,
    command: &CommandSpec,
    command_output: &CommandOutput,
) -> Result<String, DashboardError> {
    serde_json::to_string(&serde_json::json!({
        "executor": "local_command",
        "step_kind": step_kind.as_str(),
        "status": if command_output.cancelled {
            "cancelled"
        } else if command_output.success {
            "completed"
        } else {
            "failed"
        },
        "command": command,
        "command_output": command_output,
    }))
    .map_err(|e| DashboardError::Internal(format!("serializing command output: {e}")))
}

/// Return serialized export-step output.
fn export_step_output<T: serde::Serialize>(
    step_kind: ResearchStepKind,
    status: &str,
    value: &T,
) -> Result<String, DashboardError> {
    serde_json::to_string(&serde_json::json!({
        "executor": "local_export",
        "step_kind": step_kind.as_str(),
        "status": status,
        "result": value,
    }))
    .map_err(|e| DashboardError::Internal(format!("serializing export output: {e}")))
}

impl ResearchStepKind {
    /// Return the durable step name stored in the dashboard DB.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlanExport => "plan_export",
            Self::SnapshotOrCopyRuntime => "snapshot_or_copy_runtime",
            Self::WriteArtifactManifest => "write_artifact_manifest",
            Self::VerifyArtifact => "verify_artifact",
            Self::ValidateReplayData => "validate_replay_data",
            Self::ValidateBacktestInput => "validate_backtest_input",
            Self::PrepareBacktestInput => "prepare_backtest_input",
            Self::RunBacktest => "run_backtest",
            Self::RunSweep => "run_sweep",
            Self::WriteReport => "write_report",
        }
    }

    /// Return the local command kind for command-backed steps.
    fn command_kind(self) -> Option<BubaPaintCommandKind> {
        match self {
            Self::ValidateReplayData => Some(BubaPaintCommandKind::ValidateReplayData),
            Self::ValidateBacktestInput => Some(BubaPaintCommandKind::ValidateBacktestInput),
            Self::PrepareBacktestInput => Some(BubaPaintCommandKind::PrepareBacktestInput),
            Self::RunBacktest => Some(BubaPaintCommandKind::RunBacktest),
            Self::RunSweep => Some(BubaPaintCommandKind::RunSweep),
            Self::PlanExport
            | Self::SnapshotOrCopyRuntime
            | Self::WriteArtifactManifest
            | Self::VerifyArtifact
            | Self::WriteReport => None,
        }
    }
}

impl FromStr for ResearchStepKind {
    type Err = String;

    /// Parse a durable research step name.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "plan_export" => Ok(Self::PlanExport),
            "snapshot_or_copy_runtime" => Ok(Self::SnapshotOrCopyRuntime),
            "write_artifact_manifest" => Ok(Self::WriteArtifactManifest),
            "verify_artifact" => Ok(Self::VerifyArtifact),
            "validate_replay_data" => Ok(Self::ValidateReplayData),
            "validate_backtest_input" => Ok(Self::ValidateBacktestInput),
            "prepare_backtest_input" => Ok(Self::PrepareBacktestInput),
            "run_backtest" => Ok(Self::RunBacktest),
            "run_sweep" => Ok(Self::RunSweep),
            "write_report" => Ok(Self::WriteReport),
            _ => Err(format!("unknown research step '{value}'")),
        }
    }
}

#[cfg(test)]
#[path = "tests/research_worker_tests.rs"]
mod tests;
