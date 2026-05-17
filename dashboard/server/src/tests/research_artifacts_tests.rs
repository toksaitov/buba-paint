use super::*;

/// Verifies that unsafe relative paths are rejected.
#[test]
fn safe_join_rejects_absolute_and_parent_paths() {
    let root = std::path::Path::new("/tmp/artifact");

    assert!(safe_join(root, "").is_err());
    assert!(safe_join(root, ".").is_err());
    assert!(safe_join(root, "/etc/passwd").is_err());
    assert!(safe_join(root, "../paint.db").is_err());
    assert!(safe_join(root, "nested/../../paint.db").is_err());
    assert_eq!(
        safe_join(root, "./nested/paint.db").unwrap(),
        root.join("nested/paint.db")
    );
}

/// Verifies that relative paths are normalized without preserving dot segments.
#[test]
fn normalize_relative_path_removes_current_dir_segments() {
    assert_eq!(
        normalize_relative_path("./runtime/./paint.db").unwrap(),
        "runtime/paint.db"
    );
    assert!(normalize_relative_path("logs/../paint.db").is_err());
}

/// Verifies manifest building, writing, reading, and verification.
#[test]
fn manifest_round_trip_and_verify_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("logs")).unwrap();
    std::fs::write(root.join("paint.db"), b"db-bytes").unwrap();
    std::fs::write(root.join("logs/paint.log"), b"log-bytes").unwrap();

    let manifest = build_manifest(
        root,
        "artifact-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1_000,
        Some(900),
        Some(1_000),
        &[
            ArtifactFileSpec {
                logical_name: "runtime_db".to_string(),
                kind: "sqlite".to_string(),
                relative_path: "paint.db".to_string(),
            },
            ArtifactFileSpec {
                logical_name: "paint_log".to_string(),
                kind: "log".to_string(),
                relative_path: "logs/paint.log".to_string(),
            },
        ],
    )
    .unwrap();

    write_manifest_files(root, &manifest).unwrap();
    let read = read_manifest(root).unwrap();
    let verification = verify_artifact(root).unwrap();

    assert_eq!(read, manifest);
    assert_eq!(verification.artifact_id, "artifact-1");
    assert_eq!(verification.files_checked, 2);
    assert_eq!(verification.bytes_checked, 17);
    assert!(root.join("checksums.sha256").exists());
}

/// Verifies that changed artifact bytes fail verification.
#[test]
fn verify_artifact_rejects_checksum_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("paint.db"), b"original").unwrap();

    let manifest = build_manifest(
        root,
        "artifact-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1_000,
        None,
        None,
        &[ArtifactFileSpec {
            logical_name: "runtime_db".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "paint.db".to_string(),
        }],
    )
    .unwrap();
    write_manifest_files(root, &manifest).unwrap();
    std::fs::write(root.join("paint.db"), b"modified").unwrap();

    let error = verify_artifact(root).unwrap_err().to_string();

    assert!(error.contains("checksum mismatch") || error.contains("byte mismatch"));
}

/// Verifies that manifest verification reports byte mismatches directly.
#[test]
fn verify_manifest_reports_byte_mismatch_before_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("paint.db"), b"original").unwrap();
    let mut manifest = build_manifest(
        root,
        "artifact-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1_000,
        None,
        None,
        &[ArtifactFileSpec {
            logical_name: "runtime_db".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "paint.db".to_string(),
        }],
    )
    .unwrap();
    manifest.files[0].bytes += 1;

    let error = verify_manifest_files(root, &manifest)
        .unwrap_err()
        .to_string();

    assert!(error.contains("byte mismatch"));
}

/// Verifies checksum sidecar format is stable and sorted.
#[test]
fn checksum_text_is_sorted_by_line() {
    let manifest = ArtifactManifest {
        schema_version: 1,
        artifact_id: "artifact-1".to_string(),
        kind: "readonly_run".to_string(),
        source_machine_id: Some("live".to_string()),
        run_mode: Some("live_readonly".to_string()),
        created_at_ms: 1_000,
        interval_start_ms: None,
        interval_end_ms: None,
        files: vec![
            ArtifactFile {
                logical_name: "b".to_string(),
                kind: "log".to_string(),
                relative_path: "b.log".to_string(),
                bytes: 1,
                sha256: "bb".to_string(),
            },
            ArtifactFile {
                logical_name: "a".to_string(),
                kind: "db".to_string(),
                relative_path: "a.db".to_string(),
                bytes: 1,
                sha256: "aa".to_string(),
            },
        ],
    };

    assert_eq!(checksum_text(&manifest), "aa  a.db\nbb  b.log\n");
}

/// Verifies that manifest creation rejects invalid metadata and missing files.
#[test]
fn build_manifest_rejects_invalid_metadata_and_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("paint.db"), b"db-bytes").unwrap();
    let valid_file = ArtifactFileSpec {
        logical_name: "runtime_db".to_string(),
        kind: "sqlite".to_string(),
        relative_path: "paint.db".to_string(),
    };

    assert!(build_manifest(root, "", "readonly_run", None, None, 1, None, None, &[]).is_err());
    assert!(build_manifest(root, "artifact-1", "", None, None, 1, None, None, &[]).is_err());
    assert!(
        build_manifest(
            root,
            "artifact-1",
            "readonly_run",
            None,
            None,
            1,
            None,
            None,
            &[ArtifactFileSpec {
                logical_name: String::new(),
                ..valid_file.clone()
            }],
        )
        .is_err()
    );
    assert!(
        build_manifest(
            root,
            "artifact-1",
            "readonly_run",
            None,
            None,
            1,
            None,
            None,
            &[ArtifactFileSpec {
                kind: String::new(),
                ..valid_file.clone()
            }],
        )
        .is_err()
    );
    assert!(
        build_manifest(
            root,
            "artifact-1",
            "readonly_run",
            None,
            None,
            1,
            None,
            None,
            &[ArtifactFileSpec {
                relative_path: "missing.db".to_string(),
                ..valid_file
            }],
        )
        .is_err()
    );
}

/// Verifies that manifest verification rejects unsafe entries and bad JSON.
#[test]
fn verify_manifest_rejects_unsafe_entries_and_invalid_json() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("manifest.json"), "{bad-json").unwrap();
    assert!(read_manifest(root).is_err());

    let manifest = ArtifactManifest {
        schema_version: 1,
        artifact_id: "artifact-1".to_string(),
        kind: "readonly_run".to_string(),
        source_machine_id: None,
        run_mode: None,
        created_at_ms: 1,
        interval_start_ms: None,
        interval_end_ms: None,
        files: vec![ArtifactFile {
            logical_name: "escape".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "../escape.db".to_string(),
            bytes: 1,
            sha256: "00".to_string(),
        }],
    };

    assert!(verify_manifest_files(root, &manifest).is_err());
}
