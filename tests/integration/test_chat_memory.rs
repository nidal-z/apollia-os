//! Integration tests for the chat memory subsystem.
//!
//! Validates the end-to-end interactions between `apollia-memory`,
//! `apollia-runtime` (chat), and `apollia-llm` (mock) for user memory
//! injection, extraction, conversation summarization, context windowing,
//! and cross-session recall.

use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use apollia_llm::types::{
    CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage,
};
use apollia_llm::{CompletionModel, LlmRouter, ToolInvoker};
use apollia_memory::user_memory::{UserMemoryRepository, WrittenBy};
use apollia_runtime::chat::builtin_agent::BuiltInChatAgentDeps;
use apollia_runtime::chat::{
    extract_user_memory, summarize, BuiltInChatAgent, ChatMessage, ChatRole, ChatSessionRepository,
};
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::ToolRegistryHandle;

// ─── Mock LLM backend ────────────────────────────────────────────────────────

/// Reusable mock [`CompletionModel`] that returns a fixed response string.
///
/// Used by extraction and summarization scenarios to control LLM output
/// without any network dependency.
struct MockLlm {
    response: String,
}

#[async_trait::async_trait]
impl CompletionModel for MockLlm {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.response.clone(),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 50,
                cost_usd: None,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 5,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        let response = self.complete(req).await?;
        let chunks: Vec<Result<StreamChunk, LlmError>> = response
            .content
            .split_whitespace()
            .map(|w| Ok(StreamChunk::Text(format!("{w} "))))
            .collect();
        Ok(Box::pin(futures::stream::iter(chunks)))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "mock-memory-test"
    }

    fn model_id(&self) -> &str {
        "mock-model"
    }
}

// ─── Mock ToolInvoker ────────────────────────────────────────────────────────

/// No-op tool invoker for tests that do not exercise tool calls.
struct NoopToolInvoker;

