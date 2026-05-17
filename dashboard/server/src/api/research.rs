//! Authenticated dashboard routes for research orchestration state.
//!
//! These handlers expose the central control-plane view of research machines,
//! exported artifacts, transfer records, jobs, events, and generated reports.
//! Mutating routes require an admin claim; read routes are available to any
//! authenticated dashboard user.

use std::path::{Component, Path as StdPath, PathBuf};

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::IntoResponse;

use crate::api::auth_routes::AppState;
use crate::auth::Claims;
use crate::db::{
    ArtifactTransferRecord, NullableUpdate, ResearchArtifact, ResearchArtifactRecord, ResearchJob,
    ResearchJobEvent, ResearchJobStep, ResearchMachine, ResearchMachineDependencyCounts,
    ResearchMachineRecord, ResearchReport,
};
use crate::error::DashboardError;
use crate::research_artifacts;

/// Response body for `GET /api/research/machines`.
#[derive(serde::Serialize)]
pub struct MachinesResponse {
    /// Known machines that can participate in export or backtest workflows.
    pub machines: Vec<crate::db::ResearchMachine>,
}

/// Response body for a single machine mutation or read.
#[derive(serde::Serialize)]
pub struct MachineResponse {
    /// Machine metadata row.
    pub machine: ResearchMachine,
}

/// Response body for `GET /api/research/machines/:id/health`.
#[derive(serde::Serialize)]
pub struct MachineHealthResponse {
    /// Machine metadata row.
    pub machine: ResearchMachine,
    /// Parsed machine details, including heartbeat data when present.
    pub details: Option<serde_json::Value>,
    /// Durable records that reference this machine.
    pub dependencies: ResearchMachineDependencyCounts,
    /// Whether new background work should be avoided for this machine.
    pub disabled: bool,
}

/// Response body for `GET /api/research/artifacts`.
#[derive(serde::Serialize)]
pub struct ArtifactsResponse {
    /// Persisted runtime artifacts available for research jobs.
    pub artifacts: Vec<crate::db::ResearchArtifact>,
}

/// Response body returned when an artifact import verifies and registers data.
#[derive(serde::Serialize)]
pub struct ImportArtifactResponse {
    /// Registered artifact row.
    pub artifact: crate::db::ResearchArtifact,
    /// Files and bytes verified from the artifact manifest.
    pub verification: research_artifacts::ArtifactVerification,
}

/// Response body returned when an artifact manifest is verified.
#[derive(serde::Serialize)]
pub struct VerifyArtifactResponse {
    /// Artifact metadata row.
    pub artifact: crate::db::ResearchArtifact,
    /// Files and bytes verified from the artifact manifest.
    pub verification: research_artifacts::ArtifactVerification,
}

/// Response body returned when remote artifact metadata is registered.
#[derive(serde::Serialize)]
pub struct RegisterArtifactResponse {
    /// Registered artifact row.
    pub artifact: crate::db::ResearchArtifact,
    /// Manifest-derived summary; remote file bytes are verified after transfer.
    pub manifest_summary: ArtifactManifestSummary,
}

/// Manifest-derived artifact size summary.
#[derive(serde::Serialize)]
pub struct ArtifactManifestSummary {
    /// Artifact ID declared by the manifest.
    pub artifact_id: String,
    /// Number of manifest file records.
    pub files: usize,
    /// Sum of payload bytes declared by the manifest.
    pub bytes: u64,
}

/// Response body for `GET /api/research/transfers`.
#[derive(serde::Serialize)]
pub struct TransfersResponse {
    /// Transfer attempts for moving artifacts between machines.
    pub transfers: Vec<crate::db::ArtifactTransfer>,
}

/// Request body for importing an already-local research artifact.
#[derive(serde::Deserialize)]
pub struct ImportArtifactRequest {
    /// Artifact root path, absolute under or relative to configured research work root.
    pub artifact_root: String,
    /// Optional expected artifact ID. When present, it must match the manifest.
    pub artifact_id: Option<String>,
    /// Source machine override. Defaults to the manifest source machine.
    pub source_machine_id: Option<String>,
    /// Artifact status to persist. Defaults to `available`.
    pub status: Option<String>,
}

/// Request body for registering a remote artifact from a portable manifest.
#[derive(serde::Deserialize)]
pub struct RegisterArtifactRequest {
    /// Absolute artifact root path on the source machine.
    pub artifact_root: String,
    /// Portable manifest read from the source artifact root.
    pub manifest: research_artifacts::ArtifactManifest,
    /// Source machine override. Defaults to the manifest source machine.
    pub source_machine_id: Option<String>,
    /// Artifact status to persist. Defaults to `available`.
    pub status: Option<String>,
}

/// Request body for creating a transfer record.
#[derive(serde::Deserialize)]
pub struct CreateTransferRequest {
    /// Artifact being moved.
    pub artifact_id: String,
    /// Source machine override.
    pub source_machine_id: Option<String>,
    /// Destination machine.
    pub dest_machine_id: Option<String>,
    /// Expected total bytes, when known.
    pub bytes_total: Option<u64>,
}

/// Request body for updating transfer progress.
#[derive(serde::Deserialize)]
pub struct TransferProgressRequest {
    /// Transfer status after this update.
    pub status: String,
    /// Bytes transferred so far.
    pub bytes_done: Option<u64>,
    /// Expected total bytes, when known.
    pub bytes_total: Option<u64>,
    /// Checksum verification status.
    pub checksum_status: Option<String>,
    /// Last error, when status is failed or retryable.
    pub error: Option<String>,
}

/// Request body for retrying a transfer.
#[derive(serde::Deserialize)]
pub struct RetryTransferRequest {
    /// Preserve transferred bytes for a resumable retry. Defaults to true.
    pub resume: Option<bool>,
}

/// Response body returned when a transfer verifies a local artifact.
#[derive(serde::Serialize)]
pub struct VerifyTransferResponse {
    /// Updated transfer row.
    pub transfer: crate::db::ArtifactTransfer,
    /// Files and bytes verified from the local destination manifest.
    pub verification: research_artifacts::ArtifactVerification,
}

/// Response body for `GET /api/research/jobs`.
#[derive(serde::Serialize)]
pub struct JobsResponse {
    /// Research jobs ordered by creation time.
    pub jobs: Vec<ResearchJob>,
}

/// Full job response returned by create, detail, cancel, and retry routes.
#[derive(serde::Serialize)]
pub struct JobDetailResponse {
    /// Durable job metadata and current lifecycle status.
    pub job: ResearchJob,
    /// Ordered step records for the job.
    pub steps: Vec<ResearchJobStep>,
    /// Timeline events associated with the job.
    pub events: Vec<ResearchJobEvent>,
}

/// Response body for `GET /api/research/jobs/:id/events`.
#[derive(serde::Serialize)]
pub struct EventsResponse {
    /// Timeline events for a single research job.
    pub events: Vec<ResearchJobEvent>,
}

/// Response body for `GET /api/research/reports`.
#[derive(serde::Serialize)]
pub struct ReportsResponse {
    /// Generated research reports.
    pub reports: Vec<crate::db::ResearchReport>,
}

