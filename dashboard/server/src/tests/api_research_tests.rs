use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::{get, post};
use rusqlite::Connection;
use tower::ServiceExt;

use crate::api::auth_routes::AppState;
use crate::api::research;
use crate::auth::{self, AuthState, hash_password};
use crate::db::DashboardDb;
use crate::research_artifacts::{ArtifactFileSpec, build_manifest, write_manifest_files};

/// Build a test app with research routes.
fn test_app() -> (Router, Arc<DashboardDb>) {
    let db = Arc::new(DashboardDb::from_connection(
        Connection::open_in_memory().unwrap(),
    ));

    let state = AppState {
        db: Arc::clone(&db),
        jwt_secret: "test-jwt-secret".to_string(),
        research_worker_token: Some("test-worker-token".to_string()),
        research_work_root: Some(std::env::temp_dir().to_string_lossy().to_string()),
        agents: vec![],
    };

    let auth_state = AuthState {
        jwt_secret: "test-jwt-secret".to_string(),
        db: Arc::clone(&db),
    };

    let operator_routes = test_research_routes()
        .layer(middleware::from_fn(auth::require_auth))
        .layer(axum::Extension(auth_state));
    let worker_routes = test_worker_routes().layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_worker_auth,
    ));
    let app = operator_routes.merge(worker_routes).with_state(state);

    (app, db)
}

/// Build worker-token routes used by API tests.
fn test_worker_routes() -> Router<AppState> {
    Router::new().route(
        "/api/research/workers/heartbeat",
        post(research::worker_heartbeat),
    )
}

/// Build all research routes used by API tests.
fn test_research_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/machines",
            get(research::list_machines).post(research::create_machine),
        )
        .route(
            "/api/research/machines/{id}",
            get(research::get_machine)
                .patch(research::update_machine)
                .delete(research::delete_machine),
        )
        .route(
            "/api/research/machines/{id}/disable",
            post(research::disable_machine),
        )
        .route(
            "/api/research/machines/{id}/enable",
            post(research::enable_machine),
        )
        .route(
            "/api/research/machines/{id}/health",
            get(research::get_machine_health),
        )
        .route(
            "/api/research/machines/{id}/telemetry",
            get(research::get_machine_telemetry),
        )
        .merge(test_artifact_routes())
        .merge(test_transfer_routes())
        .merge(test_job_routes())
        .merge(test_template_routes())
        .route("/api/research/queue", get(research::get_queue))
        .route("/api/research/retention", get(research::get_retention))
        .route(
            "/api/research/retention/archive",
            post(research::archive_retention),
        )
        .merge(test_report_routes())
}

/// Build artifact routes used by API tests.
fn test_artifact_routes() -> Router<AppState> {
    Router::new()
        .route("/api/research/artifacts", get(research::list_artifacts))
        .route(
            "/api/research/artifacts/import",
            post(research::import_artifact),
        )
        .route(
            "/api/research/artifacts/register",
            post(research::register_artifact),
        )
        .route(
            "/api/research/artifacts/{id}",
            get(research::get_artifact)
                .patch(research::update_artifact)
                .delete(research::delete_artifact),
        )
        .route(
            "/api/research/artifacts/{id}/verify",
            post(research::verify_artifact),
        )
        .route(
            "/api/research/artifacts/{id}/archive",
            post(research::archive_artifact),
        )
        .route(
            "/api/research/artifacts/{id}/restore",
            post(research::restore_artifact),
        )
        .route(
            "/api/research/artifacts/{id}/manifest",
            get(research::get_artifact_manifest),
        )
        .route(
            "/api/research/artifacts/{id}/checksums",
            get(research::get_artifact_checksums),
        )
}

/// Build transfer routes used by API tests.
fn test_transfer_routes() -> Router<AppState> {
    Router::new()
        .route("/api/research/transfers", get(research::list_transfers))
        .route("/api/research/transfers", post(research::create_transfer))
        .route(
            "/api/research/transfers/{id}",
            get(research::get_transfer).delete(research::delete_transfer),
        )
        .route(
            "/api/research/transfers/{id}/progress",
            post(research::update_transfer_progress),
        )
        .route(
            "/api/research/transfers/{id}/cancel",
            post(research::cancel_transfer),
        )
        .route(
            "/api/research/transfers/{id}/pause",
            post(research::pause_transfer),
        )
        .route(
            "/api/research/transfers/{id}/resume",
            post(research::resume_transfer),
        )
        .route(
            "/api/research/transfers/{id}/retry",
            post(research::retry_transfer),
        )
        .route(
            "/api/research/transfers/{id}/verify",
            post(research::verify_transfer),
        )
}

/// Build job routes used by API tests.
fn test_job_routes() -> Router<AppState> {
    Router::new()
        .route("/api/research/jobs", get(research::list_jobs))
        .route("/api/research/jobs", post(research::create_job))
        .route(
            "/api/research/jobs/{id}",
            get(research::get_job)
                .patch(research::update_job)
                .delete(research::delete_job),
        )
        .route("/api/research/jobs/{id}/cancel", post(research::cancel_job))
        .route("/api/research/jobs/{id}/pause", post(research::pause_job))
        .route("/api/research/jobs/{id}/resume", post(research::resume_job))
        .route(
            "/api/research/jobs/{id}/continue",
            post(research::continue_job),
        )
        .route("/api/research/jobs/{id}/retry", post(research::retry_job))
        .route("/api/research/jobs/{id}/clone", post(research::clone_job))
        .route(
            "/api/research/jobs/{id}/report/regenerate",
            post(research::regenerate_job_report),
        )
        .route(
            "/api/research/jobs/{id}/archive-scratch",
            post(research::archive_job_scratch),
        )
        .route(
            "/api/research/jobs/{job_id}/steps/{step_id}/retry",
            post(research::retry_step),
        )
        .route(
            "/api/research/jobs/{job_id}/steps/{step_id}/cancel",
            post(research::cancel_step),
        )
        .route(
            "/api/research/jobs/{job_id}/steps/{step_id}/clear-lease",
            post(research::clear_step_lease),
        )
        .route(
            "/api/research/jobs/{job_id}/steps/{step_id}/resolve-blocker",
            post(research::resolve_step_blocker),
        )
        .route(
            "/api/research/jobs/{id}/events",
            get(research::list_job_events),
        )
        .route(
            "/api/research/jobs/{id}/events",
            post(research::append_job_event),
        )
}

/// Build reusable job template routes used by API tests.
fn test_template_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/job-templates",
            get(research::list_job_templates).post(research::create_job_template),
        )
        .route(
            "/api/research/job-templates/{id}",
            get(research::get_job_template)
                .patch(research::update_job_template)
                .delete(research::delete_job_template),
        )
        .route(
            "/api/research/job-templates/{id}/archive",
            post(research::archive_job_template),
        )
        .route(
            "/api/research/job-templates/{id}/restore",
            post(research::restore_job_template),
        )
}

/// Build report routes used by API tests.
fn test_report_routes() -> Router<AppState> {
    Router::new()
        .route("/api/research/reports", get(research::list_reports))
        .route(
            "/api/research/reports/{id}",
            get(research::get_report)
                .patch(research::update_report)
                .delete(research::delete_report),
        )
        .route(
            "/api/research/reports/{id}/archive",
            post(research::archive_report),
        )
        .route(
            "/api/research/reports/{id}/restore",
            post(research::restore_report),
        )
        .route(
            "/api/research/reports/{id}/json",
            get(research::get_report_json_file),
        )
        .route(
            "/api/research/reports/{id}/csv",
            get(research::get_report_csv_file),
        )
}

/// Create an admin token.
async fn admin_token(db: &DashboardDb) -> String {
    let hash = hash_password("pass").unwrap();
    db.create_user("admin", &hash, "admin").await.unwrap();
    let user = db.get_user_by_username("admin").await.unwrap().unwrap();
    auth::create_jwt(&user.id, "admin", "test-jwt-secret", 3600)
}

/// Create an observer token.
async fn observer_token(db: &DashboardDb) -> String {
    let hash = hash_password("pass").unwrap();
    db.create_user("observer", &hash, "observer").await.unwrap();
    let user = db.get_user_by_username("observer").await.unwrap().unwrap();
    auth::create_jwt(&user.id, "observer", "test-jwt-secret", 3600)
}

