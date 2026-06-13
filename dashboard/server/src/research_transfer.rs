//! Resumable artifact transfer execution for research workers.
//!
//! Transfer records live in the dashboard database. This module lets a research
//! worker claim queued transfers for its machine, copy the artifact into the
//! configured research work root, verify the manifest, and update artifact
//! metadata to the local destination path.

use std::io::{Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use crate::db::{ArtifactTransfer, ResearchArtifact, ResearchArtifactRecord, ResearchMachine};
use crate::error::DashboardError;
use crate::research_artifacts::{self, ArtifactManifest};
use crate::research_backend::ResearchWorkBackend;

/// Minimum operator-configured stale recovery age in milliseconds.
///
/// Stale recovery requeues a `running` transfer that any worker on the
/// destination machine owns. The system runs exactly one transfer worker per
/// research machine (see docs/deployment-and-ops.md), so a `running` row is the
/// live worker's in-flight copy. The recovery age must clearly exceed the
/// worst-case time for one single-file rsync so genuinely live transfers are
/// never requeued and two writers never share one destination root. One hour
/// is a conservative floor for a single large run artifact over SSH; values of
/// zero stay disabled and are not affected by this floor.
pub const MIN_SAFE_STALE_AFTER_MS: u64 = 60 * 60 * 1_000;

/// Runtime configuration for artifact transfer execution.
#[derive(Debug, Clone)]
pub struct ArtifactTransferConfig {
    /// Root directory that receives transferred artifacts.
    pub work_root: PathBuf,
    /// Machine ID owned by this worker.
    pub local_machine_id: String,
    /// Program used for remote `rsync` copies.
    pub rsync_program: PathBuf,
    /// Optional remote shell command passed to `rsync -e`.
    pub rsync_ssh: Option<String>,
    /// Age in milliseconds after which running transfers are recovered.
    pub stale_after_ms: Option<u64>,
}

/// Worker-side transfer executor.
pub struct ArtifactTransferWorker {
    config: ArtifactTransferConfig,
}

impl ArtifactTransferConfig {
    /// Build transfer config rooted at one research work directory.
    pub fn new(
        work_root: impl Into<PathBuf>,
        local_machine_id: impl Into<String>,
    ) -> Result<Self, DashboardError> {
        let work_root = normalize_path(&work_root.into())?;
        let local_machine_id = local_machine_id.into();
        if local_machine_id.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "local_machine_id must not be empty".to_string(),
            ));
        }
        std::fs::create_dir_all(&work_root)
            .map_err(|e| DashboardError::Internal(format!("creating transfer work root: {e}")))?;
        Ok(Self {
            work_root,
            local_machine_id,
            rsync_program: PathBuf::from("rsync"),
            rsync_ssh: None,
            stale_after_ms: Some(30 * 60 * 1_000),
        })
    }

    /// Return a copy with a custom `rsync` program path.
    #[must_use]
    pub fn with_rsync_program(mut self, program: impl Into<PathBuf>) -> Self {
        self.rsync_program = program.into();
        self
    }

    /// Return a copy with a custom `rsync -e` remote shell.
    #[must_use]
    pub fn with_rsync_ssh(mut self, command: Option<String>) -> Self {
        self.rsync_ssh = command.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        self
    }

    /// Return a copy with a custom stale running transfer age.
    #[must_use]
    pub fn with_stale_after_ms(mut self, stale_after_ms: Option<u64>) -> Self {
        self.stale_after_ms = stale_after_ms;
        self
    }

    /// Return the local destination root for one artifact ID.
    pub fn destination_root(&self, artifact_id: &str) -> Result<PathBuf, DashboardError> {
        let dir = artifact_dir_name(artifact_id)?;
        resolve_under_root(&self.work_root, &format!("artifacts/{dir}"))
    }
}

impl ArtifactTransferWorker {
    /// Create a transfer worker from runtime config.
    pub fn new(config: ArtifactTransferConfig) -> Self {
        Self { config }
    }

    /// Claim and process one queued or retryable transfer for this worker.
    pub async fn run_one(
        &self,
        db: &impl ResearchWorkBackend,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        let Some(transfer) = db
            .claim_next_artifact_transfer(&self.config.local_machine_id)
            .await?
        else {
            return Ok(None);
        };
        match self.run_claimed(db, &transfer).await {
            Ok(completed) => Ok(Some(completed)),
            Err(error) => self.mark_retryable(db, &transfer, &error.to_string()).await,
        }
    }

