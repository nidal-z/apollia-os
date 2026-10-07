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
    /// MCP server registry.
    Mcp,
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
    pub const ALL: [DataFile; 20] = [
        DataFile::Agents,
        DataFile::Artifacts,
        DataFile::AuditJournal,
        DataFile::Chat,
        DataFile::Governance,
        DataFile::Hitl,
        DataFile::LlmCalls,
        DataFile::Mailbox,
        DataFile::Mcp,
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
            DataFile::AuditJournal => "audit_journal.db",
            DataFile::Chat => "chat.db",
            DataFile::Governance => "governance.db",
            DataFile::Hitl => "hitl.db",
            DataFile::LlmCalls => "llm_calls.db",
            DataFile::Mailbox => "mailbox.db",
            DataFile::Mcp => "mcp.db",
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

/// Name of the directory, under the data directory, holding one index
/// database per source.
pub const SOURCES_DIR_NAME: &str = "sources";

/// The index database of the source `source_id`:
/// `<data_dir>/sources/<source_id>.db`.
///
/// Like the memory stores, these files are named by data rather than by the
/// layout, so they are not in [`DataFile`]. Returns `None` for an identifier
/// that is empty or holds anything but ASCII letters, digits, `-` and `_`: an
/// identifier must never name a path outside that directory.
#[must_use]
pub fn source_index_path(data_dir: &Path, source_id: &str) -> Option<PathBuf> {
    let safe = !source_id.is_empty()
        && source_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    safe.then(|| {
        data_dir
            .join(SOURCES_DIR_NAME)
            .join(format!("{source_id}.db"))
    })
}

/// The current user's home directory.
///
/// `%USERPROFILE%` on Windows, `$HOME` on Unix. Returns `None` when neither is
/// available, which is a real condition on a stripped environment and must be
/// reported rather than papered over.
pub fn home_dir() -> Option<PathBuf> {
    std::env::home_dir()
}

/// Directories a packaged build may keep a resource in, relative to the
/// executable, most specific first.
///
/// `leaf` is the resource directory name, `runners` or `python`.
///
/// The Linux packagers name their directory after the product, so a .deb and
/// an AppImage install under `/usr/lib/Apollia OS/`, with the capitals and the
/// space. Three call sites guessed `apollia-os` instead and therefore found
/// nothing: the local engine reported "llama-server not found" with the binary
/// sitting in the package, and every agent failed to load because the Python
/// bundle was invisible by the same path. Measured on 2026-09-08 on the built
/// .deb. The name is now discovered rather than guessed.
pub fn bundled_resource_dirs(exe_dir: &Path, leaf: &str) -> Vec<PathBuf> {
    let mut out = vec![
        exe_dir.join(leaf),
        exe_dir.join("../Resources").join(leaf),
        exe_dir.join("../../resources").join(leaf),
    ];
    if let Ok(entries) = std::fs::read_dir(exe_dir.join("../lib")) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name();
            if is_product_dir(&name.to_string_lossy()) {
                out.push(entry.path().join(leaf));
            }
        }
    }
    out
}

/// True when a directory under `lib/` belongs to this product, whatever the
/// packager did to the name: `Apollia OS`, `apollia-os`, `apollia_os`.
fn is_product_dir(name: &str) -> bool {
    let squashed: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    squashed.starts_with("apollia")
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

/// Name of the configuration file inside the data directory.
pub const CONFIG_FILE_NAME: &str = "apollia.toml";

/// The configuration file every surface reads and writes:
/// `<home>/.apollia/apollia.toml`.
///
/// The CLI, the daemon and the desktop used to resolve the file in five
/// different ways (the working directory, `$XDG_CONFIG_HOME/apollia/`,
/// `~/.config/apollia/`, the data directory), so a key written by one tool
/// could be read by no engine. The working directory was also ambiguous: an
/// agent package keeps its manifest under the same name. Only an explicit path
/// given on the command line replaces this location.
///
/// Returns `None` when the home directory cannot be resolved.
pub fn config_file() -> Option<PathBuf> {
    home_dir().map(config_file_under)
}

/// The configuration file under an explicit home.
pub fn config_file_under(home: impl Into<PathBuf>) -> PathBuf {
    data_dir_under(home).join(CONFIG_FILE_NAME)
}

/// Name of the API's Unix socket inside the data directory.
pub const SOCKET_FILE_NAME: &str = "runtime.sock";

/// The API's Unix socket: `<home>/.apollia/runtime.sock`.
///
/// It used to be `/tmp/apollia.sock`. On a shared machine that is a directory
/// every account can write: another user can occupy the name before the runtime
/// starts (the sticky bit then blocks the cleanup and the bind fails), or serve
/// a socket of their own to a CLI that expects the runtime's. Under the data
/// directory the path belongs to the user, and the server additionally sets the
/// file to `0o600` after binding.
///
/// Returns `None` when the home directory cannot be resolved; callers that
/// cannot fail use [`socket_path_or_temp`].
pub fn socket_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join(SOCKET_FILE_NAME))
}

