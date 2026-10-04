use std::sync::Arc;

use apollia_core::StepBudgetConfig;
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::injection_tracker::{global_record, InjectedEntry};
use apollia_memory::store::MemoryStore;
use apollia_tools::ToolRegistryHandle;
use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::routes_agents::StubAgentLoader;
use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::chat::ChatSessionManagerHandle;

/// A daemon rooted at `dir`, whose memory holds the namespace `agent-x` with
/// one episode.
fn state_with_memory(dir: &TempDir) -> AppState<MockBackend> {
    let memory = dir.path().join("memory");
    std::fs::create_dir_all(&memory).unwrap();
    let store = MemoryStore::open(&memory.join("agent-x.db")).unwrap();
    EpisodicMemory::new(&store)
        .record(
            "agent-x",
            "agent-x",
            "Quote sent to Dupont",
            0.8,
            None,
            None,
            None,
        )
        .unwrap();
    app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    )
}

fn with_chat(mut state: AppState<MockBackend>, dir: &TempDir) -> AppState<MockBackend> {
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::empty())),
        ToolRegistryHandle::start(),
        Arc::new(StubAgentLoader),
        None,
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        state.registry_handle.clone(),
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .unwrap();
    state.chat_manager = Some(chat);
    state
}

#[tokio::test]
async fn test_namespaces_entries_and_search_are_read_from_the_daemon_memory() {
    // GIVEN a daemon whose memory holds one namespace with one episode
    let dir = TempDir::new().unwrap();
    let state = state_with_memory(&dir);

    // WHEN a client lists the namespaces, the entries of one, and searches it
    let (listed, namespaces) = call(state.clone(), "GET", "/api/v1/memory/namespaces", None).await;
    let (read, entries) = call(
        state.clone(),
        "GET",
        "/api/v1/memory/namespaces/agent-x/entries",
        None,
    )
    .await;
    let (searched, results) = call(
        state,
        "GET",
        "/api/v1/memory/namespaces/agent-x/search?q=Dupont",
        None,
    )
    .await;

    // THEN each answers what the Memory screen draws
    assert_eq!(listed, StatusCode::OK, "{namespaces}");
    assert_eq!(namespaces, serde_json::json!(["agent-x"]));
    assert_eq!(read, StatusCode::OK, "{entries}");
    assert_eq!(entries[0]["entry_type"], "episodic");
    assert_eq!(entries[0]["value"], "Quote sent to Dupont");
    assert_eq!(searched, StatusCode::OK, "{results}");
    assert_eq!(results[0]["entry_type"], "episodic");
}

#[tokio::test]
async fn test_a_namespace_that_names_a_path_is_refused_with_400() {
    // GIVEN a daemon whose memory holds one namespace
    let dir = TempDir::new().unwrap();
    let state = state_with_memory(&dir);

    // WHEN a client asks for the entries, then a search, of a namespace that
    // climbs out of the memory directory
    let (listed, refusal) = call(
        state.clone(),
        "GET",
        "/api/v1/memory/namespaces/..%2Fagent-x/entries",
        None,
    )
    .await;
    let (searched, _) = call(
        state,
        "GET",
        "/api/v1/memory/namespaces/..%2Fagent-x/search?q=x",
        None,
    )
    .await;

    // THEN both are refused with the reason, and no file is opened
    assert_eq!(listed, StatusCode::BAD_REQUEST);
    assert_eq!(
        refusal["error"],
        "VALIDATION_ERROR: invalid memory namespace '../agent-x'"
    );
    assert_eq!(searched, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_a_memory_directory_that_cannot_be_read_is_refused_with_500() {
    // GIVEN a daemon whose memory directory is a file, so it cannot be listed
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("memory"), b"not a directory").unwrap();
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );

    // WHEN a client lists the namespaces
    let (listed, refusal) = call(state, "GET", "/api/v1/memory/namespaces", None).await;

    // THEN the listing is refused with the reason, not an empty list
    assert_eq!(listed, StatusCode::INTERNAL_SERVER_ERROR, "{refusal}");
    let words = refusal["error"].as_str().unwrap();
    assert!(
        words.starts_with("failed to read memory directory: "),
        "{words}"
    );
}

#[tokio::test]
async fn test_the_entries_recalled_during_a_turn_are_read_and_an_unknown_turn_has_none() {
    // GIVEN a turn during which an agent of the daemon recalled one entry
    let dir = TempDir::new().unwrap();
    let state = state_with_memory(&dir);
    global_record(
        "turn-routes-memory",
        InjectedEntry {
            id: "sem-1".to_owned(),
            content_preview: "budget 15000".to_owned(),
            namespace: "agent-x".to_owned(),
            injection_reason: "Matched query: budget".to_owned(),
            relevance_score: 0.5,
        },
    );

    // WHEN a client reads that turn, then a turn never seen
    let (read, entries) = call(
        state.clone(),
        "GET",
        "/api/v1/memory/injections/turn-routes-memory",
        None,
    )
    .await;
    let (unknown, none) = call(
        state,
        "GET",
        "/api/v1/memory/injections/turn-routes-memory-never-seen",
        None,
    )
    .await;

    // THEN the entry comes back, and the unknown turn is an empty list
    assert_eq!(read, StatusCode::OK);
    assert_eq!(entries[0]["id"], "sem-1");
    assert_eq!(entries[0]["injection_reason"], "Matched query: budget");
    assert_eq!(unknown, StatusCode::OK);
    assert_eq!(none, serde_json::json!([]));
}

#[tokio::test]
async fn test_the_stats_of_a_new_conversation_are_read() {
    // GIVEN a daemon whose chat holds one new conversation
    let dir = TempDir::new().unwrap();
    let state = with_chat(state_with_memory(&dir), &dir);
    let (created, session) = call(
        state.clone(),
        "POST",
        "/api/v1/sessions",
        Some(serde_json::json!({ "mode": "libre" })),
    )
    .await;
    assert_eq!(created, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();

    // WHEN a client reads its statistics
    let (read, stats) = call(state, "GET", &format!("/api/v1/sessions/{id}/stats"), None).await;

    // THEN nothing is counted yet, and no profile was injected since none is open
    assert_eq!(read, StatusCode::OK, "{stats}");
    assert_eq!(stats["message_count"], 0);
    assert_eq!(stats["summarized_count"], 0);
    assert_eq!(stats["context_usage_pct"], 0.0);
    assert_eq!(stats["user_memory_injected"], false);
}

#[tokio::test]
async fn test_the_stats_of_an_unknown_conversation_are_404_and_without_chat_503() {
    // GIVEN a daemon with a chat, and one whose chat did not start
    let dir = TempDir::new().unwrap();
    let with = with_chat(state_with_memory(&dir), &dir);
    let without = state_with_memory(&TempDir::new().unwrap());

    // WHEN the statistics of a session are asked of each
    let (unknown, refusal) = call(with, "GET", "/api/v1/sessions/ghost/stats", None).await;
    let (absent, unavailable) = call(without, "GET", "/api/v1/sessions/ghost/stats", None).await;

    // THEN the first is not found, the second unavailable, each in the words
    // the desktop showed when it ran the runtime itself
    assert_eq!(unknown, StatusCode::NOT_FOUND);
    assert_eq!(refusal["error"], "NOT_FOUND: Chat session not found: ghost");
    assert_eq!(absent, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        unavailable["error"],
        "NOT_INITIALIZED: Chat subsystem is not available"
    );
}
