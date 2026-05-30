//! Shared cross-platform host telemetry sampler.

use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sysinfo::{Disks, System};

/// Maximum number of samples held in the in-memory ring.
pub const RING_CAPACITY: usize = 60;

/// Sampler tick cadence in milliseconds.
pub const SAMPLE_INTERVAL_MS: u64 = 5_000;

/// One-shot host identity captured at sampler boot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostIdentity {
    /// Hostname reported by the operating system.
    pub hostname: String,
    /// Operating system name.
    pub os_name: String,
    /// Operating system version string.
    pub os_version: String,
    /// Kernel version string.
    pub kernel_version: String,
    /// Number of logical CPU cores.
    pub cpu_count: usize,
    /// Total host RAM in bytes.
    pub total_ram_bytes: u64,
}

/// Single host metric sample emitted by the background sampler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MachineSample {
    /// Wall-clock sample time in Unix epoch milliseconds.
    pub sampled_at_ms: i64,
    /// Global CPU usage percentage.
    pub cpu_percent: f32,
    /// Per-core CPU usage percentages, normalized to host CPU count.
    pub per_core_cpu: Vec<f32>,
    /// One-minute load average, absent on platforms that do not report it.
    pub load_one: Option<f32>,
    /// Five-minute load average, absent on platforms that do not report it.
    pub load_five: Option<f32>,
    /// Fifteen-minute load average, absent on platforms that do not report it.
    pub load_fifteen: Option<f32>,
    /// Used memory in bytes.
    pub mem_used_bytes: u64,
    /// Total memory in bytes.
    pub mem_total_bytes: u64,
    /// Available memory in bytes.
    pub mem_available_bytes: u64,
    /// Used swap in bytes.
    pub swap_used_bytes: u64,
    /// Total swap in bytes.
    pub swap_total_bytes: u64,
    /// Used bytes on the sampled disk mount.
    pub disk_used_bytes: u64,
    /// Total bytes on the sampled disk mount.
    pub disk_total_bytes: u64,
    /// Disk mount selected for the sampled path.
    pub disk_mount: String,
}

/// Sampler health metadata returned alongside host snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineSamplerHealth {
    /// Configured sample interval in milliseconds.
    pub sample_interval_ms: u32,
    /// Total samples successfully collected since sampler startup.
    pub samples_collected: u64,
    /// Last sampler error, if any.
    pub last_error: Option<String>,
}

/// Internal mutable state guarded by the sampler mutex.
#[derive(Debug, Clone)]
pub struct MachineSamplerState {
    /// Most recent sample, if one has been collected.
    pub current: Option<MachineSample>,
    /// Bounded sample history.
    pub history: VecDeque<MachineSample>,
    /// Last sampler error, if any.
    pub last_error: Option<String>,
    /// Total samples successfully collected.
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

    /// Records a sampler error without dropping the last successful sample.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
    }

    /// Clears the last sampler error.
    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    /// Returns health metadata for this state.
    #[must_use]
    pub fn health(&self) -> MachineSamplerHealth {
        MachineSamplerHealth {
            sample_interval_ms: SAMPLE_INTERVAL_MS as u32,
            samples_collected: self.samples_collected,
            last_error: self.last_error.clone(),
        }
    }
}

impl Default for MachineSamplerState {
    /// Delegates to `MachineSamplerState::new`.
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot returned by `MachineSampler::snapshot`.
#[derive(Debug, Clone)]
pub struct MachineSnapshot {
    /// Most recent sample, if one has been collected.
    pub current: Option<MachineSample>,
    /// Bounded sample history.
    pub history: Vec<MachineSample>,
    /// Last sampler error, if any.
    pub last_error: Option<String>,
    /// Total samples successfully collected.
    pub samples_collected: u64,
}

impl MachineSnapshot {
    /// Returns health metadata for this snapshot.
    #[must_use]
    pub fn health(&self) -> MachineSamplerHealth {
        MachineSamplerHealth {
            sample_interval_ms: SAMPLE_INTERVAL_MS as u32,
            samples_collected: self.samples_collected,
            last_error: self.last_error.clone(),
        }
    }
}

/// Background cross-platform host sampler.
pub struct MachineSampler {
    state: Arc<Mutex<MachineSamplerState>>,
    host: HostIdentity,
    started_at_ms: i64,
    sampled_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    _handle: Option<JoinHandle<()>>,
}

impl MachineSampler {
    /// Builds the sampler and spawns the background collection thread.
    pub fn start(sampled_path: PathBuf) -> Arc<Self> {
        let mut system = System::new_all();
        system.refresh_all();
        let host = build_host_identity(&system);
        let disk_mount = pick_disk_mount(&sampled_path);

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
            tracing::warn!("machine sampler thread failed to spawn");
        }

