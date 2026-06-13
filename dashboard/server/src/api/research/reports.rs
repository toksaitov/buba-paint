//! Research report metadata, archive lifecycle, and file rendering handlers.

use super::{
    AppState, Claims, DashboardError, DeleteReportQuery, Extension, IntoResponse, Json, Path,
    Query, ReportsResponse, State, UpdateReportRequest, delete_report_files, header,
    read_report_file, require_admin, research_report_by_id,
};

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
    let value = serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
        DashboardError::BadRequest(format!(
            "report JSON file is corrupt for report '{id}': {e}"
        ))
    })?;
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
