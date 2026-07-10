//! `ContextManager`: automatic management of the LLM context window.
//!
//! Detects when the conversation history approaches the model limit and compacts
//! it through a graduated pipeline. The original system prompt (messages\[0\]) is
//! always preserved.
//!
//! ## Graduated compaction
//!
//! ```text
//! maybe_compact(&messages, &llm)
//!   |-- tier 1: offload oversized tool results to disk (when a store is set)
//!   |-- count_tokens / context_limit < threshold  ->  returns messages unchanged
//!   |-- tier 2: keep the last K messages verbatim, summarize the older middle
//!   |     `-- under the limit now  ->  returns [system, summary, recent K...]
//!   `-- tier 3: still over  ->  replace everything by a single global summary
//! ```
//!
//! On an LLM failure during synthesis, a fallback text is used, so a transient
//! error never crashes the loop.

use std::sync::Arc;

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
    /// Number of most recent messages kept verbatim during tier-2 compaction.
    ///
    /// Only the older middle of the history is summarized. The system prompt
    /// (messages\[0\]) is always preserved on top of this count.
    recent_verbatim_count: usize,
    /// Optional store enabling tier-1 offload of oversized tool results.
    ///
    /// When `None`, tier 1 is skipped and the pipeline starts at the threshold
    /// check. Injected via [`ContextManager::with_tool_offload`].
    tool_offload: Option<Arc<ToolOffloadStore>>,
    /// Character length above which a `ToolResult` content is offloaded to disk.
    tool_offload_threshold_chars: usize,
}

impl ContextManager {
    /// Create a `ContextManager` with explicit threshold and budgets.
    ///
    /// `tool_offload` is the optional tier-1 store; pass `None` to skip offload.
    /// `tool_offload_threshold_chars` is the per-`ToolResult` size above which a
    /// result is written to disk. `recent_verbatim_count` is the number of trailing
    /// messages kept verbatim during tier-2 compaction.
    pub fn new(
        compact_threshold: f32,
        summary_max_chars: usize,
        recent_verbatim_count: usize,
        tool_offload: Option<Arc<ToolOffloadStore>>,
        tool_offload_threshold_chars: usize,
    ) -> Self {
        Self {
            compact_threshold,
            summary_max_chars,
            recent_verbatim_count,
            tool_offload,
            tool_offload_threshold_chars,
        }
    }

    /// Create a `ContextManager` from the `[oria]` section of `apollia.toml`.
    ///
    /// The tier-1 store is left unset (`None`): it requires a workspace path
    /// resolved at agent startup and is injected later via
    /// [`ContextManager::with_tool_offload`].
    pub fn from_config(config: &ORIAConfig) -> Self {
        Self::new(
            config.context_compact_threshold,
            config.context_summary_max_chars,
            config.recent_verbatim_count,
            None,
            config.tool_offload_threshold_chars,
        )
    }

    /// Inject the tier-1 offload store, enabling oversized tool-result offload.
    pub fn with_tool_offload(mut self, store: Arc<ToolOffloadStore>) -> Self {
        self.tool_offload = Some(store);
        self
    }

