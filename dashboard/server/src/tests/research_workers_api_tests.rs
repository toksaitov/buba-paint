use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::middleware;
use axum::routing::{get, post, put};
use rusqlite::Connection;

use crate::api::auth_routes::AppState;
use crate::api::research_workers;
use crate::auth::{self, AuthState};
use crate::db::{ArtifactTransferRecord, DashboardDb};
use crate::research_backend::ResearchWorkBackend;
use crate::research_controller_client::{ResearchControllerClient, WorkerBackend};
use crate::research_worker::LocalResearchWorker;

/// Worker route whose handler intentionally omits the token helper, proving the
/// middleware layer guards the subtree structurally.
async fn unguarded_worker_probe() -> &'static str {
    "ok"
}

/// Operator route guarded only by the JWT layer, used to prove operator auth still applies.
async fn operator_probe() -> &'static str {
    "ok"
}

static WORK_ROOT_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Shared worker token used by every protocol test.
const TEST_TOKEN: &str = "protocol-test-token";

/// Build a unique on-disk work root for one protocol test.
fn unique_work_root() -> std::path::PathBuf {
    let counter = WORK_ROOT_COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "buba-research-workers-test-{}-{counter}",
        std::process::id()
    ))
}

/// Build the worker protocol router used by these tests.
fn worker_protocol_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/workers/steps/claim",
            post(research_workers::claim_step),
        )
        .route(
            "/api/research/workers/steps/{id}/renew",
            post(research_workers::renew_step_lease),
        )
        .route(
            "/api/research/workers/steps/{id}/run",
            post(research_workers::mark_step_running),
        )
        .route(
            "/api/research/workers/steps/{id}/complete",
            post(research_workers::complete_step),
        )
        .route(
            "/api/research/workers/steps/{id}/fail",
            post(research_workers::fail_step),
        )
        .route(
            "/api/research/workers/steps/{id}/block",
            post(research_workers::block_step),
        )
        .route(
            "/api/research/workers/jobs/{id}",
            get(research_workers::get_job),
        )
        .route(
            "/api/research/workers/jobs/{id}/cancel",
            post(research_workers::cancel_job),
        )
        .route(
            "/api/research/workers/jobs/{id}/steps",
            get(research_workers::get_job_steps),
        )
        .route(
            "/api/research/workers/jobs/{id}/events",
            post(research_workers::append_job_event),
        )
        .route(
            "/api/research/workers/jobs/{job_id}/artifact/{artifact_id}",
            post(research_workers::attach_job_artifact),
        )
        .route(
            "/api/research/workers/artifacts",
            post(research_workers::upsert_artifact),
        )
        .route(
            "/api/research/workers/artifacts/{id}",
            get(research_workers::get_artifact),
        )
        .route(
            "/api/research/workers/artifacts/{id}/documents",
            put(research_workers::store_artifact_documents),
        )
        .route(
            "/api/research/workers/reports",
            post(research_workers::upsert_report),
        )
        .route(
            "/api/research/workers/reports/{id}/documents",
            put(research_workers::store_report_documents),
        )
        .route(
            "/api/research/workers/transfers/claim",
            post(research_workers::claim_transfer),
        )
        .route(
            "/api/research/workers/transfers/{id}",
            get(research_workers::get_transfer),
        )
        .route(
            "/api/research/workers/transfers/{id}/progress",
            post(research_workers::update_transfer_progress),
        )
        .route(
            "/api/research/workers/transfers/recover",
            post(research_workers::recover_stale_transfers),
        )
        .route(
            "/api/research/workers/machines/{id}",
            get(research_workers::get_machine),
        )
}

