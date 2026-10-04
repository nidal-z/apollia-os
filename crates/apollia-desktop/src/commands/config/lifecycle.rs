//! Tauri IPC commands that reset or end a session: the onboarding flag, the
//! stored memories, the log files and their folder, the factory reset, and the
//! two application lifecycle commands the webview calls instead of closing its
//! own window.
//!
//! Embedded, the five resets call [`apollia_runtime::app_lifecycle`] on the
//! data directory of this machine. Attached, they reach the same functions
//! through the routes of the daemon, which owns the files they erase. Restart
//! and quit belong to the window and stay here.

use std::path::PathBuf;

use apollia_runtime::api::routes_app_lifecycle::{
    DirectoryRemoved, MemoryWiped, OnboardingMarked, OnboardingReset,
};
use apollia_runtime::app_lifecycle;
use apollia_runtime::profile::ProfileStore;
use tauri::State;

use crate::attached::{AttachedClient, AttachedError, RuntimeLink};

/// Resolves `~/.apollia/`, the data directory of the runtime this process
/// embeds.
fn apollia_home() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home)
}

/// Writes the onboarding flag on the daemon.
pub(crate) async fn mark_onboarded_on(remote: &AttachedClient) -> Result<(), AttachedError> {
    let _: OnboardingMarked = remote.post("/api/v1/onboarding/complete", &()).await?;
    Ok(())
}

/// Resets the onboarding on the daemon; answers how many profile entries
/// were removed.
pub(crate) async fn reset_onboarding_on(remote: &AttachedClient) -> Result<usize, AttachedError> {
    let answer: OnboardingReset = remote.post("/api/v1/onboarding/reset", &()).await?;
    Ok(answer.removed_profile_entries)
}

/// Removes every memory database of the daemon; answers how many.
pub(crate) async fn clear_all_memories_on(remote: &AttachedClient) -> Result<usize, AttachedError> {
    let answer: MemoryWiped = remote.post("/api/v1/memory/wipe", &()).await?;
    Ok(answer.removed_databases)
}

/// Removes the daemon's logs directory.
pub(crate) async fn clear_logs_on(remote: &AttachedClient) -> Result<(), AttachedError> {
    let _: DirectoryRemoved = remote.post("/api/v1/logs/clear", &()).await?;
    Ok(())
}

/// Removes the daemon's whole data directory.
pub(crate) async fn factory_reset_on(remote: &AttachedClient) -> Result<(), AttachedError> {
    let _: DirectoryRemoved = remote.post("/api/v1/factory-reset", &()).await?;
    Ok(())
}

/// Marks onboarding as complete by creating the flag file
/// `~/.apollia/.onboarded` of the runtime, the daemon's when attached.
#[tauri::command]
pub async fn mark_onboarded(link: State<'_, RuntimeLink>) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => app_lifecycle::mark_onboarded(&apollia_home())
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(mark_onboarded_on(remote).await?),
    }
}

/// Fully resets onboarding: removes the completion flag, purges the flow's
/// internal markers and the visible user profile, and removes the onboarding
/// agent's memory, so the journey starts from scratch.
#[tauri::command]
pub async fn reset_onboarding(link: State<'_, RuntimeLink>) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            app_lifecycle::reset_onboarding(
                &apollia_home(),
                state.user_memory.as_ref().map(ProfileStore),
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(())
        }
        RuntimeLink::Attached(remote) => {
            reset_onboarding_on(remote).await?;
            Ok(())
        }
    }
}

/// Removes **all** of Apollia's memory databases, every file of the memory
/// directory regardless of namespace. Irreversible action.
///
/// Returns the number of `.db` files removed.
#[tauri::command]
pub async fn clear_all_memories(link: State<'_, RuntimeLink>) -> Result<usize, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => app_lifecycle::clear_all_memories(&apollia_home())
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(clear_all_memories_on(remote).await?),
    }
}

/// Removes the logs directory of the runtime, the daemon's when attached.
///
/// If the directory does not exist, the command is a no-op.
#[tauri::command]
pub async fn clear_logs(link: State<'_, RuntimeLink>) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => app_lifecycle::clear_logs(&apollia_home())
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(clear_logs_on(remote).await?),
    }
}

/// Opens the folder of this application's log files in the file manager of
/// this machine, creating it when it does not exist yet.
///
/// The folder is the window's own (`~/.apollia/logs`, where
/// [`crate::file_log`] writes), in both modes.
#[tauri::command]
pub async fn open_logs_dir() -> Result<(), String> {
    let dir = crate::file_log::logs_dir_under(&apollia_home());
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    open::that(&dir).map_err(|e| {
        tracing::event!(
            tracing::Level::WARN,
            path = %dir.display(),
            error = %e,
            "desktop.logs_dir.open_failed"
        );
        format!("failed to open {}: {e}", dir.display())
    })
}

/// Factory reset: removes **all** of the runtime's data directory, the
/// daemon's when attached.
///
/// Irreversible destructive action: removes config, memory, sessions, logs,
/// models and credentials. The user is prompted to restart after the call.
#[tauri::command]
pub async fn factory_reset(link: State<'_, RuntimeLink>) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => app_lifecycle::factory_reset(&apollia_home())
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(factory_reset_on(remote).await?),
    }
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
mod tests;
