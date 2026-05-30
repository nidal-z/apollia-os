//! File watcher configuration.
//!
//! This module exposes [`FileWatchConfig`] for the `[triggers.file_watch]`
//! section in `apollia.toml`, along with the default exclusion patterns used by
//! [`crate::types::TriggerSourceConfig::FileWatch`].

use std::path::PathBuf;

use serde::Deserialize;

/// Global file watcher configuration, mapping to `[triggers.file_watch]` in
/// `apollia.toml`.
///
/// These parameters apply to every `FileWatch` source that does not override
/// them individually via its own trigger definition.
#[derive(Debug, Clone, Deserialize)]
pub struct FileWatchConfig {
    /// Root directories to watch.
    pub watch_paths: Vec<PathBuf>,

    /// Follow symbolic links while watching (default: `false`).
    ///
    /// When `false`, events whose resolved path is a symlink are silently
    /// filtered out before propagation to the `TriggerEngine`.
    #[serde(default)]
    pub follow_symlinks: bool,

    /// Path segments or file patterns to exclude from events.
    ///
    /// Supported patterns:
    /// - `"name"` or `"name/"`: excludes any path containing the segment `name`
    /// - `"*.ext"`: excludes any file whose name ends with `.ext`
    ///
    /// Default: `[".git", "node_modules", "__pycache__", ".apollia"]`.
    #[serde(default = "default_exclude_patterns")]
    pub exclude_patterns: Vec<String>,
}

/// Returns the exclusion patterns applied by default to all `FileWatch` sources.
///
/// List: `.git`, `node_modules`, `__pycache__`, `.apollia`.
pub(crate) fn default_exclude_patterns() -> Vec<String> {
    vec![
        ".git".into(),
        "node_modules".into(),
        "__pycache__".into(),
        ".apollia".into(),
    ]
}
