//! The commands that reset or erase what an installation holds: the
//! onboarding flag, the onboarding itself, every memory database, the log
//! files, and the factory reset.
//!
//! Each function acts on the data directory of the runtime that calls it. The
//! desktop running the runtime in its own process passes its own; a client of
//! the daemon reaches the same functions through the routes of
//! [`crate::api::routes_app_lifecycle`], which pass the daemon's. Both modes
//! therefore erase the same files and refuse with the same words.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollia_memory::user_memory::UserMemoryError;

use crate::profile::ProfileStore;

/// The internal markers the onboarding flow keeps under the `__` prefix of
/// the user namespace. A reset removes each of them.
const ONBOARDING_MARKERS: &[&str] = &[
    "onboarding_phase",
    "onboarding_profile",
    "onboarding_llm_configured",
    "onboarding_stt_configured",
    "onboarding_topics_covered",
    "onboarding_mandatory_complete",
    "onboarding_tour_step_index",
    "onboarding_tour_total_steps",
    "onboarding_tour_completed",
    "onboarding_companion_session_id",
    "onboarding_voice_enabled",
    "onboarding_skipped",
    "onboarding_completed",
    "onboarding_started_at",
    "onboarding_completed_at",
    "onboarding_stats_total_time_sec",
    "onboarding_stats_actions_completed",
    "onboarding_stats_companion_questions",
    // Legacy key: the guided-tour voice path is gone, but installations
    // created before its removal still hold this entry.
    "onboarding_stats_voice_commands_used",
];

/// Why a lifecycle operation did not complete. The display text is what the
/// interface shows, the same whichever process ran the operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LifecycleError {
    /// A file or directory could not be created, listed or removed.
    #[error("failed to {action}: {source}")]
    Io {
        /// What was being done, in the words the interface shows.
        action: &'static str,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// `user_memory.db` did not open at startup.
    #[error("user memory repository not initialized")]
    ProfileUnavailable,
    /// The profile repository refused the purge.
    #[error("failed to reset user profile during reset_onboarding: {0}")]
    Profile(#[source] UserMemoryError),
    /// A writer panicked while holding the profile repository.
    #[error("mutex poisoned")]
    Poisoned,
    /// The blocking task did not complete.
    #[error("spawn_blocking failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}

fn io(action: &'static str) -> impl FnOnce(std::io::Error) -> LifecycleError {
    move |source| LifecycleError::Io { action, source }
}

/// The flag file whose presence says the onboarding is complete.
pub fn onboarded_flag(data_dir: &Path) -> PathBuf {
    data_dir.join(".onboarded")
}

/// Marks the onboarding as complete by creating the flag file, and the data
/// directory when it is missing.
///
/// # Errors
///
/// [`LifecycleError::Io`] when the directory or the flag cannot be written.
pub async fn mark_onboarded(data_dir: &Path) -> Result<(), LifecycleError> {
    let flag = onboarded_flag(data_dir);
    if let Some(parent) = flag.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(io("create directory"))?;
    }
    tokio::fs::write(&flag, "")
        .await
        .map_err(io("write onboarding flag"))
}

/// Resets the onboarding so the journey starts from scratch: removes the
/// completion flag, the flow's internal markers, every entry of the visible
/// profile, and the onboarding agent's own memory database.
///
/// Returns how many profile entries were removed.
///
/// # Errors
///
/// [`LifecycleError::ProfileUnavailable`] when `user_memory` is `None`,
/// [`LifecycleError::Io`] when the flag cannot be removed, and
/// [`LifecycleError::Profile`] when the repository refuses the purge.
pub async fn reset_onboarding(
    data_dir: &Path,
    user_memory: Option<ProfileStore<'_>>,
) -> Result<usize, LifecycleError> {
    let flag = onboarded_flag(data_dir);
    if flag.exists() {
        tokio::fs::remove_file(&flag)
            .await
            .map_err(io("remove onboarding flag"))?;
    }

    let repo = user_memory
        .map(|store| Arc::clone(store.0))
        .ok_or(LifecycleError::ProfileUnavailable)?;
    let removed = tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|_| LifecycleError::Poisoned)?;
        for key in ONBOARDING_MARKERS {
            if let Err(e) = repo.forget_internal(key) {
                tracing::warn!(key = %key, error = %e, "onboarding.marker.wipe.failed");
            }
        }
        repo.reset().map_err(LifecycleError::Profile)
    })
    .await
    .map_err(LifecycleError::Join)??;
    tracing::info!(
        removed_profile_entries = removed,
        "onboarding.profile.wiped"
    );

    // The onboarding agent's memory holds the dialogue episodes and the
    // `onboarding.completed_at` keys that otherwise keep the journey from
    // restarting cleanly.
    let onboarding_db = data_dir.join("memory").join("onboarding.db");
    if onboarding_db.exists() {
        if let Err(e) = tokio::fs::remove_file(&onboarding_db).await {
            tracing::warn!(
                path = %onboarding_db.display(),
                error = %e,
                "onboarding.store.remove.failed"
            );
        }
        for side in ["onboarding.db-wal", "onboarding.db-shm"] {
            let side = onboarding_db.with_file_name(side);
            if side.exists() {
                if let Err(e) = tokio::fs::remove_file(&side).await {
                    tracing::warn!(
                        path = %side.display(),
                        error = %e,
                        "onboarding.store.remove.failed"
                    );
                }
            }
        }
    }

    Ok(removed)
}

