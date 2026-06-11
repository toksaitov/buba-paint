//! Authenticated HTTP client that lets the research worker consume the
//! public controller's job queue.
//!
//! Every method mirrors one worker-token endpoint under
//! `/api/research/workers/` and the request payload types here are shared
//! with the controller-side handlers so both ends stay in sync.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::db::{
    ArtifactTransfer, DashboardDb, ResearchArtifact, ResearchArtifactRecord, ResearchJob,
    ResearchJobEvent, ResearchJobStep, ResearchMachine, ResearchReport, ResearchReportRecord,
    ResearchStepLease,
};
use crate::error::DashboardError;
use crate::research_backend::ResearchWorkBackend;

/// Header that carries the shared research worker token.
pub const WORKER_TOKEN_HEADER: &str = "x-buba-research-worker-token";

/// Claim request for the next executable step.
#[derive(Debug, Serialize, Deserialize)]
pub struct ClaimStepRequest {
    /// Worker identity that owns the lease.
    pub worker_id: String,
    /// Requested lease duration in milliseconds.
    pub lease_duration_ms: u64,
}

/// Lease mutation request for renew/run/complete/fail/block endpoints.
#[derive(Debug, Serialize, Deserialize)]
pub struct StepLeaseRequest {
    /// Worker identity that owns the lease.
    pub worker_id: String,
    /// Requested lease duration in milliseconds for renewals.
    pub lease_duration_ms: Option<u64>,
    /// Completion output JSON for complete requests.
    pub output_json: Option<String>,
    /// Error text for fail/block requests.
    pub error: Option<String>,
    /// Whether a failed step should stay retryable.
    pub retryable: Option<bool>,
}

/// Job event append request.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerEventRequest {
    /// Optional step the event belongs to.
    pub step_id: Option<String>,
    /// Event severity level.
    pub level: String,
    /// Event message.
    pub message: String,
    /// Optional structured details JSON.
    pub details_json: Option<String>,
}

/// Owned artifact upsert payload mirroring `ResearchArtifactRecord`.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerArtifactUpsertRequest {
    /// Stable artifact ID.
    pub id: String,
    /// Machine that produced the artifact.
    pub source_machine_id: Option<String>,
    /// Artifact class to store.
    pub kind: String,
    /// Lifecycle status to store.
    pub status: String,
    /// Source run mode to store.
    pub run_mode: Option<String>,
    /// Local artifact directory.
    pub artifact_root: Option<String>,
    /// Path to the artifact `manifest.json`.
    pub manifest_path: Option<String>,
    /// Optional bundled archive path.
    pub bundle_path: Option<String>,
    /// Original runtime database path.
    pub source_db_path: Option<String>,
    /// Optional replay interval start in milliseconds.
    pub interval_start_ms: Option<u64>,
    /// Optional replay interval end in milliseconds.
    pub interval_end_ms: Option<u64>,
    /// Total artifact bytes, when known.
    pub bytes: Option<u64>,
    /// Artifact DB checksum, when known.
    pub checksum: Option<String>,
    /// Replay quality class, when assessed.
    pub replay_quality_class: Option<String>,
    /// Backtest readiness class, when assessed.
    pub backtest_ready_class: Option<String>,
    /// Live fidelity class, when assessed.
    pub live_fidelity_class: Option<String>,
}

impl WorkerArtifactUpsertRequest {
    /// Build an owned upsert payload from a borrowed record.
    pub fn from_record(record: &ResearchArtifactRecord<'_>) -> Self {
        Self {
            id: record.id.to_string(),
            source_machine_id: record.source_machine_id.map(str::to_string),
            kind: record.kind.to_string(),
            status: record.status.to_string(),
            run_mode: record.run_mode.map(str::to_string),
            artifact_root: record.artifact_root.map(str::to_string),
            manifest_path: record.manifest_path.map(str::to_string),
            bundle_path: record.bundle_path.map(str::to_string),
            source_db_path: record.source_db_path.map(str::to_string),
            interval_start_ms: record.interval_start_ms,
            interval_end_ms: record.interval_end_ms,
            bytes: record.bytes,
            checksum: record.checksum.map(str::to_string),
            replay_quality_class: record.replay_quality_class.map(str::to_string),
            backtest_ready_class: record.backtest_ready_class.map(str::to_string),
            live_fidelity_class: record.live_fidelity_class.map(str::to_string),
        }
    }

