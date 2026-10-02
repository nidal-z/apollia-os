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
use apollia_runtime::chat::{ChatSessionManagerHandle, DEFAULT_CONTEXT_WINDOW_SIZE};
use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::profile::{self, ProfileError, ProfileStore};
pub use apollia_runtime::profile::{ProfileEntryView, UserProfileView};
use serde::{Deserialize, Serialize};
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

/// Statistics for a single chat conversation session.
#[derive(Debug, Clone, Serialize)]
pub struct ConversationStatsView {
    /// Total number of messages in the session.
    pub message_count: usize,
    /// Number of messages that have been summarized.
    pub summarized_count: usize,
    /// Approximate context window usage percentage.
    pub context_usage_pct: f64,
    /// Whether user profile was injected into the system prompt.
    pub user_memory_injected: bool,
    /// Number of past sessions referenced via cross-session recall.
    pub cross_sessions_referenced: usize,
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

fn get_chat_manager(state: &RuntimeHandle) -> Result<&ChatSessionManagerHandle, String> {
    state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "NOT_INITIALIZED: Chat subsystem is not available".to_string())
}

fn count_cross_session_refs(system_prompt: &str) -> usize {
    let marker = "## Previous conversations (for reference)";
    if !system_prompt.contains(marker) {
        return 0;
    }
    system_prompt.matches("- [").count()
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

/// Returns statistics for a chat conversation session.
#[tauri::command]
pub async fn get_conversation_stats(
    state: State<'_, RuntimeHandle>,
    session_id: String,
) -> Result<ConversationStatsView, String> {
    let manager = get_chat_manager(&state)?;

    let detail = manager
        .get_session(session_id.clone())
        .await
        .ok_or_else(|| format!("NOT_FOUND: Chat session not found: {session_id}"))?;

    let message_count = detail.message_count as usize;
    let context_window = DEFAULT_CONTEXT_WINDOW_SIZE;

    let has_summary = detail
        .session
        .system_prompt
        .contains("## Conversation summary");

    let summarized_count = if has_summary && message_count > context_window {
        message_count.saturating_sub(context_window)
    } else {
        0
    };

    // Token-based context gauge: current occupancy over the model's real window,
    // read from the accumulated session metrics. Falls back to 0 (unknown) when
    // no exchange has completed or the backend does not report a window, rather
    // than the old message-count-over-20 saturation.
    let metrics = manager.get_session_metrics(session_id.clone()).await;
    let context_usage_pct = metrics
        .filter(|m| m.context_window_tokens > 0)
        .map(|m| (m.context_tokens_used as f64 / m.context_window_tokens as f64) * 100.0)
        .map(|pct| pct.min(100.0))
        .unwrap_or(0.0);

    let prompt = &detail.session.system_prompt;
    let user_memory_injected = state.user_memory.is_some()
        && (prompt.contains("Section: identity")
            || prompt.contains("Section: work")
            || prompt.contains("Section: preferences")
            || prompt.contains("Section: constraints")
            || prompt.contains("Section: other"));

    let cross_sessions_referenced = count_cross_session_refs(&detail.session.system_prompt);

    Ok(ConversationStatsView {
        message_count,
        summarized_count,
        context_usage_pct,
        user_memory_injected,
        cross_sessions_referenced,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_session_refs_with_marker() {
        // GIVEN a prompt carrying the previous-conversations block and two entries
        let prompt = "Hello.\n\n\
            ## Previous conversations (for reference)\n\
            - [2026-03-20T10:00:00Z] foo\n\
            - [2026-03-19T14:00:00Z] bar\n";
        // WHEN the cross-session references are counted
        // THEN the count is two
        assert_eq!(count_cross_session_refs(prompt), 2);
    }

    #[test]
    fn cross_session_refs_without_marker() {
        // GIVEN a prompt with no such block
        // WHEN the cross-session references are counted
        // THEN the count is zero rather than a false positive on a plain sentence
        assert_eq!(count_cross_session_refs("Hi"), 0);
    }
}