/// Removes every file of the memory directory, whatever its namespace (user
/// profile, agent memories, projects). Subdirectories are left in place.
/// Irreversible.
///
/// Returns how many `.db` files were removed.
///
/// # Errors
///
/// [`LifecycleError::Io`] when the directory cannot be listed.
pub async fn clear_all_memories(data_dir: &Path) -> Result<usize, LifecycleError> {
    let memory_dir = data_dir.join("memory");
    if !memory_dir.exists() {
        return Ok(0);
    }

    let mut entries = tokio::fs::read_dir(&memory_dir)
        .await
        .map_err(io("list memory dir"))?;
    let mut removed_db_count = 0usize;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(io("iterate memory dir"))?
    {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        if let Err(e) = tokio::fs::remove_file(&path).await {
            tracing::warn!(path = %path.display(), error = %e, "memory.wipe.file.skipped");
            continue;
        }
        if name.ends_with(".db") {
            removed_db_count += 1;
        }
    }

    tracing::info!(removed_db_count, "memory.wipe.completed");
    Ok(removed_db_count)
}

/// Removes the logs directory. A missing directory is not an error.
///
/// Returns whether a directory was removed.
///
/// # Errors
///
/// [`LifecycleError::Io`] when the directory cannot be removed.
pub async fn clear_logs(data_dir: &Path) -> Result<bool, LifecycleError> {
    let logs = data_dir.join("logs");
    if !logs.exists() {
        return Ok(false);
    }
    tokio::fs::remove_dir_all(&logs)
        .await
        .map_err(io("remove logs directory"))?;
    Ok(true)
}

/// Factory reset: removes the whole data directory (configuration, memory,
/// sessions, logs, models, credentials). Irreversible; the application is
/// restarted afterwards.
///
/// On Windows the directory cannot be removed by the process that runs in it:
/// SQLite opens its databases without `FILE_SHARE_DELETE`, the log file and
/// the model an engine has mapped stay open, so the removal stops on the first
/// held file with "os error 32" and leaves a half-erased directory behind.
/// There the reset is recorded instead, and the next start carries it out
/// before anything opens a file ([`apply_pending_factory_reset`]).
///
/// Returns whether a directory was removed, or on Windows marked for removal.
///
/// # Errors
///
/// [`LifecycleError::Io`] when the directory cannot be removed, or on Windows
/// when the request cannot be written.
pub async fn factory_reset(data_dir: &Path) -> Result<bool, LifecycleError> {
    if !data_dir.exists() {
        return Ok(false);
    }
    if cfg!(windows) {
        schedule_factory_reset(data_dir).await?;
        tracing::warn!(path = %data_dir.display(), "lifecycle.factory_reset.scheduled");
        return Ok(true);
    }
    tokio::fs::remove_dir_all(data_dir)
        .await
        .map_err(io("remove apollia home"))?;
    tracing::warn!(path = %data_dir.display(), "lifecycle.factory_reset.completed");
    Ok(true)
}

/// The file whose presence asks the next start to erase the data directory.
const FACTORY_RESET_REQUEST: &str = ".factory-reset-pending";

/// Appended to the name of the data directory while it is being erased.
const ERASING_SUFFIX: &str = ".factory-reset";

/// How long a start waits for the process it replaces to let go of the data
/// directory. A restart launches the new process before the old one has
/// exited, and the old one's engines end with it.
pub const PENDING_RESET_WAIT: Duration = Duration::from_secs(10);

/// Pause between two attempts while that wait lasts.
const PENDING_RESET_RETRY: Duration = Duration::from_millis(250);