    /// Borrow this payload as a database record.
    pub fn as_record(&self) -> ResearchArtifactRecord<'_> {
        ResearchArtifactRecord {
            id: &self.id,
            source_machine_id: self.source_machine_id.as_deref(),
            kind: &self.kind,
            status: &self.status,
            run_mode: self.run_mode.as_deref(),
            artifact_root: self.artifact_root.as_deref(),
            manifest_path: self.manifest_path.as_deref(),
            bundle_path: self.bundle_path.as_deref(),
            source_db_path: self.source_db_path.as_deref(),
            interval_start_ms: self.interval_start_ms,
            interval_end_ms: self.interval_end_ms,
            bytes: self.bytes,
            checksum: self.checksum.as_deref(),
            replay_quality_class: self.replay_quality_class.as_deref(),
            backtest_ready_class: self.backtest_ready_class.as_deref(),
            live_fidelity_class: self.live_fidelity_class.as_deref(),
        }
    }
}

/// Owned report upsert payload mirroring `ResearchReportRecord`.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerReportUpsertRequest {
    /// Job that produced this report.
    pub job_id: String,
    /// Artifact used by the report, when applicable.
    pub artifact_id: Option<String>,
    /// Operator-facing report title.
    pub title: String,
    /// Report lifecycle status.
    pub status: String,
    /// Optional serialized report summary.
    pub summary_json: Option<String>,
    /// Worker-local report JSON path for provenance.
    pub report_path: Option<String>,
    /// Worker-local report CSV path for provenance.
    pub csv_path: Option<String>,
}

impl WorkerReportUpsertRequest {
    /// Build an owned report payload from a borrowed record.
    pub fn from_record(record: &ResearchReportRecord<'_>) -> Self {
        Self {
            job_id: record.job_id.to_string(),
            artifact_id: record.artifact_id.map(str::to_string),
            title: record.title.to_string(),
            status: record.status.to_string(),
            summary_json: record.summary_json.map(str::to_string),
            report_path: record.report_path.map(str::to_string),
            csv_path: record.csv_path.map(str::to_string),
        }
    }

    /// Borrow this payload as a database record.
    pub fn as_record(&self) -> ResearchReportRecord<'_> {
        ResearchReportRecord {
            job_id: &self.job_id,
            artifact_id: self.artifact_id.as_deref(),
            title: &self.title,
            status: &self.status,
            summary_json: self.summary_json.as_deref(),
            report_path: self.report_path.as_deref(),
            csv_path: self.csv_path.as_deref(),
        }
    }
}

/// Report document upload payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerReportDocumentsRequest {
    /// Full report JSON document.
    pub report_json: String,
    /// Full report CSV document.
    pub report_csv: String,
}

/// Artifact document upload payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerArtifactDocumentsRequest {
    /// Full `manifest.json` document, when produced.
    pub manifest_json: Option<String>,
    /// Full `checksums.sha256` document, when produced.
    pub checksums_text: Option<String>,
}

/// Transfer claim request.
#[derive(Debug, Serialize, Deserialize)]
pub struct ClaimTransferRequest {
    /// Destination machine claiming queued transfers.
    pub dest_machine_id: String,
}

/// Transfer progress update request.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerTransferProgressRequest {
    /// New transfer status.
    pub status: String,
    /// Bytes copied so far.
    pub bytes_done: Option<u64>,
    /// Total expected bytes.
    pub bytes_total: Option<u64>,
    /// Checksum verification status.
    pub checksum_status: Option<String>,
    /// Error text for failed transfers.
    pub error: Option<String>,
}

/// Stale transfer recovery request.
#[derive(Debug, Serialize, Deserialize)]
pub struct RecoverTransfersRequest {
    /// Destination machine whose stale transfers should recover.
    pub dest_machine_id: String,
    /// Staleness window in milliseconds.
    pub stale_after_ms: u64,
}

