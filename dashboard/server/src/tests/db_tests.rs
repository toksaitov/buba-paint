use buba_machine_telemetry::{HostIdentity, MachineSample, MachineSamplerHealth};
use rusqlite::Connection;

use super::*;

/// Test db.
fn test_db() -> DashboardDb {
    DashboardDb::from_connection(Connection::open_in_memory().unwrap())
}

/// Verifies that a freshly opened dashboard database has a non-zero SQLite busy timeout.
#[tokio::test]
async fn new_sets_busy_timeout_on_connection() {
    let db = DashboardDb::new(":memory:").unwrap();
    let conn = db.conn.lock().await;
    let busy_timeout_ms: i64 = conn
        .pragma_query_value(None, "busy_timeout", |row| row.get(0))
        .unwrap();
    assert_eq!(busy_timeout_ms, 5_000);
}

/// Build a deterministic host identity for telemetry tests.
fn telemetry_host() -> HostIdentity {
    HostIdentity {
        hostname: "testing".to_string(),
        os_name: "Linux".to_string(),
        os_version: "test".to_string(),
        kernel_version: "test".to_string(),
        cpu_count: 2,
        total_ram_bytes: 16_384,
    }
}

/// Build deterministic sampler health for telemetry tests.
fn telemetry_sampler(samples_collected: u64, last_error: Option<&str>) -> MachineSamplerHealth {
    MachineSamplerHealth {
        sample_interval_ms: 5_000,
        samples_collected,
        last_error: last_error.map(str::to_string),
    }
}

/// Build a deterministic host sample for telemetry tests.
fn telemetry_sample_at(sampled_at_ms: i64) -> MachineSample {
    MachineSample {
        sampled_at_ms,
        cpu_percent: 12.5,
        per_core_cpu: vec![12.5, 8.0],
        load_one: Some(0.5),
        load_five: Some(0.4),
        load_fifteen: Some(0.3),
        mem_used_bytes: 4_096,
        mem_total_bytes: 16_384,
        mem_available_bytes: 12_288,
        swap_used_bytes: 0,
        swap_total_bytes: 0,
        disk_used_bytes: 50_000,
        disk_total_bytes: 100_000,
        disk_mount: "/research".to_string(),
    }
}

/// Verifies that seed admin creates user when empty.
#[tokio::test]
async fn seed_admin_creates_user_when_empty() {
    let db = test_db();
    db.seed_admin("admin", "$argon2id$hash").await.unwrap();

    let user = db.get_user_by_username("admin").await.unwrap().unwrap();
    assert_eq!(user.username, "admin");
    assert_eq!(user.role, "admin");
}

/// Verifies that seed admin skips when users exist.
#[tokio::test]
async fn seed_admin_skips_when_users_exist() {
    let db = test_db();
    db.create_user("existing", "hash", "observer")
        .await
        .unwrap();

    db.seed_admin("admin", "hash").await.unwrap();

    let admin = db.get_user_by_username("admin").await.unwrap();
    assert!(admin.is_none());
}

/// Verifies that create user and retrieve by username.
#[tokio::test]
async fn create_user_and_retrieve_by_username() {
    let db = test_db();
    let user = db
        .create_user("alice", "hash123", "observer")
        .await
        .unwrap();

    assert_eq!(user.username, "alice");
    assert_eq!(user.role, "observer");
    assert!(!user.id.is_empty());

    let found = db.get_user_by_username("alice").await.unwrap().unwrap();
    assert_eq!(found.id, user.id);
}

/// Verifies that create user and retrieve by id.
#[tokio::test]
async fn create_user_and_retrieve_by_id() {
    let db = test_db();
    let user = db.create_user("bob", "hash456", "admin").await.unwrap();

    let found = db.get_user_by_id(&user.id).await.unwrap().unwrap();
    assert_eq!(found.username, "bob");
    assert_eq!(found.role, "admin");
}

/// Verifies that get user by username not found.
#[tokio::test]
async fn get_user_by_username_not_found() {
    let db = test_db();
    let found = db.get_user_by_username("nonexistent").await.unwrap();
    assert!(found.is_none());
}

/// Verifies that get user by id not found.
#[tokio::test]
async fn get_user_by_id_not_found() {
    let db = test_db();
    let found = db.get_user_by_id("nonexistent-id").await.unwrap();
    assert!(found.is_none());
}

/// Verifies that list users empty.
#[tokio::test]
async fn list_users_empty() {
    let db = test_db();
    let users = db.list_users().await.unwrap();
    assert!(users.is_empty());
}

/// Verifies that list users returns all.
#[tokio::test]
async fn list_users_returns_all() {
    let db = test_db();
    db.create_user("alice", "h1", "admin").await.unwrap();
    db.create_user("bob", "h2", "observer").await.unwrap();

    let users = db.list_users().await.unwrap();
    assert_eq!(users.len(), 2);
}

/// Verifies that create session and retrieve by token.
#[tokio::test]
async fn create_session_and_retrieve_by_token() {
    let db = test_db();
    let user = db.create_user("carol", "h3", "observer").await.unwrap();

    let session = db
        .create_session(&user.id, "jwt-token-123", 9_999_999_999_999)
        .await
        .unwrap();

    assert_eq!(session.user_id, user.id);
    assert_eq!(session.token, "jwt-token-123");

    let found = db
        .get_session_by_token("jwt-token-123")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.user_id, user.id);
}

/// Verifies that get session by token not found.
#[tokio::test]
async fn get_session_by_token_not_found() {
    let db = test_db();
    let found = db.get_session_by_token("nonexistent").await.unwrap();
    assert!(found.is_none());
}

/// Verifies that delete session.
#[tokio::test]
async fn delete_session() {
    let db = test_db();
    let user = db.create_user("dan", "h4", "observer").await.unwrap();
    db.create_session(&user.id, "token-del", 9_999_999_999_999)
        .await
        .unwrap();

    db.delete_session("token-del").await.unwrap();

    let found = db.get_session_by_token("token-del").await.unwrap();
    assert!(found.is_none());
}

/// Verifies that duplicate username fails.
#[tokio::test]
async fn duplicate_username_fails() {
    let db = test_db();
    db.create_user("dup", "h1", "observer").await.unwrap();

    let result = db.create_user("dup", "h2", "observer").await;
    assert!(result.is_err());
}

/// Verifies that user password hash not serialized.
#[tokio::test]
async fn user_password_hash_not_serialized() {
    let db = test_db();
    let user = db
        .create_user("eve", "secret-hash", "observer")
        .await
        .unwrap();

    let json = serde_json::to_string(&user).unwrap();
    assert!(!json.contains("secret-hash"));
}

/// Verifies that default research machine records are seeded.
#[tokio::test]
async fn research_machines_seed_default_live_and_research_hosts() {
    let db = test_db();
    let machines = db.list_research_machines().await.unwrap();

    assert_eq!(machines.len(), 2);
    assert_eq!(machines[0].id, "live");
    assert_eq!(machines[0].ssh_alias.as_deref(), Some("buba-paint"));
    assert_eq!(machines[1].id, "research");
    assert_eq!(machines[1].status, "not_configured");
}

