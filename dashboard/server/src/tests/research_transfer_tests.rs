use rusqlite::Connection;

use super::*;
use crate::db::{ArtifactTransferRecord, DashboardDb, ResearchArtifactRecord};
use crate::research_artifacts::{ArtifactFileSpec, build_manifest, write_manifest_files};

/// Build an in-memory dashboard DB.
fn test_db() -> DashboardDb {
    DashboardDb::from_connection(Connection::open_in_memory().unwrap())
}

/// Create a manifest-backed artifact directory.
fn write_artifact(root: &Path, artifact_id: &str, payload: &[u8]) -> ArtifactManifest {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("paint.db"), payload).unwrap();
    let manifest = build_manifest(
        root,
        artifact_id,
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1_000,
        Some(1_000),
        Some(2_000),
        &[ArtifactFileSpec {
            logical_name: "runtime_db".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "paint.db".to_string(),
        }],
    )
    .unwrap();
    write_manifest_files(root, &manifest).unwrap();
    manifest
}

/// Insert artifact metadata for an artifact root.
async fn upsert_artifact(
    db: &DashboardDb,
    artifact_id: &str,
    source_machine_id: &str,
    root: &Path,
    manifest: &ArtifactManifest,
) {
    let artifact_root = path_to_string(root);
    let manifest_path = path_to_string(&root.join("manifest.json"));
    db.upsert_research_artifact(&ResearchArtifactRecord {
        id: artifact_id,
        source_machine_id: Some(source_machine_id),
        kind: &manifest.kind,
        status: "available",
        run_mode: manifest.run_mode.as_deref(),
        artifact_root: Some(&artifact_root),
        manifest_path: Some(&manifest_path),
        bundle_path: None,
        source_db_path: Some(&path_to_string(&root.join("paint.db"))),
        interval_start_ms: manifest.interval_start_ms,
        interval_end_ms: manifest.interval_end_ms,
        bytes: Some(manifest_payload_bytes(manifest)),
        checksum: manifest.files.first().map(|file| file.sha256.as_str()),
        replay_quality_class: None,
        backtest_ready_class: None,
        live_fidelity_class: None,
    })
    .await
    .unwrap();
}

/// Verifies that a local transfer copies an artifact and updates metadata.
#[tokio::test]
async fn transfer_worker_copies_local_artifact_and_updates_metadata() {
    let db = test_db();
    let source = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let artifact_id = "artifact-transfer-local";
    let manifest = write_artifact(source.path(), artifact_id, b"local-transfer-bytes");
    upsert_artifact(&db, artifact_id, "research", source.path(), &manifest).await;
    let transfer = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id,
            source_machine_id: Some("research"),
            dest_machine_id: Some("research"),
            bytes_total: Some(manifest_payload_bytes(&manifest)),
        })
        .await
        .unwrap();
    let config = ArtifactTransferConfig::new(work.path(), "research").unwrap();
    let destination = config.destination_root(artifact_id).unwrap();
    let worker = ArtifactTransferWorker::new(config);

    let completed = worker.run_one(&db).await.unwrap().unwrap();
    let artifact = db
        .get_research_artifact(artifact_id)
        .await
        .unwrap()
        .unwrap();
    let destination_text = path_to_string(&destination);

    assert_eq!(completed.id, transfer.id);
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.checksum_status.as_deref(), Some("verified"));
    assert_eq!(
        std::fs::read(destination.join("paint.db")).unwrap(),
        b"local-transfer-bytes"
    );
    assert_eq!(
        artifact.artifact_root.as_deref(),
        Some(destination_text.as_str())
    );
    assert_eq!(artifact.source_machine_id.as_deref(), Some("research"));
    assert_eq!(artifact.bytes, Some(manifest_payload_bytes(&manifest)));
}

/// Verifies that a local transfer resumes a partial destination file.
#[tokio::test]
async fn transfer_worker_resumes_partial_local_copy() {
    let db = test_db();
    let source = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let artifact_id = "artifact-transfer-resume";
    let manifest = write_artifact(source.path(), artifact_id, b"abcdefghij");
    upsert_artifact(&db, artifact_id, "research", source.path(), &manifest).await;
    let config = ArtifactTransferConfig::new(work.path(), "research").unwrap();
    let destination = config.destination_root(artifact_id).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::copy(
        source.path().join("manifest.json"),
        destination.join("manifest.json"),
    )
    .unwrap();
    std::fs::copy(
        source.path().join("checksums.sha256"),
        destination.join("checksums.sha256"),
    )
    .unwrap();
    std::fs::write(destination.join("paint.db"), b"abcd").unwrap();
    db.create_artifact_transfer(&ArtifactTransferRecord {
        artifact_id,
        source_machine_id: Some("research"),
        dest_machine_id: Some("research"),
        bytes_total: Some(manifest_payload_bytes(&manifest)),
    })
    .await
    .unwrap();
    let worker = ArtifactTransferWorker::new(config);

    let completed = worker.run_one(&db).await.unwrap().unwrap();

    assert_eq!(completed.status, "completed");
    assert_eq!(completed.bytes_done, 10);
    assert_eq!(
        std::fs::read(destination.join("paint.db")).unwrap(),
        b"abcdefghij"
    );
}

