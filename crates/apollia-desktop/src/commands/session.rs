//! Commandes IPC Tauri exposant l'état de `SessionMetrics` (US-SP42-047 Pattern P11).
//!
//! Le panel frontend écoute l'événement `runtime-event`/`SessionMetricsUpdated`
//! pour les mises à jour temps-réel, et utilise `get_session_metrics` pour
//! récupérer le snapshot courant au premier rendu.

use apollia_core::session_metrics::SessionMetrics;
use apollia_runtime::SessionMetricsStore;
use tauri::State;

/// Retourne le snapshot courant des métriques pour une session donnée.
///
/// Retourne `None` si la session est inconnue (aucun événement reçu encore).
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

/// Retourne la liste des session_ids connus du store — utile pour diagnostic.
#[tauri::command]
pub fn list_session_metrics_ids(
    store: State<'_, SessionMetricsStore>,
) -> Result<Vec<String>, String> {
    let guard = store
        .lock()
        .map_err(|e| format!("SessionMetrics store poisoned: {e}"))?;
    Ok(guard.keys().cloned().collect())
}
