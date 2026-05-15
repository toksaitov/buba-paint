//! Cross-platform host sampler used by the Machine page.

use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{Disks, System};

use crate::types::RuntimeDbFiles;

/// Maximum number of samples held in the in-memory ring (5 minutes at 5 s cadence).
pub const RING_CAPACITY: usize = 60;

/// Sampler tick cadence in milliseconds.
pub const SAMPLE_INTERVAL_MS: u64 = 5_000;

/// One-shot host identity captured at sampler boot.
#[derive(Debug, Clone, Serialize)]
pub struct HostIdentity {
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub cpu_count: usize,
    pub total_ram_bytes: u64,
}

/// Single host metric sample emitted by the background sampler.
#[derive(Debug, Clone, Serialize)]
pub struct MachineSample {
    pub sampled_at_ms: i64,
    pub cpu_percent: f32,
    pub per_core_cpu: Vec<f32>,
    pub load_one: Option<f32>,
    pub load_five: Option<f32>,
    pub load_fifteen: Option<f32>,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub mem_available_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_mount: String,
}

/// Internal mutable state guarded by the sampler mutex.
#[derive(Debug)]
pub struct MachineSamplerState {
    pub current: Option<MachineSample>,
    pub history: VecDeque<MachineSample>,
    pub last_error: Option<String>,
    pub samples_collected: u64,
}

impl MachineSamplerState {
    /// Builds an empty state with the ring buffer pre-allocated.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: None,
            history: VecDeque::with_capacity(RING_CAPACITY),
            last_error: None,
            samples_collected: 0,
        }
    }

    /// Pushes one sample into the ring, evicting the oldest entry at capacity.
    pub fn push(&mut self, sample: MachineSample) {
        if self.history.len() == RING_CAPACITY {
            self.history.pop_front();
        }
        self.current = Some(sample.clone());
        self.history.push_back(sample);
        self.samples_collected = self.samples_collected.saturating_add(1);
    }
}

impl Default for MachineSamplerState {
    /// Delegates to `MachineSamplerState::new`.
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot returned by `MachineSampler::snapshot` for the route handler.
#[derive(Debug, Clone)]
pub struct MachineSnapshot {
    pub current: Option<MachineSample>,
    pub history: Vec<MachineSample>,
    pub last_error: Option<String>,
    pub samples_collected: u64,
}

/// Background cross-platform host sampler. One instance per agent process.
pub struct MachineSampler {
    state: Arc<Mutex<MachineSamplerState>>,
    host: HostIdentity,
    started_at_ms: i64,
    runtime_db_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    _handle: Option<JoinHandle<()>>,
}

impl MachineSampler {
    /// Builds the sampler, captures the one-shot host identity, and spawns the
    /// background thread that refreshes a `sysinfo::System` instance every 5 s.
    pub fn start(runtime_db_path: PathBuf) -> Arc<Self> {
        let mut system = System::new_all();
        system.refresh_all();
        let host = build_host_identity(&system);
        let disk_mount = pick_disk_mount(&runtime_db_path);

        let state = Arc::new(Mutex::new(MachineSamplerState::new()));
        let started_at_ms = now_ms();
        let shutdown = Arc::new(AtomicBool::new(false));

        let state_for_thread = Arc::clone(&state);
        let shutdown_for_thread = Arc::clone(&shutdown);
        let cpu_count = host.cpu_count;

        let handle = thread::Builder::new()
            .name("machine-sampler".into())
            .spawn(move || {
                run_sampler_loop(
                    system,
                    &state_for_thread,
                    &shutdown_for_thread,
                    &disk_mount,
                    cpu_count,
                );
            })
            .ok();

        if handle.is_none() {
            tracing::warn!("machine sampler thread failed to spawn; page will show no history");
        }

        Arc::new(Self {
            state,
            host,
            started_at_ms,
            runtime_db_path,
            shutdown,
            _handle: handle,
        })
    }

