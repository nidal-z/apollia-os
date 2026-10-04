use std::sync::Arc;

use apollia_core::{InferenceServer, LlmBackendConfig, LlmBackendRepository, LlmProvider};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::api::routes_llm::llm_routes;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::api::server::AppState;

async fn call(
    state: AppState<MockBackend>,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let app = llm_routes::<MockBackend>().with_state(state);
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

/// A runtime over a fresh `system.db` holding one local engine backend and
/// one declared server, `vllm`.
fn state_with_repo(dir: &tempfile::TempDir) -> AppState<MockBackend> {
    let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
    repo.save(&LlmBackendConfig {
        name: "local".into(),
        provider: LlmProvider::LlamaCpp,
        model: "/models/qwen3-8b.gguf".into(),
        config_json: json!({}),
        enabled: true,
        is_default: true,
    })
    .unwrap();
    repo.save_server(&InferenceServer {
        name: "vllm".into(),
        base_url: "http://10.0.0.5:8000/v1".into(),
        has_key: false,
        api_key_env: None,
        enabled: true,
    })
    .unwrap();
    let mut state = test_app_state_no_llm();
    state.llm_backend_repo = Some(Arc::new(std::sync::Mutex::new(repo)));
    state
}

// GIVEN a runtime with no preset
// WHEN a default preset is created, a second one created as default, the
//      first renamed, the list read for that model, and the second deleted
// THEN each call answers its status, the default moved to the second, and
//      the model is left with the renamed first preset only
#[tokio::test]
async fn test_preset_routes_create_list_update_delete() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_repo(&dir);

    // WHEN
    let (created, first) = call(
        state.clone(),
        "POST",
        "/api/v1/llm/presets",
        Some(json!({
            "name": "fast",
            "model_source": "local",
            "model_id": "qwen3-8b",
            "request": { "temperature": 0.7 },
            "is_model_default": true
        })),
    )
    .await;
    let (_, second) = call(
        state.clone(),
        "POST",
        "/api/v1/llm/presets",
        Some(json!({
            "name": "precise",
            "model_source": "local",
            "model_id": "qwen3-8b",
            "load": { "context_length": 16384 },
            "is_model_default": true
        })),
    )
    .await;
    let (updated, renamed) = call(
        state.clone(),
        "PUT",
        &format!("/api/v1/llm/presets/{}", first["id"]),
        Some(json!({ "name": "quick", "request": { "temperature": 0.9 } })),
    )
    .await;
    let (listed, list) = call(
        state.clone(),
        "GET",
        "/api/v1/llm/presets?model_source=local&model_id=qwen3-8b",
        None,
    )
    .await;
    let (deleted, _) = call(
        state.clone(),
        "DELETE",
        &format!("/api/v1/llm/presets/{}", second["id"]),
        None,
    )
    .await;
    let (_, after) = call(state.clone(), "GET", "/api/v1/llm/presets", None).await;

    // THEN
    assert_eq!(created, StatusCode::CREATED);
    assert_eq!(first["is_model_default"], true);
    assert_eq!(updated, StatusCode::OK);
    assert_eq!(renamed["name"], "quick");
    assert_eq!(renamed["request"], json!({ "temperature": 0.9 }));
    assert_eq!(listed, StatusCode::OK);
    let flags: Vec<(String, bool)> = list["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap().to_string(),
                p["is_model_default"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        flags,
        vec![("precise".into(), true), ("quick".into(), false)]
    );
    assert_eq!(list["presets"][0]["load"]["context_length"], 16384);
    assert_eq!(deleted, StatusCode::OK);
    assert_eq!(after["presets"].as_array().unwrap().len(), 1);
}

// GIVEN a runtime with a preset named `fast`
// WHEN a preset carrying a system prompt is created, `fast` is created
//      again for the same model, an absent preset is deleted, and the list
//      is filtered on a model with no source
// THEN 422, 409, 404 and 422
#[tokio::test]
async fn test_preset_routes_refuse_a_system_prompt_a_duplicate_and_an_absent_preset() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_repo(&dir);
    let fast = json!({ "name": "fast", "model_source": "local", "model_id": "qwen3-8b" });
    call(
        state.clone(),
        "POST",
        "/api/v1/llm/presets",
        Some(fast.clone()),
    )
    .await;

    // WHEN
    let (prompt, _) = call(
        state.clone(),
        "POST",
        "/api/v1/llm/presets",
        Some(json!({
            "name": "chatty",
            "model_source": "local",
            "model_id": "qwen3-8b",
            "request": { "system_prompt": "Be nice" }
        })),
    )
    .await;
    let (duplicate, _) = call(state.clone(), "POST", "/api/v1/llm/presets", Some(fast)).await;
    let (absent, _) = call(state.clone(), "DELETE", "/api/v1/llm/presets/999", None).await;
    let (half_filter, _) = call(
        state.clone(),
        "GET",
        "/api/v1/llm/presets?model_id=qwen3-8b",
        None,
    )
    .await;

    // THEN
    assert_eq!(prompt, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(duplicate, StatusCode::CONFLICT);
    assert_eq!(absent, StatusCode::NOT_FOUND);
    assert_eq!(half_filter, StatusCode::UNPROCESSABLE_ENTITY);
}

// GIVEN a runtime with a preset of a server model
// WHEN an agent with no assignment is read, then assigned that model and
//      preset, read again, assigned an undeclared server, and cleared
// THEN 404, 200, the assignment read back, 422, and the clear reported
#[tokio::test]
async fn test_agent_model_routes_read_set_and_clear() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_repo(&dir);
    let (_, preset) = call(
        state.clone(),
        "POST",
        "/api/v1/llm/presets",
        Some(json!({ "name": "long", "model_source": "server", "model_id": "vllm/qwen3-32b" })),
    )
    .await;
    let uri = "/api/v1/llm/agents/mail-triage/model";

    // WHEN
    let (before, _) = call(state.clone(), "GET", uri, None).await;
    let (set, _) = call(
        state.clone(),
        "PUT",
        uri,
        Some(json!({
            "model_source": "server",
            "model_id": "vllm/qwen3-32b",
            "preset_id": preset["id"]
        })),
    )
    .await;
    let (read, assigned) = call(state.clone(), "GET", uri, None).await;
    let (ghost, _) = call(
        state.clone(),
        "PUT",
        uri,
        Some(json!({ "model_source": "server", "model_id": "ghost/model" })),
    )
    .await;
    let (cleared, clear) = call(state.clone(), "DELETE", uri, None).await;

    // THEN
    assert_eq!(before, StatusCode::NOT_FOUND);
    assert_eq!(set, StatusCode::OK);
    assert_eq!(read, StatusCode::OK);
    assert_eq!(
        assigned,
        json!({
            "agent": "mail-triage",
            "model_source": "server",
            "model_id": "vllm/qwen3-32b",
            "preset_id": preset["id"]
        })
    );
    assert_eq!(ghost, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(cleared, StatusCode::OK);
    assert_eq!(clear["cleared"], true);
}

// GIVEN a runtime whose default is the local engine and a declared server
// WHEN the default is read, set to a model of the server, read again, and
//      set to an undeclared server
// THEN the local default reads back, the server model becomes the one
//      default, and the undeclared server is refused
#[tokio::test]
async fn test_default_model_routes_read_and_set_local_or_server() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_repo(&dir);
    let uri = "/api/v1/llm/default-model";

    // WHEN
    let (_, local) = call(state.clone(), "GET", uri, None).await;
    let (set, server) = call(
        state.clone(),
        "PUT",
        uri,
        Some(json!({ "model_source": "server", "model_id": "vllm/qwen3-32b" })),
    )
    .await;
    let (_, read) = call(state.clone(), "GET", uri, None).await;
    let (ghost, _) = call(
        state.clone(),
        "PUT",
        uri,
        Some(json!({ "model_source": "server", "model_id": "ghost/model" })),
    )
    .await;

    // THEN
    assert_eq!(
        local,
        json!({ "backend": "local", "model_source": "local", "model_id": "local" })
    );
    assert_eq!(set, StatusCode::OK);
    assert_eq!(server["model_id"], "vllm/qwen3-32b");
    assert_eq!(read, server);
    assert_eq!(ghost, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, backends) = call(state.clone(), "GET", "/api/v1/llm/backends", None).await;
    let defaults: Vec<&Value> = backends["backends"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["is_default"] == true)
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0]["name"], server["backend"]);
}

