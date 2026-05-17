//! Filesystem manifest helpers for research runtime artifacts.
//!
//! The dashboard stores high-level artifact metadata in `SQLite`, while the
//! artifact directory itself carries a portable manifest and checksum sidecar.
//! This module owns the path-safety, digest, manifest read/write, and manifest
//! verification rules for those directories.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::DashboardError;

/// Portable manifest describing one exported runtime artifact directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArtifactManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Stable artifact identifier used by dashboard jobs and reports.
    pub artifact_id: String,
    /// Artifact class, for example `readonly_run` or `readonly_run_snapshot`.
    pub kind: String,
    /// Machine that produced the artifact, when known.
    pub source_machine_id: Option<String>,
    /// Source run mode, such as `live_readonly`.
    pub run_mode: Option<String>,
    /// Wall-clock creation time in milliseconds since the Unix epoch.
    pub created_at_ms: u64,
    /// Optional inclusive replay interval start in milliseconds.
    pub interval_start_ms: Option<u64>,
    /// Optional replay interval end in milliseconds.
    pub interval_end_ms: Option<u64>,
    /// Files that belong to this artifact.
    pub files: Vec<ArtifactFile>,
}

/// One file entry inside an artifact manifest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArtifactFile {
    /// Stable logical name consumed by downstream pipeline planning.
    pub logical_name: String,
    /// File kind, such as `sqlite`, `log`, or `json`.
    pub kind: String,
    /// Artifact-root relative path normalized with `/` separators.
    pub relative_path: String,
    /// Byte length recorded when the manifest was built.
    pub bytes: u64,
    /// Lowercase `SHA-256` digest for integrity verification.
    pub sha256: String,
}

/// Input spec used while building a manifest from files already on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactFileSpec {
    /// Stable logical name for the manifest entry.
    pub logical_name: String,
    /// File kind for consumers and report rendering.
    pub kind: String,
    /// Artifact-root relative path to read and digest.
    pub relative_path: String,
}

/// Result of verifying all files in one artifact manifest.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ArtifactVerification {
    /// Artifact that was verified.
    pub artifact_id: String,
    /// Number of manifest files checked.
    pub files_checked: usize,
    /// Sum of verified payload bytes.
    pub bytes_checked: u64,
}

/// Build a manifest from existing files under one artifact root.
#[allow(clippy::too_many_arguments)]
pub fn build_manifest(
    artifact_root: &Path,
    artifact_id: &str,
    kind: &str,
    source_machine_id: Option<&str>,
    run_mode: Option<&str>,
    created_at_ms: u64,
    interval_start_ms: Option<u64>,
    interval_end_ms: Option<u64>,
    files: &[ArtifactFileSpec],
) -> Result<ArtifactManifest, DashboardError> {
    if artifact_id.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact_id must not be empty".to_string(),
        ));
    }
    if kind.trim().is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact kind must not be empty".to_string(),
        ));
    }

    let mut manifest_files = Vec::with_capacity(files.len());
    for spec in files {
        if spec.logical_name.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "artifact logical_name must not be empty".to_string(),
            ));
        }
        if spec.kind.trim().is_empty() {
            return Err(DashboardError::BadRequest(
                "artifact file kind must not be empty".to_string(),
            ));
        }
        let path = safe_join(artifact_root, &spec.relative_path)?;
        let (bytes, sha256) = file_digest(&path)?;
        manifest_files.push(ArtifactFile {
            logical_name: spec.logical_name.clone(),
            kind: spec.kind.clone(),
            relative_path: normalize_relative_path(&spec.relative_path)?,
            bytes,
            sha256,
        });
    }

    Ok(ArtifactManifest {
        schema_version: 1,
        artifact_id: artifact_id.to_string(),
        kind: kind.to_string(),
        source_machine_id: source_machine_id.map(str::to_string),
        run_mode: run_mode.map(str::to_string),
        created_at_ms,
        interval_start_ms,
        interval_end_ms,
        files: manifest_files,
    })
}

