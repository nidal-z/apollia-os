//! Tauri IPC commands for the user profile and conversation statistics (ADR-087).
//!
//! Surface:
//! - [`get_profile_schema`] — the canonical `PROFILE_SCHEMA` for UI rendering.
//! - [`get_profile`] — schema entries + extras for the Settings → Profile page.
//! - [`set_profile_entry`] — upsert (always `WrittenBy::User`).
//! - [`delete_profile_entry`] — remove a single entry by key.
//! - [`reset_user_profile`] — purge every user-visible entry.
//! - [`get_conversation_stats`] — per-session chat statistics.

use std::sync::Arc;

use apollia_memory::profile_schema::{ProfileFieldType, PROFILE_SCHEMA};
use apollia_memory::user_memory::{ProfileEntry, UserMemoryRepository, WrittenBy};
use apollia_runtime::chat::{ChatSessionManagerHandle, DEFAULT_CONTEXT_WINDOW_SIZE};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

// ---------------------------------------------------------------------------
// View types serialised to the Svelte frontend
// ---------------------------------------------------------------------------

/// A canonical schema field exposed to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileFieldView {
    /// Storage key (`name`, `agents.hitl`, ...).
    pub key: String,
    /// French label.
    pub label_fr: String,
    /// English label.
    pub label_en: String,
    /// French inline help.
    pub help_fr: String,
    /// English inline help.
    pub help_en: String,
    /// UI section (`identity`, `work`, `preferences`, `constraints`).
    pub section: String,
    /// `true` for fields whose change warrants a "rerun onboarding" notice.
    pub sensitive: bool,
    /// Input control to render (`text`, `long_text`, `select`).
    pub field_type: String,
    /// Allowed values for `select` fields; empty otherwise.
    pub options: Vec<String>,
}

/// A single profile entry formatted for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileEntryView {
    /// Flat storage key.
    pub key: String,
    /// Plain-text value.
    pub value: String,
    /// Provenance tag (`onboarding`, `user`, `agent:<name>`).
    pub written_by: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
    /// `true` when the key matches a canonical [`PROFILE_SCHEMA`] entry.
    pub in_schema: bool,
}

/// Aggregated user profile for the Settings → Profile page.
#[derive(Debug, Clone, Serialize)]
pub struct UserProfileView {
    /// Schema-driven entries (subset of [`Self::entries`] where `in_schema`).
    pub schema_entries: Vec<ProfileEntryView>,
    /// Free-form entries not present in the schema.
    pub extras: Vec<ProfileEntryView>,
    /// All entries combined (schema first, then extras).
    pub entries: Vec<ProfileEntryView>,
    /// Most recent `updated_at` across all entries.
    pub last_updated_at: Option<String>,
}

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

fn get_repo(state: &RuntimeHandle) -> Result<Arc<std::sync::Mutex<UserMemoryRepository>>, String> {
    state
        .user_memory
        .as_ref()
        .cloned()
        .ok_or_else(|| "NOT_INITIALIZED: User profile repository is not initialized".to_string())
}

fn get_chat_manager(state: &RuntimeHandle) -> Result<&ChatSessionManagerHandle, String> {
    state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "NOT_INITIALIZED: Chat subsystem is not available".to_string())
}

fn profile_entry_to_view(entry: &ProfileEntry) -> ProfileEntryView {
    ProfileEntryView {
        key: entry.key.clone(),
        value: entry.value.clone(),
        written_by: entry.written_by.tag(),
        created_at: entry.created_at.clone(),
        updated_at: entry.updated_at.clone(),
        in_schema: entry.in_schema,
    }
}

fn field_type_tag(t: ProfileFieldType) -> &'static str {
    match t {
        ProfileFieldType::Text => "text",
        ProfileFieldType::LongText => "long_text",
        ProfileFieldType::Select => "select",
    }
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

