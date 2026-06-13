//! Research machine inventory, health, telemetry, and worker heartbeat handlers.

use super::{
    AppState, Claims, CreateMachineRequest, DashboardError, Extension, HeaderMap, IntoResponse,
    Json, MachineHealthResponse, MachineTelemetryQuery, MachineTelemetryResponse, MachinesResponse,
    NullableUpdate, Path, Query, ResearchMachineHeartbeatRecord, ResearchMachineRecord,
    ResearchMachineTelemetryUpdate, State, UpdateMachineRequest, WorkerHeartbeatRequest,
    WorkerHeartbeatResponse, current_epoch_ms, json_value_to_string, parse_stored_json,
    require_admin, require_worker_token, research_machine_by_id, telemetry_stale_after_ms,
};

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
    Ok(Json(machine))
}

/// `GET /api/research/machines/:id`
pub async fn get_machine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let machine = research_machine_by_id(&state, &id).await?;
    Ok(Json(machine))
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
    Ok(Json(machine))
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
    Ok(Json(machine))
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
    Ok(Json(machine))
}

/// `DELETE /api/research/machines/:id`
pub async fn delete_machine(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    require_admin(&claims)?;
    let machine = state.db.delete_research_machine(&id).await?;
    Ok(Json(machine))
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

/// `GET /api/research/machines/:id/telemetry`
pub async fn get_machine_telemetry(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<MachineTelemetryQuery>,
) -> Result<impl IntoResponse, DashboardError> {
    let machine = research_machine_by_id(&state, &id).await?;
    let dependencies = state.db.research_machine_dependency_counts(&id).await?;
    let telemetry = state
        .db
        .get_research_machine_telemetry(&id, query.limit, query.since_ms)
        .await?;
    let stale_after_ms = telemetry_stale_after_ms(telemetry.state.as_ref());
    let stale = telemetry.state.as_ref().is_none_or(|state| {
        current_epoch_ms().saturating_sub(state.last_heartbeat_ms) > stale_after_ms
    });
    let disabled = machine.status == "disabled";
    Ok(Json(MachineTelemetryResponse {
        machine,
        telemetry: telemetry.state,
        samples: telemetry.samples,
        dependencies,
        disabled,
        stale,
        stale_after_ms,
    }))
}

/// `POST /api/research/workers/heartbeat`
pub async fn worker_heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WorkerHeartbeatRequest>,
) -> Result<impl IntoResponse, DashboardError> {
    require_worker_token(&state, &headers)?;
    let telemetry = ResearchMachineTelemetryUpdate {
        host: req.host.as_ref(),
        sampler: req.sampler.as_ref(),
        samples: &req.samples,
        activity: req.activity.as_ref(),
    };
    let record = ResearchMachineHeartbeatRecord {
        machine_id: &req.machine_id,
        worker_id: &req.worker_id,
        worker_version: req.worker_version.as_deref(),
        status: &req.status,
        details: req.details.as_ref(),
        telemetry,
    };
    let machine = state
        .db
        .record_research_machine_heartbeat_with_telemetry(&record)
        .await?;
    Ok(Json(WorkerHeartbeatResponse { machine }))
}
