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
/// Groups every sub-configuration related to filesystem operations: the
/// reversible journal, and the paths the operator trusts an agent to work in
/// without being asked each time.
#[derive(Debug, Clone, Deserialize)]
pub struct FilesystemConfig {
    /// Sub-section dedicated to the reversible journal.
    #[serde(default)]
    pub journal: JournalConfig,

    /// Paths an agent may read and write without an approval prompt.
    ///
    /// `~` is resolved by [`FilesystemConfig::resolved_trusted_paths`]. Default:
    /// `["~"]`, the user's home directory.
    ///
    /// This list is a friction boundary, not a wall. A path outside it is not
    /// refused: it is classified one risk level higher, which is what suspends
    /// the operation and asks the user. Emptying the list therefore does not
    /// lock an agent out of the machine, it only means every write is asked
    /// about. Naming a path here is a statement of trust, and the sensitive
    /// paths of `FilesystemRiskConfig` (`~/.ssh`, `/etc`, credentials) keep
    /// their own classification whatever this list says.
    #[serde(default = "default_trusted_paths")]
    pub trusted_paths: Vec<PathBuf>,
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            journal: JournalConfig::default(),
            trusted_paths: default_trusted_paths(),
        }
    }
}

fn default_trusted_paths() -> Vec<PathBuf> {
    vec![PathBuf::from("~")]
}

impl FilesystemConfig {
    /// Validates the filesystem configuration at startup (fail-fast).
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.journal.validate()
    }

    /// The trusted paths with `~` resolved, ready to compare against a path.
    ///
    /// An entry that resolves to nothing (a bare `~` with no `HOME`) is dropped
    /// rather than kept as an empty path, which every path on the machine would
    /// otherwise start with, turning the whole disk trusted by accident.
    pub fn resolved_trusted_paths(&self) -> Vec<PathBuf> {
        self.trusted_paths
            .iter()
            .map(|p| expand_home(p))
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    }
}

/// Resolve a leading `~` against `$HOME`, leaving every other path untouched.
fn expand_home(path: &std::path::Path) -> PathBuf {
    let s = path.to_string_lossy();
    let home = || crate::paths::home_string().unwrap_or_default();
    if s == "~" {
        PathBuf::from(home())
    } else if let Some(rest) = s.strip_prefix("~/") {
        let h = home();
        if h.is_empty() {
            PathBuf::new()
        } else {
            PathBuf::from(h).join(rest)
        }
    } else {
        path.to_path_buf()
    }
}
