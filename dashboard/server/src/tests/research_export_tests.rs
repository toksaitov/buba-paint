use rusqlite::Connection;

use super::*;
use crate::research_artifacts::read_manifest;

/// Create one valid `SQLite` source DB.
fn sqlite_source(dir: &Path) -> PathBuf {
    let path = dir.join("paint.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE ticks (id INTEGER PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO ticks (value) VALUES ('ok')", [])
        .unwrap();
    path
}

/// Create one pipeline config rooted in a temporary work directory.
fn pipeline(work_dir: &Path) -> ResearchPipelineConfig {
    ResearchPipelineConfig::new(std::env::current_dir().unwrap(), work_dir).unwrap()
}

/// Verifies that export planning defaults to dry-run and reports source facts.
#[test]
fn plan_export_defaults_to_dry_run() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let params = serde_json::json!({
        "source_db_path": source_db,
        "interval_start_ms": 1000,
        "interval_end_ms": 2000
    });

    let plan = plan_export(
        &pipeline(work_dir.path()),
        "job-1",
        Some(&params.to_string()),
    )
    .unwrap();

    assert!(plan.dry_run);
    assert_eq!(plan.safety_status, "safe");
    assert_eq!(plan.interval_start_ms, Some(1000));
    assert_eq!(plan.interval_end_ms, Some(2000));
    assert!(plan.estimated_bytes > 0);
}

/// Verifies alternate interval names and malformed params handling.
#[test]
fn plan_export_accepts_interval_aliases_and_rejects_bad_params() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let config = pipeline(work_dir.path());
    let params = serde_json::json!({
        "source_db_path": source_db,
        "start_ms": 1234,
        "end_ms": 5678
    });

    let plan = plan_export(&config, "job-1", Some(&params.to_string())).unwrap();

    assert_eq!(plan.interval_start_ms, Some(1234));
    assert_eq!(plan.interval_end_ms, Some(5678));
    assert!(plan_export(&config, "job-1", Some("{bad-json")).is_err());
    assert!(
        plan_export(
            &config,
            "job-1",
            Some(&serde_json::json!({"source_db_path": ""}).to_string())
        )
        .is_err()
    );
}

/// Verifies that funded live exports are blocked in favor of closeout.
#[test]
fn plan_export_blocks_live_trading() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let params = serde_json::json!({
        "source_db_path": source_db,
        "run_mode": "live_trading"
    });

    let plan = plan_export(
        &pipeline(work_dir.path()),
        "job-1",
        Some(&params.to_string()),
    )
    .unwrap();

    assert_eq!(plan.safety_status, "blocked");
    assert!(
        plan.safety_reasons
            .iter()
            .any(|reason| reason.contains("live-closeout"))
    );
}

/// Verifies that direct WAL export paths are rejected.
#[test]
fn plan_export_rejects_wal_as_source_or_log() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let wal_path = source_dir.path().join("paint.db-wal");
    std::fs::write(&wal_path, b"wal").unwrap();
    let source_params = serde_json::json!({
        "source_db_path": wal_path
    });
    let log_params = serde_json::json!({
        "source_db_path": source_db,
        "log_paths": [source_dir.path().join("paint.db-wal")]
    });

    assert!(
        plan_export(
            &pipeline(work_dir.path()),
            "job-1",
            Some(&source_params.to_string())
        )
        .is_err()
    );
    assert!(
        plan_export(
            &pipeline(work_dir.path()),
            "job-1",
            Some(&log_params.to_string())
        )
        .is_err()
    );
}

/// Verifies that direct SHM export paths are rejected.
#[test]
fn plan_export_rejects_shm_as_source_or_log() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let shm_path = source_dir.path().join("paint.db-shm");
    let alternate_shm_path = source_dir.path().join("paint-shm");
    std::fs::write(&shm_path, b"shm").unwrap();
    std::fs::write(&alternate_shm_path, b"shm").unwrap();
    let config = pipeline(work_dir.path());

    for params in [
        serde_json::json!({"source_db_path": shm_path}),
        serde_json::json!({
            "source_db_path": source_db,
            "log_paths": [alternate_shm_path]
        }),
    ] {
        assert!(
            plan_export(&config, "job-1", Some(&params.to_string())).is_err(),
            "{params}"
        );
    }
}

