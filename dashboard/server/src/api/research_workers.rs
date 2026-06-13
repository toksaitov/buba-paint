//! Worker-token endpoints that expose the research queue to remote workers.
//!
//! These routes mirror the local `DashboardDb` worker surface one-to-one so
//! `ResearchControllerClient` can implement `ResearchWorkBackend` against a
//! public controller. Every handler requires the shared research worker
//! token; none of them use operator JWT auth.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::fs;

use crate::api::auth_routes::AppState;
use crate::api::research::require_worker_token;
use crate::error::DashboardError;
use crate::research_controller_client::{
    ClaimStepRequest, ClaimTransferRequest, RecoverTransfersRequest, RecoverTransfersResponse,
    StepLeaseRequest, WorkerArtifactDocumentsRequest, WorkerArtifactUpsertRequest,
    WorkerEventRequest, WorkerReportDocumentsRequest, WorkerReportUpsertRequest,
    WorkerTransferProgressRequest,
};

/// Wrap an optional payload as JSON-or-204 so clients get a uniform shape.
fn optional_json<T: serde::Serialize>(value: Option<T>) -> Response {
    match value {
        Some(value) => Json(value).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

/// Reject path ids that could escape the controller work root.
fn require_safe_document_id(id: &str) -> Result<(), DashboardError> {
    let safe = !id.is_empty()
        && id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.'
        })
        && !id.starts_with('.')
        && !id.contains("..");
    if safe {
        Ok(())
    } else {
        Err(DashboardError::BadRequest(format!(
            "id '{id}' is not a safe document directory name"
        )))
    }
}

/// Resolve the configured research work root or reject the request.
fn require_work_root(state: &AppState) -> Result<String, DashboardError> {
    state.research_work_root.clone().ok_or_else(|| {
        DashboardError::BadRequest(
            "BUBA_RESEARCH_WORK_ROOT is not configured on this controller".to_string(),
        )
    })
}

/// Write one document under the controller work root and return its path.
async fn write_work_document(
    work_root: &str,
    sub_dir: &str,
    id: &str,
    file_name: &str,
    contents: &str,
) -> Result<String, DashboardError> {
    let dir = std::path::Path::new(work_root).join(sub_dir).join(id);
    fs::create_dir_all(&dir).await.map_err(|error| {
        DashboardError::Internal(format!("creating document directory: {error}"))
    })?;
    let path = dir.join(file_name);
    fs::write(&path, contents)
        .await
        .map_err(|error| DashboardError::Internal(format!("writing document: {error}")))?;
    Ok(path.to_string_lossy().to_string())
}

/// `POST /api/research/workers/steps/claim`
pub async fn claim_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ClaimStepRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let lease = state
        .db
        .lease_next_research_step(&req.worker_id, req.lease_duration_ms)
        .await?;
    Ok(optional_json(lease))
}

/// `POST /api/research/workers/steps/:id/renew`
pub async fn renew_step_lease(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<StepLeaseRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let lease_duration_ms = req.lease_duration_ms.ok_or_else(|| {
        DashboardError::BadRequest("lease_duration_ms is required for renew".to_string())
    })?;
    let step = state
        .db
        .refresh_research_step_lease(&id, &req.worker_id, lease_duration_ms)
        .await?;
    Ok(Json(step))
}

/// `POST /api/research/workers/steps/:id/run`
pub async fn mark_step_running(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<StepLeaseRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let step = state
        .db
        .mark_research_step_running(&id, &req.worker_id)
        .await?;
    Ok(Json(step))
}

/// `POST /api/research/workers/steps/:id/complete`
pub async fn complete_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<StepLeaseRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let step = state
        .db
        .complete_research_step(&id, &req.worker_id, req.output_json.as_deref())
        .await?;
    Ok(Json(step))
}

/// `POST /api/research/workers/steps/:id/fail`
pub async fn fail_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<StepLeaseRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let error = req
        .error
        .as_deref()
        .ok_or_else(|| DashboardError::BadRequest("error text is required".to_string()))?;
    let step = state
        .db
        .fail_research_step(&id, &req.worker_id, error, req.retryable.unwrap_or(false))
        .await?;
    Ok(Json(step))
}

/// `POST /api/research/workers/steps/:id/block`
pub async fn block_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<StepLeaseRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let reason = req
        .error
        .as_deref()
        .ok_or_else(|| DashboardError::BadRequest("block reason is required".to_string()))?;
    let step = state
        .db
        .block_research_step(&id, &req.worker_id, reason)
        .await?;
    Ok(Json(step))
}

/// `GET /api/research/workers/jobs/:id`
pub async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let job = state.db.get_research_job(&id).await?;
    Ok(optional_json(job))
}

/// `POST /api/research/workers/jobs/:id/cancel`
pub async fn cancel_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let job = state.db.cancel_research_job(&id).await?;
    Ok(Json(job))
}

