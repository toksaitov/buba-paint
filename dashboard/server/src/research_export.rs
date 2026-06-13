//! Planning and local filesystem execution for research artifact exports.
//!
//! Export jobs use these helpers to validate operator-provided paths, snapshot
//! a runtime `SQLite` database through the backup API, copy requested logs, and
//! write the manifest that makes the resulting directory reusable by research
//! workers.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, DatabaseName, OpenFlags};
use serde::Serialize;

use crate::error::DashboardError;
use crate::research_artifacts::{
    ArtifactFileSpec, ArtifactManifest, build_manifest, checksum_text, write_manifest_files,
};
use crate::research_pipeline::ResearchPipelineConfig;
use crate::research_util::path_to_string;

/// Validated plan for exporting one runtime data set into the research work root.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResearchExportPlan {
    /// Artifact ID that will be attached to the exported files.
    pub artifact_id: String,
    /// Destination directory under the configured research work root.
    pub artifact_root: PathBuf,
    /// Source runtime `SQLite` database.
    pub source_db_path: PathBuf,
    /// Optional logs that will be copied into the artifact.
    pub log_paths: Vec<PathBuf>,
    /// Source run mode provided by the operator or defaulted by the planner.
    pub run_mode: String,
    /// Source process state, such as `stopped` or `running_readonly`.
    pub source_state: String,
    /// Artifact kind that will be written to the manifest and database.
    pub artifact_kind: String,
    /// Whether this plan should only report facts without writing files.
    pub dry_run: bool,
    /// Operator confirmation required before non-dry-run exports.
    pub confirm_export: bool,
    /// Optional replay interval start in milliseconds.
    pub interval_start_ms: Option<u64>,
    /// Optional replay interval end in milliseconds.
    pub interval_end_ms: Option<u64>,
    /// Estimated payload bytes excluding transient `WAL`/`SHM` sidecars.
    pub estimated_bytes: u64,
    /// Source database byte size.
    pub source_db_bytes: u64,
    /// Detected `WAL` sidecar byte size.
    pub source_wal_bytes: u64,
    /// Detected `SHM` sidecar byte size.
    pub source_shm_bytes: u64,
    /// Total bytes across copied log files.
    pub log_bytes: u64,
    /// Planner safety result: `safe`, `snapshot_required`, or `blocked`.
    pub safety_status: String,
    /// Human-readable reasons explaining non-`safe` planner status.
    pub safety_reasons: Vec<String>,
}

/// Filesystem result after snapshotting/copying runtime inputs.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResearchExportResult {
    /// Artifact ID from the export plan.
    pub artifact_id: String,
    /// Destination artifact directory.
    pub artifact_root: PathBuf,
    /// Copied runtime database path inside the artifact.
    pub runtime_db_path: PathBuf,
    /// Copied log file paths inside the artifact.
    pub copied_log_paths: Vec<PathBuf>,
    /// Bytes written by the export operation.
    pub bytes_written: u64,
    /// Whether the source was running and therefore captured as a snapshot.
    pub snapshot: bool,
}

/// Result returned after writing the artifact manifest and checksum sidecar.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResearchExportManifestResult {
    /// Artifact ID written to the manifest.
    pub artifact_id: String,
    /// Artifact directory containing the manifest.
    pub artifact_root: PathBuf,
    /// Path to `manifest.json`.
    pub manifest_path: PathBuf,
    /// Files included in the manifest.
    pub files: Vec<ResearchExportedFile>,
    /// Total manifest payload bytes.
    pub bytes: u64,
    /// Digest of the generated checksum sidecar text.
    pub checksum: String,
}

/// File entry prepared for an exported artifact manifest.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResearchExportedFile {
    /// Stable logical name used by downstream jobs.
    pub logical_name: String,
    /// Exported file kind, such as `sqlite`, `log`, or `json`.
    pub kind: String,
    /// Artifact-root relative path.
    pub relative_path: String,
}

#[derive(Debug, serde::Deserialize)]
struct RawExportParams {
    source_db_path: Option<String>,
    artifact_id: Option<String>,
    artifact_root: Option<String>,
    run_mode: Option<String>,
    source_state: Option<String>,
    interval_start_ms: Option<u64>,
    interval_end_ms: Option<u64>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    #[serde(default)]
    log_paths: Vec<String>,
    dry_run: Option<bool>,
    #[serde(default)]
    confirm_export: bool,
}