/// The API's named pipe on Windows: `\\.\pipe\apollia-runtime-<user>`.
///
/// Windows has no Unix domain socket the runtime can serve, so the local
/// transport there is a named pipe. It plays the same role the socket plays
/// elsewhere. It does not remove the TCP listener: `apollia-os start` passes a
/// port on every platform, with no `cfg(windows)` guard, and the API server
/// binds it unconditionally, so a workstation does show an open port for the
/// runtime. What guards that port is the token, and a non-loopback bind
/// without one is refused at startup.
///
/// The user name is part of the name so two accounts on one machine, a
/// terminal server among them, do not contend for a single pipe. The name is
/// not a secret and is not access control: unlike the socket, which is served
/// unauthenticated behind `0o600`, the pipe carries the same bearer token the
/// TCP listener carries.
///
/// `USERNAME` absent leaves the bare name, which is correct for a single
/// account and no worse than the port it replaces.
#[cfg(windows)]
pub fn named_pipe_name() -> String {
    pipe_name_for(std::env::var("USERNAME").ok().as_deref())
}

/// The pipe name for a given account name, or the bare name without one.
///
/// Split from [`named_pipe_name`] so the derivation is exercised on every
/// platform rather than only where it runs. The two things that can be wrong,
/// a reserved character in the account name and an empty name, need no Windows
/// to be checked. Everything outside `[A-Za-z0-9]` becomes `-`, because a pipe
/// name may not carry a backslash and an account name may.
///
/// Compiled on Windows, where it is called, and under `cfg(test)`, where the
/// three cases below hold it on every platform the suite runs on.
#[cfg(any(windows, test))]
fn pipe_name_for(user: Option<&str>) -> String {
    match user {
        Some(user) if !user.trim().is_empty() => {
            let safe: String = user
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            format!(r"\\.\pipe\apollia-runtime-{safe}")
        }
        _ => r"\\.\pipe\apollia-runtime".to_owned(),
    }
}
/// The API's Unix socket under an explicit home.
pub fn socket_path_under(home: impl Into<PathBuf>) -> PathBuf {
    data_dir_under(home).join(SOCKET_FILE_NAME)
}

/// The API's Unix socket, falling back to the platform temporary directory.
///
/// For the call sites that compute a default without a `Result` to return it
/// through (a serde default, a clap default). The fallback keeps the runtime
/// startable on a stripped environment; it is not where the socket is meant to
/// live, which is why [`socket_path`] exists next to it.
pub fn socket_path_or_temp() -> PathBuf {
    socket_path().unwrap_or_else(|| std::env::temp_dir().join("apollia.sock"))
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

    // GIVEN an explicit home
    // WHEN the API socket path is resolved under it
    // THEN it sits inside the data directory, not in a shared temporary one
    #[test]
    fn test_socket_path_is_under_the_data_dir() {
        let sock = socket_path_under("/home/tester");
        assert_eq!(
            sock,
            Path::new("/home/tester")
                .join(DATA_DIR_NAME)
                .join("runtime.sock")
        );
        assert!(!sock.starts_with(std::env::temp_dir()));
    }

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

    // GIVEN an explicit home
    // WHEN the configuration file is resolved under it
    // THEN it is `apollia.toml` inside the data directory of that home
    #[test]
    fn test_config_file_under_is_in_the_data_dir() {
        let home = tempfile::tempdir().expect("a temporary home");
        assert_eq!(
            config_file_under(home.path()),
            home.path().join(".apollia").join("apollia.toml")
        );
    }

    // GIVEN a process whose HOME is a temporary directory (a child of this
    //       test binary, so no other test sees the swapped variable)
    // WHEN the configuration file is resolved without an explicit home
    // THEN it is `<temporary home>/.apollia/apollia.toml`
    #[test]
    fn test_config_file_follows_a_temporary_home() {
        const EXPECT: &str = "APOLLIA_TEST_EXPECTED_CONFIG_FILE";
        if let Some(expected) = std::env::var_os(EXPECT) {
            assert_eq!(config_file(), Some(PathBuf::from(expected)));
            return;
        }
        let home = tempfile::tempdir().expect("a temporary home");
        let expected = home.path().join(".apollia").join("apollia.toml");
        let out = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "paths::tests::test_config_file_follows_a_temporary_home",
            ])
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env(EXPECT, &expected)
            .output()
            .expect("the test binary runs");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "child failed: {stdout}");
        assert!(stdout.contains("1 passed"), "child ran no test: {stdout}");
    }

    // GIVEN an ordinary account name
    // WHEN the runtime's pipe name is derived from it
    // THEN the account is part of the name, so two sessions on one machine
    //      do not contend for a single pipe
    #[test]
    fn test_pipe_name_carries_the_account() {
        assert_eq!(pipe_name_for(Some("ada")), r"\\.\pipe\apollia-runtime-ada");
    }

    // GIVEN an account name holding characters a pipe name cannot carry
    // WHEN the pipe name is derived from it
    // THEN every one of them is replaced, so the name stays a single pipe
    //      under `\\.\pipe\` instead of naming a path that does not exist
    #[test]
    fn test_pipe_name_replaces_reserved_characters() {
        assert_eq!(
            pipe_name_for(Some(r"DOMAIN\Jean Dupont")),
            r"\\.\pipe\apollia-runtime-DOMAIN-Jean-Dupont"
        );
    }

    // GIVEN no account name, or one that is blank
    // WHEN the pipe name is derived
    // THEN the bare name answers, which serves a single account correctly
    //      rather than producing a name ending on a dangling separator
    #[test]
    fn test_pipe_name_without_an_account_stays_bare() {
        let bare = r"\\.\pipe\apollia-runtime";
        assert_eq!(pipe_name_for(None), bare);
        assert_eq!(pipe_name_for(Some("")), bare);
        assert_eq!(pipe_name_for(Some("   ")), bare);
    }
    #[test]
    fn test_a_source_index_sits_under_the_sources_directory() {
        // GIVEN a data directory and a registry identifier
        let data = Path::new("/home/tester/.apollia");

        // WHEN the index path of the source is resolved
        let path = source_index_path(data, "src-0a1b2c");

        // THEN it is `<data>/sources/<id>.db`
        assert_eq!(path, Some(data.join("sources").join("src-0a1b2c.db")));
    }

    #[test]
    fn test_a_source_identifier_never_names_a_path_outside() {
        // GIVEN identifiers that are empty or carry separators and dots
        let data = Path::new("/home/tester/.apollia");
        let hostile = ["", "..", "../system", "a/b", "a\\b", "a.b"];

        // WHEN their index paths are resolved
        let resolved: Vec<_> = hostile
            .iter()
            .map(|id| source_index_path(data, id))
            .collect();

        // THEN none of them resolves
        assert!(resolved.iter().all(Option::is_none), "{resolved:?}");
    }
}

