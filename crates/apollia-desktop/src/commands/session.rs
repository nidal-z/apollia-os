//! Tauri IPC commands exposing the `SessionMetrics` state.
//!
//! The frontend panel listens to the `runtime-event`/`SessionMetricsUpdated`
//! event for real-time updates, and uses `get_session_metrics` to fetch the
//! current snapshot on first render.

use apollia_core::session_metrics::SessionMetrics;
use apollia_runtime::SessionMetricsStore;
use tauri::State;

/// Returns the current metrics snapshot for a given session.
///
/// Returns `None` if the session is unknown (no event received yet).
#[tauri::command]
pub fn get_session_metrics(
    session_id: String,
    store: State<'_, SessionMetricsStore>,
) -> Result<Option<SessionMetrics>, String> {
    let guard = store
        .lock()
        .map_err(|e| format!("SessionMetrics store poisoned: {e}"))?;
    Ok(guard.get(&session_id).cloned())
}

/// Returns the list of session_ids known to the store; useful for diagnostics.
#[tauri::command]
pub fn list_session_metrics_ids(
    store: State<'_, SessionMetricsStore>,
) -> Result<Vec<String>, String> {
    let guard = store
        .lock()
        .map_err(|e| format!("SessionMetrics store poisoned: {e}"))?;
    Ok(guard.keys().cloned().collect())
}
