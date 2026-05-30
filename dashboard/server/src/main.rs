use std::sync::Arc;

use axum::Router;
use axum::middleware;
use axum::routing::{get, post};
use clap::Parser;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use buba_dashboard::api::auth_routes::{self, AppState};
use buba_dashboard::api::bots;
use buba_dashboard::api::research;
use buba_dashboard::api::ws_proxy;
use buba_dashboard::auth::{self, AuthState, hash_password};
use buba_dashboard::config::DashboardConfig;
use buba_dashboard::db::DashboardDb;

#[derive(Parser)]
#[command(name = "buba-dashboard", version)]
struct Cli {
    /// Path to TOML configuration file
    #[arg(long, default_value = "dashboard.toml")]
    config: String,

    /// Port to listen on (overrides config)
    #[arg(long)]
    port: Option<u16>,

    /// Path to serve static files from (built frontend)
    #[arg(long)]
    static_dir: Option<String>,
}

/// Starts the dashboard server with the configured agents, auth, and routes.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = DashboardConfig::from_file(&cli.config)?;
    let port = cli.port.unwrap_or(config.server.port);

    let db_path = std::env::var("DASHBOARD_DB_PATH").unwrap_or_else(|_| "dashboard.db".to_string());
    let db = Arc::new(DashboardDb::new(&db_path)?);

    seed_admin_from_env(&db).await?;

    let state = AppState {
        db: Arc::clone(&db),
        jwt_secret: config.server.jwt_secret.clone(),
        research_worker_token: optional_env("BUBA_RESEARCH_WORKER_TOKEN"),
        research_work_root: optional_env("BUBA_RESEARCH_WORK_ROOT"),
        agents: config.agents.clone(),
    };

    let auth_state = AuthState {
        jwt_secret: config.server.jwt_secret.clone(),
        db,
    };

    let mut app = authenticated_app(state, auth_state);

    if let Some(dir) = cli.static_dir {
        let index = std::path::PathBuf::from(&dir).join("index.html");
        app = app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("buba-dashboard listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Seeds the configured admin user when both admin env vars are present.
async fn seed_admin_from_env(db: &DashboardDb) -> anyhow::Result<()> {
    if let (Ok(user), Ok(pass)) = (std::env::var("ADMIN_USER"), std::env::var("ADMIN_PASSWORD")) {
        let hash =
            hash_password(&pass).map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?;
        db.seed_admin(&user, &hash).await?;
    }
    Ok(())
}

/// Builds the authenticated dashboard route graph.
fn authenticated_app(state: AppState, auth_state: AuthState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/auth/login", post(auth_routes::login))
        .route("/api/auth/me", get(auth_routes::me))
        .route("/api/users", post(auth_routes::create_user))
        .route("/api/users", get(auth_routes::list_users))
        .merge(research_routes())
        .merge(bot_routes())
        .layer(middleware::from_fn(auth::require_auth))
        .layer(axum::Extension(auth_state))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Builds research orchestration routes.
fn research_routes() -> Router<AppState> {
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
        .route(
            "/api/research/workers/heartbeat",
            post(research::worker_heartbeat),
        )
        .merge(research_artifact_routes())
        .merge(research_transfer_routes())
        .merge(research_job_routes())
        .merge(research_report_routes())
}

/// Builds artifact research routes.
fn research_artifact_routes() -> Router<AppState> {
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

/// Builds artifact transfer research routes.
fn research_transfer_routes() -> Router<AppState> {
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

/// Builds job research routes.
fn research_job_routes() -> Router<AppState> {
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

/// Builds report research routes.
fn research_report_routes() -> Router<AppState> {
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

/// Builds bot and websocket proxy routes.
fn bot_routes() -> Router<AppState> {
    Router::new()
        .route("/api/bots", get(bots::list_bots))
        .route("/api/bots/{id}/status", get(bots::bot_status))
        .route("/api/bots/{id}/trades", get(bots::bot_trades))
        .route("/api/bots/{id}/balance", get(bots::bot_balance))
        .route("/api/bots/{id}/equity/series", get(bots::bot_equity_series))
        .route("/api/bots/{id}/signals", get(bots::bot_signals))
        .route(
            "/api/bots/{id}/signals/groups",
            get(bots::bot_signal_groups),
        )
        .route("/api/bots/{id}/stats", get(bots::bot_stats))
        .route(
            "/api/bots/{id}/trading/summary",
            get(bots::bot_trading_summary),
        )
        .route("/api/bots/{id}/config", get(bots::bot_runtime_config))
        .route("/api/bots/{id}/machine", get(bots::bot_machine))
        .route("/api/bots/{id}/live/status", get(bots::bot_live_status))
        .route("/api/bots/{id}/live/sessions", get(bots::bot_live_sessions))
        .route("/api/bots/{id}/live/orders", get(bots::bot_live_orders))
        .route("/api/bots/{id}/live/fills", get(bots::bot_live_fills))
        .route(
            "/api/bots/{id}/live/redemptions",
            get(bots::bot_live_redemptions),
        )
        .route(
            "/api/bots/{id}/live/reconciliation",
            get(bots::bot_live_reconciliation),
        )
        .route(
            "/api/bots/{id}/live/control-audit",
            get(bots::bot_live_control_audit),
        )
        .route("/api/bots/{id}/live/control", post(bots::bot_live_control))
        .route("/api/bots/{id}/logs", get(bots::bot_logs))
        .route("/api/bots/{id}/process", get(bots::bot_process_status))
        .route("/api/bots/{id}/start", post(bots::bot_start))
        .route("/api/bots/{id}/stop", post(bots::bot_stop))
        .route("/api/bots/{id}/restart", post(bots::bot_restart))
        .route("/ws/bots/{id}", get(ws_proxy::ws_proxy))
}

/// Waits for a process shutdown signal.
async fn shutdown_signal() {
    tokio::select! {
        () = async { let _ = tokio::signal::ctrl_c().await; } => {},
        () = sigterm_future() => {},
    }
}

/// Waits for `SIGTERM` on Unix platforms.
#[cfg(unix)]
async fn sigterm_future() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut signal) => {
            signal.recv().await;
        }
        Err(error) => {
            tracing::warn!(?error, "failed to register SIGTERM handler");
            std::future::pending::<()>().await;
        }
    }
}

/// Waits forever on platforms without Unix signals.
#[cfg(not(unix))]
async fn sigterm_future() {
    std::future::pending::<()>().await
}

/// Returns a simple liveness payload for load balancers and smoke tests.
#[allow(clippy::unused_async)]
async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "ok": true }))
}

/// Read a non-empty environment value as an optional setting.
fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
