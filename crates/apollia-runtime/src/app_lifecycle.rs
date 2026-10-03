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
/// Returns whether a directory was removed.
///
/// # Errors
///
/// [`LifecycleError::Io`] when the directory cannot be removed.
pub async fn factory_reset(data_dir: &Path) -> Result<bool, LifecycleError> {
    if !data_dir.exists() {
        return Ok(false);
    }
    tokio::fs::remove_dir_all(data_dir)
        .await
        .map_err(io("remove apollia home"))?;
    tracing::warn!(path = %data_dir.display(), "lifecycle.factory_reset.completed");
    Ok(true)
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
}