/// Build a safe export plan for one research export job.
pub fn plan_export(
    pipeline: &ResearchPipelineConfig,
    job_id: &str,
    params_json: Option<&str>,
) -> Result<ResearchExportPlan, DashboardError> {
    let raw = parse_export_params(params_json)?;
    let artifact_id = raw
        .artifact_id
        .unwrap_or_else(|| format!("research-export-{job_id}"));
    validate_id(&artifact_id, "artifact_id")?;
    let artifact_root =
        resolve_artifact_root(pipeline, raw.artifact_root.as_deref(), &artifact_id)?;
    let source_db_path = raw
        .source_db_path
        .as_deref()
        .ok_or_else(|| DashboardError::BadRequest("source_db_path is required".to_string()))
        .and_then(resolve_read_path)?;
    reject_sidecar_path(&source_db_path, "source_db_path")?;
    if !source_db_path.exists() {
        return Err(DashboardError::BadRequest(format!(
            "source_db_path does not exist: {}",
            path_to_string(&source_db_path)
        )));
    }

    let log_paths = raw
        .log_paths
        .iter()
        .map(|path| {
            resolve_read_path(path)
                .and_then(|resolved| reject_sidecar_path(&resolved, "log_paths").map(|()| resolved))
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_log_names(&log_paths)?;
    let run_mode = raw.run_mode.unwrap_or_else(|| "live_readonly".to_string());
    validate_run_mode(&run_mode)?;
    let source_state = raw.source_state.unwrap_or_else(|| "stopped".to_string());
    validate_source_state(&source_state)?;
    let dry_run = raw.dry_run.unwrap_or(true);
    let source_db_bytes = file_size(&source_db_path)?;
    let source_wal_bytes = file_size_optional(&sidecar_path(&source_db_path, "wal"))?;
    let source_shm_bytes = file_size_optional(&sidecar_path(&source_db_path, "shm"))?;
    let log_bytes = log_paths
        .iter()
        .map(|path| file_size(path))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum();
    let interval_start_ms = raw.interval_start_ms.or(raw.start_ms);
    let interval_end_ms = raw.interval_end_ms.or(raw.end_ms);
    let mut safety_reasons = Vec::new();
    if run_mode == "live_trading" {
        safety_reasons
            .push("funded live_trading exports must use buba-paint live-closeout".to_string());
    }
    if source_state == "running_readonly" {
        safety_reasons.push(
            "running readonly source will be exported with SQLite backup and marked as snapshot"
                .to_string(),
        );
    }
    if source_wal_bytes > 0 {
        safety_reasons
            .push("source WAL exists; export will not raw-copy WAL sidecar bytes".to_string());
    }
    if !dry_run && !raw.confirm_export {
        safety_reasons.push("real export requires confirm_export=true".to_string());
    }
    let safety_status = if run_mode == "live_trading" || (!dry_run && !raw.confirm_export) {
        "blocked"
    } else if source_state == "running_readonly" || source_wal_bytes > 0 {
        "snapshot_required"
    } else {
        "safe"
    }
    .to_string();
    let artifact_kind = if source_state == "running_readonly" {
        "readonly_run_snapshot"
    } else {
        "readonly_run"
    }
    .to_string();

    Ok(ResearchExportPlan {
        artifact_id,
        artifact_root,
        source_db_path,
        log_paths,
        run_mode,
        source_state,
        artifact_kind,
        dry_run,
        confirm_export: raw.confirm_export,
        interval_start_ms,
        interval_end_ms,
        estimated_bytes: source_db_bytes.saturating_add(log_bytes),
        source_db_bytes,
        source_wal_bytes,
        source_shm_bytes,
        log_bytes,
        safety_status,
        safety_reasons,
    })
}

/// Export runtime DB and logs for a confirmed non-dry-run plan.
pub fn export_runtime_files(
    plan: &ResearchExportPlan,
) -> Result<ResearchExportResult, DashboardError> {
    ensure_real_export_allowed(plan)?;
    let runtime_dir = plan.artifact_root.join("runtime");
    let logs_dir = plan.artifact_root.join("logs");
    std::fs::create_dir_all(&runtime_dir)
        .map_err(|e| DashboardError::Internal(format!("creating export runtime dir: {e}")))?;
    std::fs::create_dir_all(&logs_dir)
        .map_err(|e| DashboardError::Internal(format!("creating export logs dir: {e}")))?;
    let runtime_db_path = runtime_dir.join("paint.db");
    remove_sqlite_family(&runtime_db_path)?;
    backup_sqlite_db(&plan.source_db_path, &runtime_db_path)?;
    let mut copied_log_paths = Vec::new();
    for log_path in &plan.log_paths {
        let file_name = log_path.file_name().ok_or_else(|| {
            DashboardError::BadRequest(format!(
                "log path has no file name: {}",
                path_to_string(log_path)
            ))
        })?;
        let dest = logs_dir.join(file_name);
        std::fs::copy(log_path, &dest)
            .map_err(|e| DashboardError::Internal(format!("copying export log: {e}")))?;
        copied_log_paths.push(dest);
    }
    let bytes_written = file_size(&runtime_db_path)?.saturating_add(
        copied_log_paths
            .iter()
            .map(|path| file_size(path))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum::<u64>(),
    );
    Ok(ResearchExportResult {
        artifact_id: plan.artifact_id.clone(),
        artifact_root: plan.artifact_root.clone(),
        runtime_db_path,
        copied_log_paths,
        bytes_written,
        snapshot: plan.source_state == "running_readonly",
    })
}

/// Write an artifact manifest for an exported runtime package.
pub fn write_export_manifest(
    plan: &ResearchExportPlan,
) -> Result<ResearchExportManifestResult, DashboardError> {
    ensure_real_export_allowed(plan)?;
    let files = export_file_specs(plan)?;
    write_export_summary(plan, files.len())?;
    let manifest = build_manifest(
        &plan.artifact_root,
        &plan.artifact_id,
        &plan.artifact_kind,
        Some("live"),
        Some(&plan.run_mode),
        now_ms()?,
        plan.interval_start_ms,
        plan.interval_end_ms,
        &files
            .iter()
            .map(|file| ArtifactFileSpec {
                logical_name: file.logical_name.clone(),
                kind: file.kind.clone(),
                relative_path: file.relative_path.clone(),
            })
            .collect::<Vec<_>>(),
    )?;
    let bytes = manifest.files.iter().map(|file| file.bytes).sum();
    let checksum = manifest_checksum(&manifest);
    write_manifest_files(&plan.artifact_root, &manifest)?;
    Ok(ResearchExportManifestResult {
        artifact_id: plan.artifact_id.clone(),
        artifact_root: plan.artifact_root.clone(),
        manifest_path: plan.artifact_root.join("manifest.json"),
        files: manifest
            .files
            .iter()
            .map(|file| ResearchExportedFile {
                logical_name: file.logical_name.clone(),
                kind: file.kind.clone(),
                relative_path: file.relative_path.clone(),
            })
            .collect(),
        bytes,
        checksum,
    })
}

/// Return whether one plan represents a dry-run export.
pub fn is_dry_run(plan: &ResearchExportPlan) -> bool {
    plan.dry_run
}

/// Serialize an export value to JSON for worker outputs.
pub fn to_json<T: Serialize>(value: &T) -> Result<String, DashboardError> {
    serde_json::to_string(value)
        .map_err(|e| DashboardError::Internal(format!("serializing research export: {e}")))
}

/// Parse optional export params JSON.
fn parse_export_params(params_json: Option<&str>) -> Result<RawExportParams, DashboardError> {
    match params_json {
        Some(value) if !value.trim().is_empty() => serde_json::from_str(value)
            .map_err(|e| DashboardError::BadRequest(format!("invalid export params_json: {e}"))),
        _ => Ok(RawExportParams {
            source_db_path: None,
            artifact_id: None,
            artifact_root: None,
            run_mode: None,
            source_state: None,
            interval_start_ms: None,
            interval_end_ms: None,
            start_ms: None,
            end_ms: None,
            log_paths: Vec::new(),
            dry_run: None,
            confirm_export: false,
        }),
    }
}

/// Validate a stable operator-provided identifier.
fn validate_id(value: &str, field: &str) -> Result<(), DashboardError> {
    if value.trim().is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(DashboardError::BadRequest(format!(
            "{field} must contain only ASCII letters, digits, '-' or '_'"
        )));
    }
    Ok(())
}

/// Resolve the destination artifact root under the configured work root.
fn resolve_artifact_root(
    pipeline: &ResearchPipelineConfig,
    configured: Option<&str>,
    artifact_id: &str,
) -> Result<PathBuf, DashboardError> {
    let default = format!("artifacts/{artifact_id}");
    resolve_under_root(&pipeline.work_root, configured.unwrap_or(&default))
}

/// Resolve one existing readable file path.
fn resolve_read_path(path: &str) -> Result<PathBuf, DashboardError> {
    if path.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "export source paths must not be empty".to_string(),
        ));
    }
    normalize_path(Path::new(path))
}

