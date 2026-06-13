//! Shared helpers for the research control plane.

use std::path::Path;

/// Convert a path to a lossy UTF-8 display string for storage and errors.
pub(crate) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
