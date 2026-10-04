//! Tauri IPC commands for the user profiles and conversation statistics.
//!
//! Surface, each profile named by its identifier or by `default`:
//! - [`list_profiles`], [`create_profile`], [`get_profile_by_id`],
//!   [`update_profile`], [`delete_profile`], [`set_default_profile`],
//!   [`reset_profile`]: the profiles of Settings > Profile.
//! - [`set_profile_fact`], [`delete_profile_fact`]: the facts retained in one
//!   profile.
//! - [`get_conversation_stats`]: per-session chat statistics.
//!
//! Every profile command serves both modes: the embedded arm calls
//! [`apollia_runtime::profile`], the attached arm the `/api/v1/profiles`
//! routes that serve the same functions.

use apollia_runtime::api::routes_profile::{
    DeletedProfileEntry, ProfileReset, SetProfileEntryBody,
};
use apollia_runtime::api::routes_profiles::{CreateProfileBody, DeletedProfile};
use apollia_runtime::conversation_stats::conversation_stats;
pub use apollia_runtime::conversation_stats::ConversationStatsView;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::profile::{self, ProfileError, ProfileStore};
pub use apollia_runtime::profile::{
    ProfileDetailView, ProfileEntryView, ProfileSummaryView, ProfileUpdate,
};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// The profile repository of the runtime running in this process.
fn embedded_repo(state: &RuntimeHandle) -> Result<ProfileStore<'_>, String> {
    state
        .user_memory
        .as_ref()
        .map(ProfileStore)
        .ok_or_else(|| ProfileError::Unavailable.to_string())
}

fn profile_path(id: &str) -> String {
    format!("/api/v1/profiles/{}", segment(id))
}

/// Lists every profile, the default first.
#[tauri::command]
pub async fn list_profiles(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<ProfileSummaryView>, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => profile::list_profiles(embedded_repo(state)?)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(remote.get("/api/v1/profiles").await?),
    }
}

/// Creates a profile with empty fields, not the default.
#[tauri::command]
pub async fn create_profile(
    link: State<'_, RuntimeLink>,
    name: String,
) -> Result<ProfileSummaryView, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => profile::create_profile(embedded_repo(state)?, name)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(remote
            .post("/api/v1/profiles", &CreateProfileBody { name })
            .await?),
    }
}

/// Reads one profile in full: its fields, in the order of the schema, and
/// its facts.
#[tauri::command]
pub async fn get_profile_by_id(
    link: State<'_, RuntimeLink>,
    id: String,
) -> Result<ProfileDetailView, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => profile::read_profile_by_id(embedded_repo(state)?, id)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(remote.get(&profile_path(&id)).await?),
    }
}

/// Renames a profile and writes its fields.
#[tauri::command]
pub async fn update_profile(
    link: State<'_, RuntimeLink>,
    id: String,
    update: ProfileUpdate,
) -> Result<ProfileDetailView, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => profile::update_profile(embedded_repo(state)?, id, update)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(remote.patch(&profile_path(&id), &update).await?),
    }
}

/// Deletes a profile and its facts, and returns the conversations that
/// designated it to the default profile; the default profile is refused.
#[tauri::command]
pub async fn delete_profile(link: State<'_, RuntimeLink>, id: String) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => profile::delete_profile_and_release(
            embedded_repo(state)?,
            state.chat_manager.as_ref(),
            id,
        )
        .await
        .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => {
            let _: DeletedProfile = remote.delete(&profile_path(&id)).await?;
            Ok(())
        }
    }
}

/// Makes a profile the default.
#[tauri::command]
pub async fn set_default_profile(
    link: State<'_, RuntimeLink>,
    id: String,
) -> Result<ProfileSummaryView, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => profile::set_default_profile(embedded_repo(state)?, id)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(remote
            .post(&format!("{}/default", profile_path(&id)), &())
            .await?),
    }
}

/// Empties a profile and deletes its facts; returns how many entries went.
#[tauri::command]
pub async fn reset_profile(link: State<'_, RuntimeLink>, id: String) -> Result<usize, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => profile::reset_profile_by_id(embedded_repo(state)?, id)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => {
            let reset: ProfileReset = remote
                .post(&format!("{}/reset", profile_path(&id)), &())
                .await?;
            Ok(reset.removed)
        }
    }
}

/// Retains a fact in a profile, written by the user.
#[tauri::command]
pub async fn set_profile_fact(
    link: State<'_, RuntimeLink>,
    id: String,
    key: String,
    value: String,
) -> Result<ProfileEntryView, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            profile::set_profile_fact(embedded_repo(state)?, id, key, value)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => {
            let path = format!("{}/facts/{}", profile_path(&id), segment(&key));
            Ok(remote.put(&path, &SetProfileEntryBody { value }).await?)
        }
    }
}

/// Forgets a fact of a profile.
#[tauri::command]
pub async fn delete_profile_fact(
    link: State<'_, RuntimeLink>,
    id: String,
    key: String,
) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            profile::delete_profile_fact(embedded_repo(state)?, id, key)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => {
            let path = format!("{}/facts/{}", profile_path(&id), segment(&key));
            let _: DeletedProfileEntry = remote.delete(&path).await?;
            Ok(())
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