/// Response body returned after report files are regenerated.
#[derive(serde::Serialize)]
pub struct RegenerateReportResponse {
    /// Updated or created report metadata row.
    pub report: ResearchReport,
    /// JSON report path that was written.
    pub report_path: String,
    /// CSV report path that was written.
    pub csv_path: String,
}

/// Request body for creating a research machine.
#[derive(serde::Deserialize)]
pub struct CreateMachineRequest {
    /// Stable machine ID used by workers, inventory, and URLs.
    pub id: String,
    /// Operator-facing display name.
    pub name: String,
    /// Machine role: `live`, `research`, or `controller`.
    pub role: String,
    /// Optional SSH alias used by operators and deployment scripts.
    pub ssh_alias: Option<String>,
    /// Optional initial status. Defaults to `not_configured`.
    pub status: Option<String>,
    /// Optional structured machine details.
    pub details: Option<serde_json::Value>,
}

/// Request body for patching research machine metadata.
#[derive(serde::Deserialize)]
pub struct UpdateMachineRequest {
    /// Optional replacement display name.
    pub name: Option<String>,
    /// Optional replacement role.
    pub role: Option<String>,
    /// Optional SSH alias; explicit `null` clears it.
    #[serde(default, deserialize_with = "deserialize_nullable_update")]
    pub ssh_alias: NullableUpdate<String>,
    /// Optional replacement lifecycle/readiness status.
    pub status: Option<String>,
    /// Optional structured details; explicit `null` clears it.
    #[serde(default, deserialize_with = "deserialize_nullable_update")]
    pub details: NullableUpdate<serde_json::Value>,
}

/// Request body sent by a remote research worker heartbeat.
#[derive(serde::Deserialize)]
pub struct WorkerHeartbeatRequest {
    /// Machine row that owns this worker.
    pub machine_id: String,
    /// Stable worker process ID used in leases and logs.
    pub worker_id: String,
    /// Optional worker binary version.
    pub worker_version: Option<String>,
    /// Worker status: `online`, `idle`, `busy`, `degraded`, or `error`.
    #[serde(default = "default_worker_status")]
    pub status: String,
    /// Optional structured machine or worker telemetry.
    pub details: Option<serde_json::Value>,
}

/// Response body returned after a heartbeat updates machine state.
#[derive(serde::Serialize)]
pub struct WorkerHeartbeatResponse {
    /// Updated machine row.
    pub machine: crate::db::ResearchMachine,
}

/// Request body for creating a research job.
#[derive(serde::Deserialize)]
pub struct CreateJobRequest {
    /// Durable job type, such as `export`, `current_params`, or `sweep`.
    pub job_type: String,
    /// Optional artifact to use as the input for backtest and sweep jobs.
    pub artifact_id: Option<String>,
    /// Queue priority, where larger values are leased before lower values.
    #[serde(default)]
    pub priority: i64,
    /// Job-specific parameters persisted as raw structured data.
    pub params: Option<serde_json::Value>,
}

/// Request body for updating a queued research job.
#[derive(serde::Deserialize)]
pub struct UpdateJobRequest {
    /// Optional artifact replacement; explicit `null` clears it when allowed.
    #[serde(default, deserialize_with = "deserialize_nullable_update")]
    pub artifact_id: NullableUpdate<String>,
    /// Optional replacement queue priority.
    pub priority: Option<i64>,
    /// Optional replacement params; explicit `null` clears stored params.
    #[serde(default, deserialize_with = "deserialize_nullable_update")]
    pub params: NullableUpdate<serde_json::Value>,
}

/// Request body for cloning a research job.
#[derive(serde::Deserialize)]
pub struct CloneJobRequest {
    /// Optional artifact override; explicit `null` clears it when allowed.
    #[serde(default, deserialize_with = "deserialize_nullable_update")]
    pub artifact_id: NullableUpdate<String>,
    /// Optional replacement queue priority.
    pub priority: Option<i64>,
    /// Optional replacement params; explicit `null` clears stored params.
    #[serde(default, deserialize_with = "deserialize_nullable_update")]
    pub params: NullableUpdate<serde_json::Value>,
}

/// Request body for appending a timeline event to a job.
#[derive(serde::Deserialize)]
pub struct AppendEventRequest {
    /// Optional step that owns this event; it must belong to the target job.
    pub step_id: Option<String>,
    /// Event severity accepted by the database validation layer.
    pub level: String,
    /// Human-readable event message.
    pub message: String,
    /// Optional structured event payload.
    pub details: Option<serde_json::Value>,
}

/// Request body for updating report metadata.
#[derive(serde::Deserialize)]
pub struct UpdateReportRequest {
    /// Optional replacement title.
    pub title: Option<String>,
    /// Optional lifecycle status, currently `available` or `archived`.
    pub status: Option<String>,
}

/// Query parameters for deleting a report.
#[derive(serde::Deserialize)]
pub struct DeleteReportQuery {
    /// Delete report files as well as metadata when explicitly true.
    #[serde(default)]
    pub delete_files: bool,
}

/// Request body for updating artifact metadata.
#[derive(serde::Deserialize)]
pub struct UpdateArtifactRequest {
    /// Optional corrected source machine.
    pub source_machine_id: Option<String>,
    /// Optional corrected run mode.
    pub run_mode: Option<String>,
    /// Optional replay quality class override.
    pub replay_quality_class: Option<String>,
    /// Optional backtest readiness class override.
    pub backtest_ready_class: Option<String>,
    /// Optional live fidelity class override.
    pub live_fidelity_class: Option<String>,
}

/// Query parameters for deleting an artifact.
#[derive(serde::Deserialize)]
pub struct DeleteArtifactQuery {
    /// Delete artifact files as well as metadata when explicitly true.
    #[serde(default)]
    pub delete_files: bool,
}

/// `GET /api/research/machines`
pub async fn list_machines(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, DashboardError> {
    let machines = state.db.list_research_machines().await?;
    Ok(Json(MachinesResponse { machines }))
}

/// `POST /api/research/machines`
pub async fn create_machine(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateMachineRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let status = req.status.as_deref().unwrap_or("not_configured");
    let details_json = json_value_to_string(req.details)?;
    let machine = state
        .db
        .create_research_machine(&ResearchMachineRecord {
            id: &req.id,
            name: &req.name,
            role: &req.role,
            ssh_alias: req.ssh_alias.as_deref(),
            status,
            details_json: details_json.as_deref(),
        })
        .await?;
    Ok(Json(MachineResponse { machine }))
}

/// `GET /api/research/machines/:id`
pub async fn get_machine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let machine = research_machine_by_id(&state, &id).await?;
    Ok(Json(MachineResponse { machine }))
}

