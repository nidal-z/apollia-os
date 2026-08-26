//! Tauri IPC commands that reset or end a session: the onboarding flag, the
//! stored memories, the log files, the factory reset, and the two application
//! lifecycle commands the webview calls instead of closing its own window.

use std::path::PathBuf;

/// Resolves the onboarding flag path `~/.apollia/.onboarded`.
fn onboarded_flag_path() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home).join(".onboarded")
}

/// Marks onboarding as complete by creating the flag file.
///
/// Creates `~/.apollia/.onboarded` (and the parent directory if needed).
#[tauri::command]
pub async fn mark_onboarded() -> Result<(), String> {
    let flag_path = onboarded_flag_path();

    if let Some(parent) = flag_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create directory: {e}"))?;
    }

    tokio::fs::write(&flag_path, "")
        .await
        .map_err(|e| format!("failed to write onboarding flag: {e}"))
}

/// Fully resets onboarding: removes the completion flag, purges the desktop
/// flow's internal state markers, and purges the visible user profile (Tier 1
/// + extras) so the journey starts from scratch.
///
/// Details:
/// - UI markers (`onboarding_phase`, `onboarding_completed_at`, etc.) are
///   stored under the `__` prefix in `__user__`, so they are wiped via
///   `forget_internal`.
/// - The profile's Tier 1 facts are flat keys (`name`, `role`,
///   `agents.hitl`, `constraints.sovereignty`), so a plain `repo.reset()`
///   removes them all, including any extras.
#[tauri::command]
pub async fn reset_onboarding(
    state: tauri::State<'_, apollia_runtime::embedded::RuntimeHandle>,
) -> Result<(), String> {
    let flag_path = onboarded_flag_path();

    if flag_path.exists() {
        tokio::fs::remove_file(&flag_path)
            .await
            .map_err(|e| format!("failed to remove onboarding flag: {e}"))?;
    }

    let repo = state
        .user_memory
        .as_ref()
        .cloned()
        .ok_or_else(|| "user memory repository not initialized".to_string())?;

    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;

        // 1. Internal onboarding-flow markers (`__` prefix).
        let internal_keys = [
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
        for key in &internal_keys {
            if let Err(e) = repo.forget_internal(key) {
                tracing::warn!(
                    key = %key,
                    error = %e,
                    "onboarding.marker.wipe.failed"
                );
            }
        }

        // 2. Full purge of the visible user profile (Tier 1 + extras).
        //    The onboarding journey will repopulate Tier 1 on next launch.
        let removed = repo
            .reset()
            .map_err(|e| format!("failed to reset user profile during reset_onboarding: {e}"))?;
        tracing::info!(
            removed_profile_entries = removed,
            "onboarding.profile.wiped"
        );

        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    // 3. Purge the memory database owned by the onboarding-agent (`onboarding.db`).
    //    It holds the dialogue episodes and `onboarding.completed_at` keys that
    //    otherwise prevent the journey from restarting cleanly.
    let onboarding_db =
        apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp())
            .join("memory")
            .join("onboarding.db");
    if onboarding_db.exists() {
        if let Err(e) = tokio::fs::remove_file(&onboarding_db).await {
            tracing::warn!(
                path = %onboarding_db.display(),
                error = %e,
                "onboarding.store.remove.failed"
            );
        }
        // WAL side-files; ignore errors.
        for ext in ["onboarding.db-wal", "onboarding.db-shm"] {
            let side = onboarding_db.with_file_name(ext);
            if side.exists() {
                let _ = tokio::fs::remove_file(&side).await;
            }
        }
    }

    Ok(())
}

/// Removes **all** of Apollia's memory databases.
///
/// Hard wipe: removes every `*.db` file (and their WAL/SHM side-files) from the
/// `~/.apollia/memory/` directory, regardless of namespace (user profile, agent
/// memories, projects, etc.). Irreversible action.
///
/// Returns the number of `.db` files removed.
#[tauri::command]
pub async fn clear_all_memories() -> Result<usize, String> {
    let memory_dir = apollia_home().join("memory");
    if !memory_dir.exists() {
        return Ok(0);
    }

    let mut entries = tokio::fs::read_dir(&memory_dir)
        .await
        .map_err(|e| format!("failed to list memory dir: {e}"))?;

    let mut removed_db_count = 0usize;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("read_dir iteration failed: {e}"))?
    {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Broad removal: every file (db, wal, shm, possible backups) in the
        // memory directory is removed. Subdirectories are left untouched.
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

/// Resolves `~/.apollia/`.
fn apollia_home() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home)
}

/// Removes the logs directory (`~/.apollia/logs/`).
///
/// If the directory does not exist, the command is a no-op and returns `Ok(())`.
#[tauri::command]
pub async fn clear_logs() -> Result<(), String> {
    let logs = apollia_home().join("logs");
    if logs.exists() {
        tokio::fs::remove_dir_all(&logs)
            .await
            .map_err(|e| format!("failed to remove logs directory: {e}"))?;
    }
    Ok(())
}

/// Factory reset: removes **all** of `~/.apollia/`'s content.
///
/// Irreversible destructive action: removes config, memory, sessions, logs,
/// models and credentials. The user is prompted to restart after the call.
#[tauri::command]
pub async fn factory_reset() -> Result<(), String> {
    let home = apollia_home();
    if home.exists() {
        tokio::fs::remove_dir_all(&home)
            .await
            .map_err(|e| format!("failed to remove apollia home: {e}"))?;
    }
    Ok(())
}

/// Restarts the application. Called after a destructive action that needs a
/// cold-start (onboarding reset, factory reset).
///
/// **Important:** this function never returns on success because it kills the
/// current process and spawns a new one. In dev mode the restart may fail, in
/// which case the user must relaunch manually.
#[tauri::command]
pub async fn app_restart(app: tauri::AppHandle) -> Result<(), String> {
    // app.restart() kills the current process and spawns a new one, so this
    // function never returns on success. In dev mode, restart may fail silently
    // (no packaged bundle to relaunch); the frontend should handle this by
    // showing a manual reload prompt if the app doesn't actually restart.
    app.restart();
}

/// Quits the application gracefully from a webview surface (in-app user menu,
/// command palette).
///
/// Routes through the same shared quit as the tray and the macOS app menu:
/// it drains the embedded runtime, then exits via `AppHandle::exit`, which
/// bypasses the window `CloseRequested` handler (close-to-tray). Calling
/// `window.close()` from the webview instead would only hide the window.
#[tauri::command]
pub async fn quit_app(app: tauri::AppHandle) -> Result<(), String> {
    crate::tray::initiate_quit(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onboarded_flag_path_ends_with_onboarded() {
        // GIVEN the onboarded flag path
        let path = onboarded_flag_path();
        // WHEN it is resolved
        // THEN it ends with .onboarded inside .apollia directory
        assert!(path.ends_with(".onboarded"));
        assert!(path.to_string_lossy().contains(".apollia"));
    }
}
