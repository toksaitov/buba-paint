use std::sync::Arc;

use axum::Router;
use axum::middleware;
use axum::routing::{get, post};
use clap::Parser;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use buba_dashboard::api::auth_routes::{self, AppState};
use buba_dashboard::api::bots;
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

    if let (Ok(user), Ok(pass)) = (std::env::var("ADMIN_USER"), std::env::var("ADMIN_PASSWORD")) {
        let hash =
            hash_password(&pass).map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?;
        db.seed_admin(&user, &hash).await?;
    }

    let state = AppState {
        db: Arc::clone(&db),
        jwt_secret: config.server.jwt_secret.clone(),
        agents: config.agents.clone(),
    };

    let auth_state = AuthState {
        jwt_secret: config.server.jwt_secret.clone(),
        db,
    };

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/api/auth/login", post(auth_routes::login))
        .route("/api/auth/me", get(auth_routes::me))
        .route("/api/users", post(auth_routes::create_user))
        .route("/api/users", get(auth_routes::list_users))
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
        .layer(middleware::from_fn(auth::require_auth))
        .layer(axum::Extension(auth_state))
        .layer(CorsLayer::permissive())
        .with_state(state);

    if let Some(dir) = cli.static_dir {
        let index = std::path::PathBuf::from(&dir).join("index.html");
        app = app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("buba-dashboard listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Returns a simple liveness payload for load balancers and smoke tests.
#[allow(clippy::unused_async)]
async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "ok": true }))
}