/// `PATCH /api/research/machines/:id`
pub async fn update_machine(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<UpdateMachineRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let current = research_machine_by_id(&state, &id).await?;
    let name = req.name.as_deref().unwrap_or(&current.name);
    let role = req.role.as_deref().unwrap_or(&current.role);
    let status = req.status.as_deref().unwrap_or(&current.status);
    let ssh_alias = match req.ssh_alias {
        NullableUpdate::Unchanged => current.ssh_alias,
        NullableUpdate::Clear => None,
        NullableUpdate::Set(value) => Some(value),
    };
    let details_json = match req.details {
        NullableUpdate::Unchanged => current.details_json,
        NullableUpdate::Clear => None,
        NullableUpdate::Set(value) => json_value_to_string(Some(value))?,
    };
    let machine = state
        .db
        .update_research_machine(&ResearchMachineRecord {
            id: &current.id,
            name,
            role,
            ssh_alias: ssh_alias.as_deref(),
            status,
            details_json: details_json.as_deref(),
        })
        .await?;
    Ok(Json(MachineResponse { machine }))
}

/// `POST /api/research/machines/:id/disable`
pub async fn disable_machine(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let machine = state
        .db
        .set_research_machine_status(&id, "disabled")
        .await?;
    Ok(Json(MachineResponse { machine }))
}

/// `POST /api/research/machines/:id/enable`
pub async fn enable_machine(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let machine = state
        .db
        .set_research_machine_status(&id, "configured")
        .await?;
    Ok(Json(MachineResponse { machine }))
}

/// `DELETE /api/research/machines/:id`
pub async fn delete_machine(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let machine = state.db.delete_research_machine(&id).await?;
    Ok(Json(MachineResponse { machine }))
}

/// `GET /api/research/machines/:id/health`
pub async fn get_machine_health(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let machine = research_machine_by_id(&state, &id).await?;
    let dependencies = state.db.research_machine_dependency_counts(&id).await?;
    let details = parse_stored_json("machine details_json", machine.details_json.as_deref())?;
    let disabled = machine.status == "disabled";
    Ok(Json(MachineHealthResponse {
        machine,
        details,
        dependencies,
        disabled,
    }))
}

/// `POST /api/research/workers/heartbeat`
pub async fn worker_heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WorkerHeartbeatRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let machine = state
        .db
        .record_research_machine_heartbeat(
            &req.machine_id,
            &req.worker_id,
            req.worker_version.as_deref(),
            &req.status,
            req.details,
        )
        .await?;
    Ok(Json(WorkerHeartbeatResponse { machine }))
}

/// `GET /api/research/artifacts`
pub async fn list_artifacts(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, DashboardError> {
    let artifacts = state.db.list_research_artifacts().await?;
    Ok(Json(ArtifactsResponse { artifacts }))
}

/// `GET /api/research/artifacts/:id`
pub async fn get_artifact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let artifact = research_artifact_by_id(&state, &id).await?;
    Ok(Json(artifact))
}

/// `PATCH /api/research/artifacts/:id`
pub async fn update_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<UpdateArtifactRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let current = research_artifact_by_id(&state, &id).await?;
    let source_machine_id = req
        .source_machine_id
        .as_deref()
        .or(current.source_machine_id.as_deref());
    ensure_source_machine(&state, source_machine_id).await?;
    let run_mode = req.run_mode.as_deref().or(current.run_mode.as_deref());
    validate_optional_metadata("run_mode", run_mode)?;
    let replay_quality_class = req
        .replay_quality_class
        .as_deref()
        .or(current.replay_quality_class.as_deref());
    let backtest_ready_class = req
        .backtest_ready_class
        .as_deref()
        .or(current.backtest_ready_class.as_deref());
    let live_fidelity_class = req
        .live_fidelity_class
        .as_deref()
        .or(current.live_fidelity_class.as_deref());
    validate_optional_metadata("replay_quality_class", replay_quality_class)?;
    validate_optional_metadata("backtest_ready_class", backtest_ready_class)?;
    validate_optional_metadata("live_fidelity_class", live_fidelity_class)?;
    let artifact = state
        .db
        .upsert_research_artifact(&ResearchArtifactRecord {
            id: &current.id,
            source_machine_id,
            kind: &current.kind,
            status: &current.status,
            run_mode,
            artifact_root: current.artifact_root.as_deref(),
            manifest_path: current.manifest_path.as_deref(),
            bundle_path: current.bundle_path.as_deref(),
            source_db_path: current.source_db_path.as_deref(),
            interval_start_ms: current.interval_start_ms,
            interval_end_ms: current.interval_end_ms,
            bytes: current.bytes,
            checksum: current.checksum.as_deref(),
            replay_quality_class,
            backtest_ready_class,
            live_fidelity_class,
        })
        .await?;
    Ok(Json(artifact))
}

/// `POST /api/research/artifacts/:id/verify`
pub async fn verify_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let artifact = research_artifact_by_id(&state, &id).await?;
    let root = resolve_artifact_root_path(&state, &artifact)?;
    let verification = research_artifacts::verify_artifact(&root)?;
    Ok(Json(VerifyArtifactResponse {
        artifact,
        verification,
    }))
}

/// `POST /api/research/artifacts/:id/archive`
pub async fn archive_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let artifact = state.db.archive_research_artifact(&id).await?;
    Ok(Json(artifact))
}

/// `POST /api/research/artifacts/:id/restore`
pub async fn restore_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let artifact = state.db.restore_research_artifact(&id).await?;
    Ok(Json(artifact))
}

/// `DELETE /api/research/artifacts/:id`
pub async fn delete_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(query): Query<DeleteArtifactQuery>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let artifact = state.db.ensure_research_artifact_deletable(&id).await?;
    if query.delete_files {
        delete_artifact_files(&state, &artifact)?;
    }
    let deleted = state.db.delete_research_artifact(&id).await?;
    Ok(Json(deleted))
}

/// `GET /api/research/artifacts/:id/manifest`
pub async fn get_artifact_manifest(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let artifact = research_artifact_by_id(&state, &id).await?;
    let root = resolve_artifact_root_path(&state, &artifact)?;
    let manifest = research_artifacts::read_manifest(&root)?;
    Ok(Json(manifest))
}

/// `GET /api/research/artifacts/:id/checksums`
pub async fn get_artifact_checksums(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let artifact = research_artifact_by_id(&state, &id).await?;
    let root = resolve_artifact_root_path(&state, &artifact)?;
    let manifest = research_artifacts::read_manifest(&root)?;
    let checksums = research_artifacts::checksum_text(&manifest);
    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        checksums,
    ))
}

