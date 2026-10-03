//! Automatic naming of a chat session. The prompt, the sanitising of the model
//! answer and the rename live in `apollia_runtime::chat::title`, which the
//! daemon serves at `POST /api/v1/sessions/{id}/title/generate`.

use tauri::{Manager, State};

use crate::attached::RuntimeLink;
use crate::SharedLlmRouter;

/// Generates a short title for a chat session from its first user message,
/// stores it, and returns it.
///
/// Embedded, the title is drawn by the live desktop router, which
/// `reload_llm_from_db` rebuilds; attached, by the daemon's own router. The
/// desktop router is read from the application rather than taken as a state
/// argument, because the attached application never builds one.
#[tauri::command]
pub async fn generate_chat_session_name(
    link: State<'_, RuntimeLink>,
    app: tauri::AppHandle,
    session_id: String,
    first_message: String,
) -> Result<String, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return super::conversations::name(remote, &session_id, &first_message).await;
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    // Use the live desktop router (rebuilt by reload_llm_from_db), not the
    // boot-time RuntimeHandle snapshot, which stays None when the backend is
    // configured after startup (e.g. during onboarding).
    let llm = match app.try_state::<SharedLlmRouter>() {
        Some(shared) => shared
            .read()
            .map_err(|e| format!("lock poisoned: {e}"))?
            .clone(),
        None => None,
    };

    apollia_runtime::chat::title::name_session(manager, llm.as_deref(), session_id, first_message)
        .await
        .map_err(|e| e.to_string())
}