    /// Process available transfers up to a bounded limit.
    pub async fn run_until_idle(
        &self,
        db: &impl ResearchWorkBackend,
        max_transfers: usize,
    ) -> Result<usize, DashboardError> {
        if max_transfers == 0 {
            return Ok(0);
        }
        self.recover_stale(db).await?;
        let mut processed = 0;
        for _ in 0..max_transfers {
            if self.run_one(db).await?.is_none() {
                break;
            }
            processed += 1;
        }
        Ok(processed)
    }

    /// Recover stale running transfers for this worker's destination machine.
    pub async fn recover_stale(
        &self,
        db: &impl ResearchWorkBackend,
    ) -> Result<usize, DashboardError> {
        let Some(stale_after_ms) = self.config.stale_after_ms else {
            return Ok(0);
        };
        db.recover_stale_artifact_transfers(&self.config.local_machine_id, stale_after_ms)
            .await
    }

    /// Execute one transfer that has already been claimed.
    async fn run_claimed(
        &self,
        db: &impl ResearchWorkBackend,
        transfer: &ArtifactTransfer,
    ) -> Result<ArtifactTransfer, DashboardError> {
        let artifact = db
            .get_research_artifact(&transfer.artifact_id)
            .await?
            .ok_or_else(|| {
                DashboardError::NotFound(format!("artifact '{}' not found", transfer.artifact_id))
            })?;
        let source_machine = self.source_machine(db, transfer, &artifact).await?;
        let source_root = artifact_source_root(&artifact)?;
        let destination_root = self.config.destination_root(&artifact.id)?;
        let source_is_local = source_machine
            .as_ref()
            .is_none_or(|machine| machine.id == self.config.local_machine_id);

        if source_is_local {
            self.copy_local_artifact(db, transfer, &source_root, &destination_root)
                .await?;
        } else {
            let machine = source_machine.as_ref().ok_or_else(|| {
                DashboardError::BadRequest("remote transfer requires a source machine".to_string())
            })?;
            self.copy_remote_artifact(db, transfer, machine, &source_root, &destination_root)
                .await?;
        }

        let verification = research_artifacts::verify_artifact(&destination_root)?;
        self.upsert_local_artifact(db, &artifact, &destination_root, &verification)
            .await?;
        db.update_artifact_transfer_progress(
            &transfer.id,
            "completed",
            Some(verification.bytes_checked),
            Some(verification.bytes_checked),
            Some("verified"),
            None,
        )
        .await
    }

    /// Return the source machine declared by the transfer or artifact.
    async fn source_machine(
        &self,
        db: &impl ResearchWorkBackend,
        transfer: &ArtifactTransfer,
        artifact: &ResearchArtifact,
    ) -> Result<Option<ResearchMachine>, DashboardError> {
        let source_id = transfer
            .source_machine_id
            .as_deref()
            .or(artifact.source_machine_id.as_deref());
        let Some(source_id) = source_id else {
            return Ok(None);
        };
        db.get_research_machine(source_id).await?.map_or_else(
            || {
                Err(DashboardError::NotFound(format!(
                    "research machine '{source_id}' not found"
                )))
            },
            |machine| Ok(Some(machine)),
        )
    }

    /// Copy an artifact directory from a local source using resumable file appends.
    async fn copy_local_artifact(
        &self,
        db: &impl ResearchWorkBackend,
        transfer: &ArtifactTransfer,
        source_root: &Path,
        destination_root: &Path,
    ) -> Result<(), DashboardError> {
        if same_path(source_root, destination_root)? {
            let manifest = research_artifacts::read_manifest(source_root)?;
            let bytes_done = payload_bytes_from_manifest(destination_root, &manifest)?;
            self.update_running_progress(
                db,
                transfer,
                bytes_done.max(transfer.bytes_done),
                Some(manifest_payload_bytes(&manifest)),
            )
            .await?;
            return Ok(());
        }
        let manifest = research_artifacts::read_manifest(source_root)?;
        std::fs::create_dir_all(destination_root).map_err(|e| {
            DashboardError::Internal(format!("creating transfer destination root: {e}"))
        })?;
        copy_support_file(source_root, destination_root, "manifest.json")?;
        copy_support_file(source_root, destination_root, "checksums.sha256")?;
        let total = manifest_payload_bytes(&manifest);
        let mut copied = payload_bytes_from_manifest(destination_root, &manifest)?;
        self.update_running_progress(db, transfer, copied.max(transfer.bytes_done), Some(total))
            .await?;
        for file in &manifest.files {
            let source = research_artifacts::safe_join(source_root, &file.relative_path)?;
            let destination = research_artifacts::safe_join(destination_root, &file.relative_path)?;
            copy_file_resumable(&source, &destination)?;
            copied = payload_bytes_from_manifest(destination_root, &manifest)?;
            self.update_running_progress(db, transfer, copied, Some(total))
                .await?;
        }
        Ok(())
    }