/// `POST /api/research/artifacts/import`
pub async fn import_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ImportArtifactRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let work_root = state.research_work_root.as_deref().ok_or_else(|| {
        DashboardError::BadRequest("BUBA_RESEARCH_WORK_ROOT is not configured".to_string())
    })?;
    let artifact_root = resolve_research_path(work_root, &req.artifact_root)?;
    let verification = research_artifacts::verify_artifact(&artifact_root)?;
    if let Some(expected) = req.artifact_id.as_deref()
        && expected != verification.artifact_id
    {
        return Err(DashboardError::BadRequest(format!(
            "artifact_id '{}' does not match manifest '{}'",
            expected, verification.artifact_id
        )));
    }
    let manifest = research_artifacts::read_manifest(&artifact_root)?;
    validate_artifact_manifest(&manifest)?;
    let status = req.status.unwrap_or_else(|| "available".to_string());
    if status.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact status must not be empty".to_string(),
        ));
    }
    let source_db_path = runtime_db_relative_path(&manifest)
        .map(|path| {
            research_artifacts::safe_join(&artifact_root, path).map(|path| path_to_string(&path))
        })
        .transpose()?;
    let checksum = artifact_checksum(&manifest);
    let bytes = Some(manifest_payload_bytes(&manifest));
    let artifact_root_text = path_to_string(&artifact_root);
    let manifest_path_text = path_to_string(&artifact_root.join("manifest.json"));
    let source_machine_id = req
        .source_machine_id
        .as_deref()
        .or(manifest.source_machine_id.as_deref());
    ensure_manifest_source_matches(req.source_machine_id.as_deref(), &manifest)?;
    ensure_source_machine(&state, source_machine_id).await?;
    let artifact = state
        .db
        .upsert_research_artifact(&ResearchArtifactRecord {
            id: &manifest.artifact_id,
            source_machine_id,
            kind: &manifest.kind,
            status: &status,
            run_mode: manifest.run_mode.as_deref(),
            artifact_root: Some(&artifact_root_text),
            manifest_path: Some(&manifest_path_text),
            bundle_path: None,
            source_db_path: source_db_path.as_deref(),
            interval_start_ms: manifest.interval_start_ms,
            interval_end_ms: manifest.interval_end_ms,
            bytes,
            checksum,
            replay_quality_class: None,
            backtest_ready_class: None,
            live_fidelity_class: None,
        })
        .await?;
    Ok(Json(ImportArtifactResponse {
        artifact,
        verification,
    }))
}

/// `POST /api/research/artifacts/register`
pub async fn register_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterArtifactRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    validate_artifact_manifest(&req.manifest)?;
    ensure_manifest_source_matches(req.source_machine_id.as_deref(), &req.manifest)?;
    let artifact_root = normalize_remote_artifact_root(&req.artifact_root)?;
    let source_machine_id = req
        .source_machine_id
        .as_deref()
        .or(req.manifest.source_machine_id.as_deref());
    ensure_source_machine(&state, source_machine_id).await?;
    let status = req.status.unwrap_or_else(|| "available".to_string());
    if status.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact status must not be empty".to_string(),
        ));
    }

    let source_db_path = runtime_db_relative_path(&req.manifest)
        .map(|path| remote_child_path(&artifact_root, path))
        .transpose()?;
    let manifest_path = remote_child_path(&artifact_root, "manifest.json")?;
    let bytes = manifest_payload_bytes(&req.manifest);
    let artifact = state
        .db
        .upsert_research_artifact(&ResearchArtifactRecord {
            id: &req.manifest.artifact_id,
            source_machine_id,
            kind: &req.manifest.kind,
            status: &status,
            run_mode: req.manifest.run_mode.as_deref(),
            artifact_root: Some(&artifact_root),
            manifest_path: Some(&manifest_path),
            bundle_path: None,
            source_db_path: source_db_path.as_deref(),
            interval_start_ms: req.manifest.interval_start_ms,
            interval_end_ms: req.manifest.interval_end_ms,
            bytes: Some(bytes),
            checksum: artifact_checksum(&req.manifest),
            replay_quality_class: None,
            backtest_ready_class: None,
            live_fidelity_class: None,
        })
        .await?;
    Ok(Json(RegisterArtifactResponse {
        artifact,
        manifest_summary: ArtifactManifestSummary {
            artifact_id: req.manifest.artifact_id,
            files: req.manifest.files.len(),
            bytes,
        },
    }))
}

/// `GET /api/research/transfers`
pub async fn list_transfers(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, DashboardError> {
    let transfers = state.db.list_artifact_transfers().await?;
    Ok(Json(TransfersResponse { transfers }))
}

/// `POST /api/research/transfers`
pub async fn create_transfer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTransferRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let artifact = state
        .db
        .get_research_artifact(&req.artifact_id)
        .await?
        .ok_or_else(|| {
            DashboardError::NotFound(format!("artifact '{}' not found", req.artifact_id))
        })?;
    let source_machine_id = req
        .source_machine_id
        .as_deref()
        .or(artifact.source_machine_id.as_deref());
    let bytes_total = req.bytes_total.or(artifact.bytes);
    let transfer = state
        .db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &req.artifact_id,
            source_machine_id,
            dest_machine_id: req.dest_machine_id.as_deref(),
            bytes_total,
        })
        .await?;
    Ok(Json(transfer))
}

/// `GET /api/research/transfers/:id`
pub async fn get_transfer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let transfer =
        state.db.get_artifact_transfer(&id).await?.ok_or_else(|| {
            DashboardError::NotFound(format!("artifact transfer '{id}' not found"))
        })?;
    Ok(Json(transfer))
}

/// `POST /api/research/transfers/:id/progress`
pub async fn update_transfer_progress(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<TransferProgressRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let transfer = state
        .db
        .update_artifact_transfer_progress(
            &id,
            &req.status,
            req.bytes_done,
            req.bytes_total,
            req.checksum_status.as_deref(),
            req.error.as_deref(),
        )
        .await?;
    Ok(Json(transfer))
}

/// `POST /api/research/transfers/:id/cancel`
pub async fn cancel_transfer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let transfer = state.db.cancel_artifact_transfer(&id).await?;
    Ok(Json(transfer))
}

/// `POST /api/research/transfers/:id/pause`
pub async fn pause_transfer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let transfer = state.db.pause_artifact_transfer(&id).await?;
    Ok(Json(transfer))
}

/// `POST /api/research/transfers/:id/resume`
pub async fn resume_transfer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let transfer = state.db.resume_artifact_transfer(&id).await?;
    Ok(Json(transfer))
}

/// `POST /api/research/transfers/:id/retry`
pub async fn retry_transfer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<RetryTransferRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let transfer = state
        .db
        .retry_artifact_transfer(&id, req.resume.unwrap_or(true))
        .await?;
    Ok(Json(transfer))
}

/// `POST /api/research/transfers/:id/verify`
pub async fn verify_transfer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let transfer =
        state.db.get_artifact_transfer(&id).await?.ok_or_else(|| {
            DashboardError::NotFound(format!("artifact transfer '{id}' not found"))
        })?;
    let artifact = state
        .db
        .get_research_artifact(&transfer.artifact_id)
        .await?
        .ok_or_else(|| {
            DashboardError::NotFound(format!("artifact '{}' not found", transfer.artifact_id))
        })?;
    let work_root = state.research_work_root.as_deref().ok_or_else(|| {
        DashboardError::BadRequest("BUBA_RESEARCH_WORK_ROOT is not configured".to_string())
    })?;
    let artifact_root = artifact.artifact_root.as_deref().ok_or_else(|| {
        DashboardError::BadRequest(format!("artifact '{}' has no artifact_root", artifact.id))
    })?;
    let local_root = resolve_research_path(work_root, artifact_root)?;
    let verification = research_artifacts::verify_artifact(&local_root)?;
    let transfer = state
        .db
        .update_artifact_transfer_progress(
            &id,
            "completed",
            Some(verification.bytes_checked),
            Some(verification.bytes_checked),
            Some("verified"),
            None,
        )
        .await?;
    Ok(Json(VerifyTransferResponse {
        transfer,
        verification,
    }))
}

