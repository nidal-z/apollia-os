//! Platform home and data directory resolution.
//!
//! One place, so a platform difference is fixed once rather than in every caller.
//!
//! # Why this exists
//!
//! The codebase used to read `std::env::var("HOME")` directly, in 98 places,
//! and 27 of those fell back to a literal `"/tmp"` when the variable was absent.
//! That is wrong twice over. On Windows `HOME` is simply not set by default, so
//! the profile would land in a directory named `/tmp` relative to the current
//! drive; and even on Unix, silently writing user state to a world-writable
//! temporary directory when the environment is unusual is worse than failing.
//!
//! [`std::env::home_dir`] resolves `%USERPROFILE%` on Windows and `$HOME` on
//! Unix, and no longer carries the historical Windows defect that had it
//! deprecated. Callers that genuinely cannot proceed without a home directory
//! should surface the `None`, not invent a path.

use std::path::{Path, PathBuf};

/// Name of the runtime's directory inside the user's home.
pub const DATA_DIR_NAME: &str = ".apollia";

/// Name of the legacy permission store, kept only so [`DataFile::Governance`]
/// openers can migrate it. Nothing creates a file under this name any more.
pub const LEGACY_PERMISSIONS_DB_NAME: &str = "permissions.db";

/// One database at the root of the data directory.
///
/// This catalogue is the single source for the layout of the databases under
/// `~/.apollia`. Every module resolves its database file through it, the CLI
/// seed fixture keeps one schema per entry, and `scripts/check_data_layout.py`
/// refuses a database-name literal outside this file. A new database starts by
/// adding a variant here; the guard then walks the seed into agreement.
///
/// Per-namespace memory stores (`memory/<namespace>.db`) are the one family
/// not listed: their names are data, not layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DataFile {
    /// Installed-agents inventory.
    Agents,
    /// Chat artifacts produced by sessions.
    Artifacts,
    /// Tool-invocation audit log.
    Audit,
    /// Hash-chained audit journal.
    AuditJournal,
    /// Chat sessions, messages and approvals.
    Chat,
    /// Consolidated tool and permission governance store.
    Governance,
    /// Human-in-the-loop tasks and notification log.
    Hitl,
    /// Per-call LLM usage log.
    LlmCalls,
    /// Inter-agent mailbox.
    Mailbox,
    /// MCP server registry and approvals cache.
    Mcp,
    /// MCP tool-approval decisions.
    McpApprovals,
    /// Notification channels and delivery log.
    Notifications,
    /// ORIA plan cache.
    PlanCache,
    /// Execution plans and steps.
    Plans,
    /// Projects, documents and providers.
    Projects,
    /// Persisted runtime events (observability).
    RuntimeEvents,
    /// A2A task sidechains.
    Sidechains,
    /// Dictation transcription history.
    SttTranscriptions,
    /// System-wide settings (LLM backends, STT configuration).
    System,
    /// Filesystem and cron trigger state.
    Triggers,
    /// Trigger definitions.
    TriggersDef,
    /// User-level memory store.
    UserMemory,
}

impl DataFile {
    /// Every root database, one entry per file the product creates.
    ///
    /// The seed fixture (`tests/cli/seed/schemas/`) carries one schema per
    /// entry, and `scripts/check_data_layout.py` holds the two lists equal.
    pub const ALL: [DataFile; 22] = [
        DataFile::Agents,
        DataFile::Artifacts,
        DataFile::Audit,
        DataFile::AuditJournal,
        DataFile::Chat,
        DataFile::Governance,
        DataFile::Hitl,
        DataFile::LlmCalls,
        DataFile::Mailbox,
        DataFile::Mcp,
        DataFile::McpApprovals,
        DataFile::Notifications,
        DataFile::PlanCache,
        DataFile::Plans,
        DataFile::Projects,
        DataFile::RuntimeEvents,
        DataFile::Sidechains,
        DataFile::SttTranscriptions,
        DataFile::System,
        DataFile::Triggers,
        DataFile::TriggersDef,
        DataFile::UserMemory,
    ];

    /// Base name of the database file at the root of the data directory.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            DataFile::Agents => "agents.db",
            DataFile::Artifacts => "artifacts.db",
            DataFile::Audit => "audit.db",
            DataFile::AuditJournal => "audit_journal.db",
            DataFile::Chat => "chat.db",
            DataFile::Governance => "governance.db",
            DataFile::Hitl => "hitl.db",
            DataFile::LlmCalls => "llm_calls.db",
            DataFile::Mailbox => "mailbox.db",
            DataFile::Mcp => "mcp.db",
            DataFile::McpApprovals => "mcp_approvals.db",
            DataFile::Notifications => "notifications.db",
            DataFile::PlanCache => "plan_cache.db",
            DataFile::Plans => "plans.db",
            DataFile::Projects => "projects.db",
            DataFile::RuntimeEvents => "runtime_events.db",
            DataFile::Sidechains => "sidechains.db",
            DataFile::SttTranscriptions => "stt_transcriptions.db",
            DataFile::System => "system.db",
            DataFile::Triggers => "triggers.db",
            DataFile::TriggersDef => "triggers_def.db",
            DataFile::UserMemory => "user_memory.db",
        }
    }

    /// Path of this database under the given data directory root.
    #[must_use]
    pub fn path(self, data_dir: &Path) -> PathBuf {
        data_dir.join(self.file_name())
    }
}