    /// Run the graduated compaction pipeline and return `(messages, was_compacted)`.
    ///
    /// Stages:
    /// - Tier 1: when a tool-offload store is configured, oversized `ToolResult`
    ///   contents are written to disk and replaced by a compact stub.
    /// - Threshold check on the real token count: under it, the (possibly
    ///   offloaded) messages are returned with `was_compacted = false`.
    /// - Tier 2: the system prompt and the last `recent_verbatim_count` messages
    ///   are kept verbatim; the older middle is replaced by one summary message.
    /// - Tier 3: if tier 2 is still over the limit, the whole history is replaced
    ///   by a single global summary (`[system, summary]`).
    ///
    /// When `was_compacted = false`, `result[0]` is always the original
    /// `messages[0]`. If `messages` is empty or has a single message, it is
    /// returned unchanged.
    pub async fn maybe_compact(
        &self,
        messages: &[ChatMessage],
        llm: &LlmRouter,
    ) -> (Vec<ChatMessage>, bool) {
        if messages.len() < 2 {
            return (messages.to_vec(), false);
        }

        // Tier 1: offload oversized tool results before measuring the window.
        let messages: Vec<ChatMessage> = match &self.tool_offload {
            Some(store) => {
                self.offload_large_tool_results(messages, store, self.tool_offload_threshold_chars)
            }
            None => messages.to_vec(),
        };

        let limit = llm.context_limit();
        let over_threshold = |msgs: &[ChatMessage]| {
            (llm.count_tokens(msgs) as f32 / limit as f32) >= self.compact_threshold
        };

        if !over_threshold(&messages) {
            return (messages, false);
        }

        // Tier 2: keep the recent tail verbatim, summarize the older middle.
        let compacted = self.compact_graduated(&messages, llm).await;
        if !over_threshold(&compacted) {
            return (compacted, true);
        }

        // Tier 3: collapse the whole history into a single global summary.
        let summary = self.summarize(&messages, llm).await;
        let fallback = vec![
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

        (fallback, true)
    }

    /// Tier-2 compaction: preserve the system prompt and the last
    /// `recent_verbatim_count` messages verbatim, replacing the older middle with
    /// a single summary message.
    ///
    /// Returns the messages unchanged when there are not enough older messages to
    /// be worth summarizing (`len <= recent_verbatim_count + 1`).
    async fn compact_graduated(
        &self,
        messages: &[ChatMessage],
        llm: &LlmRouter,
    ) -> Vec<ChatMessage> {
        let total = messages.len();
        if total <= self.recent_verbatim_count + 1 {
            return messages.to_vec();
        }

        let recent_start = total - self.recent_verbatim_count;
        let system = &messages[0];
        let old_messages = &messages[1..recent_start];
        let recent_messages = &messages[recent_start..];

        let old_summary = if old_messages.is_empty() {
            String::new()
        } else {
            self.summarize_partial(old_messages, llm).await
        };

        let mut result = Vec::with_capacity(2 + recent_messages.len());
        result.push(system.clone());
        if !old_summary.is_empty() {
            result.push(ChatMessage {
                role: Role::User,
                content: MessageContent::Text(format!(
                    "[Résumé des échanges précédents]\n\n{}",
                    old_summary
                )),
                cache_control: None,
            });
        }
        result.extend_from_slice(recent_messages);
        result
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

    /// Generate a global LLM summary of the whole history (tier 3), truncated to
    /// `summary_max_chars`.
    ///
    /// Uses the `LlmRouter`'s default backend. On an error (network, model
    /// unavailable), returns a fallback text without propagating the error.
    async fn summarize(&self, messages: &[ChatMessage], llm: &LlmRouter) -> String {
        let prompt = format!(
            "Summarize this agent conversation history concisely (max {} chars). \
             Preserve: current task objective, decisions made, files modified, pending steps.\n\n{}",
            self.summary_max_chars,
            render_history(messages),
        );
        self.run_summary(prompt, llm).await
    }

    /// Summarize an older segment of the history (tier 2), truncated to
    /// `summary_max_chars`.
    ///
    /// The prompt is tuned to preserve the operational state needed to keep
    /// reasoning on the recent verbatim tail: active task, key decisions, files
    /// modified, errors and their fixes, and pending next steps. Falls back to a
    /// placeholder on backend error; never panics.
    async fn summarize_partial(&self, messages: &[ChatMessage], llm: &LlmRouter) -> String {
        let prompt = format!(
            "Summarize this earlier segment of an agent conversation concisely (max {} chars). \
             Preserve: the active task objective, key decisions made, files modified, \
             errors encountered and their fixes, and pending next steps.\n\n{}",
            self.summary_max_chars,
            render_history(messages),
        );
        self.run_summary(prompt, llm).await
    }

    /// Run a summarization `prompt` against the default backend and truncate the
    /// output to `summary_max_chars` (on a UTF-8 boundary). Returns a fallback
    /// text when no backend is available or the call fails.
    async fn run_summary(&self, prompt: String, llm: &LlmRouter) -> String {
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
            Ok(resp) => truncate_on_char_boundary(resp.content, self.summary_max_chars),
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

/// Render messages as a role-prefixed, length-capped transcript for summarization.
fn render_history(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|m| {
            let role_str = match m.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };
            format!("{role_str}: {}", message_text_preview(m, 500))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Truncate `s` to at most `max_bytes`, never splitting a UTF-8 code point.
fn truncate_on_char_boundary(s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
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
        // Truncate on a UTF-8 char boundary so multibyte input (emoji, CJK)
        // never panics on a byte-slice that splits a code point.
        format!("{}…", truncate_on_char_boundary(full.to_owned(), max_chars))
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

    fn message_text(msg: &ChatMessage) -> &str {
        match &msg.content {
            MessageContent::Text(s) => s.as_str(),
            MessageContent::ToolResult { content, .. } => content.as_str(),
            MessageContent::WithToolCalls { text, .. } => text.as_str(),
        }
    }

    // Tests

    /// GIVEN a history estimated at 60% of the window
    /// WHEN maybe_compact is called with threshold = 0.80
    /// THEN was_compacted = false and messages unchanged
    #[tokio::test]
    async fn test_no_compact_below_threshold() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);
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
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);
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
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);
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
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);
        let llm = LlmRouter::empty();

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&[], &llm).await;

