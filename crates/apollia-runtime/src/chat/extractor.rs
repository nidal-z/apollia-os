//! Memory extraction from chat conversations via LLM inference.
//!
//! After a session closes (if it contained enough messages), an asynchronous
//! LLM call extracts durable user information.  Extracted entries are stored
//! flat in [`UserMemoryRepository`] tagged with
//! [`WrittenBy::Agent("chat-extractor")`].
//!
//! [`UserMemoryExtractor`] adds stateful enrichment with rate limiting and
//! deduplication against existing entries.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tracing::warn;

use apollia_llm::types::ChatMessage as LlmChatMessage;
use apollia_llm::{CompletionRequest, LlmRouter, ObservabilityConfig};
use apollia_memory::user_memory::{UserMemoryRepository, WrittenBy};

/// Provenance tag used for entries derived from chat extraction.
fn chat_extractor_provenance() -> WrittenBy {
    WrittenBy::Agent("chat-extractor".to_owned())
}

use super::types::ChatMessage;

/// Minimum number of messages required to trigger basic extraction.
const MIN_MESSAGES_FOR_EXTRACTION: usize = 4;

/// Minimum number of messages required for passive enrichment extraction.
const MIN_MESSAGES_FOR_ENRICHMENT: usize = 6;

/// Timeout for the asynchronous extraction task.
const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Cooldown between two enrichment extractions (1 hour).
const EXTRACTION_COOLDOWN: Duration = Duration::from_secs(3600);

/// Prompt sent to the LLM to extract user information from a conversation.
const EXTRACTION_PROMPT: &str = r#"Analyze this conversation and extract durable user information — things that will still be true in future conversations.

Return ONLY a JSON object:
{
  "preferences": [{"key": "...", "value": "..."}],
  "habits": [{"key": "...", "value": "..."}],
  "context": [{"key": "...", "value": "..."}]
}

Rules:
- Only include information explicitly stated or strongly implied by the user.
- Focus on stable traits: role, expertise, tools, preferences, domain — not ephemeral task details.
- Use concise, specific keys (e.g. "preferred_language", "ide", "expertise_level") — not generic ones.
- Skip information that is obvious from the system context (OS, working directory).
- If nothing durable can be extracted for a category, use an empty array.
- Do not invent or assume information not present in the conversation."#;

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
#[derive(Debug, Clone, Deserialize, PartialEq)]
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
    /// A user memory operation failed.
    #[error("memory error: {0}")]
    MemoryError(String),
}

// ---------------------------------------------------------------------------
// UserMemoryExtractor, stateful enrichment with rate limiting
// ---------------------------------------------------------------------------

/// Stateful extractor that enriches user memory from chat conversations.
///
/// Provides rate limiting (at most one extraction per [`EXTRACTION_COOLDOWN`])
/// and deduplication against existing [`UserMemoryRepository`] entries.
/// Entries are stored flat with [`WrittenBy::Agent`] tagged
/// `"chat-extractor"`.
pub struct UserMemoryExtractor {
    /// Timestamp of the last successful extraction.
    last_extraction: Option<Instant>,
    /// LLM router for inference calls.
    llm_router: Arc<LlmRouter>,
    /// Shared user memory repository.
    user_memory: Arc<std::sync::Mutex<UserMemoryRepository>>,
}

impl UserMemoryExtractor {
    /// Creates a new extractor bound to the given LLM router and memory repository.
    pub fn new(
        llm_router: Arc<LlmRouter>,
        user_memory: Arc<std::sync::Mutex<UserMemoryRepository>>,
    ) -> Self {
        Self {
            last_extraction: None,
            llm_router,
            user_memory,
        }
    }

    /// Attempts to extract and persist user information from a closed session.
    ///
    /// Returns the number of new entries created. Returns `Ok(0)` without
    /// calling the LLM if the conversation is too short or if the rate
    /// limit has not elapsed.
    pub async fn extract_from_session(
        &mut self,
        messages: &[ChatMessage],
    ) -> Result<usize, ExtractionError> {
        if messages.len() < MIN_MESSAGES_FOR_ENRICHMENT {
            return Ok(0);
        }

        if let Some(last) = self.last_extraction {
            if last.elapsed() < EXTRACTION_COOLDOWN {
                tracing::debug!(
                    elapsed_secs = %last.elapsed().as_secs(),
                    cooldown_secs = %EXTRACTION_COOLDOWN.as_secs(),
                    "extraction rate limited"
                );
                return Ok(0);
            }
        }

        let extraction = extract_user_memory(messages, &self.llm_router).await?;

        let all_entries = flatten_extraction(&extraction);
        let new_entries = self.deduplicate(&all_entries)?;

        let count = self.store_new_entries(&new_entries)?;

        self.last_extraction = Some(Instant::now());
        if count > 0 {
            tracing::info!(
                entries_created = count,
                "user memory enriched from conversation"
            );
        }

        Ok(count)
    }