    /// Copy an artifact directory from a remote source using `rsync` over SSH.
    async fn copy_remote_artifact(
        &self,
        db: &impl ResearchWorkBackend,
        transfer: &ArtifactTransfer,
        source_machine: &ResearchMachine,
        source_root: &Path,
        destination_root: &Path,
    ) -> Result<(), DashboardError> {
        let ssh_alias = source_machine.ssh_alias.as_deref().ok_or_else(|| {
            DashboardError::BadRequest(format!(
                "source machine '{}' has no ssh_alias",
                source_machine.id
            ))
        })?;
        std::fs::create_dir_all(destination_root).map_err(|e| {
            DashboardError::Internal(format!("creating transfer destination root: {e}"))
        })?;
        let bytes_done = destination_payload_bytes(destination_root)?;
        let bytes_total = transfer.bytes_total;
        self.update_running_progress(
            db,
            transfer,
            clamp_done(bytes_done.max(transfer.bytes_done), bytes_total),
            bytes_total,
        )
        .await?;
        let output = self.run_rsync(ssh_alias, source_root, destination_root)?;
        let bytes_done = destination_payload_bytes(destination_root)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DashboardError::Internal(format!(
                "rsync transfer failed with status {:?}: {stderr}",
                output.status.code()
            )));
        }
        self.update_running_progress(
            db,
            transfer,
            clamp_done(bytes_done, bytes_total),
            bytes_total,
        )
        .await?;
        Ok(())
    }

    /// Run one remote `rsync` process.
    fn run_rsync(
        &self,
        ssh_alias: &str,
        source_root: &Path,
        destination_root: &Path,
    ) -> Result<std::process::Output, DashboardError> {
        let spec = rsync_command_spec(
            &self.config.rsync_program,
            self.config.rsync_ssh.as_deref(),
            ssh_alias,
            source_root,
            destination_root,
        )?;
        let output = std::process::Command::new(&spec.program)
            .args(&spec.args)
            .output()
            .map_err(|e| DashboardError::Internal(format!("executing rsync transfer: {e}")))?;
        Ok(output)
    }

    /// Persist an artifact row that points at the verified local destination.
    async fn upsert_local_artifact(
        &self,
        db: &impl ResearchWorkBackend,
        artifact: &ResearchArtifact,
        destination_root: &Path,
        verification: &research_artifacts::ArtifactVerification,
    ) -> Result<(), DashboardError> {
        let manifest = research_artifacts::read_manifest(destination_root)?;
        let runtime_db_file = manifest
            .files
            .iter()
            .find(|file| file.logical_name == "runtime_db")
            .or_else(|| manifest.files.iter().find(|file| file.kind == "sqlite"));
        let source_db_path = runtime_db_file
            .map(|file| {
                research_artifacts::safe_join(destination_root, &file.relative_path)
                    .map(|path| path_to_string(&path))
            })
            .transpose()?;
        let checksum = runtime_db_file
            .map(|file| file.sha256.as_str())
            .or_else(|| manifest.files.first().map(|file| file.sha256.as_str()));
        let artifact_root = path_to_string(destination_root);
        let manifest_path = path_to_string(&destination_root.join("manifest.json"));
        db.upsert_research_artifact(&ResearchArtifactRecord {
            id: &manifest.artifact_id,
            source_machine_id: artifact
                .source_machine_id
                .as_deref()
                .or(manifest.source_machine_id.as_deref()),
            kind: &manifest.kind,
            status: "available",
            run_mode: manifest
                .run_mode
                .as_deref()
                .or(artifact.run_mode.as_deref()),
            artifact_root: Some(&artifact_root),
            manifest_path: Some(&manifest_path),
            bundle_path: artifact.bundle_path.as_deref(),
            source_db_path: source_db_path.as_deref(),
            interval_start_ms: manifest.interval_start_ms.or(artifact.interval_start_ms),
            interval_end_ms: manifest.interval_end_ms.or(artifact.interval_end_ms),
            bytes: Some(verification.bytes_checked),
            checksum,
            replay_quality_class: artifact.replay_quality_class.as_deref(),
            backtest_ready_class: artifact.backtest_ready_class.as_deref(),
            live_fidelity_class: artifact.live_fidelity_class.as_deref(),
        })
        .await?;
        Ok(())
    }

    /// Update transfer progress while preserving monotonic byte counts.
    async fn update_running_progress(
        &self,
        db: &impl ResearchWorkBackend,
        transfer: &ArtifactTransfer,
        bytes_done: u64,
        bytes_total: Option<u64>,
    ) -> Result<ArtifactTransfer, DashboardError> {
        db.update_artifact_transfer_progress(
            &transfer.id,
            "running",
            Some(bytes_done),
            bytes_total,
            Some("verifying"),
            None,
        )
        .await
    }

    /// Mark a failed transfer as retryable unless it became terminal meanwhile.
    async fn mark_retryable(
        &self,
        db: &impl ResearchWorkBackend,
        transfer: &ArtifactTransfer,
        error: &str,
    ) -> Result<Option<ArtifactTransfer>, DashboardError> {
        let current = db
            .get_artifact_transfer(&transfer.id)
            .await?
            .ok_or_else(|| {
                DashboardError::NotFound(format!("artifact transfer '{}' not found", transfer.id))
            })?;
        if matches!(current.status.as_str(), "completed" | "cancelled") {
            return Ok(Some(current));
        }
        let failed = db
            .update_artifact_transfer_progress(
                &transfer.id,
                "retryable",
                Some(current.bytes_done),
                current.bytes_total,
                Some("failed"),
                Some(error),
            )
            .await?;
        Ok(Some(failed))
    }
}