/// Verifies that a confirmed export writes a manifest-backed artifact.
#[test]
fn confirmed_export_writes_manifest_artifact() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let log_path = source_dir.path().join("runtime.log");
    std::fs::write(&log_path, b"log").unwrap();
    let params = serde_json::json!({
        "source_db_path": source_db,
        "log_paths": [log_path],
        "interval_start_ms": 1000,
        "interval_end_ms": 2000,
        "dry_run": false,
        "confirm_export": true
    });
    let plan = plan_export(
        &pipeline(work_dir.path()),
        "job-1",
        Some(&params.to_string()),
    )
    .unwrap();

    let export = export_runtime_files(&plan).unwrap();
    let manifest = write_export_manifest(&plan).unwrap();
    let read_back = read_manifest(&plan.artifact_root).unwrap();

    assert!(export.runtime_db_path.exists());
    assert_eq!(manifest.artifact_id, plan.artifact_id);
    assert_eq!(read_back.kind, "readonly_run");
    assert!(
        read_back
            .files
            .iter()
            .any(|file| file.relative_path == "runtime/paint.db")
    );
    assert!(
        read_back
            .files
            .iter()
            .any(|file| file.relative_path == "logs/runtime.log")
    );
    assert!(!plan.artifact_root.join("runtime/paint.db-wal").exists());
}

/// Verifies that running readonly exports are marked as snapshots.
#[test]
fn running_readonly_export_is_marked_as_snapshot() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let params = serde_json::json!({
        "source_db_path": source_db,
        "source_state": "running_readonly",
        "dry_run": false,
        "confirm_export": true
    });
    let plan = plan_export(
        &pipeline(work_dir.path()),
        "job-1",
        Some(&params.to_string()),
    )
    .unwrap();

    let export = export_runtime_files(&plan).unwrap();
    let manifest = write_export_manifest(&plan).unwrap();

    assert!(export.snapshot);
    assert_eq!(plan.safety_status, "snapshot_required");
    assert_eq!(manifest.artifact_id, plan.artifact_id);
    assert_eq!(
        read_manifest(&plan.artifact_root).unwrap().kind,
        "readonly_run_snapshot"
    );
}

/// Verifies that export planning validates paths, modes, IDs, and roots.
#[test]
fn plan_export_validates_required_paths_modes_ids_and_roots() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let config = pipeline(work_dir.path());

    assert!(plan_export(&config, "job-1", None).is_err());
    assert!(
        plan_export(
            &config,
            "job-1",
            Some(
                &serde_json::json!({
                    "source_db_path": source_db,
                    "artifact_id": "bad/id"
                })
                .to_string()
            )
        )
        .is_err()
    );
    assert!(
        plan_export(
            &config,
            "job-1",
            Some(
                &serde_json::json!({
                    "source_db_path": source_db,
                    "artifact_root": "../outside"
                })
                .to_string()
            )
        )
        .is_err()
    );
    assert!(
        plan_export(
            &config,
            "job-1",
            Some(
                &serde_json::json!({
                    "source_db_path": source_db,
                    "run_mode": "funded"
                })
                .to_string()
            )
        )
        .is_err()
    );
    assert!(
        plan_export(
            &config,
            "job-1",
            Some(
                &serde_json::json!({
                    "source_db_path": source_db,
                    "source_state": "copying"
                })
                .to_string()
            )
        )
        .is_err()
    );
}

/// Verifies that export planning rejects missing and duplicate logs.
#[test]
fn plan_export_rejects_missing_empty_and_duplicate_logs() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let first_dir = source_dir.path().join("first");
    let second_dir = source_dir.path().join("second");
    std::fs::create_dir_all(&first_dir).unwrap();
    std::fs::create_dir_all(&second_dir).unwrap();
    let first_log = first_dir.join("runtime.log");
    let second_log = second_dir.join("runtime.log");
    std::fs::write(&first_log, b"one").unwrap();
    std::fs::write(&second_log, b"two").unwrap();
    let config = pipeline(work_dir.path());

    assert!(
        plan_export(
            &config,
            "job-1",
            Some(
                &serde_json::json!({
                    "source_db_path": source_db,
                    "log_paths": [source_dir.path().join("missing.log")]
                })
                .to_string()
            )
        )
        .is_err()
    );
    assert!(
        plan_export(
            &config,
            "job-1",
            Some(
                &serde_json::json!({
                    "source_db_path": source_db,
                    "log_paths": [""]
                })
                .to_string()
            )
        )
        .is_err()
    );
    assert!(
        plan_export(
            &config,
            "job-1",
            Some(
                &serde_json::json!({
                    "source_db_path": source_db,
                    "log_paths": [first_log, second_log]
                })
                .to_string()
            )
        )
        .is_err()
    );
}