/// Reject direct use of `SQLite` sidecar files.
fn reject_sidecar_path(path: &Path, field: &str) -> Result<(), DashboardError> {
    let value = path_to_string(path);
    if value.ends_with(".db-wal")
        || value.ends_with(".db-shm")
        || value.ends_with("-wal")
        || value.ends_with("-shm")
    {
        return Err(DashboardError::BadRequest(format!(
            "{field} must not point at a SQLite WAL or SHM sidecar"
        )));
    }
    Ok(())
}

/// Validate that log destinations will not collide.
fn validate_unique_log_names(paths: &[PathBuf]) -> Result<(), DashboardError> {
    let mut names = HashSet::new();
    for path in paths {
        if !path.exists() {
            return Err(DashboardError::BadRequest(format!(
                "log path does not exist: {}",
                path_to_string(path)
            )));
        }
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| {
                DashboardError::BadRequest(format!(
                    "log path has no file name: {}",
                    path_to_string(path)
                ))
            })?;
        if !names.insert(name.to_string()) {
            return Err(DashboardError::BadRequest(format!(
                "duplicate export log file name: {name}"
            )));
        }
    }
    Ok(())
}

/// Validate the supported export run modes.
fn validate_run_mode(value: &str) -> Result<(), DashboardError> {
    match value {
        "paper" | "live_readonly" | "live_trading" => Ok(()),
        _ => Err(DashboardError::BadRequest(format!(
            "unsupported export run_mode: {value}"
        ))),
    }
}

