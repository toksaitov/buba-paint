use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::Mutex;

use crate::error::DashboardError;

/// Dashboard database — manages users and sessions.
pub struct DashboardDb {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub created_at: u64,
    pub expires_at: u64,
}

/// Dashboard record for a machine that can participate in research workflows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResearchMachine {
    /// Stable machine ID, for example `live` or `research`.
    pub id: String,
    /// Operator-facing machine name.
    pub name: String,
    /// Machine role: `live`, `research`, or `controller`.
    pub role: String,
    /// Optional SSH alias used by deployment scripts and operators.
    pub ssh_alias: Option<String>,
    /// Current readiness state for dashboard display.
    pub status: String,
    /// Optional serialized machine details collected by health checks.
    pub details_json: Option<String>,
    /// Creation time in milliseconds since the Unix epoch.
    pub created_at: u64,
    /// Last update time in milliseconds since the Unix epoch.
    pub updated_at: u64,
}

/// Input record used to create or replace research machine metadata.
pub struct ResearchMachineRecord<'a> {
    /// Stable machine ID used by APIs, workers, and deployment inventory.
    pub id: &'a str,
    /// Operator-facing display name.
    pub name: &'a str,
    /// Machine role: `live`, `research`, or `controller`.
    pub role: &'a str,
    /// Optional SSH alias used by operators and deployment scripts.
    pub ssh_alias: Option<&'a str>,
    /// Current lifecycle or readiness status.
    pub status: &'a str,
    /// Optional structured details serialized as JSON.
    pub details_json: Option<&'a str>,
}

/// Counts of durable research records that reference one machine.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResearchMachineDependencyCounts {
    /// Artifacts whose source is this machine.
    pub artifacts: u64,
    /// Transfers that read from this machine.
    pub transfers_as_source: u64,
    /// Transfers that write to this machine.
    pub transfers_as_destination: u64,
    /// Non-terminal transfers involving this machine.
    pub active_transfers: u64,
    /// Jobs whose attached artifact came from this machine.
    pub jobs_using_source_artifacts: u64,
    /// Reports whose attached artifact came from this machine.
    pub reports_using_source_artifacts: u64,
}

/// Persisted metadata for one exported runtime artifact.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResearchArtifact {
    /// Stable artifact ID.
    pub id: String,
    /// Machine that produced the artifact.
    pub source_machine_id: Option<String>,
    /// Artifact class, such as `readonly_run` or `readonly_run_snapshot`.
    pub kind: String,
    /// Artifact lifecycle status.
    pub status: String,
    /// Source run mode used to produce the artifact.
    pub run_mode: Option<String>,
    /// Local directory containing the artifact files.
    pub artifact_root: Option<String>,
    /// Path to the artifact `manifest.json`.
    pub manifest_path: Option<String>,
    /// Optional bundled archive path for future transfer implementations.
    pub bundle_path: Option<String>,
    /// Original runtime database path used to create the artifact.
    pub source_db_path: Option<String>,
    /// Optional replay interval start in milliseconds.
    pub interval_start_ms: Option<u64>,
    /// Optional replay interval end in milliseconds.
    pub interval_end_ms: Option<u64>,
    /// Total artifact payload bytes.
    pub bytes: Option<u64>,
    /// Artifact checksum or checksum-sidecar digest.
    pub checksum: Option<String>,
    /// Replay validation quality class.
    pub replay_quality_class: Option<String>,
    /// Backtest-input validation class.
    pub backtest_ready_class: Option<String>,
    /// Fidelity class assigned when comparing live and research behavior.
    pub live_fidelity_class: Option<String>,
    /// Creation time in milliseconds since the Unix epoch.
    pub created_at: u64,
    /// Last update time in milliseconds since the Unix epoch.
    pub updated_at: u64,
    /// Archive time in milliseconds when scratch-heavy data is retired.
    pub archived_at: Option<u64>,
}

/// Borrowed input record for inserting or updating an artifact row.
#[derive(Debug, Clone)]
pub struct ResearchArtifactRecord<'a> {
    /// Stable artifact ID.
    pub id: &'a str,
    /// Machine that produced the artifact.
    pub source_machine_id: Option<&'a str>,
    /// Artifact class to store.
    pub kind: &'a str,
    /// Lifecycle status to store.
    pub status: &'a str,
    /// Source run mode to store.
    pub run_mode: Option<&'a str>,
    /// Local artifact directory.
    pub artifact_root: Option<&'a str>,
    /// Path to the artifact `manifest.json`.
    pub manifest_path: Option<&'a str>,
    /// Optional bundled archive path.
    pub bundle_path: Option<&'a str>,
    /// Original runtime database path.
    pub source_db_path: Option<&'a str>,
    /// Optional replay interval start in milliseconds.
    pub interval_start_ms: Option<u64>,
    /// Optional replay interval end in milliseconds.
    pub interval_end_ms: Option<u64>,
    /// Total artifact payload bytes.
    pub bytes: Option<u64>,
    /// Artifact checksum or checksum-sidecar digest.
    pub checksum: Option<&'a str>,
    /// Replay validation quality class.
    pub replay_quality_class: Option<&'a str>,
    /// Backtest-input validation class.
    pub backtest_ready_class: Option<&'a str>,
    /// Live-to-research fidelity class.
    pub live_fidelity_class: Option<&'a str>,
}

/// Borrowed input record for creating or updating a research report row.
#[derive(Debug, Clone)]
pub struct ResearchReportRecord<'a> {
    /// Job that produced this report.
    pub job_id: &'a str,
    /// Artifact used by the report, when applicable.
    pub artifact_id: Option<&'a str>,
    /// Operator-facing report title.
    pub title: &'a str,
    /// Report lifecycle status.
    pub status: &'a str,
    /// Optional serialized report summary.
    pub summary_json: Option<&'a str>,
    /// Path to the generated report `JSON`.
    pub report_path: Option<&'a str>,
    /// Path to the generated report `CSV`.
    pub csv_path: Option<&'a str>,
}

/// Persisted status for a machine-to-machine artifact transfer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArtifactTransfer {
    /// Stable transfer ID.
    pub id: String,
    /// Artifact being moved.
    pub artifact_id: String,
    /// Source machine for the transfer.
    pub source_machine_id: Option<String>,
    /// Destination machine for the transfer.
    pub dest_machine_id: Option<String>,
    /// Transfer lifecycle status.
    pub status: String,
    /// Expected total bytes, when known.
    pub bytes_total: Option<u64>,
    /// Bytes successfully transferred so far.
    pub bytes_done: u64,
    /// Checksum verification status, when known.
    pub checksum_status: Option<String>,
    /// Last transfer error, when any.
    pub error: Option<String>,
    /// Creation time in milliseconds since the Unix epoch.
    pub created_at: u64,
    /// Last update time in milliseconds since the Unix epoch.
    pub updated_at: u64,
    /// Completion time in milliseconds, when complete.
    pub completed_at: Option<u64>,
}

/// Borrowed input record for creating one artifact transfer.
#[derive(Debug, Clone)]
pub struct ArtifactTransferRecord<'a> {
    /// Artifact being moved.
    pub artifact_id: &'a str,
    /// Source machine for the transfer.
    pub source_machine_id: Option<&'a str>,
    /// Destination machine for the transfer.
    pub dest_machine_id: Option<&'a str>,
    /// Expected total bytes, when known.
    pub bytes_total: Option<u64>,
}

/// Tri-state update for nullable database fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullableUpdate<T> {
    /// Keep the stored value unchanged.
    Unchanged,
    /// Store NULL.
    Clear,
    /// Replace with the supplied value.
    Set(T),
}

impl<T> Default for NullableUpdate<T> {
    /// Return an unchanged update by default.
    fn default() -> Self {
        Self::Unchanged
    }
}

/// Durable research job requested from the dashboard.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResearchJob {
    /// Stable job ID.
    pub id: String,
    /// Job type, such as `export`, `current_params`, or `sweep`.
    pub job_type: String,
    /// Input artifact attached to this job, when required.
    pub artifact_id: Option<String>,
    /// Current job lifecycle status.
    pub status: String,
    /// Queue priority; larger values are leased first.
    pub priority: i64,
    /// User ID that requested the job.
    pub requested_by: String,
    /// Optional serialized job parameters.
    pub params_json: Option<String>,
    /// Creation time in milliseconds since the Unix epoch.
    pub created_at: u64,
    /// Last update time in milliseconds since the Unix epoch.
    pub updated_at: u64,
    /// Cancellation time in milliseconds, when cancelled.
    pub cancelled_at: Option<u64>,
    /// Completion time in milliseconds, when completed.
    pub completed_at: Option<u64>,
}

/// Durable step belonging to a research job.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResearchJobStep {
    /// Stable step ID.
    pub id: String,
    /// Owning job ID.
    pub job_id: String,
    /// Zero-based execution order within the job.
    pub step_index: i64,
    /// Durable step name.
    pub name: String,
    /// Current step lifecycle status.
    pub status: String,
    /// Worker currently holding the lease.
    pub lease_owner: Option<String>,
    /// Lease expiration time in milliseconds.
    pub leased_until_ms: Option<u64>,
    /// Number of times the step has been leased.
    pub attempts: i64,
    /// Optional serialized step input.
    pub input_json: Option<String>,
    /// Optional serialized step output.
    pub output_json: Option<String>,
    /// Last step error, when any.
    pub error: Option<String>,
    /// Creation time in milliseconds since the Unix epoch.
    pub created_at: u64,
    /// Last update time in milliseconds since the Unix epoch.
    pub updated_at: u64,
    /// Start time in milliseconds, when the worker marks it running.
    pub started_at: Option<u64>,
    /// Completion time in milliseconds, when complete.
    pub completed_at: Option<u64>,
}

/// Job and step returned together when a worker obtains a lease.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResearchStepLease {
    /// Job that owns the leased step.
    pub job: ResearchJob,
    /// Step leased to the worker.
    pub step: ResearchJobStep,
}

/// Timeline event recorded for a research job.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResearchJobEvent {
    /// Stable event ID.
    pub id: String,
    /// Owning job ID.
    pub job_id: String,
    /// Optional owning step ID.
    pub step_id: Option<String>,
    /// Event timestamp in milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
    /// Event level, such as `info`, `warn`, or `error`.
    pub level: String,
    /// Human-readable event message.
    pub message: String,
    /// Optional serialized event details.
    pub details_json: Option<String>,
}