/// Records that the data directory is to be erased at the next start.
///
/// # Errors
///
/// [`LifecycleError::Io`] when the request cannot be written.
pub async fn schedule_factory_reset(data_dir: &Path) -> Result<(), LifecycleError> {
    tokio::fs::write(data_dir.join(FACTORY_RESET_REQUEST), "")
        .await
        .map_err(io("record the factory reset"))
}

/// What a start found and did about a recorded factory reset.
#[derive(Debug)]
#[non_exhaustive]
pub enum PendingReset {
    /// No reset was recorded.
    None,
    /// The data directory was erased. `left_behind` names why its renamed
    /// copy could not be removed in full; the next start tries again.
    Applied {
        /// The removal error of the renamed copy, if any.
        left_behind: Option<std::io::Error>,
    },
    /// Another process still holds the directory: nothing was touched and the
    /// request stays for the next start.
    Deferred(std::io::Error),
}

/// Carries out a factory reset recorded by [`factory_reset`], before this
/// process opens anything under `data_dir`.
///
/// The directory is first renamed, then removed. Windows refuses to rename a
/// directory while any process holds a file inside it, so the rename is the
/// test that nothing does: it either moves the whole directory or touches
/// nothing, and a held directory is never left half-erased. A copy a previous
/// start could not finish removing is removed here too.
///
/// Blocking: called at startup, before the async runtime, or through
/// `spawn_blocking`.
pub fn apply_pending_factory_reset(data_dir: &Path, wait: Duration) -> PendingReset {
    apply_pending_with(data_dir, wait, &|from, to| std::fs::rename(from, to))
}

