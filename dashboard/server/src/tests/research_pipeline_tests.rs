use crate::db::{ResearchArtifact, ResearchJob};
use crate::research_artifacts::{ArtifactFileSpec, build_manifest, write_manifest_files};

use super::*;

/// Return a minimal research job for pipeline tests.
fn test_job(job_type: &str, params_json: Option<String>) -> ResearchJob {
    ResearchJob {
        id: "job-1".to_string(),
        job_type: job_type.to_string(),
        artifact_id: Some("artifact-1".to_string()),
        status: "queued".to_string(),
        priority: 0,
        requested_by: "user-1".to_string(),
        params_json,
        created_at: 1,
        updated_at: 1,
        cancelled_at: None,
        completed_at: None,
    }
}

/// Return a minimal artifact pointing at one manifest path.
fn test_artifact(manifest_path: &Path) -> ResearchArtifact {
    ResearchArtifact {
        id: "artifact-1".to_string(),
        source_machine_id: Some("live".to_string()),
        kind: "readonly_run".to_string(),
        status: "available".to_string(),
        run_mode: Some("live_readonly".to_string()),
        artifact_root: None,
        manifest_path: Some(manifest_path.to_string_lossy().to_string()),
        bundle_path: None,
        source_db_path: None,
        interval_start_ms: Some(1_000),
        interval_end_ms: Some(2_000),
        bytes: None,
        checksum: None,
        replay_quality_class: None,
        backtest_ready_class: None,
        live_fidelity_class: None,
        created_at: 1,
        updated_at: 1,
        archived_at: None,
    }
}

/// Build one artifact root containing a manifest and runtime DB fixture.
fn artifact_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("paint.db"), b"fixture-db").unwrap();
    let manifest = build_manifest(
        dir.path(),
        "artifact-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1,
        Some(1_000),
        Some(2_000),
        &[ArtifactFileSpec {
            logical_name: "runtime_db".to_string(),
            kind: "sqlite".to_string(),
            relative_path: "paint.db".to_string(),
        }],
    )
    .unwrap();
    write_manifest_files(dir.path(), &manifest).unwrap();
    dir
}

/// Return an artifact fixture without interval metadata.
fn artifact_without_interval(manifest_path: &Path) -> ResearchArtifact {
    let mut artifact = test_artifact(manifest_path);
    artifact.interval_start_ms = None;
    artifact.interval_end_ms = None;
    artifact
}

/// Return one research step fixture for report tests.
fn test_step(index: i64, name: &str, status: &str, error: Option<&str>) -> ResearchJobStep {
    ResearchJobStep {
        id: format!("step-{index}"),
        job_id: "job-1".to_string(),
        step_index: index,
        name: name.to_string(),
        status: status.to_string(),
        lease_owner: None,
        leased_until_ms: None,
        attempts: index + 1,
        input_json: None,
        output_json: None,
        error: error.map(str::to_string),
        created_at: 1,
        updated_at: 1,
        started_at: None,
        completed_at: None,
    }
}

/// Return one hand-built pipeline plan rooted under a job directory.
fn manual_plan(job_root: &Path) -> ResearchPipelinePlan {
    ResearchPipelinePlan {
        job_id: "job-1".to_string(),
        job_type: "current_params".to_string(),
        artifact_id: Some("artifact-1".to_string()),
        artifact_root: None,
        job_root: job_root.to_path_buf(),
        data_db_path: job_root.join("artifact/paint.db"),
        start: "1970-01-01T00:00:01Z".to_string(),
        end: "1970-01-01T00:00:02Z".to_string(),
        prepared_db_output_path: job_root.join("prepared-backtest.db"),
        backtest_output_path: job_root.join("backtest.db"),
        sweep_output_path: job_root.join("sweep.csv"),
        report_json_path: job_root.join("nested/report.json"),
        report_csv_path: job_root.join("nested/report.csv"),
        balance: 200.0,
        sets: Vec::new(),
        sweeps: Vec::new(),
        archive_scratch: true,
    }
}

/// Verifies that params, manifest data, and command arguments are resolved.
#[test]
fn pipeline_plan_builds_allowlisted_sweep_command() {
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();
    let config = ResearchPipelineConfig::new(&repo_root, work_dir.path()).unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z",
        "balance": 123.5,
        "sets": ["PEAK_DD_PAUSE_PCT=1.0"],
        "sweeps": ["LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.002:0.001"]
    });
    let job = test_job("sweep", Some(params.to_string()));
    let artifact = test_artifact(&artifact_dir.path().join("manifest.json"));

    let plan = config.plan_for_job(&job, Some(&artifact)).unwrap();
    let command = config.command_for_step(BubaPaintCommandKind::RunSweep, &plan);

    assert_eq!(plan.data_db_path, artifact_dir.path().join("paint.db"));
    assert!(plan.prepared_db_output_path.starts_with(work_dir.path()));
    assert!(command.args.contains(&"sweep".to_string()));
    assert!(command.args.contains(&"--sweep".to_string()));
    assert!(
        command
            .args
            .contains(&"LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.002:0.001".to_string())
    );
    assert!(command.args.contains(&"PEAK_DD_PAUSE_PCT=1.0".to_string()));
}

