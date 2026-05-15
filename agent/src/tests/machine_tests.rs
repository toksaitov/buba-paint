use std::path::PathBuf;

use super::{
    HostIdentity, MachineSample, MachineSampler, MachineSamplerState, RING_CAPACITY,
    stat_runtime_db_files,
};

/// Builds a deterministic sample with the given identifier baked into a few fields.
fn sample_at(sampled_at_ms: i64) -> MachineSample {
    MachineSample {
        sampled_at_ms,
        cpu_percent: 42.0,
        per_core_cpu: vec![40.0, 44.0],
        load_one: Some(1.0),
        load_five: Some(0.8),
        load_fifteen: Some(0.6),
        mem_used_bytes: 1_000,
        mem_total_bytes: 4_000,
        mem_available_bytes: 3_000,
        swap_used_bytes: 0,
        swap_total_bytes: 0,
        disk_used_bytes: 10_000,
        disk_total_bytes: 100_000,
        disk_mount: "/".into(),
    }
}

/// Builds a HostIdentity fixture for seeded tests.
fn fixture_host() -> HostIdentity {
    HostIdentity {
        hostname: "test-host".into(),
        os_name: "test-os".into(),
        os_version: "1.0".into(),
        kernel_version: "5.0".into(),
        cpu_count: 2,
        total_ram_bytes: 4_000,
    }
}

#[test]
/// Ring buffer must evict the oldest entry when pushing past capacity.
fn ring_buffer_evicts_oldest_at_capacity() {
    let mut state = MachineSamplerState::new();
    for i in 0..(RING_CAPACITY as i64 + 1) {
        state.push(sample_at(i));
    }
    assert_eq!(state.history.len(), RING_CAPACITY);
    assert_eq!(
        state.history.front().expect("ring non-empty").sampled_at_ms,
        1
    );
    assert_eq!(
        state.history.back().expect("ring non-empty").sampled_at_ms,
        RING_CAPACITY as i64
    );
    assert_eq!(state.samples_collected, RING_CAPACITY as u64 + 1);
}

#[test]
/// `with_seeded_state` produces a sampler whose snapshot reflects the seed.
fn seeded_state_produces_expected_snapshot() {
    let mut state = MachineSamplerState::new();
    state.push(sample_at(100));
    state.push(sample_at(200));
    let sampler = MachineSampler::with_seeded_state(
        fixture_host(),
        state,
        1_700_000_000_000,
        PathBuf::from("/tmp/test.db"),
    );
    let snapshot = sampler.snapshot();
    assert_eq!(snapshot.history.len(), 2);
    assert_eq!(
        snapshot
            .current
            .as_ref()
            .expect("current present")
            .sampled_at_ms,
        200
    );
    assert_eq!(snapshot.samples_collected, 2);
    assert_eq!(sampler.host().hostname, "test-host");
    assert_eq!(sampler.started_at_ms(), 1_700_000_000_000);
}

#[test]
/// `stat_runtime_db_files` reports `None` for every missing file.
fn stat_runtime_db_files_reports_none_when_missing() {
    let path = PathBuf::from("/tmp/buba-machine-tests-nonexistent.db");
    let files = stat_runtime_db_files(&path);
    assert_eq!(files.db_bytes, None);
    assert_eq!(files.wal_bytes, None);
    assert_eq!(files.shm_bytes, None);
    assert_eq!(files.db_path, "/tmp/buba-machine-tests-nonexistent.db");
}

#[test]
/// `stat_runtime_db_files` reports the actual file length when files exist.
fn stat_runtime_db_files_reports_sizes_when_present() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("paint.db");
    std::fs::write(&db_path, b"hello").expect("write db");
    std::fs::write(tmp.path().join("paint.db-wal"), b"wallong").expect("write wal");
    let files = stat_runtime_db_files(&db_path);
    assert_eq!(files.db_bytes, Some(5));
    assert_eq!(files.wal_bytes, Some(7));
    assert_eq!(files.shm_bytes, None);
}