/// Verifies that research worker heartbeats update machine state.
#[tokio::test]
async fn research_machine_heartbeat_updates_status_and_details() {
    let db = test_db();

    let machine = db
        .record_research_machine_heartbeat_at(
            "research",
            "research-worker-testing",
            Some("0.1.0"),
            "idle",
            Some(serde_json::json!({"queue_depth": 0})),
            42_000,
        )
        .await
        .unwrap();

    assert_eq!(machine.id, "research");
    assert_eq!(machine.status, "idle");
    assert_eq!(machine.updated_at, 42_000);
    let details: serde_json::Value =
        serde_json::from_str(machine.details_json.as_deref().unwrap()).unwrap();
    assert_eq!(details["worker_id"], "research-worker-testing");
    assert_eq!(details["worker_version"], "0.1.0");
    assert_eq!(details["last_heartbeat_ms"], 42_000);
    assert_eq!(details["details"]["queue_depth"], 0);
}

/// Verifies that research machine heartbeats validate identity and status.
#[tokio::test]
async fn research_machine_heartbeat_rejects_invalid_inputs() {
    let db = test_db();

    for (machine_id, worker_id, status) in [
        ("", "worker", "idle"),
        ("research", "", "idle"),
        ("research", "worker", "mystery"),
        ("missing", "worker", "idle"),
    ] {
        let result = db
            .record_research_machine_heartbeat_at(machine_id, worker_id, None, status, None, 42_000)
            .await;
        assert!(result.is_err(), "{machine_id} {worker_id} {status}");
    }
}

/// Verifies the heartbeat validator caps sample counts and JSON payload sizes.
#[test]
fn research_machine_heartbeat_rejects_oversized_telemetry() {
    let base_machine = "research";
    let base_worker = "worker-a";

    let too_many_samples: Vec<MachineSample> = (0..(RESEARCH_HEARTBEAT_MAX_SAMPLES as i64 + 1))
        .map(|index| telemetry_sample_at(1_000 + index))
        .collect();
    let over_cap = ResearchMachineHeartbeatRecord {
        machine_id: base_machine,
        worker_id: base_worker,
        worker_version: None,
        status: "idle",
        details: None,
        telemetry: ResearchMachineTelemetryUpdate {
            host: None,
            sampler: None,
            samples: &too_many_samples,
            activity: None,
        },
    };
    let samples_result = validate_research_machine_heartbeat(&over_cap);
    assert!(
        matches!(samples_result, Err(DashboardError::BadRequest(_))),
        "over-cap samples must be rejected"
    );

    let big_blob = serde_json::json!({
        "padding": "x".repeat(RESEARCH_HEARTBEAT_MAX_JSON_BYTES + 1),
    });
    let one_sample = vec![telemetry_sample_at(1_000)];

    let over_details = ResearchMachineHeartbeatRecord {
        machine_id: base_machine,
        worker_id: base_worker,
        worker_version: None,
        status: "idle",
        details: Some(&big_blob),
        telemetry: ResearchMachineTelemetryUpdate {
            host: None,
            sampler: None,
            samples: &one_sample,
            activity: None,
        },
    };
    let details_result = validate_research_machine_heartbeat(&over_details);
    assert!(
        matches!(details_result, Err(DashboardError::BadRequest(_))),
        "oversized details must be rejected"
    );

    let over_activity = ResearchMachineHeartbeatRecord {
        machine_id: base_machine,
        worker_id: base_worker,
        worker_version: None,
        status: "idle",
        details: None,
        telemetry: ResearchMachineTelemetryUpdate {
            host: None,
            sampler: None,
            samples: &one_sample,
            activity: Some(&big_blob),
        },
    };
    let activity_result = validate_research_machine_heartbeat(&over_activity);
    assert!(
        matches!(activity_result, Err(DashboardError::BadRequest(_))),
        "oversized activity must be rejected"
    );

    let within_details = serde_json::json!({"phase": "idle"});
    let within_activity = serde_json::json!({"phase": "idle"});
    let within_caps = ResearchMachineHeartbeatRecord {
        machine_id: base_machine,
        worker_id: base_worker,
        worker_version: None,
        status: "idle",
        details: Some(&within_details),
        telemetry: ResearchMachineTelemetryUpdate {
            host: None,
            sampler: None,
            samples: &one_sample,
            activity: Some(&within_activity),
        },
    };
    assert!(
        validate_research_machine_heartbeat(&within_caps).is_ok(),
        "a heartbeat within all caps must validate"
    );
}

/// Verifies telemetry state upsert creates and replaces latest state.
#[tokio::test]
async fn research_machine_telemetry_state_upsert_replaces_latest_state() {
    let db = test_db();
    let host = telemetry_host();
    let first_sampler = telemetry_sampler(1, None);
    let first_activity = serde_json::json!({"phase":"idle","heartbeat_interval_ms":30_000});
    let first_samples = vec![telemetry_sample_at(1_000)];
    db.record_research_machine_heartbeat_with_telemetry_at(
        &ResearchMachineHeartbeatRecord {
            machine_id: "research",
            worker_id: "worker-a",
            worker_version: Some("0.1.0"),
            status: "idle",
            details: Some(&first_activity),
            telemetry: ResearchMachineTelemetryUpdate {
                host: Some(&host),
                sampler: Some(&first_sampler),
                samples: &first_samples,
                activity: Some(&first_activity),
            },
        },
        10_000,
    )
    .await
    .unwrap();

    let second_sampler = telemetry_sampler(2, Some("sampler warning"));
    let second_activity = serde_json::json!({"phase":"processed","processed_last_tick":1,"heartbeat_interval_ms":10_000});
    let second_samples = vec![telemetry_sample_at(2_000)];
    db.record_research_machine_heartbeat_with_telemetry_at(
        &ResearchMachineHeartbeatRecord {
            machine_id: "research",
            worker_id: "worker-a",
            worker_version: Some("0.2.0"),
            status: "busy",
            details: Some(&second_activity),
            telemetry: ResearchMachineTelemetryUpdate {
                host: Some(&host),
                sampler: Some(&second_sampler),
                samples: &second_samples,
                activity: Some(&second_activity),
            },
        },
        20_000,
    )
    .await
    .unwrap();

    let telemetry = db
        .get_research_machine_telemetry("research", None, None)
        .await
        .unwrap();
    let state = telemetry.state.unwrap();
    assert_eq!(state.worker_version.as_deref(), Some("0.2.0"));
    assert_eq!(state.worker_status, "busy");
    assert_eq!(state.last_heartbeat_ms, 20_000);
    assert_eq!(state.last_sample_ms, Some(2_000));
    assert_eq!(state.last_error.as_deref(), Some("sampler warning"));
    assert_eq!(telemetry.samples.len(), 2);
}

