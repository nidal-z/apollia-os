use std::path::PathBuf;

use serde::Deserialize;

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// FilesystemConfig / JournalConfig
// ─────────────────────────────────────────────

/// Reversible filesystem journal configuration (`[filesystem.journal]` section in `apollia.toml`).
///
/// Controls the journal that persists the prior state of each native mutation
/// before it is applied. Lets `apollia-os rollback` restore the disk after an
/// agent performs unwanted operations.
///
/// Every field has a sane default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct JournalConfig {
    /// Enables the reversible journal. Default: `true`.
    ///
    /// When `false`, `FileWrite` and `FileEdit` mutate without recording.
    /// Disable only for controlled test environments.
    #[serde(default = "default_journal_enabled")]
    pub enabled: bool,

    /// Maximum number of sessions kept on disk before the oldest is purged.
    ///
    /// Default: 50. Bounds: [1, 10 000].
    #[serde(default = "default_journal_max_sessions")]
    pub max_sessions: usize,

    /// Journal root directory. `~` is resolved at startup.
    ///
    /// Default: `~/.apollia/journal`.
    #[serde(default = "default_journal_root")]
    pub root: PathBuf,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            enabled: default_journal_enabled(),
            max_sessions: default_journal_max_sessions(),
            root: default_journal_root(),
        }
    }
}

impl JournalConfig {
    /// Validates the journal configuration bounds at startup (fail-fast).
    ///
    /// - `max_sessions`: must be in [1, 10 000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "filesystem.journal.max_sessions",
            self.max_sessions,
            1_usize,
            10_000_usize,
        )?;
        Ok(())
    }

    /// Resolves `~` in `root` to the effective home directory.
    ///
    /// Returns the resolved path without modifying `self`.
    pub fn resolved_root(&self) -> PathBuf {
        let s = self.root.to_string_lossy();
        if s.starts_with("~/") {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(s.trim_start_matches("~/"))
        } else if s == "~" {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
        } else {
            self.root.clone()
        }
    }
}

fn default_journal_enabled() -> bool {
    true
}

fn default_journal_max_sessions() -> usize {
    50
}

fn default_journal_root() -> PathBuf {
    PathBuf::from("~/.apollia/journal")
}

/// Agent filesystem configuration (`[filesystem]` section in `apollia.toml`).
///
/// Groups every sub-configuration related to filesystem operations: currently
/// the reversible journal.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FilesystemConfig {
    /// Sub-section dedicated to the reversible journal.
    #[serde(default)]
    pub journal: JournalConfig,
}

impl FilesystemConfig {
    /// Validates the filesystem configuration at startup (fail-fast).
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.journal.validate()
    }
}
