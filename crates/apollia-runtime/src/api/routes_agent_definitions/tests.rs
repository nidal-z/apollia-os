use std::sync::Arc;

use apollia_core::model_preset::{AgentModel, ModelSource};
use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
use apollia_tools::{AgentOrigin, StoredAgent};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::*;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::governance::governance_run_id;

const KEY: &[u8] = b"agent-definitions-journal-key-01";

async fn call(
    state: AppState<MockBackend>,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let app = agent_definition_routes::<MockBackend>().with_state(state);
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(match body {
            Some(b) => Body::from(serde_json::to_vec(&b).unwrap()),
            None => Body::empty(),
        })
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A runtime over a data directory holding one agent, `invoices`, assigned a
/// local model in `system.db`; with a signed journal when `journal` is set.
async fn state(
    dir: &tempfile::TempDir,
    journal: bool,
) -> (AppState<MockBackend>, Option<AuditJournalHandle>) {
    let agents =
        AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path())).unwrap();
    agents
        .save_definition(&StoredAgent {
            name: "invoices".into(),
            origin: AgentOrigin::Package,
            package: Some("office".into()),
            version: "1.0.0".into(),
            definition: json!({
                "id": "invoices",
                "name": "Invoices",
                "mission": "Extract the totals.",
                "defaults": {"access": ["file_read"]}
            }),
            enabled: true,
            installed_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
        })
        .unwrap();
    let llm = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
    llm.save(&LlmBackendConfig {
        name: "local".into(),
        provider: LlmProvider::LlamaCpp,
        model: "/models/qwen3-8b.gguf".into(),
        config_json: json!({}),
        enabled: true,
        is_default: true,
    })
    .unwrap();
    llm.set_agent_model(&AgentModel {
        agent: "invoices".into(),
        model_source: ModelSource::Local,
        model_id: "qwen3-8b".into(),
        preset_id: None,
    })
    .unwrap();
    let mut state = test_app_state_no_llm();
    state.data_dir = dir.path().to_path_buf();
    state.llm_backend_repo = Some(Arc::new(std::sync::Mutex::new(llm)));
    let handle = if journal {
        let handle =
            AuditJournalHandle::open_with_key_bytes(&dir.path().join("journal.db"), KEY.to_vec())
                .await
                .unwrap();
        state.audit_journal = Some(handle.clone());
        state.governance = Some(crate::governance::GovernanceHandle::detached(
            crate::governance::GovernanceState::default(),
            Some(handle.clone()),
        ));
        Some(handle)
    } else {
        None
    };
    (state, handle)
}

// GIVEN a stored agent with a model assigned in system.db
// WHEN the agents are listed and the agent is read
// THEN both answer the agent resolved, the model read from agent_model
#[tokio::test]
async fn test_list_and_read_resolved_agent() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, _) = state(&dir, false).await;

    // WHEN
    let (listed, list) = call(state.clone(), "GET", "/api/v1/agent-definitions", None).await;
    let (read, agent) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-definitions/invoices",
        None,
    )
    .await;
    let (missing, _) = call(state, "GET", "/api/v1/agent-definitions/ghost", None).await;

    // THEN
    assert_eq!(listed, StatusCode::OK);
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["apollia:chat", "invoices"]);
    assert_eq!(read, StatusCode::OK);
    assert_eq!(agent["mission"], "Extract the totals.");
    assert_eq!(agent["origin"], "package");
    assert_eq!(agent["package"], "office");
    assert_eq!(agent["model"]["model_id"], "qwen3-8b");
    assert_eq!(agent["overridden"], json!([]));
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

// GIVEN a stored agent with a model assigned, and a signed journal
// WHEN its access is overridden, its mission overridden then removed, and the
//      agent sent back to defaults
// THEN each answer is the agent in force, back to defaults its model assignment
//      is gone too, and the journal of the agent holds one record per write,
//      plus the access granted and revoked, plus the model removed
#[tokio::test]
async fn test_override_routes_write_and_journal() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, journal) = state(&dir, true).await;
    let journal = journal.unwrap();

    // WHEN
    let (set_access, after_access) = call(
        state.clone(),
        "PUT",
        "/api/v1/agent-definitions/invoices/overrides/access",
        Some(json!({"value": ["web_fetch"]})),
    )
    .await;
    let (set_mission, _) = call(
        state.clone(),
        "PUT",
        "/api/v1/agent-definitions/invoices/overrides/mission",
        Some(json!({"value": "Mine."})),
    )
    .await;
    let (removed, after_remove) = call(
        state.clone(),
        "DELETE",
        "/api/v1/agent-definitions/invoices/overrides/mission",
        None,
    )
    .await;
    let (reset, after_reset) = call(
        state,
        "DELETE",
        "/api/v1/agent-definitions/invoices/overrides",
        None,
    )
    .await;

    // THEN
    assert_eq!(set_access, StatusCode::OK);
    assert_eq!(after_access["access"], json!(["web_fetch"]));
    assert_eq!(after_access["overridden"], json!(["access"]));
    assert_eq!(set_mission, StatusCode::OK);
    assert_eq!(removed, StatusCode::OK);
    assert_eq!(after_remove["mission"], "Extract the totals.");
    assert_eq!(reset, StatusCode::OK);
    assert_eq!(after_reset["access"], json!(["file_read"]));
    assert_eq!(after_reset["overridden"], json!([]));
    assert_eq!(after_reset["model"], Value::Null);
    let entries = journal.query_run(&governance_run_id("invoices")).await;
    assert_eq!(
        entries.last().map(|entry| entry.payload["field"].clone()),
        Some(json!("model"))
    );
    let kinds: Vec<JournalEntryKind> = entries.into_iter().map(|entry| entry.kind).collect();
    assert_eq!(
        kinds,
        vec![
            // access set: the override, web_fetch granted, file_read revoked
            JournalEntryKind::AgentOverrideChanged,
            JournalEntryKind::AccessChanged,
            JournalEntryKind::AccessChanged,
            // mission set, then removed
            JournalEntryKind::AgentOverrideChanged,
            JournalEntryKind::AgentOverrideChanged,
            // back to defaults: access removed, file_read granted, web_fetch revoked
            JournalEntryKind::AgentOverrideChanged,
            JournalEntryKind::AccessChanged,
            JournalEntryKind::AccessChanged,
            // and the model assigned in system.db removed
            JournalEntryKind::AgentOverrideChanged,
        ]
    );
}