/// Spawn an in-process controller and return its base URL plus its database.
async fn spawn_controller(work_root: &std::path::Path) -> (String, Arc<DashboardDb>) {
    let db = Arc::new(DashboardDb::from_connection(
        Connection::open_in_memory().unwrap(),
    ));
    let state = AppState {
        db: Arc::clone(&db),
        jwt_secret: "protocol-test-jwt".to_string(),
        research_worker_token: Some(TEST_TOKEN.to_string()),
        research_work_root: Some(work_root.to_string_lossy().to_string()),
        agents: vec![],
    };
    let auth_state = AuthState {
        jwt_secret: "protocol-test-jwt".to_string(),
        db: Arc::clone(&db),
    };
    let operator_routes = Router::new()
        .route("/api/research/operator-probe", get(operator_probe))
        .layer(middleware::from_fn(auth::require_auth))
        .layer(axum::Extension(auth_state));
    let worker_routes = worker_protocol_routes()
        .route(
            "/api/research/workers/unguarded-probe",
            get(unguarded_worker_probe),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_worker_auth,
        ));
    let app = operator_routes.merge(worker_routes).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), db)
}

/// Seed one current-params job with an artifact and return its ID.
async fn seed_backtest_job(db: &DashboardDb) -> String {
    db.upsert_research_artifact(&crate::db::ResearchArtifactRecord {
        id: "artifact-1",
        source_machine_id: Some("live"),
        kind: "readonly_run",
        status: "available",
        run_mode: Some("live_readonly"),
        artifact_root: None,
        manifest_path: None,
        bundle_path: None,
        source_db_path: None,
        interval_start_ms: Some(1_000),
        interval_end_ms: Some(2_000),
        bytes: Some(10),
        checksum: None,
        replay_quality_class: None,
        backtest_ready_class: None,
        live_fidelity_class: None,
    })
    .await
    .unwrap();
    let admin = db
        .create_user("protocol-admin", "unused-password-hash", "admin")
        .await
        .unwrap();
    let job = db
        .create_research_job("current_params", Some("artifact-1"), &admin.id, 0, None)
        .await
        .unwrap();
    job.id
}