/// Persisted report metadata produced by a completed research job.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResearchReport {
    /// Stable report ID.
    pub id: String,
    /// Job that produced this report.
    pub job_id: String,
    /// Artifact used by the report, when applicable.
    pub artifact_id: Option<String>,
    /// Operator-facing report title.
    pub title: String,
    /// Report lifecycle status.
    pub status: String,
    /// Optional serialized summary for dashboards.
    pub summary_json: Option<String>,
    /// Path to the generated report `JSON`.
    pub report_path: Option<String>,
    /// Path to the generated report `CSV`.
    pub csv_path: Option<String>,
    /// Creation time in milliseconds since the Unix epoch.
    pub created_at: u64,
    /// Last update time in milliseconds since the Unix epoch.
    pub updated_at: u64,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS users (
    id         TEXT PRIMARY KEY,
    username   TEXT NOT NULL UNIQUE,
    password   TEXT NOT NULL,
    role       TEXT NOT NULL DEFAULT 'observer' CHECK(role IN ('admin','observer')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    token      TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);

CREATE TABLE IF NOT EXISTS research_machines (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    role         TEXT NOT NULL CHECK(role IN ('live','research','controller')),
    ssh_alias    TEXT,
    status       TEXT NOT NULL,
    details_json TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS run_artifacts (
    id                   TEXT PRIMARY KEY,
    source_machine_id    TEXT REFERENCES research_machines(id),
    kind                 TEXT NOT NULL,
    status               TEXT NOT NULL,
    run_mode             TEXT,
    artifact_root        TEXT,
    manifest_path        TEXT,
    bundle_path          TEXT,
    source_db_path       TEXT,
    interval_start_ms    INTEGER,
    interval_end_ms      INTEGER,
    bytes                INTEGER,
    checksum             TEXT,
    replay_quality_class TEXT,
    backtest_ready_class TEXT,
    live_fidelity_class  TEXT,
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL,
    archived_at          INTEGER
);
CREATE INDEX IF NOT EXISTS idx_run_artifacts_source ON run_artifacts(source_machine_id, created_at);
CREATE INDEX IF NOT EXISTS idx_run_artifacts_status ON run_artifacts(status, created_at);

CREATE TABLE IF NOT EXISTS artifact_transfers (
    id                TEXT PRIMARY KEY,
    artifact_id       TEXT NOT NULL REFERENCES run_artifacts(id),
    source_machine_id TEXT REFERENCES research_machines(id),
    dest_machine_id   TEXT REFERENCES research_machines(id),
    status            TEXT NOT NULL,
    bytes_total       INTEGER,
    bytes_done        INTEGER NOT NULL DEFAULT 0,
    checksum_status   TEXT,
    error             TEXT,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    completed_at      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_artifact_transfers_artifact ON artifact_transfers(artifact_id, created_at);
CREATE INDEX IF NOT EXISTS idx_artifact_transfers_status ON artifact_transfers(status, created_at);

CREATE TABLE IF NOT EXISTS research_jobs (
    id           TEXT PRIMARY KEY,
    job_type     TEXT NOT NULL CHECK(job_type IN ('export','current_params','sweep')),
    artifact_id  TEXT REFERENCES run_artifacts(id),
    status       TEXT NOT NULL,
    priority     INTEGER NOT NULL DEFAULT 0,
    requested_by TEXT NOT NULL REFERENCES users(id),
    params_json  TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    cancelled_at INTEGER,
    completed_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_research_jobs_status ON research_jobs(status, updated_at);
CREATE INDEX IF NOT EXISTS idx_research_jobs_artifact ON research_jobs(artifact_id, created_at);

CREATE TABLE IF NOT EXISTS research_job_steps (
    id              TEXT PRIMARY KEY,
    job_id          TEXT NOT NULL REFERENCES research_jobs(id),
    step_index      INTEGER NOT NULL,
    name            TEXT NOT NULL,
    status          TEXT NOT NULL,
    lease_owner     TEXT,
    leased_until_ms INTEGER,
    attempts        INTEGER NOT NULL DEFAULT 0,
    input_json      TEXT,
    output_json     TEXT,
    error           TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    started_at      INTEGER,
    completed_at    INTEGER,
    UNIQUE(job_id, step_index)
);
CREATE INDEX IF NOT EXISTS idx_research_job_steps_job ON research_job_steps(job_id, step_index);
CREATE INDEX IF NOT EXISTS idx_research_job_steps_status ON research_job_steps(status, updated_at);

CREATE TABLE IF NOT EXISTS research_job_events (
    id           TEXT PRIMARY KEY,
    job_id       TEXT NOT NULL REFERENCES research_jobs(id),
    step_id      TEXT REFERENCES research_job_steps(id),
    timestamp_ms INTEGER NOT NULL,
    level        TEXT NOT NULL,
    message      TEXT NOT NULL,
    details_json TEXT
);
CREATE INDEX IF NOT EXISTS idx_research_job_events_job ON research_job_events(job_id, timestamp_ms);

CREATE TABLE IF NOT EXISTS research_reports (
    id           TEXT PRIMARY KEY,
    job_id       TEXT NOT NULL REFERENCES research_jobs(id),
    artifact_id  TEXT REFERENCES run_artifacts(id),
    title        TEXT NOT NULL,
    status       TEXT NOT NULL,
    summary_json TEXT,
    report_path  TEXT,
    csv_path     TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_research_reports_job ON research_reports(job_id, created_at);
";

impl DashboardDb {
    /// Open or create the dashboard database.
    pub fn new(db_path: &str) -> Result<Self, DashboardError> {
        let conn = if db_path == ":memory:" {
            Connection::open_in_memory()
        } else {
            if let Some(parent) = std::path::Path::new(db_path).parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)
                    .map_err(|e| DashboardError::Internal(format!("creating db directory: {e}")))?;
            }
            Connection::open(db_path)
        }
        .map_err(|e| DashboardError::Internal(format!("opening database: {e}")))?;

        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(DashboardError::Database)?;
        conn.execute_batch(SCHEMA)
            .map_err(DashboardError::Database)?;
        seed_default_research_machines(&conn).map_err(DashboardError::Database)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create from an existing connection (for testing).
    #[cfg(test)]
    pub fn from_connection(conn: Connection) -> Self {
        conn.execute_batch(SCHEMA).unwrap();
        seed_default_research_machines(&conn).unwrap();
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Seed an admin user if no users exist.
    pub async fn seed_admin(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<(), DashboardError> {
        let conn = self.conn.lock().await;
        let count: u64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;

        if count == 0 {
            let now = now_ms();
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO users (id, username, password, role, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 'admin', ?4, ?5)",
                params![id, username, password_hash, now, now],
            )?;
            tracing::info!("seeded admin user: {username}");
        }

        Ok(())
    }

    /// Create a new user.
    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        role: &str,
    ) -> Result<User, DashboardError> {
        let conn = self.conn.lock().await;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO users (id, username, password, role, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, username, password_hash, role, now, now],
        )?;

        Ok(User {
            id,
            username: username.to_string(),
            password_hash: password_hash.to_string(),
            role: role.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Get a user by username.
    pub async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<User>, DashboardError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT id, username, password, role, created_at, updated_at FROM users WHERE username = ?1",
            params![username],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    role: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(user) => Ok(Some(user)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DashboardError::Database(e)),
        }
    }

    /// Get a user by ID.
    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>, DashboardError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT id, username, password, role, created_at, updated_at FROM users WHERE id = ?1",
            params![id],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    role: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(user) => Ok(Some(user)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DashboardError::Database(e)),
        }
    }

    /// List all users.
    pub async fn list_users(&self) -> Result<Vec<User>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, username, password, role, created_at, updated_at FROM users ORDER BY created_at",
        )?;

        let users = stmt
            .query_map([], |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    role: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(users)
    }

    /// Create a session.
    pub async fn create_session(
        &self,
        user_id: &str,
        token: &str,
        expires_at: u64,
    ) -> Result<Session, DashboardError> {
        let conn = self.conn.lock().await;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO sessions (id, user_id, token, created_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, user_id, token, now, expires_at],
        )?;

        Ok(Session {
            id,
            user_id: user_id.to_string(),
            token: token.to_string(),
            created_at: now,
            expires_at,
        })
    }

    /// Get a session by token.
    pub async fn get_session_by_token(
        &self,
        token: &str,
    ) -> Result<Option<Session>, DashboardError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT id, user_id, token, created_at, expires_at FROM sessions WHERE token = ?1",
            params![token],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    token: row.get(2)?,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DashboardError::Database(e)),
        }
    }

    /// Delete a session by token.
    pub async fn delete_session(&self, token: &str) -> Result<(), DashboardError> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
        Ok(())
    }

    /// List configured research orchestration machines.
    pub async fn list_research_machines(&self) -> Result<Vec<ResearchMachine>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, name, role, ssh_alias, status, details_json, created_at, updated_at
             FROM research_machines
             ORDER BY CASE role WHEN 'live' THEN 0 WHEN 'research' THEN 1 ELSE 2 END, id",
        )?;
        let rows = stmt
            .query_map([], research_machine_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return one configured research orchestration machine by ID.
    pub async fn get_research_machine(
        &self,
        id: &str,
    ) -> Result<Option<ResearchMachine>, DashboardError> {
        let conn = self.conn.lock().await;
        conn.query_row(
            "SELECT id, name, role, ssh_alias, status, details_json, created_at, updated_at
             FROM research_machines
             WHERE id = ?1",
            params![id],
            research_machine_from_row,
        )
        .optional()
        .map_err(DashboardError::from)
    }

    /// Create a research orchestration machine with the current wall clock.
    pub async fn create_research_machine(
        &self,
        record: &ResearchMachineRecord<'_>,
    ) -> Result<ResearchMachine, DashboardError> {
        self.create_research_machine_at(record, now_ms()).await
    }

    /// Create a research orchestration machine at a deterministic timestamp.
    pub async fn create_research_machine_at(
        &self,
        record: &ResearchMachineRecord<'_>,
        now: u64,
    ) -> Result<ResearchMachine, DashboardError> {
        validate_research_machine_record(record)?;
        let conn = self.conn.lock().await;
        let exists = conn
            .query_row(
                "SELECT 1 FROM research_machines WHERE id = ?1",
                params![record.id],
                |_| Ok(()),
            )
            .optional()?;
        if exists.is_some() {
            return Err(DashboardError::BadRequest(format!(
                "research machine '{}' already exists",
                record.id
            )));
        }
        conn.execute(
            "INSERT INTO research_machines (
                id, name, role, ssh_alias, status, details_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id,
                record.name,
                record.role,
                record.ssh_alias,
                record.status,
                record.details_json,
                now,
                now
            ],
        )?;
        query_research_machine(&conn, record.id)
    }

    /// Replace editable research machine metadata with the current wall clock.
    pub async fn update_research_machine(
        &self,
        record: &ResearchMachineRecord<'_>,
    ) -> Result<ResearchMachine, DashboardError> {
        self.update_research_machine_at(record, now_ms()).await
    }

    /// Replace editable research machine metadata at a deterministic timestamp.
    pub async fn update_research_machine_at(
        &self,
        record: &ResearchMachineRecord<'_>,
        now: u64,
    ) -> Result<ResearchMachine, DashboardError> {
        validate_research_machine_record(record)?;
        let conn = self.conn.lock().await;
        let updated = conn.execute(
            "UPDATE research_machines
             SET name = ?2, role = ?3, ssh_alias = ?4, status = ?5,
                 details_json = ?6, updated_at = ?7
             WHERE id = ?1",
            params![
                record.id,
                record.name,
                record.role,
                record.ssh_alias,
                record.status,
                record.details_json,
                now
            ],
        )?;
        if updated == 0 {
            return Err(DashboardError::NotFound(format!(
                "research machine '{}' not found",
                record.id
            )));
        }
        query_research_machine(&conn, record.id)
    }

    /// Set one research machine lifecycle status with the current wall clock.
    pub async fn set_research_machine_status(
        &self,
        id: &str,
        status: &str,
    ) -> Result<ResearchMachine, DashboardError> {
        self.set_research_machine_status_at(id, status, now_ms())
            .await
    }

    /// Set one research machine lifecycle status at a deterministic timestamp.
    pub async fn set_research_machine_status_at(
        &self,
        id: &str,
        status: &str,
        now: u64,
    ) -> Result<ResearchMachine, DashboardError> {
        validate_research_machine_id(id)?;
        validate_research_machine_status(status)?;
        let conn = self.conn.lock().await;
        let updated = conn.execute(
            "UPDATE research_machines
             SET status = ?2, updated_at = ?3
             WHERE id = ?1",
            params![id, status, now],
        )?;
        if updated == 0 {
            return Err(DashboardError::NotFound(format!(
                "research machine '{id}' not found"
            )));
        }
        query_research_machine(&conn, id)
    }

    /// Return dependency counts used by machine health and delete guards.
    pub async fn research_machine_dependency_counts(
        &self,
        id: &str,
    ) -> Result<ResearchMachineDependencyCounts, DashboardError> {
        validate_research_machine_id(id)?;
        let conn = self.conn.lock().await;
        ensure_research_machine_exists(&conn, id)?;
        research_machine_dependency_counts(&conn, id)
    }

    /// Delete a custom research machine when no durable records reference it.
    pub async fn delete_research_machine(
        &self,
        id: &str,
    ) -> Result<ResearchMachine, DashboardError> {
        validate_research_machine_id(id)?;
        let conn = self.conn.lock().await;
        let machine = query_research_machine(&conn, id)?;
        if matches!(id, "live" | "research") {
            return Err(DashboardError::BadRequest(format!(
                "default research machine '{id}' cannot be deleted; disable it instead"
            )));
        }
        let references = research_machine_dependency_counts(&conn, id)?;
        if references.artifacts > 0
            || references.transfers_as_source > 0
            || references.transfers_as_destination > 0
            || references.jobs_using_source_artifacts > 0
            || references.reports_using_source_artifacts > 0
        {
            return Err(DashboardError::BadRequest(format!(
                "research machine '{id}' is still referenced by research state"
            )));
        }
        conn.execute("DELETE FROM research_machines WHERE id = ?1", params![id])?;
        Ok(machine)
    }

    /// Record a research worker heartbeat with the current wall clock.
    pub async fn record_research_machine_heartbeat(
        &self,
        machine_id: &str,
        worker_id: &str,
        worker_version: Option<&str>,
        status: &str,
        details: Option<serde_json::Value>,
    ) -> Result<ResearchMachine, DashboardError> {
        self.record_research_machine_heartbeat_at(
            machine_id,
            worker_id,
            worker_version,
            status,
            details,
            now_ms(),
        )
        .await
    }

    /// Record a research worker heartbeat at a deterministic timestamp.
    pub async fn record_research_machine_heartbeat_at(
        &self,
        machine_id: &str,
        worker_id: &str,
        worker_version: Option<&str>,
        status: &str,
        details: Option<serde_json::Value>,
        now: u64,
    ) -> Result<ResearchMachine, DashboardError> {
        if machine_id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "machine_id must not be empty".to_string(),
            ));
        }
        if worker_id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "worker_id must not be empty".to_string(),
            ));
        }
        if !matches!(status, "online" | "idle" | "busy" | "degraded" | "error") {
            return Err(DashboardError::BadRequest(
                "worker status must be online, idle, busy, degraded, or error".to_string(),
            ));
        }

        let conn = self.conn.lock().await;
        let payload = serde_json::json!({
            "worker_id": worker_id,
            "worker_version": worker_version,
            "last_heartbeat_ms": now,
            "heartbeat_status": status,
            "details": details.unwrap_or_else(|| serde_json::json!({})),
        });
        let details_json = serde_json::to_string(&payload)
            .map_err(|e| DashboardError::Internal(format!("serializing heartbeat: {e}")))?;
        let updated = conn.execute(
            "UPDATE research_machines
             SET status = CASE WHEN status = 'disabled' THEN status ELSE ?2 END,
                 details_json = ?3,
                 updated_at = ?4
             WHERE id = ?1",
            params![machine_id, status, details_json, now],
        )?;
        if updated == 0 {
            return Err(DashboardError::NotFound(format!(
                "research machine '{machine_id}' not found"
            )));
        }
        conn.query_row(
            "SELECT id, name, role, ssh_alias, status, details_json, created_at, updated_at
             FROM research_machines
             WHERE id = ?1",
            params![machine_id],
            research_machine_from_row,
        )
        .map_err(DashboardError::from)
    }

    /// List exported run artifacts.
    pub async fn list_research_artifacts(&self) -> Result<Vec<ResearchArtifact>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, source_machine_id, kind, status, run_mode, artifact_root, manifest_path,
                    bundle_path, source_db_path, interval_start_ms, interval_end_ms, bytes,
                    checksum, replay_quality_class, backtest_ready_class, live_fidelity_class,
                    created_at, updated_at, archived_at
             FROM run_artifacts
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([], research_artifact_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return one exported run artifact by ID.
    pub async fn get_research_artifact(
        &self,
        id: &str,
    ) -> Result<Option<ResearchArtifact>, DashboardError> {
        let conn = self.conn.lock().await;
        let artifact = conn
            .query_row(
                "SELECT id, source_machine_id, kind, status, run_mode, artifact_root, manifest_path,
                        bundle_path, source_db_path, interval_start_ms, interval_end_ms, bytes,
                        checksum, replay_quality_class, backtest_ready_class, live_fidelity_class,
                        created_at, updated_at, archived_at
                 FROM run_artifacts
                 WHERE id = ?1",
                params![id],
                research_artifact_from_row,
            )
            .optional()?;
        Ok(artifact)
    }

    /// Insert run artifact metadata.
    pub async fn create_research_artifact(
        &self,
        source_machine_id: Option<&str>,
        kind: &str,
        status: &str,
        run_mode: Option<&str>,
        manifest_path: Option<&str>,
    ) -> Result<ResearchArtifact, DashboardError> {
        let conn = self.conn.lock().await;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO run_artifacts (
                id, source_machine_id, kind, status, run_mode, manifest_path, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                source_machine_id,
                kind,
                status,
                run_mode,
                manifest_path,
                now,
                now
            ],
        )?;
        let artifact = conn.query_row(
            "SELECT id, source_machine_id, kind, status, run_mode, artifact_root, manifest_path,
                    bundle_path, source_db_path, interval_start_ms, interval_end_ms, bytes,
                    checksum, replay_quality_class, backtest_ready_class, live_fidelity_class,
                    created_at, updated_at, archived_at
             FROM run_artifacts
             WHERE id = ?1",
            params![id],
            research_artifact_from_row,
        )?;
        Ok(artifact)
    }

    /// Insert or update full run artifact metadata.
    pub async fn upsert_research_artifact(
        &self,
        record: &ResearchArtifactRecord<'_>,
    ) -> Result<ResearchArtifact, DashboardError> {
        if record.id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "artifact id must not be empty".to_string(),
            ));
        }
        if record.kind.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "artifact kind must not be empty".to_string(),
            ));
        }
        if record.status.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "artifact status must not be empty".to_string(),
            ));
        }

        let conn = self.conn.lock().await;
        let now = now_ms();
        conn.execute(
            "INSERT INTO run_artifacts (
                id, source_machine_id, kind, status, run_mode, artifact_root, manifest_path,
                bundle_path, source_db_path, interval_start_ms, interval_end_ms, bytes, checksum,
                replay_quality_class, backtest_ready_class, live_fidelity_class, created_at,
                updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(id) DO UPDATE SET
                source_machine_id = excluded.source_machine_id,
                kind = excluded.kind,
                status = excluded.status,
                run_mode = excluded.run_mode,
                artifact_root = excluded.artifact_root,
                manifest_path = excluded.manifest_path,
                bundle_path = excluded.bundle_path,
                source_db_path = excluded.source_db_path,
                interval_start_ms = excluded.interval_start_ms,
                interval_end_ms = excluded.interval_end_ms,
                bytes = excluded.bytes,
                checksum = excluded.checksum,
                replay_quality_class = excluded.replay_quality_class,
                backtest_ready_class = excluded.backtest_ready_class,
                live_fidelity_class = excluded.live_fidelity_class,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.source_machine_id,
                record.kind,
                record.status,
                record.run_mode,
                record.artifact_root,
                record.manifest_path,
                record.bundle_path,
                record.source_db_path,
                record.interval_start_ms,
                record.interval_end_ms,
                record.bytes,
                record.checksum,
                record.replay_quality_class,
                record.backtest_ready_class,
                record.live_fidelity_class,
                now,
                now
            ],
        )?;
        conn.query_row(
            "SELECT id, source_machine_id, kind, status, run_mode, artifact_root, manifest_path,
                    bundle_path, source_db_path, interval_start_ms, interval_end_ms, bytes,
                    checksum, replay_quality_class, backtest_ready_class, live_fidelity_class,
                    created_at, updated_at, archived_at
             FROM run_artifacts
             WHERE id = ?1",
            params![record.id],
            research_artifact_from_row,
        )
        .map_err(DashboardError::from)
    }

    /// Mark one research artifact archived.
    pub async fn archive_research_artifact(
        &self,
        id: &str,
    ) -> Result<ResearchArtifact, DashboardError> {
        self.set_research_artifact_archive_state(id, true).await
    }

    /// Restore one archived research artifact.
    pub async fn restore_research_artifact(
        &self,
        id: &str,
    ) -> Result<ResearchArtifact, DashboardError> {
        self.set_research_artifact_archive_state(id, false).await
    }

    /// Check that an artifact can be deleted without dangling dependencies.
    pub async fn ensure_research_artifact_deletable(
        &self,
        id: &str,
    ) -> Result<ResearchArtifact, DashboardError> {
        let conn = self.conn.lock().await;
        ensure_research_artifact_deletable(&conn, id)
    }

    /// Delete one unreferenced research artifact metadata row.
    pub async fn delete_research_artifact(
        &self,
        id: &str,
    ) -> Result<ResearchArtifact, DashboardError> {
        let conn = self.conn.lock().await;
        let artifact = ensure_research_artifact_deletable(&conn, id)?;
        conn.execute("DELETE FROM run_artifacts WHERE id = ?1", params![id])?;
        Ok(artifact)
    }

    /// Update archived state and lifecycle fields for one artifact.
    async fn set_research_artifact_archive_state(
        &self,
        id: &str,
        archived: bool,
    ) -> Result<ResearchArtifact, DashboardError> {
        let conn = self.conn.lock().await;
        let now = now_ms();
        let status = if archived { "archived" } else { "available" };
        let archived_at = archived.then_some(now);
        let changed = conn.execute(
            "UPDATE run_artifacts
             SET status = ?2, archived_at = ?3, updated_at = ?4
             WHERE id = ?1",
            params![id, status, archived_at, now],
        )?;
        if changed == 0 {
            return Err(DashboardError::NotFound(format!(
                "artifact '{id}' not found"
            )));
        }
        conn.query_row(
            "SELECT id, source_machine_id, kind, status, run_mode, artifact_root, manifest_path,
                    bundle_path, source_db_path, interval_start_ms, interval_end_ms, bytes,
                    checksum, replay_quality_class, backtest_ready_class, live_fidelity_class,
                    created_at, updated_at, archived_at
             FROM run_artifacts
             WHERE id = ?1",
            params![id],
            research_artifact_from_row,
        )
        .map_err(DashboardError::from)
    }

    /// Attach one artifact to an existing research job.
    pub async fn attach_research_job_artifact(
        &self,
        job_id: &str,
        artifact_id: &str,
    ) -> Result<ResearchJob, DashboardError> {
        let conn = self.conn.lock().await;
        let now = now_ms();
        let artifact_exists = conn
            .query_row(
                "SELECT 1 FROM run_artifacts WHERE id = ?1",
                params![artifact_id],
                |_| Ok(()),
            )
            .optional()?;
        if artifact_exists.is_none() {
            return Err(DashboardError::NotFound(format!(
                "artifact '{artifact_id}' not found"
            )));
        }
        let updated = conn.execute(
            "UPDATE research_jobs
             SET artifact_id = ?2, updated_at = ?3
             WHERE id = ?1",
            params![job_id, artifact_id, now],
        )?;
        if updated == 0 {
            return Err(DashboardError::NotFound(format!(
                "research job '{job_id}' not found"
            )));
        }
        conn.query_row(
            "SELECT id, job_type, artifact_id, status, priority, requested_by, params_json,
                    created_at, updated_at, cancelled_at, completed_at
             FROM research_jobs
             WHERE id = ?1",
            params![job_id],
            research_job_from_row,
        )
        .map_err(DashboardError::from)
    }

    /// List artifact transfers.
    pub async fn list_artifact_transfers(&self) -> Result<Vec<ArtifactTransfer>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, artifact_id, source_machine_id, dest_machine_id, status, bytes_total,
                    bytes_done, checksum_status, error, created_at, updated_at, completed_at
             FROM artifact_transfers
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([], artifact_transfer_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return one artifact transfer by ID.
    pub async fn get_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        let conn = self.conn.lock().await;
        conn.query_row(
            "SELECT id, artifact_id, source_machine_id, dest_machine_id, status, bytes_total,
                    bytes_done, checksum_status, error, created_at, updated_at, completed_at
             FROM artifact_transfers
             WHERE id = ?1",
            params![id],
            artifact_transfer_from_row,
        )
        .optional()
        .map_err(DashboardError::from)
    }

    /// Claim the oldest queued or retryable transfer for one destination machine.
    pub async fn claim_next_artifact_transfer(
        &self,
        dest_machine_id: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        if dest_machine_id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "dest_machine_id must not be empty".to_string(),
            ));
        }
        let mut conn = self.conn.lock().await;
        if !research_machine_accepts_work(&conn, dest_machine_id)? {
            return Ok(None);
        }
        let now = now_ms();
        let tx = conn.transaction()?;
        let transfer_id: Option<String> = tx
            .query_row(
                "SELECT id
                 FROM artifact_transfers
                 WHERE status IN ('queued','retryable')
                   AND (dest_machine_id IS NULL OR dest_machine_id = ?1)
                 ORDER BY created_at, id
                 LIMIT 1",
                params![dest_machine_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(transfer_id) = transfer_id else {
            tx.commit()?;
            return Ok(None);
        };
        let updated = tx.execute(
            "UPDATE artifact_transfers
             SET status = 'running', checksum_status = 'pending', error = NULL, updated_at = ?2
             WHERE id = ?1 AND status IN ('queued','retryable')",
            params![transfer_id, now],
        )?;
        tx.commit()?;
        if updated == 0 {
            return Ok(None);
        }
        query_artifact_transfer(&conn, &transfer_id).map(Some)
    }

    /// Mark stale running transfers as retryable for one destination machine.
    pub async fn recover_stale_artifact_transfers(
        &self,
        dest_machine_id: &str,
        stale_after_ms: u64,
    ) -> Result<usize, DashboardError> {
        if dest_machine_id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "dest_machine_id must not be empty".to_string(),
            ));
        }
        let conn = self.conn.lock().await;
        let now = now_ms();
        let cutoff = now.saturating_sub(stale_after_ms);
        let error = format!("transfer became stale after {stale_after_ms} ms; retrying");
        let updated = conn.execute(
            "UPDATE artifact_transfers
             SET status = 'retryable', checksum_status = 'failed', error = ?3, updated_at = ?4,
                 completed_at = NULL
             WHERE status = 'running'
               AND updated_at <= ?2
               AND (dest_machine_id IS NULL OR dest_machine_id = ?1)",
            params![dest_machine_id, cutoff, error, now],
        )?;
        Ok(updated)
    }

    /// Create one queued artifact transfer.
    pub async fn create_artifact_transfer(
        &self,
        record: &ArtifactTransferRecord<'_>,
    ) -> Result<ArtifactTransfer, DashboardError> {
        if record.artifact_id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "artifact_id must not be empty".to_string(),
            ));
        }
        if record.bytes_total == Some(0) {
            return Err(DashboardError::BadRequest(
                "bytes_total must be positive when provided".to_string(),
            ));
        }
        let conn = self.conn.lock().await;
        ensure_artifact_exists(&conn, record.artifact_id)?;
        if let Some(machine_id) = record.source_machine_id {
            ensure_research_machine_exists(&conn, machine_id)?;
        }
        if let Some(machine_id) = record.dest_machine_id {
            ensure_research_machine_exists(&conn, machine_id)?;
        }
        let now = now_ms();
        let transfer_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO artifact_transfers (
                id, artifact_id, source_machine_id, dest_machine_id, status, bytes_total,
                bytes_done, checksum_status, error, created_at, updated_at, completed_at
             ) VALUES (?1, ?2, ?3, ?4, 'queued', ?5, 0, NULL, NULL, ?6, ?6, NULL)",
            params![
                transfer_id,
                record.artifact_id,
                record.source_machine_id,
                record.dest_machine_id,
                record.bytes_total,
                now
            ],
        )?;
        query_artifact_transfer(&conn, &transfer_id)
    }

    /// Update transfer progress and status.
    pub async fn update_artifact_transfer_progress(
        &self,
        id: &str,
        status: &str,
        bytes_done: Option<u64>,
        bytes_total: Option<u64>,
        checksum_status: Option<&str>,
        error: Option<&str>,
    ) -> Result<ArtifactTransfer, DashboardError> {
        validate_transfer_status(status)?;
        if let Some(value) = checksum_status {
            validate_transfer_checksum_status(value)?;
        }
        if error.is_some_and(|value| value.trim().is_empty()) {
            return Err(DashboardError::BadRequest(
                "transfer error must not be empty".to_string(),
            ));
        }

        let conn = self.conn.lock().await;
        let existing = query_artifact_transfer(&conn, id)?;
        if matches!(existing.status.as_str(), "completed" | "cancelled")
            && existing.status != status
        {
            return Err(DashboardError::BadRequest(format!(
                "transfer '{}' is terminal with status '{}'",
                existing.id, existing.status
            )));
        }
        let new_total = bytes_total.or(existing.bytes_total);
        if new_total == Some(0) {
            return Err(DashboardError::BadRequest(
                "bytes_total must be positive when provided".to_string(),
            ));
        }
        let new_done = bytes_done.unwrap_or(existing.bytes_done);
        if new_done < existing.bytes_done {
            return Err(DashboardError::BadRequest(
                "bytes_done must not decrease".to_string(),
            ));
        }
        if let Some(total) = new_total
            && new_done > total
        {
            return Err(DashboardError::BadRequest(
                "bytes_done must not exceed bytes_total".to_string(),
            ));
        }
        if status == "completed" && checksum_status != Some("verified") {
            return Err(DashboardError::BadRequest(
                "completed transfers require checksum_status 'verified'".to_string(),
            ));
        }
        let now = now_ms();
        let completed_at = if status == "completed" {
            Some(now)
        } else {
            existing.completed_at
        };
        conn.execute(
            "UPDATE artifact_transfers
             SET status = ?2, bytes_total = ?3, bytes_done = ?4, checksum_status = ?5,
                 error = ?6, updated_at = ?7, completed_at = ?8
             WHERE id = ?1",
            params![
                id,
                status,
                new_total,
                new_done,
                checksum_status.or(existing.checksum_status.as_deref()),
                error,
                now,
                completed_at
            ],
        )?;
        query_artifact_transfer(&conn, id)
    }

    /// Cancel a queued, running, or retryable artifact transfer.
    pub async fn cancel_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<ArtifactTransfer, DashboardError> {
        let conn = self.conn.lock().await;
        let existing = query_artifact_transfer(&conn, id)?;
        if matches!(existing.status.as_str(), "completed" | "cancelled") {
            return Err(DashboardError::BadRequest(format!(
                "transfer '{}' cannot be cancelled from status '{}'",
                existing.id, existing.status
            )));
        }
        let now = now_ms();
        conn.execute(
            "UPDATE artifact_transfers
             SET status = 'cancelled', updated_at = ?2, completed_at = NULL
             WHERE id = ?1",
            params![id, now],
        )?;
        query_artifact_transfer(&conn, id)
    }

    /// Pause a queued, running, or retryable artifact transfer.
    pub async fn pause_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<ArtifactTransfer, DashboardError> {
        let conn = self.conn.lock().await;
        let existing = query_artifact_transfer(&conn, id)?;
        if existing.status == "paused" {
            return Ok(existing);
        }
        if !matches!(existing.status.as_str(), "queued" | "running" | "retryable") {
            return Err(DashboardError::BadRequest(format!(
                "transfer '{}' cannot be paused from status '{}'",
                existing.id, existing.status
            )));
        }
        let now = now_ms();
        conn.execute(
            "UPDATE artifact_transfers
             SET status = 'paused', updated_at = ?2, completed_at = NULL
             WHERE id = ?1",
            params![id, now],
        )?;
        query_artifact_transfer(&conn, id)
    }

    /// Resume a paused artifact transfer without resetting bytes done.
    pub async fn resume_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<ArtifactTransfer, DashboardError> {
        let conn = self.conn.lock().await;
        let existing = query_artifact_transfer(&conn, id)?;
        if existing.status != "paused" {
            return Err(DashboardError::BadRequest(format!(
                "transfer '{}' cannot be resumed from status '{}'",
                existing.id, existing.status
            )));
        }
        let now = now_ms();
        conn.execute(
            "UPDATE artifact_transfers
             SET status = 'queued', checksum_status = NULL, error = NULL, updated_at = ?2,
                 completed_at = NULL
             WHERE id = ?1",
            params![id, now],
        )?;
        query_artifact_transfer(&conn, id)
    }

    /// Requeue a failed, retryable, or cancelled transfer.
    pub async fn retry_artifact_transfer(
        &self,
        id: &str,
        resume: bool,
    ) -> Result<ArtifactTransfer, DashboardError> {
        let conn = self.conn.lock().await;
        let existing = query_artifact_transfer(&conn, id)?;
        if !matches!(
            existing.status.as_str(),
            "failed" | "retryable" | "cancelled"
        ) {
            return Err(DashboardError::BadRequest(format!(
                "transfer '{}' cannot be retried from status '{}'",
                existing.id, existing.status
            )));
        }
        let now = now_ms();
        let bytes_done = if resume { existing.bytes_done } else { 0 };
        conn.execute(
            "UPDATE artifact_transfers
             SET status = 'queued', bytes_done = ?2, checksum_status = NULL, error = NULL,
                 updated_at = ?3, completed_at = NULL
             WHERE id = ?1",
            params![id, bytes_done, now],
        )?;
        query_artifact_transfer(&conn, id)
    }

    /// Delete an inactive artifact transfer record without deleting artifacts.
    pub async fn delete_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<ArtifactTransfer, DashboardError> {
        let conn = self.conn.lock().await;
        let existing = query_artifact_transfer(&conn, id)?;
        if !matches!(
            existing.status.as_str(),
            "completed" | "cancelled" | "failed"
        ) {
            return Err(DashboardError::BadRequest(format!(
                "transfer '{}' cannot be deleted from status '{}'",
                existing.id, existing.status
            )));
        }
        conn.execute("DELETE FROM artifact_transfers WHERE id = ?1", params![id])?;
        Ok(existing)
    }

    /// Create a research job and its deterministic initial step list.
    pub async fn create_research_job(
        &self,
        job_type: &str,
        artifact_id: Option<&str>,
        requested_by: &str,
        priority: i64,
        params_json: Option<&str>,
    ) -> Result<ResearchJob, DashboardError> {
        let step_names = step_templates_for_job(job_type).ok_or_else(|| {
            DashboardError::BadRequest(
                "job_type must be 'export', 'current_params', or 'sweep'".to_string(),
            )
        })?;
        if matches!(job_type, "current_params" | "sweep") && artifact_id.is_none() {
            return Err(DashboardError::BadRequest(
                "artifact_id is required for backtest and sweep jobs".to_string(),
            ));
        }

        let mut conn = self.conn.lock().await;
        if let Some(id) = artifact_id {
            let exists = conn
                .query_row(
                    "SELECT 1 FROM run_artifacts WHERE id = ?1",
                    params![id],
                    |_| Ok(()),
                )
                .optional()?;
            if exists.is_none() {
                return Err(DashboardError::NotFound(format!(
                    "artifact '{id}' not found"
                )));
            }
        }

        let now = now_ms();
        let job_id = uuid::Uuid::new_v4().to_string();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO research_jobs (
                id, job_type, artifact_id, status, priority, requested_by, params_json,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'queued', ?4, ?5, ?6, ?7, ?8)",
            params![
                job_id,
                job_type,
                artifact_id,
                priority,
                requested_by,
                params_json,
                now,
                now
            ],
        )?;
        for (index, name) in step_names.iter().enumerate() {
            let step_index = i64::try_from(index)
                .map_err(|e| DashboardError::Internal(format!("step index overflow: {e}")))?;
            let step_id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO research_job_steps (
                    id, job_id, step_index, name, status, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?6)",
                params![step_id, job_id, step_index, name, now, now],
            )?;
        }
        tx.commit()?;
        let job = conn.query_row(
            "SELECT id, job_type, artifact_id, status, priority, requested_by, params_json,
                    created_at, updated_at, cancelled_at, completed_at
             FROM research_jobs
             WHERE id = ?1",
            params![job_id],
            research_job_from_row,
        )?;
        Ok(job)
    }

    /// List research jobs.
    pub async fn list_research_jobs(&self) -> Result<Vec<ResearchJob>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, job_type, artifact_id, status, priority, requested_by, params_json,
                    created_at, updated_at, cancelled_at, completed_at
             FROM research_jobs
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([], research_job_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return one research job by ID.
    pub async fn get_research_job(&self, id: &str) -> Result<Option<ResearchJob>, DashboardError> {
        let conn = self.conn.lock().await;
        let job = conn
            .query_row(
                "SELECT id, job_type, artifact_id, status, priority, requested_by, params_json,
                        created_at, updated_at, cancelled_at, completed_at
                 FROM research_jobs
                 WHERE id = ?1",
                params![id],
                research_job_from_row,
            )
            .optional()?;
        Ok(job)
    }

    /// Return steps for one research job.
    pub async fn get_research_job_steps(
        &self,
        job_id: &str,
    ) -> Result<Vec<ResearchJobStep>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, job_id, step_index, name, status, lease_owner, leased_until_ms, attempts,
                    input_json, output_json, error, created_at, updated_at, started_at, completed_at
             FROM research_job_steps
             WHERE job_id = ?1
             ORDER BY step_index, id",
        )?;
        let rows = stmt
            .query_map(params![job_id], research_job_step_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Update a queued research job before any step has started.
    pub async fn update_queued_research_job(
        &self,
        id: &str,
        artifact_id: NullableUpdate<&str>,
        priority: Option<i64>,
        params_json: NullableUpdate<&str>,
    ) -> Result<ResearchJob, DashboardError> {
        let conn = self.conn.lock().await;
        let existing = query_research_job(&conn, id)?;
        if existing.status != "queued" {
            return Err(DashboardError::BadRequest(format!(
                "research job '{}' can only be updated while queued",
                existing.id
            )));
        }
        let started_steps: u64 = conn.query_row(
            "SELECT COUNT(*)
             FROM research_job_steps
             WHERE job_id = ?1
               AND (
                   status <> 'queued'
                   OR attempts > 0
                   OR started_at IS NOT NULL
                   OR completed_at IS NOT NULL
               )",
            params![id],
            |row| row.get(0),
        )?;
        if started_steps > 0 {
            return Err(DashboardError::BadRequest(format!(
                "research job '{}' cannot be updated after steps have started",
                existing.id
            )));
        }

        let new_artifact_id = match artifact_id {
            NullableUpdate::Unchanged => existing.artifact_id.as_deref(),
            NullableUpdate::Clear => None,
            NullableUpdate::Set(value) => Some(value),
        };
        if let Some(value) = new_artifact_id
            && value.trim().is_empty()
        {
            return Err(DashboardError::BadRequest(
                "artifact_id must not be empty".to_string(),
            ));
        }
        if matches!(existing.job_type.as_str(), "current_params" | "sweep")
            && new_artifact_id.is_none()
        {
            return Err(DashboardError::BadRequest(
                "artifact_id is required for backtest and sweep jobs".to_string(),
            ));
        }
        if let Some(value) = new_artifact_id {
            ensure_artifact_exists(&conn, value)?;
        }

        let new_priority = priority.unwrap_or(existing.priority);
        let new_params_json = match params_json {
            NullableUpdate::Unchanged => existing.params_json.as_deref(),
            NullableUpdate::Clear => None,
            NullableUpdate::Set(value) => Some(value),
        };
        let now = now_ms();
        conn.execute(
            "UPDATE research_jobs
             SET artifact_id = ?2, priority = ?3, params_json = ?4, updated_at = ?5
             WHERE id = ?1",
            params![id, new_artifact_id, new_priority, new_params_json, now],
        )?;
        query_research_job(&conn, id)
    }

    /// Pause a queued, running, or retryable research job.
    pub async fn pause_research_job(&self, id: &str) -> Result<ResearchJob, DashboardError> {
        let mut conn = self.conn.lock().await;
        let existing = query_research_job(&conn, id)?;
        if existing.status == "paused" {
            return Ok(existing);
        }
        if !matches!(existing.status.as_str(), "queued" | "running" | "retryable") {
            return Err(DashboardError::BadRequest(format!(
                "research job '{}' cannot be paused from status '{}'",
                existing.id, existing.status
            )));
        }
        let now = now_ms();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_jobs
             SET status = 'paused', updated_at = ?2
             WHERE id = ?1",
            params![id, now],
        )?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'paused', lease_owner = NULL, leased_until_ms = NULL, updated_at = ?2
             WHERE job_id = ?1 AND status IN ('queued','retryable','leased')",
            params![id, now],
        )?;
        tx.commit()?;
        query_research_job(&conn, id)
    }

    /// Resume a paused, failed, blocked, retryable, or cancelled research job.
    pub async fn resume_research_job(&self, id: &str) -> Result<ResearchJob, DashboardError> {
        let mut conn = self.conn.lock().await;
        let existing = query_research_job(&conn, id)?;
        if existing.status != "paused" {
            drop(conn);
            return self.retry_research_job(id).await;
        }
        let now = now_ms();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_jobs
             SET status = 'queued', cancelled_at = NULL, completed_at = NULL, updated_at = ?2
             WHERE id = ?1",
            params![id, now],
        )?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'queued', lease_owner = NULL, leased_until_ms = NULL, error = NULL,
                 updated_at = ?2
             WHERE job_id = ?1 AND status = 'paused'",
            params![id, now],
        )?;
        tx.commit()?;
        query_research_job(&conn, id)
    }

    /// Delete an inactive research job that has no reports.
    pub async fn delete_research_job(&self, id: &str) -> Result<ResearchJob, DashboardError> {
        let mut conn = self.conn.lock().await;
        let job = query_research_job(&conn, id)?;
        if matches!(job.status.as_str(), "queued" | "running") {
            return Err(DashboardError::BadRequest(format!(
                "research job '{}' cannot be deleted from status '{}'",
                job.id, job.status
            )));
        }
        let reports: u64 = conn.query_row(
            "SELECT COUNT(*) FROM research_reports WHERE job_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if reports > 0 {
            return Err(DashboardError::BadRequest(format!(
                "research job '{}' has reports and cannot be deleted",
                job.id
            )));
        }
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM research_job_events WHERE job_id = ?1",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM research_job_steps WHERE job_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM research_jobs WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(job)
    }

    /// Cancel one queued or running research job.
    pub async fn cancel_research_job(&self, id: &str) -> Result<ResearchJob, DashboardError> {
        let mut conn = self.conn.lock().await;
        let now = now_ms();
        let tx = conn.transaction()?;
        let updated = tx.execute(
            "UPDATE research_jobs
             SET status = 'cancelled', cancelled_at = ?2, updated_at = ?2
             WHERE id = ?1 AND status NOT IN ('completed','cancelled')",
            params![id, now],
        )?;
        if updated == 0 {
            let exists: Option<String> = tx
                .query_row(
                    "SELECT id FROM research_jobs WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(DashboardError::NotFound(format!(
                    "research job '{id}' not found"
                )));
            }
        }
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'cancelled', lease_owner = NULL, leased_until_ms = NULL, updated_at = ?2
             WHERE job_id = ?1 AND status NOT IN ('completed','cancelled')",
            params![id, now],
        )?;
        tx.commit()?;
        let job = conn.query_row(
            "SELECT id, job_type, artifact_id, status, priority, requested_by, params_json,
                    created_at, updated_at, cancelled_at, completed_at
             FROM research_jobs
             WHERE id = ?1",
            params![id],
            research_job_from_row,
        )?;
        Ok(job)
    }

    /// Retry one failed, blocked, retryable, or cancelled research job.
    pub async fn retry_research_job(&self, id: &str) -> Result<ResearchJob, DashboardError> {
        let mut conn = self.conn.lock().await;
        let now = now_ms();
        let tx = conn.transaction()?;
        let updated = tx.execute(
            "UPDATE research_jobs
             SET status = 'queued', cancelled_at = NULL, completed_at = NULL, updated_at = ?2
             WHERE id = ?1 AND status IN ('failed','blocked','retryable','cancelled')",
            params![id, now],
        )?;
        if updated == 0 {
            let current: Option<String> = tx
                .query_row(
                    "SELECT status FROM research_jobs WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;
            match current {
                None => {
                    return Err(DashboardError::NotFound(format!(
                        "research job '{id}' not found"
                    )));
                }
                Some(status) => {
                    return Err(DashboardError::BadRequest(format!(
                        "research job '{id}' cannot be retried from status '{status}'"
                    )));
                }
            }
        }
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'queued', lease_owner = NULL, leased_until_ms = NULL, error = NULL,
                 updated_at = ?2
             WHERE job_id = ?1 AND status IN ('failed','blocked','retryable','cancelled')",
            params![id, now],
        )?;
        tx.commit()?;
        let job = conn.query_row(
            "SELECT id, job_type, artifact_id, status, priority, requested_by, params_json,
                    created_at, updated_at, cancelled_at, completed_at
             FROM research_jobs
             WHERE id = ?1",
            params![id],
            research_job_from_row,
        )?;
        Ok(job)
    }

    /// Append one research job event.
    pub async fn append_research_job_event(
        &self,
        job_id: &str,
        step_id: Option<&str>,
        level: &str,
        message: &str,
        details_json: Option<&str>,
    ) -> Result<ResearchJobEvent, DashboardError> {
        if !matches!(level, "info" | "warn" | "error" | "progress") {
            return Err(DashboardError::BadRequest(
                "event level must be info, warn, error, or progress".to_string(),
            ));
        }
        if message.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "event message must not be empty".to_string(),
            ));
        }

        let conn = self.conn.lock().await;
        let job_exists = conn
            .query_row(
                "SELECT 1 FROM research_jobs WHERE id = ?1",
                params![job_id],
                |_| Ok(()),
            )
            .optional()?;
        if job_exists.is_none() {
            return Err(DashboardError::NotFound(format!(
                "research job '{job_id}' not found"
            )));
        }
        if let Some(step_id) = step_id {
            let step_exists = conn
                .query_row(
                    "SELECT 1 FROM research_job_steps WHERE id = ?1 AND job_id = ?2",
                    params![step_id, job_id],
                    |_| Ok(()),
                )
                .optional()?;
            if step_exists.is_none() {
                return Err(DashboardError::BadRequest(
                    "step_id does not belong to job".to_string(),
                ));
            }
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        conn.execute(
            "INSERT INTO research_job_events (
                id, job_id, step_id, timestamp_ms, level, message, details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, job_id, step_id, now, level, message, details_json],
        )?;
        let event = conn.query_row(
            "SELECT id, job_id, step_id, timestamp_ms, level, message, details_json
             FROM research_job_events
             WHERE id = ?1",
            params![id],
            research_job_event_from_row,
        )?;
        Ok(event)
    }

    /// List events for one research job.
    pub async fn list_research_job_events(
        &self,
        job_id: &str,
    ) -> Result<Vec<ResearchJobEvent>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, job_id, step_id, timestamp_ms, level, message, details_json
             FROM research_job_events
             WHERE job_id = ?1
             ORDER BY timestamp_ms, id",
        )?;
        let rows = stmt
            .query_map(params![job_id], research_job_event_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List generated research reports.
    pub async fn list_research_reports(&self) -> Result<Vec<ResearchReport>, DashboardError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, job_id, artifact_id, title, status, summary_json, report_path, csv_path,
                    created_at, updated_at
             FROM research_reports
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([], research_report_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return one research report by ID.
    pub async fn get_research_report(
        &self,
        id: &str,
    ) -> Result<Option<ResearchReport>, DashboardError> {
        let conn = self.conn.lock().await;
        let report = conn
            .query_row(
                "SELECT id, job_id, artifact_id, title, status, summary_json, report_path, csv_path,
                        created_at, updated_at
                 FROM research_reports
                 WHERE id = ?1",
                params![id],
                research_report_from_row,
            )
            .optional()?;
        Ok(report)
    }

    /// Return the most recent research report for one job.
    pub async fn get_research_report_for_job(
        &self,
        job_id: &str,
    ) -> Result<Option<ResearchReport>, DashboardError> {
        let conn = self.conn.lock().await;
        let report = conn
            .query_row(
                "SELECT id, job_id, artifact_id, title, status, summary_json, report_path, csv_path,
                        created_at, updated_at
                 FROM research_reports
                 WHERE job_id = ?1
                 ORDER BY created_at DESC, id DESC
                 LIMIT 1",
                params![job_id],
                research_report_from_row,
            )
            .optional()?;
        Ok(report)
    }

    /// Create or update one generated research report for a job.
    pub async fn create_or_update_research_report(
        &self,
        record: &ResearchReportRecord<'_>,
    ) -> Result<ResearchReport, DashboardError> {
        if record.title.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "research report title must not be empty".to_string(),
            ));
        }
        validate_research_report_status(record.status)?;

        let conn = self.conn.lock().await;
        let now = now_ms();
        let existing_id = conn
            .query_row(
                "SELECT id FROM research_reports WHERE job_id = ?1 ORDER BY created_at DESC LIMIT 1",
                params![record.job_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let report_id = existing_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        conn.execute(
            "INSERT INTO research_reports (
                id, job_id, artifact_id, title, status, summary_json, report_path, csv_path,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                artifact_id = excluded.artifact_id,
                title = excluded.title,
                status = excluded.status,
                summary_json = excluded.summary_json,
                report_path = excluded.report_path,
                csv_path = excluded.csv_path,
                updated_at = excluded.updated_at",
            params![
                report_id,
                record.job_id,
                record.artifact_id,
                record.title,
                record.status,
                record.summary_json,
                record.report_path,
                record.csv_path,
                now,
                now
            ],
        )?;
        let report = conn.query_row(
            "SELECT id, job_id, artifact_id, title, status, summary_json, report_path, csv_path,
                    created_at, updated_at
             FROM research_reports
             WHERE id = ?1",
            params![report_id],
            research_report_from_row,
        )?;
        Ok(report)
    }

    /// Update operator-editable metadata for one research report.
    pub async fn update_research_report_metadata(
        &self,
        id: &str,
        title: &str,
        status: &str,
    ) -> Result<ResearchReport, DashboardError> {
        if title.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "research report title must not be empty".to_string(),
            ));
        }
        validate_research_report_status(status)?;
        let conn = self.conn.lock().await;
        let now = now_ms();
        let changed = conn.execute(
            "UPDATE research_reports
             SET title = ?2, status = ?3, updated_at = ?4
             WHERE id = ?1",
            params![id, title.trim(), status.trim(), now],
        )?;
        if changed == 0 {
            return Err(DashboardError::NotFound(format!("report '{id}' not found")));
        }
        let report = conn.query_row(
            "SELECT id, job_id, artifact_id, title, status, summary_json, report_path, csv_path,
                    created_at, updated_at
             FROM research_reports
             WHERE id = ?1",
            params![id],
            research_report_from_row,
        )?;
        Ok(report)
    }

    /// Delete one research report metadata row.
    pub async fn delete_research_report(&self, id: &str) -> Result<ResearchReport, DashboardError> {
        let conn = self.conn.lock().await;
        let report = conn
            .query_row(
                "SELECT id, job_id, artifact_id, title, status, summary_json, report_path, csv_path,
                        created_at, updated_at
                 FROM research_reports
                 WHERE id = ?1",
                params![id],
                research_report_from_row,
            )
            .optional()?
            .ok_or_else(|| DashboardError::NotFound(format!("report '{id}' not found")))?;
        conn.execute("DELETE FROM research_reports WHERE id = ?1", params![id])?;
        Ok(report)
    }

    /// Lease the next executable research step with the current wall clock.
    pub async fn lease_next_research_step(
        &self,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<Option<ResearchStepLease>, DashboardError> {
        self.lease_next_research_step_at(worker_id, now_ms(), lease_duration_ms)
            .await
    }

    /// Lease the next executable research step at a deterministic timestamp.
    pub async fn lease_next_research_step_at(
        &self,
        worker_id: &str,
        now: u64,
        lease_duration_ms: u64,
    ) -> Result<Option<ResearchStepLease>, DashboardError> {
        if worker_id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "worker_id must not be empty".to_string(),
            ));
        }
        if lease_duration_ms == 0 {
            return Err(DashboardError::BadRequest(
                "lease_duration_ms must be positive".to_string(),
            ));
        }

        let mut conn = self.conn.lock().await;
        let tx = conn.transaction()?;
        let candidate: Option<(String, String)> = tx
            .query_row(
                "SELECT s.id, s.job_id
                 FROM research_job_steps s
                 JOIN research_jobs j ON j.id = s.job_id
                 WHERE j.status IN ('queued','running','retryable')
                   AND (
                     s.status IN ('queued','retryable')
                     OR (
                       s.status IN ('leased','running')
                       AND s.leased_until_ms IS NOT NULL
                       AND s.leased_until_ms <= ?1
                     )
                   )
                   AND NOT EXISTS (
                     SELECT 1
                     FROM research_job_steps previous
                     WHERE previous.job_id = s.job_id
                       AND previous.step_index < s.step_index
                       AND previous.status <> 'completed'
                   )
                 ORDER BY j.priority DESC, j.created_at, s.step_index
                 LIMIT 1",
                params![now],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((step_id, job_id)) = candidate else {
            tx.commit()?;
            return Ok(None);
        };

        let lease_until = now.saturating_add(lease_duration_ms);
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'leased', lease_owner = ?2, leased_until_ms = ?3,
                 attempts = attempts + 1, updated_at = ?4, error = NULL
             WHERE id = ?1",
            params![step_id, worker_id, lease_until, now],
        )?;
        tx.execute(
            "UPDATE research_jobs
             SET status = 'running', updated_at = ?2
             WHERE id = ?1 AND status IN ('queued','retryable')",
            params![job_id, now],
        )?;
        tx.commit()?;

        let job = conn.query_row(
            "SELECT id, job_type, artifact_id, status, priority, requested_by, params_json,
                    created_at, updated_at, cancelled_at, completed_at
             FROM research_jobs
             WHERE id = ?1",
            params![job_id],
            research_job_from_row,
        )?;
        let step = conn.query_row(
            "SELECT id, job_id, step_index, name, status, lease_owner, leased_until_ms, attempts,
                    input_json, output_json, error, created_at, updated_at, started_at, completed_at
             FROM research_job_steps
             WHERE id = ?1",
            params![step_id],
            research_job_step_from_row,
        )?;
        Ok(Some(ResearchStepLease { job, step }))
    }

    /// Mark a leased research step as running.
    pub async fn mark_research_step_running(
        &self,
        step_id: &str,
        worker_id: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        self.mark_research_step_running_at(step_id, worker_id, now_ms())
            .await
    }

    /// Mark a leased research step as running at a deterministic timestamp.
    pub async fn mark_research_step_running_at(
        &self,
        step_id: &str,
        worker_id: &str,
        now: u64,
    ) -> Result<ResearchJobStep, DashboardError> {
        let conn = self.conn.lock().await;
        let updated = conn.execute(
            "UPDATE research_job_steps
             SET status = 'running', started_at = COALESCE(started_at, ?3), updated_at = ?3
             WHERE id = ?1 AND lease_owner = ?2 AND status IN ('leased','running')",
            params![step_id, worker_id, now],
        )?;
        if updated == 0 {
            ensure_step_exists(&conn, step_id)?;
            return Err(DashboardError::BadRequest(format!(
                "step '{step_id}' is not leased by worker '{worker_id}'"
            )));
        }
        query_research_step(&conn, step_id)
    }

    /// Mark a research step as completed.
    pub async fn complete_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        output_json: Option<&str>,
    ) -> Result<ResearchJobStep, DashboardError> {
        self.complete_research_step_at(step_id, worker_id, output_json, now_ms())
            .await
    }

    /// Mark a research step as completed at a deterministic timestamp.
    pub async fn complete_research_step_at(
        &self,
        step_id: &str,
        worker_id: &str,
        output_json: Option<&str>,
        now: u64,
    ) -> Result<ResearchJobStep, DashboardError> {
        let mut conn = self.conn.lock().await;
        let existing = query_research_step(&conn, step_id)?;
        if existing.status == "completed" {
            return Ok(existing);
        }
        if existing.lease_owner.as_deref() != Some(worker_id)
            || !matches!(existing.status.as_str(), "leased" | "running")
        {
            return Err(DashboardError::BadRequest(format!(
                "step '{step_id}' is not active for worker '{worker_id}'"
            )));
        }

        let job_id = existing.job_id.clone();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'completed', output_json = ?3, lease_owner = NULL, leased_until_ms = NULL,
                 completed_at = ?4, updated_at = ?4
             WHERE id = ?1 AND lease_owner = ?2",
            params![step_id, worker_id, output_json, now],
        )?;
        let incomplete: u64 = tx.query_row(
            "SELECT COUNT(*)
             FROM research_job_steps
             WHERE job_id = ?1 AND status <> 'completed'",
            params![job_id],
            |row| row.get(0),
        )?;
        if incomplete == 0 {
            tx.execute(
                "UPDATE research_jobs
                 SET status = 'completed', completed_at = ?2, updated_at = ?2
                 WHERE id = ?1",
                params![job_id, now],
            )?;
        }
        tx.commit()?;
        query_research_step(&conn, step_id)
    }

    /// Mark a research step as retryable or permanently failed.
    pub async fn fail_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        error: &str,
        retryable: bool,
    ) -> Result<ResearchJobStep, DashboardError> {
        self.fail_research_step_at(step_id, worker_id, error, retryable, now_ms())
            .await
    }

    /// Mark a research step as retryable or permanently failed at a deterministic timestamp.
    pub async fn fail_research_step_at(
        &self,
        step_id: &str,
        worker_id: &str,
        error: &str,
        retryable: bool,
        now: u64,
    ) -> Result<ResearchJobStep, DashboardError> {
        if error.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "error must not be empty".to_string(),
            ));
        }
        let mut conn = self.conn.lock().await;
        let existing = query_research_step(&conn, step_id)?;
        if existing.lease_owner.as_deref() != Some(worker_id)
            || !matches!(existing.status.as_str(), "leased" | "running")
        {
            return Err(DashboardError::BadRequest(format!(
                "step '{step_id}' is not active for worker '{worker_id}'"
            )));
        }

        let new_status = if retryable { "retryable" } else { "failed" };
        let job_id = existing.job_id.clone();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = ?3, error = ?4, lease_owner = NULL, leased_until_ms = NULL,
                 updated_at = ?5
             WHERE id = ?1 AND lease_owner = ?2",
            params![step_id, worker_id, new_status, error, now],
        )?;
        tx.execute(
            "UPDATE research_jobs
             SET status = ?2, updated_at = ?3
             WHERE id = ?1",
            params![job_id, new_status, now],
        )?;
        tx.commit()?;
        query_research_step(&conn, step_id)
    }

    /// Mark a research step as blocked on an external prerequisite.
    pub async fn block_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        reason: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        self.block_research_step_at(step_id, worker_id, reason, now_ms())
            .await
    }

    /// Mark a research step as blocked at a deterministic timestamp.
    pub async fn block_research_step_at(
        &self,
        step_id: &str,
        worker_id: &str,
        reason: &str,
        now: u64,
    ) -> Result<ResearchJobStep, DashboardError> {
        if reason.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "block reason must not be empty".to_string(),
            ));
        }
        let mut conn = self.conn.lock().await;
        let existing = query_research_step(&conn, step_id)?;
        if existing.lease_owner.as_deref() != Some(worker_id)
            || !matches!(existing.status.as_str(), "leased" | "running")
        {
            return Err(DashboardError::BadRequest(format!(
                "step '{step_id}' is not active for worker '{worker_id}'"
            )));
        }

        let job_id = existing.job_id.clone();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'blocked', error = ?3, lease_owner = NULL, leased_until_ms = NULL,
                 updated_at = ?4
             WHERE id = ?1 AND lease_owner = ?2",
            params![step_id, worker_id, reason, now],
        )?;
        tx.execute(
            "UPDATE research_jobs
             SET status = 'blocked', updated_at = ?2
             WHERE id = ?1",
            params![job_id, now],
        )?;
        tx.commit()?;
        query_research_step(&conn, step_id)
    }

    /// Retry one failed, blocked, retryable, cancelled, or paused step.
    pub async fn retry_research_step(
        &self,
        job_id: &str,
        step_id: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        let mut conn = self.conn.lock().await;
        let existing = query_research_step(&conn, step_id)?;
        ensure_step_belongs_to_job(&existing, job_id)?;
        if !matches!(
            existing.status.as_str(),
            "failed" | "blocked" | "retryable" | "cancelled" | "paused"
        ) {
            return Err(DashboardError::BadRequest(format!(
                "research step '{}' cannot be retried from status '{}'",
                existing.id, existing.status
            )));
        }
        let now = now_ms();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'queued', lease_owner = NULL, leased_until_ms = NULL, error = NULL,
                 completed_at = NULL, updated_at = ?3
             WHERE id = ?1 AND job_id = ?2",
            params![step_id, job_id, now],
        )?;
        tx.execute(
            "UPDATE research_jobs
             SET status = 'queued', cancelled_at = NULL, completed_at = NULL, updated_at = ?2
             WHERE id = ?1",
            params![job_id, now],
        )?;
        tx.commit()?;
        query_research_step(&conn, step_id)
    }

    /// Cancel one non-terminal research step and its owning job.
    pub async fn cancel_research_step(
        &self,
        job_id: &str,
        step_id: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        let mut conn = self.conn.lock().await;
        let existing = query_research_step(&conn, step_id)?;
        ensure_step_belongs_to_job(&existing, job_id)?;
        if matches!(existing.status.as_str(), "completed" | "cancelled") {
            return Err(DashboardError::BadRequest(format!(
                "research step '{}' cannot be cancelled from status '{}'",
                existing.id, existing.status
            )));
        }
        let now = now_ms();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'cancelled', lease_owner = NULL, leased_until_ms = NULL,
                 error = 'cancelled by operator', updated_at = ?3
             WHERE id = ?1 AND job_id = ?2",
            params![step_id, job_id, now],
        )?;
        tx.execute(
            "UPDATE research_jobs
             SET status = 'cancelled', cancelled_at = ?2, completed_at = NULL, updated_at = ?2
             WHERE id = ?1",
            params![job_id, now],
        )?;
        tx.commit()?;
        query_research_step(&conn, step_id)
    }

    /// Clear a stale lease and return the step to retryable state.
    pub async fn clear_stale_research_step_lease(
        &self,
        job_id: &str,
        step_id: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        self.clear_stale_research_step_lease_at(job_id, step_id, now_ms())
            .await
    }

    /// Clear a stale lease at a deterministic timestamp.
    pub async fn clear_stale_research_step_lease_at(
        &self,
        job_id: &str,
        step_id: &str,
        now: u64,
    ) -> Result<ResearchJobStep, DashboardError> {
        let mut conn = self.conn.lock().await;
        let existing = query_research_step(&conn, step_id)?;
        ensure_step_belongs_to_job(&existing, job_id)?;
        if !matches!(existing.status.as_str(), "leased" | "running") {
            return Err(DashboardError::BadRequest(format!(
                "research step '{}' has no active lease",
                existing.id
            )));
        }
        let Some(leased_until_ms) = existing.leased_until_ms else {
            return Err(DashboardError::BadRequest(format!(
                "research step '{}' has no lease expiry",
                existing.id
            )));
        };
        if leased_until_ms > now {
            return Err(DashboardError::BadRequest(format!(
                "research step '{}' lease is not stale",
                existing.id
            )));
        }
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'retryable', lease_owner = NULL, leased_until_ms = NULL,
                 error = 'stale lease cleared by operator', updated_at = ?3
             WHERE id = ?1 AND job_id = ?2",
            params![step_id, job_id, now],
        )?;
        tx.execute(
            "UPDATE research_jobs
             SET status = 'retryable', updated_at = ?2
             WHERE id = ?1",
            params![job_id, now],
        )?;
        tx.commit()?;
        query_research_step(&conn, step_id)
    }

    /// Resolve a blocked step by returning it to the queued state.
    pub async fn resolve_research_step_blocker(
        &self,
        job_id: &str,
        step_id: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        let mut conn = self.conn.lock().await;
        let existing = query_research_step(&conn, step_id)?;
        ensure_step_belongs_to_job(&existing, job_id)?;
        if existing.status != "blocked" {
            return Err(DashboardError::BadRequest(format!(
                "research step '{}' is not blocked",
                existing.id
            )));
        }
        let now = now_ms();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE research_job_steps
             SET status = 'queued', lease_owner = NULL, leased_until_ms = NULL, error = NULL,
                 updated_at = ?3
             WHERE id = ?1 AND job_id = ?2",
            params![step_id, job_id, now],
        )?;
        tx.execute(
            "UPDATE research_jobs
             SET status = 'queued', updated_at = ?2
             WHERE id = ?1",
            params![job_id, now],
        )?;
        tx.commit()?;
        query_research_step(&conn, step_id)
    }
}