    /// Returns the immutable host identity captured at boot.
    #[must_use]
    pub fn host(&self) -> &HostIdentity {
        &self.host
    }

    /// Returns the wall-clock millisecond epoch at sampler startup.
    #[must_use]
    pub fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }

    /// Returns the configured path to the bot's runtime DB, used by the handler
    /// to stat file sizes per request.
    #[must_use]
    pub fn runtime_db_path(&self) -> &Path {
        &self.runtime_db_path
    }

    /// Returns a cloned snapshot of the current sample, the history ring,
    /// and sampler health metadata.
    pub fn snapshot(&self) -> MachineSnapshot {
        let guard = self
            .state
            .lock()
            .expect("machine sampler mutex poisoned by sampler thread panic");
        MachineSnapshot {
            current: guard.current.clone(),
            history: guard.history.iter().cloned().collect(),
            last_error: guard.last_error.clone(),
            samples_collected: guard.samples_collected,
        }
    }

    /// Builds a sampler with prebuilt state and no background thread. Used by
    /// integration tests and unit tests; exposed here because it crosses crate
    /// boundaries.
    pub fn with_seeded_state(
        host: HostIdentity,
        state: MachineSamplerState,
        started_at_ms: i64,
        runtime_db_path: PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(Mutex::new(state)),
            host,
            started_at_ms,
            runtime_db_path,
            shutdown: Arc::new(AtomicBool::new(true)),
            _handle: None,
        })
    }
}

impl Drop for MachineSampler {
    /// Signals the background thread to stop. The thread will exit on its next
    /// loop iteration.
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// Sampler thread main loop. Wraps each refresh in `catch_unwind` so a
/// platform-specific `sysinfo` bug surfaces as `last_error` instead of crashing
/// the whole agent.
fn run_sampler_loop(
    mut system: System,
    state: &Arc<Mutex<MachineSamplerState>>,
    shutdown: &Arc<AtomicBool>,
    disk_mount: &str,
    cpu_count: usize,
) {
    while !shutdown.load(Ordering::Relaxed) {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            sample_once(&mut system, disk_mount, cpu_count)
        }));
        match result {
            Ok(sample) => {
                if let Ok(mut guard) = state.lock() {
                    guard.last_error = None;
                    guard.push(sample);
                }
            }
            Err(payload) => {
                let message = panic_message(payload.as_ref());
                tracing::warn!("machine sampler tick panicked: {message}");
                if let Ok(mut guard) = state.lock() {
                    guard.last_error = Some(message);
                }
            }
        }
        thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
    }
}

/// Produces one machine sample from the shared `System` instance.
fn sample_once(system: &mut System, disk_mount: &str, cpu_count: usize) -> MachineSample {
    system.refresh_cpu_usage();
    system.refresh_memory();
    let cpu_percent = system.global_cpu_usage();
    let mut per_core_cpu: Vec<f32> = system.cpus().iter().map(sysinfo::Cpu::cpu_usage).collect();
    if per_core_cpu.len() != cpu_count {
        per_core_cpu.resize(cpu_count, 0.0);
    }

    let load = System::load_average();
    let load_one = optional_load(load.one);
    let load_five = optional_load(load.five);
    let load_fifteen = optional_load(load.fifteen);

    let mem_used_bytes = system.used_memory();
    let mem_total_bytes = system.total_memory();
    let mem_available_bytes = system.available_memory();
    let swap_used_bytes = system.used_swap();
    let swap_total_bytes = system.total_swap();

    let disks = Disks::new_with_refreshed_list();
    let (disk_used_bytes, disk_total_bytes, resolved_mount) =
        read_disk_for_mount(&disks, disk_mount);

    MachineSample {
        sampled_at_ms: now_ms(),
        cpu_percent,
        per_core_cpu,
        load_one,
        load_five,
        load_fifteen,
        mem_used_bytes,
        mem_total_bytes,
        mem_available_bytes,
        swap_used_bytes,
        swap_total_bytes,
        disk_used_bytes,
        disk_total_bytes,
        disk_mount: resolved_mount,
    }
}

