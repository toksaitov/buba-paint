//! Research home queue cockpit handler.

use super::{AppState, DashboardError, IntoResponse, Json, State, build_research_queue_response};

/// `GET /api/research/queue`
pub async fn get_queue(State(state): State<AppState>) -> Result<impl IntoResponse, DashboardError> {
    let response = build_research_queue_response(&state).await?;
    Ok(Json(response))
}
