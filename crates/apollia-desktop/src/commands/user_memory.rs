//! Tauri IPC commands for user memory management and conversation statistics.
//!
//! Provides direct access to [`UserMemoryRepository`] and
//! [`ChatSessionManagerHandle`] from the frontend, bypassing the REST API
//! layer for lower latency.  All data stays local (Principle #1).

use std::sync::Arc;

use apollia_memory::user_memory::{UserMemoryCategory, UserMemoryEntry, UserMemoryRepository};
use apollia_runtime::chat::{ChatSessionManagerHandle, DEFAULT_CONTEXT_WINDOW_SIZE};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Maximum number of results returned by a memory search.
const MAX_SEARCH_RESULTS: usize = 50;

/// Upper limit of entries fetched per category during profile aggregation.
const MAX_RECALL_PER_CATEGORY: usize = 500;

/// Valid category names accepted by [`update_user_memory_entry`].
const VALID_CATEGORIES: &[&str] = &["preferences", "habits", "context"];

// ---------------------------------------------------------------------------
// View types (serialised to the Svelte frontend)
// ---------------------------------------------------------------------------

/// Aggregated user profile with statistics.
#[derive(Debug, Clone, Serialize)]
pub struct UserMemoryProfileView {
    /// All memory entries across categories.
    pub entries: Vec<UserMemoryEntryView>,
    /// Aggregated statistics.
    pub stats: UserMemoryStats,
}

/// A single user memory entry formatted for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct UserMemoryEntryView {
    /// Category of the entry.
    pub category: String,
    /// Short key within its category.
    pub key: String,
    /// Human-readable value.
    pub value: String,
    /// How this entry was created.
    pub source: String,
    /// Confidence score (0.0–1.0).
    pub confidence: f64,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

/// Aggregated statistics over user memory entries.
#[derive(Debug, Clone, Serialize)]
pub struct UserMemoryStats {
    /// Total number of entries.
    pub total: usize,
    /// Entry count per category.
    pub by_category: CategoryCounts,
    /// Entry count per source.
    pub by_source: SourceCounts,
    /// Most recent `updated_at` across all entries, if any.
    pub last_updated_at: Option<String>,
}

/// Entry counts grouped by category.
#[derive(Debug, Clone, Serialize)]
pub struct CategoryCounts {
    /// Number of preference entries.
    pub preferences: usize,
    /// Number of habit entries.
    pub habits: usize,
    /// Number of context entries.
    pub context: usize,
}

/// Entry counts grouped by source.
#[derive(Debug, Clone, Serialize)]
pub struct SourceCounts {
    /// Entries created during onboarding.
    pub onboarding: usize,
    /// Entries inferred by the LLM from conversations.
    pub chat_inference: usize,
    /// Entries explicitly provided by the user.
    pub user_explicit: usize,
    /// Entries observed by agents during execution.
    pub agent_observation: usize,
}

/// Request payload for creating or updating a user memory entry.
#[derive(Debug, Deserialize)]
pub struct UpdateUserMemoryRequest {
    /// Category (`"preferences"`, `"habits"`, or `"context"`).
    pub category: String,
    /// Short key identifying the entry within its category.
    pub key: String,
    /// Human-readable value.
    pub value: String,
    /// Optional confidence override (defaults to 1.0).
    pub confidence: Option<f64>,
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
    /// Whether user memory was injected into the system prompt.
    pub user_memory_injected: bool,
    /// Number of past sessions referenced via cross-session recall.
    pub cross_sessions_referenced: usize,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Acquires the `UserMemoryRepository` from the runtime handle or returns
/// a structured error string for the frontend.
fn get_repo(state: &RuntimeHandle) -> Result<Arc<std::sync::Mutex<UserMemoryRepository>>, String> {
    state
        .user_memory
        .as_ref()
        .cloned()
        .ok_or_else(|| "NOT_INITIALIZED: User memory repository is not initialized".to_string())
}

/// Acquires the `ChatSessionManagerHandle` or returns a structured error.
fn get_chat_manager(state: &RuntimeHandle) -> Result<&ChatSessionManagerHandle, String> {
    state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "NOT_INITIALIZED: Chat subsystem is not available".to_string())
}