/// Verifies custom launcher prefixes are preserved in generated commands.
#[test]
fn pipeline_command_uses_custom_buba_paint_launcher() {
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();
    let config = ResearchPipelineConfig::new(&repo_root, work_dir.path())
        .unwrap()
        .with_buba_paint_command(BubaPaintCommand {
            program: PathBuf::from("/opt/bin/buba-paint"),
            fixed_args: vec!["--profile".to_string(), "research".to_string()],
        });
    let job = test_job("current_params", None);
    let artifact = test_artifact(&artifact_dir.path().join("manifest.json"));

    let plan = config.plan_for_job(&job, Some(&artifact)).unwrap();
    let command = config.command_for_step(BubaPaintCommandKind::ValidateReplayData, &plan);

    assert_eq!(command.program, "/opt/bin/buba-paint");
    assert_eq!(command.args[0], "--profile");
    assert_eq!(command.args[1], "research");
    assert!(command.args.contains(&"validate-replay-data".to_string()));
}

/// Verifies pipeline config rejects empty configured roots.
#[test]
fn pipeline_config_rejects_empty_roots() {
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();

    assert!(ResearchPipelineConfig::new("", work_dir.path()).is_err());
    assert!(ResearchPipelineConfig::new(repo_root, "").is_err());
}

/// Verifies that output paths cannot leave the research work root.
#[test]
fn pipeline_plan_rejects_output_traversal() {
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();
    let config = ResearchPipelineConfig::new(&repo_root, work_dir.path()).unwrap();
    let params = serde_json::json!({
        "start": "1970-01-01T00:00:01Z",
        "end": "1970-01-01T00:00:02Z",
        "prepared_db_output_path": "../manifest.json"
    });
    let job = test_job("current_params", Some(params.to_string()));
    let artifact = test_artifact(&artifact_dir.path().join("manifest.json"));

    let result = config.plan_for_job(&job, Some(&artifact));

    assert!(result.is_err());
}

/// Verifies that output paths must name child files under the job root.
#[test]
fn pipeline_plan_rejects_job_root_as_output_path() {
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();
    let config = ResearchPipelineConfig::new(&repo_root, work_dir.path()).unwrap();
    let artifact = test_artifact(&artifact_dir.path().join("manifest.json"));

    for params in [
        serde_json::json!({
            "start": "1970-01-01T00:00:01Z",
            "end": "1970-01-01T00:00:02Z",
            "report_json_path": "."
        }),
        serde_json::json!({
            "start": "1970-01-01T00:00:01Z",
            "end": "1970-01-01T00:00:02Z",
            "report_csv_path": ""
        }),
    ] {
        assert!(
            config
                .plan_for_job(
                    &test_job("current_params", Some(params.to_string())),
                    Some(&artifact)
                )
                .is_err(),
            "{params}"
        );
    }
}

/// Verifies that artifact intervals are used when job params omit times.
#[test]
fn pipeline_plan_uses_artifact_interval_when_params_omit_times() {
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();
    let config = ResearchPipelineConfig::new(&repo_root, work_dir.path()).unwrap();
    let job = test_job("current_params", None);
    let artifact = test_artifact(&artifact_dir.path().join("manifest.json"));

    let plan = config.plan_for_job(&job, Some(&artifact)).unwrap();
    let command = config.command_for_step(BubaPaintCommandKind::RunBacktest, &plan);

    assert_eq!(plan.start, "1970-01-01T00:00:01.000Z");
    assert_eq!(plan.end, "1970-01-01T00:00:02.000Z");
    assert!(command.args.contains(&"backtest".to_string()));
    assert!(command.args.contains(&"--balance".to_string()));
}

/// Verifies planning can use artifact roots when manifest paths are absent.
#[test]
fn pipeline_plan_uses_artifact_root_when_manifest_path_is_absent() {
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();
    let config = ResearchPipelineConfig::new(&repo_root, work_dir.path()).unwrap();
    let mut artifact = test_artifact(&artifact_dir.path().join("manifest.json"));
    artifact.artifact_root = Some(artifact_dir.path().to_string_lossy().to_string());
    artifact.manifest_path = None;
    let job = test_job("current_params", None);

    let plan = config.plan_for_job(&job, Some(&artifact)).unwrap();

    assert_eq!(plan.artifact_root.as_deref(), Some(artifact_dir.path()));
    assert_eq!(plan.data_db_path, artifact_dir.path().join("paint.db"));
}