/// Seed the local-first machine records used before remote setup exists.
fn seed_default_research_machines(conn: &Connection) -> rusqlite::Result<()> {
    let now = now_ms();
    for (id, name, role, ssh_alias, status, details_json) in [
        (
            "live",
            "Buba Paint Live",
            "live",
            Some("buba-paint"),
            "configured",
            Some(r#"{"host":"buba-paint","phase":"local_first"}"#),
        ),
        (
            "research",
            "Research Worker",
            "research",
            Some("testing"),
            "not_configured",
            Some(r#"{"host":"testing","deferred_until_phase":7}"#),
        ),
    ] {
        conn.execute(
            "INSERT OR IGNORE INTO research_machines (
                id, name, role, ssh_alias, status, details_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, name, role, ssh_alias, status, details_json, now, now],
        )?;
    }
    Ok(())
}

/// Return deterministic step templates for a supported research job type.
fn step_templates_for_job(job_type: &str) -> Option<Vec<&'static str>> {
    match job_type {
        "export" => Some(vec![
            "plan_export",
            "snapshot_or_copy_runtime",
            "write_artifact_manifest",
            "verify_artifact",
        ]),
        "current_params" => Some(vec![
            "verify_artifact",
            "validate_replay_data",
            "validate_backtest_input",
            "prepare_backtest_input",
            "run_backtest",
            "write_report",
        ]),
        "sweep" => Some(vec![
            "verify_artifact",
            "validate_replay_data",
            "validate_backtest_input",
            "prepare_backtest_input",
            "run_sweep",
            "write_report",
        ]),
        _ => None,
    }
}