// GIVEN a stored agent
// WHEN an unknown field, a value of the wrong shape, an unknown agent, and a
//      write without a journal are sent
// THEN they answer 422, 422, 404 and 503, and nothing is stored
#[tokio::test]
async fn test_override_routes_refuse() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (with_journal, _) = state(&dir, true).await;
    let other = tempfile::tempdir().unwrap();
    let (without_journal, _) = state(&other, false).await;

    // WHEN
    let (unknown_field, _) = call(
        with_journal.clone(),
        "PUT",
        "/api/v1/agent-definitions/invoices/overrides/model",
        Some(json!({"value": "x"})),
    )
    .await;
    let (bad_value, _) = call(
        with_journal.clone(),
        "PUT",
        "/api/v1/agent-definitions/invoices/overrides/access",
        Some(json!({"value": "file_read"})),
    )
    .await;
    let (unknown_agent, _) = call(
        with_journal.clone(),
        "PUT",
        "/api/v1/agent-definitions/ghost/overrides/mission",
        Some(json!({"value": "x"})),
    )
    .await;
    let (no_journal, _) = call(
        without_journal.clone(),
        "PUT",
        "/api/v1/agent-definitions/invoices/overrides/mission",
        Some(json!({"value": "x"})),
    )
    .await;

    // THEN
    assert_eq!(unknown_field, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad_value, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(unknown_agent, StatusCode::NOT_FOUND);
    assert_eq!(no_journal, StatusCode::SERVICE_UNAVAILABLE);
    let (_, agent) = call(
        with_journal,
        "GET",
        "/api/v1/agent-definitions/invoices",
        None,
    )
    .await;
    assert_eq!(agent["overridden"], json!([]));
    let (_, untouched) = call(
        without_journal,
        "GET",
        "/api/v1/agent-definitions/invoices",
        None,
    )
    .await;
    assert_eq!(untouched["mission"], "Extract the totals.");
}

// GIVEN the Apollia agent every agents.db holds, and a signed journal
// WHEN the settings of the chat panel are written as the routes take them,
//      then read back
// THEN Apollia is an agent of origin interface, and the panel reads back the
//      prompt and tools it wrote
#[tokio::test]
async fn test_chat_settings_are_the_overrides_of_apollia() {
    use crate::agents::apollia::{changes, definition_path, ChatConfigView, ConfigWrite};

    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, _journal) = state(&dir, true).await;
    let (_, before) = call(state.clone(), "GET", &definition_path(), None).await;
    let desired = ChatConfigView {
        system_prompt: "Answer in French.".into(),
        allowed_tools: vec!["web_search".into()],
        llm_backend: None,
    };

    // WHEN
    for write in changes(&ChatConfigView::from_resolved(&before), &desired) {
        let (method, body) = match &write {
            ConfigWrite::SetMission(prompt) => ("PUT", Some(json!({ "value": prompt }))),
            ConfigWrite::SetAccess(tools) => ("PUT", Some(json!({ "value": tools }))),
            _ => ("DELETE", None),
        };
        let (status, _) = call(state.clone(), method, &write.path(), body).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (read, after) = call(state, "GET", &definition_path(), None).await;

    // THEN
    assert_eq!(read, StatusCode::OK);
    assert_eq!(before["origin"], "interface");
    assert_eq!(before["name"], "Apollia");
    assert_eq!(ChatConfigView::from_resolved(&after), desired);
}

// GIVEN a runtime whose registry holds file_read (reading) and file_write,
//       Apollia without an access override, and a package agent that
//       declares file_read
// WHEN both cards are read
// THEN Apollia shows the reading family, not as the user's choice, and the
//      package agent shows what its package declares
#[tokio::test]
async fn test_the_card_shows_the_default_access() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _) = state(&dir, false).await;
    let registry = apollia_tools::ToolRegistryHandle::start();
    for descriptor in [
        apollia_tools::tools::file_read::FileRead::descriptor(),
        apollia_tools::tools::file_write::FileWrite::descriptor(),
    ] {
        registry.register(descriptor).await.unwrap();
    }
    state.tool_registry_handle = Some(registry.clone());

    // WHEN
    let (_, apollia) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-definitions/apollia:chat",
        None,
    )
    .await;
    let (_, invoices) = call(state, "GET", "/api/v1/agent-definitions/invoices", None).await;

    // THEN
    assert_eq!(apollia["access"], json!(["file_read"]));
    assert_eq!(apollia["overridden"], json!([]));
    assert_eq!(invoices["access"], json!(["file_read"]));
    registry.shutdown().await;
}