/// Verifies sample inserts de-duplicate, bound query sizes, and honor `since_ms`.
#[tokio::test]
async fn research_machine_telemetry_samples_dedupe_limits_and_since() {
    let db = test_db();
    let host = telemetry_host();
    let sampler = telemetry_sampler(800, None);
    let activity = serde_json::json!({"phase":"idle","heartbeat_interval_ms":30_000});
    let samples = (0..800)
        .map(|index| telemetry_sample_at(1_000 + index))
        .collect::<Vec<_>>();
    let mut first_batch = vec![telemetry_sample_at(1_100)];
    first_batch.extend_from_slice(&samples[0..200]);
    let batches: [&[MachineSample]; 4] = [
        first_batch.as_slice(),
        &samples[200..400],
        &samples[400..600],
        &samples[600..800],
    ];

    for batch in batches {
        db.record_research_machine_heartbeat_with_telemetry_at(
            &ResearchMachineHeartbeatRecord {
                machine_id: "research",
                worker_id: "worker-a",
                worker_version: Some("0.1.0"),
                status: "idle",
                details: Some(&activity),
                telemetry: ResearchMachineTelemetryUpdate {
                    host: Some(&host),
                    sampler: Some(&sampler),
                    samples: batch,
                    activity: Some(&activity),
                },
            },
            10_000,
        )
        .await
        .unwrap();
    }

    let default_query = db
        .get_research_machine_telemetry("research", None, None)
        .await
        .unwrap();
    assert_eq!(
        default_query.samples.len(),
        RESEARCH_TELEMETRY_DEFAULT_LIMIT
    );
    assert_eq!(default_query.samples.first().unwrap().sampled_at_ms, 1_740);
    assert_eq!(default_query.samples.last().unwrap().sampled_at_ms, 1_799);

    let explicit = db
        .get_research_machine_telemetry("research", Some(10), Some(1_795))
        .await
        .unwrap();
    assert_eq!(
        explicit
            .samples
            .iter()
            .map(|sample| sample.sampled_at_ms)
            .collect::<Vec<_>>(),
        vec![1_795, 1_796, 1_797, 1_798, 1_799]
    );

    let max_limited = db
        .get_research_machine_telemetry("research", Some(10_000), None)
        .await
        .unwrap();
    assert_eq!(max_limited.samples.len(), RESEARCH_TELEMETRY_MAX_LIMIT);
}

/// Verifies pruning removes samples outside the retention window.
#[tokio::test]
async fn research_machine_telemetry_prunes_old_samples() {
    let db = test_db();
    let host = telemetry_host();
    let sampler = telemetry_sampler(2, None);
    let activity = serde_json::json!({"phase":"idle"});
    let old_sample = telemetry_sample_at(1_000);
    let recent_sample = telemetry_sample_at((RESEARCH_TELEMETRY_RETENTION_MS + 2_000) as i64);
    let samples = vec![old_sample, recent_sample.clone()];

    db.record_research_machine_heartbeat_with_telemetry_at(
        &ResearchMachineHeartbeatRecord {
            machine_id: "research",
            worker_id: "worker-a",
            worker_version: Some("0.1.0"),
            status: "idle",
            details: Some(&activity),
            telemetry: ResearchMachineTelemetryUpdate {
                host: Some(&host),
                sampler: Some(&sampler),
                samples: &samples,
                activity: Some(&activity),
            },
        },
        RESEARCH_TELEMETRY_RETENTION_MS + 2_500,
    )
    .await
    .unwrap();

    let telemetry = db
        .get_research_machine_telemetry("research", Some(10), None)
        .await
        .unwrap();
    assert_eq!(telemetry.samples.len(), 1);
    assert_eq!(
        telemetry.samples[0].sampled_at_ms,
        recent_sample.sampled_at_ms
    );
}

/// Verifies disabled machine status survives telemetry updates.
#[tokio::test]
async fn disabled_research_machine_preserves_status_while_telemetry_updates() {
    let db = test_db();
    let host = telemetry_host();
    let sampler = telemetry_sampler(1, None);
    let activity = serde_json::json!({"phase":"disabled","disabled":true});
    let samples = vec![telemetry_sample_at(1_000)];
    db.set_research_machine_status_at("research", "disabled", 1_000)
        .await
        .unwrap();

    let machine = db
        .record_research_machine_heartbeat_with_telemetry_at(
            &ResearchMachineHeartbeatRecord {
                machine_id: "research",
                worker_id: "worker-a",
                worker_version: Some("0.1.0"),
                status: "idle",
                details: Some(&activity),
                telemetry: ResearchMachineTelemetryUpdate {
                    host: Some(&host),
                    sampler: Some(&sampler),
                    samples: &samples,
                    activity: Some(&activity),
                },
            },
            2_000,
        )
        .await
        .unwrap();

    let telemetry = db
        .get_research_machine_telemetry("research", None, None)
        .await
        .unwrap();
    assert_eq!(machine.status, "disabled");
    assert_eq!(
        telemetry.state.unwrap().activity.unwrap()["disabled"],
        serde_json::json!(true)
    );
}