/// Map one machine row.
fn research_machine_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchMachine> {
    Ok(ResearchMachine {
        id: row.get(0)?,
        name: row.get(1)?,
        role: row.get(2)?,
        ssh_alias: row.get(3)?,
        status: row.get(4)?,
        details_json: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Return one research machine by ID.
fn query_research_machine(conn: &Connection, id: &str) -> Result<ResearchMachine, DashboardError> {
    conn.query_row(
        "SELECT id, name, role, ssh_alias, status, details_json, created_at, updated_at
         FROM research_machines
         WHERE id = ?1",
        params![id],
        research_machine_from_row,
    )
    .optional()?
    .ok_or_else(|| DashboardError::NotFound(format!("research machine '{id}' not found")))
}

/// Return dependency counts for one research machine.
fn research_machine_dependency_counts(
    conn: &Connection,
    id: &str,
) -> Result<ResearchMachineDependencyCounts, DashboardError> {
    conn.query_row(
        "SELECT
            (SELECT COUNT(*) FROM run_artifacts WHERE source_machine_id = ?1),
            (SELECT COUNT(*) FROM artifact_transfers WHERE source_machine_id = ?1),
            (SELECT COUNT(*) FROM artifact_transfers WHERE dest_machine_id = ?1),
            (SELECT COUNT(*) FROM artifact_transfers
                 WHERE (source_machine_id = ?1 OR dest_machine_id = ?1)
                   AND status IN ('queued','running','retryable','paused')),
            (SELECT COUNT(*) FROM research_jobs
                 WHERE artifact_id IN (
                     SELECT id FROM run_artifacts WHERE source_machine_id = ?1
                 )),
            (SELECT COUNT(*) FROM research_reports
                 WHERE artifact_id IN (
                     SELECT id FROM run_artifacts WHERE source_machine_id = ?1
                 ))",
        params![id],
        |row| {
            Ok(ResearchMachineDependencyCounts {
                artifacts: row.get(0)?,
                transfers_as_source: row.get(1)?,
                transfers_as_destination: row.get(2)?,
                active_transfers: row.get(3)?,
                jobs_using_source_artifacts: row.get(4)?,
                reports_using_source_artifacts: row.get(5)?,
            })
        },
    )
    .map_err(DashboardError::from)
}

/// Return whether a machine should receive new background work.
fn research_machine_accepts_work(conn: &Connection, id: &str) -> Result<bool, DashboardError> {
    let status: String = conn
        .query_row(
            "SELECT status FROM research_machines WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| DashboardError::NotFound(format!("research machine '{id}' not found")))?;
    Ok(status != "disabled")
}

/// Validate a complete research machine input record.
fn validate_research_machine_record(
    record: &ResearchMachineRecord<'_>,
) -> Result<(), DashboardError> {
    validate_research_machine_id(record.id)?;
    validate_research_machine_name(record.name)?;
    validate_research_machine_role(record.role)?;
    validate_optional_machine_text("ssh_alias", record.ssh_alias)?;
    validate_research_machine_status(record.status)?;
    validate_optional_json_text("details_json", record.details_json)?;
    Ok(())
}

/// Validate a machine ID used by APIs and workers.
fn validate_research_machine_id(id: &str) -> Result<(), DashboardError> {
    let value = id.trim();
    if value.is_empty() {
        return Err(DashboardError::BadRequest(
            "machine id must not be empty".to_string(),
        ));
    }
    if value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(DashboardError::BadRequest(
            "machine id must contain only ASCII letters, numbers, dash, underscore, or dot"
                .to_string(),
        ));
    }
    Ok(())
}

/// Validate a human-facing machine name.
fn validate_research_machine_name(name: &str) -> Result<(), DashboardError> {
    if name.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "machine name must not be empty".to_string(),
        ));
    }
    Ok(())
}