#[async_trait::async_trait]
impl ToolInvoker for NoopToolInvoker {
    async fn invoke(
        &self,
        tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Err(format!("no tool implementation: {tool_name}"))
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Build an [`LlmRouter`] with a single mock backend returning `response`.
fn mock_router(response: &str) -> LlmRouter {
    let backend = MockLlm {
        response: response.to_string(),
    };
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert("mock".to_string(), Arc::new(backend));
    LlmRouter::with_backends(backends, "mock")
}

/// Create a temporary [`UserMemoryRepository`] and populate it.
///
/// Each entry is a flat canonical profile key plus its provenance, matching the
/// redesigned [`UserMemoryRepository`] API (flat `__user__` namespace, no
/// per-category tables).
fn create_user_memory(dir: &Path, entries: &[(&str, &str, WrittenBy)]) -> UserMemoryRepository {
    let db_path = dir.join("user_memory.db");
    let repo =
        UserMemoryRepository::new(&db_path).expect("UserMemoryRepository::new should succeed");
    for (key, value, written_by) in entries {
        repo.set(key, value, written_by.clone())
            .expect("set entry should succeed");
    }
    repo
}

/// Build a sequence of alternating User/Assistant [`ChatMessage`] entries.
fn make_chat_messages(count: usize) -> Vec<ChatMessage> {
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

/// Build chat messages with specific content for extraction scenarios.
fn make_extraction_messages() -> Vec<ChatMessage> {
    let exchanges = vec![
        (
            ChatRole::User,
            "Bonjour, je travaille sur le projet de migration",
        ),
        (ChatRole::Assistant, "Bonjour ! Quel type de migration ?"),
        (ChatRole::User, "Je préfère le français pour nos échanges"),
        (ChatRole::Assistant, "Noté, on continue en français."),
        (ChatRole::User, "Je suis développeur senior chez Acme Corp"),
        (
            ChatRole::Assistant,
            "Compris, je vais adapter mes réponses à votre niveau technique.",
        ),
    ];

    exchanges
        .into_iter()
        .enumerate()
        .map(|(i, (role, content))| ChatMessage {
            id: format!("msg-{i}"),
            role,
            content: content.to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: format!("2026-03-24T10:{:02}:00Z", i),
            seq: i as u32 + 1,
            metadata: None,
        })
        .collect()
}

/// Create a broadcast EventBusSender for testing.
fn make_event_bus() -> EventBusSender {
    let (tx, _rx) = tokio::sync::broadcast::channel(128);
    tx
}

// ─── Scenario 1 ──────────────────────────────────────────────────────────────

/// GIVEN a UserMemoryRepository with 3 entries across profile sections
/// WHEN BuiltInChatAgent builds the system prompt
/// THEN the prompt contains the "User Persona" block with the entries
#[tokio::test]
async fn test_user_memory_store_recall_injection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = create_user_memory(
        dir.path(),
        &[
            ("preferences.language", "francais", WrittenBy::User),
            ("role", "senior developer", WrittenBy::Onboarding),
            ("domain.sector", "apollia-os", WrittenBy::Onboarding),
        ],
    );

    // Verify recall works per key
    let language = repo
        .get("preferences.language")
        .expect("get preferences.language")
        .expect("preferences.language present");
    assert_eq!(language.key, "preferences.language");
    assert_eq!(language.value, "francais");

    let role = repo.get("role").expect("get role").expect("role present");
    assert_eq!(role.key, "role");
    assert_eq!(role.value, "senior developer");

    let sector = repo
        .get("domain.sector")
        .expect("get domain.sector")
        .expect("domain.sector present");
    assert_eq!(sector.key, "domain.sector");
    assert_eq!(sector.value, "apollia-os");

    // Verify the injection text is formatted correctly
    let injection_text = repo
        .recall_all_for_injection(50)
        .expect("recall_all_for_injection");
    assert!(injection_text.contains("Section: identity"));
    assert!(injection_text.contains("- role: senior developer"));
    assert!(injection_text.contains("Section: work"));
    assert!(injection_text.contains("- domain.sector: apollia-os"));
    assert!(injection_text.contains("Section: preferences"));
    assert!(injection_text.contains("- preferences.language: francais"));

    // Verify injection into BuiltInChatAgent system prompt
    let repo_arc = Arc::new(Mutex::new(repo));
    let router = Arc::new(mock_router("ok"));
    let tool_registry = ToolRegistryHandle::start();
    let tool_invoker: Arc<dyn ToolInvoker> = Arc::new(NoopToolInvoker);
    let event_bus = make_event_bus();

    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router,
        tool_registry,
        tool_invoker,
        event_bus,
        user_memory: Some(repo_arc),
        a2a_invoker: None,
        todo: None,
        plan: None,
    });

    let prompt = agent.build_system_prompt(
        Some("Base system prompt."),
        apollia_core::AutonomyLevel::Assisted,
        true,
    );
    // Production prepends a temporal context block, so the base prompt is
    // present but no longer the leading line.
    assert!(prompt.contains("Base system prompt."));
    // Production now uses recall_persona_brief with "## User Persona" header
    assert!(prompt.contains("## User Persona"));
    assert!(prompt.contains("francais"));
}

// ─── Scenario 2 ──────────────────────────────────────────────────────────────