        Arc::new(Self {
            state,
            host,
            started_at_ms,
            sampled_path,
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

    /// Returns the path used to select the disk mount.
    #[must_use]
    pub fn sampled_path(&self) -> &Path {
        &self.sampled_path
    }

    /// Returns a cloned snapshot of samples and sampler health metadata.
    #[must_use]
    pub fn snapshot(&self) -> MachineSnapshot {
        let guard = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        MachineSnapshot {
            current: guard.current.clone(),
            history: guard.history.iter().cloned().collect(),
            last_error: guard.last_error.clone(),
            samples_collected: guard.samples_collected,
        }
    }

    /// Builds a sampler with prebuilt state and no background thread.
    #[must_use]
    pub fn with_seeded_state(
        host: HostIdentity,
        state: MachineSamplerState,
        started_at_ms: i64,
        sampled_path: PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(Mutex::new(state)),
            host,
            started_at_ms,
            sampled_path,
            shutdown: Arc::new(AtomicBool::new(true)),
            _handle: None,
        })
    }
}

impl Drop for MachineSampler {
    /// Signals the background thread to stop.
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// Runs the sampler thread loop until shutdown is requested.
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
                    guard.clear_error();
                    guard.push(sample);
                }
            }
            Err(payload) => {
                let message = panic_message(payload.as_ref());
                tracing::warn!("machine sampler tick panicked: {message}");
                if let Ok(mut guard) = state.lock() {
                    guard.set_error(message);
                }
            }
        }
        thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
    }
}

/// Produces one host sample from the shared `System` instance.
fn sample_once(system: &mut System, disk_mount: &str, cpu_count: usize) -> MachineSample {
    system.refresh_cpu_usage();
    system.refresh_memory();
    let cpu_percent = system.global_cpu_usage();
    let per_core_cpu = normalize_per_core_cpu(
        system.cpus().iter().map(sysinfo::Cpu::cpu_usage).collect(),
        cpu_count,
    );

    let load = System::load_average();
    let load_one = optional_load(load.one);
    let load_five = optional_load(load.five);
    let load_fifteen = optional_load(load.fifteen);

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
        mem_used_bytes: system.used_memory(),
        mem_total_bytes: system.total_memory(),
        mem_available_bytes: system.available_memory(),
        swap_used_bytes: system.used_swap(),
        swap_total_bytes: system.total_swap(),
        disk_used_bytes,
        disk_total_bytes,
        disk_mount: resolved_mount,
    }
}

/// Normalizes per-core CPU samples to the captured host CPU count.
fn normalize_per_core_cpu(mut per_core_cpu: Vec<f32>, cpu_count: usize) -> Vec<f32> {
    per_core_cpu.resize(cpu_count.max(1), 0.0);
    per_core_cpu
}

/// Maps a sysinfo load value to `None` when the platform reports zero.
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

/// Finds the mount point of the disk containing the sampled path.
fn pick_disk_mount(sampled_path: &Path) -> String {
    let canonical_path =
        std::fs::canonicalize(sampled_path).unwrap_or_else(|_| sampled_path.to_path_buf());
    let disks = Disks::new_with_refreshed_list();
    let mut best: Option<(usize, String)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point().to_path_buf();
        let canonical_mount = std::fs::canonicalize(&mount).unwrap_or(mount);
        if canonical_path.starts_with(&canonical_mount) {
            let depth = canonical_mount.components().count();
            if best.as_ref().is_none_or(|(d, _)| depth > *d) {
                best = Some((depth, canonical_mount.display().to_string()));
            }
        }
    }
    best.map_or_else(|| "/".to_string(), |(_, mount)| mount)
}

/// Reads disk usage for the given mount and returns used, total, and label.
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

/// Returns the current wall-clock millisecond epoch.
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
#[path = "tests.rs"]
mod tests;