/// Validate a machine role.
fn validate_research_machine_role(role: &str) -> Result<(), DashboardError> {
    if matches!(role, "live" | "research" | "controller") {
        Ok(())
    } else {
        Err(DashboardError::BadRequest(
            "machine role must be live, research, or controller".to_string(),
        ))
    }
}

/// Validate a machine lifecycle or readiness status.
fn validate_research_machine_status(status: &str) -> Result<(), DashboardError> {
    if matches!(
        status,
        "not_configured"
            | "configured"
            | "online"
            | "idle"
            | "busy"
            | "degraded"
            | "error"
            | "disabled"
            | "unreachable"
            | "maintenance"
    ) {
        Ok(())
    } else {
        Err(DashboardError::BadRequest(
            "machine status must be not_configured, configured, online, idle, busy, degraded, error, disabled, unreachable, or maintenance"
                .to_string(),
        ))
    }
}

/// Validate optional short machine metadata text.
fn validate_optional_machine_text(name: &str, value: Option<&str>) -> Result<(), DashboardError> {
    if let Some(value) = value
        && value.trim().is_empty()
    {
        return Err(DashboardError::BadRequest(format!(
            "{name} must not be empty when provided"
        )));
    }
    Ok(())
}

/// Validate optional serialized JSON text.
fn validate_optional_json_text(name: &str, value: Option<&str>) -> Result<(), DashboardError> {
    if let Some(value) = value {
        serde_json::from_str::<serde_json::Value>(value).map_err(|error| {
            DashboardError::BadRequest(format!("{name} must be valid JSON: {error}"))
        })?;
    }
    Ok(())
}

