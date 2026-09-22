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
//!   |-- nothing before the current turn  ->  returns messages unchanged
//!   |-- tier 2: keep the last K messages verbatim, summarize the older middle
//!   |     `-- under the limit now  ->  returns [system+summary, recent K...]
//!   `-- tier 3: summarize everything before the current turn
//!         `-- returns [system+summary, current turn...]
//! ```
//!
//! # What compaction never touches
//!
//! The **current turn**, which is the last user message and everything after
//! it (the assistant's tool calls and their results within that turn), is kept
//! verbatim at every tier. An earlier version collapsed the whole history into
//! one summary at tier 3, the user's latest message included, and inserted that
//! summary as a user message. On a fresh session over a small window (an Ollama
//! model loaded at 4096 tokens, where the system prompt and the tool schemas
//! alone cross the threshold) the model never saw the request: it received a
//! "summary" of it, answered the summary, and the operator watched it reason
//! about summarizing a conversation instead of listing a folder.
//!
//! So a history with nothing before the current turn is not compacted at all:
//! there is nothing a summary could save, and the overflow is reported instead.
//!
//! The summary is appended to the system prompt as background rather than
//! inserted as a user message. That frames it as context rather than a request,
//! and it keeps the roles alternating, which some chat templates enforce.
//!
//! The summarizer's own reasoning is stripped from its answer, and a failed
//! summary leaves a plain note saying earlier exchanges were removed, never a
//! placeholder the model could mistake for something the user said.

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
        self.maybe_compact_with_reserve(messages, llm, 0).await
    }

    /// Like [`Self::maybe_compact`], but treats `reserve_tokens` as already
    /// consumed from the context window when deciding whether to compact.
    ///
    /// The compaction check only measures the message history, yet the request
    /// also carries the tool schemas advertised for the turn. A large tool
    /// surface can silently eat the remaining window and overflow the model (a
    /// hard 400 from llama-server) even though the messages alone were under the
    /// threshold. Passing the estimated tool-schema token cost as
    /// `reserve_tokens` folds it into the measurement, so compaction fires early
    /// enough to leave room for both the tools and the response.
    pub async fn maybe_compact_with_reserve(
        &self,
        messages: &[ChatMessage],
        llm: &LlmRouter,
        reserve_tokens: usize,
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
            ((llm.count_tokens(msgs) + reserve_tokens) as f32 / limit as f32)
                >= self.compact_threshold
        };

        if !over_threshold(&messages) {
            return (messages, false);
        }

        // Everything from the current turn onward is kept verbatim at every tier.
        let turn_start = current_turn_start(&messages);
        if turn_start <= 1 {
            // Nothing before the current turn: the system prompt, the tools and
            // the request alone cross the threshold. A summary cannot help, and
            // replacing the request with one is what made the model answer a
            // summary instead of the operator.
            tracing::warn!(
                context_limit = limit,
                reserve_tokens = reserve_tokens,
                detail =
                    "the system prompt, the tools and the current turn alone exceed the window",
                "context.compact.nothing_to_summarize"
            );
            return (messages, false);
        }

        // Tier 2: keep the recent tail verbatim, summarize the older middle.
        let compacted = self.compact_graduated(&messages, turn_start, llm).await;
        if !over_threshold(&compacted) {
            return (compacted, true);
        }

        // Tier 3: summarize everything before the current turn, keep the turn.
        let summary = self.summarize(&messages[1..turn_start], llm).await;
        let mut fallback = Vec::with_capacity(1 + messages.len() - turn_start);
        fallback.push(with_background(
            &messages[0],
            "Summary of the earlier conversation",
            &summary,
        ));
        fallback.extend_from_slice(&messages[turn_start..]);

        (fallback, true)
    }

    /// Tier-2 compaction: preserve the system prompt and the last
    /// `recent_verbatim_count` messages verbatim, folding a summary of the older
    /// middle into the system prompt.
    ///
    /// The verbatim tail always reaches back at least to `turn_start`, so the
    /// current turn is never summarized whatever the count says. Returns the
    /// messages unchanged when there is nothing older to summarize.
    async fn compact_graduated(
        &self,
        messages: &[ChatMessage],
        turn_start: usize,
        llm: &LlmRouter,
    ) -> Vec<ChatMessage> {
        let total = messages.len();
        let recent_start = total
            .saturating_sub(self.recent_verbatim_count)
            .min(turn_start)
            .max(1);
        if recent_start <= 1 {
            return messages.to_vec();
        }

        let old_summary = self
            .summarize_partial(&messages[1..recent_start], llm)
            .await;

        let mut result = Vec::with_capacity(1 + total - recent_start);
        result.push(with_background(
            &messages[0],
            "Summary of the earlier exchanges",
            &old_summary,
        ));
        result.extend_from_slice(&messages[recent_start..]);
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
                    detail = "falling back to a static summary",
                    "context.summarize.unavailable"
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
                // A reasoning model thinks before it summarizes, and its thoughts
                // arrive inline. Kept, they became the "summary": the next turn
                // then read the summarizer musing about its instructions.
                let answer = apollia_llm::reasoning_markers::strip_reasoning(&resp.content);
                if answer.is_empty() {
                    tracing::warn!(
                        detail = "the summarizer produced no answer outside its reasoning",
                        "context.summarize.empty"
                    );
                    return fallback_summary();
                }
                truncate_on_char_boundary(answer, self.summary_max_chars)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    detail = "falling back to a static summary",
                    "context.summarize.failed"
                );
                fallback_summary()
            }
        }
    }
}