/// Verifies that custom research machines can be created, updated, and deleted.
#[tokio::test]
async fn custom_research_machine_crud_round_trip() {
    let db = test_db();

    let created = db
        .create_research_machine_at(
            &ResearchMachineRecord {
                id: "gpu-1",
                name: "GPU Worker 1",
                role: "research",
                ssh_alias: Some("testing-gpu-1"),
                status: "configured",
                details_json: Some(r#"{"zone":"desk"}"#),
            },
            1_000,
        )
        .await
        .unwrap();
    assert_eq!(created.id, "gpu-1");
    assert_eq!(created.created_at, 1_000);

    let updated = db
        .update_research_machine_at(
            &ResearchMachineRecord {
                id: "gpu-1",
                name: "GPU Worker A",
                role: "research",
                ssh_alias: None,
                status: "maintenance",
                details_json: Some(r#"{"zone":"rack"}"#),
            },
            2_000,
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "GPU Worker A");
    assert_eq!(updated.ssh_alias, None);
    assert_eq!(updated.status, "maintenance");
    assert_eq!(updated.updated_at, 2_000);

    let deleted = db.delete_research_machine("gpu-1").await.unwrap();
    assert_eq!(deleted.id, "gpu-1");
    assert!(db.get_research_machine("gpu-1").await.unwrap().is_none());
}

/// Verifies machine CRUD rejects invalid identity and metadata.
#[tokio::test]
async fn research_machine_crud_rejects_invalid_records() {
    let db = test_db();

    for record in [
        ResearchMachineRecord {
            id: "",
            name: "Worker",
            role: "research",
            ssh_alias: None,
            status: "configured",
            details_json: None,
        },
        ResearchMachineRecord {
            id: "bad id",
            name: "Worker",
            role: "research",
            ssh_alias: None,
            status: "configured",
            details_json: None,
        },
        ResearchMachineRecord {
            id: "bad-role",
            name: "Worker",
            role: "build",
            ssh_alias: None,
            status: "configured",
            details_json: None,
        },
        ResearchMachineRecord {
            id: "bad-status",
            name: "Worker",
            role: "research",
            ssh_alias: None,
            status: "mystery",
            details_json: None,
        },
        ResearchMachineRecord {
            id: "bad-json",
            name: "Worker",
            role: "research",
            ssh_alias: None,
            status: "configured",
            details_json: Some("{"),
        },
    ] {
        let result = db.create_research_machine(&record).await;
        assert!(result.is_err(), "{}", record.id);
    }
}

/// Verifies delete protects default and referenced research machines.
#[tokio::test]
async fn delete_research_machine_requires_custom_unreferenced_machine() {
    let db = test_db();
    db.create_research_machine(&ResearchMachineRecord {
        id: "source-a",
        name: "Source A",
        role: "live",
        ssh_alias: None,
        status: "configured",
        details_json: None,
    })
    .await
    .unwrap();
    db.create_research_artifact(
        Some("source-a"),
        "readonly_run",
        "available",
        Some("paper"),
        Some("/tmp/source-a/manifest.json"),
    )
    .await
    .unwrap();

    assert!(db.delete_research_machine("live").await.is_err());
    assert!(db.delete_research_machine("source-a").await.is_err());
}

/// Verifies disabled machines preserve heartbeat details but refuse new transfer work.
#[tokio::test]
async fn disabled_research_machine_preserves_status_and_skips_transfer_claims() {
    let db = test_db();
    db.create_research_machine(&ResearchMachineRecord {
        id: "gpu-1",
        name: "GPU Worker 1",
        role: "research",
        ssh_alias: None,
        status: "configured",
        details_json: None,
    })
    .await
    .unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("paper"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    db.create_artifact_transfer(&ArtifactTransferRecord {
        artifact_id: &artifact.id,
        source_machine_id: Some("live"),
        dest_machine_id: Some("gpu-1"),
        bytes_total: Some(100),
    })
    .await
    .unwrap();

    db.set_research_machine_status_at("gpu-1", "disabled", 3_000)
        .await
        .unwrap();
    let heartbeat = db
        .record_research_machine_heartbeat_at(
            "gpu-1",
            "worker-gpu-1",
            Some("0.6.0"),
            "idle",
            Some(serde_json::json!({"queue_depth": 1})),
            4_000,
        )
        .await
        .unwrap();
    assert_eq!(heartbeat.status, "disabled");
    assert!(
        db.claim_next_artifact_transfer("gpu-1")
            .await
            .unwrap()
            .is_none()
    );

    db.set_research_machine_status("gpu-1", "configured")
        .await
        .unwrap();
    assert!(
        db.claim_next_artifact_transfer("gpu-1")
            .await
            .unwrap()
            .is_some()
    );
}

/// Verifies that research job creation writes deterministic steps.
#[tokio::test]
async fn create_research_job_writes_deterministic_steps() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();

    let job = db
        .create_research_job(
            "sweep",
            Some(&artifact.id),
            &user.id,
            5,
            Some(r#"{"dimensions":["LATENCY_ARB_MIN_ASK"]}"#),
        )
        .await
        .unwrap();
    let steps = db.get_research_job_steps(&job.id).await.unwrap();

    assert_eq!(job.status, "queued");
    assert_eq!(job.priority, 5);
    assert_eq!(steps.len(), 6);
    assert_eq!(steps[0].name, "verify_artifact");
    assert_eq!(steps[4].name, "run_sweep");
}

/// Verifies that backtest research jobs require an artifact.
#[tokio::test]
async fn create_research_job_requires_artifact_for_backtest_jobs() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();

    let result = db
        .create_research_job("current_params", None, &user.id, 0, None)
        .await;

    assert!(result.is_err());
}

/// Verifies that unsupported research jobs and missing artifacts are rejected.
#[tokio::test]
async fn create_research_job_rejects_invalid_type_and_missing_artifact() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();

    let invalid_type = db
        .create_research_job("optimize", None, &user.id, 0, None)
        .await;
    let missing_artifact = db
        .create_research_job("sweep", Some("missing-artifact"), &user.id, 0, None)
        .await;

    assert!(invalid_type.is_err());
    assert!(missing_artifact.is_err());
}

/// Verifies reusable research job templates can be managed and marked used.
#[tokio::test]
async fn research_job_templates_crud_and_usage() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();

    let template = db
        .create_research_job_template(&ResearchJobTemplateRecord {
            name: "Two minute backtest",
            description: Some("bounded smoke"),
            job_type: "current_params",
            artifact_id: Some(&artifact.id),
            priority: 3,
            params_json: r#"{"start_ms":1,"end_ms":2}"#,
            operator_id: &user.id,
        })
        .await
        .unwrap();
    let listed = db.list_research_job_templates().await.unwrap();
    let updated = db
        .update_research_job_template(
            &template.id,
            &ResearchJobTemplateRecord {
                name: "Narrow sweep",
                description: None,
                job_type: "sweep",
                artifact_id: Some(&artifact.id),
                priority: 5,
                params_json: r#"{"sweeps":["A=1,2"],"start_ms":1,"end_ms":2}"#,
                operator_id: &user.id,
            },
        )
        .await
        .unwrap();
    let archived = db
        .archive_research_job_template(&template.id)
        .await
        .unwrap();
    let restored = db
        .restore_research_job_template(&template.id)
        .await
        .unwrap();
    let used = db
        .record_research_job_template_use(&template.id)
        .await
        .unwrap();
    let deleted = db.delete_research_job_template(&template.id).await.unwrap();
    let missing = db.get_research_job_template(&template.id).await.unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(template.status, "active");
    assert_eq!(updated.name, "Narrow sweep");
    assert_eq!(updated.description, None);
    assert_eq!(updated.priority, 5);
    assert_eq!(archived.status, "archived");
    assert_eq!(restored.status, "active");
    assert_eq!(used.usage_count, 1);
    assert!(used.last_used_at.is_some());
    assert_eq!(deleted.id, template.id);
    assert!(missing.is_none());
}

/// Verifies reusable research job templates validate type, params, artifact, and name.
#[tokio::test]
async fn research_job_templates_validate_inputs() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let base = ResearchJobTemplateRecord {
        name: "Valid",
        description: None,
        job_type: "current_params",
        artifact_id: None,
        priority: 0,
        params_json: "{}",
        operator_id: &user.id,
    };

    let empty_name = db
        .create_research_job_template(&ResearchJobTemplateRecord {
            name: "",
            ..base.clone()
        })
        .await;
    let invalid_type = db
        .create_research_job_template(&ResearchJobTemplateRecord {
            job_type: "export",
            ..base.clone()
        })
        .await;
    let invalid_params = db
        .create_research_job_template(&ResearchJobTemplateRecord {
            params_json: "[]",
            ..base.clone()
        })
        .await;
    let missing_artifact = db
        .create_research_job_template(&ResearchJobTemplateRecord {
            artifact_id: Some("missing-artifact"),
            ..base
        })
        .await;

    assert!(empty_name.is_err());
    assert!(invalid_type.is_err());
    assert!(invalid_params.is_err());
    assert!(missing_artifact.is_err());
}

/// Verifies that research jobs can be cancelled and retried.
#[tokio::test]
async fn research_job_cancel_and_retry_updates_status() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let cancelled = db.cancel_research_job(&job.id).await.unwrap();
    let cancelled_steps = db.get_research_job_steps(&job.id).await.unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(
        cancelled_steps
            .iter()
            .all(|step| step.status == "cancelled")
    );

    let retried = db.retry_research_job(&job.id).await.unwrap();
    let retried_steps = db.get_research_job_steps(&job.id).await.unwrap();
    assert_eq!(retried.status, "queued");
    assert!(retried_steps.iter().all(|step| step.status == "queued"));
}