    /// Launches a fire-and-forget enrichment task after a session closes.
    ///
    /// Spawns a Tokio task with a [`EXTRACTION_TIMEOUT`] timeout.
    /// Errors are logged as warnings but never propagated.
    pub fn spawn_enrichment(extractor: Arc<tokio::sync::Mutex<Self>>, messages: Vec<ChatMessage>) {
        if messages.len() < MIN_MESSAGES_FOR_ENRICHMENT {
            return;
        }

        tokio::spawn(async move {
            let result = tokio::time::timeout(EXTRACTION_TIMEOUT, async {
                let mut guard = extractor.lock().await;
                guard.extract_from_session(&messages).await
            })
            .await;

            match result {
                Ok(Ok(count)) => {
                    tracing::debug!(count, "passive enrichment completed");
                }
                Ok(Err(e)) => {
                    warn!(error = %e, "passive enrichment failed");
                }
                Err(_) => {
                    warn!("passive enrichment timed out");
                }
            }
        });
    }

    /// Filters out entries that already exist with the same value.  Entries
    /// previously written by [`WrittenBy::Onboarding`] or [`WrittenBy::User`]
    /// always win, chat extraction never overwrites a higher-trust source.
    fn deduplicate(
        &self,
        entries: &[&ExtractedEntry],
    ) -> Result<Vec<ExtractedEntry>, ExtractionError> {
        let repo = self
            .user_memory
            .lock()
            .map_err(|e| ExtractionError::MemoryError(format!("lock poisoned: {e}")))?;

        let mut new_entries = Vec::new();

        for &entry in entries {
            match repo.get(&entry.key) {
                Ok(Some(existing)) if existing.value == entry.value => {
                    tracing::debug!(key = %entry.key, "duplicate skipped — same value");
                }
                Ok(Some(existing))
                    if matches!(existing.written_by, WrittenBy::Onboarding | WrittenBy::User) =>
                {
                    tracing::debug!(
                        key = %entry.key,
                        provenance = %existing.written_by.tag(),
                        "higher-trust entry exists, skipping"
                    );
                }
                Ok(_) => {
                    new_entries.push(entry.clone());
                }
                Err(e) => {
                    warn!(key = %entry.key, error = %e, "deduplication lookup failed, skipping entry");
                }
            }
        }

        Ok(new_entries)
    }

    /// Persists deduplicated entries tagged with the chat-extractor provenance.
    fn store_new_entries(&self, entries: &[ExtractedEntry]) -> Result<usize, ExtractionError> {
        let repo = self
            .user_memory
            .lock()
            .map_err(|e| ExtractionError::MemoryError(format!("lock poisoned: {e}")))?;

        let mut stored = 0;
        for entry in entries {
            if let Err(e) = repo.set(&entry.key, &entry.value, chat_extractor_provenance()) {
                warn!(key = %entry.key, error = %e, "failed to store enrichment entry");
            } else {
                stored += 1;
            }
        }

        Ok(stored)
    }
}

// ---------------------------------------------------------------------------
// Public free functions
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

/// Flattens an [`ExtractionResult`] into a flat list of entries.  The
/// preferences/habits/context buckets are a hint to the LLM about what kind
/// of information to look for; storage is flat.
fn flatten_extraction(extraction: &ExtractionResult) -> Vec<&ExtractedEntry> {
    extraction
        .preferences
        .iter()
        .chain(extraction.habits.iter())
        .chain(extraction.context.iter())
        .collect()
}