/// Verifies the transfer worker recovers a stale running transfer and resumes it.
#[tokio::test]
async fn transfer_worker_recovers_stale_running_local_transfer() {
    let db = test_db();
    let source = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let artifact_id = "artifact-transfer-stale-recovery";
    let manifest = write_artifact(source.path(), artifact_id, b"stale-transfer");
    upsert_artifact(&db, artifact_id, "research", source.path(), &manifest).await;
    let created = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id,
            source_machine_id: Some("research"),
            dest_machine_id: Some("research"),
            bytes_total: Some(manifest_payload_bytes(&manifest)),
        })
        .await
        .unwrap();
    let claimed = db
        .claim_next_artifact_transfer("research")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, created.id);
    assert_eq!(claimed.status, "running");
    let config = ArtifactTransferConfig::new(work.path(), "research")
        .unwrap()
        .with_stale_after_ms(Some(0));
    let destination = config.destination_root(artifact_id).unwrap();
    let worker = ArtifactTransferWorker::new(config);

    let processed = worker.run_until_idle(&db, 1).await.unwrap();
    let completed = db
        .get_artifact_transfer(&created.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(processed, 1);
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.checksum_status.as_deref(), Some("verified"));
    assert_eq!(
        std::fs::read(destination.join("paint.db")).unwrap(),
        b"stale-transfer"
    );
}

/// Verifies that failed transfer execution becomes retryable.
#[tokio::test]
async fn transfer_worker_marks_failed_transfer_retryable() {
    let db = test_db();
    let source = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let artifact_id = "artifact-transfer-failure";
    let artifact_root = path_to_string(source.path());
    let manifest_path = path_to_string(&source.path().join("manifest.json"));
    db.upsert_research_artifact(&ResearchArtifactRecord {
        id: artifact_id,
        source_machine_id: Some("research"),
        kind: "readonly_run",
        status: "available",
        run_mode: Some("live_readonly"),
        artifact_root: Some(&artifact_root),
        manifest_path: Some(&manifest_path),
        bundle_path: None,
        source_db_path: None,
        interval_start_ms: None,
        interval_end_ms: None,
        bytes: Some(10),
        checksum: None,
        replay_quality_class: None,
        backtest_ready_class: None,
        live_fidelity_class: None,
    })
    .await
    .unwrap();
    db.create_artifact_transfer(&ArtifactTransferRecord {
        artifact_id,
        source_machine_id: Some("research"),
        dest_machine_id: Some("research"),
        bytes_total: Some(10),
    })
    .await
    .unwrap();
    let worker =
        ArtifactTransferWorker::new(ArtifactTransferConfig::new(work.path(), "research").unwrap());

    let transfer = worker.run_one(&db).await.unwrap().unwrap();

    assert_eq!(transfer.status, "retryable");
    assert_eq!(transfer.checksum_status.as_deref(), Some("failed"));
    assert!(
        transfer
            .error
            .as_deref()
            .is_some_and(|error| !error.is_empty())
    );
}

/// Verifies that remote transfer command construction is resumable and compressed.
#[test]
fn rsync_command_uses_resumable_compressed_flags() {
    let spec = rsync_command_spec(
        Path::new("rsync"),
        Some("ssh -F /home/buba/.ssh/config"),
        "buba-paint",
        Path::new("/tmp/source artifact"),
        Path::new("/research/artifacts/artifact-transfer-remote"),
    )
    .unwrap();

    assert_eq!(spec.program, "rsync");
    assert!(spec.args.contains(&"--partial".to_string()));
    assert!(spec.args.contains(&"--append-verify".to_string()));
    assert!(spec.args.contains(&"--compress".to_string()));
    assert!(spec.args.contains(&"--protect-args".to_string()));
    assert!(
        spec.args
            .contains(&"ssh -F /home/buba/.ssh/config".to_string())
    );
    assert_eq!(
        spec.args[spec.args.len() - 2],
        "buba-paint:/tmp/source artifact/"
    );
    assert_eq!(
        spec.args[spec.args.len() - 1],
        "/research/artifacts/artifact-transfer-remote/"
    );
}