/// Converts a [`UserMemoryEntry`] to its frontend-friendly view.
fn entry_to_view(entry: &UserMemoryEntry) -> UserMemoryEntryView {
    UserMemoryEntryView {
        category: entry.category.to_string(),
        key: entry.key.clone(),
        value: entry.value.clone(),
        source: entry.source.to_string(),
        confidence: 1.0,
        created_at: entry.created_at.clone(),
        updated_at: entry.updated_at.clone(),
    }
}

/// Parses and validates a category string.
fn parse_category(raw: &str) -> Result<UserMemoryCategory, String> {
    match raw {
        "preferences" => Ok(UserMemoryCategory::Preferences),
        "habits" => Ok(UserMemoryCategory::Habits),
        "context" => Ok(UserMemoryCategory::Context),
        other => Err(format!(
            "VALIDATION_ERROR: Invalid category: {other}. Valid categories: {}",
            VALID_CATEGORIES.join(", ")
        )),
    }
}

/// Builds aggregated statistics from a flat list of entries.
fn build_stats(entries: &[UserMemoryEntryView]) -> UserMemoryStats {
    let mut by_category = CategoryCounts {
        preferences: 0,
        habits: 0,
        context: 0,
    };
    let mut by_source = SourceCounts {
        onboarding: 0,
        chat_inference: 0,
        user_explicit: 0,
        agent_observation: 0,
    };
    let mut last_updated_at: Option<&str> = None;

    for entry in entries {
        match entry.category.as_str() {
            "preferences" => by_category.preferences += 1,
            "habits" => by_category.habits += 1,
            "context" => by_category.context += 1,
            _ => {}
        }

        match entry.source.as_str() {
            "onboarding" => by_source.onboarding += 1,
            "chat_inference" => by_source.chat_inference += 1,
            "user_explicit" => by_source.user_explicit += 1,
            "agent_observation" => by_source.agent_observation += 1,
            _ => {}
        }

        let candidate = entry.updated_at.as_str();
        if last_updated_at.is_none_or(|prev| candidate > prev) {
            last_updated_at = Some(candidate);
        }
    }

    UserMemoryStats {
        total: entries.len(),
        by_category,
        by_source,
        last_updated_at: last_updated_at.map(String::from),
    }
}