/// Map one artifact row.
fn research_artifact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchArtifact> {
    Ok(ResearchArtifact {
        id: row.get(0)?,
        source_machine_id: row.get(1)?,
        kind: row.get(2)?,
        status: row.get(3)?,
        run_mode: row.get(4)?,
        artifact_root: row.get(5)?,
        manifest_path: row.get(6)?,
        bundle_path: row.get(7)?,
        source_db_path: row.get(8)?,
        interval_start_ms: row.get(9)?,
        interval_end_ms: row.get(10)?,
        bytes: row.get(11)?,
        checksum: row.get(12)?,
        replay_quality_class: row.get(13)?,
        backtest_ready_class: row.get(14)?,
        live_fidelity_class: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        archived_at: row.get(18)?,
    })
}

/// Return an artifact only when no durable records depend on it.
fn ensure_research_artifact_deletable(
    conn: &Connection,
    id: &str,
) -> Result<ResearchArtifact, DashboardError> {
    let artifact = conn
        .query_row(
            "SELECT id, source_machine_id, kind, status, run_mode, artifact_root, manifest_path,
                    bundle_path, source_db_path, interval_start_ms, interval_end_ms, bytes,
                    checksum, replay_quality_class, backtest_ready_class, live_fidelity_class,
                    created_at, updated_at, archived_at
             FROM run_artifacts
             WHERE id = ?1",
            params![id],
            research_artifact_from_row,
        )
        .optional()?
        .ok_or_else(|| DashboardError::NotFound(format!("artifact '{id}' not found")))?;
    let references: i64 = conn.query_row(
        "SELECT
            (SELECT COUNT(*) FROM artifact_transfers WHERE artifact_id = ?1) +
            (SELECT COUNT(*) FROM research_jobs WHERE artifact_id = ?1) +
            (SELECT COUNT(*) FROM research_reports WHERE artifact_id = ?1)",
        params![id],
        |row| row.get(0),
    )?;
    if references > 0 {
        return Err(DashboardError::BadRequest(format!(
            "artifact '{id}' is still referenced by research state"
        )));
    }
    Ok(artifact)
}