/// Serializable remote command specification used by tests and process execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsyncCommandSpec {
    /// Program path.
    pub program: String,
    /// Ordered arguments.
    pub args: Vec<String>,
}

/// Build the `rsync` command used for remote artifact copies.
pub fn rsync_command_spec(
    program: &Path,
    rsync_ssh: Option<&str>,
    ssh_alias: &str,
    source_root: &Path,
    destination_root: &Path,
) -> Result<RsyncCommandSpec, DashboardError> {
    if ssh_alias.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "ssh_alias must not be empty".to_string(),
        ));
    }
    let mut args = vec![
        "-a".to_string(),
        "--partial".to_string(),
        "--append-verify".to_string(),
        "--compress".to_string(),
        "--protect-args".to_string(),
        "--stats".to_string(),
    ];
    if let Some(command) = rsync_ssh.filter(|value| !value.trim().is_empty()) {
        args.push("-e".to_string());
        args.push(command.to_string());
    }
    let source = format!(
        "{}:{}/",
        ssh_alias,
        path_to_string(source_root).trim_end_matches('/')
    );
    let destination = format!(
        "{}/",
        path_to_string(destination_root).trim_end_matches('/')
    );
    args.push(source);
    args.push(destination);
    Ok(RsyncCommandSpec {
        program: path_to_string(program),
        args,
    })
}

/// Return an artifact source directory from stored artifact metadata.
fn artifact_source_root(artifact: &ResearchArtifact) -> Result<PathBuf, DashboardError> {
    if let Some(root) = artifact.artifact_root.as_deref() {
        return normalize_path(Path::new(root));
    }
    if let Some(manifest_path) = artifact.manifest_path.as_deref()
        && let Some(parent) = Path::new(manifest_path).parent()
    {
        return normalize_path(parent);
    }
    Err(DashboardError::BadRequest(format!(
        "artifact '{}' has no artifact_root or manifest_path",
        artifact.id
    )))
}

/// Return a safe single-directory name for an artifact ID.
fn artifact_dir_name(artifact_id: &str) -> Result<String, DashboardError> {
    let normalized = research_artifacts::normalize_relative_path(artifact_id)?;
    if normalized != artifact_id || normalized.contains('/') || normalized.contains('\\') {
        return Err(DashboardError::BadRequest(format!(
            "artifact_id is not a safe directory name: {artifact_id}"
        )));
    }
    Ok(normalized)
}

/// Copy a manifest sidecar file when it exists.
fn copy_support_file(
    source_root: &Path,
    destination_root: &Path,
    relative_path: &str,
) -> Result<(), DashboardError> {
    let source = research_artifacts::safe_join(source_root, relative_path)?;
    if !source.exists() {
        return Ok(());
    }
    let destination = research_artifacts::safe_join(destination_root, relative_path)?;
    copy_file_resumable(&source, &destination)
}