/// Verifies is_job_cancelled tracks cancellation and treats a missing job as cancelled.
#[tokio::test]
async fn is_job_cancelled_reflects_status_and_missing_jobs() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    assert!(!db.is_job_cancelled(&job.id).await.unwrap());

    db.cancel_research_job(&job.id).await.unwrap();
    assert!(db.is_job_cancelled(&job.id).await.unwrap());

    assert!(db.is_job_cancelled("missing-job").await.unwrap());
}

/// Verifies retry resumes after completed work instead of rerunning all steps.
#[tokio::test]
async fn research_job_retry_preserves_completed_steps() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    db.mark_research_step_running_at(&lease.step.id, "worker-a", 1_100)
        .await
        .unwrap();
    db.complete_research_step_at(&lease.step.id, "worker-a", Some(r#"{"ok":true}"#), 1_200)
        .await
        .unwrap();

    db.cancel_research_job(&job.id).await.unwrap();
    let cancelled_steps = db.get_research_job_steps(&job.id).await.unwrap();
    assert_eq!(cancelled_steps[0].status, "completed");
    assert!(
        cancelled_steps[1..]
            .iter()
            .all(|step| step.status == "cancelled")
    );

    db.retry_research_job(&job.id).await.unwrap();
    let retried_steps = db.get_research_job_steps(&job.id).await.unwrap();
    assert_eq!(retried_steps[0].status, "completed");
    assert!(
        retried_steps[1..]
            .iter()
            .all(|step| step.status == "queued")
    );
}

/// Verifies queued job metadata updates before any work starts.
#[tokio::test]
async fn update_queued_research_job_changes_metadata_before_steps_start() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let first_artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-1/manifest.json"),
        )
        .await
        .unwrap();
    let second_artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-2/manifest.json"),
        )
        .await
        .unwrap();
    let job = db
        .create_research_job(
            "current_params",
            Some(&first_artifact.id),
            &user.id,
            1,
            Some(r#"{"mode":"one"}"#),
        )
        .await
        .unwrap();

    let updated = db
        .update_queued_research_job(
            &job.id,
            NullableUpdate::Set(&second_artifact.id),
            Some(9),
            NullableUpdate::Clear,
        )
        .await
        .unwrap();
    let clear_required_artifact = db
        .update_queued_research_job(
            &job.id,
            NullableUpdate::Clear,
            None,
            NullableUpdate::Unchanged,
        )
        .await;
    let export = db
        .create_research_job("export", None, &user.id, 1, Some(r#"{"dry_run":true}"#))
        .await
        .unwrap();
    let cleared_export = db
        .update_queued_research_job(
            &export.id,
            NullableUpdate::Clear,
            Some(0),
            NullableUpdate::Clear,
        )
        .await
        .unwrap();

    assert_eq!(
        updated.artifact_id.as_deref(),
        Some(second_artifact.id.as_str())
    );
    assert_eq!(updated.priority, 9);
    assert_eq!(updated.params_json, None);
    assert!(clear_required_artifact.is_err());
    assert_eq!(cleared_export.artifact_id, None);
    assert_eq!(cleared_export.params_json, None);
}

/// Verifies job metadata updates are rejected after step execution starts.
#[tokio::test]
async fn update_queued_research_job_rejects_started_work() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    db.mark_research_step_running_at(&lease.step.id, "worker-a", 1_100)
        .await
        .unwrap();
    db.complete_research_step_at(&lease.step.id, "worker-a", None, 1_200)
        .await
        .unwrap();
    db.cancel_research_job(&job.id).await.unwrap();
    db.resume_research_job(&job.id).await.unwrap();

    let result = db
        .update_queued_research_job(
            &job.id,
            NullableUpdate::Unchanged,
            Some(10),
            NullableUpdate::Unchanged,
        )
        .await;

    assert!(result.is_err());
}

/// Verifies job pause and resume block and restore queued work.
#[tokio::test]
async fn research_job_pause_and_resume_updates_pending_steps() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let paused = db.pause_research_job(&job.id).await.unwrap();
    let paused_steps = db.get_research_job_steps(&job.id).await.unwrap();
    let blocked_lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap();
    let resumed = db.resume_research_job(&job.id).await.unwrap();
    let resumed_steps = db.get_research_job_steps(&job.id).await.unwrap();
    let cancelled = db.cancel_research_job(&job.id).await.unwrap();
    let continued = db.resume_research_job(&job.id).await.unwrap();
    let continued_steps = db.get_research_job_steps(&job.id).await.unwrap();

    assert_eq!(paused.status, "paused");
    assert!(paused_steps.iter().all(|step| step.status == "paused"));
    assert!(blocked_lease.is_none());
    assert_eq!(resumed.status, "queued");
    assert!(resumed_steps.iter().all(|step| step.status == "queued"));
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(continued.status, "queued");
    assert!(continued_steps.iter().all(|step| step.status == "queued"));
}

/// Verifies research job deletion guards active and reported jobs.
#[tokio::test]
async fn delete_research_job_requires_inactive_unreported_job() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let active = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let deletable = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let reported = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    db.cancel_research_job(&deletable.id).await.unwrap();
    db.cancel_research_job(&reported.id).await.unwrap();
    db.append_research_job_event(&deletable.id, None, "info", "delete me", None)
        .await
        .unwrap();
    db.create_or_update_research_report(&ResearchReportRecord {
        job_id: &reported.id,
        artifact_id: None,
        title: "Reported job",
        status: "available",
        summary_json: None,
        report_path: None,
        csv_path: None,
    })
    .await
    .unwrap();

    let active_delete = db.delete_research_job(&active.id).await;
    let reported_delete = db.delete_research_job(&reported.id).await;
    let deleted = db.delete_research_job(&deletable.id).await.unwrap();
    let missing_job = db.get_research_job(&deletable.id).await.unwrap();
    let remaining_steps = db.get_research_job_steps(&deletable.id).await.unwrap();
    let remaining_events = db.list_research_job_events(&deletable.id).await.unwrap();

    assert!(active_delete.is_err());
    assert!(reported_delete.is_err());
    assert_eq!(deleted.id, deletable.id);
    assert!(missing_job.is_none());
    assert!(remaining_steps.is_empty());
    assert!(remaining_events.is_empty());
}

/// Verifies that retry and cancel reject invalid job transitions.
#[tokio::test]
async fn research_job_cancel_and_retry_reject_invalid_transitions() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let retry_queued = db.retry_research_job(&job.id).await;
    let cancel_missing = db.cancel_research_job("missing-job").await;
    assert!(retry_queued.is_err());
    assert!(cancel_missing.is_err());

    while let Some(lease) = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
    {
        db.complete_research_step_at(&lease.step.id, "worker-a", None, 1_100)
            .await
            .unwrap();
    }

    let completed = db.get_research_job(&job.id).await.unwrap().unwrap();
    assert_eq!(completed.status, "completed");
    assert!(db.retry_research_job(&job.id).await.is_err());
    assert_eq!(
        db.cancel_research_job(&job.id).await.unwrap().status,
        "completed"
    );
}