/// Verifies that real exports require confirmation and dry-runs do not write.
#[test]
fn real_export_requires_confirmation_and_dry_run_does_not_write() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let config = pipeline(work_dir.path());
    let blocked = plan_export(
        &config,
        "job-1",
        Some(
            &serde_json::json!({
                "source_db_path": source_db,
                "dry_run": false
            })
            .to_string(),
        ),
    )
    .unwrap();
    let dry_run = plan_export(
        &config,
        "job-2",
        Some(
            &serde_json::json!({
                "source_db_path": source_db
            })
            .to_string(),
        ),
    )
    .unwrap();

    assert_eq!(blocked.safety_status, "blocked");
    assert!(export_runtime_files(&blocked).is_err());
    assert!(write_export_manifest(&blocked).is_err());
    assert!(export_runtime_files(&dry_run).is_err());
    assert!(write_export_manifest(&dry_run).is_err());
}

/// Verifies that WAL sidecars require snapshots but are not counted as payload.
#[test]
fn wal_sidecar_marks_snapshot_required_without_counting_estimated_bytes() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    std::fs::write(format!("{}-wal", source_db.to_string_lossy()), b"wal-bytes").unwrap();
    let params = serde_json::json!({
        "source_db_path": source_db
    });

    let plan = plan_export(
        &pipeline(work_dir.path()),
        "job-1",
        Some(&params.to_string()),
    )
    .unwrap();

    assert_eq!(plan.safety_status, "snapshot_required");
    assert_eq!(plan.source_wal_bytes, 9);
    assert_eq!(plan.estimated_bytes, plan.source_db_bytes);
    assert!(
        plan.safety_reasons
            .iter()
            .any(|reason| reason.contains("WAL exists"))
    );
}

/// Verifies that SHM sidecars are reported but excluded from payload estimates.
#[test]
fn shm_sidecar_is_reported_without_counting_estimated_bytes() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    std::fs::write(format!("{}-shm", source_db.to_string_lossy()), b"shm-bytes").unwrap();
    let params = serde_json::json!({
        "source_db_path": source_db
    });

    let plan = plan_export(
        &pipeline(work_dir.path()),
        "job-1",
        Some(&params.to_string()),
    )
    .unwrap();

    assert_eq!(plan.source_shm_bytes, 9);
    assert_eq!(plan.estimated_bytes, plan.source_db_bytes);
}

/// Verifies that confirmed exports replace stale destination sidecars.
#[test]
fn confirmed_export_replaces_stale_destination_family() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let artifact_root = work_dir.path().join("artifacts/stale");
    let runtime_dir = artifact_root.join("runtime");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    std::fs::write(runtime_dir.join("paint.db"), b"stale").unwrap();
    std::fs::write(runtime_dir.join("paint.db-wal"), b"stale-wal").unwrap();
    std::fs::write(runtime_dir.join("paint.db-shm"), b"stale-shm").unwrap();
    let params = serde_json::json!({
        "source_db_path": source_db,
        "artifact_root": "artifacts/stale",
        "dry_run": false,
        "confirm_export": true
    });
    let plan = plan_export(
        &pipeline(work_dir.path()),
        "job-1",
        Some(&params.to_string()),
    )
    .unwrap();

    let result = export_runtime_files(&plan).unwrap();

    assert!(result.runtime_db_path.exists());
    assert!(!runtime_dir.join("paint.db-wal").exists());
    assert!(!runtime_dir.join("paint.db-shm").exists());
    assert!(result.bytes_written > 0);
}

/// Verifies that manifest writing requires copied runtime files first.
#[test]
fn confirmed_export_manifest_requires_runtime_copy_first() {
    let source_dir = tempfile::tempdir().unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let source_db = sqlite_source(source_dir.path());
    let params = serde_json::json!({
        "source_db_path": source_db,
        "dry_run": false,
        "confirm_export": true
    });
    let plan = plan_export(
        &pipeline(work_dir.path()),
        "job-1",
        Some(&params.to_string()),
    )
    .unwrap();

    let result = write_export_manifest(&plan);

    assert!(result.is_err());
}

/// Verifies that export payloads serialize to JSON for worker output.
#[test]
fn export_to_json_serializes_worker_payloads() {
    let text = to_json(&serde_json::json!({
        "step": "plan_export",
        "ok": true
    }))
    .unwrap();

    assert!(text.contains("\"step\":\"plan_export\""));
    assert!(text.contains("\"ok\":true"));
}
