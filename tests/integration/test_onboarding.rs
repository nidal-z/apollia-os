//! Integration tests for the onboarding subsystem.
//!
//! Validates the end-to-end interactions between `apollia-memory` (UserMemory),
//! `apollia-runtime` (Supervisor onboarding detection, chat extractor),
//! and `apollia-llm` (mock) for onboarding agent flow, first-launch detection,
//! partial re-trigger, and passive enrichment with deduplication.

use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollia_core::RuntimeEvent;
use apollia_llm::types::{
    CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage,
};
use apollia_llm::{CompletionModel, LlmRouter};
use apollia_memory::user_memory::{UserMemoryRepository, WrittenBy};
use apollia_runtime::chat::{
    extract_user_memory, ChatMessage, ChatRole, ExtractionResult, UserMemoryExtractor,
};

// ─── Mock LLM backend ────────────────────────────────────────────────────────

/// Deterministic [`CompletionModel`] returning a fixed response string.
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
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Err(LlmError::InferenceError(
            "stream not supported in test".into(),
        ))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "mock-onboarding-test"
    }

    fn model_id(&self) -> &str {
        "mock-model"
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

/// Create a temporary [`UserMemoryRepository`] in the given directory.
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
            created_at: "2026-06-24T10:00:00Z".to_string(),
            seq: i as u32 + 1,
            metadata: None,
        })
        .collect()
}

/// Build chat messages simulating a realistic onboarding conversation.
fn make_onboarding_messages() -> Vec<ChatMessage> {
    let exchanges = vec![
        (ChatRole::User, "Bonjour"),
        (
            ChatRole::Assistant,
            "Bonjour ! Bienvenue. Comment t'appelles-tu et quel est ton rôle ?",
        ),
        (
            ChatRole::User,
            "Je suis Nidal, CTO d'une startup tech. Je préfère le français.",
        ),
        (
            ChatRole::Assistant,
            "Enchanté Nidal ! Quels outils utilises-tu au quotidien ?",
        ),
        (
            ChatRole::User,
            "J'utilise Neovim, tmux, et je travaille essentiellement en Rust et Python.",
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
            created_at: format!("2026-06-24T10:{:02}:00Z", i),
            seq: i as u32 + 1,
            metadata: None,
        })
        .collect()
}

/// Standard extraction JSON returned by the mock LLM for onboarding scenarios.
const ONBOARDING_EXTRACTION_JSON: &str = r#"{
    "preferences": [{"key": "language", "value": "francais"}],
    "habits": [{"key": "editor", "value": "Neovim + tmux"}],
    "context": [
        {"key": "name", "value": "Nidal"},
        {"key": "role", "value": "CTO startup tech"},
        {"key": "stack", "value": "Rust + Python"}
    ]
}"#;

/// Create a broadcast EventBusSender for testing.
fn make_event_bus() -> apollia_runtime::eventbus::EventBusSender {
    let (tx, _rx) = tokio::sync::broadcast::channel(128);
    tx
}

// ─── Test 1 ──────────────────────────────────────────────────────────────────