/// Stale transfer recovery response.
#[derive(Debug, Serialize, Deserialize)]
pub struct RecoverTransfersResponse {
    /// Number of transfers returned to `retryable`.
    pub recovered: usize,
}

/// HTTP client for the public controller's worker endpoints.
#[derive(Debug, Clone)]
pub struct ResearchControllerClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl ResearchControllerClient {
    /// Build a client for one controller URL and worker token.
    pub fn new(base_url: &str, token: &str) -> Result<Self, DashboardError> {
        if base_url.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "controller base URL must not be empty".to_string(),
            ));
        }
        if token.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "research worker token must not be empty".to_string(),
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|error| {
                DashboardError::Internal(format!("building controller http client: {error}"))
            })?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http,
        })
    }

    /// Build a full endpoint URL.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Send one JSON request and decode an optional JSON response.
    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<Option<T>, DashboardError> {
        let mut request = self
            .http
            .request(method, self.url(path))
            .header(WORKER_TOKEN_HEADER, &self.token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|error| {
            DashboardError::Internal(format!("controller request {path} failed: {error}"))
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        let text = response.text().await.map_err(|error| {
            DashboardError::Internal(format!("reading controller response {path}: {error}"))
        })?;
        if !status.is_success() {
            return Err(DashboardError::Internal(format!(
                "controller {path} returned {status}: {text}"
            )));
        }
        let decoded: T = serde_json::from_str(&text).map_err(|error| {
            DashboardError::Internal(format!("decoding controller response {path}: {error}"))
        })?;
        Ok(Some(decoded))
    }

    /// Send one JSON request and require a JSON response.
    async fn send_required<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<T, DashboardError> {
        self.send(method, path, body).await?.ok_or_else(|| {
            DashboardError::Internal(format!("controller {path} returned no content"))
        })
    }

    /// Encode one path segment for URL safety.
    fn segment(value: &str) -> String {
        use std::fmt::Write;
        let mut encoded = String::with_capacity(value.len());
        for byte in value.bytes() {
            match byte {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(byte as char);
                }
                _ => {
                    let _ = write!(encoded, "%{byte:02X}");
                }
            }
        }
        encoded
    }
}

impl ResearchWorkBackend for ResearchControllerClient {
    /// Claim the next executable step from the controller queue.
    async fn lease_next_research_step(
        &self,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<Option<ResearchStepLease>, DashboardError> {
        self.send(
            reqwest::Method::POST,
            "/api/research/workers/steps/claim",
            Some(&ClaimStepRequest {
                worker_id: worker_id.to_string(),
                lease_duration_ms,
            }),
        )
        .await
    }

    /// Refresh a step lease on the controller.
    async fn refresh_research_step_lease(
        &self,
        step_id: &str,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<ResearchJobStep, DashboardError> {
        let path = format!(
            "/api/research/workers/steps/{}/renew",
            Self::segment(step_id)
        );
        self.send_required(
            reqwest::Method::POST,
            &path,
            Some(&StepLeaseRequest {
                worker_id: worker_id.to_string(),
                lease_duration_ms: Some(lease_duration_ms),
                output_json: None,
                error: None,
                retryable: None,
            }),
        )
        .await
    }

    /// Mark a step running on the controller.
    async fn mark_research_step_running(
        &self,
        step_id: &str,
        worker_id: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        let path = format!("/api/research/workers/steps/{}/run", Self::segment(step_id));
        self.send_required(
            reqwest::Method::POST,
            &path,
            Some(&StepLeaseRequest {
                worker_id: worker_id.to_string(),
                lease_duration_ms: None,
                output_json: None,
                error: None,
                retryable: None,
            }),
        )
        .await
    }

    /// Complete a step on the controller.
    async fn complete_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        output_json: Option<&str>,
    ) -> Result<ResearchJobStep, DashboardError> {
        let path = format!(
            "/api/research/workers/steps/{}/complete",
            Self::segment(step_id)
        );
        self.send_required(
            reqwest::Method::POST,
            &path,
            Some(&StepLeaseRequest {
                worker_id: worker_id.to_string(),
                lease_duration_ms: None,
                output_json: output_json.map(str::to_string),
                error: None,
                retryable: None,
            }),
        )
        .await
    }

    /// Fail a step on the controller.
    async fn fail_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        error: &str,
        retryable: bool,
    ) -> Result<ResearchJobStep, DashboardError> {
        let path = format!(
            "/api/research/workers/steps/{}/fail",
            Self::segment(step_id)
        );
        self.send_required(
            reqwest::Method::POST,
            &path,
            Some(&StepLeaseRequest {
                worker_id: worker_id.to_string(),
                lease_duration_ms: None,
                output_json: None,
                error: Some(error.to_string()),
                retryable: Some(retryable),
            }),
        )
        .await
    }

