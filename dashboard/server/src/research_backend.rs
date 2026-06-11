//! Work-source abstraction for the research worker.
//!
//! The worker leases steps, reports progress, and persists results through
//! this trait so the same execution pipeline can run against the local
//! `SQLite` database (tests, local dev, private stacks) or a remote public
//! controller over authenticated HTTP (the deployed worker on `testing`).

use std::future::Future;

use crate::db::{
    ArtifactTransfer, DashboardDb, ResearchArtifact, ResearchArtifactRecord, ResearchJob,
    ResearchJobEvent, ResearchJobStep, ResearchMachine, ResearchReport, ResearchReportRecord,
    ResearchStepLease,
};
use crate::error::DashboardError;

/// Research work source used by the worker loops.
pub trait ResearchWorkBackend: Send + Sync {
    /// Lease the next executable research step for this worker.
    fn lease_next_research_step(
        &self,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> impl Future<Output = Result<Option<ResearchStepLease>, DashboardError>> + Send;

    /// Refresh the lease on an actively supervised step.
    fn refresh_research_step_lease(
        &self,
        step_id: &str,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> impl Future<Output = Result<ResearchJobStep, DashboardError>> + Send;

    /// Mark a leased step as running.
    fn mark_research_step_running(
        &self,
        step_id: &str,
        worker_id: &str,
    ) -> impl Future<Output = Result<ResearchJobStep, DashboardError>> + Send;

    /// Complete a step with optional output JSON.
    fn complete_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        output_json: Option<&str>,
    ) -> impl Future<Output = Result<ResearchJobStep, DashboardError>> + Send;

    /// Fail a step, optionally leaving it retryable.
    fn fail_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        error: &str,
        retryable: bool,
    ) -> impl Future<Output = Result<ResearchJobStep, DashboardError>> + Send;