/// Map one transfer row.
fn artifact_transfer_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactTransfer> {
    Ok(ArtifactTransfer {
        id: row.get(0)?,
        artifact_id: row.get(1)?,
        source_machine_id: row.get(2)?,
        dest_machine_id: row.get(3)?,
        status: row.get(4)?,
        bytes_total: row.get(5)?,
        bytes_done: row.get(6)?,
        checksum_status: row.get(7)?,
        error: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        completed_at: row.get(11)?,
    })
}

/// Return one artifact transfer by ID.
fn query_artifact_transfer(
    conn: &Connection,
    transfer_id: &str,
) -> Result<ArtifactTransfer, DashboardError> {
    conn.query_row(
        "SELECT id, artifact_id, source_machine_id, dest_machine_id, status, bytes_total,
                bytes_done, checksum_status, error, created_at, updated_at, completed_at
         FROM artifact_transfers
         WHERE id = ?1",
        params![transfer_id],
        artifact_transfer_from_row,
    )
    .optional()?
    .ok_or_else(|| DashboardError::NotFound(format!("artifact transfer '{transfer_id}' not found")))
}

/// Verify that one artifact exists.
fn ensure_artifact_exists(conn: &Connection, artifact_id: &str) -> Result<(), DashboardError> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM run_artifacts WHERE id = ?1",
            params![artifact_id],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_none() {
        return Err(DashboardError::NotFound(format!(
            "artifact '{artifact_id}' not found"
        )));
    }
    Ok(())
}

