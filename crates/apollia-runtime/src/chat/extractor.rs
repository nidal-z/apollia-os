//! Memory extraction from chat conversations via LLM inference.
//!
//! After a session closes (if it contained enough messages), an asynchronous
//! LLM call extracts user preferences, habits, and contextual information.
//! Extracted entries are stored in [`UserMemoryRepository`] with source
//! [`UserMemorySource::ChatInference`].

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tracing::warn;

use apollia_llm::types::ChatMessage as LlmChatMessage;
use apollia_llm::{CompletionRequest, LlmRouter, ObservabilityConfig};
use apollia_memory::user_memory::{UserMemoryCategory, UserMemoryRepository, UserMemorySource};

use super::types::ChatMessage;

/// Minimum number of messages required to trigger extraction.
const MIN_MESSAGES_FOR_EXTRACTION: usize = 4;

/// Timeout for the asynchronous extraction task.
const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Prompt sent to the LLM to extract user information from a conversation.
const EXTRACTION_PROMPT: &str = r#"Analyze this conversation and extract user preferences, habits, and context.
Return ONLY a JSON object with this structure:
{
  "preferences": [{"key": "...", "value": "..."}],
  "habits": [{"key": "...", "value": "..."}],
  "context": [{"key": "...", "value": "..."}]
}
Only include information explicitly stated or strongly implied by the user.
Do not invent or assume information not present in the conversation.
If no information can be extracted for a category, use an empty array."#;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Structured result returned by the LLM extraction call.
#[derive(Debug, Deserialize)]
pub struct ExtractionResult {
    /// User preferences detected in the conversation.
    #[serde(default)]
    pub preferences: Vec<ExtractedEntry>,
    /// User habits detected in the conversation.
    #[serde(default)]
    pub habits: Vec<ExtractedEntry>,
    /// Contextual information detected in the conversation.
    #[serde(default)]
    pub context: Vec<ExtractedEntry>,
}

/// A single key-value pair extracted from a conversation.
#[derive(Debug, Deserialize)]
pub struct ExtractedEntry {
    /// Identifier for the extracted information.
    pub key: String,
    /// Value of the extracted information.
    pub value: String,
}

/// Errors that can occur during memory extraction.
#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    /// The LLM call itself failed.
    #[error("LLM call failed: {0}")]
    LlmError(String),
    /// The LLM response could not be parsed as a valid extraction result.
    #[error("extraction parse failed: {0}")]
    ParseError(String),
    /// The conversation does not contain enough messages to warrant extraction.
    #[error("conversation too short: {message_count} messages")]
    ConversationTooShort {
        /// Number of messages in the conversation.
        message_count: usize,
    },
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Extracts user information from a conversation via an LLM call.
///
/// Converts the chat messages into an LLM prompt, sends it for completion,
/// and parses the JSON response into an [`ExtractionResult`].
///
/// Returns [`ExtractionError::ConversationTooShort`] if the conversation has
/// fewer than [`MIN_MESSAGES_FOR_EXTRACTION`] messages.
pub async fn extract_user_memory(
    messages: &[ChatMessage],
    llm: &LlmRouter,
) -> Result<ExtractionResult, ExtractionError> {
    if messages.len() < MIN_MESSAGES_FOR_EXTRACTION {
        return Err(ExtractionError::ConversationTooShort {
            message_count: messages.len(),
        });
    }

    let conversation_text = format_conversation(messages);

    let request = CompletionRequest {
        messages: vec![
            LlmChatMessage::system(EXTRACTION_PROMPT.to_string()),
            LlmChatMessage::user(conversation_text),
        ],
        ..CompletionRequest::default()
    };

    let obs = ObservabilityConfig::default();
    let response = llm
        .complete_with_observability(None, request, None, &obs)
        .await
        .map_err(|e| ExtractionError::LlmError(e.to_string()))?;

    parse_extraction_response(&response.content)
}