/// Copy one file with append-style resume when the destination is shorter.
fn copy_file_resumable(source: &Path, destination: &Path) -> Result<(), DashboardError> {
    let source_len = source
        .metadata()
        .map_err(|e| DashboardError::Internal(format!("stat transfer source: {e}")))?
        .len();
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            DashboardError::Internal(format!("creating transfer destination parent: {e}"))
        })?;
    }
    let mut destination_len = destination.metadata().map_or(0, |metadata| metadata.len());
    if destination_len > source_len {
        std::fs::remove_file(destination).map_err(|e| {
            DashboardError::Internal(format!("removing oversized transfer destination: {e}"))
        })?;
        destination_len = 0;
    }
    if destination_len == source_len {
        return Ok(());
    }
    let mut source_file = std::fs::File::open(source)
        .map_err(|e| DashboardError::Internal(format!("opening transfer source: {e}")))?;
    source_file
        .seek(SeekFrom::Start(destination_len))
        .map_err(|e| DashboardError::Internal(format!("seeking transfer source: {e}")))?;
    let mut destination_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(destination)
        .map_err(|e| DashboardError::Internal(format!("opening transfer destination: {e}")))?;
    std::io::copy(&mut source_file, &mut destination_file)
        .map_err(|e| DashboardError::Internal(format!("copying transfer file: {e}")))?;
    Ok(())
}

/// Sum actual destination bytes for files listed in a manifest.
fn payload_bytes_from_manifest(
    root: &Path,
    manifest: &ArtifactManifest,
) -> Result<u64, DashboardError> {
    let mut bytes = 0_u64;
    for file in &manifest.files {
        let path = research_artifacts::safe_join(root, &file.relative_path)?;
        bytes = bytes.saturating_add(path.metadata().map_or(0, |metadata| metadata.len()));
    }
    Ok(bytes)
}

/// Sum expected payload bytes recorded in a manifest.
fn manifest_payload_bytes(manifest: &ArtifactManifest) -> u64 {
    manifest
        .files
        .iter()
        .map(|file| file.bytes)
        .fold(0_u64, u64::saturating_add)
}

/// Return destination payload bytes using a manifest when available.
fn destination_payload_bytes(root: &Path) -> Result<u64, DashboardError> {
    match research_artifacts::read_manifest(root) {
        Ok(manifest) => payload_bytes_from_manifest(root, &manifest),
        Err(_) => dir_size(root),
    }
}

/// Return recursive file bytes for a directory that may not yet have a manifest.
fn dir_size(root: &Path) -> Result<u64, DashboardError> {
    if !root.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in std::fs::read_dir(root)
        .map_err(|e| DashboardError::Internal(format!("reading transfer destination: {e}")))?
    {
        let entry = entry.map_err(|e| {
            DashboardError::Internal(format!("reading transfer directory entry: {e}"))
        })?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|e| DashboardError::Internal(format!("stat transfer path: {e}")))?;
        if metadata.is_dir() {
            total = total.saturating_add(dir_size(&path)?);
        } else if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

/// Clamp bytes done to the known total when a total is available.
fn clamp_done(bytes_done: u64, bytes_total: Option<u64>) -> u64 {
    bytes_total.map_or(bytes_done, |total| bytes_done.min(total))
}

/// Resolve one relative path under a root.
fn resolve_under_root(root: &Path, path: &str) -> Result<PathBuf, DashboardError> {
    let candidate = normalize_path(&root.join(path))?;
    if !candidate.starts_with(root) {
        return Err(DashboardError::BadRequest(format!(
            "transfer path escapes configured root: {}",
            path_to_string(&candidate)
        )));
    }
    Ok(candidate)
}

/// Normalize a path lexically and reject parent traversal.
fn normalize_path(path: &Path) -> Result<PathBuf, DashboardError> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                out.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(DashboardError::BadRequest(format!(
                    "transfer paths must not contain parent traversal: {}",
                    path_to_string(path)
                )));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(DashboardError::BadRequest(
            "transfer path must not be empty".to_string(),
        ));
    }
    Ok(out)
}

/// Return whether two lexical paths normalize to the same path.
fn same_path(left: &Path, right: &Path) -> Result<bool, DashboardError> {
    Ok(normalize_path(left)? == normalize_path(right)?)
}

/// Convert a path to a stable lossy string.
fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Raise an operator-configured stale recovery age to the single-worker safety floor.
///
/// Returns `None` unchanged so a value of zero keeps automatic recovery disabled.
/// Any nonzero age below `MIN_SAFE_STALE_AFTER_MS` is raised to the floor so a
/// long single-file transfer owned by the one live worker is never requeued.
pub fn clamp_operator_stale_after_ms(stale_after_ms: Option<u64>) -> Option<u64> {
    stale_after_ms.map(|value| value.max(MIN_SAFE_STALE_AFTER_MS))
}

#[cfg(test)]
#[path = "tests/research_transfer_tests.rs"]
mod tests;
