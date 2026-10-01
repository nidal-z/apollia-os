#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Junction test between the `system.db` migration that removes the dedicated
//! cloud providers (writer) and the router that serves the remaining rows
//! (reader).
//!
//! The migration keeps every `openai` row that designates a self-hosted
//! server. Keeping a row is only worth something if the router still builds a
//! client for it, so the router is built here on the migrated file.

use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{LlmBackendRepository, LlmProvider};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk,
};
use apollia_llm::LlmRouter;
use futures::Stream;
use rusqlite::{params, Connection};

/// Stand-in for the embedded llama-server, which the runtime injects through
/// the override factory.
struct LocalEngine;

#[async_trait::async_trait]
impl CompletionModel for LocalEngine {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(LlmError::BackendUnavailable {
            backend: "local".into(),
            reason: "test stand-in".into(),
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        Err(LlmError::BackendUnavailable {
            backend: "local".into(),
            reason: "test stand-in".into(),
        })
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "local"
    }

    fn model_id(&self) -> &str {
        "local"
    }
}

/// The v1 `llm_backends` table, as a v1 binary created it.
const V1_LLM_BACKENDS: &str = "CREATE TABLE llm_backends (
    name         TEXT PRIMARY KEY,
    provider     TEXT NOT NULL,
    model        TEXT NOT NULL,
    config_json  TEXT NOT NULL DEFAULT '{}',
    enabled      INTEGER NOT NULL DEFAULT 1,
    is_default   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (provider IN ('llama-cpp', 'openai', 'mistral', 'anthropic', 'ollama'))
);
CREATE TABLE stt_config (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    enabled              INTEGER NOT NULL DEFAULT 0,
    model_path           TEXT    NOT NULL DEFAULT '',
    hotkey               TEXT    NOT NULL DEFAULT 'ctrl+shift+space',
    clipboard_mode       TEXT    NOT NULL DEFAULT 'paste',
    clipboard_restore    INTEGER NOT NULL DEFAULT 1,
    silence_threshold_db REAL    NOT NULL DEFAULT -40.0,
    max_recording_sec    INTEGER NOT NULL DEFAULT 60,
    language             TEXT,
    trigger_mode         TEXT    NOT NULL DEFAULT 'toggle',
    input_device         TEXT,
    updated_at           TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
PRAGMA user_version = 1;";

// GIVEN a v1 system.db holding eight backends: three cloud providers, two
//       `openai` rows on a cloud host, one `openai` row with no address, a
//       self-hosted vLLM server, a local LM Studio server and the local
//       engine, the default being a cloud row
// WHEN the repository opens the file and the router is built from it
// THEN the two self-hosted servers are kept as `openai` rows and the router
//      holds a client for each of them, next to the local engine it defaults to
#[tokio::test]
async fn test_router_serves_the_self_hosted_servers_kept_by_the_cloud_removal() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("system.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(V1_LLM_BACKENDS).unwrap();
        let rows = [
            (
                "claude",
                "anthropic",
                "claude-sonnet-4-6",
                r#"{"api_key":"k"}"#,
                1,
            ),
            ("mistral-small", "mistral", "mistral-small-latest", "{}", 0),
            (
                "gpt",
                "openai",
                "gpt-4o",
                r#"{"base_url":"https://api.openai.com/v1"}"#,
                0,
            ),
            (
                "mistral-compat",
                "openai",
                "mistral-large",
                r#"{"base_url":"https://api.mistral.ai/v1"}"#,
                0,
            ),
            (
                "no-address",
                "openai",
                "gpt-4o-mini",
                r#"{"api_key":"k"}"#,
                0,
            ),
            (
                "vllm",
                "openai",
                "qwen3-32b",
                r#"{"base_url":"http://10.0.0.5:8000/v1","api_key":"vllm-key"}"#,
                0,
            ),
            (
                "lmstudio",
                "openai",
                "gemma-3-12b",
                r#"{"base_url":"http://localhost:1234/v1"}"#,
                0,
            ),
            ("local", "llama-cpp", "/models/m.gguf", "{}", 0),
        ];
        for (name, provider, model, config_json, is_default) in rows {
            conn.execute(
                "INSERT INTO llm_backends (name, provider, model, config_json, is_default)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![name, provider, model, config_json, is_default],
            )
            .unwrap();
        }
    }

    // WHEN
    let repo = LlmBackendRepository::open(&path).unwrap();
    let router = LlmRouter::from_repository_with_override(&repo, |cfg| {
        (cfg.provider == LlmProvider::LlamaCpp)
            .then(|| Arc::new(LocalEngine) as Arc<dyn CompletionModel>)
    })
    .await
    .unwrap();

    // THEN
    let kept: Vec<(String, LlmProvider)> = repo
        .list()
        .unwrap()
        .into_iter()
        .map(|cfg| (cfg.name, cfg.provider))
        .collect();
    assert_eq!(
        kept,
        vec![
            ("lmstudio".to_string(), LlmProvider::OpenAi),
            ("local".to_string(), LlmProvider::LlamaCpp),
            ("vllm".to_string(), LlmProvider::OpenAi),
        ]
    );
    assert_eq!(
        router.backend_names(),
        vec![
            "lmstudio".to_string(),
            "local".to_string(),
            "vllm".to_string()
        ]
    );
    assert_eq!(router.default_name(), "local");
}