    /// Block a step on the controller.
    async fn block_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        reason: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        let path = format!(
            "/api/research/workers/steps/{}/block",
            Self::segment(step_id)
        );
        self.send_required(
            reqwest::Method::POST,
            &path,
            Some(&StepLeaseRequest {
                worker_id: worker_id.to_string(),
                lease_duration_ms: None,
                output_json: None,
                error: Some(reason.to_string()),
                retryable: None,
            }),
        )
        .await
    }

    /// Append a job event on the controller.
    async fn append_research_job_event(
        &self,
        job_id: &str,
        step_id: Option<&str>,
        level: &str,
        message: &str,
        details_json: Option<&str>,
    ) -> Result<ResearchJobEvent, DashboardError> {
        let path = format!(
            "/api/research/workers/jobs/{}/events",
            Self::segment(job_id)
        );
        self.send_required(
            reqwest::Method::POST,
            &path,
            Some(&WorkerEventRequest {
                step_id: step_id.map(str::to_string),
                level: level.to_string(),
                message: message.to_string(),
                details_json: details_json.map(str::to_string),
            }),
        )
        .await
    }

    /// Fetch one job from the controller for context and cancel polling.
    async fn get_research_job(&self, id: &str) -> Result<Option<ResearchJob>, DashboardError> {
        let path = format!("/api/research/workers/jobs/{}", Self::segment(id));
        self.send(reqwest::Method::GET, &path, None::<&()>).await
    }

    /// Cancel one job on the controller after command-level cancellation.
    async fn cancel_research_job(&self, id: &str) -> Result<ResearchJob, DashboardError> {
        let path = format!("/api/research/workers/jobs/{}/cancel", Self::segment(id));
        self.send_required(reqwest::Method::POST, &path, None::<&()>)
            .await
    }

    /// Fetch job steps from the controller.
    async fn get_research_job_steps(
        &self,
        job_id: &str,
    ) -> Result<Vec<ResearchJobStep>, DashboardError> {
        let path = format!("/api/research/workers/jobs/{}/steps", Self::segment(job_id));
        self.send_required(reqwest::Method::GET, &path, None::<&()>)
            .await
    }

    /// Fetch one artifact from the controller.
    async fn get_research_artifact(
        &self,
        id: &str,
    ) -> Result<Option<ResearchArtifact>, DashboardError> {
        let path = format!("/api/research/workers/artifacts/{}", Self::segment(id));
        self.send(reqwest::Method::GET, &path, None::<&()>).await
    }

    /// Upsert an artifact on the controller.
    async fn upsert_research_artifact(
        &self,
        record: &ResearchArtifactRecord<'_>,
    ) -> Result<ResearchArtifact, DashboardError> {
        self.send_required(
            reqwest::Method::POST,
            "/api/research/workers/artifacts",
            Some(&WorkerArtifactUpsertRequest::from_record(record)),
        )
        .await
    }

    /// Attach a produced artifact to its job on the controller.
    async fn attach_research_job_artifact(
        &self,
        job_id: &str,
        artifact_id: &str,
    ) -> Result<ResearchJob, DashboardError> {
        let path = format!(
            "/api/research/workers/jobs/{}/artifact/{}",
            Self::segment(job_id),
            Self::segment(artifact_id)
        );
        self.send_required(reqwest::Method::POST, &path, None::<&()>)
            .await
    }

    /// Create or update a report row on the controller.
    async fn create_or_update_research_report(
        &self,
        record: &ResearchReportRecord<'_>,
    ) -> Result<ResearchReport, DashboardError> {
        self.send_required(
            reqwest::Method::POST,
            "/api/research/workers/reports",
            Some(&WorkerReportUpsertRequest::from_record(record)),
        )
        .await
    }

    /// Upload report documents so the controller can serve them.
    async fn store_research_report_documents(
        &self,
        report_id: &str,
        report_json: &str,
        report_csv: &str,
    ) -> Result<(), DashboardError> {
        let path = format!(
            "/api/research/workers/reports/{}/documents",
            Self::segment(report_id)
        );
        let _: ResearchReport = self
            .send_required(
                reqwest::Method::PUT,
                &path,
                Some(&WorkerReportDocumentsRequest {
                    report_json: report_json.to_string(),
                    report_csv: report_csv.to_string(),
                }),
            )
            .await?;
        Ok(())
    }

    /// Upload artifact documents so the controller can serve them.
    async fn store_research_artifact_documents(
        &self,
        artifact_id: &str,
        manifest_json: Option<&str>,
        checksums_text: Option<&str>,
    ) -> Result<(), DashboardError> {
        if manifest_json.is_none() && checksums_text.is_none() {
            return Ok(());
        }
        let path = format!(
            "/api/research/workers/artifacts/{}/documents",
            Self::segment(artifact_id)
        );
        let _: ResearchArtifact = self
            .send_required(
                reqwest::Method::PUT,
                &path,
                Some(&WorkerArtifactDocumentsRequest {
                    manifest_json: manifest_json.map(str::to_string),
                    checksums_text: checksums_text.map(str::to_string),
                }),
            )
            .await?;
        Ok(())
    }

    /// Claim the next queued transfer from the controller.
    async fn claim_next_artifact_transfer(
        &self,
        dest_machine_id: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        self.send(
            reqwest::Method::POST,
            "/api/research/workers/transfers/claim",
            Some(&ClaimTransferRequest {
                dest_machine_id: dest_machine_id.to_string(),
            }),
        )
        .await
    }

    /// Fetch one transfer from the controller.
    async fn get_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        let path = format!("/api/research/workers/transfers/{}", Self::segment(id));
        self.send(reqwest::Method::GET, &path, None::<&()>).await
    }

    /// Update transfer progress on the controller.
    async fn update_artifact_transfer_progress(
        &self,
        id: &str,
        status: &str,
        bytes_done: Option<u64>,
        bytes_total: Option<u64>,
        checksum_status: Option<&str>,
        error: Option<&str>,
    ) -> Result<ArtifactTransfer, DashboardError> {
        let path = format!(
            "/api/research/workers/transfers/{}/progress",
            Self::segment(id)
        );
        self.send_required(
            reqwest::Method::POST,
            &path,
            Some(&WorkerTransferProgressRequest {
                status: status.to_string(),
                bytes_done,
                bytes_total,
                checksum_status: checksum_status.map(str::to_string),
                error: error.map(str::to_string),
            }),
        )
        .await
    }

    /// Recover stale running transfers on the controller.
    async fn recover_stale_artifact_transfers(
        &self,
        dest_machine_id: &str,
        stale_after_ms: u64,
    ) -> Result<usize, DashboardError> {
        let response: RecoverTransfersResponse = self
            .send_required(
                reqwest::Method::POST,
                "/api/research/workers/transfers/recover",
                Some(&RecoverTransfersRequest {
                    dest_machine_id: dest_machine_id.to_string(),
                    stale_after_ms,
                }),
            )
            .await?;
        Ok(response.recovered)
    }

    /// Fetch one machine from the controller.
    async fn get_research_machine(
        &self,
        id: &str,
    ) -> Result<Option<ResearchMachine>, DashboardError> {
        let path = format!("/api/research/workers/machines/{}", Self::segment(id));
        self.send(reqwest::Method::GET, &path, None::<&()>).await
    }
}