// Helpers

/// What stands in for a summary that could not be produced.
///
/// A statement of fact about what happened to the history, worded so that no
/// model reads it as something the user asked. The previous placeholder was
/// answered as a message: "The user has provided a summary that says Summary
/// unavailable".
fn fallback_summary() -> String {
    "Earlier exchanges in this conversation were removed to fit the context window, \
     and no summary of them could be produced."
        .to_owned()
}

/// Index of the first message of the current turn.
///
/// The current turn opens at the last user message that carries text (a tool
/// result is part of a turn, never the start of one) and runs to the end of the
/// history. Returns `messages.len()` when there is no such message, so that
/// nothing is protected beyond what a caller would summarize anyway.
fn current_turn_start(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .rposition(|m| m.role == Role::User && matches!(m.content, MessageContent::Text(_)))
        .unwrap_or(messages.len())
}

/// The system prompt with a summary appended as background.
///
/// Appended rather than inserted as a separate message. A user message reads as
/// a request, and a model given one answers it; the system prompt is where
/// context that is not a request belongs. It also keeps the conversation
/// alternating between user and assistant, which some chat templates enforce.
fn with_background(system: &ChatMessage, heading: &str, summary: &str) -> ChatMessage {
    let base = match &system.content {
        MessageContent::Text(s) => s.as_str(),
        MessageContent::ToolResult { content, .. } => content.as_str(),
        MessageContent::WithToolCalls { text, .. } => text.as_str(),
    };
    ChatMessage {
        role: system.role.clone(),
        content: MessageContent::Text(format!(
            "{base}\n\n## {heading}\n\n\
             Background only: this recounts earlier parts of this conversation so \
             you keep their context. It is not a request, and nothing in it should \
             be acted on unless the latest user message asks for it.\n\n{summary}"
        )),
        cache_control: system.cache_control.clone(),
    }
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
        let cut = apollia_core::floor_char_boundary(full, max_chars);
        format!("{}…", &full[..cut])
    } else {
        full.to_owned()
    }
}