        // THEN
        assert!(!was_compacted);
        assert!(result.is_empty());
    }

    // Tier-2 graduated compaction

    /// GIVEN a 20-message history above threshold and recent_verbatim_count = 6
    /// WHEN maybe_compact is called
    /// THEN the system prompt and the last 6 messages are preserved verbatim and
    ///      the older middle is replaced by a single summary message
    #[tokio::test]
    async fn test_tier2_keeps_recent_verbatim_and_summarizes_old() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000, 6, None, 8000);
        let mut messages = vec![ChatMessage::system("system prompt")];
        // 13 large older messages push the history over the 80% threshold.
        for i in 0..13 {
            messages.push(ChatMessage::user(format!("{} {}", "x".repeat(45_000), i)));
        }
        // 6 small recent messages must survive verbatim.
        for i in 0..6 {
            messages.push(ChatMessage::user(format!("recent message {i}")));
        }
        let llm = make_llm(MockSummaryModel::with_response("compact summary"));

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN
        assert!(was_compacted);
        assert_eq!(result.len(), 8); // system + summary + 6 recent
        assert_eq!(result[0].role, Role::System);
        assert_eq!(message_text(&result[0]), "system prompt");
        let summary_text = message_text(&result[1]);
        assert!(summary_text.contains("[Résumé des échanges précédents]"));
        assert!(summary_text.contains("compact summary"));
        for i in 0..6 {
            assert_eq!(message_text(&result[2 + i]), format!("recent message {i}"));
        }
    }

    /// GIVEN a history whose recent tail alone still exceeds the threshold
    /// WHEN maybe_compact is called
    /// THEN tier 3 collapses everything to [system, global summary]
    #[tokio::test]
    async fn test_tier3_fallback_when_tier2_insufficient() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000, 6, None, 8000);
        let mut messages = vec![ChatMessage::system("system")];
        // Even the recent 6 messages are huge, so tier 2 cannot get under the limit.
        for i in 0..10 {
            messages.push(ChatMessage::user(format!("{} {}", "y".repeat(100_000), i)));
        }
        let llm = make_llm(MockSummaryModel::with_response("global summary"));

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN
        assert!(was_compacted);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, Role::System);
        let summary_text = message_text(&result[1]);
        assert!(summary_text.contains("[Résumé de la conversation précédente]"));
        assert!(summary_text.contains("global summary"));
    }

    /// GIVEN a history that drops below threshold once its big tool result is offloaded
    /// WHEN maybe_compact runs with a tool-offload store injected
    /// THEN tier 1 offload happens, no tier 2/3 compaction, was_compacted = false
    #[tokio::test]
    async fn test_tier1_offload_below_threshold_no_compaction() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let store = Arc::new(ToolOffloadStore::new(dir.path().to_path_buf()));
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000).with_tool_offload(store);
        // One oversized tool result drives the pre-offload history over the threshold.
        let big = "z".repeat(700_000);
        let messages = vec![
            ChatMessage::system("system"),
            ChatMessage::tool_result("call_01", &big),
        ];
        let llm = make_llm(MockSummaryModel::with_response("unused"));

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN
        assert!(!was_compacted);
        assert_eq!(result.len(), 2);
        let stub = match &result[1].content {
            MessageContent::ToolResult { content, .. } => content,
            _ => panic!("expected ToolResult"),
        };
        assert!(stub.starts_with("[resultat deporte:"));
        assert!(std::fs::read_dir(dir.path())
            .expect("readdir")
            .next()
            .is_some());
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
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);

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

    /// GIVEN a multibyte string and a byte budget that lands inside a code point
    /// WHEN truncate_on_char_boundary is called
    /// THEN it backs off to the nearest boundary instead of panicking
    #[test]
    fn test_truncate_on_char_boundary_never_splits_codepoint() {
        // GIVEN "ab" (2 bytes) followed by a 4-byte emoji at bytes 2..6
        let s = "ab😀cd".to_string();

        // WHEN truncating to 4 bytes, which falls in the middle of the emoji
        let out = truncate_on_char_boundary(s, 4);

        // THEN it backs off to byte 2, the boundary before the emoji
        assert_eq!(out, "ab");
    }

    /// GIVEN a tool-result message whose content is entirely multibyte
    /// WHEN message_text_preview truncates it at a mid-code-point offset
    /// THEN it yields a boundary-safe preview and never panics
    #[test]
    fn test_message_text_preview_truncates_multibyte_without_panic() {
        // GIVEN a message of ten 4-byte emoji (40 bytes)
        let msg = ChatMessage::tool_result("c1", &"😀".repeat(10));

        // WHEN previewed with a byte budget of 5 (mid-emoji)
        let preview = message_text_preview(&msg, 5);

        // THEN it truncates on the boundary before the second emoji, plus ellipsis
        assert_eq!(preview, "😀…");
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
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);

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
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);

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
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);

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
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);

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