/// Validate the supported source states.
fn validate_source_state(value: &str) -> Result<(), DashboardError> {
    match value {
        "stopped" | "running_readonly" => Ok(()),
        _ => Err(DashboardError::BadRequest(format!(
            "unsupported export source_state: {value}"
        ))),
    }
}

/// Ensure a plan is allowed to perform filesystem export work.
fn ensure_real_export_allowed(plan: &ResearchExportPlan) -> Result<(), DashboardError> {
    if plan.dry_run {
        return Err(DashboardError::BadRequest(
            "dry-run export does not write runtime files".to_string(),
        ));
    }
    if !plan.confirm_export {
        return Err(DashboardError::BadRequest(
            "real export requires confirm_export=true".to_string(),
        ));
    }
    if plan.run_mode == "live_trading" {
        return Err(DashboardError::BadRequest(
            "live_trading exports must use buba-paint live-closeout".to_string(),
        ));
    }
    Ok(())
}

/// Copy one `SQLite` database through the `SQLite` backup API.
fn backup_sqlite_db(source: &Path, destination: &Path) -> Result<(), DashboardError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DashboardError::Internal(format!("creating DB backup dir: {e}")))?;
    }
    let source_conn = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(DashboardError::Database)?;
    source_conn
        .backup(DatabaseName::Main, destination, None)
        .map_err(DashboardError::Database)
}

/// Remove an existing `SQLite` destination family before writing a fresh backup.
fn remove_sqlite_family(path: &Path) -> Result<(), DashboardError> {
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path_to_string(path))),
        PathBuf::from(format!("{}-shm", path_to_string(path))),
    ] {
        if candidate.exists() {
            std::fs::remove_file(&candidate)
                .map_err(|e| DashboardError::Internal(format!("removing stale export DB: {e}")))?;
        }
    }
    Ok(())
}