/// Verifies worker endpoints reject requests without the worker token.
#[tokio::test]
async fn worker_protocol_rejects_missing_token() {
    let work_root = unique_work_root();
    let (base_url, _db) = spawn_controller(&work_root).await;
    let response = reqwest::Client::new()
        .post(format!("{base_url}/api/research/workers/steps/claim"))
        .json(&serde_json::json!({"worker_id": "w", "lease_duration_ms": 1000}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

/// Verifies artifact and report upserts reject path-unsafe ids with 400.
#[tokio::test]
async fn worker_protocol_rejects_unsafe_upsert_ids() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    let job_id = seed_backtest_job(&db).await;
    let client = reqwest::Client::new();

    for unsafe_id in ["../escape", ".hidden", "a/b"] {
        let artifact_response = client
            .post(format!("{base_url}/api/research/workers/artifacts"))
            .header("x-buba-research-worker-token", TEST_TOKEN)
            .json(&serde_json::json!({
                "id": unsafe_id,
                "kind": "readonly_run",
                "status": "available",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            artifact_response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "artifact upsert should reject id {unsafe_id}"
        );

        let report_response = client
            .post(format!("{base_url}/api/research/workers/reports"))
            .header("x-buba-research-worker-token", TEST_TOKEN)
            .json(&serde_json::json!({
                "job_id": job_id,
                "artifact_id": unsafe_id,
                "title": "Protocol test report",
                "status": "available",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            report_response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "report upsert should reject artifact_id {unsafe_id}"
        );
    }

    assert!(db.list_research_reports().await.unwrap().is_empty());
    std::fs::remove_dir_all(&work_root).ok();
}

/// Verifies an HTTP claim returns 204 when the queue is empty.
#[tokio::test]
async fn worker_protocol_claim_returns_none_on_empty_queue() {
    let work_root = unique_work_root();
    let (base_url, _db) = spawn_controller(&work_root).await;
    let client = ResearchControllerClient::new(&base_url, TEST_TOKEN).unwrap();
    let lease = client
        .lease_next_research_step("worker-a", 1_000)
        .await
        .unwrap();
    assert!(lease.is_none());
}

/// Verifies the full claim, run, renew, event, and complete cycle over HTTP.
#[tokio::test]
async fn worker_protocol_supports_full_step_lifecycle() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    let job_id = seed_backtest_job(&db).await;
    let client = ResearchControllerClient::new(&base_url, TEST_TOKEN).unwrap();

    let lease = client
        .lease_next_research_step("worker-a", 60_000)
        .await
        .unwrap()
        .expect("first step should lease");
    assert_eq!(lease.job.id, job_id);

    let running = client
        .mark_research_step_running(&lease.step.id, "worker-a")
        .await
        .unwrap();
    assert_eq!(running.status, "running");

    let renewed = client
        .refresh_research_step_lease(&lease.step.id, "worker-a", 90_000)
        .await
        .unwrap();
    assert_eq!(renewed.lease_owner.as_deref(), Some("worker-a"));

    client
        .append_research_job_event(&job_id, Some(&lease.step.id), "info", "protocol test", None)
        .await
        .unwrap();

    let completed = client
        .complete_research_step(&lease.step.id, "worker-a", Some("{\"ok\":true}"))
        .await
        .unwrap();
    assert_eq!(completed.status, "completed");

    let job = client.get_research_job(&job_id).await.unwrap().unwrap();
    assert_eq!(job.status, "running");
    let steps = client.get_research_job_steps(&job_id).await.unwrap();
    assert_eq!(steps.first().unwrap().status, "completed");
}

/// Verifies fail and block transitions over HTTP.
#[tokio::test]
async fn worker_protocol_supports_fail_and_block() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    seed_backtest_job(&db).await;
    let client = ResearchControllerClient::new(&base_url, TEST_TOKEN).unwrap();

    let first = client
        .lease_next_research_step("worker-a", 60_000)
        .await
        .unwrap()
        .expect("step should lease");
    let failed = client
        .fail_research_step(&first.step.id, "worker-a", "boom", true)
        .await
        .unwrap();
    assert_eq!(failed.status, "retryable");

    let again = client
        .lease_next_research_step("worker-a", 60_000)
        .await
        .unwrap()
        .expect("retryable step should lease again");
    assert_eq!(again.step.id, first.step.id);
    let blocked = client
        .block_research_step(&again.step.id, "worker-a", "needs operator")
        .await
        .unwrap();
    assert_eq!(blocked.status, "blocked");
}

/// Verifies job cancellation polling works through the client.
#[tokio::test]
async fn worker_protocol_supports_job_cancellation() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    let job_id = seed_backtest_job(&db).await;
    let client = ResearchControllerClient::new(&base_url, TEST_TOKEN).unwrap();
    let cancelled = client.cancel_research_job(&job_id).await.unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let fetched = client.get_research_job(&job_id).await.unwrap().unwrap();
    assert_eq!(fetched.status, "cancelled");
}

/// Verifies report metadata and document upload land on the controller.
#[tokio::test]
async fn worker_protocol_uploads_report_documents() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    let job_id = seed_backtest_job(&db).await;
    let client = ResearchControllerClient::new(&base_url, TEST_TOKEN).unwrap();

    let report = client
        .create_or_update_research_report(&crate::db::ResearchReportRecord {
            job_id: &job_id,
            artifact_id: Some("artifact-1"),
            title: "Protocol test report",
            status: "available",
            summary_json: Some("{\"net_pnl\":1.5}"),
            report_path: Some("/worker/local/report.json"),
            csv_path: Some("/worker/local/report.csv"),
        })
        .await
        .unwrap();

    client
        .store_research_report_documents(&report.id, "{\"net_pnl\":1.5}", "metric,value\n")
        .await
        .unwrap();

    let stored = db.get_research_report(&report.id).await.unwrap().unwrap();
    let report_path = stored.report_path.expect("report path set");
    let csv_path = stored.csv_path.expect("csv path set");
    assert!(report_path.starts_with(work_root.to_string_lossy().as_ref()));
    assert_eq!(
        std::fs::read_to_string(&report_path).unwrap(),
        "{\"net_pnl\":1.5}"
    );
    assert_eq!(
        std::fs::read_to_string(&csv_path).unwrap(),
        "metric,value\n"
    );
    std::fs::remove_dir_all(&work_root).ok();
}

/// Verifies artifact document upload writes controller-rooted files.
#[tokio::test]
async fn worker_protocol_uploads_artifact_documents() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    seed_backtest_job(&db).await;
    let client = ResearchControllerClient::new(&base_url, TEST_TOKEN).unwrap();

    client
        .store_research_artifact_documents(
            "artifact-1",
            Some("{\"files\":[]}"),
            Some("abc  manifest.json\n"),
        )
        .await
        .unwrap();

    let stored = db
        .get_research_artifact("artifact-1")
        .await
        .unwrap()
        .unwrap();
    let manifest_path = stored.manifest_path.expect("manifest path set");
    assert!(manifest_path.starts_with(work_root.to_string_lossy().as_ref()));
    assert_eq!(
        std::fs::read_to_string(&manifest_path).unwrap(),
        "{\"files\":[]}"
    );
    std::fs::remove_dir_all(&work_root).ok();
}

/// Verifies transfer claim, progress, and recovery over HTTP.
#[tokio::test]
async fn worker_protocol_supports_transfer_lifecycle() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    seed_backtest_job(&db).await;
    let transfer = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: "artifact-1",
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(100),
        })
        .await
        .unwrap();
    let client = ResearchControllerClient::new(&base_url, TEST_TOKEN).unwrap();

    let claimed = client
        .claim_next_artifact_transfer("research")
        .await
        .unwrap()
        .expect("queued transfer should claim");
    assert_eq!(claimed.id, transfer.id);

    let progressed = client
        .update_artifact_transfer_progress(&transfer.id, "running", Some(40), Some(100), None, None)
        .await
        .unwrap();
    assert_eq!(progressed.bytes_done, 40);

    let fetched = client
        .get_artifact_transfer(&transfer.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.status, "running");

    let recovered = client
        .recover_stale_artifact_transfers("research", 1)
        .await
        .unwrap();
    assert!(recovered <= 1);

    let machine = client
        .get_research_machine("research")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(machine.id, "research");
}