/// Maps a sysinfo load value to None when the platform reports zero (Windows).
fn optional_load(value: f64) -> Option<f32> {
    if value > 0.0 {
        Some(value as f32)
    } else {
        None
    }
}

/// Builds the one-shot host identity record from the initial refresh.
fn build_host_identity(system: &System) -> HostIdentity {
    HostIdentity {
        hostname: System::host_name().unwrap_or_else(|| "unknown".to_string()),
        os_name: System::name().unwrap_or_else(|| "unknown".to_string()),
        os_version: System::os_version().unwrap_or_else(|| "unknown".to_string()),
        kernel_version: System::kernel_version().unwrap_or_else(|| "unknown".to_string()),
        cpu_count: system.cpus().len().max(1),
        total_ram_bytes: system.total_memory(),
    }
}

/// Finds the mount point of the disk containing the bot's runtime DB, falling
/// back to "/" if no ancestor match is found.
fn pick_disk_mount(runtime_db_path: &Path) -> String {
    let canonical_db =
        std::fs::canonicalize(runtime_db_path).unwrap_or_else(|_| runtime_db_path.to_path_buf());
    let disks = Disks::new_with_refreshed_list();
    let mut best: Option<(usize, String)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point().to_path_buf();
        let canonical_mount = std::fs::canonicalize(&mount).unwrap_or(mount);
        if canonical_db.starts_with(&canonical_mount) {
            let depth = canonical_mount.components().count();
            if best.as_ref().is_none_or(|(d, _)| depth > *d) {
                best = Some((depth, canonical_mount.display().to_string()));
            }
        }
    }
    best.map_or_else(|| "/".to_string(), |(_, mount)| mount)
}

/// Reads disk usage for the given mount and returns (used, total, label).
fn read_disk_for_mount(disks: &Disks, mount: &str) -> (u64, u64, String) {
    for disk in disks.list() {
        if disk.mount_point().to_string_lossy() == mount {
            let total = disk.total_space();
            let available = disk.available_space();
            return (total.saturating_sub(available), total, mount.to_string());
        }
    }
    for disk in disks.list() {
        if disk.mount_point().to_string_lossy() == "/" {
            let total = disk.total_space();
            let available = disk.available_space();
            return (total.saturating_sub(available), total, "/".to_string());
        }
    }
    if let Some(disk) = disks.list().first() {
        let total = disk.total_space();
        let available = disk.available_space();
        return (
            total.saturating_sub(available),
            total,
            disk.mount_point().display().to_string(),
        );
    }
    (0, 0, "unknown".to_string())
}

/// Builds the on-demand DB / WAL / SHM file-size record. Missing files map to
/// `None`, not an error.
#[must_use]
pub fn stat_runtime_db_files(runtime_db_path: &Path) -> RuntimeDbFiles {
    let db_bytes = std::fs::metadata(runtime_db_path).ok().map(|m| m.len());
    let wal_bytes = std::fs::metadata(sibling(runtime_db_path, "-wal"))
        .ok()
        .map(|m| m.len());
    let shm_bytes = std::fs::metadata(sibling(runtime_db_path, "-shm"))
        .ok()
        .map(|m| m.len());
    RuntimeDbFiles {
        db_path: runtime_db_path.display().to_string(),
        db_bytes,
        wal_bytes,
        shm_bytes,
    }
}

/// Returns a sibling path with the given suffix appended to the original file
/// name (e.g. "paint.db" + "-wal" -> "paint.db-wal").
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    parent.join(format!("{file_name}{suffix}"))
}

/// Returns the current wall-clock millisecond epoch (0 on clock errors).
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Best-effort downcast of a panic payload to a human-readable message.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "sampler panicked".to_string()
    }
}

#[cfg(test)]
#[path = "tests/machine_tests.rs"]
mod tests;