struct Silent;

#[async_trait::async_trait]
impl apollia_llm::CompletionModel for Silent {
    async fn complete(
        &self,
        _req: apollia_llm::CompletionRequest,
    ) -> Result<apollia_llm::CompletionResponse, apollia_llm::LlmError> {
        Err(apollia_llm::LlmError::InferenceError("not called".into()))
    }

    async fn stream(
        &self,
        _req: apollia_llm::CompletionRequest,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, apollia_llm::LlmError>>
                    + Send,
            >,
        >,
        apollia_llm::LlmError,
    > {
        Err(apollia_llm::LlmError::InferenceError("not called".into()))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "silent"
    }
    fn model_id(&self) -> &str {
        "silent"
    }
}

fn router_of(name: &str) -> apollia_llm::LlmRouter {
    let mut backends: std::collections::HashMap<String, Arc<dyn apollia_llm::CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(name.to_string(), Arc::new(Silent));
    apollia_llm::LlmRouter::with_backends(backends, name)
}

// GIVEN a runtime whose router saw agent `mail` ask for a model it lacks
// WHEN the fallback route is read for `mail`, for an agent that never fell
//      back, and on a runtime with no router
// THEN `mail` reads the model asked for and the one used, the other agent
//      reads 404, and the runtime without a router reads 503
#[tokio::test]
async fn test_agent_model_fallback_route_reads_the_last_fallback() {
    // GIVEN
    let state = test_app_state_no_llm();
    let router = Arc::new(router_of("local"));
    router.resolve_agent_model("mail", Some("vllm/gone"), None, None);
    *state.llm_router.write().await = Some(router);

    // WHEN
    let (found, fallback) = call(
        state.clone(),
        "GET",
        "/api/v1/llm/agents/mail/model/fallback",
        None,
    )
    .await;
    let (never, _) = call(
        state.clone(),
        "GET",
        "/api/v1/llm/agents/notes/model/fallback",
        None,
    )
    .await;
    let (no_router, _) = call(
        test_app_state_no_llm(),
        "GET",
        "/api/v1/llm/agents/mail/model/fallback",
        None,
    )
    .await;

    // THEN
    assert_eq!(found, StatusCode::OK);
    assert_eq!(fallback["agent"], "mail");
    assert_eq!(fallback["requested"], "vllm/gone");
    assert_eq!(fallback["used"], "local");
    assert!(fallback["at"].as_str().is_some_and(|at| !at.is_empty()));
    assert_eq!(never, StatusCode::NOT_FOUND);
    assert_eq!(no_router, StatusCode::SERVICE_UNAVAILABLE);
}

// GIVEN a runtime whose boot router carries `[llm.routing]` and an agent's
//       last fallback, held by a running agent, and a `system.db` whose
//       default is a server backend
// WHEN POST /api/v1/llm/reload
// THEN the response says running agents are reached, the agent holding the
//      boot router reaches the rebuilt one, and the routing and the fallback
//      survive the reload
#[tokio::test]
async fn test_reload_keeps_routing_and_reaches_running_agents() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
    repo.save(&LlmBackendConfig {
        name: "cloud".into(),
        provider: LlmProvider::OpenAi,
        model: "qwen3-32b".into(),
        config_json: json!({ "api_url": "http://127.0.0.1:9/v1" }),
        enabled: true,
        is_default: true,
    })
    .unwrap();
    let mut state = test_app_state_no_llm();
    state.llm_backend_repo = Some(Arc::new(std::sync::Mutex::new(repo)));
    let routing = apollia_core::LlmRoutingConfig {
        precise: "cloud".into(),
        fast: "cloud".into(),
        hybrid: None,
        format_version: 1,
    };
    let boot = Arc::new(router_of("cloud").with_routing(routing));
    boot.resolve_agent_model("mail", Some("gone"), None, None);
    let held_by_agent = Arc::clone(&boot);
    *state.llm_router.write().await = Some(boot);

