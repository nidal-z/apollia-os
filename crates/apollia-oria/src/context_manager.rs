//! `ContextManager`: automatic management of the LLM context window.
//!
//! Detects when the conversation history approaches the model limit and compacts
//! it via an LLM summary. The original system prompt (messages\[0\]) is always
//! preserved; everything else is replaced by a user message containing the
//! synthesized summary.
//!
//! ## Compaction strategy
//!
//! ```text
//! maybe_compact(&messages, &llm)
//!   |-- estimate_tokens(messages)  ->  estimated_tokens
//!   |-- estimated_tokens / context_limit < threshold  ->  returns messages unchanged
//!   `-- estimated_tokens / context_limit >= threshold
//!         |-- summarize(messages, llm)  ->  summary (max summary_max_chars)
//!         `-- returns [messages[0], User("[Resume...]\n\n{summary}")]
//! ```
//!
//! On an LLM failure during synthesis, a fallback text is used
//! (no crash on a transient error).

use apollia_core::ORIAConfig;
use apollia_llm::{
    types::{ChatMessage, CompletionRequest, MessageContent, Role},
    LlmRouter,
};

use crate::tool_offload::{OffloadRef, ToolOffloadStore};

// ContextManager

/// Manages the LLM context window: detects threshold overruns and compacts.
///
/// Instantiated from `ORIAConfig` via [`ContextManager::from_config`].
/// Used in `BuiltInChatAgent`'s ReAct loop (iterative inference loop) to avoid
/// `context_length_exceeded` errors on long sessions.
#[derive(Debug, Clone)]
pub struct ContextManager {
    /// Fraction of `context_limit` above which compaction is triggered.
    ///
    /// `0.80` leaves 20% headroom for at least one more conversation turn.
    compact_threshold: f32,
    /// Maximum character length of the summary generated during compaction.
    ///
    /// ~1000 tokens at 4 chars/token, enough to capture the state of a complex task.
    summary_max_chars: usize,
}

impl ContextManager {
    /// Create a `ContextManager` with explicit threshold and summary budget.
    pub fn new(compact_threshold: f32, summary_max_chars: usize) -> Self {
        Self {
            compact_threshold,
            summary_max_chars,
        }
    }

    /// Create a `ContextManager` from the `[oria]` section of `apollia.toml`.
    pub fn from_config(config: &ORIAConfig) -> Self {
        Self::new(
            config.context_compact_threshold,
            config.context_summary_max_chars,
        )
    }

    /// Check whether the messages exceed the threshold and compact if needed.
    ///
    /// Returns `(resulting_messages, was_compacted)`.
    ///
    /// When `was_compacted = true`:
    /// - `result[0]`: the original `messages[0]` preserved verbatim (system prompt)
    /// - `result[1]`: `User`, an LLM summary of the whole conversation
    ///
    /// When `was_compacted = false`, the messages are returned unchanged.
    /// If `messages` is empty or has a single message, returns unchanged.
    pub async fn maybe_compact(
        &self,
        messages: &[ChatMessage],
        llm: &LlmRouter,
    ) -> (Vec<ChatMessage>, bool) {
        if messages.len() < 2 {
            return (messages.to_vec(), false);
        }

        let estimated = Self::estimate_tokens(messages);
        let limit = llm.context_limit();

        if (estimated as f32 / limit as f32) < self.compact_threshold {
            return (messages.to_vec(), false);
        }

        let summary = self.summarize(messages, llm).await;

        let compacted = vec![
            messages[0].clone(),
            ChatMessage {
                role: Role::User,
                content: MessageContent::Text(format!(
                    "[Résumé de la conversation précédente]\n\n{}",
                    summary
                )),
                cache_control: None,
            },
        ];

        (compacted, true)
    }

