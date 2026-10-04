use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// FilesystemConfig
// ─────────────────────────────────────────────

/// Agent filesystem configuration (`[filesystem]` section in `apollia.toml`).
///
/// Holds the paths the operator trusts an agent to work in without being asked
/// each time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemConfig {
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
            trusted_paths: default_trusted_paths(),
        }
    }
}

fn default_trusted_paths() -> Vec<PathBuf> {
    vec![PathBuf::from("~")]
}

impl FilesystemConfig {
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