/// Verifies that research job events are appended and listed in order.
#[tokio::test]
async fn research_job_events_append_and_list() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let event = db
        .append_research_job_event(
            &job.id,
            None,
            "info",
            "created export plan",
            Some(r#"{"phase":2}"#),
        )
        .await
        .unwrap();
    let events = db.list_research_job_events(&job.id).await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, event.id);
    assert_eq!(events[0].message, "created export plan");
}

/// Verifies that job events validate level, message, job, and step ownership.
#[tokio::test]
async fn research_job_events_validate_level_message_job_and_step() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let first = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let second = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let second_step = db.get_research_job_steps(&second.id).await.unwrap()[0]
        .id
        .clone();

    assert!(
        db.append_research_job_event(&first.id, None, "debug", "message", None)
            .await
            .is_err()
    );
    assert!(
        db.append_research_job_event(&first.id, None, "info", "", None)
            .await
            .is_err()
    );
    assert!(
        db.append_research_job_event("missing-job", None, "info", "message", None)
            .await
            .is_err()
    );
    assert!(
        db.append_research_job_event(&first.id, Some(&second_step), "info", "message", None)
            .await
            .is_err()
    );
}

/// Verifies that leasing claims the first runnable step only.
#[tokio::test]
async fn lease_next_research_step_claims_first_runnable_step() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease.job.id, job.id);
    assert_eq!(lease.job.status, "running");
    assert_eq!(lease.step.step_index, 0);
    assert_eq!(lease.step.status, "leased");
    assert_eq!(lease.step.lease_owner.as_deref(), Some("worker-a"));
    assert_eq!(lease.step.leased_until_ms, Some(6_000));
    assert_eq!(lease.step.attempts, 1);

    let next = db
        .lease_next_research_step_at("worker-b", 1_100, 5_000)
        .await
        .unwrap();
    assert!(next.is_none());
}

/// Verifies that an expired lease can be reclaimed.
#[tokio::test]
async fn expired_research_step_lease_can_be_reclaimed() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    db.create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let first = db
        .lease_next_research_step_at("worker-a", 1_000, 500)
        .await
        .unwrap()
        .unwrap();
    let second = db
        .lease_next_research_step_at("worker-b", 1_501, 500)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(first.step.id, second.step.id);
    assert_eq!(second.step.lease_owner.as_deref(), Some("worker-b"));
    assert_eq!(second.step.attempts, 2);
}

/// Verifies that an active worker can refresh its step lease.
#[tokio::test]
async fn refresh_research_step_lease_extends_active_lease() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    db.create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 500)
        .await
        .unwrap()
        .unwrap();
    db.mark_research_step_running_at(&lease.step.id, "worker-a", 1_100)
        .await
        .unwrap();

    let refreshed = db
        .refresh_research_step_lease_at(&lease.step.id, "worker-a", 1_400, 1_000)
        .await
        .unwrap();
    let stolen = db
        .lease_next_research_step_at("worker-b", 1_501, 500)
        .await
        .unwrap();
    let bad_owner = db
        .refresh_research_step_lease_at(&lease.step.id, "worker-b", 1_600, 1_000)
        .await;

    assert_eq!(refreshed.status, "running");
    assert_eq!(refreshed.lease_owner.as_deref(), Some("worker-a"));
    assert_eq!(refreshed.leased_until_ms, Some(2_400));
    assert_eq!(refreshed.attempts, 1);
    assert!(stolen.is_none());
    assert!(bad_owner.is_err());
}

/// Verifies that completing a step unlocks the next step.
#[tokio::test]
async fn complete_research_step_unlocks_next_step() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    db.mark_research_step_running_at(&lease.step.id, "worker-a", 1_100)
        .await
        .unwrap();
    let completed = db
        .complete_research_step_at(&lease.step.id, "worker-a", Some(r#"{"ok":true}"#), 1_200)
        .await
        .unwrap();
    let completed_again = db
        .complete_research_step_at(&lease.step.id, "worker-a", None, 1_300)
        .await
        .unwrap();
    let next = db
        .lease_next_research_step_at("worker-a", 1_400, 5_000)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(completed.status, "completed");
    assert_eq!(completed.output_json.as_deref(), Some(r#"{"ok":true}"#));
    assert_eq!(
        completed_again.output_json.as_deref(),
        Some(r#"{"ok":true}"#)
    );
    assert_eq!(next.job.id, job.id);
    assert_eq!(next.step.step_index, 1);
}

/// Verifies that retryable failures update job and step state.
#[tokio::test]
async fn fail_research_step_retryable_updates_job_state() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();

    let failed = db
        .fail_research_step_at(&lease.step.id, "worker-a", "temporary error", true, 1_100)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();

    assert_eq!(failed.status, "retryable");
    assert_eq!(failed.error.as_deref(), Some("temporary error"));
    assert_eq!(job.status, "retryable");
}

/// Verifies that blocked steps update job state.
#[tokio::test]
async fn block_research_step_updates_job_state() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();

    let blocked = db
        .block_research_step_at(&lease.step.id, "worker-a", "waiting for SSH", 1_100)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();

    assert_eq!(blocked.status, "blocked");
    assert_eq!(blocked.error.as_deref(), Some("waiting for SSH"));
    assert_eq!(job.status, "blocked");
}

/// Verifies that a step exceeding the attempt cap is failed on the next lease
/// attempt rather than re-leased forever.
#[tokio::test]
async fn lease_next_research_step_fails_poison_step_at_attempt_cap() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();

    let mut now = 1_000;
    let lease_duration = 500;
    let mut step_id = String::new();
    for expected_attempt in 1..=RESEARCH_STEP_MAX_ATTEMPTS {
        let lease = db
            .lease_next_research_step_at("worker-a", now, lease_duration)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lease.step.attempts, expected_attempt);
        assert_eq!(lease.step.status, "leased");
        step_id = lease.step.id.clone();
        now += lease_duration + 1;
    }

    let poisoned = db
        .lease_next_research_step_at("worker-b", now, lease_duration)
        .await
        .unwrap();
    assert!(poisoned.is_none());

    let steps = db.get_research_job_steps(&job.id).await.unwrap();
    let failed_step = steps
        .iter()
        .find(|step| step.id == step_id)
        .expect("poison step present");
    assert_eq!(failed_step.status, "failed");
    assert_eq!(failed_step.attempts, RESEARCH_STEP_MAX_ATTEMPTS);
    assert_eq!(failed_step.lease_owner, None);
    assert_eq!(failed_step.leased_until_ms, None);
    assert!(
        failed_step
            .error
            .as_deref()
            .is_some_and(|message| message.contains("maximum"))
    );

    let job = db.get_research_job(&job.id).await.unwrap().unwrap();
    assert_eq!(job.status, "failed");
}

/// Verifies operator step retry, cancel, and blocker resolution controls.
#[tokio::test]
async fn research_step_retry_cancel_and_resolve_controls_update_job_state() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    db.fail_research_step_at(&lease.step.id, "worker-a", "temporary", true, 1_100)
        .await
        .unwrap();

    let retried = db
        .retry_research_step(&job.id, &lease.step.id)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 2_000, 5_000)
        .await
        .unwrap()
        .unwrap();
    db.block_research_step_at(&lease.step.id, "worker-a", "operator action", 2_100)
        .await
        .unwrap();
    let resolved = db
        .resolve_research_step_blocker(&job.id, &lease.step.id)
        .await
        .unwrap();
    let cancelled = db
        .cancel_research_step(&job.id, &lease.step.id)
        .await
        .unwrap();
    let continued = db
        .retry_research_step(&job.id, &lease.step.id)
        .await
        .unwrap();
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();

    assert_eq!(retried.status, "queued");
    assert_eq!(resolved.status, "queued");
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(continued.status, "queued");
    assert_eq!(job.status, "queued");
}