/// Verifies that pipeline planning rejects invalid params and missing fields.
#[test]
fn pipeline_plan_rejects_invalid_params_and_required_fields() {
    let artifact_dir = artifact_fixture();
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();
    let config = ResearchPipelineConfig::new(&repo_root, work_dir.path()).unwrap();
    let artifact = test_artifact(&artifact_dir.path().join("manifest.json"));
    let no_interval = artifact_without_interval(&artifact_dir.path().join("manifest.json"));

    assert!(
        config
            .plan_for_job(
                &test_job("current_params", Some("{bad-json".to_string())),
                Some(&artifact)
            )
            .is_err()
    );
    assert!(
        config
            .plan_for_job(&test_job("current_params", None), Some(&no_interval))
            .is_err()
    );
    assert!(
        config
            .plan_for_job(
                &test_job(
                    "current_params",
                    Some(
                        serde_json::json!({"start": "", "end": "1970-01-01T00:00:02Z"}).to_string()
                    )
                ),
                Some(&artifact)
            )
            .is_err()
    );
    assert!(
        config
            .plan_for_job(
                &test_job(
                    "current_params",
                    Some(serde_json::json!({"start": "1970-01-01T00:00:01Z", "end": "1970-01-01T00:00:02Z", "balance": 0.0}).to_string())
                ),
                Some(&artifact)
            )
            .is_err()
    );
    assert!(
        config
            .plan_for_job(
                &test_job(
                    "current_params",
                    Some(serde_json::json!({"start": "1970-01-01T00:00:01Z", "end": "1970-01-01T00:00:02Z", "sets": [""]}).to_string())
                ),
                Some(&artifact)
            )
            .is_err()
    );
    assert!(
        config
            .plan_for_job(
                &test_job(
                    "sweep",
                    Some(serde_json::json!({"start": "1970-01-01T00:00:01Z", "end": "1970-01-01T00:00:02Z"}).to_string())
                ),
                Some(&artifact)
            )
            .is_err()
    );
}

/// Verifies that pipeline planning rejects unsafe or missing data DB paths.
#[test]
fn pipeline_plan_rejects_bad_data_paths_and_missing_runtime_db() {
    let artifact_dir = artifact_fixture();
    let no_db_dir = tempfile::tempdir().unwrap();
    std::fs::write(no_db_dir.path().join("notes.txt"), b"notes").unwrap();
    let no_db_manifest = build_manifest(
        no_db_dir.path(),
        "artifact-1",
        "readonly_run",
        Some("live"),
        Some("live_readonly"),
        1,
        Some(1_000),
        Some(2_000),
        &[ArtifactFileSpec {
            logical_name: "notes".to_string(),
            kind: "text".to_string(),
            relative_path: "notes.txt".to_string(),
        }],
    )
    .unwrap();
    write_manifest_files(no_db_dir.path(), &no_db_manifest).unwrap();
    let work_dir = tempfile::tempdir().unwrap();
    let repo_root = std::env::current_dir().unwrap();
    let config = ResearchPipelineConfig::new(&repo_root, work_dir.path()).unwrap();
    let artifact = test_artifact(&artifact_dir.path().join("manifest.json"));
    let no_db_artifact = test_artifact(&no_db_dir.path().join("manifest.json"));

    assert!(
        config
            .plan_for_job(
                &test_job(
                    "current_params",
                    Some(serde_json::json!({"data_db_path": "../paint.db"}).to_string())
                ),
                Some(&artifact)
            )
            .is_err()
    );
    assert!(
        config
            .plan_for_job(
                &test_job(
                    "current_params",
                    Some(serde_json::json!({"data_db_path": "missing.db"}).to_string())
                ),
                Some(&artifact)
            )
            .is_err()
    );
    assert!(
        config
            .plan_for_job(&test_job("current_params", None), Some(&no_db_artifact))
            .is_err()
    );
    assert!(
        config
            .plan_for_job(&test_job("current_params", None), None)
            .is_err()
    );
}

