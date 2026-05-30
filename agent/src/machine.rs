//! Bot-agent machine telemetry adapter.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::types::RuntimeDbFiles;

pub use buba_machine_telemetry::{
    HostIdentity, MachineSample, MachineSamplerHealth, MachineSamplerState, MachineSnapshot,
    RING_CAPACITY, SAMPLE_INTERVAL_MS,
};

use buba_machine_telemetry::MachineSampler as SharedMachineSampler;

/// Agent-owned wrapper around the shared host sampler.
pub struct MachineSampler {
    inner: Arc<SharedMachineSampler>,
    runtime_db_path: PathBuf,
}

impl MachineSampler {
    /// Builds the shared sampler against the bot runtime DB path.
    pub fn start(runtime_db_path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            inner: SharedMachineSampler::start(runtime_db_path.clone()),
            runtime_db_path,
        })
    }

    /// Returns the immutable host identity captured at sampler boot.
    #[must_use]
    pub fn host(&self) -> &HostIdentity {
        self.inner.host()
    }

    /// Returns the wall-clock millisecond epoch at sampler startup.
    #[must_use]
    pub fn started_at_ms(&self) -> i64 {
        self.inner.started_at_ms()
    }

    /// Returns the bot runtime DB path used for per-request file-size stats.
    #[must_use]
    pub fn runtime_db_path(&self) -> &Path {
        &self.runtime_db_path
    }

    /// Returns a cloned snapshot of samples and sampler health metadata.
    #[must_use]
    pub fn snapshot(&self) -> MachineSnapshot {
        self.inner.snapshot()
    }

    /// Builds a sampler with prebuilt state and no background thread.
    #[must_use]
    pub fn with_seeded_state(
        host: HostIdentity,
        state: MachineSamplerState,
        started_at_ms: i64,
        runtime_db_path: PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: SharedMachineSampler::with_seeded_state(
                host,
                state,
                started_at_ms,
                runtime_db_path.clone(),
            ),
            runtime_db_path,
        })
    }
}

/// Builds the on-demand DB / WAL / SHM file-size record.
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

/// Returns a sibling path with the given suffix appended to the original file name.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    parent.join(format!("{file_name}{suffix}"))
}

#[cfg(test)]
#[path = "tests/machine_tests.rs"]
mod tests;