/// Verifies stale lease clearing requires an expired active lease.
#[tokio::test]
async fn clear_stale_research_step_lease_requires_expired_active_lease() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 500)
        .await
        .unwrap()
        .unwrap();

    let too_early = db
        .clear_stale_research_step_lease_at(&job.id, &lease.step.id, 1_400)
        .await;
    let cleared = db
        .clear_stale_research_step_lease_at(&job.id, &lease.step.id, 1_600)
        .await
        .unwrap();
    let repeated = db
        .clear_stale_research_step_lease_at(&job.id, &lease.step.id, 1_700)
        .await;
    let job = db.get_research_job(&job.id).await.unwrap().unwrap();

    assert!(too_early.is_err());
    assert_eq!(cleared.status, "retryable");
    assert_eq!(
        cleared.error.as_deref(),
        Some("stale lease cleared by operator")
    );
    assert!(repeated.is_err());
    assert_eq!(job.status, "retryable");
}

/// Verifies that step updates validate worker ownership and messages.
#[tokio::test]
async fn research_step_terminal_updates_validate_worker_and_messages() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let lease = db
        .lease_next_research_step_at("worker-a", 1_000, 5_000)
        .await
        .unwrap()
        .unwrap();

    assert!(
        db.mark_research_step_running_at(&lease.step.id, "worker-b", 1_100)
            .await
            .is_err()
    );
    assert!(
        db.fail_research_step_at(&lease.step.id, "worker-a", "", true, 1_100)
            .await
            .is_err()
    );
    assert!(
        db.block_research_step_at(&lease.step.id, "worker-a", "", 1_100)
            .await
            .is_err()
    );
    assert!(
        db.complete_research_step_at("missing-step", "worker-a", None, 1_100)
            .await
            .is_err()
    );

    let still_first = db.get_research_job_steps(&job.id).await.unwrap().remove(0);
    assert_eq!(still_first.status, "leased");
    assert_eq!(still_first.lease_owner.as_deref(), Some("worker-a"));
}

/// Verifies that artifact upsert and job attachment validate inputs.
#[tokio::test]
async fn research_artifact_upsert_and_attach_validate_inputs() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let mut record = ResearchArtifactRecord {
        id: "artifact-1",
        source_machine_id: Some("live"),
        kind: "readonly_run",
        status: "available",
        run_mode: Some("live_readonly"),
        artifact_root: Some("/tmp/artifact"),
        manifest_path: Some("/tmp/artifact/manifest.json"),
        bundle_path: None,
        source_db_path: Some("/tmp/paint.db"),
        interval_start_ms: Some(1_000),
        interval_end_ms: Some(2_000),
        bytes: Some(10),
        checksum: Some("checksum-1"),
        replay_quality_class: Some("sweep_grade"),
        backtest_ready_class: Some("backtest_ready"),
        live_fidelity_class: Some("research_grade"),
    };

    assert!(
        db.upsert_research_artifact(&ResearchArtifactRecord {
            id: "",
            ..record.clone()
        })
        .await
        .is_err()
    );
    assert!(
        db.upsert_research_artifact(&ResearchArtifactRecord {
            kind: "",
            ..record.clone()
        })
        .await
        .is_err()
    );
    assert!(
        db.upsert_research_artifact(&ResearchArtifactRecord {
            status: "",
            ..record.clone()
        })
        .await
        .is_err()
    );

    let inserted = db.upsert_research_artifact(&record).await.unwrap();
    record.status = "archived";
    record.bytes = Some(11);
    let updated = db.upsert_research_artifact(&record).await.unwrap();
    let attached = db
        .attach_research_job_artifact(&job.id, &updated.id)
        .await
        .unwrap();

    assert_eq!(inserted.id, "artifact-1");
    assert_eq!(updated.status, "archived");
    assert_eq!(updated.bytes, Some(11));
    assert_eq!(attached.artifact_id.as_deref(), Some("artifact-1"));
    assert!(
        db.attach_research_job_artifact(&job.id, "missing-artifact")
            .await
            .is_err()
    );
    assert!(
        db.attach_research_job_artifact("missing-job", &updated.id)
            .await
            .is_err()
    );
}

/// Verifies that artifact transfers are listed with progress metadata.
#[tokio::test]
async fn artifact_transfers_list_progress_records() {
    let db = test_db();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    {
        let conn = db.conn.lock().await;
        conn.execute(
            "INSERT INTO artifact_transfers (
                id, artifact_id, source_machine_id, dest_machine_id, status, bytes_total,
                bytes_done, checksum_status, error, created_at, updated_at, completed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                "transfer-1",
                artifact.id.as_str(),
                "live",
                "research",
                "completed",
                100_i64,
                100_i64,
                "verified",
                Option::<&str>::None,
                1_i64,
                2_i64,
                3_i64
            ],
        )
        .unwrap();
    }

    let transfers = db.list_artifact_transfers().await.unwrap();

    assert_eq!(transfers.len(), 1);
    assert_eq!(transfers[0].id, "transfer-1");
    assert_eq!(transfers[0].artifact_id, artifact.id);
    assert_eq!(transfers[0].source_machine_id.as_deref(), Some("live"));
    assert_eq!(transfers[0].dest_machine_id.as_deref(), Some("research"));
    assert_eq!(transfers[0].bytes_total, Some(100));
    assert_eq!(transfers[0].bytes_done, 100);
    assert_eq!(transfers[0].checksum_status.as_deref(), Some("verified"));
    assert_eq!(transfers[0].completed_at, Some(3));
}