/// Verifies that scratch DB archive deletes only DB families under the job root.
#[test]
fn archive_scratch_dbs_deletes_db_family_under_job_root() {
    let work_dir = tempfile::tempdir().unwrap();
    let job_root = work_dir.path().join("jobs/job-1");
    std::fs::create_dir_all(&job_root).unwrap();
    let prepared = job_root.join("prepared-backtest.db");
    let backtest = job_root.join("backtest.db");
    std::fs::write(&prepared, b"prepared").unwrap();
    std::fs::write(format!("{}-wal", prepared.to_string_lossy()), b"wal").unwrap();
    std::fs::write(&backtest, b"backtest").unwrap();
    let plan = ResearchPipelinePlan {
        job_id: "job-1".to_string(),
        job_type: "current_params".to_string(),
        artifact_id: Some("artifact-1".to_string()),
        artifact_root: None,
        job_root: job_root.clone(),
        data_db_path: work_dir.path().join("artifact/paint.db"),
        start: "1970-01-01T00:00:01Z".to_string(),
        end: "1970-01-01T00:00:02Z".to_string(),
        prepared_db_output_path: prepared.clone(),
        backtest_output_path: backtest.clone(),
        sweep_output_path: job_root.join("sweep.csv"),
        report_json_path: job_root.join("report.json"),
        report_csv_path: job_root.join("report.csv"),
        balance: 200.0,
        sets: Vec::new(),
        sweeps: Vec::new(),
        archive_scratch: true,
    };

    let summary = archive_scratch_dbs(&plan).unwrap();

    assert!(!prepared.exists());
    assert!(!PathBuf::from(format!("{}-wal", prepared.to_string_lossy())).exists());
    assert!(!backtest.exists());
    assert!(summary.deleted_paths.len() >= 3);
}

/// Verifies that archive refuses DB deletion outside the job root.
#[test]
fn archive_scratch_dbs_refuses_paths_outside_job_root() {
    let work_dir = tempfile::tempdir().unwrap();
    let job_root = work_dir.path().join("jobs/job-1");
    let outside = work_dir.path().join("outside.db");
    std::fs::create_dir_all(&job_root).unwrap();
    std::fs::write(&outside, b"outside").unwrap();
    let plan = ResearchPipelinePlan {
        job_id: "job-1".to_string(),
        job_type: "current_params".to_string(),
        artifact_id: Some("artifact-1".to_string()),
        artifact_root: None,
        job_root,
        data_db_path: work_dir.path().join("artifact/paint.db"),
        start: "1970-01-01T00:00:01Z".to_string(),
        end: "1970-01-01T00:00:02Z".to_string(),
        prepared_db_output_path: outside,
        backtest_output_path: work_dir.path().join("jobs/job-1/backtest.db"),
        sweep_output_path: work_dir.path().join("jobs/job-1/sweep.csv"),
        report_json_path: work_dir.path().join("jobs/job-1/report.json"),
        report_csv_path: work_dir.path().join("jobs/job-1/report.csv"),
        balance: 200.0,
        sets: Vec::new(),
        sweeps: Vec::new(),
        archive_scratch: true,
    };

    let result = archive_scratch_dbs(&plan);

    assert!(result.is_err());
}

/// Verifies that scratch archiving rejects non-database outputs.
#[test]
fn archive_scratch_dbs_rejects_non_db_outputs() {
    let work_dir = tempfile::tempdir().unwrap();
    let job_root = work_dir.path().join("jobs/job-1");
    std::fs::create_dir_all(&job_root).unwrap();
    let mut plan = manual_plan(&job_root);
    plan.prepared_db_output_path = plan.job_root.join("prepared.txt");

    let result = archive_scratch_dbs(&plan);

    assert!(result.is_err());
}

/// Verifies that report writing escapes CSV cells and creates output files.
#[test]
fn write_report_files_escapes_csv_cells_and_creates_outputs() {
    let work_dir = tempfile::tempdir().unwrap();
    let job_root = work_dir.path().join("jobs/job-1");
    let plan = manual_plan(&job_root);
    let steps = vec![
        test_step(0, "verify_artifact", "completed", None),
        test_step(
            1,
            "run_backtest",
            "blocked",
            Some("bad \"quote\", line\nnext"),
        ),
    ];

    let summary = write_report_files(&plan, &steps).unwrap();
    let csv = std::fs::read_to_string(&plan.report_csv_path).unwrap();

    assert!(plan.report_json_path.exists());
    assert!(summary.contains("\"job_id\": \"job-1\""));
    assert!(csv.contains("\"bad \"\"quote\"\", line\nnext\""));
}

/// Verifies that the process executor captures status, stdout, and stderr.
#[test]
fn process_command_executor_captures_status_stdout_and_stderr() {
    let executor = ProcessCommandExecutor;
    let command = CommandSpec {
        program: "/bin/sh".to_string(),
        args: vec![
            "-c".to_string(),
            "printf ok; printf err >&2; exit 7".to_string(),
        ],
        cwd: std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string(),
    };

    let output = executor.execute(&command).unwrap();

    assert!(!output.success);
    assert_eq!(output.status_code, Some(7));
    assert_eq!(output.stdout, "ok");
    assert_eq!(output.stderr, "err");
}