/// The current user's home directory.
///
/// `%USERPROFILE%` on Windows, `$HOME` on Unix. Returns `None` when neither is
/// available, which is a real condition on a stripped environment and must be
/// reported rather than papered over.
pub fn home_dir() -> Option<PathBuf> {
    std::env::home_dir()
}

/// The runtime data directory: `<home>/.apollia`.
///
/// Holds the API token, the SQLite databases, models and configuration.
/// Returns `None` when the home directory cannot be resolved.
pub fn data_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(DATA_DIR_NAME))
}

/// The runtime data directory under an explicit home.
///
/// For call sites that carry their own home (a `$HOME` override, a seeded
/// profile, a test), so the directory name is composed in one place rather
/// than by each caller.
pub fn data_dir_under(home: impl Into<PathBuf>) -> PathBuf {
    home.into().join(DATA_DIR_NAME)
}

/// The home directory, falling back to the platform temporary directory.
///
/// For the call sites that cannot return an error. Prefer [`data_dir_or_err`]
/// wherever the signature allows it: a temporary directory is a place to fail
/// visibly, not a place to keep user state.
///
/// The point of routing the fallback through [`std::env::temp_dir`] rather than
/// a literal `"/tmp"` is that it resolves correctly on every platform, instead
/// of creating a directory named `/tmp` on the current drive under Windows.
pub fn home_dir_or_temp() -> PathBuf {
    home_dir().unwrap_or_else(std::env::temp_dir)
}

/// The home directory as a `String`, for call sites that compose paths by string.
///
/// Prefer [`home_dir`] and `PathBuf::join`: string composition loses the
/// platform separator and breaks on non-UTF-8 paths. This exists to migrate the
/// call sites that already worked that way, not as the recommended shape.
pub fn home_string() -> Option<String> {
    home_dir().map(|p| p.display().to_string())
}

/// The home directory as a `String`, or an error message naming the cause.
pub fn home_string_or_err() -> Result<String, String> {
    home_string().ok_or_else(|| {
        "cannot resolve the home directory (USERPROFILE on Windows, HOME on Unix)".to_string()
    })
}

/// The runtime data directory, or an error message naming the cause.
///
/// For call sites that already return a `Result<_, String>` and want the reason
/// to reach the user instead of a silent fallback.
pub fn data_dir_or_err() -> Result<PathBuf, String> {
    data_dir().ok_or_else(|| {
        "cannot resolve the home directory (USERPROFILE on Windows, HOME on Unix)".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a normal environment
    // WHEN the data directory is resolved
    // THEN it sits under the home directory and carries the runtime's name,
    //      with no temporary-directory fallback anywhere in the result
    #[test]
    fn test_data_dir_is_under_home_and_named() {
        let home = home_dir().expect("a test host has a home directory");
        let data = data_dir().expect("data dir follows from home");
        assert!(data.starts_with(&home), "{data:?} should be under {home:?}");
        assert_eq!(
            data.file_name().and_then(|s| s.to_str()),
            Some(DATA_DIR_NAME)
        );
    }

    // GIVEN the fallible accessor
    // WHEN the home directory resolves
    // THEN it agrees with the optional accessor, so the two cannot drift
    #[test]
    fn test_fallible_accessor_agrees_with_optional_one() {
        assert_eq!(data_dir_or_err().ok(), data_dir());
    }

    // GIVEN the catalogue of root databases
    // WHEN collecting every file name
    // THEN each name is distinct, ends in .db, and is a bare base name,
    //      so two variants cannot silently share a file
    #[test]
    fn test_catalogue_names_are_distinct_flat_db_files() {
        let names: std::collections::BTreeSet<&str> =
            DataFile::ALL.iter().map(|f| f.file_name()).collect();
        assert_eq!(names.len(), DataFile::ALL.len());
        for name in names {
            assert!(name.ends_with(".db"), "{name} should end with .db");
            assert!(!name.contains('/'), "{name} should be a base name");
        }
    }

    // GIVEN a data directory root
    // WHEN resolving a catalogue entry
    // THEN the path is the root joined with the entry's file name
    #[test]
    fn test_catalogue_path_joins_root_and_file_name() {
        let root = PathBuf::from("/data/root");
        assert_eq!(
            DataFile::Chat.path(&root),
            PathBuf::from("/data/root/chat.db")
        );
    }

    // GIVEN an explicit home directory
    // WHEN composing the data directory under it
    // THEN the result carries the runtime's directory name under that home
    #[test]
    fn test_data_dir_under_composes_home_and_name() {
        let home = PathBuf::from("/somewhere/home");
        assert_eq!(
            data_dir_under(home),
            PathBuf::from("/somewhere/home").join(DATA_DIR_NAME)
        );
    }
}
