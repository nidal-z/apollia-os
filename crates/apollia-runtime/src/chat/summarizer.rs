//! Conversation summarization via LLM inference.
//!
//! Produces a concise 2-3 paragraph summary of a chat conversation,
//! focusing on key decisions, established context, and unresolved items.
//! The summary is capped at [`MAX_SUMMARY_TOKENS`] to avoid overloading
//! the context window when injected into future exchanges.

use apollia_llm::types::ChatMessage as LlmChatMessage;
use apollia_llm::{CompletionRequest, LlmRouter, ObservabilityConfig};

use super::types::ChatMessage;

/// Maximum number of tokens the LLM may produce for a summary.
const MAX_SUMMARY_TOKENS: u32 = 500;

/// System prompt instructing the LLM to produce a conversation summary.
const SUMMARIZATION_PROMPT: &str = "\
Summarize this conversation in 2-3 concise paragraphs (max 500 tokens).
Focus on:
1. Key decisions made
2. Important context established
3. Unresolved questions or pending items

Do not include greetings or small talk. Be factual and precise.";

/// Errors that can occur during conversation summarization.
#[derive(Debug, thiserror::Error)]
pub enum SummarizerError {
    /// The LLM call failed.
    #[error("LLM call failed: {0}")]
    LlmError(String),
    /// The conversation is empty and cannot be summarized.
    #[error("empty conversation")]
    EmptyConversation,
}

/// Summarizes a conversation into 2-3 concise paragraphs via an LLM call.
///
/// The summary covers key decisions, established context, and unresolved
/// questions. The LLM response is capped at [`MAX_SUMMARY_TOKENS`].
///
/// Returns [`SummarizerError::EmptyConversation`] if `messages` is empty.
pub async fn summarize(
    messages: &[ChatMessage],
    llm: &LlmRouter,
) -> Result<String, SummarizerError> {
    if messages.is_empty() {
        return Err(SummarizerError::EmptyConversation);
    }

    let conversation_text = format_conversation(messages);

    let request = CompletionRequest {
        messages: vec![
            LlmChatMessage::system(SUMMARIZATION_PROMPT.to_string()),
            LlmChatMessage::user(conversation_text),
        ],
        max_tokens: Some(MAX_SUMMARY_TOKENS),
        ..CompletionRequest::default()
    };

    let obs = ObservabilityConfig::default();
    let response = llm
        .complete_with_observability(None, request, None, &obs)
        .await
        .map_err(|e| SummarizerError::LlmError(e.to_string()))?;

    Ok(response.content)
}

/// Formats chat messages into a readable transcript for the LLM.
fn format_conversation(messages: &[ChatMessage]) -> String {
    let mut buf = String::with_capacity(messages.len() * 80);
    for msg in messages {
        let role = msg.role.as_sql();
        buf.push_str(role);
        buf.push_str(": ");
        buf.push_str(&msg.content);
        buf.push('\n');
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::super::types::ChatRole;
    use super::*;
    use apollia_llm::types::{CompletionResponse, FinishReason, TokenUsage};
    use apollia_llm::CompletionModel;
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::sync::Arc;

    /// Test LLM backend that returns a fixed summary response.
    struct MockSummaryBackend {
        response: String,
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockSummaryBackend {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::LlmError> {
            Ok(CompletionResponse {
                content: self.response.clone(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 200,
                    completion_tokens: 80,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 15,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<
                Box<
                    dyn futures::Stream<
                            Item = Result<apollia_llm::StreamChunk, apollia_llm::LlmError>,
                        > + Send,
                >,
            >,
            apollia_llm::LlmError,
        > {
            Err(apollia_llm::LlmError::InferenceError(
                "stream not supported in test".into(),
            ))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            "mock-summary"
        }

        fn model_id(&self) -> &str {
            "mock-model"
        }
    }

    fn make_test_messages(count: usize) -> Vec<ChatMessage> {
        (0..count)
            .map(|i| ChatMessage {
                id: format!("msg-{i}"),
                role: if i % 2 == 0 {
                    ChatRole::User
                } else {
                    ChatRole::Assistant
                },
                content: format!("message {i}"),
                tool_calls: None,
                tool_name: None,
                created_at: "2026-03-24T10:00:00Z".to_string(),
                seq: i as u32 + 1,
                metadata: None,
            })
            .collect()
    }

    fn make_router_with_response(text: &str) -> LlmRouter {
        let backend = MockSummaryBackend {
            response: text.to_string(),
        };
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("mock".to_string(), Arc::new(backend));
        LlmRouter::with_backends(backends, "mock")
    }

    // GIVEN a conversation of 30 messages
    // WHEN summarize() is called with a mock LLM
    // THEN a non-empty summary string is returned
    #[tokio::test]
    async fn test_summarize_returns_non_empty_summary() {
        let messages = make_test_messages(30);
        let expected = "The conversation covered project setup and deployment decisions. \
                        Key decisions include using Docker for CI and SQLite for local storage.";
        let router = make_router_with_response(expected);

        let result = summarize(&messages, &router).await;

        let summary = result.expect("summarize should succeed");
        assert!(!summary.is_empty());
        assert!(summary.contains("Docker"));
        assert!(summary.contains("SQLite"));
    }

    // GIVEN an empty message list
    // WHEN summarize() is called
    // THEN EmptyConversation error is returned
    #[tokio::test]
    async fn test_summarize_empty_conversation_returns_error() {
        let messages: Vec<ChatMessage> = vec![];
        let router = make_router_with_response("should not be called");

        let result = summarize(&messages, &router).await;

        match result {
            Err(SummarizerError::EmptyConversation) => {}
            other => panic!("expected EmptyConversation, got {other:?}"),
        }
    }
}
