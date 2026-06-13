//! Research artifact metadata, verification, import, and registration handlers.

use super::{
    AppState, ArtifactManifestSummary, ArtifactsResponse, Claims, DashboardError,
    DeleteArtifactQuery, Extension, ImportArtifactRequest, ImportArtifactResponse, IntoResponse,
    Json, Path, Query, RegisterArtifactRequest, RegisterArtifactResponse, ResearchArtifactRecord,
    State, UpdateArtifactRequest, VerifyArtifactResponse, artifact_checksum, delete_artifact_files,
    ensure_manifest_source_matches, ensure_source_machine, header, manifest_payload_bytes,
    normalize_remote_artifact_root, path_to_string, remote_child_path, require_admin,
    research_artifact_by_id, research_artifacts, resolve_artifact_root_path, resolve_research_path,
    runtime_db_relative_path, validate_artifact_manifest, validate_optional_metadata,
};

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
    let mut record = current.to_record();
    record.source_machine_id = source_machine_id;
    record.run_mode = run_mode;
    record.replay_quality_class = replay_quality_class;
    record.backtest_ready_class = backtest_ready_class;
    record.live_fidelity_class = live_fidelity_class;
    let artifact = state.db.upsert_research_artifact(&record).await?;
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
