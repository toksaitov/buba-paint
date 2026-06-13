//! Shared helpers for the research control plane.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Convert a path to a lossy UTF-8 display string for storage and errors.
pub(crate) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Return the current Unix time in milliseconds, or zero before the epoch.
pub(crate) fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Normalize an optional raw string to a trimmed, non-empty owned value.
pub fn optional_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Read an environment variable as a trimmed, non-empty owned value.
pub fn optional_env(name: &str) -> Option<String> {
    optional_value(std::env::var(name).ok().as_deref())
}