#[cfg(test)]
mod bundled_resource_tests {
    use super::{bundled_resource_dirs, is_product_dir};
    use std::path::Path;

    #[test]
    fn a_product_directory_is_recognised_however_it_was_written() {
        // GIVEN the three spellings packagers produce for the same product
        // WHEN each is examined
        // THEN all three are recognised, and a neighbour is not
        assert!(is_product_dir("Apollia OS"));
        assert!(is_product_dir("apollia-os"));
        assert!(is_product_dir("apollia_os"));
        assert!(!is_product_dir("systemd"));
    }

    #[test]
    fn the_linux_install_layout_is_among_the_candidates() {
        // GIVEN the layout a .deb produces: the binary in usr/bin, the
        // resources under usr/lib/Apollia OS/
        let root = tempfile::tempdir().expect("a temporary root");
        let bin = root.path().join("usr/bin");
        let res = root.path().join("usr/lib/Apollia OS/runners");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::create_dir_all(&res).expect("resources");

        // WHEN the candidates are listed from the executable's directory
        let candidates = bundled_resource_dirs(&bin, "runners");

        // THEN the directory the packager actually created is one of them,
        // which the hardcoded `apollia-os` spelling never was
        assert!(
            candidates
                .iter()
                .any(|c| c.canonicalize().ok() == res.canonicalize().ok()),
            "candidates were {candidates:?}"
        );
    }

    #[test]
    fn a_directory_that_is_not_ours_is_left_alone() {
        // GIVEN a lib directory holding somebody else's product
        let root = tempfile::tempdir().expect("a temporary root");
        std::fs::create_dir_all(root.path().join("usr/bin")).expect("bin");
        std::fs::create_dir_all(root.path().join("usr/lib/other-app/runners")).expect("other");

        // WHEN the candidates are listed
        let candidates = bundled_resource_dirs(&root.path().join("usr/bin"), "runners");

        // THEN the neighbour is not probed
        assert!(
            !candidates
                .iter()
                .any(|c| c.to_string_lossy().contains("other-app")),
            "candidates were {candidates:?}"
        );
    }

    #[test]
    fn the_macos_layout_is_still_first_in_line() {
        // GIVEN the macOS bundle layout, Contents/MacOS beside Contents/Resources
        let exe_dir = Path::new("/Applications/Apollia OS.app/Contents/MacOS");

        // WHEN the candidates are listed
        let candidates = bundled_resource_dirs(exe_dir, "python");

        // THEN the Resources path is among them: `../Resources` is kept as
        // written rather than normalised, which is what the resolver hands to
        // the filesystem.
        assert!(
            candidates.iter().any(|c| c
                .to_string_lossy()
                .contains("Contents/MacOS/../Resources/python")),
            "candidates were {candidates:?}"
        );
    }
}