    /// Block a step pending operator review.
    fn block_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        reason: &str,
    ) -> impl Future<Output = Result<ResearchJobStep, DashboardError>> + Send;

    /// Append a timeline event to a job.
    fn append_research_job_event(
        &self,
        job_id: &str,
        step_id: Option<&str>,
        level: &str,
        message: &str,
        details_json: Option<&str>,
    ) -> impl Future<Output = Result<ResearchJobEvent, DashboardError>> + Send;

    /// Fetch one job for context and cancellation polling.
    fn get_research_job(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<ResearchJob>, DashboardError>> + Send;

    /// Cancel one job after its supervised command observed cancellation.
    fn cancel_research_job(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<ResearchJob, DashboardError>> + Send;

    /// Fetch the steps of one job.
    fn get_research_job_steps(
        &self,
        job_id: &str,
    ) -> impl Future<Output = Result<Vec<ResearchJobStep>, DashboardError>> + Send;

    /// Fetch one artifact for job context.
    fn get_research_artifact(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<ResearchArtifact>, DashboardError>> + Send;

    /// Create or update an artifact record.
    fn upsert_research_artifact(
        &self,
        record: &ResearchArtifactRecord<'_>,
    ) -> impl Future<Output = Result<ResearchArtifact, DashboardError>> + Send;

    /// Attach an artifact produced by a job to that job.
    fn attach_research_job_artifact(
        &self,
        job_id: &str,
        artifact_id: &str,
    ) -> impl Future<Output = Result<ResearchJob, DashboardError>> + Send;

    /// Create or update the report row for a job.
    fn create_or_update_research_report(
        &self,
        record: &ResearchReportRecord<'_>,
    ) -> impl Future<Output = Result<ResearchReport, DashboardError>> + Send;

    /// Publish generated report documents to the work source.
    ///
    /// The local backend is a no-op because the generated files already live
    /// at the recorded paths. The remote backend uploads both documents so
    /// the public controller can serve them and rewrites the stored report
    /// paths to controller-rooted locations.
    fn store_research_report_documents(
        &self,
        report_id: &str,
        report_json: &str,
        report_csv: &str,
    ) -> impl Future<Output = Result<(), DashboardError>> + Send;

    /// Publish artifact manifest and checksum documents to the work source.
    ///
    /// The local backend is a no-op for the same reason as report documents.
    fn store_research_artifact_documents(
        &self,
        artifact_id: &str,
        manifest_json: Option<&str>,
        checksums_text: Option<&str>,
    ) -> impl Future<Output = Result<(), DashboardError>> + Send;

    /// Claim the next queued transfer destined for this machine.
    fn claim_next_artifact_transfer(
        &self,
        dest_machine_id: &str,
    ) -> impl Future<Output = Result<Option<ArtifactTransfer>, DashboardError>> + Send;

    /// Fetch one transfer for cancellation and progress checks.
    fn get_artifact_transfer(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<ArtifactTransfer>, DashboardError>> + Send;

    /// Update transfer progress, completion, or failure state.
    fn update_artifact_transfer_progress(
        &self,
        id: &str,
        status: &str,
        bytes_done: Option<u64>,
        bytes_total: Option<u64>,
        checksum_status: Option<&str>,
        error: Option<&str>,
    ) -> impl Future<Output = Result<ArtifactTransfer, DashboardError>> + Send;

    /// Recover running transfers whose updates have gone stale.
    fn recover_stale_artifact_transfers(
        &self,
        dest_machine_id: &str,
        stale_after_ms: u64,
    ) -> impl Future<Output = Result<usize, DashboardError>> + Send;

    /// Fetch one machine for transfer endpoint resolution.
    fn get_research_machine(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<ResearchMachine>, DashboardError>> + Send;
}

impl ResearchWorkBackend for DashboardDb {
    /// Lease the next executable research step from the local database.
    async fn lease_next_research_step(
        &self,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<Option<ResearchStepLease>, DashboardError> {
        DashboardDb::lease_next_research_step(self, worker_id, lease_duration_ms).await
    }

    /// Refresh a step lease in the local database.
    async fn refresh_research_step_lease(
        &self,
        step_id: &str,
        worker_id: &str,
        lease_duration_ms: u64,
    ) -> Result<ResearchJobStep, DashboardError> {
        DashboardDb::refresh_research_step_lease(self, step_id, worker_id, lease_duration_ms).await
    }

    /// Mark a step running in the local database.
    async fn mark_research_step_running(
        &self,
        step_id: &str,
        worker_id: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        DashboardDb::mark_research_step_running(self, step_id, worker_id).await
    }

    /// Complete a step in the local database.
    async fn complete_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        output_json: Option<&str>,
    ) -> Result<ResearchJobStep, DashboardError> {
        DashboardDb::complete_research_step(self, step_id, worker_id, output_json).await
    }

    /// Fail a step in the local database.
    async fn fail_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        error: &str,
        retryable: bool,
    ) -> Result<ResearchJobStep, DashboardError> {
        DashboardDb::fail_research_step(self, step_id, worker_id, error, retryable).await
    }

    /// Block a step in the local database.
    async fn block_research_step(
        &self,
        step_id: &str,
        worker_id: &str,
        reason: &str,
    ) -> Result<ResearchJobStep, DashboardError> {
        DashboardDb::block_research_step(self, step_id, worker_id, reason).await
    }

    /// Append a job event to the local database.
    async fn append_research_job_event(
        &self,
        job_id: &str,
        step_id: Option<&str>,
        level: &str,
        message: &str,
        details_json: Option<&str>,
    ) -> Result<ResearchJobEvent, DashboardError> {
        DashboardDb::append_research_job_event(self, job_id, step_id, level, message, details_json)
            .await
    }

    /// Fetch one job from the local database.
    async fn get_research_job(&self, id: &str) -> Result<Option<ResearchJob>, DashboardError> {
        DashboardDb::get_research_job(self, id).await
    }

    /// Cancel one job in the local database.
    async fn cancel_research_job(&self, id: &str) -> Result<ResearchJob, DashboardError> {
        DashboardDb::cancel_research_job(self, id).await
    }

    /// Fetch job steps from the local database.
    async fn get_research_job_steps(
        &self,
        job_id: &str,
    ) -> Result<Vec<ResearchJobStep>, DashboardError> {
        DashboardDb::get_research_job_steps(self, job_id).await
    }

    /// Fetch one artifact from the local database.
    async fn get_research_artifact(
        &self,
        id: &str,
    ) -> Result<Option<ResearchArtifact>, DashboardError> {
        DashboardDb::get_research_artifact(self, id).await
    }

    /// Upsert an artifact in the local database.
    async fn upsert_research_artifact(
        &self,
        record: &ResearchArtifactRecord<'_>,
    ) -> Result<ResearchArtifact, DashboardError> {
        DashboardDb::upsert_research_artifact(self, record).await
    }

    /// Attach a produced artifact to its job in the local database.
    async fn attach_research_job_artifact(
        &self,
        job_id: &str,
        artifact_id: &str,
    ) -> Result<ResearchJob, DashboardError> {
        DashboardDb::attach_research_job_artifact(self, job_id, artifact_id).await
    }

    /// Create or update a report row in the local database.
    async fn create_or_update_research_report(
        &self,
        record: &ResearchReportRecord<'_>,
    ) -> Result<ResearchReport, DashboardError> {
        DashboardDb::create_or_update_research_report(self, record).await
    }

    /// Local report files already live at their recorded paths.
    async fn store_research_report_documents(
        &self,
        _report_id: &str,
        _report_json: &str,
        _report_csv: &str,
    ) -> Result<(), DashboardError> {
        Ok(())
    }

    /// Local artifact documents already live at their recorded paths.
    async fn store_research_artifact_documents(
        &self,
        _artifact_id: &str,
        _manifest_json: Option<&str>,
        _checksums_text: Option<&str>,
    ) -> Result<(), DashboardError> {
        Ok(())
    }

    /// Claim the next queued transfer from the local database.
    async fn claim_next_artifact_transfer(
        &self,
        dest_machine_id: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        DashboardDb::claim_next_artifact_transfer(self, dest_machine_id).await
    }

    /// Fetch one transfer from the local database.
    async fn get_artifact_transfer(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        DashboardDb::get_artifact_transfer(self, id).await
    }

    /// Update transfer progress in the local database.
    async fn update_artifact_transfer_progress(
        &self,
        id: &str,
        status: &str,
        bytes_done: Option<u64>,
        bytes_total: Option<u64>,
        checksum_status: Option<&str>,
        error: Option<&str>,
    ) -> Result<ArtifactTransfer, DashboardError> {
        DashboardDb::update_artifact_transfer_progress(
            self,
            id,
            status,
            bytes_done,
            bytes_total,
            checksum_status,
            error,
        )
        .await
    }

    /// Recover stale running transfers in the local database.
    async fn recover_stale_artifact_transfers(
        &self,
        dest_machine_id: &str,
        stale_after_ms: u64,
    ) -> Result<usize, DashboardError> {
        DashboardDb::recover_stale_artifact_transfers(self, dest_machine_id, stale_after_ms).await
    }

    /// Fetch one machine from the local database.
    async fn get_research_machine(
        &self,
        id: &str,
    ) -> Result<Option<ResearchMachine>, DashboardError> {
        DashboardDb::get_research_machine(self, id).await
    }
}
