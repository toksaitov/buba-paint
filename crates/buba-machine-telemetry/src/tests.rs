use std::path::PathBuf;

use super::*;

/// Builds a deterministic sample with the given timestamp.
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

/// Builds a host identity fixture for seeded tests.
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
/// State health reflects collection count and the latest sampler error.
fn state_health_reflects_samples_and_errors() {
    let mut state = MachineSamplerState::new();
    state.push(sample_at(10));
    state.set_error("disk refresh failed");
    let health = state.health();
    assert_eq!(health.sample_interval_ms, SAMPLE_INTERVAL_MS as u32);
    assert_eq!(health.samples_collected, 1);
    assert_eq!(health.last_error.as_deref(), Some("disk refresh failed"));
    state.clear_error();
    assert_eq!(state.health().last_error, None);
}

#[test]
/// Seeded sampler snapshots expose the supplied state without spawning a thread.
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
    assert_eq!(snapshot.health().samples_collected, 2);
    assert_eq!(sampler.host().hostname, "test-host");
    assert_eq!(sampler.started_at_ms(), 1_700_000_000_000);
    assert_eq!(sampler.sampled_path(), PathBuf::from("/tmp/test.db"));
}

#[test]
/// Per-core CPU vectors are padded when sysinfo returns fewer cores than identity.
fn per_core_cpu_is_padded_to_cpu_count() {
    assert_eq!(normalize_per_core_cpu(vec![11.0], 3), vec![11.0, 0.0, 0.0]);
}

#[test]
/// Per-core CPU vectors are truncated when sysinfo returns more cores than identity.
fn per_core_cpu_is_truncated_to_cpu_count() {
    assert_eq!(
        normalize_per_core_cpu(vec![1.0, 2.0, 3.0], 2),
        vec![1.0, 2.0]
    );
}

#[test]
/// Per-core CPU normalization always returns at least one element.
fn per_core_cpu_never_returns_empty() {
    assert_eq!(normalize_per_core_cpu(Vec::new(), 0), vec![0.0]);
}

#[test]
/// Load averages reported as zero map to absent values.
fn optional_load_omits_zero() {
    assert_eq!(optional_load(0.0), None);
    assert_eq!(optional_load(-1.0), None);
    assert_eq!(optional_load(0.5), Some(0.5));
}

#[test]
/// Disk mount selection always returns a non-empty label.
fn disk_mount_selection_has_fallback() {
    assert!(!pick_disk_mount(PathBuf::from("/definitely/missing/file.db").as_path()).is_empty());
}

#[test]
/// Empty disk lists produce an explicit unknown disk sample.
fn empty_disk_list_returns_unknown_usage() {
    let disks = Disks::new();
    assert_eq!(
        read_disk_for_mount(&disks, "/missing"),
        (0, 0, "unknown".to_string())
    );
}

#[test]
/// Panic payloads are rendered as operator-readable sampler errors.
fn panic_payloads_are_readable() {
    assert_eq!(panic_message(&"plain panic"), "plain panic");
    assert_eq!(panic_message(&"owned panic".to_string()), "owned panic");
    assert_eq!(panic_message(&123_i32), "sampler panicked");
}