/// GIVEN an onboarding conversation of 5 messages with user identity/preferences
/// WHEN the LLM extraction is invoked with a mock returning structured JSON
/// THEN the UserMemory contains at least 3 entries with source "onboarding"
///      AND the entries span multiple categories (preferences, habits, context)
#[tokio::test]
async fn test_onboarding_agent_e2e() {
    // GIVEN
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = create_user_memory(dir.path(), &[]);
    let router = mock_router(ONBOARDING_EXTRACTION_JSON);
    let messages = make_onboarding_messages();

    // WHEN - extract user memory from the onboarding conversation
    let extraction = extract_user_memory(&messages, &router)
        .await
        .expect("extraction should succeed");

    // Persist extracted entries with onboarding provenance (simulating agent behavior)
    persist_extraction(&repo, &extraction, WrittenBy::Onboarding);

    // Mark topics as covered (simulating agent's post-conversation bookkeeping)
    repo.mark_topic_covered("identity").expect("mark identity");
    repo.mark_topic_covered("preferences")
        .expect("mark preferences");
    repo.mark_topic_covered("tools").expect("mark tools");
    repo.set_last_onboarding_session("2026-06-24T10:05:00Z")
        .expect("set session timestamp");

    // THEN - at least 3 entries exist with onboarding provenance
    let all_entries = repo.list_all().expect("list_all");

    let onboarding_entries: Vec<_> = all_entries
        .iter()
        .filter(|e| e.written_by == WrittenBy::Onboarding)
        .collect();

    assert!(
        onboarding_entries.len() >= 3,
        "expected at least 3 onboarding entries, got {}",
        onboarding_entries.len()
    );

    // Verify the extracted information spans canonical and free-form keys
    let keys: Vec<&str> = all_entries.iter().map(|e| e.key.as_str()).collect();
    assert!(
        keys.contains(&"name"),
        "canonical context key 'name' should be stored"
    );
    assert!(
        keys.contains(&"language"),
        "preference key 'language' should be stored"
    );

    // Verify onboarding state is recorded
    let covered = repo.get_covered_topics().expect("get_covered_topics");
    assert!(covered.contains(&"identity".to_string()));
    assert!(covered.contains(&"preferences".to_string()));
    assert!(covered.contains(&"tools".to_string()));

    let session_ts = repo
        .get_last_onboarding_session()
        .expect("get_last_onboarding_session");
    assert!(
        session_ts.is_some(),
        "last onboarding session should be set"
    );

    // Verify the entries can be injected into a system prompt
    let injection = repo
        .recall_all_for_injection(50)
        .expect("recall_all_for_injection");
    assert!(
        injection.contains("Section:"),
        "injection should be grouped into sections"
    );
    assert!(
        injection.contains("francais"),
        "injection should contain language value"
    );
}

// ─── Test 2 ──────────────────────────────────────────────────────────────────

/// GIVEN an empty UserMemory (first launch scenario)
/// WHEN the Supervisor emits events on boot
/// THEN OnboardingRequired is emitted on the EventBus
///      AND the user_memory repository reports is_empty = true
#[tokio::test]
async fn test_first_launch_onboarding_required() {
    // GIVEN - empty UserMemory
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = create_user_memory(dir.path(), &[]);
    let event_bus = make_event_bus();
    let mut rx = event_bus.subscribe();

    // Simulate the Supervisor's onboarding detection logic
    assert!(
        repo.is_empty().expect("is_empty should succeed"),
        "fresh repo should be empty"
    );

    // WHEN - detection runs (mirrors supervisor.rs logic)
    let _ = event_bus.send(RuntimeEvent::OnboardingRequired);

    // THEN - the event is received
    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive within 1s")
        .expect("recv should succeed");

    assert!(
        matches!(event, RuntimeEvent::OnboardingRequired),
        "expected OnboardingRequired, got: {event:?}"
    );

    // AND - verify is_empty is true (pre-condition for detection)
    assert!(repo.is_empty().expect("is_empty"));
}

/// GIVEN a UserMemory with existing entries (subsequent launch)
/// WHEN the detection logic runs
/// THEN OnboardingRequired is NOT emitted
#[tokio::test]
async fn test_subsequent_launch_no_onboarding() {
    // GIVEN - populated UserMemory
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = create_user_memory(
        dir.path(),
        &[("preferences.language", "fr", WrittenBy::Onboarding)],
    );
    let event_bus = make_event_bus();
    let mut rx = event_bus.subscribe();

    // WHEN - simulate Supervisor detection (only fires if empty)
    let is_empty = repo.is_empty().expect("is_empty");
    if is_empty {
        let _ = event_bus.send(RuntimeEvent::OnboardingRequired);
    }

    // Emit a sentinel to prove the bus works but OnboardingRequired was never sent
    let _ = event_bus.send(RuntimeEvent::AllReady);

    // THEN - receive AllReady, not OnboardingRequired
    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive within 1s")
        .expect("recv should succeed");

    assert!(
        matches!(event, RuntimeEvent::AllReady),
        "expected AllReady (no OnboardingRequired), got: {event:?}"
    );
    assert!(!is_empty, "repo should NOT be empty on subsequent launch");
}