/// Build an authenticated get request.
fn auth_get(path: &str, token: &str) -> Request<Body> {
    Request::get(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Build an authenticated empty post request.
fn auth_post(path: &str, token: &str) -> Request<Body> {
    Request::post(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Build an authenticated delete request.
fn auth_delete(path: &str, token: &str) -> Request<Body> {
    Request::delete(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Build an authenticated JSON post request.
fn auth_json_post(path: &str, token: &str, body: &serde_json::Value) -> Request<Body> {
    Request::post(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Build an authenticated JSON patch request.
fn auth_json_patch(path: &str, token: &str, body: &serde_json::Value) -> Request<Body> {
    Request::patch(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Build a worker-authenticated JSON post request.
fn worker_json_post(path: &str, token: Option<&str>, body: &serde_json::Value) -> Request<Body> {
    let mut builder = Request::post(path).header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("x-buba-research-worker-token", token);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

/// Parse a JSON response body.
async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Build a deterministic telemetry sample JSON object.
fn telemetry_sample_json(sampled_at_ms: i64) -> serde_json::Value {
    serde_json::json!({
        "sampled_at_ms": sampled_at_ms,
        "cpu_percent": 12.5,
        "per_core_cpu": [12.5, 8.0],
        "load_one": 0.5,
        "load_five": 0.4,
        "load_fifteen": 0.3,
        "mem_used_bytes": 4096,
        "mem_total_bytes": 16384,
        "mem_available_bytes": 12288,
        "swap_used_bytes": 0,
        "swap_total_bytes": 0,
        "disk_used_bytes": 50000,
        "disk_total_bytes": 100000,
        "disk_mount": "/research"
    })
}

/// Build a complete worker heartbeat body with typed telemetry.
fn telemetry_heartbeat_body() -> serde_json::Value {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(10_000);
    serde_json::json!({
        "machine_id": "research",
        "worker_id": "research-worker-testing",
        "worker_version": "0.2.0",
        "status": "idle",
        "details": {"queue_depth": 0},
        "host": {
            "hostname": "testing",
            "os_name": "Linux",
            "os_version": "test",
            "kernel_version": "test",
            "cpu_count": 2,
            "total_ram_bytes": 16384
        },
        "sampler": {
            "sample_interval_ms": 5000,
            "samples_collected": 2,
            "last_error": null
        },
        "samples": [
            telemetry_sample_json(now_ms.saturating_sub(5_000)),
            telemetry_sample_json(now_ms)
        ],
        "activity": {
            "phase": "idle",
            "heartbeat_interval_ms": 30000,
            "processed_last_tick": 0,
            "transfers_processed_last_tick": 0
        }
    })
}

/// Parse a text response body.
async fn text_body(resp: axum::response::Response) -> String {
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

/// Report fixture with temporary files kept alive by the owning directory.
struct ReportFileFixture {
    /// Persisted report row.
    report: crate::db::ResearchReport,
    /// JSON report path on disk.
    report_json_path: PathBuf,
    /// CSV report path on disk.
    report_csv_path: PathBuf,
    /// Temporary directory that owns the report files.
    _report_dir: tempfile::TempDir,
}

/// Create a report row backed by readable JSON and CSV files.
async fn report_file_fixture(db: &DashboardDb) -> ReportFileFixture {
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &admin_user.id,
            0,
            None,
        )
        .await
        .unwrap();
    let report_dir = tempfile::tempdir().unwrap();
    let report_json_path = report_dir.path().join("report.json");
    let report_csv_path = report_dir.path().join("report.csv");
    std::fs::write(&report_json_path, r#"{"equity":[1,2,3]}"#).unwrap();
    std::fs::write(&report_csv_path, "step,status\nwrite_report,completed\n").unwrap();
    let report = db
        .create_or_update_research_report(&crate::db::ResearchReportRecord {
            job_id: &job.id,
            artifact_id: Some(&artifact.id),
            title: "Detail report",
            status: "available",
            summary_json: Some(r#"{"ok":true}"#),
            report_path: Some(report_json_path.to_str().unwrap()),
            csv_path: Some(report_csv_path.to_str().unwrap()),
        })
        .await
        .unwrap();
    ReportFileFixture {
        report,
        report_json_path,
        report_csv_path,
        _report_dir: report_dir,
    }
}

/// Build a manifest-backed artifact directory under the OS temp root.
fn artifact_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("paint.db"), b"db-bytes").unwrap();
    let manifest = build_manifest(
        dir.path(),
        "artifact-import-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1_000,
        Some(1_000),
        Some(2_000),
        &[ArtifactFileSpec {
            logical_name: "runtime_db".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "paint.db".to_string(),
        }],
    )
    .unwrap();
    write_manifest_files(dir.path(), &manifest).unwrap();
    dir
}

/// Import one artifact fixture through the API and return its ID.
async fn import_artifact_fixture(
    app: &Router,
    token: &str,
    artifact_dir: &tempfile::TempDir,
) -> String {
    let response = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/artifacts/import",
            token,
            &serde_json::json!({"artifact_root": artifact_dir.path().to_str().unwrap()}),
        ))
        .await
        .unwrap();
    json_body(response).await["artifact"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Complete every currently queued step for one research job.
async fn complete_all_job_steps(db: &DashboardDb, worker_id: &str) {
    let mut now = 1_000;
    while let Some(lease) = db
        .lease_next_research_step_at(worker_id, now, 5_000)
        .await
        .unwrap()
    {
        db.mark_research_step_running_at(&lease.step.id, worker_id, now + 1)
            .await
            .unwrap();
        db.complete_research_step_at(
            &lease.step.id,
            worker_id,
            Some(r#"{"status":"completed"}"#),
            now + 2,
        )
        .await
        .unwrap();
        now += 10;
    }
}

/// Verifies that observer users can read machine readiness.
#[tokio::test]
async fn observer_can_list_research_machines() {
    let (app, db) = test_app();
    let token = observer_token(&db).await;

    let resp = app
        .oneshot(auth_get("/api/research/machines", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["machines"].as_array().unwrap().len(), 2);
    assert_eq!(body["machines"][0]["id"], "live");
    assert_eq!(body["machines"][1]["status"], "not_configured");
}

/// Verifies admins can create, patch, disable, enable, delete, and health-check machines.
#[tokio::test]
async fn admin_can_manage_research_machine_lifecycle() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;

    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/machines",
            &token,
            &serde_json::json!({
                "id": "gpu-1",
                "name": "GPU Worker 1",
                "role": "research",
                "ssh_alias": "testing-gpu-1",
                "status": "configured",
                "details": {"zone": "desk"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let created = json_body(create_resp).await;
    assert_eq!(created["machine"]["id"], "gpu-1");

    let patch_resp = app
        .clone()
        .oneshot(auth_json_patch(
            "/api/research/machines/gpu-1",
            &token,
            &serde_json::json!({
                "name": "GPU Worker A",
                "ssh_alias": null,
                "status": "maintenance",
                "details": {"zone": "rack"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), StatusCode::OK);
    let patched = json_body(patch_resp).await;
    assert_eq!(patched["machine"]["name"], "GPU Worker A");
    assert_eq!(patched["machine"]["ssh_alias"], serde_json::Value::Null);
    assert_eq!(patched["machine"]["status"], "maintenance");

    let health_resp = app
        .clone()
        .oneshot(auth_get("/api/research/machines/gpu-1/health", &token))
        .await
        .unwrap();
    assert_eq!(health_resp.status(), StatusCode::OK);
    let health = json_body(health_resp).await;
    assert_eq!(health["details"]["zone"], "rack");
    assert_eq!(health["dependencies"]["artifacts"], 0);

    for (path, expected_status) in [
        ("/api/research/machines/gpu-1/disable", "disabled"),
        ("/api/research/machines/gpu-1/enable", "configured"),
    ] {
        let resp = app.clone().oneshot(auth_post(path, &token)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_body(resp).await["machine"]["status"], expected_status);
    }

    let delete_resp = app
        .clone()
        .oneshot(auth_delete("/api/research/machines/gpu-1", &token))
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::OK);
    assert_eq!(json_body(delete_resp).await["machine"]["id"], "gpu-1");

    let missing_resp = app
        .oneshot(auth_get("/api/research/machines/gpu-1", &token))
        .await
        .unwrap();
    assert_eq!(missing_resp.status(), StatusCode::NOT_FOUND);
}

/// Verifies observers cannot mutate machine lifecycle records.
#[tokio::test]
async fn observer_cannot_mutate_research_machines() {
    let (app, db) = test_app();
    let token = observer_token(&db).await;
    db.create_research_machine(&crate::db::ResearchMachineRecord {
        id: "observer-gpu",
        name: "Observer GPU",
        role: "research",
        ssh_alias: None,
        status: "configured",
        details_json: None,
    })
    .await
    .unwrap();

    let post_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/machines",
            &token,
            &serde_json::json!({
                "id": "forbidden-gpu",
                "name": "Forbidden GPU",
                "role": "research"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(post_resp.status(), StatusCode::FORBIDDEN);

    for request in [
        auth_json_patch(
            "/api/research/machines/observer-gpu",
            &token,
            &serde_json::json!({"name": "Nope"}),
        ),
        auth_post("/api/research/machines/observer-gpu/disable", &token),
        auth_post("/api/research/machines/observer-gpu/enable", &token),
        auth_delete("/api/research/machines/observer-gpu", &token),
    ] {
        let resp = app.clone().oneshot(request).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}

/// Verifies machine delete rejects default and referenced machines through the API.
#[tokio::test]
async fn machine_api_delete_rejects_default_and_referenced_machines() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    db.create_research_machine(&crate::db::ResearchMachineRecord {
        id: "source-a",
        name: "Source A",
        role: "live",
        ssh_alias: None,
        status: "configured",
        details_json: None,
    })
    .await
    .unwrap();
    db.create_research_artifact(
        Some("source-a"),
        "readonly_run",
        "available",
        Some("paper"),
        Some("/tmp/source-a/manifest.json"),
    )
    .await
    .unwrap();

    for path in [
        "/api/research/machines/live",
        "/api/research/machines/source-a",
    ] {
        let resp = app
            .clone()
            .oneshot(auth_delete(path, &token))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}

/// Verifies that worker heartbeats update machine readiness without user JWT auth.
#[tokio::test]
async fn worker_heartbeat_updates_machine_status() {
    let (app, _db) = test_app();

    let resp = app
        .oneshot(worker_json_post(
            "/api/research/workers/heartbeat",
            Some("test-worker-token"),
            &serde_json::json!({
                "machine_id": "research",
                "worker_id": "research-worker-testing",
                "worker_version": "0.1.0",
                "status": "idle",
                "details": {"queue_depth": 0}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["machine"]["id"], "research");
    assert_eq!(body["machine"]["status"], "idle");
    let details: serde_json::Value =
        serde_json::from_str(body["machine"]["details_json"].as_str().unwrap()).unwrap();
    assert_eq!(details["worker_id"], "research-worker-testing");
    assert_eq!(details["details"]["queue_depth"], 0);
}

/// Verifies observer users can read typed research machine telemetry.
#[tokio::test]
async fn observer_can_read_research_machine_telemetry() {
    let (app, db) = test_app();
    let observer = observer_token(&db).await;

    let heartbeat = app
        .clone()
        .oneshot(worker_json_post(
            "/api/research/workers/heartbeat",
            Some("test-worker-token"),
            &telemetry_heartbeat_body(),
        ))
        .await
        .unwrap();
    assert_eq!(heartbeat.status(), StatusCode::OK);

    let resp = app
        .oneshot(auth_get(
            "/api/research/machines/research/telemetry",
            &observer,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["machine"]["id"], "research");
    assert_eq!(body["telemetry"]["worker_id"], "research-worker-testing");
    assert_eq!(body["telemetry"]["host"]["hostname"], "testing");
    assert_eq!(body["samples"].as_array().unwrap().len(), 2);
    assert_eq!(body["dependencies"]["artifacts"], 0);
    assert_eq!(body["disabled"], false);
    assert_eq!(body["stale_after_ms"], 90_000);
}

/// Verifies missing machine telemetry reads return not found.
#[tokio::test]
async fn machine_telemetry_missing_machine_returns_404() {
    let (app, db) = test_app();
    let observer = observer_token(&db).await;

    let resp = app
        .oneshot(auth_get(
            "/api/research/machines/missing/telemetry",
            &observer,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// Verifies stale state is computed from persisted heartbeat timestamps.
#[tokio::test]
async fn machine_telemetry_reports_stale_heartbeats() {
    let (app, db) = test_app();
    let observer = observer_token(&db).await;
    let host = buba_machine_telemetry::HostIdentity {
        hostname: "testing".to_string(),
        os_name: "Linux".to_string(),
        os_version: "test".to_string(),
        kernel_version: "test".to_string(),
        cpu_count: 2,
        total_ram_bytes: 16_384,
    };
    let sampler = buba_machine_telemetry::MachineSamplerHealth {
        sample_interval_ms: 5_000,
        samples_collected: 1,
        last_error: None,
    };
    let activity = serde_json::json!({"phase":"idle","heartbeat_interval_ms":1000});
    let samples = vec![buba_machine_telemetry::MachineSample {
        sampled_at_ms: 1_000,
        cpu_percent: 12.5,
        per_core_cpu: vec![12.5, 8.0],
        load_one: None,
        load_five: None,
        load_fifteen: None,
        mem_used_bytes: 4_096,
        mem_total_bytes: 16_384,
        mem_available_bytes: 12_288,
        swap_used_bytes: 0,
        swap_total_bytes: 0,
        disk_used_bytes: 50_000,
        disk_total_bytes: 100_000,
        disk_mount: "/research".to_string(),
    }];
    db.record_research_machine_heartbeat_with_telemetry_at(
        &crate::db::ResearchMachineHeartbeatRecord {
            machine_id: "research",
            worker_id: "research-worker-testing",
            worker_version: Some("0.2.0"),
            status: "idle",
            details: Some(&activity),
            telemetry: crate::db::ResearchMachineTelemetryUpdate {
                host: Some(&host),
                sampler: Some(&sampler),
                samples: &samples,
                activity: Some(&activity),
            },
        },
        1_000,
    )
    .await
    .unwrap();

    let resp = app
        .oneshot(auth_get(
            "/api/research/machines/research/telemetry",
            &observer,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    assert_eq!(body["stale"], true);
    assert_eq!(body["stale_after_ms"], 90_000);
}

/// Verifies that worker heartbeats require the configured worker token.
#[tokio::test]
async fn worker_heartbeat_rejects_missing_or_invalid_token() {
    let (app, _db) = test_app();
    let body = serde_json::json!({
        "machine_id": "research",
        "worker_id": "research-worker-testing"
    });

    for token in [None, Some("wrong-token")] {
        let resp = app
            .clone()
            .oneshot(worker_json_post(
                "/api/research/workers/heartbeat",
                token,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

/// Verifies that worker heartbeat payload validation rejects unsafe values.
#[tokio::test]
async fn worker_heartbeat_rejects_invalid_payloads() {
    let (app, _db) = test_app();

    for body in [
        serde_json::json!({"machine_id": "", "worker_id": "worker"}),
        serde_json::json!({"machine_id": "research", "worker_id": ""}),
        serde_json::json!({
            "machine_id": "research",
            "worker_id": "worker",
            "status": "mystery"
        }),
        serde_json::json!({"machine_id": "missing", "worker_id": "worker"}),
    ] {
        let resp = app
            .clone()
            .oneshot(worker_json_post(
                "/api/research/workers/heartbeat",
                Some("test-worker-token"),
                &body,
            ))
            .await
            .unwrap();
        assert!(
            matches!(
                resp.status(),
                StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
            ),
            "{body}"
        );
    }
}

/// Verifies typed heartbeat sample validation rejects unsafe sample values.
#[tokio::test]
async fn worker_heartbeat_rejects_invalid_telemetry_samples() {
    let (app, _db) = test_app();
    let mut body = telemetry_heartbeat_body();
    body["samples"][0]["disk_mount"] = serde_json::json!("");

    let resp = app
        .oneshot(worker_json_post(
            "/api/research/workers/heartbeat",
            Some("test-worker-token"),
            &body,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Verifies admins can import an already-local artifact by manifest.
#[tokio::test]
async fn admin_can_import_local_artifact() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact_dir = artifact_fixture();

    let resp = app
        .oneshot(auth_json_post(
            "/api/research/artifacts/import",
            &token,
            &serde_json::json!({
                "artifact_root": artifact_dir.path().to_str().unwrap(),
                "artifact_id": "artifact-import-1"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["artifact"]["id"], "artifact-import-1");
    assert_eq!(body["artifact"]["source_machine_id"], "live");
    assert_eq!(body["artifact"]["kind"], "readonly_run");
    assert_eq!(body["artifact"]["run_mode"], "live_readonly");
    assert_eq!(body["artifact"]["bytes"], 8);
    assert_eq!(body["verification"]["files_checked"], 1);
    assert_eq!(body["verification"]["bytes_checked"], 8);
}

/// Verifies artifact lifecycle controls and local manifest reads.
#[tokio::test]
async fn admin_can_verify_archive_restore_read_and_delete_artifact() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact_dir = artifact_fixture();
    let artifact_id = import_artifact_fixture(&app, &token, &artifact_dir).await;

    let patched = json_body(
        app.clone()
            .oneshot(auth_json_patch(
                &format!("/api/research/artifacts/{artifact_id}"),
                &token,
                &serde_json::json!({"replay_quality_class": "sweep_grade"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let manifest = json_body(
        app.clone()
            .oneshot(auth_get(
                &format!("/api/research/artifacts/{artifact_id}/manifest"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let checksums = text_body(
        app.clone()
            .oneshot(auth_get(
                &format!("/api/research/artifacts/{artifact_id}/checksums"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let verified = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/artifacts/{artifact_id}/verify"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let archived = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/artifacts/{artifact_id}/archive"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let restored = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/artifacts/{artifact_id}/restore"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let deleted = json_body(
        app.clone()
            .oneshot(auth_delete(
                &format!("/api/research/artifacts/{artifact_id}?delete_files=true"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let missing = app
        .oneshot(auth_get(
            &format!("/api/research/artifacts/{artifact_id}"),
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(patched["replay_quality_class"], "sweep_grade");
    assert_eq!(manifest["artifact_id"], artifact_id);
    assert!(checksums.contains("paint.db"));
    assert_eq!(verified["verification"]["files_checked"], 1);
    assert_eq!(archived["status"], "archived");
    assert!(archived["archived_at"].is_number());
    assert_eq!(restored["status"], "available");
    assert!(restored["archived_at"].is_null());
    assert_eq!(deleted["id"], artifact_id);
    assert!(!artifact_dir.path().exists());
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// Verifies artifact delete refuses records with durable dependencies.
#[tokio::test]
async fn artifact_delete_rejects_referenced_artifacts() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact_dir = artifact_fixture();
    let artifact_id = import_artifact_fixture(&app, &token, &artifact_dir).await;
    let admin = db.get_user_by_username("admin").await.unwrap().unwrap();
    db.create_research_job("current_params", Some(&artifact_id), &admin.id, 0, None)
        .await
        .unwrap();

    let deleted = app
        .clone()
        .oneshot(auth_delete(
            &format!("/api/research/artifacts/{artifact_id}?delete_files=true"),
            &token,
        ))
        .await
        .unwrap();
    let still_present = app
        .oneshot(auth_get(
            &format!("/api/research/artifacts/{artifact_id}"),
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(deleted.status(), StatusCode::BAD_REQUEST);
    assert_eq!(still_present.status(), StatusCode::OK);
    assert!(artifact_dir.path().exists());
}

/// Verifies admins can register remote artifact metadata without local file access.
#[tokio::test]
async fn admin_can_register_remote_artifact_from_manifest() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact_dir = artifact_fixture();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(artifact_dir.path().join("manifest.json")).unwrap(),
    )
    .unwrap();

    let resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/artifacts/register",
            &token,
            &serde_json::json!({
                "artifact_root": "/tmp/remote artifact",
                "manifest": manifest
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["artifact"]["id"], "artifact-import-1");
    assert_eq!(body["artifact"]["source_machine_id"], "live");
    assert_eq!(body["artifact"]["artifact_root"], "/tmp/remote artifact");
    assert_eq!(
        body["artifact"]["manifest_path"],
        "/tmp/remote artifact/manifest.json"
    );
    assert_eq!(
        body["artifact"]["source_db_path"],
        "/tmp/remote artifact/paint.db"
    );
    assert_eq!(body["artifact"]["bytes"], 8);
    assert_eq!(body["manifest_summary"]["files"], 1);
    assert_eq!(body["manifest_summary"]["bytes"], 8);

    let transfer = app
        .oneshot(auth_json_post(
            "/api/research/transfers",
            &token,
            &serde_json::json!({
                "artifact_id": "artifact-import-1",
                "dest_machine_id": "research"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(transfer.status(), StatusCode::OK);
    let transfer_body = json_body(transfer).await;
    assert_eq!(transfer_body["source_machine_id"], "live");
    assert_eq!(transfer_body["bytes_total"], 8);
}

/// Verifies artifact import rejects unsafe paths and manifest mismatches.
#[tokio::test]
async fn import_artifact_rejects_unsafe_or_mismatched_requests() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact_dir = artifact_fixture();

    for body in [
        serde_json::json!({
            "artifact_root": artifact_dir.path().to_str().unwrap(),
            "artifact_id": "wrong-id"
        }),
        serde_json::json!({
            "artifact_root": "../outside"
        }),
    ] {
        let resp = app
            .clone()
            .oneshot(auth_json_post(
                "/api/research/artifacts/import",
                &token,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{body}");
    }
}

/// Verifies remote artifact registration rejects unsafe or inconsistent metadata.
#[tokio::test]
async fn register_artifact_rejects_bad_remote_metadata() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact_dir = artifact_fixture();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(artifact_dir.path().join("manifest.json")).unwrap(),
    )
    .unwrap();
    let mut unsafe_manifest = manifest.clone();
    unsafe_manifest["artifact_id"] = serde_json::json!("../bad");
    let mut no_source_manifest = manifest.clone();
    no_source_manifest["source_machine_id"] = serde_json::Value::Null;
    let mut wal_manifest = manifest.clone();
    wal_manifest["files"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "logical_name": "runtime_db_wal",
            "kind": "sqlite_wal",
            "relative_path": "paint.db-wal",
            "bytes": 1,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }));

    for body in [
        serde_json::json!({
            "artifact_root": "relative/path",
            "manifest": manifest.clone()
        }),
        serde_json::json!({
            "artifact_root": "/tmp/remote",
            "source_machine_id": "research",
            "manifest": manifest.clone()
        }),
        serde_json::json!({
            "artifact_root": "/tmp/remote",
            "manifest": unsafe_manifest
        }),
        serde_json::json!({
            "artifact_root": "/tmp/remote",
            "source_machine_id": "missing-machine",
            "manifest": no_source_manifest
        }),
        serde_json::json!({
            "artifact_root": "/tmp/remote",
            "manifest": wal_manifest
        }),
    ] {
        let resp = app
            .clone()
            .oneshot(auth_json_post(
                "/api/research/artifacts/register",
                &token,
                &body,
            ))
            .await
            .unwrap();
        assert!(
            matches!(
                resp.status(),
                StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
            ),
            "{body}"
        );
    }
}

/// Verifies that admins can create a sweep job and get deterministic steps.
#[tokio::test]
async fn admin_can_create_sweep_job() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({
                "job_type": "sweep",
                "artifact_id": artifact.id,
                "priority": 3,
                "params": {"sweep": ["LATENCY_ARB_MIN_ASK=0.30,0.35"]}
            }),
        ))
        .await
        .unwrap();
    let probe_status = resp.status();
    let body = json_body(resp).await;
    assert_eq!(probe_status, StatusCode::OK, "body: {body}");
    assert_eq!(body["job"]["job_type"], "sweep");
    assert_eq!(body["job"]["status"], "queued");
    assert_eq!(body["steps"].as_array().unwrap().len(), 6);
    assert_eq!(body["steps"][4]["name"], "run_sweep");
}

/// Verifies admins can update queued jobs.
#[tokio::test]
async fn admin_can_update_queued_job() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let first_artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-1/manifest.json"),
        )
        .await
        .unwrap();
    let second_artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-2/manifest.json"),
        )
        .await
        .unwrap();
    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({
                "job_type": "current_params",
                "artifact_id": first_artifact.id,
                "priority": 1,
                "params": {"mode": "initial"}
            }),
        ))
        .await
        .unwrap();
    let create_body = json_body(create_resp).await;
    let job_id = create_body["job"]["id"].as_str().unwrap();

    let update_body = json_body(
        app.clone()
            .oneshot(auth_json_patch(
                &format!("/api/research/jobs/{job_id}"),
                &token,
                &serde_json::json!({
                    "artifact_id": second_artifact.id,
                    "priority": 8,
                    "params": {"mode": "updated"}
                }),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(update_body["job"]["artifact_id"], second_artifact.id);
    assert_eq!(update_body["job"]["priority"], 8);
    assert_eq!(update_body["job"]["params_json"], r#"{"mode":"updated"}"#);
}

/// Verifies admins can pause, resume, and continue jobs.
#[tokio::test]
async fn admin_can_pause_resume_and_continue_job() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let create_body = json_body(
        app.clone()
            .oneshot(auth_json_post(
                "/api/research/jobs",
                &token,
                &serde_json::json!({"job_type": "export"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let job_id = create_body["job"]["id"].as_str().unwrap();

    let pause_body = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/jobs/{job_id}/pause"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let resume_body = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/jobs/{job_id}/resume"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let cancel_body = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/jobs/{job_id}/cancel"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let continue_body = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/jobs/{job_id}/continue"),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;

    assert_eq!(pause_body["job"]["status"], "paused");
    assert!(
        pause_body["steps"]
            .as_array()
            .unwrap()
            .iter()
            .all(|step| step["status"] == "paused")
    );
    assert_eq!(resume_body["job"]["status"], "queued");
    assert_eq!(cancel_body["job"]["status"], "cancelled");
    assert_eq!(continue_body["job"]["status"], "queued");
}

/// Verifies admins can clone jobs with provenance events.
#[tokio::test]
async fn admin_can_clone_job_with_provenance() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    let create_body = json_body(
        app.clone()
            .oneshot(auth_json_post(
                "/api/research/jobs",
                &token,
                &serde_json::json!({
                    "job_type": "current_params",
                    "artifact_id": artifact.id,
                    "priority": 1,
                    "params": {"mode": "source"}
                }),
            ))
            .await
            .unwrap(),
    )
    .await;
    let job_id = create_body["job"]["id"].as_str().unwrap();
    let clone_body = json_body(
        app.oneshot(auth_json_post(
            &format!("/api/research/jobs/{job_id}/clone"),
            &token,
            &serde_json::json!({"priority": 4}),
        ))
        .await
        .unwrap(),
    )
    .await;

    assert_ne!(clone_body["job"]["id"], job_id);
    assert_eq!(clone_body["job"]["job_type"], "current_params");
    assert_eq!(clone_body["job"]["artifact_id"], artifact.id);
    assert_eq!(clone_body["job"]["priority"], 4);
    assert_eq!(clone_body["events"][0]["details_json"].as_str().unwrap(), {
        let expected = serde_json::json!({
            "source_job_id": job_id,
            "source_job_status": "queued"
        });
        expected.to_string()
    });
}

/// Verifies reusable job templates support admin mutation, observer reads, and job provenance.
#[tokio::test]
async fn job_template_api_crud_permissions_and_usage() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();

    let created = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/job-templates",
            &admin,
            &serde_json::json!({
                "name": "Bounded smoke",
                "description": "short current params",
                "job_type": "current_params",
                "artifact_id": artifact.id,
                "priority": 4,
                "params": {"start_ms": 1, "end_ms": 2, "balance": 200}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let template_body = json_body(created).await;
    let template_id = template_body["template"]["id"].as_str().unwrap();

    let observer_list = app
        .clone()
        .oneshot(auth_get("/api/research/job-templates", &observer))
        .await
        .unwrap();
    let observer_create = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/job-templates",
            &observer,
            &serde_json::json!({
                "name": "Observer template",
                "job_type": "current_params",
                "params": {}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(observer_list.status(), StatusCode::OK);
    assert_eq!(observer_create.status(), StatusCode::FORBIDDEN);

    let job_body = json_body(
        app.clone()
            .oneshot(auth_json_post(
                "/api/research/jobs",
                &admin,
                &serde_json::json!({
                    "job_type": "current_params",
                    "artifact_id": artifact.id,
                    "template_id": template_id,
                    "priority": 8,
                    "params": {"start_ms": 10, "end_ms": 20, "balance": 250}
                }),
            ))
            .await
            .unwrap(),
    )
    .await;
    let template_after_use = json_body(
        app.clone()
            .oneshot(auth_get(
                &format!("/api/research/job-templates/{template_id}"),
                &admin,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(job_body["job"]["priority"], 8);
    assert_eq!(
        job_body["events"][0]["message"],
        "created from research job template"
    );
    assert_eq!(template_after_use["template"]["usage_count"], 1);

    let archived = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/job-templates/{template_id}/archive"),
            &admin,
        ))
        .await
        .unwrap();
    assert_eq!(archived.status(), StatusCode::OK);
    let archived_job = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &admin,
            &serde_json::json!({
                "job_type": "current_params",
                "artifact_id": artifact.id,
                "template_id": template_id,
                "params": {"start_ms": 10, "end_ms": 20}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(archived_job.status(), StatusCode::BAD_REQUEST);

    let restored = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/job-templates/{template_id}/restore"),
            &admin,
        ))
        .await
        .unwrap();
    assert_eq!(restored.status(), StatusCode::OK);
    let deleted = app
        .oneshot(auth_delete(
            &format!("/api/research/job-templates/{template_id}"),
            &admin,
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);
}

/// Verifies admins can retry, resolve, cancel, and continue individual steps.
#[tokio::test]
async fn admin_can_recover_and_cancel_individual_steps() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let admin = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job("export", None, &admin.id, 0, None)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    db.fail_research_step_at(&lease.step.id, "worker-a", "temporary", true, 1_100)
        .await
        .unwrap();

    let retry_body = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!(
                    "/api/research/jobs/{}/steps/{}/retry",
                    job.id, lease.step.id
                ),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let lease = db
        .lease_next_research_step_at("worker-a", 2_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    db.block_research_step_at(&lease.step.id, "worker-a", "operator action", 2_100)
        .await
        .unwrap();
    let resolve_body = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!(
                    "/api/research/jobs/{}/steps/{}/resolve-blocker",
                    job.id, lease.step.id
                ),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let cancel_body = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!(
                    "/api/research/jobs/{}/steps/{}/cancel",
                    job.id, lease.step.id
                ),
                &token,
            ))
            .await
            .unwrap(),
    )
    .await;
    let continue_body = json_body(
        app.oneshot(auth_post(
            &format!(
                "/api/research/jobs/{}/steps/{}/retry",
                job.id, lease.step.id
            ),
            &token,
        ))
        .await
        .unwrap(),
    )
    .await;

    assert_eq!(retry_body["job"]["status"], "queued");
    assert_eq!(retry_body["steps"][0]["status"], "queued");
    assert_eq!(resolve_body["job"]["status"], "queued");
    assert_eq!(resolve_body["steps"][0]["status"], "queued");
    assert_eq!(cancel_body["job"]["status"], "cancelled");
    assert_eq!(cancel_body["steps"][0]["status"], "cancelled");
    assert_eq!(continue_body["job"]["status"], "queued");
    assert_eq!(continue_body["steps"][0]["status"], "queued");
}

/// Verifies admins can clear stale step leases.
#[tokio::test]
async fn admin_can_clear_stale_step_lease() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let admin = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job("export", None, &admin.id, 0, None)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 1)
        .await
        .unwrap()
        .unwrap();

    let clear_body = json_body(
        app.oneshot(auth_post(
            &format!(
                "/api/research/jobs/{}/steps/{}/clear-lease",
                job.id, lease.step.id
            ),
            &token,
        ))
        .await
        .unwrap(),
    )
    .await;

    assert_eq!(clear_body["job"]["status"], "retryable");
    assert_eq!(clear_body["steps"][0]["status"], "retryable");
    assert_eq!(
        clear_body["steps"][0]["error"],
        "stale lease cleared by operator"
    );
}

/// Verifies that observers cannot create research jobs.
#[tokio::test]
async fn observer_cannot_create_research_job() {
    let (app, db) = test_app();
    let token = observer_token(&db).await;

    let resp = app
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// Verifies that observers cannot mutate research job controls.
#[tokio::test]
async fn observer_cannot_mutate_job_controls() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &admin,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let create_body = json_body(create_resp).await;
    let job_id = create_body["job"]["id"].as_str().unwrap();

    let cancel_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/cancel"),
            &observer,
        ))
        .await
        .unwrap();
    let update_resp = app
        .clone()
        .oneshot(auth_json_patch(
            &format!("/api/research/jobs/{job_id}"),
            &observer,
            &serde_json::json!({"priority": 10}),
        ))
        .await
        .unwrap();
    let delete_resp = app
        .clone()
        .oneshot(auth_delete(
            &format!("/api/research/jobs/{job_id}"),
            &observer,
        ))
        .await
        .unwrap();
    let pause_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/pause"),
            &observer,
        ))
        .await
        .unwrap();
    let resume_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/resume"),
            &observer,
        ))
        .await
        .unwrap();
    let continue_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/continue"),
            &observer,
        ))
        .await
        .unwrap();
    let retry_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/retry"),
            &observer,
        ))
        .await
        .unwrap();
    let clone_resp = app
        .clone()
        .oneshot(auth_json_post(
            &format!("/api/research/jobs/{job_id}/clone"),
            &observer,
            &serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(cancel_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(update_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(delete_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(pause_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(resume_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(continue_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(retry_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(clone_resp.status(), StatusCode::FORBIDDEN);
}

/// Verifies that observers cannot mutate research job steps or events.
#[tokio::test]
async fn observer_cannot_mutate_job_steps_or_events() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &admin,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let create_body = json_body(create_resp).await;
    let job_id = create_body["job"]["id"].as_str().unwrap();
    let step_id = create_body["steps"][0]["id"].as_str().unwrap();

    let step_retry_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/steps/{step_id}/retry"),
            &observer,
        ))
        .await
        .unwrap();
    let event_resp = app
        .oneshot(auth_json_post(
            &format!("/api/research/jobs/{job_id}/events"),
            &observer,
            &serde_json::json!({"level": "info", "message": "note"}),
        ))
        .await
        .unwrap();

    assert_eq!(step_retry_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(event_resp.status(), StatusCode::FORBIDDEN);
}

/// Verifies observers cannot import artifacts or mutate transfers.
#[tokio::test]
async fn observer_cannot_mutate_artifacts_or_transfers() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let artifact_dir = artifact_fixture();
    let import_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/artifacts/import",
            &admin,
            &serde_json::json!({"artifact_root": artifact_dir.path().to_str().unwrap()}),
        ))
        .await
        .unwrap();
    let imported = json_body(import_resp).await;
    let artifact_id = imported["artifact"]["id"].as_str().unwrap();
    let admin_transfer = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/transfers",
            &admin,
            &serde_json::json!({
                "artifact_id": artifact_id,
                "source_machine_id": "live",
                "dest_machine_id": "research"
            }),
        ))
        .await
        .unwrap();
    let transfer_id = json_body(admin_transfer).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let observer_import = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/artifacts/import",
            &observer,
            &serde_json::json!({"artifact_root": artifact_dir.path().to_str().unwrap()}),
        ))
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(artifact_dir.path().join("manifest.json")).unwrap(),
    )
    .unwrap();
    let observer_register = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/artifacts/register",
            &observer,
            &serde_json::json!({
                "artifact_root": "/tmp/remote-artifact",
                "manifest": manifest
            }),
        ))
        .await
        .unwrap();
    let observer_transfer = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/transfers",
            &observer,
            &serde_json::json!({
                "artifact_id": artifact_id,
                "source_machine_id": "live",
                "dest_machine_id": "research"
            }),
        ))
        .await
        .unwrap();
    let observer_pause = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/transfers/{transfer_id}/pause"),
            &observer,
        ))
        .await
        .unwrap();
    let observer_verify = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/transfers/{transfer_id}/verify"),
            &observer,
        ))
        .await
        .unwrap();
    let observer_delete = app
        .clone()
        .oneshot(auth_delete(
            &format!("/api/research/transfers/{transfer_id}"),
            &observer,
        ))
        .await
        .unwrap();

    assert_eq!(observer_import.status(), StatusCode::FORBIDDEN);
    assert_eq!(observer_register.status(), StatusCode::FORBIDDEN);
    assert_eq!(observer_transfer.status(), StatusCode::FORBIDDEN);
    assert_eq!(observer_pause.status(), StatusCode::FORBIDDEN);
    assert_eq!(observer_verify.status(), StatusCode::FORBIDDEN);
    assert_eq!(observer_delete.status(), StatusCode::FORBIDDEN);
}

/// Verifies observers cannot mutate artifact lifecycle controls.
#[tokio::test]
async fn observer_cannot_mutate_artifact_lifecycle() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let artifact_dir = artifact_fixture();
    let artifact_id = import_artifact_fixture(&app, &admin, &artifact_dir).await;

    let patch = app
        .clone()
        .oneshot(auth_json_patch(
            &format!("/api/research/artifacts/{artifact_id}"),
            &observer,
            &serde_json::json!({"replay_quality_class": "manual"}),
        ))
        .await
        .unwrap();
    let verify = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/artifacts/{artifact_id}/verify"),
            &observer,
        ))
        .await
        .unwrap();
    let archive = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/artifacts/{artifact_id}/archive"),
            &observer,
        ))
        .await
        .unwrap();
    let restore = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/artifacts/{artifact_id}/restore"),
            &observer,
        ))
        .await
        .unwrap();
    let delete = app
        .oneshot(auth_delete(
            &format!("/api/research/artifacts/{artifact_id}"),
            &observer,
        ))
        .await
        .unwrap();

    assert_eq!(patch.status(), StatusCode::FORBIDDEN);
    assert_eq!(verify.status(), StatusCode::FORBIDDEN);
    assert_eq!(archive.status(), StatusCode::FORBIDDEN);
    assert_eq!(restore.status(), StatusCode::FORBIDDEN);
    assert_eq!(delete.status(), StatusCode::FORBIDDEN);
}

/// Verifies event append and list endpoints.
#[tokio::test]
async fn admin_can_append_and_list_job_events() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;

    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let create_body = json_body(create_resp).await;
    let job_id = create_body["job"]["id"].as_str().unwrap();

    let append_resp = app
        .clone()
        .oneshot(auth_json_post(
            &format!("/api/research/jobs/{job_id}/events"),
            &token,
            &serde_json::json!({
                "level": "info",
                "message": "phase 2 event",
                "details": {"ok": true}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(append_resp.status(), StatusCode::OK);

    let list_resp = app
        .oneshot(auth_get(
            &format!("/api/research/jobs/{job_id}/events"),
            &token,
        ))
        .await
        .unwrap();
    let body = json_body(list_resp).await;
    assert_eq!(body["events"].as_array().unwrap().len(), 1);
    assert_eq!(body["events"][0]["message"], "phase 2 event");
}

/// Verifies admins can regenerate report files from persisted job detail.
#[tokio::test]
async fn admin_can_regenerate_job_report_files() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job("export", None, &admin_user.id, 0, None)
        .await
        .unwrap();
    db.append_research_job_event(&job.id, None, "info", "ready to regenerate", None)
        .await
        .unwrap();
    complete_all_job_steps(&db, "worker-report").await;

    let resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{}/report/regenerate", job.id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let report_path = PathBuf::from(body["report_path"].as_str().unwrap());
    let csv_path = PathBuf::from(body["csv_path"].as_str().unwrap());
    assert!(report_path.exists());
    assert!(csv_path.exists());
    assert_eq!(body["report"]["job_id"], job.id);

    let report_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(report_path).unwrap()).unwrap();
    assert_eq!(report_json["regenerated"], true);
    assert_eq!(report_json["events"][0]["message"], "ready to regenerate");
    assert_eq!(report_json["steps"].as_array().unwrap().len(), 4);
    let csv = std::fs::read_to_string(csv_path).unwrap();
    assert!(csv.contains("step_index,name,status,attempts,error"));
}

/// Verifies admins can idempotently archive only bulky job scratch DB files.
#[tokio::test]
async fn admin_can_archive_completed_job_scratch_dbs() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let artifact_dir = artifact_fixture();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z"
    });
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &admin_user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    complete_all_job_steps(&db, "worker-archive").await;

    let job_root = std::env::temp_dir().join("jobs").join(&job.id);
    std::fs::create_dir_all(&job_root).unwrap();
    let prepared = job_root.join("prepared-backtest.db");
    let prepared_wal = job_root.join("prepared-backtest.db-wal");
    let prepared_shm = job_root.join("prepared-backtest.db-shm");
    let backtest = job_root.join("backtest.db");
    let report_json = job_root.join("report.json");
    let report_csv = job_root.join("report.csv");
    for path in [&prepared, &prepared_wal, &prepared_shm, &backtest] {
        std::fs::write(path, b"scratch").unwrap();
    }
    std::fs::write(&report_json, r#"{"schema_version":1}"#).unwrap();
    std::fs::write(&report_csv, "step,status\n").unwrap();
    db.create_or_update_research_report(&crate::db::ResearchReportRecord {
        job_id: &job.id,
        artifact_id: Some(&artifact.id),
        title: "Completed backtest",
        status: "available",
        summary_json: Some(r#"{"schema_version":1}"#),
        report_path: Some(report_json.to_str().unwrap()),
        csv_path: Some(report_csv.to_str().unwrap()),
    })
    .await
    .unwrap();

    let resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{}/archive-scratch", job.id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["job"]["status"], "completed");
    assert!(body["archive"]["deleted_paths"].as_array().unwrap().len() >= 4);
    for path in [&prepared, &prepared_wal, &prepared_shm, &backtest] {
        assert!(!path.exists(), "scratch file should be deleted: {path:?}");
    }
    assert!(report_json.exists());
    assert!(report_csv.exists());
    assert!(artifact_dir.path().join("manifest.json").exists());

    let second = app
        .oneshot(auth_post(
            &format!("/api/research/jobs/{}/archive-scratch", job.id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let second_body = json_body(second).await;
    assert_eq!(
        second_body["archive"]["deleted_paths"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert!(
        second_body["archive"]["skipped_paths"]
            .as_array()
            .unwrap()
            .len()
            >= 6
    );
}

/// Verifies queue and retention endpoints summarize attention and archive-only candidates.
#[tokio::test]
async fn queue_and_retention_endpoints_summarize_operator_state() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    db.set_research_machine_status("research", "disabled")
        .await
        .unwrap();
    let artifact_dir = artifact_fixture();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some(artifact_dir.path().join("manifest.json").to_str().unwrap()),
        )
        .await
        .unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z"
    });
    let completed = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &admin_user.id,
            0,
            Some(&params.to_string()),
        )
        .await
        .unwrap();
    complete_all_job_steps(&db, "worker-retention").await;
    let job_root = std::env::temp_dir().join("jobs").join(&completed.id);
    std::fs::create_dir_all(&job_root).unwrap();
    let prepared = job_root.join("prepared-backtest.db");
    let backtest = job_root.join("backtest.db");
    let report_json = job_root.join("report.json");
    let report_csv = job_root.join("report.csv");
    std::fs::write(&prepared, b"scratch").unwrap();
    std::fs::write(&backtest, b"scratch").unwrap();
    std::fs::write(&report_json, r#"{"schema_version":1}"#).unwrap();
    std::fs::write(&report_csv, "step,status\n").unwrap();
    let report = db
        .create_or_update_research_report(&crate::db::ResearchReportRecord {
            job_id: &completed.id,
            artifact_id: Some(&artifact.id),
            title: "Retention report",
            status: "available",
            summary_json: Some(r#"{"schema_version":1}"#),
            report_path: Some(report_json.to_str().unwrap()),
            csv_path: Some(report_csv.to_str().unwrap()),
        })
        .await
        .unwrap();
    db.create_research_job("export", None, &admin_user.id, 0, None)
        .await
        .unwrap();
    let transfer = db
        .create_artifact_transfer(&crate::db::ArtifactTransferRecord {
            artifact_id: &artifact.id,
            source_machine_id: None,
            dest_machine_id: None,
            bytes_total: Some(100),
        })
        .await
        .unwrap();
    db.update_artifact_transfer_progress(
        &transfer.id,
        "failed",
        Some(10),
        Some(100),
        Some("failed"),
        Some("checksum mismatch"),
    )
    .await
    .unwrap();

    let queue = json_body(
        app.clone()
            .oneshot(auth_get("/api/research/queue", &observer))
            .await
            .unwrap(),
    )
    .await;
    let retention = json_body(
        app.clone()
            .oneshot(auth_get("/api/research/retention", &observer))
            .await
            .unwrap(),
    )
    .await;
    let forbidden = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/retention/archive",
            &observer,
            &serde_json::json!({"job_ids": [completed.id]}),
        ))
        .await
        .unwrap();
    let archived = json_body(
        app.oneshot(auth_json_post(
            "/api/research/retention/archive",
            &admin,
            &serde_json::json!({
                "job_ids": [completed.id],
                "report_ids": [report.id],
                "artifact_ids": [artifact.id]
            }),
        ))
        .await
        .unwrap(),
    )
    .await;

    assert_eq!(queue["counts"]["jobs_waiting"], 1);
    assert_eq!(queue["counts"]["transfers_attention"], 1);
    assert_eq!(queue["counts"]["disabled_hosts"], 1);
    assert_eq!(retention["totals"]["jobs"], 1);
    assert_eq!(retention["totals"]["reports"], 1);
    assert_eq!(retention["totals"]["artifacts"], 1);
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(archived["jobs"][0]["status"], "archived");
    assert_eq!(archived["reports"][0]["status"], "archived");
    assert_eq!(archived["artifacts"][0]["status"], "archived");
    assert!(!prepared.exists());
    assert!(!backtest.exists());
    assert!(report_json.exists());
    assert!(report_csv.exists());
}

/// Verifies scratch archival rejects unsafe or premature lifecycle states.
#[tokio::test]
async fn archive_scratch_requires_admin_completed_job_and_report() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job("export", None, &admin_user.id, 0, None)
        .await
        .unwrap();

    let observer_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{}/archive-scratch", job.id),
            &observer,
        ))
        .await
        .unwrap();
    assert_eq!(observer_resp.status(), StatusCode::FORBIDDEN);

    let queued_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{}/archive-scratch", job.id),
            &admin,
        ))
        .await
        .unwrap();
    assert_eq!(queued_resp.status(), StatusCode::BAD_REQUEST);

    complete_all_job_steps(&db, "worker-no-report").await;
    let no_report_resp = app
        .oneshot(auth_post(
            &format!("/api/research/jobs/{}/archive-scratch", job.id),
            &admin,
        ))
        .await
        .unwrap();
    assert_eq!(no_report_resp.status(), StatusCode::BAD_REQUEST);
}

/// Verifies report regeneration requires admin permissions.
#[tokio::test]
async fn observer_cannot_regenerate_job_report() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &admin,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let job_id = json_body(create_resp).await["job"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/report/regenerate"),
            &observer,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// Verifies regeneration refuses unsafe stored report paths before writing files.
#[tokio::test]
async fn report_regeneration_rejects_unsafe_existing_paths() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job("export", None, &admin_user.id, 0, None)
        .await
        .unwrap();
    db.create_or_update_research_report(&crate::db::ResearchReportRecord {
        job_id: &job.id,
        artifact_id: None,
        title: "Unsafe report",
        status: "available",
        summary_json: None,
        report_path: Some("/etc/passwd"),
        csv_path: Some("/etc/shadow"),
    })
    .await
    .unwrap();

    let resp = app
        .oneshot(auth_post(
            &format!("/api/research/jobs/{}/report/regenerate", job.id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Verifies regeneration without an existing report rejects jobs that have not produced results.
#[tokio::test]
async fn report_regeneration_rejects_unready_jobs_without_report() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let job_id = json_body(create_resp).await["job"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/report/regenerate"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Verifies that event append validation errors surface through the API.
#[tokio::test]
async fn append_job_event_rejects_invalid_api_payloads() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;

    let first_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let second_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let first_body = json_body(first_resp).await;
    let second_body = json_body(second_resp).await;
    let first_job_id = first_body["job"]["id"].as_str().unwrap();
    let second_job_id = second_body["job"]["id"].as_str().unwrap();
    let foreign_step_id = db.get_research_job_steps(second_job_id).await.unwrap()[0]
        .id
        .clone();

    for body in [
        serde_json::json!({"level": "debug", "message": "hidden"}),
        serde_json::json!({"level": "info", "message": ""}),
        serde_json::json!({
            "step_id": foreign_step_id,
            "level": "info",
            "message": "wrong job"
        }),
    ] {
        let resp = app
            .clone()
            .oneshot(auth_json_post(
                &format!("/api/research/jobs/{first_job_id}/events"),
                &token,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{body}");
    }
}

/// Verifies cancel and retry endpoints change job state.
#[tokio::test]
async fn admin_can_cancel_and_retry_export_job() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;

    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let create_body = json_body(create_resp).await;
    let job_id = create_body["job"]["id"].as_str().unwrap();

    let cancel_resp = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/cancel"),
            &token,
        ))
        .await
        .unwrap();
    let cancel_body = json_body(cancel_resp).await;
    assert_eq!(cancel_body["job"]["status"], "cancelled");

    let retry_resp = app
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/retry"),
            &token,
        ))
        .await
        .unwrap();
    let retry_body = json_body(retry_resp).await;
    assert_eq!(retry_body["job"]["status"], "queued");
}

/// Verifies admins can delete cancelled jobs without reports.
#[tokio::test]
async fn admin_can_delete_cancelled_unreported_job() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let create_body = json_body(
        app.clone()
            .oneshot(auth_json_post(
                "/api/research/jobs",
                &token,
                &serde_json::json!({"job_type": "export"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let job_id = create_body["job"]["id"].as_str().unwrap();

    let active_delete = app
        .clone()
        .oneshot(auth_delete(&format!("/api/research/jobs/{job_id}"), &token))
        .await
        .unwrap();
    app.clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/cancel"),
            &token,
        ))
        .await
        .unwrap();
    let deleted = json_body(
        app.clone()
            .oneshot(auth_delete(&format!("/api/research/jobs/{job_id}"), &token))
            .await
            .unwrap(),
    )
    .await;
    let missing = app
        .oneshot(auth_get(&format!("/api/research/jobs/{job_id}"), &token))
        .await
        .unwrap();

    assert_eq!(active_delete.status(), StatusCode::BAD_REQUEST);
    assert_eq!(deleted["id"], job_id);
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// Verifies that missing research detail records return not found.
#[tokio::test]
async fn research_api_returns_not_found_for_missing_detail_records() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;

    for path in [
        "/api/research/artifacts/missing-artifact",
        "/api/research/jobs/missing-job",
        "/api/research/jobs/missing-job/events",
        "/api/research/reports/missing-report",
    ] {
        let resp = app.clone().oneshot(auth_get(path, &token)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

/// Verifies that invalid job creation and control requests are rejected.
#[tokio::test]
async fn research_api_rejects_invalid_job_mutations() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;

    let invalid_job = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "optimize"}),
        ))
        .await
        .unwrap();
    let missing_artifact = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "sweep", "artifact_id": "missing-artifact"}),
        ))
        .await
        .unwrap();
    let create_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs",
            &token,
            &serde_json::json!({"job_type": "export"}),
        ))
        .await
        .unwrap();
    let create_body = json_body(create_resp).await;
    let job_id = create_body["job"]["id"].as_str().unwrap();
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let reported = db
        .create_research_job("export", None, &admin_user.id, 0, None)
        .await
        .unwrap();
    db.cancel_research_job(&reported.id).await.unwrap();
    db.create_or_update_research_report(&crate::db::ResearchReportRecord {
        job_id: &reported.id,
        artifact_id: None,
        title: "Reported job",
        status: "available",
        summary_json: None,
        report_path: None,
        csv_path: None,
    })
    .await
    .unwrap();
    let retry_queued = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/jobs/{job_id}/retry"),
            &token,
        ))
        .await
        .unwrap();
    db.lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    let update_started = app
        .clone()
        .oneshot(auth_json_patch(
            &format!("/api/research/jobs/{job_id}"),
            &token,
            &serde_json::json!({"priority": 10}),
        ))
        .await
        .unwrap();
    let cancel_missing = app
        .clone()
        .oneshot(auth_post("/api/research/jobs/missing-job/cancel", &token))
        .await
        .unwrap();
    let clone_missing = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/jobs/missing-job/clone",
            &token,
            &serde_json::json!({}),
        ))
        .await
        .unwrap();
    let delete_reported = app
        .oneshot(auth_delete(
            &format!("/api/research/jobs/{}", reported.id),
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(invalid_job.status(), StatusCode::BAD_REQUEST);
    assert_eq!(missing_artifact.status(), StatusCode::NOT_FOUND);
    assert_eq!(retry_queued.status(), StatusCode::BAD_REQUEST);
    assert_eq!(update_started.status(), StatusCode::BAD_REQUEST);
    assert_eq!(cancel_missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(clone_missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(delete_reported.status(), StatusCode::BAD_REQUEST);
}

/// Verifies that research list endpoints return expected arrays.
#[tokio::test]
async fn research_api_lists_artifacts_jobs_reports_and_transfers() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    let admin = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job("current_params", Some(&artifact.id), &admin.id, 0, None)
        .await
        .unwrap();
    db.create_or_update_research_report(&crate::db::ResearchReportRecord {
        job_id: &job.id,
        artifact_id: Some(&artifact.id),
        title: "Report",
        status: "available",
        summary_json: Some(r#"{"ok":true}"#),
        report_path: Some("/tmp/report.json"),
        csv_path: Some("/tmp/report.csv"),
    })
    .await
    .unwrap();

    let artifacts = json_body(
        app.clone()
            .oneshot(auth_get("/api/research/artifacts", &token))
            .await
            .unwrap(),
    )
    .await;
    let jobs = json_body(
        app.clone()
            .oneshot(auth_get("/api/research/jobs", &token))
            .await
            .unwrap(),
    )
    .await;
    let reports = json_body(
        app.clone()
            .oneshot(auth_get("/api/research/reports", &token))
            .await
            .unwrap(),
    )
    .await;
    let transfers = json_body(
        app.oneshot(auth_get("/api/research/transfers", &token))
            .await
            .unwrap(),
    )
    .await;

    assert_eq!(artifacts["artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(reports["reports"].as_array().unwrap().len(), 1);
    assert_eq!(transfers["transfers"].as_array().unwrap().len(), 0);
}

/// Verifies transfer API lifecycle controls.
#[tokio::test]
async fn admin_can_create_update_cancel_and_retry_transfer() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();

    let create = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/transfers",
            &token,
            &serde_json::json!({
                "artifact_id": artifact.id,
                "source_machine_id": "live",
                "dest_machine_id": "research",
                "bytes_total": 100
            }),
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_body = json_body(create).await;
    let transfer_id = create_body["id"].as_str().unwrap();
    assert_eq!(create_body["status"], "queued");

    let running = app
        .clone()
        .oneshot(auth_json_post(
            &format!("/api/research/transfers/{transfer_id}/progress"),
            &token,
            &serde_json::json!({
                "status": "running",
                "bytes_done": 40,
                "checksum_status": "pending"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(running.status(), StatusCode::OK);
    let running_body = json_body(running).await;
    assert_eq!(running_body["bytes_done"], 40);

    let cancel = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/transfers/{transfer_id}/cancel"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(cancel.status(), StatusCode::OK);
    assert_eq!(json_body(cancel).await["status"], "cancelled");

    let retry = app
        .clone()
        .oneshot(auth_json_post(
            &format!("/api/research/transfers/{transfer_id}/retry"),
            &token,
            &serde_json::json!({"resume": true}),
        ))
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::OK);
    let retry_body = json_body(retry).await;
    assert_eq!(retry_body["status"], "queued");
    assert_eq!(retry_body["bytes_done"], 40);

    let complete = app
        .clone()
        .oneshot(auth_json_post(
            &format!("/api/research/transfers/{transfer_id}/progress"),
            &token,
            &serde_json::json!({
                "status": "completed",
                "bytes_done": 100,
                "checksum_status": "verified"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(complete.status(), StatusCode::OK);
    assert_eq!(json_body(complete).await["status"], "completed");

    let detail = app
        .oneshot(auth_get(
            &format!("/api/research/transfers/{transfer_id}"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    assert_eq!(json_body(detail).await["checksum_status"], "verified");
}

/// Verifies transfer pause, resume, verify, and delete endpoints.
#[tokio::test]
async fn admin_can_pause_resume_verify_and_delete_transfer() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact_dir = artifact_fixture();
    let import_resp = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/artifacts/import",
            &token,
            &serde_json::json!({"artifact_root": artifact_dir.path().to_str().unwrap()}),
        ))
        .await
        .unwrap();
    assert_eq!(import_resp.status(), StatusCode::OK);
    let artifact_id = json_body(import_resp).await["artifact"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let create = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/transfers",
            &token,
            &serde_json::json!({
                "artifact_id": artifact_id,
                "dest_machine_id": "research"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let transfer_id = json_body(create).await["id"].as_str().unwrap().to_string();

    let pause = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/transfers/{transfer_id}/pause"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(pause.status(), StatusCode::OK);
    assert_eq!(json_body(pause).await["status"], "paused");
    assert!(
        db.claim_next_artifact_transfer("research")
            .await
            .unwrap()
            .is_none()
    );

    let resume = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/transfers/{transfer_id}/resume"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resume.status(), StatusCode::OK);
    assert_eq!(json_body(resume).await["status"], "queued");

    let verify = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/transfers/{transfer_id}/verify"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);
    let verify_body = json_body(verify).await;
    assert_eq!(verify_body["transfer"]["status"], "completed");
    assert_eq!(verify_body["transfer"]["checksum_status"], "verified");
    assert_eq!(verify_body["verification"]["bytes_checked"], 8);

    let delete = app
        .clone()
        .oneshot(auth_delete(
            &format!("/api/research/transfers/{transfer_id}"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
    assert_eq!(json_body(delete).await["id"], transfer_id);

    let missing = app
        .oneshot(auth_get(
            &format!("/api/research/transfers/{transfer_id}"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// Verifies transfer APIs reject invalid transitions and missing records.
#[tokio::test]
async fn transfer_api_rejects_invalid_requests() {
    let (app, db) = test_app();
    let token = admin_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();

    let missing_artifact = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/transfers",
            &token,
            &serde_json::json!({"artifact_id": "missing-artifact"}),
        ))
        .await
        .unwrap();
    let archived_artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "archived",
            Some("live_readonly"),
            Some("/tmp/archived-artifact/manifest.json"),
        )
        .await
        .unwrap();
    let archived_transfer = app
        .clone()
        .oneshot(auth_json_post(
            "/api/research/transfers",
            &token,
            &serde_json::json!({"artifact_id": archived_artifact.id}),
        ))
        .await
        .unwrap();
    let transfer = db
        .create_artifact_transfer(&crate::db::ArtifactTransferRecord {
            artifact_id: &artifact.id,
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(100),
        })
        .await
        .unwrap();
    let invalid_progress = app
        .clone()
        .oneshot(auth_json_post(
            &format!("/api/research/transfers/{}/progress", transfer.id),
            &token,
            &serde_json::json!({
                "status": "completed",
                "bytes_done": 100,
                "checksum_status": "pending"
            }),
        ))
        .await
        .unwrap();
    let invalid_resume = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/transfers/{}/resume", transfer.id),
            &token,
        ))
        .await
        .unwrap();
    let invalid_delete = app
        .clone()
        .oneshot(auth_delete(
            &format!("/api/research/transfers/{}", transfer.id),
            &token,
        ))
        .await
        .unwrap();
    let missing_detail = app
        .clone()
        .oneshot(auth_get("/api/research/transfers/missing-transfer", &token))
        .await
        .unwrap();
    let missing_verify = app
        .oneshot(auth_post(
            "/api/research/transfers/missing-transfer/verify",
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(missing_artifact.status(), StatusCode::NOT_FOUND);
    assert_eq!(archived_transfer.status(), StatusCode::BAD_REQUEST);
    assert_eq!(invalid_progress.status(), StatusCode::BAD_REQUEST);
    assert_eq!(invalid_resume.status(), StatusCode::BAD_REQUEST);
    assert_eq!(invalid_delete.status(), StatusCode::BAD_REQUEST);
    assert_eq!(missing_detail.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing_verify.status(), StatusCode::NOT_FOUND);
}

/// Verifies that detail endpoints return artifact and report payloads.
#[tokio::test]
async fn research_api_returns_artifact_and_report_details() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let observer = observer_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &admin_user.id,
            0,
            None,
        )
        .await
        .unwrap();
    let report = db
        .create_or_update_research_report(&crate::db::ResearchReportRecord {
            job_id: &job.id,
            artifact_id: Some(&artifact.id),
            title: "Detail report",
            status: "available",
            summary_json: Some(r#"{"ok":true}"#),
            report_path: Some("/tmp/report.json"),
            csv_path: Some("/tmp/report.csv"),
        })
        .await
        .unwrap();

    let artifact_body = json_body(
        app.clone()
            .oneshot(auth_get(
                &format!("/api/research/artifacts/{}", artifact.id),
                &observer,
            ))
            .await
            .unwrap(),
    )
    .await;
    let report_body = json_body(
        app.oneshot(auth_get(
            &format!("/api/research/reports/{}", report.id),
            &admin,
        ))
        .await
        .unwrap(),
    )
    .await;

    assert_eq!(artifact_body["id"], artifact.id);
    assert_eq!(artifact_body["source_machine_id"], "live");
    assert_eq!(report_body["id"], report.id);
    assert_eq!(report_body["artifact_id"], artifact.id);
    assert_eq!(report_body["title"], "Detail report");
}

/// Verifies report metadata and file lifecycle endpoints.
#[tokio::test]
async fn admin_can_update_archive_restore_read_and_delete_report_files() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let fixture = report_file_fixture(&db).await;
    let report = &fixture.report;

    let json_file = json_body(
        app.clone()
            .oneshot(auth_get(
                &format!("/api/research/reports/{}/json", report.id),
                &admin,
            ))
            .await
            .unwrap(),
    )
    .await;
    let csv_response = app
        .clone()
        .oneshot(auth_get(
            &format!("/api/research/reports/{}/csv", report.id),
            &admin,
        ))
        .await
        .unwrap();
    let patched = json_body(
        app.clone()
            .oneshot(auth_json_patch(
                &format!("/api/research/reports/{}", report.id),
                &admin,
                &serde_json::json!({"title": "Renamed report"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let archived = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/reports/{}/archive", report.id),
                &admin,
            ))
            .await
            .unwrap(),
    )
    .await;
    let restored = json_body(
        app.clone()
            .oneshot(auth_post(
                &format!("/api/research/reports/{}/restore", report.id),
                &admin,
            ))
            .await
            .unwrap(),
    )
    .await;
    let deleted = json_body(
        app.clone()
            .oneshot(auth_delete(
                &format!("/api/research/reports/{}?delete_files=true", report.id),
                &admin,
            ))
            .await
            .unwrap(),
    )
    .await;
    let report_id = report.id.clone();
    let missing = app
        .oneshot(auth_get(
            &format!("/api/research/reports/{}", report.id),
            &admin,
        ))
        .await
        .unwrap();

    assert_eq!(json_file["equity"], serde_json::json!([1, 2, 3]));
    assert_eq!(
        text_body(csv_response).await,
        "step,status\nwrite_report,completed\n"
    );
    assert_eq!(patched["title"], "Renamed report");
    assert_eq!(archived["status"], "archived");
    assert_eq!(restored["status"], "available");
    assert_eq!(deleted["id"], report_id);
    assert!(!fixture.report_json_path.exists());
    assert!(!fixture.report_csv_path.exists());
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// Verifies observers cannot mutate report lifecycle state.
#[tokio::test]
async fn observer_cannot_mutate_reports() {
    let (app, db) = test_app();
    let observer = observer_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    let admin_hash = hash_password("pass").unwrap();
    let admin_user = db.create_user("admin", &admin_hash, "admin").await.unwrap();
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &admin_user.id,
            0,
            None,
        )
        .await
        .unwrap();
    let report = db
        .create_or_update_research_report(&crate::db::ResearchReportRecord {
            job_id: &job.id,
            artifact_id: Some(&artifact.id),
            title: "Report",
            status: "available",
            summary_json: Some(r#"{"ok":true}"#),
            report_path: Some("/tmp/report.json"),
            csv_path: Some("/tmp/report.csv"),
        })
        .await
        .unwrap();

    let patch = app
        .clone()
        .oneshot(auth_json_patch(
            &format!("/api/research/reports/{}", report.id),
            &observer,
            &serde_json::json!({"title": "Nope"}),
        ))
        .await
        .unwrap();
    let archive = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/reports/{}/archive", report.id),
            &observer,
        ))
        .await
        .unwrap();
    let restore = app
        .clone()
        .oneshot(auth_post(
            &format!("/api/research/reports/{}/restore", report.id),
            &observer,
        ))
        .await
        .unwrap();
    let delete = app
        .oneshot(auth_delete(
            &format!("/api/research/reports/{}", report.id),
            &observer,
        ))
        .await
        .unwrap();

    assert_eq!(patch.status(), StatusCode::FORBIDDEN);
    assert_eq!(archive.status(), StatusCode::FORBIDDEN);
    assert_eq!(restore.status(), StatusCode::FORBIDDEN);
    assert_eq!(delete.status(), StatusCode::FORBIDDEN);
}

/// Verifies report file routes reject stored paths outside the work root.
#[tokio::test]
async fn report_file_routes_reject_paths_outside_work_root() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    let admin_user = db.get_user_by_username("admin").await.unwrap().unwrap();
    let job = db
        .create_research_job(
            "current_params",
            Some(&artifact.id),
            &admin_user.id,
            0,
            None,
        )
        .await
        .unwrap();
    let report = db
        .create_or_update_research_report(&crate::db::ResearchReportRecord {
            job_id: &job.id,
            artifact_id: Some(&artifact.id),
            title: "Unsafe report",
            status: "available",
            summary_json: Some(r#"{"ok":true}"#),
            report_path: Some("/etc/passwd"),
            csv_path: Some("/etc/passwd"),
        })
        .await
        .unwrap();

    let read = app
        .clone()
        .oneshot(auth_get(
            &format!("/api/research/reports/{}/json", report.id),
            &admin,
        ))
        .await
        .unwrap();
    let delete = app
        .clone()
        .oneshot(auth_delete(
            &format!("/api/research/reports/{}?delete_files=true", report.id),
            &admin,
        ))
        .await
        .unwrap();
    let still_present = app
        .oneshot(auth_get(
            &format!("/api/research/reports/{}", report.id),
            &admin,
        ))
        .await
        .unwrap();

    assert_eq!(read.status(), StatusCode::BAD_REQUEST);
    assert_eq!(delete.status(), StatusCode::BAD_REQUEST);
    assert_eq!(still_present.status(), StatusCode::OK);
}

/// Verifies a present-but-malformed report JSON file yields a client error, not a 500.
#[tokio::test]
async fn corrupt_report_json_file_yields_bad_request() {
    let (app, db) = test_app();
    let admin = admin_token(&db).await;
    let fixture = report_file_fixture(&db).await;
    std::fs::write(&fixture.report_json_path, b"{not valid json").unwrap();

    let resp = app
        .oneshot(auth_get(
            &format!("/api/research/reports/{}/json", fixture.report.id),
            &admin,
        ))
        .await
        .unwrap();
    let status = resp.status();
    let body = json_body(resp).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
    let message = body["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("report JSON file is corrupt"),
        "expected corrupt-file message, got: {message}"
    );
    assert!(
        message.contains(&fixture.report.id),
        "expected report id in message, got: {message}"
    );
}