/// Return one research job by ID.
fn query_research_job(conn: &Connection, job_id: &str) -> Result<ResearchJob, DashboardError> {
    conn.query_row(
        "SELECT id, job_type, artifact_id, status, priority, requested_by, params_json,
                created_at, updated_at, cancelled_at, completed_at
         FROM research_jobs
         WHERE id = ?1",
        params![job_id],
        research_job_from_row,
    )
    .optional()?
    .ok_or_else(|| DashboardError::NotFound(format!("research job '{job_id}' not found")))
}

/// Verify that one research machine exists.
fn ensure_research_machine_exists(
    conn: &Connection,
    machine_id: &str,
) -> Result<(), DashboardError> {
    validate_research_machine_id(machine_id)?;
    let exists = conn
        .query_row(
            "SELECT 1 FROM research_machines WHERE id = ?1",
            params![machine_id],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_none() {
        return Err(DashboardError::NotFound(format!(
            "research machine '{machine_id}' not found"
        )));
    }
    Ok(())
}

/// Validate a transfer lifecycle status.
fn validate_transfer_status(status: &str) -> Result<(), DashboardError> {
    if matches!(
        status,
        "queued" | "running" | "retryable" | "paused" | "failed" | "cancelled" | "completed"
    ) {
        Ok(())
    } else {
        Err(DashboardError::BadRequest(
            "transfer status must be queued, running, retryable, paused, failed, cancelled, or completed"
                .to_string(),
        ))
    }
}

/// Validate optional checksum state for transfers.
fn validate_transfer_checksum_status(status: &str) -> Result<(), DashboardError> {
    if matches!(
        status,
        "pending" | "verifying" | "verified" | "failed" | "skipped"
    ) {
        Ok(())
    } else {
        Err(DashboardError::BadRequest(
            "checksum_status must be pending, verifying, verified, failed, or skipped".to_string(),
        ))
    }
}

/// Validate operator-facing report lifecycle status.
fn validate_research_report_status(status: &str) -> Result<(), DashboardError> {
    if matches!(status.trim(), "available" | "archived") {
        Ok(())
    } else {
        Err(DashboardError::BadRequest(
            "research report status must be available or archived".to_string(),
        ))
    }
}

/// Map one research job row.
fn research_job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchJob> {
    Ok(ResearchJob {
        id: row.get(0)?,
        job_type: row.get(1)?,
        artifact_id: row.get(2)?,
        status: row.get(3)?,
        priority: row.get(4)?,
        requested_by: row.get(5)?,
        params_json: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        cancelled_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

/// Map one research job step row.
fn research_job_step_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchJobStep> {
    Ok(ResearchJobStep {
        id: row.get(0)?,
        job_id: row.get(1)?,
        step_index: row.get(2)?,
        name: row.get(3)?,
        status: row.get(4)?,
        lease_owner: row.get(5)?,
        leased_until_ms: row.get(6)?,
        attempts: row.get(7)?,
        input_json: row.get(8)?,
        output_json: row.get(9)?,
        error: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        started_at: row.get(13)?,
        completed_at: row.get(14)?,
    })
}

/// Return one research step by ID.
fn query_research_step(
    conn: &Connection,
    step_id: &str,
) -> Result<ResearchJobStep, DashboardError> {
    let step = conn
        .query_row(
            "SELECT id, job_id, step_index, name, status, lease_owner, leased_until_ms, attempts,
                    input_json, output_json, error, created_at, updated_at, started_at, completed_at
             FROM research_job_steps
             WHERE id = ?1",
            params![step_id],
            research_job_step_from_row,
        )
        .optional()?
        .ok_or_else(|| DashboardError::NotFound(format!("research step '{step_id}' not found")))?;
    Ok(step)
}

/// Verify that one research step exists.
fn ensure_step_exists(conn: &Connection, step_id: &str) -> Result<(), DashboardError> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM research_job_steps WHERE id = ?1",
            params![step_id],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_none() {
        return Err(DashboardError::NotFound(format!(
            "research step '{step_id}' not found"
        )));
    }
    Ok(())
}

/// Verify that a step belongs to the route job.
fn ensure_step_belongs_to_job(step: &ResearchJobStep, job_id: &str) -> Result<(), DashboardError> {
    if step.job_id != job_id {
        return Err(DashboardError::BadRequest(format!(
            "research step '{}' does not belong to job '{}'",
            step.id, job_id
        )));
    }
    Ok(())
}

/// Map one research job event row.
fn research_job_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchJobEvent> {
    Ok(ResearchJobEvent {
        id: row.get(0)?,
        job_id: row.get(1)?,
        step_id: row.get(2)?,
        timestamp_ms: row.get(3)?,
        level: row.get(4)?,
        message: row.get(5)?,
        details_json: row.get(6)?,
    })
}

/// Map one research report row.
fn research_report_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchReport> {
    Ok(ResearchReport {
        id: row.get(0)?,
        job_id: row.get(1)?,
        artifact_id: row.get(2)?,
        title: row.get(3)?,
        status: row.get(4)?,
        summary_json: row.get(5)?,
        report_path: row.get(6)?,
        csv_path: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

/// Now ms.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "tests/db_tests.rs"]
mod tests;
