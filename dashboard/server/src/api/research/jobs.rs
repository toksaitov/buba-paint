//! Research job, job template, step, event, and report-regeneration handlers.

use super::{
    AppState, AppendEventRequest, Claims, CloneJobRequest, CreateJobRequest, DashboardError,
    EventsResponse, Extension, IntoResponse, JobTemplatesResponse, JobsResponse, Json,
    NullableUpdate, Path, RegenerateReportResponse, ResearchJobTemplateRecord, State,
    UpdateJobRequest, UpsertJobTemplateRequest, archive_job_scratch_for_id, display_user_name,
    ensure_job_exists, humanize_job_audit, humanize_template_audit, job_detail,
    job_type_supports_report_regeneration, nullable_string_update_as_deref, path_to_string,
    pipeline_for_archive, report_analysis_source_exists, report_status_allows_regeneration,
    require_admin, research_artifact_for_job_id, research_job_template_by_id,
    resolve_regenerated_report_paths, serialize_optional_json_update, template_params_json,
    write_regenerated_report_files, write_report_files,
};

/// `GET /api/research/jobs`
pub async fn list_jobs(State(state): State<AppState>) -> Result<impl IntoResponse, DashboardError> {
    let mut jobs = state.db.list_research_jobs().await?;
    humanize_job_audit(&state, &mut jobs).await;
    Ok(Json(JobsResponse { jobs }))
}

/// `GET /api/research/job-templates`
pub async fn list_job_templates(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, DashboardError> {
    let mut templates = state.db.list_research_job_templates().await?;
    humanize_template_audit(&state, &mut templates).await;
    Ok(Json(JobTemplatesResponse { templates }))
}

/// `POST /api/research/job-templates`
pub async fn create_job_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<UpsertJobTemplateRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let params_json = template_params_json(&req.params)?;
    let mut template = state
        .db
        .create_research_job_template(&ResearchJobTemplateRecord {
            name: &req.name,
            description: req.description.as_deref(),
            job_type: &req.job_type,
            artifact_id: req.artifact_id.as_deref(),
            priority: req.priority,
            params_json: &params_json,
            operator_id: &claims.sub,
        })
        .await?;
    template.created_by = display_user_name(&state, &template.created_by).await;
    Ok(Json(template))
}

/// `GET /api/research/job-templates/:id`
pub async fn get_job_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let mut template = research_job_template_by_id(&state, &id).await?;
    template.created_by = display_user_name(&state, &template.created_by).await;
    Ok(Json(template))
}

/// `PATCH /api/research/job-templates/:id`
pub async fn update_job_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<UpsertJobTemplateRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let params_json = template_params_json(&req.params)?;
    let mut template = state
        .db
        .update_research_job_template(
            &id,
            &ResearchJobTemplateRecord {
                name: &req.name,
                description: req.description.as_deref(),
                job_type: &req.job_type,
                artifact_id: req.artifact_id.as_deref(),
                priority: req.priority,
                params_json: &params_json,
                operator_id: &claims.sub,
            },
        )
        .await?;
    template.created_by = display_user_name(&state, &template.created_by).await;
    Ok(Json(template))
}

/// `POST /api/research/job-templates/:id/archive`
pub async fn archive_job_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let mut template = state.db.archive_research_job_template(&id).await?;
    template.created_by = display_user_name(&state, &template.created_by).await;
    Ok(Json(template))
}

/// `POST /api/research/job-templates/:id/restore`
pub async fn restore_job_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let mut template = state.db.restore_research_job_template(&id).await?;
    template.created_by = display_user_name(&state, &template.created_by).await;
    Ok(Json(template))
}

/// `DELETE /api/research/job-templates/:id`
pub async fn delete_job_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let template = state.db.delete_research_job_template(&id).await?;
    Ok(Json(template))
}

/// `POST /api/research/jobs`
pub async fn create_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let template = if let Some(template_id) = req.template_id.as_deref() {
        let template = research_job_template_by_id(&state, template_id).await?;
        if template.status != "active" {
            return Err(DashboardError::BadRequest(format!(
                "research job template '{}' is archived",
                template.id
            )));
        }
        if template.job_type != req.job_type {
            return Err(DashboardError::BadRequest(format!(
                "research job template '{}' is for '{}' jobs, not '{}'",
                template.id, template.job_type, req.job_type
            )));
        }
        Some(template)
    } else {
        None
    };
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
    if let Some(template) = template {
        state
            .db
            .record_research_job_template_use(&template.id)
            .await?;
        let details = serde_json::json!({
            "template_id": template.id,
            "template_name": template.name,
        });
        state
            .db
            .append_research_job_event(
                &job.id,
                None,
                "info",
                "created from research job template",
                Some(&details.to_string()),
            )
            .await?;
    }
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
///
/// Alias of `resume_job`; the verb split is operator-facing wording only and
/// both paths drive the same `resume_research_job` status dispatch.
pub async fn continue_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    resume_job(State(state), Extension(claims), Path(id)).await
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
    if existing.is_none() && !report_status_allows_regeneration(&detail.job.status) {
        return Err(DashboardError::BadRequest(format!(
            "research job '{}' is not ready for report regeneration from status '{}'",
            detail.job.id, detail.job.status
        )));
    }
    let report_paths = resolve_regenerated_report_paths(&state, &detail.job.id, existing.as_ref())?;
    let summary_json = if job_type_supports_report_regeneration(&detail.job.job_type) {
        let artifact =
            research_artifact_for_job_id(&*state.db, detail.job.artifact_id.as_deref()).await?;
        let pipeline = pipeline_for_archive(&state)?;
        let mut plan = pipeline.plan_for_job(&detail.job, artifact.as_ref())?;
        plan.report_json_path.clone_from(&report_paths.report_path);
        plan.report_csv_path.clone_from(&report_paths.csv_path);
        if report_analysis_source_exists(&plan) {
            write_report_files(&plan, &detail.steps)?
        } else {
            write_regenerated_report_files(&detail, existing.as_ref(), &report_paths)?
        }
    } else {
        write_regenerated_report_files(&detail, existing.as_ref(), &report_paths)?
    };
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

/// `POST /api/research/jobs/:id/archive-scratch`
pub async fn archive_job_scratch(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let response = archive_job_scratch_for_id(&state, &id).await?;
    Ok(Json(response))
}