/// Launches a fire-and-forget extraction task after a session closes.
///
/// Spawns a Tokio task with a [`EXTRACTION_TIMEOUT`] timeout. If the
/// conversation is too short, no task is spawned. Errors are logged as
/// warnings but never propagated.
pub fn spawn_extraction(
    messages: Vec<ChatMessage>,
    llm: Arc<LlmRouter>,
    user_memory: Arc<std::sync::Mutex<UserMemoryRepository>>,
) {
    if messages.len() < MIN_MESSAGES_FOR_EXTRACTION {
        return;
    }

    tokio::spawn(async move {
        let result = tokio::time::timeout(EXTRACTION_TIMEOUT, async {
            extract_user_memory(&messages, &llm).await
        })
        .await;

        match result {
            Ok(Ok(extraction)) => {
                store_extraction(&user_memory, &extraction);
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Memory extraction failed");
            }
            Err(_) => {
                warn!("Memory extraction timed out");
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Formats chat messages into a readable conversation transcript for the LLM.
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

/// Parses the LLM response content into an [`ExtractionResult`].
///
/// Attempts to find a JSON object in the response, stripping markdown
/// code fences if present.
fn parse_extraction_response(content: &str) -> Result<ExtractionResult, ExtractionError> {
    let trimmed = content.trim();

    // Strip markdown code fences if present
    let json_str = if trimmed.starts_with("```") {
        let without_prefix = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_prefix
            .strip_suffix("```")
            .unwrap_or(without_prefix)
            .trim()
    } else {
        trimmed
    };

    serde_json::from_str(json_str)
        .map_err(|e| ExtractionError::ParseError(format!("{e}: {json_str}")))
}

/// Stores extracted entries into the user memory repository.
///
/// Each entry is upserted with [`UserMemorySource::ChatInference`].
/// Errors on individual entries are logged but do not abort the process.
fn store_extraction(
    user_memory: &std::sync::Mutex<UserMemoryRepository>,
    extraction: &ExtractionResult,
) {
    let repo = match user_memory.lock() {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Failed to acquire user memory lock for extraction");
            return;
        }
    };

    let entries: Vec<(UserMemoryCategory, &ExtractedEntry)> = extraction
        .preferences
        .iter()
        .map(|e| (UserMemoryCategory::Preferences, e))
        .chain(
            extraction
                .habits
                .iter()
                .map(|e| (UserMemoryCategory::Habits, e)),
        )
        .chain(
            extraction
                .context
                .iter()
                .map(|e| (UserMemoryCategory::Context, e)),
        )
        .collect();

    for (category, entry) in &entries {
        if let Err(e) = repo.store(
            *category,
            &entry.key,
            &entry.value,
            UserMemorySource::ChatInference,
        ) {
            warn!(
                key = %entry.key,
                error = %e,
                "Failed to store extracted memory entry"
            );
        }
    }

    let count = entries.len();
    if count > 0 {
        tracing::info!(count, "Stored extracted user memory entries from chat");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::types::ChatRole;
    use super::*;
    use apollia_llm::types::{CompletionResponse, FinishReason, TokenUsage};
    use apollia_llm::CompletionModel;
    use std::collections::HashMap;
    use std::pin::Pin;

    /// Test LLM backend that returns a fixed JSON extraction response.
    struct MockExtractionBackend {
        response: String,
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockExtractionBackend {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::LlmError> {
            Ok(CompletionResponse {
                content: self.response.clone(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    cost_usd: None,
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 10,
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
            "mock-extraction"
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
            })
            .collect()
    }

    fn make_router_with_response(json: &str) -> LlmRouter {
        let backend = MockExtractionBackend {
            response: json.to_string(),
        };
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("mock".to_string(), Arc::new(backend));
        LlmRouter::with_backends(backends, "mock")
    }

    // GIVEN a conversation with 6 messages including explicit preferences
    // WHEN extract_user_memory is called with a mock LLM
    // THEN an ExtractionResult with the correct entries is returned
    #[tokio::test]
    async fn test_extract_user_memory_valid_response() {
        let messages = make_test_messages(6);
        let json = r#"{
            "preferences": [{"key": "language", "value": "french"}],
            "habits": [],
            "context": [{"key": "role", "value": "developer"}]
        }"#;
        let router = make_router_with_response(json);

        let result = extract_user_memory(&messages, &router).await;

        let extraction = result.expect("extraction should succeed");
        assert_eq!(extraction.preferences.len(), 1);
        assert_eq!(extraction.preferences[0].key, "language");
        assert_eq!(extraction.preferences[0].value, "french");
        assert!(extraction.habits.is_empty());
        assert_eq!(extraction.context.len(), 1);
        assert_eq!(extraction.context[0].key, "role");
    }

    // GIVEN a conversation with only 2 messages
    // WHEN extract_user_memory is called
    // THEN ConversationTooShort error is returned
    #[tokio::test]
    async fn test_extract_conversation_too_short() {
        let messages = make_test_messages(2);
        let router = make_router_with_response("{}");

        let result = extract_user_memory(&messages, &router).await;

        match result {
            Err(ExtractionError::ConversationTooShort { message_count }) => {
                assert_eq!(message_count, 2);
            }
            other => panic!("expected ConversationTooShort, got {other:?}"),
        }
    }

    // GIVEN a mock LLM that returns invalid JSON
    // WHEN extract_user_memory is called
    // THEN ParseError is returned
    #[tokio::test]
    async fn test_extract_invalid_json_response() {
        let messages = make_test_messages(5);
        let router = make_router_with_response("this is not json at all");

        let result = extract_user_memory(&messages, &router).await;

        match result {
            Err(ExtractionError::ParseError(_)) => {}
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    // GIVEN a response wrapped in markdown code fences
    // WHEN parse_extraction_response is called
    // THEN the JSON is correctly parsed
    #[test]
    fn test_parse_extraction_with_code_fences() {
        let content = r#"```json
{
    "preferences": [{"key": "tone", "value": "formal"}],
    "habits": [],
    "context": []
}
```"#;

        let result = parse_extraction_response(content);
        let extraction = result.expect("should parse code-fenced JSON");
        assert_eq!(extraction.preferences.len(), 1);
        assert_eq!(extraction.preferences[0].key, "tone");
    }

    // GIVEN a conversation with varying roles
    // WHEN format_conversation is called
    // THEN the output contains role prefixes and content
    #[test]
    fn test_format_conversation() {
        let messages = vec![
            ChatMessage {
                id: "m1".to_string(),
                role: ChatRole::User,
                content: "hello".to_string(),
                tool_calls: None,
                tool_name: None,
                created_at: "2026-03-24T10:00:00Z".to_string(),
                seq: 1,
            },
            ChatMessage {
                id: "m2".to_string(),
                role: ChatRole::Assistant,
                content: "hi there".to_string(),
                tool_calls: None,
                tool_name: None,
                created_at: "2026-03-24T10:00:01Z".to_string(),
                seq: 2,
            },
        ];

        let result = format_conversation(&messages);
        assert!(result.contains("user: hello"));
        assert!(result.contains("assistant: hi there"));
    }
}
