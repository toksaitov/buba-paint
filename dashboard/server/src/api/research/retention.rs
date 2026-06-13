//! Research retention snapshot and bulk archive-only cleanup handlers.

use super::{
    AppState, Claims, DashboardError, Extension, IntoResponse, Json, RetentionArchiveRequest,
    RetentionArchiveResponse, State, archive_retention_artifact, archive_retention_job,
    archive_retention_report, build_research_retention_response, require_admin,
};

/// `GET /api/research/retention`
pub async fn get_retention(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, DashboardError> {
    let response = build_research_retention_response(&state).await?;
    Ok(Json(response))
}

/// `POST /api/research/retention/archive`
pub async fn archive_retention(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RetentionArchiveRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let mut job_results = Vec::new();
    let mut report_results = Vec::new();
    let mut artifact_results = Vec::new();

    for job_id in req.job_ids {
        job_results.push(archive_retention_job(&state, &job_id).await);
    }
    for report_id in req.report_ids {
        report_results.push(archive_retention_report(&state, &report_id).await);
    }
    for artifact_id in req.artifact_ids {
        artifact_results.push(archive_retention_artifact(&state, &artifact_id).await);
    }

    let totals = build_research_retention_response(&state).await?.totals;
    Ok(Json(RetentionArchiveResponse {
        jobs: job_results,
        reports: report_results,
        artifacts: artifact_results,
        totals,
    }))
}