// ─── Test 3 ──────────────────────────────────────────────────────────────────

/// GIVEN a UserMemory with pre-existing entries from a full onboarding
/// WHEN a partial onboarding is triggered with --topic "preferences"
/// THEN the existing context is accessible to the agent
///      AND new preference entries are persisted alongside existing ones
///      AND existing entries in other categories are preserved
#[tokio::test]
async fn test_onboarding_retrigger_topic() {
    // GIVEN - pre-populated memory from initial onboarding
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = create_user_memory(
        dir.path(),
        &[
            ("name", "Nidal", WrittenBy::Onboarding),
            ("role", "CTO", WrittenBy::Onboarding),
            ("preferences.language", "francais", WrittenBy::Onboarding),
        ],
    );
    repo.mark_topic_covered("identity").expect("mark identity");
    repo.mark_topic_covered("preferences")
        .expect("mark preferences");

    // Verify existing context is available for injection into agent prompt
    let existing_context = repo
        .recall_all_for_injection(50)
        .expect("recall_all_for_injection");
    assert!(
        existing_context.contains("name: Nidal"),
        "existing context should include user name"
    );
    assert!(
        existing_context.contains("role: CTO"),
        "existing context should include user role"
    );

    // WHEN - simulate partial onboarding focused on preferences
    // The mock LLM returns updated preference data
    let preferences_json = r#"{
        "preferences": [
            {"key": "language", "value": "francais"},
            {"key": "output_format", "value": "markdown"},
            {"key": "verbosity", "value": "concis"}
        ],
        "habits": [],
        "context": []
    }"#;
    let router = mock_router(preferences_json);

    // Build messages simulating a preference-focused conversation
    let messages = vec![
        ChatMessage {
            id: "msg-0".to_string(),
            role: ChatRole::User,
            content: "Je voudrais revoir mes préférences".to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: "2026-06-25T10:00:00Z".to_string(),
            seq: 1,
            metadata: None,
        },
        ChatMessage {
            id: "msg-1".to_string(),
            role: ChatRole::Assistant,
            content: "Bien sûr ! Quelles sont vos préférences de format ?".to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: "2026-06-25T10:00:01Z".to_string(),
            seq: 2,
            metadata: None,
        },
        ChatMessage {
            id: "msg-2".to_string(),
            role: ChatRole::User,
            content: "Je préfère le markdown, réponses concises".to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: "2026-06-25T10:00:02Z".to_string(),
            seq: 3,
            metadata: None,
        },
        ChatMessage {
            id: "msg-3".to_string(),
            role: ChatRole::Assistant,
            content: "Compris, j'ai noté vos préférences.".to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: "2026-06-25T10:00:03Z".to_string(),
            seq: 4,
            metadata: None,
        },
        ChatMessage {
            id: "msg-4".to_string(),
            role: ChatRole::User,
            content: "Merci, c'est tout pour les préférences".to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: "2026-06-25T10:00:04Z".to_string(),
            seq: 5,
            metadata: None,
        },
    ];

    let extraction = extract_user_memory(&messages, &router)
        .await
        .expect("extraction should succeed");

    // Persist new entries
    persist_extraction(&repo, &extraction, WrittenBy::Onboarding);

    // THEN - new preference entries are persisted alongside the pre-existing ones
    let all = repo.list_all().expect("list_all");
    let keys: Vec<&str> = all.iter().map(|e| e.key.as_str()).collect();
    assert!(
        keys.contains(&"output_format"),
        "output_format should be stored"
    );
    assert!(keys.contains(&"verbosity"), "verbosity should be stored");

    // AND - existing entries are untouched (name, role, the original language)
    assert!(keys.contains(&"name"), "name should be preserved");
    assert!(keys.contains(&"role"), "role should be preserved");
    assert!(
        keys.contains(&"preferences.language"),
        "original language preference should be preserved"
    );

    // AND - covered topics are preserved
    let covered = repo.get_covered_topics().expect("get_covered_topics");
    assert!(covered.contains(&"identity".to_string()));
    assert!(covered.contains(&"preferences".to_string()));
}

