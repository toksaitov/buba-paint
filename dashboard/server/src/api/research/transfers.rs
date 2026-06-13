//! Artifact transfer lifecycle handlers for moving artifacts between machines.

use super::{
    AppState, ArtifactTransferRecord, Claims, CreateTransferRequest, DashboardError, Extension,
    IntoResponse, Json, Path, RetryTransferRequest, State, TransferProgressRequest,
    TransfersResponse, VerifyTransferResponse, require_admin, research_artifacts,
    resolve_research_path,
};

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
    if artifact.status != "available" {
        return Err(DashboardError::BadRequest(format!(
            "artifact '{}' must be available before it can be transferred",
            artifact.id
        )));
    }
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