    /// Estimate the token count of a list of messages.
    ///
    /// Uses the `total_chars / 4 * 1.2` proxy, conservative to prefer early
    /// compaction over a context overflow.
    pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
        let total_chars: usize = messages.iter().map(message_char_len).sum();
        ((total_chars as f32) / 4.0 * 1.2) as usize
    }

    /// Offload `ToolResult` messages exceeding `threshold_chars` to disk.
    ///
    /// Returns a new message list where each oversized `ToolResult` content is
    /// written to `store` and replaced by the compact stub
    /// `[resultat deporte: <ref>, N lignes]`. Messages that are not
    /// `ToolResult`, whose content is below the threshold, or that already carry
    /// a stub are returned unchanged. On a write error the original content is
    /// kept inline and a `warn` is emitted. This method is synchronous, never
    /// panics, and never returns an error to the caller.
    pub fn offload_large_tool_results(
        &self,
        messages: &[ChatMessage],
        store: &ToolOffloadStore,
        threshold_chars: usize,
    ) -> Vec<ChatMessage> {
        messages
            .iter()
            .map(|msg| {
                let (tool_call_id, content) = match &msg.content {
                    MessageContent::ToolResult {
                        tool_call_id,
                        content,
                    } => (tool_call_id, content),
                    _ => return msg.clone(),
                };
                // Idempotence (stub already present) takes precedence over size.
                if content.starts_with("[resultat deporte:") || content.len() < threshold_chars {
                    return msg.clone();
                }
                let line_count = content.lines().count();
                let oref = OffloadRef::new_unique();
                match store.write(&oref, content) {
                    Ok(()) => ChatMessage {
                        role: msg.role.clone(),
                        content: MessageContent::ToolResult {
                            tool_call_id: tool_call_id.clone(),
                            content: format!(
                                "[resultat deporte: {}, {} lignes]",
                                oref.filename(),
                                line_count
                            ),
                        },
                        cache_control: msg.cache_control.clone(),
                    },
                    Err(e) => {
                        tracing::warn!(
                            tool_call_id = %tool_call_id,
                            error = %e,
                            "tool_offload.write_failed_keeping_inline"
                        );
                        msg.clone()
                    }
                }
            })
            .collect()
    }

    /// Generate an LLM summary of the history, truncated to `summary_max_chars`.
    ///
    /// Uses the `LlmRouter`'s default backend. On an error (network, model
    /// unavailable), returns a fallback text without propagating the error.
    async fn summarize(&self, messages: &[ChatMessage], llm: &LlmRouter) -> String {
        let history = messages
            .iter()
            .map(|m| {
                let role_str = match m.role {
                    Role::System => "System",
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                    Role::Tool => "Tool",
                };
                let preview = message_text_preview(m, 500);
                format!("{role_str}: {preview}")
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Summarize this agent conversation history concisely (max {} chars). \
             Preserve: current task objective, decisions made, files modified, pending steps.\n\n{}",
            self.summary_max_chars, history
        );

        let backend = match llm.get(None) {
            Some(b) => b,
            None => {
                tracing::warn!(
                    "no LLM backend available for context summarization, using fallback"
                );
                return fallback_summary();
            }
        };

        let max_output_tokens = ((self.summary_max_chars / 4) as u32).min(2048);

        match backend
            .complete(CompletionRequest {
                messages: vec![ChatMessage::user(prompt)],
                max_tokens: Some(max_output_tokens),
                ..Default::default()
            })
            .await
        {
            Ok(resp) => {
                let text = resp.content;
                if text.len() > self.summary_max_chars {
                    text[..self.summary_max_chars].to_owned()
                } else {
                    text
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "context summarization failed, using fallback");
                fallback_summary()
            }
        }
    }
}

// Helpers

fn fallback_summary() -> String {
    "[Résumé indisponible]\n\n[Session continue avec contexte compacté]".to_owned()
}

/// Returns the total character length of the text content in a `ChatMessage`.
pub fn message_char_len(msg: &ChatMessage) -> usize {
    match &msg.content {
        MessageContent::Text(s) => s.len(),
        MessageContent::ToolResult { content, .. } => content.len(),
        MessageContent::WithToolCalls { text, tool_calls } => {
            text.len()
                + tool_calls
                    .iter()
                    .map(|tc| tc.arguments.to_string().len())
                    .sum::<usize>()
        }
    }
}