    // WHEN
    let (status, body) = call(state.clone(), "POST", "/api/v1/llm/reload", None).await;

    // THEN
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["reaches_running_agents"], true);
    let rebuilt = state.llm_router.read().await.clone().unwrap();
    let seen = held_by_agent.latest();
    assert!(Arc::ptr_eq(&seen, &rebuilt));
    assert_eq!(seen.routing().map(|r| r.precise.as_str()), Some("cloud"));
    assert_eq!(
        seen.agent_model_fallback("mail").map(|f| f.requested),
        Some("gone".to_string())
    );
}

// GIVEN a runtime with a signed journal
// WHEN the model of `mail` is assigned, then removed, then removed again
// THEN the history of `mail` holds the model set then removed, and the second
//      removal, which changed nothing, is not recorded
#[tokio::test]
async fn test_agent_model_assignment_is_recorded_in_the_history_of_the_agent() {
    use crate::audit_journal::AuditJournalHandle;
    use crate::governance::governance_run_id;

    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_with_repo(&dir);
    let journal = AuditJournalHandle::open_with_key_bytes(
        &dir.path().join("journal.db"),
        b"agent-model-journal-key-00000001".to_vec(),
    )
    .await
    .unwrap();
    state.audit_journal = Some(journal.clone());
    state.governance = Some(crate::governance::GovernanceHandle::detached(
        crate::governance::GovernanceState::default(),
        Some(journal.clone()),
    ));
    let uri = "/api/v1/llm/agents/mail/model";

    // WHEN
    let (set, _) = call(
        state.clone(),
        "PUT",
        uri,
        Some(json!({"model_source": "local", "model_id": "local"})),
    )
    .await;
    let (cleared, _) = call(state.clone(), "DELETE", uri, None).await;
    let (again, _) = call(state, "DELETE", uri, None).await;

    // THEN
    assert_eq!(set, StatusCode::OK);
    assert_eq!(cleared, StatusCode::OK);
    assert_eq!(again, StatusCode::OK);
    let records: Vec<(Value, Value)> = journal
        .query_run(&governance_run_id("mail"))
        .await
        .into_iter()
        .map(|entry| {
            (
                entry.payload["field"].clone(),
                entry.payload["change"].clone(),
            )
        })
        .collect();
    assert_eq!(
        records,
        vec![
            (json!("model"), json!("set")),
            (json!("model"), json!("removed")),
        ]
    );
}
