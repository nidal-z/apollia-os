//! Integration tests for Sprint 28 config migrations, LLM routing, MCP
//! one-shot import, and memory clear.
//!
//! Covers:
//!   - system.db creation with llm_backends + stt_config tables
//!   - LlmRouter routing by explicit backend name
//!   - LlmRouter fallback to default when no name given
//!   - LlmRouter graceful degradation on unknown backend
//!   - McpServerRepository one-shot idempotent import
//!   - MemoryStore::clear_episodic deletes entries and resets stats

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use tempfile::TempDir;

use apollia_core::{LlmBackendRepository, SttConfigRepository};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
    StreamChunk, TokenUsage,
};
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::McpServerRepository;
use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::store::MemoryStore;

// ── Mock CompletionModel ──────────────────────────────────────────────────────

struct FixedBackend {
    name: String,
    model: String,
}

#[async_trait]
impl CompletionModel for FixedBackend {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            content: format!("response from {}", self.name),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                cost_usd: None,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        Ok(Box::pin(futures::stream::empty()))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        &self.name
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

fn fixed_backend(name: &str, model: &str) -> Arc<dyn CompletionModel> {
    Arc::new(FixedBackend {
        name: name.to_owned(),
        model: model.to_owned(),
    })
}

fn mcp_server(name: &str) -> McpServerConfig {
    McpServerConfig {
        name: name.to_owned(),
        command: "npx".to_owned(),
        args: vec!["-y".to_owned(), format!("@{name}/server")],
        env: HashMap::new(),
        transport: "stdio".to_owned(),
        url: None,
        requires_approval: false,
        init_timeout_secs: 30,
        call_timeout_secs: 60,
        tags: vec![],
    }
}

// ── AC-1: system.db is created with llm_backends and stt_config tables ────────

/// GIVEN a fresh data directory
/// WHEN LlmBackendRepository and SttConfigRepository are opened on the same system.db
/// THEN both tables are present and no error is emitted
#[test]
fn test_boot_creates_system_db() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("system.db");

    LlmBackendRepository::open(&path).expect("LlmBackendRepository::open failed");
    SttConfigRepository::open(&path).expect("SttConfigRepository::open failed");

    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(
        tables.contains(&"llm_backends".to_string()),
        "llm_backends table missing; found: {tables:?}"
    );
    assert!(
        tables.contains(&"stt_config".to_string()),
        "stt_config table missing; found: {tables:?}"
    );
}

// ── AC-2: explicit backend name is routed correctly ───────────────────────────

/// GIVEN a router with "local-code" and "mistral-small" (default)
/// WHEN route(Some("local-code")) is called
/// THEN the "local-code" backend is returned
#[tokio::test]
async fn test_routing_explicit_backend() {
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "local-code".to_owned(),
        fixed_backend("local-code", "codellama-7b"),
    );
    backends.insert(
        "mistral-small".to_owned(),
        fixed_backend("mistral-small", "mistral-small-latest"),
    );

    let router = LlmRouter::with_backends(backends, "mistral-small");
    let backend = router.route(Some("local-code"));

    assert_eq!(backend.backend_name(), "local-code");
    assert_eq!(backend.model_id(), "codellama-7b");
}

// ── AC-3: None routes to default backend ─────────────────────────────────────

/// GIVEN a router with "mistral-small" marked as default
/// WHEN route(None) is called
/// THEN the "mistral-small" backend is returned
#[tokio::test]
async fn test_routing_fallback_to_default() {
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "local-code".to_owned(),
        fixed_backend("local-code", "codellama-7b"),
    );
    backends.insert(
        "mistral-small".to_owned(),
        fixed_backend("mistral-small", "mistral-small-latest"),
    );

    let router = LlmRouter::with_backends(backends, "mistral-small");
    let backend = router.route(None);

    assert_eq!(backend.backend_name(), "mistral-small");
    assert_eq!(backend.model_id(), "mistral-small-latest");
}

// ── Graceful degradation on unknown backend ───────────────────────────────────

/// GIVEN a router with "mistral-small" as default
/// WHEN route(Some("does-not-exist")) is called
/// THEN the default "mistral-small" backend is returned without panic
#[tokio::test]
async fn test_routing_unknown_backend_uses_default_with_warning() {
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "mistral-small".to_owned(),
        fixed_backend("mistral-small", "mistral-small-latest"),
    );

    let router = LlmRouter::with_backends(backends, "mistral-small");
    let backend = router.route(Some("does-not-exist"));

    assert_eq!(backend.backend_name(), "mistral-small");
}

// ── AC-4: MCP one-shot import is idempotent ───────────────────────────────────

/// GIVEN an empty mcp.db and two server configs from a TOML migration
/// WHEN import_from_toml is called once
/// THEN 2 rows are inserted; a second call returns Ok(0) without duplication
#[test]
fn test_mcp_one_shot_import() {
    let dir = TempDir::new().unwrap();
    let repo = McpServerRepository::open(&dir.path().join("mcp.db"))
        .expect("McpServerRepository::open failed");

    let configs = vec![mcp_server("notion"), mcp_server("sqlite")];

    let first = repo
        .import_from_toml(configs.clone())
        .expect("first import_from_toml failed");
    assert_eq!(first, 2, "expected 2 servers imported on empty table");
    assert_eq!(
        repo.list().expect("list failed").len(),
        2,
        "table should contain 2 servers"
    );

    let second = repo
        .import_from_toml(configs)
        .expect("second import_from_toml failed");
    assert_eq!(second, 0, "second import must be a no-op and return Ok(0)");
    assert_eq!(
        repo.list().expect("list failed").len(),
        2,
        "table should still contain exactly 2 servers"
    );
}

// ── AC-5: clear_episodic deletes all entries and resets stats ─────────────────

/// GIVEN 3 episodic entries in namespace "test-agent"
/// WHEN clear_episodic("test-agent") is called
/// THEN Ok(3) is returned and stats().episodic_count == 0
#[test]
fn test_memory_clear_episodic() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test-agent.db");
    let store = MemoryStore::open(&db_path).expect("MemoryStore::open failed");
    let ep = EpisodicMemory::new(&store);

    for i in 0..3u32 {
        ep.record(
            "test-agent",
            "agent-x",
            &format!("event {i}"),
            0.5,
            None,
            None,
            None,
        )
        .expect("record failed");
    }

    let deleted = store
        .clear_episodic("test-agent")
        .expect("clear_episodic failed");
    assert_eq!(deleted, 3, "expected 3 rows deleted");

    let stats = store.stats("test-agent", &db_path).expect("stats failed");
    assert_eq!(
        stats.episodic_count, 0,
        "episodic_count must be 0 after clear"
    );
}