/// `DELETE /api/research/transfers/:id`
pub async fn delete_transfer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let transfer = state.db.delete_artifact_transfer(&id).await?;
    Ok(Json(transfer))
}

/// `GET /api/research/jobs`
pub async fn list_jobs(State(state): State<AppState>) -> Result<impl IntoResponse, DashboardError> {
    let jobs = state.db.list_research_jobs().await?;
    Ok(Json(JobsResponse { jobs }))
}

/// `POST /api/research/jobs`
pub async fn create_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let params_json = req.params.as_ref().map(serde_json::Value::to_string);
    let job = state
        .db
        .create_research_job(
            &req.job_type,
            req.artifact_id.as_deref(),
            &claims.sub,
            req.priority,
            params_json.as_deref(),
        )
        .await?;
    let detail = job_detail(&state, &job.id).await?;
    Ok(Json(detail))
}

/// `GET /api/research/jobs/:id`
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let detail = job_detail(&state, &id).await?;
    Ok(Json(detail))
}

/// `DELETE /api/research/jobs/:id`
pub async fn delete_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let job = state.db.delete_research_job(&id).await?;
    Ok(Json(job))
}

/// `PATCH /api/research/jobs/:id`
pub async fn update_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<UpdateJobRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let params_json = serialize_optional_json_update(req.params);
    state
        .db
        .update_queued_research_job(
            &id,
            nullable_string_update_as_deref(&req.artifact_id),
            req.priority,
            nullable_string_update_as_deref(&params_json),
        )
        .await?;
    let detail = job_detail(&state, &id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:id/cancel`
pub async fn cancel_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state.db.cancel_research_job(&id).await?;
    let detail = job_detail(&state, &id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:id/pause`
pub async fn pause_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state.db.pause_research_job(&id).await?;
    let detail = job_detail(&state, &id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:id/resume`
pub async fn resume_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state.db.resume_research_job(&id).await?;
    let detail = job_detail(&state, &id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:id/continue`
pub async fn continue_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state.db.resume_research_job(&id).await?;
    let detail = job_detail(&state, &id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:id/retry`
pub async fn retry_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state.db.retry_research_job(&id).await?;
    let detail = job_detail(&state, &id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:id/clone`
pub async fn clone_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<CloneJobRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let source = state
        .db
        .get_research_job(&id)
        .await?
        .ok_or_else(|| DashboardError::NotFound(format!("research job '{id}' not found")))?;
    let params_json = serialize_optional_json_update(req.params);
    let artifact_id = match req.artifact_id {
        NullableUpdate::Unchanged => source.artifact_id,
        NullableUpdate::Clear => None,
        NullableUpdate::Set(value) => Some(value),
    };
    let priority = req.priority.unwrap_or(source.priority);
    let params_json = match params_json {
        NullableUpdate::Unchanged => source.params_json,
        NullableUpdate::Clear => None,
        NullableUpdate::Set(value) => Some(value),
    };
    let cloned = state
        .db
        .create_research_job(
            &source.job_type,
            artifact_id.as_deref(),
            &claims.sub,
            priority,
            params_json.as_deref(),
        )
        .await?;
    let details = serde_json::json!({
        "source_job_id": id,
        "source_job_status": source.status,
    });
    state
        .db
        .append_research_job_event(
            &cloned.id,
            None,
            "info",
            "cloned from prior research job",
            Some(&details.to_string()),
        )
        .await?;
    let detail = job_detail(&state, &cloned.id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:job_id/steps/:step_id/retry`
pub async fn retry_step(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((job_id, step_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state.db.retry_research_step(&job_id, &step_id).await?;
    let detail = job_detail(&state, &job_id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:job_id/steps/:step_id/cancel`
pub async fn cancel_step(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((job_id, step_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state.db.cancel_research_step(&job_id, &step_id).await?;
    let detail = job_detail(&state, &job_id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:job_id/steps/:step_id/clear-lease`
pub async fn clear_step_lease(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((job_id, step_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state
        .db
        .clear_stale_research_step_lease(&job_id, &step_id)
        .await?;
    let detail = job_detail(&state, &job_id).await?;
    Ok(Json(detail))
}

/// `POST /api/research/jobs/:job_id/steps/:step_id/resolve-blocker`
pub async fn resolve_step_blocker(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((job_id, step_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    state
        .db
        .resolve_research_step_blocker(&job_id, &step_id)
        .await?;
    let detail = job_detail(&state, &job_id).await?;
    Ok(Json(detail))
}

/// `GET /api/research/jobs/:id/events`
pub async fn list_job_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    ensure_job_exists(&state, &id).await?;
    let events = state.db.list_research_job_events(&id).await?;
    Ok(Json(EventsResponse { events }))
}

/// `POST /api/research/jobs/:id/events`
pub async fn append_job_event(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<AppendEventRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let details_json = req.details.as_ref().map(serde_json::Value::to_string);
    let event = state
        .db
        .append_research_job_event(
            &id,
            req.step_id.as_deref(),
            &req.level,
            &req.message,
            details_json.as_deref(),
        )
        .await?;
    Ok(Json(event))
}

/// `POST /api/research/jobs/:id/report/regenerate`
pub async fn regenerate_job_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let detail = job_detail(&state, &id).await?;
    let existing = state.db.get_research_report_for_job(&id).await?;
    if existing.is_none() && !can_regenerate_report_for_status(&detail.job.status) {
        return Err(DashboardError::BadRequest(format!(
            "research job '{}' is not ready for report regeneration from status '{}'",
            detail.job.id, detail.job.status
        )));
    }
    let report_paths = resolve_regenerated_report_paths(&state, &detail.job.id, existing.as_ref())?;
    let summary_json = write_regenerated_report_files(&detail, existing.as_ref(), &report_paths)?;
    let default_title = format!("Research job {}", detail.job.id);
    let title = existing
        .as_ref()
        .map_or(default_title.as_str(), |report| report.title.as_str());
    let status = existing
        .as_ref()
        .map_or("available", |report| report.status.as_str());
    let artifact_id = existing
        .as_ref()
        .and_then(|report| report.artifact_id.as_deref())
        .or(detail.job.artifact_id.as_deref());
    let report = state
        .db
        .create_or_update_research_report(&crate::db::ResearchReportRecord {
            job_id: &detail.job.id,
            artifact_id,
            title,
            status,
            summary_json: Some(&summary_json),
            report_path: Some(&path_to_string(&report_paths.report_path)),
            csv_path: Some(&path_to_string(&report_paths.csv_path)),
        })
        .await?;
    Ok(Json(RegenerateReportResponse {
        report,
        report_path: path_to_string(&report_paths.report_path),
        csv_path: path_to_string(&report_paths.csv_path),
    }))
}

/// `GET /api/research/reports`
pub async fn list_reports(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, DashboardError> {
    let reports = state.db.list_research_reports().await?;
    Ok(Json(ReportsResponse { reports }))
}

/// `GET /api/research/reports/:id`
pub async fn get_report(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let report = research_report_by_id(&state, &id).await?;
    Ok(Json(report))
}

/// `PATCH /api/research/reports/:id`
pub async fn update_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<UpdateReportRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let current = research_report_by_id(&state, &id).await?;
    let title = req.title.as_deref().unwrap_or(&current.title);
    let status = req.status.as_deref().unwrap_or(&current.status);
    let report = state
        .db
        .update_research_report_metadata(&id, title, status)
        .await?;
    Ok(Json(report))
}

/// `POST /api/research/reports/:id/archive`
pub async fn archive_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let current = research_report_by_id(&state, &id).await?;
    let report = state
        .db
        .update_research_report_metadata(&id, &current.title, "archived")
        .await?;
    Ok(Json(report))
}

/// `POST /api/research/reports/:id/restore`
pub async fn restore_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let current = research_report_by_id(&state, &id).await?;
    let report = state
        .db
        .update_research_report_metadata(&id, &current.title, "available")
        .await?;
    Ok(Json(report))
}

/// `DELETE /api/research/reports/:id`
pub async fn delete_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(query): Query<DeleteReportQuery>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let current = research_report_by_id(&state, &id).await?;
    if query.delete_files {
        delete_report_files(&state, &current)?;
    }
    let report = state.db.delete_research_report(&id).await?;
    Ok(Json(report))
}

/// `GET /api/research/reports/:id/json`
pub async fn get_report_json_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let report = research_report_by_id(&state, &id).await?;
    let text = read_report_file(&state, report.report_path.as_deref(), "report_path")?;
    let value = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|e| DashboardError::Internal(format!("parsing report JSON: {e}")))?;
    Ok(Json(value))
}

/// `GET /api/research/reports/:id/csv`
pub async fn get_report_csv_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let report = research_report_by_id(&state, &id).await?;
    let text = read_report_file(&state, report.csv_path.as_deref(), "csv_path")?;
    Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8")], text))
}

/// Return one research machine by ID or a route-level not found error.
async fn research_machine_by_id(
    state: &AppState,
    id: &str,
) -> Result<ResearchMachine, DashboardError> {
    state
        .db
        .get_research_machine(id)
        .await?
        .ok_or_else(|| DashboardError::NotFound(format!("research machine '{id}' not found")))
}

/// Serialize an optional JSON value for DB storage.
fn json_value_to_string(
    value: Option<serde_json::Value>,
) -> Result<Option<String>, DashboardError> {
    value
        .map(|value| {
            serde_json::to_string(&value)
                .map_err(|error| DashboardError::Internal(format!("serializing JSON: {error}")))
        })
        .transpose()
}

/// Parse optional JSON stored in machine metadata.
fn parse_stored_json(
    name: &str,
    value: Option<&str>,
) -> Result<Option<serde_json::Value>, DashboardError> {
    value
        .map(|value| {
            serde_json::from_str(value)
                .map_err(|error| DashboardError::Internal(format!("parsing {name}: {error}")))
        })
        .transpose()
}

/// Return one artifact by ID or a route-level not found error.
async fn research_artifact_by_id(
    state: &AppState,
    id: &str,
) -> Result<ResearchArtifact, DashboardError> {
    state
        .db
        .get_research_artifact(id)
        .await?
        .ok_or_else(|| DashboardError::NotFound(format!("artifact '{id}' not found")))
}

/// Validate an optional non-empty metadata string.
fn validate_optional_metadata(name: &str, value: Option<&str>) -> Result<(), DashboardError> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(DashboardError::BadRequest(format!(
            "{name} must not be empty"
        )));
    }
    Ok(())
}

/// Resolve one artifact root under the configured research work root.
fn resolve_artifact_root_path(
    state: &AppState,
    artifact: &ResearchArtifact,
) -> Result<PathBuf, DashboardError> {
    let work_root = state.research_work_root.as_deref().ok_or_else(|| {
        DashboardError::BadRequest("research work root is not configured".to_string())
    })?;
    let stored_root = if let Some(root) = artifact.artifact_root.as_deref() {
        root.to_string()
    } else if let Some(manifest_path) = artifact.manifest_path.as_deref() {
        PathBuf::from(manifest_path)
            .parent()
            .map(path_to_string)
            .ok_or_else(|| {
                DashboardError::BadRequest(format!(
                    "artifact '{}' manifest path has no parent",
                    artifact.id
                ))
            })?
    } else {
        return Err(DashboardError::BadRequest(format!(
            "artifact '{}' has no local artifact root",
            artifact.id
        )));
    };
    let root = resolve_research_path(work_root, &stored_root)?;
    let work_root = normalize_path(&PathBuf::from(work_root))?;
    if root == work_root {
        return Err(DashboardError::BadRequest(
            "artifact root must not be the research work root".to_string(),
        ));
    }
    Ok(root)
}

/// Delete local artifact files after metadata dependency checks pass.
fn delete_artifact_files(
    state: &AppState,
    artifact: &ResearchArtifact,
) -> Result<(), DashboardError> {
    let root = resolve_artifact_root_path(state, artifact)?;
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(DashboardError::BadRequest(format!(
            "artifact root is not a directory: {}",
            path_to_string(&root)
        )));
    }
    std::fs::remove_dir_all(&root)
        .map_err(|e| DashboardError::Internal(format!("deleting artifact files: {e}")))
}

/// Return one report by ID or a route-level not found error.
async fn research_report_by_id(
    state: &AppState,
    id: &str,
) -> Result<ResearchReport, DashboardError> {
    state
        .db
        .get_research_report(id)
        .await?
        .ok_or_else(|| DashboardError::NotFound(format!("report '{id}' not found")))
}

/// Local file paths used when regenerating one report.
struct RegeneratedReportPaths {
    /// JSON report path.
    report_path: PathBuf,
    /// CSV report path.
    csv_path: PathBuf,
}

/// Return whether a job status is ready for report regeneration without execution.
fn can_regenerate_report_for_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "blocked" | "failed" | "cancelled" | "retryable"
    )
}

/// Resolve report paths from existing metadata or default job paths.
fn resolve_regenerated_report_paths(
    state: &AppState,
    job_id: &str,
    existing: Option<&ResearchReport>,
) -> Result<RegeneratedReportPaths, DashboardError> {
    let report_path = match existing.and_then(|report| report.report_path.as_deref()) {
        Some(path) => resolve_report_file_path(state, Some(path), "report_path")?,
        None => resolve_default_report_file_path(state, job_id, "report.json")?,
    };
    let csv_path = match existing.and_then(|report| report.csv_path.as_deref()) {
        Some(path) => resolve_report_file_path(state, Some(path), "csv_path")?,
        None => resolve_default_report_file_path(state, job_id, "report.csv")?,
    };
    Ok(RegeneratedReportPaths {
        report_path,
        csv_path,
    })
}

/// Resolve a default generated report path under the configured work root.
fn resolve_default_report_file_path(
    state: &AppState,
    job_id: &str,
    file_name: &str,
) -> Result<PathBuf, DashboardError> {
    let work_root = state.research_work_root.as_deref().ok_or_else(|| {
        DashboardError::BadRequest("research work root is not configured".to_string())
    })?;
    resolve_research_path(work_root, &format!("jobs/{job_id}/{file_name}"))
}

/// Write regenerated report JSON and CSV files from durable job detail.
fn write_regenerated_report_files(
    detail: &JobDetailResponse,
    existing: Option<&ResearchReport>,
    paths: &RegeneratedReportPaths,
) -> Result<String, DashboardError> {
    create_report_parent_dir(&paths.report_path, "report_path")?;
    create_report_parent_dir(&paths.csv_path, "csv_path")?;
    let summary = serde_json::json!({
        "schema_version": 1,
        "regenerated": true,
        "job": &detail.job,
        "steps": &detail.steps,
        "events": &detail.events,
        "existing_report": existing,
        "report_json_path": path_to_string(&paths.report_path),
        "report_csv_path": path_to_string(&paths.csv_path),
    });
    let summary_json = serde_json::to_string_pretty(&summary)
        .map_err(|error| DashboardError::Internal(format!("serializing report: {error}")))?;
    std::fs::write(&paths.report_path, &summary_json)
        .map_err(|error| DashboardError::Internal(format!("writing report JSON: {error}")))?;
    std::fs::write(&paths.csv_path, regenerated_report_csv(&detail.steps))
        .map_err(|error| DashboardError::Internal(format!("writing report CSV: {error}")))?;
    Ok(summary_json)
}

/// Ensure a report file parent directory exists.
fn create_report_parent_dir(path: &StdPath, field_name: &str) -> Result<(), DashboardError> {
    let parent = path.parent().ok_or_else(|| {
        DashboardError::BadRequest(format!("report {field_name} has no parent directory"))
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|error| DashboardError::Internal(format!("creating report directory: {error}")))
}

/// Render a compact CSV summary of regenerated report steps.
fn regenerated_report_csv(steps: &[ResearchJobStep]) -> String {
    let mut csv =
        String::from("step_index,name,status,attempts,error,started_at,completed_at,updated_at\n");
    for step in steps {
        csv.push_str(&csv_field(&step.step_index));
        csv.push(',');
        csv.push_str(&csv_field(&step.name));
        csv.push(',');
        csv.push_str(&csv_field(&step.status));
        csv.push(',');
        csv.push_str(&csv_field(&step.attempts));
        csv.push(',');
        csv.push_str(&csv_optional_field(step.error.as_deref()));
        csv.push(',');
        csv.push_str(&csv_optional_field(step.started_at.as_ref()));
        csv.push(',');
        csv.push_str(&csv_optional_field(step.completed_at.as_ref()));
        csv.push(',');
        csv.push_str(&csv_field(&step.updated_at));
        csv.push('\n');
    }
    csv
}

/// Render one CSV field with minimal RFC 4180 escaping.
fn csv_field<T: ToString + ?Sized>(value: &T) -> String {
    let value = value.to_string();
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

/// Render an optional CSV field.
fn csv_optional_field<T: ToString + ?Sized>(value: Option<&T>) -> String {
    value.map(csv_field).unwrap_or_default()
}

/// Read one stored report file as UTF-8 text.
fn read_report_file(
    state: &AppState,
    stored_path: Option<&str>,
    field_name: &str,
) -> Result<String, DashboardError> {
    let path = resolve_report_file_path(state, stored_path, field_name)?;
    std::fs::read_to_string(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            DashboardError::NotFound(format!("report file not found for {field_name}"))
        }
        _ => DashboardError::Internal(format!("reading report file: {e}")),
    })
}

/// Delete report files referenced by a metadata row.
fn delete_report_files(state: &AppState, report: &ResearchReport) -> Result<(), DashboardError> {
    remove_report_file(state, report.report_path.as_deref(), "report_path")?;
    remove_report_file(state, report.csv_path.as_deref(), "csv_path")?;
    Ok(())
}

/// Remove one stored report file if it exists.
fn remove_report_file(
    state: &AppState,
    stored_path: Option<&str>,
    field_name: &str,
) -> Result<(), DashboardError> {
    let path = resolve_report_file_path(state, stored_path, field_name)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DashboardError::Internal(format!(
            "deleting report file: {error}"
        ))),
    }
}

/// Resolve a stored report file under the configured research work root.
fn resolve_report_file_path(
    state: &AppState,
    stored_path: Option<&str>,
    field_name: &str,
) -> Result<PathBuf, DashboardError> {
    let work_root = state.research_work_root.as_deref().ok_or_else(|| {
        DashboardError::BadRequest("research work root is not configured".to_string())
    })?;
    let stored_path = stored_path.ok_or_else(|| {
        DashboardError::BadRequest(format!("report {field_name} is not available"))
    })?;
    resolve_research_path(work_root, stored_path)
}

/// Deserialize a nullable field update while preserving missing versus null.
fn deserialize_nullable_update<'de, D, T>(deserializer: D) -> Result<NullableUpdate<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    let value = <Option<T> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(match value {
        Some(value) => NullableUpdate::Set(value),
        None => NullableUpdate::Clear,
    })
}

/// Serialize a nullable JSON field update.
fn serialize_optional_json_update(
    update: NullableUpdate<serde_json::Value>,
) -> NullableUpdate<String> {
    match update {
        NullableUpdate::Unchanged => NullableUpdate::Unchanged,
        NullableUpdate::Clear => NullableUpdate::Clear,
        NullableUpdate::Set(json) => NullableUpdate::Set(json.to_string()),
    }
}

/// Borrow a nullable string update as string slices.
fn nullable_string_update_as_deref(update: &NullableUpdate<String>) -> NullableUpdate<&str> {
    match update {
        NullableUpdate::Unchanged => NullableUpdate::Unchanged,
        NullableUpdate::Clear => NullableUpdate::Clear,
        NullableUpdate::Set(value) => NullableUpdate::Set(value.as_str()),
    }
}

/// Return full job detail for one job ID.
async fn job_detail(state: &AppState, id: &str) -> Result<JobDetailResponse, DashboardError> {
    let job = state
        .db
        .get_research_job(id)
        .await?
        .ok_or_else(|| DashboardError::NotFound(format!("research job '{id}' not found")))?;
    let steps = state.db.get_research_job_steps(id).await?;
    let events = state.db.list_research_job_events(id).await?;
    Ok(JobDetailResponse { job, steps, events })
}

/// Check that one job exists.
async fn ensure_job_exists(state: &AppState, id: &str) -> Result<(), DashboardError> {
    if state.db.get_research_job(id).await?.is_none() {
        return Err(DashboardError::NotFound(format!(
            "research job '{id}' not found"
        )));
    }
    Ok(())
}

/// Ensure an optional source machine ID exists.
async fn ensure_source_machine(
    state: &AppState,
    machine_id: Option<&str>,
) -> Result<(), DashboardError> {
    let Some(machine_id) = machine_id else {
        return Ok(());
    };
    if machine_id.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "source_machine_id must not be empty".to_string(),
        ));
    }
    if state.db.get_research_machine(machine_id).await?.is_none() {
        return Err(DashboardError::NotFound(format!(
            "research machine '{machine_id}' not found"
        )));
    }
    Ok(())
}

/// Require an admin role for research mutations.
fn require_admin(claims: &Claims) -> Result<(), DashboardError> {
    if claims.role != "admin" {
        return Err(DashboardError::Forbidden("admin role required".to_string()));
    }
    Ok(())
}

/// Resolve an import path under the configured research work root.
fn resolve_research_path(work_root: &str, requested: &str) -> Result<PathBuf, DashboardError> {
    if requested.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact_root must not be empty".to_string(),
        ));
    }
    let root = normalize_path(&PathBuf::from(work_root))?;
    let candidate = if PathBuf::from(requested).is_absolute() {
        normalize_path(&PathBuf::from(requested))?
    } else {
        normalize_path(&root.join(requested))?
    };
    if !candidate.starts_with(&root) {
        return Err(DashboardError::BadRequest(format!(
            "artifact_root escapes configured research work root: {}",
            path_to_string(&candidate)
        )));
    }
    Ok(candidate)
}

/// Normalize an absolute remote artifact root path without requiring local access.
fn normalize_remote_artifact_root(requested: &str) -> Result<String, DashboardError> {
    if requested.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact_root must not be empty".to_string(),
        ));
    }
    let path = PathBuf::from(requested);
    if !path.is_absolute() {
        return Err(DashboardError::BadRequest(
            "remote artifact_root must be an absolute source-machine path".to_string(),
        ));
    }
    Ok(path_to_string(&normalize_path(&path)?))
}

/// Normalize a path lexically and reject parent traversal.
fn normalize_path(path: &std::path::Path) -> Result<PathBuf, DashboardError> {
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

/// Convert a path to a display string for API storage and errors.
fn path_to_string(path: &std::path::Path) -> String {
    path.to_string_lossy().to_string()
}

/// Validate a manifest before trusting it as remote metadata.
fn validate_artifact_manifest(
    manifest: &research_artifacts::ArtifactManifest,
) -> Result<(), DashboardError> {
    validate_artifact_id(&manifest.artifact_id)?;
    if manifest.schema_version != 1 {
        return Err(DashboardError::BadRequest(
            "artifact manifest schema_version must be 1".to_string(),
        ));
    }
    if manifest.kind.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact kind must not be empty".to_string(),
        ));
    }
    if manifest.files.is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact manifest must include at least one file".to_string(),
        ));
    }
    for file in &manifest.files {
        if file.logical_name.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "artifact logical_name must not be empty".to_string(),
            ));
        }
        if file.kind.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "artifact file kind must not be empty".to_string(),
            ));
        }
        research_artifacts::normalize_relative_path(&file.relative_path)?;
        validate_sha256(&file.sha256)?;
    }
    Ok(())
}

/// Validate an artifact ID that will later be used as a transfer directory.
fn validate_artifact_id(artifact_id: &str) -> Result<(), DashboardError> {
    let normalized = research_artifacts::normalize_relative_path(artifact_id)?;
    if normalized != artifact_id || normalized.contains('/') || normalized.contains('\\') {
        return Err(DashboardError::BadRequest(format!(
            "artifact_id is not a safe artifact directory name: {artifact_id}"
        )));
    }
    Ok(())
}

/// Validate one lowercase SHA-256 digest string.
fn validate_sha256(value: &str) -> Result<(), DashboardError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(DashboardError::BadRequest(
            "artifact file sha256 must be 64 lowercase hexadecimal characters".to_string(),
        ))
    }
}

/// Reject conflicting source-machine declarations.
fn ensure_manifest_source_matches(
    override_source: Option<&str>,
    manifest: &research_artifacts::ArtifactManifest,
) -> Result<(), DashboardError> {
    if let (Some(override_source), Some(manifest_source)) =
        (override_source, manifest.source_machine_id.as_deref())
        && override_source != manifest_source
    {
        return Err(DashboardError::BadRequest(format!(
            "source_machine_id '{override_source}' does not match manifest source '{manifest_source}'"
        )));
    }
    Ok(())
}

/// Return the manifest runtime DB relative path, when present.
fn runtime_db_relative_path(manifest: &research_artifacts::ArtifactManifest) -> Option<&str> {
    manifest
        .files
        .iter()
        .find(|file| file.logical_name == "runtime_db")
        .or_else(|| manifest.files.iter().find(|file| file.kind == "sqlite"))
        .map(|file| file.relative_path.as_str())
}

/// Return the checksum that best represents the artifact runtime payload.
fn artifact_checksum(manifest: &research_artifacts::ArtifactManifest) -> Option<&str> {
    manifest
        .files
        .iter()
        .find(|file| file.logical_name == "runtime_db")
        .or_else(|| manifest.files.iter().find(|file| file.kind == "sqlite"))
        .map(|file| file.sha256.as_str())
        .or_else(|| manifest.files.first().map(|file| file.sha256.as_str()))
}

/// Sum manifest payload bytes.
fn manifest_payload_bytes(manifest: &research_artifacts::ArtifactManifest) -> u64 {
    manifest
        .files
        .iter()
        .map(|file| file.bytes)
        .fold(0_u64, u64::saturating_add)
}

/// Join a normalized remote root and safe artifact-relative path as text.
fn remote_child_path(root: &str, relative_path: &str) -> Result<String, DashboardError> {
    let relative_path = research_artifacts::normalize_relative_path(relative_path)?;
    Ok(format!("{}/{}", root.trim_end_matches('/'), relative_path))
}

/// Default worker heartbeat status.
fn default_worker_status() -> String {
    "online".to_string()
}

/// Require the configured research worker token on machine endpoints.
fn require_worker_token(state: &AppState, headers: &HeaderMap) -> Result<(), DashboardError> {
    let Some(expected) = state.research_worker_token.as_deref() else {
        return Err(DashboardError::Unauthorized(
            "research worker token is not configured".to_string(),
        ));
    };
    let presented = worker_token_from_headers(headers)
        .ok_or_else(|| DashboardError::Unauthorized("missing research worker token".to_string()))?;
    if !constant_time_eq(expected, &presented) {
        return Err(DashboardError::Unauthorized(
            "invalid research worker token".to_string(),
        ));
    }
    Ok(())
}

/// Extract a worker token from either the worker header or a bearer header.
fn worker_token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-buba-research-worker-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

/// Compare short secrets without early return on byte mismatches.
fn constant_time_eq(expected: &str, presented: &str) -> bool {
    let expected = expected.as_bytes();
    let presented = presented.as_bytes();
    let mut diff = expected.len() ^ presented.len();
    let max_len = expected.len().max(presented.len());
    for index in 0..max_len {
        let left = expected.get(index).copied().unwrap_or_default();
        let right = presented.get(index).copied().unwrap_or_default();
        diff |= usize::from(left ^ right);
    }
    diff == 0
}

#[cfg(test)]
#[path = "../tests/api_research_tests.rs"]
mod tests;