fn apply_pending_with(
    data_dir: &Path,
    wait: Duration,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> PendingReset {
    let Some(erasing) = erasing_path(data_dir) else {
        return PendingReset::None;
    };
    // A copy that still resists is tried again at the next start; the rename
    // below then fails and the request is deferred rather than lost.
    let _ = remove_erasing(&erasing);
    if !data_dir.join(FACTORY_RESET_REQUEST).is_file() {
        return PendingReset::None;
    }
    let deadline = Instant::now() + wait;
    loop {
        match rename(data_dir, &erasing) {
            Ok(()) => break,
            Err(e) if Instant::now() >= deadline => return PendingReset::Deferred(e),
            Err(_) => std::thread::sleep(PENDING_RESET_RETRY),
        }
    }
    PendingReset::Applied {
        left_behind: remove_erasing(&erasing),
    }
}

/// The sibling the data directory is renamed to while it is erased.
fn erasing_path(data_dir: &Path) -> Option<PathBuf> {
    let mut name = data_dir.file_name()?.to_os_string();
    name.push(ERASING_SUFFIX);
    Some(data_dir.with_file_name(name))
}

/// Removes the renamed copy when there is one; answers why it could not be.
fn remove_erasing(erasing: &Path) -> Option<std::io::Error> {
    if !erasing.exists() {
        return None;
    }
    std::fs::remove_dir_all(erasing).err()
}

/// Traces what [`apply_pending_factory_reset`] did, once the subscriber is up.
pub fn log_pending_reset(outcome: &PendingReset, data_dir: &Path) {
    match outcome {
        PendingReset::None => {}
        PendingReset::Applied { left_behind: None } => {
            tracing::warn!(path = %data_dir.display(), "lifecycle.factory_reset.applied");
        }
        PendingReset::Applied {
            left_behind: Some(e),
        } => {
            tracing::warn!(
                path = %data_dir.display(),
                error = %e,
                detail = "the renamed copy is removed at the next start",
                "lifecycle.factory_reset.applied_with_leftover"
            );
        }
        PendingReset::Deferred(e) => {
            tracing::error!(
                path = %data_dir.display(),
                error = %e,
                detail = "the directory is still held; the reset stays recorded",
                "lifecycle.factory_reset.deferred"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mark_onboarded_creates_the_flag_and_its_directory() {
        // GIVEN a data directory that does not exist yet
        let dir = tempfile::TempDir::new().unwrap();
        let data_dir = dir.path().join(".apollia");

        // WHEN the onboarding is marked complete
        mark_onboarded(&data_dir).await.unwrap();

        // THEN the flag file is there
        assert!(data_dir.join(".onboarded").is_file());
    }

    #[tokio::test]
    async fn test_reset_onboarding_without_the_profile_store_is_refused_after_the_flag() {
        // GIVEN a completed onboarding and no profile store
        let dir = tempfile::TempDir::new().unwrap();
        mark_onboarded(dir.path()).await.unwrap();

        // WHEN the onboarding is reset
        let error = reset_onboarding(dir.path(), None).await.unwrap_err();

        // THEN the refusal names the store, and the flag is already gone
        assert_eq!(error.to_string(), "user memory repository not initialized");
        assert!(!dir.path().join(".onboarded").exists());
    }

    #[tokio::test]
    async fn test_clear_all_memories_counts_the_databases_and_keeps_subdirectories() {
        // GIVEN a memory directory with two databases, a side file and a
        // subdirectory
        let dir = tempfile::TempDir::new().unwrap();
        let memory = dir.path().join("memory");
        std::fs::create_dir_all(memory.join("keep")).unwrap();
        for name in ["a.db", "b.db", "b.db-wal"] {
            std::fs::write(memory.join(name), "x").unwrap();
        }

        // WHEN every memory is cleared
        let removed = clear_all_memories(dir.path()).await.unwrap();

        // THEN the two databases are counted, every file is gone, the
        // subdirectory stays
        assert_eq!(removed, 2);
        assert!(!memory.join("b.db-wal").exists());
        assert!(memory.join("keep").is_dir());
    }

    /// A data directory holding a configuration file and a database.
    fn populated_data_dir(root: &Path) -> PathBuf {
        let data_dir = root.join(".apollia");
        std::fs::create_dir_all(data_dir.join("memory")).unwrap();
        std::fs::write(data_dir.join("apollia.toml"), "x").unwrap();
        std::fs::write(data_dir.join("memory").join("a.db"), "x").unwrap();
        data_dir
    }

    #[test]
    fn test_apply_pending_factory_reset_without_a_request_touches_nothing() {
        // GIVEN a data directory with no recorded reset
        let root = tempfile::TempDir::new().unwrap();
        let data_dir = populated_data_dir(root.path());

        // WHEN a start looks for a pending reset
        let outcome = apply_pending_factory_reset(&data_dir, Duration::ZERO);

        // THEN nothing is done and the files stay
        assert!(matches!(outcome, PendingReset::None));
        assert!(data_dir.join("apollia.toml").is_file());
    }

    #[tokio::test]
    async fn test_apply_pending_factory_reset_erases_a_recorded_directory() {
        // GIVEN a data directory whose reset was recorded
        let root = tempfile::TempDir::new().unwrap();
        let data_dir = populated_data_dir(root.path());
        schedule_factory_reset(&data_dir).await.unwrap();

        // WHEN the next start carries it out
        let outcome = apply_pending_factory_reset(&data_dir, Duration::ZERO);

        // THEN the directory and its renamed copy are both gone
        assert!(matches!(
            outcome,
            PendingReset::Applied { left_behind: None }
        ));
        assert!(!data_dir.exists());
        assert!(!root.path().join(".apollia.factory-reset").exists());
    }

    #[tokio::test]
    async fn test_apply_pending_factory_reset_on_a_held_directory_touches_nothing() {
        // GIVEN a recorded reset on a directory another process holds, which
        // Windows reports by refusing the rename
        let root = tempfile::TempDir::new().unwrap();
        let data_dir = populated_data_dir(root.path());
        schedule_factory_reset(&data_dir).await.unwrap();
        let held = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        };

        // WHEN the start tries to carry it out
        let outcome = apply_pending_with(&data_dir, Duration::ZERO, &held);

        // THEN it is deferred, every file stays, and the request is kept
        assert!(matches!(outcome, PendingReset::Deferred(_)));
        assert!(data_dir.join("apollia.toml").is_file());
        assert!(data_dir.join("memory").join("a.db").is_file());
        assert!(data_dir.join(FACTORY_RESET_REQUEST).is_file());
    }

    #[test]
    fn test_apply_pending_factory_reset_removes_a_copy_left_by_a_previous_start() {
        // GIVEN no recorded reset, and the renamed copy a previous start
        // could not finish removing
        let root = tempfile::TempDir::new().unwrap();
        let data_dir = populated_data_dir(root.path());
        let leftover = root.path().join(".apollia.factory-reset");
        std::fs::create_dir_all(leftover.join("models")).unwrap();

        // WHEN a start looks for a pending reset
        let outcome = apply_pending_factory_reset(&data_dir, Duration::ZERO);

        // THEN the copy is removed and the live directory is left alone
        assert!(matches!(outcome, PendingReset::None));
        assert!(!leftover.exists());
        assert!(data_dir.join("apollia.toml").is_file());
    }

    #[tokio::test]
    async fn test_factory_reset_of_a_missing_directory_removes_nothing() {
        // GIVEN no data directory
        let root = tempfile::TempDir::new().unwrap();

        // WHEN a factory reset is asked
        let removed = factory_reset(&root.path().join(".apollia")).await.unwrap();

        // THEN nothing was removed or recorded
        assert!(!removed);
    }
}
