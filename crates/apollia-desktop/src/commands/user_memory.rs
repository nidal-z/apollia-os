//! Tauri IPC commands for the user profile and conversation statistics.
//!
//! Surface:
//! - [`get_profile`]: schema entries + extras for the Settings → Profile page.
//! - [`set_profile_entry`]: upsert (always `WrittenBy::User`).
//! - [`delete_profile_entry`]: remove a single entry by key.
//! - [`reset_user_profile`]: purge every user-visible entry.
//! - [`get_conversation_stats`]: per-session chat statistics.

use apollia_runtime::api::routes_profile::{
    DeletedProfileEntry, ProfileReset, SetProfileEntryBody,
};
use apollia_runtime::conversation_stats::conversation_stats;
pub use apollia_runtime::conversation_stats::ConversationStatsView;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::profile::{self, ProfileError, ProfileStore};
pub use apollia_runtime::profile::{ProfileEntryView, UserProfileView};
use serde::Deserialize;
use tauri::State;

use crate::attached::{segment, RuntimeLink};

// ---------------------------------------------------------------------------
// View types serialised to the Svelte frontend
// ---------------------------------------------------------------------------

/// Request payload for [`set_profile_entry`].
#[derive(Debug, Deserialize)]
pub struct SetProfileEntryRequest {
    /// Flat storage key.
    pub key: String,
    /// Plain-text value.
    pub value: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The profile repository of the runtime running in this process.
fn embedded_repo(state: &RuntimeHandle) -> Result<ProfileStore<'_>, String> {
    state
        .user_memory
        .as_ref()
        .map(ProfileStore)
        .ok_or_else(|| ProfileError::Unavailable.to_string())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Returns the user profile (schema entries + extras) for the
/// Settings → Profile page. Attached to a daemon, `GET /api/v1/profile`
/// answers the same view.
#[tauri::command]
pub async fn get_profile(link: State<'_, RuntimeLink>) -> Result<UserProfileView, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => Ok(profile::read_profile(embedded_repo(state)?)
            .await
            .map_err(|e| e.to_string())?),
        RuntimeLink::Attached(remote) => Ok(remote.get("/api/v1/profile").await?),
    }
}

/// Creates or updates a single profile entry.
///
/// Always written with `WrittenBy::User`; the source represents the
/// frontend operator action.
#[tauri::command]
pub async fn set_profile_entry(
    link: State<'_, RuntimeLink>,
    request: SetProfileEntryRequest,
) -> Result<ProfileEntryView, String> {
    if request.key.is_empty() {
        return Err(ProfileError::EmptyKey.to_string());
    }
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            Ok(
                profile::write_entry(embedded_repo(state)?, request.key, request.value)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        RuntimeLink::Attached(remote) => {
            let path = format!("/api/v1/profile/entries/{}", segment(&request.key));
            let body = SetProfileEntryBody {
                value: request.value,
            };
            Ok(remote.put(&path, &body).await?)
        }
    }
}

/// Deletes a single profile entry by key.
#[tauri::command]
pub async fn delete_profile_entry(link: State<'_, RuntimeLink>, key: String) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => Ok(profile::delete_entry(embedded_repo(state)?, key)
            .await
            .map_err(|e| e.to_string())?),
        RuntimeLink::Attached(remote) => {
            let path = format!("/api/v1/profile/entries/{}", segment(&key));
            let _: DeletedProfileEntry = remote.delete(&path).await?;
            Ok(())
        }
    }
}

/// Deletes every user-visible profile entry, preserving onboarding markers.
#[tauri::command]
pub async fn reset_user_profile(link: State<'_, RuntimeLink>) -> Result<usize, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => Ok(profile::reset_profile(embedded_repo(state)?)
            .await
            .map_err(|e| e.to_string())?),
        RuntimeLink::Attached(remote) => {
            let reset: ProfileReset = remote.post("/api/v1/profile/reset", &()).await?;
            Ok(reset.removed)
        }
    }
}

/// Returns statistics for a chat conversation session. Attached to a daemon,
/// `GET /api/v1/sessions/{id}/stats` answers the same view.
#[tauri::command]
pub async fn get_conversation_stats(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<ConversationStatsView, String> {
    stats_on(link.inner(), session_id).await
}

pub(crate) async fn stats_on(
    link: &RuntimeLink,
    session_id: String,
) -> Result<ConversationStatsView, String> {
    match link {
        RuntimeLink::Embedded(state) => conversation_stats(
            state.chat_manager.as_ref(),
            state.user_memory.is_some(),
            session_id,
        )
        .await
        .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => {
            let path = format!("/api/v1/sessions/{}/stats", segment(&session_id));
            Ok(remote.get(&path).await?)
        }
    }
}