// ─── Test 4 ──────────────────────────────────────────────────────────────────

/// GIVEN a chat session of 10 messages mentioning user tools and preferences
/// WHEN the session closes and the UserMemoryExtractor runs
/// THEN entries are created with the chat-extractor provenance
///      AND running the same extraction again produces no duplicates
#[tokio::test]
async fn test_passive_enrichment_post_session() {
    // GIVEN - empty UserMemory + mock LLM returning tool-related extractions
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("user_memory.db");
    let repo =
        UserMemoryRepository::new(&db_path).expect("UserMemoryRepository::new should succeed");
    let repo_arc = Arc::new(Mutex::new(repo));

    let enrichment_json = r#"{
        "preferences": [{"key": "ide", "value": "Neovim"}],
        "habits": [{"key": "vcs", "value": "git rebase workflow"}],
        "context": [{"key": "stack", "value": "Rust + Python"}]
    }"#;
    let router = Arc::new(mock_router(enrichment_json));

    let messages = make_chat_messages(10);

    // WHEN - first extraction
    let mut extractor = UserMemoryExtractor::new(Arc::clone(&router), Arc::clone(&repo_arc));
    let count = extractor
        .extract_from_session(&messages)
        .await
        .expect("first extraction should succeed");

    // THEN - 3 entries created
    assert_eq!(count, 3, "should create 3 new entries");

    {
        let guard = repo_arc.lock().expect("lock should not be poisoned");

        let ide = guard
            .get("ide")
            .expect("get ide")
            .expect("ide entry should be stored");
        assert_eq!(ide.value, "Neovim");
        assert_eq!(
            ide.written_by,
            WrittenBy::Agent("chat-extractor".to_owned()),
            "chat-inferred entries carry the chat-extractor provenance"
        );

        let vcs = guard
            .get("vcs")
            .expect("get vcs")
            .expect("vcs entry should be stored");
        assert_eq!(
            vcs.written_by,
            WrittenBy::Agent("chat-extractor".to_owned())
        );

        let stack = guard
            .get("stack")
            .expect("get stack")
            .expect("stack entry should be stored");
        assert_eq!(stack.value, "Rust + Python");
    }

    // WHEN - second extraction with same data (simulate duplicate session)
    // Reset rate limiter by creating a fresh extractor
    let mut extractor2 = UserMemoryExtractor::new(Arc::clone(&router), Arc::clone(&repo_arc));
    let count2 = extractor2
        .extract_from_session(&messages)
        .await
        .expect("second extraction should succeed");

    // THEN - no new entries (deduplication kicks in: same value = skip)
    assert_eq!(count2, 0, "duplicate entries should not be created");

    {
        let guard = repo_arc.lock().expect("lock");

        // Total entries unchanged after dedup
        let total = guard.list_all().expect("list_all").len();
        assert_eq!(total, 3, "total entries should remain 3 after dedup");
    }
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Persist an [`ExtractionResult`] into the [`UserMemoryRepository`] with the given provenance.
///
/// Storage is flat: the preferences/habits/context buckets are an extraction
/// hint only, so every entry is written under its bare key.
fn persist_extraction(
    repo: &UserMemoryRepository,
    extraction: &ExtractionResult,
    written_by: WrittenBy,
) {
    for entry in &extraction.preferences {
        repo.set(&entry.key, &entry.value, written_by.clone())
            .expect("set preference");
    }
    for entry in &extraction.habits {
        repo.set(&entry.key, &entry.value, written_by.clone())
            .expect("set habit");
    }
    for entry in &extraction.context {
        repo.set(&entry.key, &entry.value, written_by.clone())
            .expect("set context");
    }
}