/// `GET /api/research/workers/jobs/:id/steps`
pub async fn get_job_steps(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let steps = state.db.get_research_job_steps(&id).await?;
    Ok(Json(steps))
}

/// `POST /api/research/workers/jobs/:id/events`
pub async fn append_job_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<WorkerEventRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let event = state
        .db
        .append_research_job_event(
            &id,
            req.step_id.as_deref(),
            &req.level,
            &req.message,
            req.details_json.as_deref(),
        )
        .await?;
    Ok(Json(event))
}

/// `POST /api/research/workers/jobs/:job_id/artifact/:artifact_id`
pub async fn attach_job_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((job_id, artifact_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let job = state
        .db
        .attach_research_job_artifact(&job_id, &artifact_id)
        .await?;
    Ok(Json(job))
}

/// `GET /api/research/workers/artifacts/:id`
pub async fn get_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let artifact = state.db.get_research_artifact(&id).await?;
    Ok(optional_json(artifact))
}

/// `POST /api/research/workers/artifacts`
pub async fn upsert_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WorkerArtifactUpsertRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    require_safe_document_id(&req.id)?;
    let artifact = state.db.upsert_research_artifact(&req.as_record()).await?;
    Ok(Json(artifact))
}

/// `PUT /api/research/workers/artifacts/:id/documents`
///
/// Document bodies are stored verbatim; only the id is validated for path safety.
pub async fn store_artifact_documents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<WorkerArtifactDocumentsRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    require_safe_document_id(&id)?;
    if req.manifest_json.is_none() && req.checksums_text.is_none() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    let work_root = require_work_root(&state)?;
    let artifact = state
        .db
        .get_research_artifact(&id)
        .await?
        .ok_or_else(|| DashboardError::NotFound(format!("artifact '{id}' not found")))?;
    let mut manifest_path = artifact.manifest_path.clone();
    if let Some(manifest_json) = req.manifest_json.as_deref() {
        let path =
            write_work_document(&work_root, "artifacts", &id, "manifest.json", manifest_json)
                .await?;
        manifest_path = Some(path);
    }
    if let Some(checksums) = req.checksums_text.as_deref() {
        write_work_document(&work_root, "artifacts", &id, "checksums.sha256", checksums).await?;
    }
    let artifact_root = std::path::Path::new(&work_root)
        .join("artifacts")
        .join(&id)
        .to_string_lossy()
        .to_string();
    let mut record = artifact.to_record();
    record.artifact_root = Some(&artifact_root);
    record.manifest_path = manifest_path.as_deref();
    let updated = state.db.upsert_research_artifact(&record).await?;
    Ok(Json(updated).into_response())
}

/// `POST /api/research/workers/reports`
pub async fn upsert_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WorkerReportUpsertRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    if let Some(artifact_id) = req.artifact_id.as_deref() {
        require_safe_document_id(artifact_id)?;
    }
    let report = state
        .db
        .create_or_update_research_report(&req.as_record())
        .await?;
    Ok(Json(report))
}

/// `PUT /api/research/workers/reports/:id/documents`
///
/// Document bodies are stored verbatim; only the id is validated for path safety.
pub async fn store_report_documents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<WorkerReportDocumentsRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    require_safe_document_id(&id)?;
    let work_root = require_work_root(&state)?;
    let report_path =
        write_work_document(&work_root, "reports", &id, "report.json", &req.report_json).await?;
    let csv_path =
        write_work_document(&work_root, "reports", &id, "report.csv", &req.report_csv).await?;
    let report = state
        .db
        .update_research_report_paths(&id, &report_path, &csv_path)
        .await?;
    Ok(Json(report))
}

/// `POST /api/research/workers/transfers/claim`
pub async fn claim_transfer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ClaimTransferRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let transfer = state
        .db
        .claim_next_artifact_transfer(&req.dest_machine_id)
        .await?;
    Ok(optional_json(transfer))
}

/// `GET /api/research/workers/transfers/:id`
pub async fn get_transfer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let transfer = state.db.get_artifact_transfer(&id).await?;
    Ok(optional_json(transfer))
}

/// `POST /api/research/workers/transfers/:id/progress`
pub async fn update_transfer_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<WorkerTransferProgressRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
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

/// `POST /api/research/workers/transfers/recover`
pub async fn recover_stale_transfers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RecoverTransfersRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let recovered = state
        .db
        .recover_stale_artifact_transfers(&req.dest_machine_id, req.stale_after_ms)
        .await?;
    Ok(Json(RecoverTransfersResponse { recovered }))
}

/// `GET /api/research/workers/machines/:id`
pub async fn get_machine(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let machine = state.db.get_research_machine(&id).await?;
    Ok(optional_json(machine))
}

#[cfg(test)]
#[path = "../tests/research_workers_api_tests.rs"]
mod tests;