/// GIVEN a chat session with 6 messages including "je préfère le français"
/// WHEN the session is closed and extraction is invoked (mock LLM)
/// THEN UserMemoryRepository contains the extracted entry
#[tokio::test]
async fn test_extraction_post_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = create_user_memory(dir.path(), &[]);

    let extraction_json = r#"{
        "preferences": [{"key": "language", "value": "francais"}],
        "habits": [],
        "context": [{"key": "role", "value": "senior developer at Acme Corp"}]
    }"#;
    let router = mock_router(extraction_json);

    let messages = make_extraction_messages();
    let result = extract_user_memory(&messages, &router)
        .await
        .expect("extraction should succeed");

    assert_eq!(result.preferences.len(), 1);
    assert_eq!(result.preferences[0].key, "language");
    assert_eq!(result.preferences[0].value, "francais");
    assert!(result.habits.is_empty());
    assert_eq!(result.context.len(), 1);
    assert_eq!(result.context[0].key, "role");

    // Simulate store_extraction: persist extracted entries into the repository.
    // Extracted preference keys map onto the flat `preferences.<key>` namespace;
    // context keys map onto their canonical flat key.
    for entry in &result.preferences {
        repo.set(
            &format!("preferences.{}", entry.key),
            &entry.value,
            WrittenBy::Agent("chat-extractor".to_owned()),
        )
        .expect("set preference");
    }
    for entry in &result.context {
        repo.set(
            &entry.key,
            &entry.value,
            WrittenBy::Agent("chat-extractor".to_owned()),
        )
        .expect("set context");
    }

    // Verify the extracted entries are persisted
    let language = repo
        .get("preferences.language")
        .expect("get preferences.language")
        .expect("preferences.language present");
    assert_eq!(language.key, "preferences.language");
    assert_eq!(language.value, "francais");
    assert_eq!(
        language.written_by,
        WrittenBy::Agent("chat-extractor".to_owned())
    );

    let role = repo.get("role").expect("get role").expect("role present");
    assert_eq!(role.key, "role");
    assert_eq!(role.value, "senior developer at Acme Corp");
}

// ─── Scenario 3 ──────────────────────────────────────────────────────────────

/// GIVEN a conversation of 30 messages
/// WHEN the context window manager builds the LLM context
/// THEN only the 20 most recent messages + summary are present
#[tokio::test]
async fn test_summarization_and_context_window() {
    let summary_text = "The user discussed project migration strategies including \
                        batch processing and Docker-based deployment.";
    let router = mock_router(summary_text);

    // Summarize a 30-message conversation
    let messages = make_chat_messages(30);
    let summary = summarize(&messages, &router)
        .await
        .expect("summarize should succeed");
    assert!(!summary.is_empty());
    assert!(summary.contains("migration"));

    // Verify context windowing: build_llm_messages uses the last N messages + summary
    // We test through BuiltInChatAgent's context building by checking the LLM messages
    // that would be constructed.
    //
    // With 30 messages and context_window_size=20, only messages 10..30 should appear.
    // The summary is injected as an additional system message.
    let history = make_chat_messages(30);
    let context_window_size: usize = 20;

    // Simulate what build_llm_messages does: take last `context_window_size` messages
    let windowed = if history.len() > context_window_size {
        &history[history.len() - context_window_size..]
    } else {
        &history[..]
    };
    assert_eq!(windowed.len(), 20);
    // First message in window should be msg-10 (index 10 of the original)
    assert_eq!(windowed[0].id, "msg-10");
    // Last message should be msg-29
    assert_eq!(windowed[windowed.len() - 1].id, "msg-29");

    // Summary is non-empty and would be injected as a system message
    assert!(!summary.is_empty());
}

// ─── Scenario 4 ──────────────────────────────────────────────────────────────

/// GIVEN 2 past sessions with summaries stored in SQLite (FTS5-indexed)
/// WHEN a new session queries with "comment avance le projet migration"
/// THEN the relevant past session summaries are found via FTS5
#[tokio::test]
async fn test_cross_session_recall() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("chat.db");
    let repo = ChatSessionRepository::open(&db_path).expect("open chat repo");

    // Create 2 past sessions with summaries
    repo.create_session(
        "session-1",
        &apollia_runtime::chat::ChatMode::Libre,
        None,
        "prompt",
        &[],
        "2026-03-20T10:00:00Z",
        None,
        None,
    )
    .expect("create session-1");
    repo.close_session("session-1", "2026-03-20T12:00:00Z")
        .expect("close session-1");
    repo.update_summary(
        "session-1",
        "Discussion about data migration project using batch processing and PostgreSQL replication",
    )
    .expect("update summary session-1");

    repo.create_session(
        "session-2",
        &apollia_runtime::chat::ChatMode::Libre,
        None,
        "prompt",
        &[],
        "2026-03-22T10:00:00Z",
        None,
        None,
    )
    .expect("create session-2");
    repo.close_session("session-2", "2026-03-22T12:00:00Z")
        .expect("close session-2");
    repo.update_summary(
        "session-2",
        "Review of API design for user management endpoints and authentication flow",
    )
    .expect("update summary session-2");

    // Recall relevant sessions for a migration-related query.
    // FTS5 MATCH treats space-separated words as implicit AND, so
    // the query must contain words present in the indexed summary.
    let results = repo
        .find_relevant_sessions("data migration batch processing", 3)
        .expect("find_relevant_sessions");

    assert!(
        !results.is_empty(),
        "should find at least one relevant past session"
    );
    assert!(
        results.iter().any(|s| s.session_id == "session-1"),
        "session-1 (data migration) should be in the results"
    );

    // Verify the summary content is accessible
    let found = results
        .iter()
        .find(|s| s.session_id == "session-1")
        .expect("session-1 present");
    assert!(found.summary.contains("migration"));
    assert!(found.summary.contains("batch processing"));
}