/// Returns the canonical profile schema declared in
/// [`apollia_memory::profile_schema::PROFILE_SCHEMA`].
#[tauri::command]
pub async fn get_profile_schema() -> Result<Vec<ProfileFieldView>, String> {
    Ok(PROFILE_SCHEMA
        .iter()
        .map(|f| ProfileFieldView {
            key: f.key.to_owned(),
            label_fr: f.label_fr.to_owned(),
            label_en: f.label_en.to_owned(),
            help_fr: f.help_fr.to_owned(),
            help_en: f.help_en.to_owned(),
            section: f.section.tag().to_owned(),
            sensitive: f.sensitive,
            field_type: field_type_tag(f.field_type).to_owned(),
            options: f.options.iter().map(|s| (*s).to_owned()).collect(),
        })
        .collect())
}

/// Returns the user profile (schema entries + extras) for the
/// Settings → Profile page.
#[tauri::command]
pub async fn get_profile(state: State<'_, RuntimeHandle>) -> Result<UserProfileView, String> {
    let repo = get_repo(&state)?;

    let entries = tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.list_all().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    let views: Vec<ProfileEntryView> = entries.iter().map(profile_entry_to_view).collect();
    let schema_entries = views.iter().filter(|e| e.in_schema).cloned().collect();
    let extras = views.iter().filter(|e| !e.in_schema).cloned().collect();
    let last_updated_at = views.iter().map(|e| e.updated_at.clone()).max();

    Ok(UserProfileView {
        schema_entries,
        extras,
        entries: views,
        last_updated_at,
    })
}

/// Creates or updates a single profile entry.
///
/// Always written with [`WrittenBy::User`] — the source represents the
/// frontend operator action.
#[tauri::command]
pub async fn set_profile_entry(
    state: State<'_, RuntimeHandle>,
    request: SetProfileEntryRequest,
) -> Result<ProfileEntryView, String> {
    if request.key.is_empty() {
        return Err("VALIDATION_ERROR: Key must not be empty".to_string());
    }

    let repo = get_repo(&state)?;
    let key = request.key.clone();
    let value = request.value.clone();

    let entry = tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.set(&key, &value, WrittenBy::User)
            .map_err(|e| e.to_string())?;
        let stored = repo
            .get(&key)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("entry '{key}' missing after store"))?;
        Ok::<ProfileEntry, String>(stored)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    Ok(profile_entry_to_view(&entry))
}

/// Deletes a single profile entry by key.
#[tauri::command]
pub async fn delete_profile_entry(
    state: State<'_, RuntimeHandle>,
    key: String,
) -> Result<(), String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.forget(&key).map_err(|e| {
            if e.to_string().contains("not found") {
                format!("NOT_FOUND: Profile entry not found: {key}")
            } else {
                e.to_string()
            }
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Deletes every user-visible profile entry, preserving onboarding markers.
#[tauri::command]
pub async fn reset_user_profile(state: State<'_, RuntimeHandle>) -> Result<usize, String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.reset().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
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

    let context_usage_pct = if context_window > 0 {
        let used = message_count.min(context_window);
        (used as f64 / context_window as f64) * 100.0
    } else {
        0.0
    };

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
        let prompt = "Hello.\n\n\
            ## Previous conversations (for reference)\n\
            - [2026-03-20T10:00:00Z] foo\n\
            - [2026-03-19T14:00:00Z] bar\n";
        assert_eq!(count_cross_session_refs(prompt), 2);
    }

    #[test]
    fn cross_session_refs_without_marker() {
        assert_eq!(count_cross_session_refs("Hi"), 0);
    }

    #[test]
    fn field_type_tag_maps_correctly() {
        assert_eq!(field_type_tag(ProfileFieldType::Text), "text");
        assert_eq!(field_type_tag(ProfileFieldType::LongText), "long_text");
        assert_eq!(field_type_tag(ProfileFieldType::Select), "select");
    }

    #[test]
    fn profile_entry_to_view_preserves_fields() {
        let entry = ProfileEntry {
            key: "name".to_owned(),
            value: "Nidal".to_owned(),
            written_by: WrittenBy::Onboarding,
            created_at: "2026-05-11T10:00:00Z".to_owned(),
            updated_at: "2026-05-11T10:00:00Z".to_owned(),
            in_schema: true,
        };
        let view = profile_entry_to_view(&entry);
        assert_eq!(view.key, "name");
        assert_eq!(view.value, "Nidal");
        assert_eq!(view.written_by, "onboarding");
        assert!(view.in_schema);
    }
}