/// Returns a text preview of a message's content, truncated to `max_chars`.
fn message_text_preview(msg: &ChatMessage, max_chars: usize) -> String {
    let full = match &msg.content {
        MessageContent::Text(s) => s.as_str(),
        MessageContent::ToolResult { content, .. } => content.as_str(),
        MessageContent::WithToolCalls { text, .. } => text.as_str(),
    };
    if full.len() > max_chars {
        format!("{}…", &full[..max_chars])
    } else {
        full.to_owned()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::{
        types::{CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage},
        CompletionModel,
    };
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::sync::Arc;

    // Mock LLM

    struct MockSummaryModel {
        response: String,
    }

    impl MockSummaryModel {
        fn with_response(text: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                response: text.into(),
            })
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockSummaryModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: self.response.clone(),
                tool_calls: vec![],
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError("mock does not stream".into()))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            "mock"
        }

        fn model_id(&self) -> &str {
            "mock-model"
        }
    }

    fn make_llm(model: Arc<dyn CompletionModel>) -> LlmRouter {
        let mut map = HashMap::new();
        map.insert("mock".to_string(), model);
        LlmRouter::with_backends(map, "mock")
    }

    // Tests

    /// GIVEN a history estimated at 60% of the window
    /// WHEN maybe_compact is called with threshold = 0.80
    /// THEN was_compacted = false and messages unchanged
    #[tokio::test]
    async fn test_no_compact_below_threshold() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000);
        let messages = vec![
            ChatMessage::system("system prompt"),
            ChatMessage::user("hi"),
        ];
        let llm = make_llm(MockSummaryModel::with_response("summary"));

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN
        assert!(!was_compacted);
        assert_eq!(result.len(), 2);
    }

    /// GIVEN a history estimated at 85% (big_content)
    /// WHEN maybe_compact is called with threshold = 0.80
    /// THEN was_compacted = true, 2 messages, system preserved, summary present
    #[tokio::test]
    async fn test_compact_above_threshold() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000);
        // 600_000 chars / 4 * 1.2 = 180_000 tokens, 90% of 200_000
        let big_content = "x".repeat(600_000);
        let messages = vec![
            ChatMessage::system("system"),
            ChatMessage::user(big_content),
        ];
        let llm = make_llm(MockSummaryModel::with_response("résumé du contexte"));

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN
        assert!(was_compacted);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, Role::System);
        let summary_text = match &result[1].content {
            MessageContent::Text(s) => s.as_str(),
            _ => panic!("expected Text"),
        };
        assert!(summary_text.contains("[Résumé de la conversation précédente]"));
        assert!(summary_text.contains("résumé du contexte"));
    }

    /// GIVEN 4000 chars of content
    /// WHEN estimate_tokens is called
    /// THEN result = 4000 / 4 * 1.2 = 1200
    #[test]
    fn test_estimate_tokens_proportional() {
        // GIVEN
        let messages = vec![ChatMessage::user("a".repeat(4000))];

        // WHEN
        let tokens = ContextManager::estimate_tokens(&messages);

        // THEN
        assert_eq!(tokens, 1200);
    }

    /// GIVEN an empty LlmRouter (no backend)
    /// WHEN maybe_compact is called on a history above the threshold
    /// THEN was_compacted = true, summary = fallback text
    #[tokio::test]
    async fn test_fallback_when_no_backend() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000);
        let big_content = "x".repeat(600_000);
        let messages = vec![
            ChatMessage::system("system"),
            ChatMessage::user(big_content),
        ];
        let llm = LlmRouter::empty();

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN
        assert!(was_compacted);
        let summary_text = match &result[1].content {
            MessageContent::Text(s) => s.as_str(),
            _ => panic!("expected Text"),
        };
        assert!(summary_text.contains("[Résumé indisponible]"));
    }

    /// GIVEN empty messages
    /// WHEN maybe_compact is called
    /// THEN was_compacted = false
    #[tokio::test]
    async fn test_empty_messages_no_compact() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000);
        let llm = LlmRouter::empty();

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&[], &llm).await;

        // THEN
        assert!(!was_compacted);
        assert!(result.is_empty());
    }

    // Tier-1 microcompaction: offload_large_tool_results

    use crate::tool_offload::ToolOffloadStore;
    use tempfile::TempDir;

    /// GIVEN a 9000-char ToolResult and a 8000-char threshold
    /// WHEN offload_large_tool_results is called
    /// THEN the message becomes a stub and the full content lands on disk
    #[test]
    fn test_offload_large_tool_result() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let store = ToolOffloadStore::new(dir.path().to_path_buf());
        let big = "x\n".repeat(4500); // 9000 chars, 4500 lines
        let messages = vec![ChatMessage::tool_result("call_01", &big)];
        let manager = ContextManager::new(0.80, 4000);

        // WHEN
        let result = manager.offload_large_tool_results(&messages, &store, 8000);

        // THEN
        assert_eq!(result.len(), 1);
        let stub = match &result[0].content {
            MessageContent::ToolResult { content, .. } => content,
            _ => panic!("expected ToolResult"),
        };
        assert!(stub.starts_with("[resultat deporte:"));
        assert!(stub.contains("4500 lignes]"));
        let entry = std::fs::read_dir(dir.path())
            .expect("readdir")
            .next()
            .expect("one offloaded file")
            .expect("dir entry");
        let written = std::fs::read_to_string(entry.path()).expect("read offloaded file");
        assert_eq!(written, big);
    }

    /// GIVEN a 100-char ToolResult below the threshold
    /// WHEN offload_large_tool_results is called
    /// THEN the message is unchanged and no file is created
    #[test]
    fn test_small_tool_result_unchanged() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let store = ToolOffloadStore::new(dir.path().to_path_buf());
        let messages = vec![ChatMessage::tool_result("call_01", &"x".repeat(100))];
        let manager = ContextManager::new(0.80, 4000);

        // WHEN
        let result = manager.offload_large_tool_results(&messages, &store, 8000);

        // THEN
        match &result[0].content {
            MessageContent::ToolResult { content, .. } => assert_eq!(content.len(), 100),
            _ => panic!("expected ToolResult"),
        }
        assert!(std::fs::read_dir(dir.path())
            .expect("readdir")
            .next()
            .is_none());
    }

    /// GIVEN a ToolResult already carrying a stub (and over threshold)
    /// WHEN offload_large_tool_results is called
    /// THEN it is returned as-is and no new file is created (idempotence)
    #[test]
    fn test_idempotent_stub_not_reprocessed() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let store = ToolOffloadStore::new(dir.path().to_path_buf());
        // Stub prefix, padded above the threshold to prove the guard wins.
        let stub = format!(
            "[resultat deporte: offload-abc.txt, 10 lignes]{}",
            " ".repeat(9000)
        );
        let messages = vec![ChatMessage::tool_result("call_01", &stub)];
        let manager = ContextManager::new(0.80, 4000);

        // WHEN
        let result = manager.offload_large_tool_results(&messages, &store, 8000);

        // THEN
        match &result[0].content {
            MessageContent::ToolResult { content, .. } => assert_eq!(content, &stub),
            _ => panic!("expected ToolResult"),
        }
        assert!(std::fs::read_dir(dir.path())
            .expect("readdir")
            .next()
            .is_none());
    }

    /// GIVEN a store whose directory does not exist (write fails)
    /// WHEN offload_large_tool_results is called on an oversized ToolResult
    /// THEN the content stays inline and no error is propagated
    #[test]
    fn test_write_error_keeps_content_inline() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let store = ToolOffloadStore::new(dir.path().join("does-not-exist"));
        let big = "x".repeat(9000);
        let messages = vec![ChatMessage::tool_result("call_01", &big)];
        let manager = ContextManager::new(0.80, 4000);

        // WHEN
        let result = manager.offload_large_tool_results(&messages, &store, 8000);

        // THEN
        match &result[0].content {
            MessageContent::ToolResult { content, .. } => assert_eq!(content, &big),
            _ => panic!("expected ToolResult"),
        }
    }

    /// GIVEN System, User, Assistant messages (none are ToolResult)
    /// WHEN offload_large_tool_results is called
    /// THEN they pass through untouched and no file is created
    #[test]
    fn test_non_tool_result_messages_unchanged() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let store = ToolOffloadStore::new(dir.path().to_path_buf());
        let big = "x".repeat(9000);
        let messages = vec![
            ChatMessage::system(big.clone()),
            ChatMessage::user(big.clone()),
            ChatMessage::assistant(big.clone()),
        ];
        let manager = ContextManager::new(0.80, 4000);

        // WHEN
        let result = manager.offload_large_tool_results(&messages, &store, 8000);

        // THEN
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0].content, MessageContent::Text(_)));
        assert!(matches!(result[1].content, MessageContent::Text(_)));
        assert!(matches!(result[2].content, MessageContent::Text(_)));
        assert!(std::fs::read_dir(dir.path())
            .expect("readdir")
            .next()
            .is_none());
    }
}