/// Stores extracted entries flat, tagged with the chat-extractor provenance.
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

    let entries = flatten_extraction(extraction);
    for entry in &entries {
        if let Err(e) = repo.set(&entry.key, &entry.value, chat_extractor_provenance()) {
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
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 10,
                ttft_ms: None,
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
                metadata: None,
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

    fn make_test_repo() -> (
        Arc<std::sync::Mutex<UserMemoryRepository>>,
        std::path::PathBuf,
    ) {
        let dir = std::env::temp_dir().join(format!("apollia_extract_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user_memory.db");
        let repo = UserMemoryRepository::new(&path).unwrap();
        (Arc::new(std::sync::Mutex::new(repo)), dir)
    }

    // -- Existing free-function tests --

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
                metadata: None,
            },
            ChatMessage {
                id: "m2".to_string(),
                role: ChatRole::Assistant,
                content: "hi there".to_string(),
                tool_calls: None,
                tool_name: None,
                created_at: "2026-03-24T10:00:01Z".to_string(),
                seq: 2,
                metadata: None,
            },
        ];

        let result = format_conversation(&messages);
        assert!(result.contains("user: hello"));
        assert!(result.contains("assistant: hi there"));
    }

    // -- UserMemoryExtractor tests --

    // GIVEN a session with 10 messages mentioning user tools
    // WHEN extract_from_session is called
    // THEN entries are created in user memory with chat_inference source and 0.5 confidence
    #[tokio::test]
    async fn test_extraction_produces_memory_entries() {
        let json = r#"{
            "preferences": [{"key": "ide", "value": "Neovim"}],
            "habits": [],
            "context": [{"key": "stack", "value": "Python"}]
        }"#;
        let router = Arc::new(make_router_with_response(json));
        let (repo, _dir) = make_test_repo();

        let mut extractor = UserMemoryExtractor::new(router, Arc::clone(&repo));
        let messages = make_test_messages(10);

        let count = extractor
            .extract_from_session(&messages)
            .await
            .expect("extraction should succeed");

        assert_eq!(count, 2);

        let guard = repo.lock().expect("lock should not be poisoned");
        let ide = guard
            .get("ide")
            .expect("get should succeed")
            .expect("ide entry should be stored");
        assert_eq!(ide.value, "Neovim");
        assert_eq!(
            ide.written_by,
            WrittenBy::Agent("chat-extractor".to_owned())
        );

        let stack = guard
            .get("stack")
            .expect("get should succeed")
            .expect("stack entry should be stored");
        assert_eq!(stack.value, "Python");
    }

    // GIVEN an existing entry with higher confidence (onboarding, confidence=1.0)
    // WHEN the extractor detects the same key
    // THEN the existing entry is not modified and no duplicate is created
    #[tokio::test]
    async fn test_duplicate_detection_skips() {
        let json = r#"{
            "preferences": [{"key": "ide", "value": "Neovim"}],
            "habits": [],
            "context": []
        }"#;
        let router = Arc::new(make_router_with_response(json));
        let (repo, _dir) = make_test_repo();

        // Pre-populate with an onboarding-tagged entry (higher trust).
        {
            let guard = repo.lock().expect("lock");
            guard
                .set("ide", "Neovim", WrittenBy::Onboarding)
                .expect("set should succeed");
        }

        let mut extractor = UserMemoryExtractor::new(router, Arc::clone(&repo));
        let messages = make_test_messages(10);

        let count = extractor
            .extract_from_session(&messages)
            .await
            .expect("extraction should succeed");

        // No new entries, existing has same value.
        assert_eq!(count, 0);

        // Verify the original entry is untouched.
        let guard = repo.lock().expect("lock");
        let ide = guard
            .get("ide")
            .expect("get")
            .expect("ide entry must remain");
        assert_eq!(ide.written_by, WrittenBy::Onboarding);
    }

    // GIVEN an extraction performed less than 1 hour ago
    // WHEN a new session closes with enough messages
    // THEN the extraction is skipped (rate limited)
    #[tokio::test]
    async fn test_rate_limiting_enforced() {
        let json = r#"{
            "preferences": [{"key": "ide", "value": "Neovim"}],
            "habits": [],
            "context": []
        }"#;
        let router = Arc::new(make_router_with_response(json));
        let (repo, _dir) = make_test_repo();

        let mut extractor = UserMemoryExtractor::new(router, Arc::clone(&repo));
        let messages = make_test_messages(10);

        // First extraction should succeed
        let first = extractor
            .extract_from_session(&messages)
            .await
            .expect("first extraction");
        assert_eq!(first, 1);

        // Second extraction immediately after should be rate limited
        let second = extractor
            .extract_from_session(&messages)
            .await
            .expect("second extraction");
        assert_eq!(second, 0);
    }

    // GIVEN a session with fewer than 6 messages
    // WHEN extract_from_session is called
    // THEN no extraction is performed
    #[tokio::test]
    async fn test_short_session_skipped() {
        let json = r#"{"preferences": [{"key": "x", "value": "y"}], "habits": [], "context": []}"#;
        let router = Arc::new(make_router_with_response(json));
        let (repo, _dir) = make_test_repo();

        let mut extractor = UserMemoryExtractor::new(router, Arc::clone(&repo));
        let messages = make_test_messages(5);

        let count = extractor
            .extract_from_session(&messages)
            .await
            .expect("should return Ok(0)");

        assert_eq!(count, 0);

        // Nothing stored
        let guard = repo.lock().expect("lock");
        assert!(guard.is_empty().expect("is_empty"));
    }
}