/// Verifies the worker loop completes queued steps through the remote backend.
#[tokio::test]
async fn worker_loop_runs_noop_steps_through_remote_backend() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    let job_id = seed_backtest_job(&db).await;
    let client = ResearchControllerClient::new(&base_url, TEST_TOKEN).unwrap();
    let backend = WorkerBackend::Remote(client);
    let worker = LocalResearchWorker::new("remote-worker", 60_000).unwrap();

    let processed = worker.run_noop_until_idle(&backend, 32).await.unwrap();
    assert!(processed >= 1);

    let job = db.get_research_job(&job_id).await.unwrap().unwrap();
    assert_eq!(job.status, "completed");
    let steps = db.get_research_job_steps(&job_id).await.unwrap();
    assert!(steps.iter().all(|step| step.status == "completed"));
}

/// Verifies the worker-token middleware guards the whole worker subtree even when
/// a handler omits the helper, and that operator JWT routes still require a JWT.
#[tokio::test]
async fn worker_subtree_is_guarded_structurally() {
    let work_root = unique_work_root();
    let (base_url, db) = spawn_controller(&work_root).await;
    let client = reqwest::Client::new();

    let missing = client
        .get(format!("{base_url}/api/research/workers/unguarded-probe"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::UNAUTHORIZED);

    let invalid = client
        .get(format!("{base_url}/api/research/workers/unguarded-probe"))
        .header("x-buba-research-worker-token", "wrong-token")
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), reqwest::StatusCode::UNAUTHORIZED);

    let valid = client
        .get(format!("{base_url}/api/research/workers/unguarded-probe"))
        .header("x-buba-research-worker-token", TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(valid.status(), reqwest::StatusCode::OK);

    let operator_no_jwt = client
        .get(format!("{base_url}/api/research/operator-probe"))
        .send()
        .await
        .unwrap();
    assert_eq!(operator_no_jwt.status(), reqwest::StatusCode::UNAUTHORIZED);

    let token = {
        let admin = db
            .create_user("probe-admin", "unused-password-hash", "admin")
            .await
            .unwrap();
        crate::auth::create_jwt(&admin.id, "admin", "protocol-test-jwt", 3600)
    };
    let operator_with_jwt = client
        .get(format!("{base_url}/api/research/operator-probe"))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(operator_with_jwt.status(), reqwest::StatusCode::OK);
}