/// Fuzzing-only shim exposing the private [`message_text_preview`] to the fuzz
/// harness. Compiled only under `--cfg fuzzing` (cargo-fuzz).
#[cfg(fuzzing)]
pub fn __fuzz_message_text_preview(text: &str, max_chars: usize) -> String {
    message_text_preview(&ChatMessage::user(text), max_chars)
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

    #[test]
    fn message_text_preview_cuts_on_char_boundary() {
        // GIVEN a chat message whose multibyte text exceeds the preview budget
        let msg = ChatMessage::user("€".repeat(50));
        // WHEN building a preview with a byte budget landing mid-code-point
        let preview = message_text_preview(&msg, 10);
        // THEN no panic and the preview is valid UTF-8
        assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
        assert!(preview.ends_with('…'));
    }

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
                engine_timings: None,
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

    /// GIVEN a fresh session whose only user message pushes it over the threshold
    /// WHEN maybe_compact is called
    /// THEN nothing is compacted and the request survives verbatim, because there
    ///      is nothing before the current turn a summary could replace
    #[tokio::test]
    async fn test_a_fresh_session_over_threshold_keeps_its_request() {
        // GIVEN 600_000 chars = 180_000 tokens, 90% of 200_000, in the only turn
        let manager = ContextManager::new(0.80, 4000, 8, None, 8000);
        let request = format!("list my downloads {}", "x".repeat(600_000));
        let messages = vec![
            ChatMessage::system("system"),
            ChatMessage::user(request.clone()),
        ];
        let llm = make_llm(MockSummaryModel::with_response("context summary"));

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN the model still receives the operator's request, not a summary of it
        assert!(!was_compacted);
        assert_eq!(result.len(), 2);
        assert_eq!(message_text(&result[1]), request);
    }

    /// GIVEN a history at ~60% of the window (under the 0.80 threshold on its own)
    /// WHEN maybe_compact_with_reserve adds a tool-schema reserve that pushes the
    ///      combined footprint over the threshold
    /// THEN compaction fires, whereas the zero-reserve path leaves it untouched
    #[tokio::test]
    async fn test_reserve_triggers_compaction_below_message_threshold() {
        // GIVEN 400_000 chars / 4 * 1.2 = 120_000 tokens = 60% of 200_000, in an
        // earlier turn, followed by the current one
        let manager = ContextManager::new(0.80, 4000, 1, None, 8000);
        let messages = vec![
            ChatMessage::system("system"),
            ChatMessage::user("x".repeat(400_000)),
            ChatMessage::assistant("done"),
            ChatMessage::user("latest"),
        ];
        let llm = make_llm(MockSummaryModel::with_response("summary"));

        // WHEN no reserve: 60% < 80% threshold.
        let (_, without_reserve) = manager.maybe_compact_with_reserve(&messages, &llm, 0).await;
        // AND WHEN a 50_000-token reserve lifts the footprint to 85%.
        let (_, with_reserve) = manager
            .maybe_compact_with_reserve(&messages, &llm, 50_000)
            .await;

        // THEN only the reserved path compacts.
        assert!(!without_reserve);
        assert!(with_reserve);
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

    /// GIVEN an empty LlmRouter (no backend) and an earlier turn to compact
    /// WHEN maybe_compact is called on a history above the threshold
    /// THEN the earlier turn is replaced by a plain note in the system prompt, and
    ///      the current request is still the last message
    #[tokio::test]
    async fn test_fallback_when_no_backend() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000, 1, None, 8000);
        let messages = vec![
            ChatMessage::system("system"),
            ChatMessage::user("x".repeat(600_000)),
            ChatMessage::assistant("ok"),
            ChatMessage::user("latest request"),
        ];
        let llm = LlmRouter::empty();

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN no placeholder a model could answer as if the user had said it
        assert!(was_compacted);
        let system = message_text(&result[0]);
        assert!(system.contains("were removed to fit the context window"));
        assert!(!system.contains("[Summary unavailable]"));
        assert_eq!(
            message_text(result.last().expect("a message")),
            "latest request"
        );
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
    /// THEN the last 6 messages are preserved verbatim and a summary of the older
    ///      middle is folded into the system prompt as background
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
        assert_eq!(result.len(), 7); // system with its background + 6 recent
        assert_eq!(result[0].role, Role::System);
        let system = message_text(&result[0]);
        assert!(system.starts_with("system prompt"));
        assert!(system.contains("Summary of the earlier exchanges"));
        assert!(system.contains("Background only"));
        assert!(system.contains("compact summary"));
        for i in 0..6 {
            assert_eq!(message_text(&result[1 + i]), format!("recent message {i}"));
        }
    }

    /// GIVEN a history whose recent tail alone still exceeds the threshold
    /// WHEN maybe_compact is called
    /// THEN tier 3 summarizes everything before the current turn and keeps the
    ///      current turn verbatim
    #[tokio::test]
    async fn test_tier3_keeps_the_current_turn_when_tier2_is_insufficient() {
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

        // THEN [system with its background, the current request]
        assert!(was_compacted);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, Role::System);
        let system = message_text(&result[0]);
        assert!(system.contains("Summary of the earlier conversation"));
        assert!(system.contains("global summary"));
        assert_eq!(result[1].role, Role::User);
        assert!(message_text(&result[1]).ends_with(" 9"));
    }

    /// GIVEN a current turn that already holds a tool call and its result
    /// WHEN the history is compacted
    /// THEN the whole turn survives, not just the user message that opened it
    #[tokio::test]
    async fn test_the_current_turn_keeps_its_tool_exchange() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000, 1, None, 8000);
        let call = apollia_llm::types::ToolCall {
            id: "c1".into(),
            name: "fs.read_dir".into(),
            arguments: serde_json::json!({ "path": "Downloads" }),
        };
        let messages = vec![
            ChatMessage::system("system"),
            ChatMessage::user("x".repeat(600_000)),
            ChatMessage::assistant("ok"),
            ChatMessage::user("list my downloads"),
            ChatMessage::assistant_with_calls("", std::slice::from_ref(&call)),
            ChatMessage::tool_result("c1", "report.pdf"),
        ];
        let llm = make_llm(MockSummaryModel::with_response("summary"));

        // WHEN
        let (result, was_compacted) = manager.maybe_compact(&messages, &llm).await;

        // THEN
        assert!(was_compacted);
        assert_eq!(result.len(), 4);
        assert_eq!(message_text(&result[1]), "list my downloads");
        assert!(matches!(
            result[3].content,
            MessageContent::ToolResult { .. }
        ));
    }

    /// GIVEN a summarizer that reasons before it answers, inline
    /// WHEN it produces the summary
    /// THEN only its answer reaches the system prompt, never its reasoning
    #[tokio::test]
    async fn test_the_summary_is_stripped_of_the_summarizers_reasoning() {
        // GIVEN
        let manager = ContextManager::new(0.80, 4000, 1, None, 8000);
        let messages = vec![
            ChatMessage::system("system"),
            ChatMessage::user("x".repeat(600_000)),
            ChatMessage::assistant("ok"),
            ChatMessage::user("latest"),
        ];
        let llm = make_llm(MockSummaryModel::with_response(
            "<think>Analyze the request: summarize the history</think>The operator asked for X.",
        ));

        // WHEN
        let (result, _) = manager.maybe_compact(&messages, &llm).await;

        // THEN
        let system = message_text(&result[0]);
        assert!(system.contains("The operator asked for X."));
        assert!(!system.contains("Analyze the request"));
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