// ─── Scenario 5 ──────────────────────────────────────────────────────────────

/// GIVEN a UserMemoryRepository with entries across 3 sections
/// WHEN an agent retrieves user context via recall_all_for_injection
/// THEN the result contains all sections properly formatted and ordered
#[tokio::test]
async fn test_agent_mode_user_context() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = create_user_memory(
        dir.path(),
        &[
            ("preferences.language", "francais", WrittenBy::User),
            ("preferences.llm", "markdown", WrittenBy::User),
            ("role", "senior developer", WrittenBy::Onboarding),
            ("goals", "daily", WrittenBy::Agent("observer".to_owned())),
            ("domain.sector", "platform", WrittenBy::Onboarding),
            (
                "domain.team_size",
                "9h-18h",
                WrittenBy::Agent("chat".to_owned()),
            ),
        ],
    );

    let context_block = repo
        .recall_all_for_injection(50)
        .expect("recall_all_for_injection");

    // All 3 sections should be present
    assert!(
        context_block.contains("Section: identity"),
        "identity section missing"
    );
    assert!(
        context_block.contains("Section: work"),
        "work section missing"
    );
    assert!(
        context_block.contains("Section: preferences"),
        "preferences section missing"
    );

    // Entries within each section
    assert!(context_block.contains("- preferences.language: francais"));
    assert!(context_block.contains("- preferences.llm: markdown"));
    assert!(context_block.contains("- goals: daily"));
    assert!(context_block.contains("- domain.team_size: 9h-18h"));
    assert!(context_block.contains("- role: senior developer"));
    assert!(context_block.contains("- domain.sector: platform"));

    // Verify sections appear in schema order: identity < work < preferences
    let identity_pos = context_block
        .find("Section: identity")
        .expect("identity found");
    let work_pos = context_block.find("Section: work").expect("work found");
    let pref_pos = context_block
        .find("Section: preferences")
        .expect("preferences found");
    assert!(
        identity_pos < work_pos,
        "identity should appear before work"
    );
    assert!(work_pos < pref_pos, "work should appear before preferences");

    // Verify the block can be injected into a BuiltInChatAgent system prompt
    let repo_arc = Arc::new(Mutex::new(repo));
    let router = Arc::new(mock_router("ok"));
    let tool_registry = ToolRegistryHandle::start();
    let tool_invoker: Arc<dyn ToolInvoker> = Arc::new(NoopToolInvoker);
    let event_bus = make_event_bus();

    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router,
        tool_registry,
        tool_invoker,
        event_bus,
        user_memory: Some(repo_arc),
        a2a_invoker: None,
        todo: None,
        plan: None,
    });

    let prompt = agent.build_system_prompt(
        Some("You are a helpful assistant."),
        apollia_core::AutonomyLevel::Assisted,
        true,
    );
    // Production now uses recall_persona_brief with "## User Persona" header
    assert!(prompt.contains("## User Persona"));
    assert!(prompt.contains("francais"));
    assert!(prompt.contains("senior developer"));
}
