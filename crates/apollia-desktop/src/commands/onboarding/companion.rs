//! The in-app companion: whether it is on, and the chat session it runs
//! against the guide agent.
//!
//! What the guide says of the screen it opens on is a text of the interface
//! catalogues, in the language of the interface, and never passes through
//! here. The preference and the session belong to the runtime: embedded, the
//! commands call [`apollia_runtime::companion`] directly; attached, they reach
//! the same functions through `/api/v1/companion/...` on the daemon.

use apollia_runtime::api::routes_companion::{
    CompanionEnabled, CompanionSession, CreateCompanionSessionBody,
};
use apollia_runtime::companion;
use apollia_runtime::profile::ProfileStore;
use tauri::State;

use crate::attached::{AttachedClient, AttachedError, RuntimeLink};

/// Reads whether the companion is on, from the daemon.
pub(crate) async fn enabled_on(remote: &AttachedClient) -> Result<bool, AttachedError> {
    let answer: CompanionEnabled = remote.get("/api/v1/companion/enabled").await?;
    Ok(answer.enabled)
}

/// Records on the daemon whether the companion is on.
pub(crate) async fn set_enabled_on(
    remote: &AttachedClient,
    enabled: bool,
) -> Result<(), AttachedError> {
    let _: CompanionEnabled = remote
        .put("/api/v1/companion/enabled", &CompanionEnabled { enabled })
        .await?;
    Ok(())
}

/// Opens a guide session on the daemon.
pub(crate) async fn create_session_on(
    remote: &AttachedClient,
    context: Option<String>,
) -> Result<CompanionSession, AttachedError> {
    remote
        .post(
            "/api/v1/companion/sessions",
            &CreateCompanionSessionBody { context },
        )
        .await
}

/// Creates a companion session backed by the apollia-guide agent.
///
/// The session runs in Agent mode so the panel is driven by the single
/// product coach (knowledge base and action buttons), and its identifier is
/// recorded in the onboarding state. A guide that is not installed is
/// reported as such, so the panel shows an install hint instead of an opaque
/// failure.
#[tauri::command]
pub async fn create_companion_session(
    context: Option<String>,
    link: State<'_, RuntimeLink>,
) -> Result<CompanionSession, String> {
    let created = match link.inner() {
        RuntimeLink::Embedded(state) => companion::create_session(
            state.chat_manager.as_ref(),
            state.user_memory.as_ref().map(ProfileStore),
            context,
        )
        .await
        .map(|session_id| CompanionSession { session_id })
        .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => create_session_on(remote, context)
            .await
            .map_err(String::from),
    };
    created.inspect_err(|error| {
        tracing::event!(
            tracing::Level::ERROR,
            error = %error,
            "companion.session.create.failed"
        );
    })
}

/// Persists the companion-enabled preference to UserMemory internal state.
///
/// Stored under the internal-state key `companion_enabled` (auto-prefixed
/// with `__` so it stays out of the user profile listing).
#[tauri::command]
pub async fn set_companion_enabled(
    enabled: bool,
    link: State<'_, RuntimeLink>,
) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            companion::write_enabled(state.user_memory.as_ref().map(ProfileStore), enabled)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => Ok(set_enabled_on(remote, enabled).await?),
    }
}

/// Reads the companion-enabled preference from UserMemory internal state.
/// Defaults to `false` when the key has never been written.
#[tauri::command]
pub async fn get_companion_enabled(link: State<'_, RuntimeLink>) -> Result<bool, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            companion::read_enabled(state.user_memory.as_ref().map(ProfileStore))
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => Ok(enabled_on(remote).await?),
    }
}

#[cfg(test)]
mod attached_tests;