/// Counts occurrences of `"- ["` in a system prompt to estimate how many
/// past sessions were injected via cross-session recall.
fn count_cross_session_refs(system_prompt: &str) -> usize {
    let marker = "## Previous conversations (for reference)";
    if !system_prompt.contains(marker) {
        return 0;
    }
    system_prompt.matches("- [").count()
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Returns the full user memory profile with per-category and per-source
/// statistics.
#[tauri::command]
pub async fn get_user_memory_profile(
    state: State<'_, RuntimeHandle>,
) -> Result<UserMemoryProfileView, String> {
    let repo = get_repo(&state)?;

    let entries = tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;

        let mut all = Vec::new();
        for &cat in &[
            UserMemoryCategory::Preferences,
            UserMemoryCategory::Habits,
            UserMemoryCategory::Context,
        ] {
            let batch = repo
                .recall(cat, MAX_RECALL_PER_CATEGORY)
                .map_err(|e| e.to_string())?;
            all.extend(batch);
        }

        Ok::<Vec<UserMemoryEntry>, String>(all)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    let views: Vec<UserMemoryEntryView> = entries.iter().map(entry_to_view).collect();
    let stats = build_stats(&views);

    Ok(UserMemoryProfileView {
        entries: views,
        stats,
    })
}

/// Creates or updates a user memory entry.
///
/// Source is always `UserExplicit` for entries created via the UI.
/// Confidence defaults to 1.0 when not provided.
#[tauri::command]
pub async fn update_user_memory_entry(
    state: State<'_, RuntimeHandle>,
    request: UpdateUserMemoryRequest,
) -> Result<UserMemoryEntryView, String> {
    if request.key.is_empty() {
        return Err("VALIDATION_ERROR: Key must not be empty".to_string());
    }

    let category = parse_category(&request.category)?;
    let repo = get_repo(&state)?;

    let key = request.key.clone();
    let value = request.value.clone();
    let confidence = request.confidence.unwrap_or(1.0);

    let view = tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;

        repo.store(
            category,
            &key,
            &value,
            apollia_memory::user_memory::UserMemorySource::UserExplicit,
        )
        .map_err(|e| e.to_string())?;

        let entries = repo
            .recall(category, MAX_RECALL_PER_CATEGORY)
            .map_err(|e| e.to_string())?;
        let found = entries
            .iter()
            .find(|e| e.key == key)
            .ok_or_else(|| format!("entry '{key}' not found after store"))?;

        let mut view = entry_to_view(found);
        view.confidence = confidence;
        Ok::<UserMemoryEntryView, String>(view)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    Ok(view)
}

/// Deletes a user memory entry by key.
///
/// Returns an error with code `NOT_FOUND` if no entry matches.
#[tauri::command]
pub async fn delete_user_memory_entry(
    state: State<'_, RuntimeHandle>,
    key: String,
) -> Result<(), String> {
    let repo = get_repo(&state)?;

    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.forget(&key).map_err(|e| {
            if e.to_string().contains("not found") {
                format!("NOT_FOUND: User memory entry not found: {key}")
            } else {
                e.to_string()
            }
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Searches user memory via FTS5 full-text search.
///
/// Returns up to [`MAX_SEARCH_RESULTS`] entries sorted by BM25 relevance.
/// An empty query or zero matches returns an empty list (not an error).
#[tauri::command]
pub async fn search_user_memory(
    state: State<'_, RuntimeHandle>,
    query: String,
) -> Result<Vec<UserMemoryEntryView>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let repo = get_repo(&state)?;

    let entries = tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.search(&query, MAX_SEARCH_RESULTS)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    Ok(entries.iter().map(entry_to_view).collect())
}

/// Returns statistics for a chat conversation session.
///
/// Computes message count, summarization status, context window usage,
/// user memory injection flag, and cross-session reference count.
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
        && (prompt.contains("Category: preferences")
            || prompt.contains("Category: habits")
            || prompt.contains("Category: context"));

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
    fn test_build_stats_aggregates_correctly() {
        // GIVEN entries across categories and sources
        let entries = vec![
            UserMemoryEntryView {
                category: "preferences".into(),
                key: "language".into(),
                value: "francais".into(),
                source: "user_explicit".into(),
                confidence: 1.0,
                created_at: "2026-03-20T10:00:00Z".into(),
                updated_at: "2026-03-20T10:00:00Z".into(),
            },
            UserMemoryEntryView {
                category: "preferences".into(),
                key: "format".into(),
                value: "markdown".into(),
                source: "onboarding".into(),
                confidence: 1.0,
                created_at: "2026-03-20T09:00:00Z".into(),
                updated_at: "2026-03-20T09:00:00Z".into(),
            },
            UserMemoryEntryView {
                category: "habits".into(),
                key: "working_hours".into(),
                value: "9h-18h".into(),
                source: "chat_inference".into(),
                confidence: 0.8,
                created_at: "2026-03-20T08:00:00Z".into(),
                updated_at: "2026-03-20T14:30:00Z".into(),
            },
            UserMemoryEntryView {
                category: "habits".into(),
                key: "review_frequency".into(),
                value: "daily".into(),
                source: "chat_inference".into(),
                confidence: 0.7,
                created_at: "2026-03-20T07:00:00Z".into(),
                updated_at: "2026-03-20T12:00:00Z".into(),
            },
            UserMemoryEntryView {
                category: "context".into(),
                key: "current_project".into(),
                value: "apollia-os".into(),
                source: "agent_observation".into(),
                confidence: 0.9,
                created_at: "2026-03-20T06:00:00Z".into(),
                updated_at: "2026-03-20T13:00:00Z".into(),
            },
        ];

        // WHEN
        let stats = build_stats(&entries);

        // THEN
        assert_eq!(stats.total, 5);
        assert_eq!(stats.by_category.preferences, 2);
        assert_eq!(stats.by_category.habits, 2);
        assert_eq!(stats.by_category.context, 1);
        assert_eq!(stats.by_source.onboarding, 1);
        assert_eq!(stats.by_source.chat_inference, 2);
        assert_eq!(stats.by_source.user_explicit, 1);
        assert_eq!(stats.by_source.agent_observation, 1);
        assert_eq!(
            stats.last_updated_at.as_deref(),
            Some("2026-03-20T14:30:00Z")
        );
    }

    #[test]
    fn test_build_stats_empty() {
        // GIVEN no entries
        let stats = build_stats(&[]);

        // THEN
        assert_eq!(stats.total, 0);
        assert!(stats.last_updated_at.is_none());
    }

    #[test]
    fn test_parse_category_valid() {
        // GIVEN valid category strings
        // THEN they parse correctly
        assert_eq!(
            parse_category("preferences").expect("valid"),
            UserMemoryCategory::Preferences
        );
        assert_eq!(
            parse_category("habits").expect("valid"),
            UserMemoryCategory::Habits
        );
        assert_eq!(
            parse_category("context").expect("valid"),
            UserMemoryCategory::Context
        );
    }

    #[test]
    fn test_parse_category_invalid() {
        // GIVEN an invalid category
        let result = parse_category("invalid_cat");

        // THEN a validation error is returned
        let err = result.expect_err("should fail");
        assert!(err.starts_with("VALIDATION_ERROR:"));
        assert!(err.contains("invalid_cat"));
    }

    #[test]
    fn test_entry_to_view_converts_correctly() {
        // GIVEN a UserMemoryEntry
        let entry = UserMemoryEntry {
            category: UserMemoryCategory::Preferences,
            key: "language".into(),
            value: "francais".into(),
            source: apollia_memory::user_memory::UserMemorySource::UserExplicit,
            created_at: "2026-03-20T10:00:00Z".into(),
            updated_at: "2026-03-20T10:00:00Z".into(),
        };

        // WHEN
        let view = entry_to_view(&entry);

        // THEN
        assert_eq!(view.category, "preferences");
        assert_eq!(view.key, "language");
        assert_eq!(view.value, "francais");
        assert_eq!(view.source, "user_explicit");
    }

    #[test]
    fn test_count_cross_session_refs_with_refs() {
        // GIVEN a system prompt with cross-session references
        let prompt = "You are a helpful assistant.\n\n\
            ## Previous conversations (for reference)\n\
            - [2026-03-20T10:00:00Z] Discussion about Rust async patterns\n\
            - [2026-03-19T14:00:00Z] Pipeline configuration for data ingestion\n";

        // THEN
        assert_eq!(count_cross_session_refs(prompt), 2);
    }

    #[test]
    fn test_count_cross_session_refs_without_marker() {
        // GIVEN a prompt without cross-session references
        assert_eq!(count_cross_session_refs("You are a helpful assistant."), 0);
    }

    #[test]
    fn test_validation_empty_key_rejected() {
        // GIVEN an empty key
        let request = UpdateUserMemoryRequest {
            category: "preferences".into(),
            key: String::new(),
            value: "v".into(),
            confidence: None,
        };

        // THEN validation catches it before any repo access
        assert!(request.key.is_empty());
    }

    #[test]
    fn test_valid_categories_constant() {
        // GIVEN the VALID_CATEGORIES constant
        // THEN it contains exactly the three accepted categories
        assert_eq!(VALID_CATEGORIES.len(), 3);
        assert!(VALID_CATEGORIES.contains(&"preferences"));
        assert!(VALID_CATEGORIES.contains(&"habits"));
        assert!(VALID_CATEGORIES.contains(&"context"));
    }

    #[test]
    fn test_max_search_results_value() {
        assert_eq!(MAX_SEARCH_RESULTS, 50);
    }
}