/// Write manifest and checksum sidecar files under one artifact root.
pub fn write_manifest_files(
    artifact_root: &Path,
    manifest: &ArtifactManifest,
) -> Result<(), DashboardError> {
    std::fs::create_dir_all(artifact_root)
        .map_err(|e| DashboardError::Internal(format!("creating artifact root: {e}")))?;
    let manifest_path = safe_join(artifact_root, "manifest.json")?;
    let checksum_path = safe_join(artifact_root, "checksums.sha256")?;
    let manifest_text = serde_json::to_string_pretty(manifest)
        .map_err(|e| DashboardError::Internal(format!("serializing manifest: {e}")))?;
    std::fs::write(&manifest_path, manifest_text)
        .map_err(|e| DashboardError::Internal(format!("writing manifest: {e}")))?;
    std::fs::write(&checksum_path, checksum_text(manifest))
        .map_err(|e| DashboardError::Internal(format!("writing checksums: {e}")))?;
    Ok(())
}

/// Read a manifest from one artifact root.
pub fn read_manifest(artifact_root: &Path) -> Result<ArtifactManifest, DashboardError> {
    let manifest_path = safe_join(artifact_root, "manifest.json")?;
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| DashboardError::Internal(format!("reading manifest: {e}")))?;
    serde_json::from_str(&text)
        .map_err(|e| DashboardError::BadRequest(format!("invalid artifact manifest: {e}")))
}

/// Verify every file listed in the artifact manifest.
pub fn verify_artifact(artifact_root: &Path) -> Result<ArtifactVerification, DashboardError> {
    let manifest = read_manifest(artifact_root)?;
    verify_manifest_files(artifact_root, &manifest)
}

/// Verify every file listed in a supplied manifest.
pub fn verify_manifest_files(
    artifact_root: &Path,
    manifest: &ArtifactManifest,
) -> Result<ArtifactVerification, DashboardError> {
    let mut bytes_checked = 0_u64;
    for file in &manifest.files {
        let path = safe_join(artifact_root, &file.relative_path)?;
        let (bytes, sha256) = file_digest(&path)?;
        if bytes != file.bytes {
            return Err(DashboardError::BadRequest(format!(
                "artifact file '{}' byte mismatch: expected {}, got {}",
                file.relative_path, file.bytes, bytes
            )));
        }
        if sha256 != file.sha256 {
            return Err(DashboardError::BadRequest(format!(
                "artifact file '{}' checksum mismatch",
                file.relative_path
            )));
        }
        bytes_checked = bytes_checked.saturating_add(bytes);
    }
    Ok(ArtifactVerification {
        artifact_id: manifest.artifact_id.clone(),
        files_checked: manifest.files.len(),
        bytes_checked,
    })
}

/// Join a configured root with a safe repository-local style relative path.
pub fn safe_join(root: &Path, relative_path: &str) -> Result<PathBuf, DashboardError> {
    Ok(root.join(normalize_relative_path(relative_path)?))
}

/// Normalize one relative artifact path and reject traversal.
pub fn normalize_relative_path(relative_path: &str) -> Result<String, DashboardError> {
    let path = Path::new(relative_path);
    if relative_path.trim().is_empty() || path.is_absolute() {
        return Err(DashboardError::BadRequest(
            "artifact paths must be non-empty relative paths".to_string(),
        ));
    }

    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(DashboardError::BadRequest(format!(
                    "unsafe artifact path: {relative_path}"
                )));
            }
        }
    }
    if parts.is_empty() {
        return Err(DashboardError::BadRequest(
            "artifact paths must contain a file name".to_string(),
        ));
    }
    Ok(parts.join("/"))
}

/// Return checksum file text for one manifest.
pub fn checksum_text(manifest: &ArtifactManifest) -> String {
    let mut lines = manifest
        .files
        .iter()
        .map(|file| format!("{}  {}", file.sha256, file.relative_path))
        .collect::<Vec<_>>();
    lines.sort();
    lines.join("\n") + "\n"
}

/// Return byte length and SHA-256 hex digest for a file.
fn file_digest(path: &Path) -> Result<(u64, String), DashboardError> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| DashboardError::Internal(format!("opening artifact file: {e}")))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| DashboardError::Internal(format!("reading artifact file: {e}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes.saturating_add(read as u64);
    }
    Ok((bytes, hex_digest(&hasher.finalize())))
}

/// Render digest bytes as lowercase hexadecimal without an extra dependency.
fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
#[path = "tests/research_artifacts_tests.rs"]
mod tests;