/// Verifies transfer CRUD transitions and validation rules.
#[tokio::test]
async fn artifact_transfer_lifecycle_validates_progress_cancel_and_retry() {
    let db = test_db();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    let transfer = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &artifact.id,
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(100),
        })
        .await
        .unwrap();

    assert_eq!(transfer.status, "queued");
    assert_eq!(transfer.bytes_done, 0);

    let running = db
        .update_artifact_transfer_progress(
            &transfer.id,
            "running",
            Some(40),
            None,
            Some("pending"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(running.status, "running");
    assert_eq!(running.bytes_done, 40);
    assert_eq!(running.checksum_status.as_deref(), Some("pending"));

    assert!(
        db.update_artifact_transfer_progress(&transfer.id, "running", Some(39), None, None, None)
            .await
            .is_err()
    );
    assert!(
        db.update_artifact_transfer_progress(
            &transfer.id,
            "completed",
            Some(100),
            None,
            Some("pending"),
            None,
        )
        .await
        .is_err()
    );

    let cancelled = db.cancel_artifact_transfer(&transfer.id).await.unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let retried = db
        .retry_artifact_transfer(&transfer.id, true)
        .await
        .unwrap();
    assert_eq!(retried.status, "queued");
    assert_eq!(retried.bytes_done, 40);
    let completed = db
        .update_artifact_transfer_progress(
            &transfer.id,
            "completed",
            Some(100),
            None,
            Some("verified"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.completed_at, Some(completed.updated_at));
    assert!(db.cancel_artifact_transfer(&transfer.id).await.is_err());
}

/// Verifies transfer pause, resume, and delete rules.
#[tokio::test]
async fn artifact_transfer_pause_resume_and_delete_rules() {
    let db = test_db();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();
    let transfer = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &artifact.id,
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(100),
        })
        .await
        .unwrap();
    db.update_artifact_transfer_progress(
        &transfer.id,
        "running",
        Some(40),
        None,
        Some("pending"),
        None,
    )
    .await
    .unwrap();

    let paused = db.pause_artifact_transfer(&transfer.id).await.unwrap();
    assert_eq!(paused.status, "paused");
    assert_eq!(paused.bytes_done, 40);
    assert!(
        db.claim_next_artifact_transfer("research")
            .await
            .unwrap()
            .is_none()
    );
    let resumed = db.resume_artifact_transfer(&transfer.id).await.unwrap();
    assert_eq!(resumed.status, "queued");
    assert_eq!(resumed.bytes_done, 40);
    assert!(db.delete_artifact_transfer(&transfer.id).await.is_err());

    let cancelled = db.cancel_artifact_transfer(&transfer.id).await.unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let deleted = db.delete_artifact_transfer(&cancelled.id).await.unwrap();
    assert_eq!(deleted.id, cancelled.id);
    assert!(
        db.get_artifact_transfer(&cancelled.id)
            .await
            .unwrap()
            .is_none()
    );
}

/// Verifies transfer creation rejects missing references and bad sizes.
#[tokio::test]
async fn artifact_transfer_creation_rejects_invalid_references() {
    let db = test_db();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact/manifest.json"),
        )
        .await
        .unwrap();

    assert!(
        db.create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: "missing-artifact",
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(10),
        })
        .await
        .is_err()
    );
    assert!(
        db.create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &artifact.id,
            source_machine_id: Some("missing-machine"),
            dest_machine_id: Some("research"),
            bytes_total: Some(10),
        })
        .await
        .is_err()
    );
    assert!(
        db.create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &artifact.id,
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(0),
        })
        .await
        .is_err()
    );
}

/// Verifies transfer claims are destination-scoped and skip running rows.
#[tokio::test]
async fn artifact_transfer_claims_next_destination_transfer() {
    let db = test_db();
    let first = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-1/manifest.json"),
        )
        .await
        .unwrap();
    let second = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-2/manifest.json"),
        )
        .await
        .unwrap();
    let claimed_source = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &first.id,
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(10),
        })
        .await
        .unwrap();
    db.create_artifact_transfer(&ArtifactTransferRecord {
        artifact_id: &second.id,
        source_machine_id: Some("live"),
        dest_machine_id: Some("live"),
        bytes_total: Some(10),
    })
    .await
    .unwrap();

    let claimed = db
        .claim_next_artifact_transfer("research")
        .await
        .unwrap()
        .unwrap();
    let none = db.claim_next_artifact_transfer("research").await.unwrap();

    assert_eq!(claimed.id, claimed_source.id);
    assert_eq!(claimed.status, "running");
    assert_eq!(claimed.checksum_status.as_deref(), Some("pending"));
    assert!(none.is_none());
}

/// Verifies stale running transfer recovery is destination-scoped.
#[tokio::test]
async fn artifact_transfer_recovers_stale_running_destination_transfer() {
    let db = test_db();
    let research_artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-research/manifest.json"),
        )
        .await
        .unwrap();
    let live_artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-live/manifest.json"),
        )
        .await
        .unwrap();
    let research_transfer = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &research_artifact.id,
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(10),
        })
        .await
        .unwrap();
    let live_transfer = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &live_artifact.id,
            source_machine_id: Some("live"),
            dest_machine_id: Some("live"),
            bytes_total: Some(10),
        })
        .await
        .unwrap();
    db.claim_next_artifact_transfer("research")
        .await
        .unwrap()
        .unwrap();
    db.claim_next_artifact_transfer("live")
        .await
        .unwrap()
        .unwrap();

    let recovered = db
        .recover_stale_artifact_transfers("research", 0)
        .await
        .unwrap();
    let research = db
        .get_artifact_transfer(&research_transfer.id)
        .await
        .unwrap()
        .unwrap();
    let live = db
        .get_artifact_transfer(&live_transfer.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(recovered, 1);
    assert_eq!(research.status, "retryable");
    assert_eq!(research.checksum_status.as_deref(), Some("failed"));
    assert!(
        research
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("stale")
    );
    assert_eq!(live.status, "running");
}

/// Verifies fresh running transfers are not recovered before the stale window.
#[tokio::test]
async fn artifact_transfer_keeps_fresh_running_transfer() {
    let db = test_db();
    let artifact = db
        .create_research_artifact(
            Some("live"),
            "readonly_run",
            "available",
            Some("live_readonly"),
            Some("/tmp/artifact-fresh/manifest.json"),
        )
        .await
        .unwrap();
    let transfer = db
        .create_artifact_transfer(&ArtifactTransferRecord {
            artifact_id: &artifact.id,
            source_machine_id: Some("live"),
            dest_machine_id: Some("research"),
            bytes_total: Some(10),
        })
        .await
        .unwrap();
    db.claim_next_artifact_transfer("research")
        .await
        .unwrap()
        .unwrap();

    let recovered = db
        .recover_stale_artifact_transfers("research", u64::MAX)
        .await
        .unwrap();
    let current = db
        .get_artifact_transfer(&transfer.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(recovered, 0);
    assert_eq!(current.status, "running");
}

/// Verifies that report upsert validates and updates one report per job.
#[tokio::test]
async fn research_report_upsert_validates_and_updates_existing_report() {
    let db = test_db();
    let user = db.create_user("researcher", "hash", "admin").await.unwrap();
    let job = db
        .create_research_job("export", None, &user.id, 0, None)
        .await
        .unwrap();
    let record = ResearchReportRecord {
        job_id: &job.id,
        artifact_id: None,
        title: "Initial report",
        status: "available",
        summary_json: Some(r#"{"ok":true}"#),
        report_path: Some("/tmp/report.json"),
        csv_path: Some("/tmp/report.csv"),
    };

    assert!(
        db.create_or_update_research_report(&ResearchReportRecord {
            title: "",
            ..record.clone()
        })
        .await
        .is_err()
    );
    assert!(
        db.create_or_update_research_report(&ResearchReportRecord {
            status: "",
            ..record.clone()
        })
        .await
        .is_err()
    );

    let first = db.create_or_update_research_report(&record).await.unwrap();
    let updated = db
        .create_or_update_research_report(&ResearchReportRecord {
            title: "Updated report",
            status: "archived",
            summary_json: Some(r#"{"ok":false}"#),
            ..record
        })
        .await
        .unwrap();
    let reports = db.list_research_reports().await.unwrap();

    assert_eq!(first.id, updated.id);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].title, "Updated report");
    assert_eq!(reports[0].status, "archived");
}