/// Work backend selected by the research worker at startup.
pub enum WorkerBackend {
    /// Lease and persist research work in the local `SQLite` database.
    Local(Arc<DashboardDb>),
    /// Lease and persist research work on a remote public controller.
    Remote(ResearchControllerClient),
}

impl WorkerBackend {
    /// Operator-facing name of the selected work source.
    pub fn describe(&self) -> &'static str {
        match self {
            WorkerBackend::Local(_) => "local database",
            WorkerBackend::Remote(_) => "remote controller",
        }
    }
}

impl ResearchWorkBackend for WorkerBackend {
    /// Lease the next executable research step.
    async fn lease_next_research_step(
        &self,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<Option<ResearchStepLease>, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .lease_next_research_step(worker_id, lease_duration_ms)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .lease_next_research_step(worker_id, lease_duration_ms)
                    .await
            }
        }
    }

    /// Refresh a step lease.
    async fn refresh_research_step_lease(
        &self,
        step_id: &str,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<ResearchJobStep, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .refresh_research_step_lease(step_id, worker_id, lease_duration_ms)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .refresh_research_step_lease(step_id, worker_id, lease_duration_ms)
                    .await
            }
        }
    }

    /// Mark a step running.
    async fn mark_research_step_running(
        &self,
        step_id: &str,
        worker_id: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .mark_research_step_running(step_id, worker_id)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client.mark_research_step_running(step_id, worker_id).await
            }
        }
    }

    /// Complete a step.
    async fn complete_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        output_json: Option<&str>,
    ) -> Result<ResearchJobStep, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .complete_research_step(step_id, worker_id, output_json)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .complete_research_step(step_id, worker_id, output_json)
                    .await
            }
        }
    }

    /// Fail a step.
    async fn fail_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        error: &str,
        retryable: bool,
    ) -> Result<ResearchJobStep, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .fail_research_step(step_id, worker_id, error, retryable)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .fail_research_step(step_id, worker_id, error, retryable)
                    .await
            }
        }
    }

    /// Block a step.
    async fn block_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        reason: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .block_research_step(step_id, worker_id, reason)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client.block_research_step(step_id, worker_id, reason).await
            }
        }
    }

    /// Append a job event.
    async fn append_research_job_event(
        &self,
        job_id: &str,
        step_id: Option<&str>,
        level: &str,
        message: &str,
        details_json: Option<&str>,
    ) -> Result<ResearchJobEvent, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .append_research_job_event(job_id, step_id, level, message, details_json)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .append_research_job_event(job_id, step_id, level, message, details_json)
                    .await
            }
        }
    }

    /// Fetch one job.
    async fn get_research_job(&self, id: &str) -> Result<Option<ResearchJob>, DashboardError> {
        match self {
            WorkerBackend::Local(db) => db.as_ref().get_research_job(id).await,
            WorkerBackend::Remote(client) => client.get_research_job(id).await,
        }
    }

    /// Cancel one job.
    async fn cancel_research_job(&self, id: &str) -> Result<ResearchJob, DashboardError> {
        match self {
            WorkerBackend::Local(db) => db.as_ref().cancel_research_job(id).await,
            WorkerBackend::Remote(client) => client.cancel_research_job(id).await,
        }
    }

    /// Fetch job steps.
    async fn get_research_job_steps(
        &self,
        job_id: &str,
    ) -> Result<Vec<ResearchJobStep>, DashboardError> {
        match self {
            WorkerBackend::Local(db) => db.as_ref().get_research_job_steps(job_id).await,
            WorkerBackend::Remote(client) => client.get_research_job_steps(job_id).await,
        }
    }

    /// Fetch one artifact.
    async fn get_research_artifact(
        &self,
        id: &str,
    ) -> Result<Option<ResearchArtifact>, DashboardError> {
        match self {
            WorkerBackend::Local(db) => db.as_ref().get_research_artifact(id).await,
            WorkerBackend::Remote(client) => client.get_research_artifact(id).await,
        }
    }

    /// Upsert an artifact.
    async fn upsert_research_artifact(
        &self,
        record: &ResearchArtifactRecord<'_>,
    ) -> Result<ResearchArtifact, DashboardError> {
        match self {
            WorkerBackend::Local(db) => db.as_ref().upsert_research_artifact(record).await,
            WorkerBackend::Remote(client) => client.upsert_research_artifact(record).await,
        }
    }

    /// Attach a produced artifact to its job.
    async fn attach_research_job_artifact(
        &self,
        job_id: &str,
        artifact_id: &str,
    ) -> Result<ResearchJob, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .attach_research_job_artifact(job_id, artifact_id)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .attach_research_job_artifact(job_id, artifact_id)
                    .await
            }
        }
    }

    /// Create or update a report row.
    async fn create_or_update_research_report(
        &self,
        record: &ResearchReportRecord<'_>,
    ) -> Result<ResearchReport, DashboardError> {
        match self {
            WorkerBackend::Local(db) => db.as_ref().create_or_update_research_report(record).await,
            WorkerBackend::Remote(client) => client.create_or_update_research_report(record).await,
        }
    }

    /// Publish report documents.
    async fn store_research_report_documents(
        &self,
        report_id: &str,
        report_json: &str,
        report_csv: &str,
    ) -> Result<(), DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .store_research_report_documents(report_id, report_json, report_csv)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .store_research_report_documents(report_id, report_json, report_csv)
                    .await
            }
        }
    }

    /// Publish artifact documents.
    async fn store_research_artifact_documents(
        &self,
        artifact_id: &str,
        manifest_json: Option<&str>,
        checksums_text: Option<&str>,
    ) -> Result<(), DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .store_research_artifact_documents(artifact_id, manifest_json, checksums_text)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .store_research_artifact_documents(artifact_id, manifest_json, checksums_text)
                    .await
            }
        }
    }

    /// Claim the next queued transfer.
    async fn claim_next_artifact_transfer(
        &self,
        dest_machine_id: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .claim_next_artifact_transfer(dest_machine_id)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client.claim_next_artifact_transfer(dest_machine_id).await
            }
        }
    }

    /// Fetch one transfer.
    async fn get_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        match self {
            WorkerBackend::Local(db) => db.as_ref().get_artifact_transfer(id).await,
            WorkerBackend::Remote(client) => client.get_artifact_transfer(id).await,
        }
    }

    /// Update transfer progress.
    async fn update_artifact_transfer_progress(
        &self,
        id: &str,
        status: &str,
        bytes_done: Option<u64>,
        bytes_total: Option<u64>,
        checksum_status: Option<&str>,
        error: Option<&str>,
    ) -> Result<ArtifactTransfer, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .update_artifact_transfer_progress(
                        id,
                        status,
                        bytes_done,
                        bytes_total,
                        checksum_status,
                        error,
                    )
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .update_artifact_transfer_progress(
                        id,
                        status,
                        bytes_done,
                        bytes_total,
                        checksum_status,
                        error,
                    )
                    .await
            }
        }
    }

    /// Recover stale running transfers.
    async fn recover_stale_artifact_transfers(
        &self,
        dest_machine_id: &str,
        stale_after_ms: u64,
    ) -> Result<usize, DashboardError> {
        match self {
            WorkerBackend::Local(db) => {
                db.as_ref()
                    .recover_stale_artifact_transfers(dest_machine_id, stale_after_ms)
                    .await
            }
            WorkerBackend::Remote(client) => {
                client
                    .recover_stale_artifact_transfers(dest_machine_id, stale_after_ms)
                    .await
            }
        }
    }

    /// Fetch one machine.
    async fn get_research_machine(
        &self,
        id: &str,
    ) -> Result<Option<ResearchMachine>, DashboardError> {
        match self {
            WorkerBackend::Local(db) => db.as_ref().get_research_machine(id).await,
            WorkerBackend::Remote(client) => client.get_research_machine(id).await,
        }
    }
}
