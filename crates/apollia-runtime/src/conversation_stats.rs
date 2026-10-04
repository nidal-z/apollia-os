//! The statistics of one chat conversation, as the memory gauge of the chat
//! draws them.
//!
//! The desktop running the runtime in its own process calls
//! [`conversation_stats`] directly; a client of the daemon reaches it through
//! `GET /api/v1/sessions/{id}/stats`. Both answer the same view and refuse
//! with the same words.

use std::sync::Mutex;

use apollia_memory::user_memory::UserMemoryRepository;
use serde::{Deserialize, Serialize};

use crate::chat::{profile_injection, ChatSessionManagerHandle, DEFAULT_CONTEXT_WINDOW_SIZE};

/// The marker of the block that lists the past conversations.
const CROSS_SESSION_MARKER: &str = "## Previous conversations (for reference)";

/// Statistics of one chat conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConversationStatsView {
    /// Total number of messages in the session.
    pub message_count: usize,
    /// Number of messages that have been summarized.
    pub summarized_count: usize,
    /// Approximate context window usage percentage.
    pub context_usage_pct: f64,
    /// Whether the assistant's prompt carries the profile of the conversation.
    pub user_memory_injected: bool,
    /// Number of past sessions referenced via cross-session recall.
    pub cross_sessions_referenced: usize,
}

/// Why the statistics of a conversation could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConversationStatsError {
    /// The chat subsystem did not start.
    #[error("NOT_INITIALIZED: Chat subsystem is not available")]
    Unavailable,
    /// No session has this id.
    #[error("NOT_FOUND: Chat session not found: {0}")]
    NotFound(String),
}

/// How many past sessions the system prompt references.
pub(crate) fn count_cross_session_refs(system_prompt: &str) -> usize {
    if !system_prompt.contains(CROSS_SESSION_MARKER) {
        return 0;
    }
    system_prompt.matches("- [").count()
}

/// The statistics of `session_id`. `profiles` is the profile store, without
/// which no profile can be injected.
///
/// # Errors
///
/// [`ConversationStatsError::Unavailable`] without a chat manager,
/// [`ConversationStatsError::NotFound`] for an unknown session.
pub async fn conversation_stats(
    manager: Option<&ChatSessionManagerHandle>,
    profiles: Option<&Mutex<UserMemoryRepository>>,
    session_id: String,
) -> Result<ConversationStatsView, ConversationStatsError> {
    let manager = manager.ok_or(ConversationStatsError::Unavailable)?;
    let detail = manager
        .get_session(session_id.clone())
        .await
        .ok_or_else(|| ConversationStatsError::NotFound(session_id.clone()))?;

    let message_count = detail.message_count as usize;
    let prompt = &detail.session.system_prompt;
    let has_summary = prompt.contains("## Conversation summary");
    let summarized_count = if has_summary && message_count > DEFAULT_CONTEXT_WINDOW_SIZE {
        message_count.saturating_sub(DEFAULT_CONTEXT_WINDOW_SIZE)
    } else {
        0
    };

    // Token-based context gauge: current occupancy over the model's real
    // window, read from the accumulated session metrics. 0 (unknown) when no
    // exchange has completed or the backend does not report a window.
    let context_usage_pct = manager
        .get_session_metrics(session_id)
        .await
        .filter(|m| m.context_window_tokens > 0)
        .map(|m| (m.context_tokens_used as f64 / m.context_window_tokens as f64) * 100.0)
        .map(|pct| pct.min(100.0))
        .unwrap_or(0.0);

    let user_memory_injected = profile_injection::injected(&detail.session, profiles);

    Ok(ConversationStatsView {
        message_count,
        summarized_count,
        context_usage_pct,
        user_memory_injected,
        cross_sessions_referenced: count_cross_session_refs(prompt),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_session_refs_are_counted_under_the_marker() {
        // GIVEN a prompt carrying the previous-conversations block and two entries
        let prompt = "Hello.\n\n\
            ## Previous conversations (for reference)\n\
            - [2026-03-20T10:00:00Z] foo\n\
            - [2026-03-19T14:00:00Z] bar\n";
        // WHEN the cross-session references are counted
        let count = count_cross_session_refs(prompt);
        // THEN the count is two
        assert_eq!(count, 2);
    }

    #[test]
    fn test_cross_session_refs_without_the_marker_are_zero() {
        // GIVEN a prompt with no such block
        let prompt = "Hi - [not a reference]";
        // WHEN the cross-session references are counted
        let count = count_cross_session_refs(prompt);
        // THEN the count is zero rather than a false positive on a plain sentence
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_stats_without_a_chat_manager_are_refused_as_unavailable() {
        // GIVEN a runtime whose chat subsystem did not start
        // WHEN the statistics of a session are asked
        let error = conversation_stats(None, None, "s-1".to_owned())
            .await
            .unwrap_err();
        // THEN the refusal names the subsystem, in the words the desktop showed
        assert_eq!(
            error.to_string(),
            "NOT_INITIALIZED: Chat subsystem is not available"
        );
    }
}