/// Return export manifest file specs for already copied files.
fn export_file_specs(
    plan: &ResearchExportPlan,
) -> Result<Vec<ResearchExportedFile>, DashboardError> {
    let mut files = vec![ResearchExportedFile {
        logical_name: if plan.source_state == "running_readonly" {
            "runtime_db_snapshot".to_string()
        } else {
            "runtime_db".to_string()
        },
        kind: "sqlite".to_string(),
        relative_path: "runtime/paint.db".to_string(),
    }];
    for log_path in &plan.log_paths {
        let name = log_path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| {
                DashboardError::BadRequest(format!(
                    "log path has no file name: {}",
                    path_to_string(log_path)
                ))
            })?;
        files.push(ResearchExportedFile {
            logical_name: format!("log_{name}"),
            kind: "log".to_string(),
            relative_path: format!("logs/{name}"),
        });
    }
    files.push(ResearchExportedFile {
        logical_name: "export_summary".to_string(),
        kind: "json".to_string(),
        relative_path: "export-summary.json".to_string(),
    });
    Ok(files)
}

/// Write an export summary sidecar JSON file.
fn write_export_summary(
    plan: &ResearchExportPlan,
    manifest_file_count: usize,
) -> Result<(), DashboardError> {
    let summary = serde_json::json!({
        "schema_version": 1,
        "artifact_id": plan.artifact_id,
        "run_mode": plan.run_mode,
        "source_state": plan.source_state,
        "snapshot": plan.source_state == "running_readonly",
        "source_db_path": path_to_string(&plan.source_db_path),
        "source_wal_bytes": plan.source_wal_bytes,
        "source_shm_bytes": plan.source_shm_bytes,
        "interval_start_ms": plan.interval_start_ms,
        "interval_end_ms": plan.interval_end_ms,
        "manifest_file_count": manifest_file_count,
    });
    let text = serde_json::to_string_pretty(&summary)
        .map_err(|e| DashboardError::Internal(format!("serializing export summary: {e}")))?;
    std::fs::write(plan.artifact_root.join("export-summary.json"), text)
        .map_err(|e| DashboardError::Internal(format!("writing export summary: {e}")))
}

/// Hash the checksum sidecar text for DB metadata.
fn manifest_checksum(manifest: &ArtifactManifest) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(checksum_text(manifest).as_bytes());
    hex_digest(&hasher.finalize())
}

/// Render bytes as lowercase hexadecimal.
fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Resolve one path under a configured root.
fn resolve_under_root(root: &Path, path: &str) -> Result<PathBuf, DashboardError> {
    let root = normalize_path(root)?;
    let candidate = if Path::new(path).is_absolute() {
        normalize_path(Path::new(path))?
    } else {
        normalize_path(&root.join(path))?
    };
    if !candidate.starts_with(&root) {
        return Err(DashboardError::BadRequest(format!(
            "export path escapes configured root: {}",
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
                    "export paths must not contain parent traversal: {}",
                    path_to_string(path)
                )));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(DashboardError::BadRequest(
            "export path must not be empty".to_string(),
        ));
    }
    Ok(out)
}

/// Return a `SQLite` sidecar path.
fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}-{suffix}", path_to_string(path)))
}

/// Return the exact size of one required file.
fn file_size(path: &Path) -> Result<u64, DashboardError> {
    std::fs::metadata(path)
        .map(|meta| meta.len())
        .map_err(|e| DashboardError::Internal(format!("reading file metadata: {e}")))
}

/// Return the size of one optional file.
fn file_size_optional(path: &Path) -> Result<u64, DashboardError> {
    match std::fs::metadata(path) {
        Ok(meta) => Ok(meta.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(DashboardError::Internal(format!(
            "reading optional file metadata: {error}"
        ))),
    }
}

/// Return current wall-clock milliseconds.
fn now_ms() -> Result<u64, DashboardError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| DashboardError::Internal(format!("system clock before epoch: {e}")))?;
    Ok(duration.as_millis() as u64)
}

#[cfg(test)]
#[path = "tests/research_export_tests.rs"]
mod tests;
