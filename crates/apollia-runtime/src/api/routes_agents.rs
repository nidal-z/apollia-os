//! REST routes for agent management — GET/POST/DELETE `/api/v1/agents`.
//!
//! These routes expose agent lifecycle management via the API. They delegate
//! to [`AgentRegistryHandle`] for state reads and transitions.
//!
//! Agent loading is abstracted via the [`AgentLoader`] trait (ADR-019) to keep
//! `apollia-runtime` decoupled from PyO3/apollia-aip.

use std::path::Path;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend, ExecutionCoordinator};
use crate::registry::AgentRegistryError;

use apollia_core::{AgentManifest, ProcessState};

/// Trait for loading and validating Python agent modules (ADR-019).
///
/// Abstracts away the PyO3-based AIP loading so that `apollia-runtime` does
/// not depend on `apollia-aip`. The concrete implementation lives in `apollia-cli`.
pub trait AgentLoader: Send + Sync {
    /// Load a Python agent module from `path`, validate AIP duck typing,
    /// and return the deserialized [`AgentManifest`].
    ///
    /// Returns a human-readable error string on failure.
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String>;
}

/// Factory for creating per-agent execution backends (ADR-019 extension).
///
/// Abstracts away PyO3/AIPBridge creation so that `apollia-runtime` remains
/// decoupled from `apollia-aip`. The concrete implementation lives in `apollia-cli`.
///
/// Called once per agent at start time from [`start_agent`].
pub trait AgentBackendFactory: Send + Sync {
    /// Creates a real execution backend for the given agent.
    ///
    /// The returned `DynBackend` is used for all tasks routed to this agent.
    /// Implementations load the Python module, create an `AIPBridge`, and
    /// build a `RuntimeContext` factory that is closed over in the backend.
    fn create_for_agent(&self, agent_path: &Path, manifest: &AgentManifest) -> DynBackend;
}

/// Stub [`AgentLoader`] that builds a minimal manifest from the file name.
///
/// Intended for tests that don't exercise agent loading logic.
pub struct StubAgentLoader;

impl AgentLoader for StubAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("stub-agent")
            .replace('_', "-");
        Ok(AgentManifest {
            name,
            version: "0.0.0".to_string(),
            description: "stub agent for tests".to_string(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
        })
    }
}

/// Request body for `POST /api/v1/agents`.
#[derive(Debug, Deserialize)]
pub struct StartAgentRequest {
    /// Path to the agent Python module.
    pub agent_path: String,
}

/// Response body for agent operations.
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    /// Agent identifier (UUID v4).
    pub agent_id: String,
    /// Agent name from manifest (always present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Current process state as string.
    pub state: String,
    /// Agent manifest (present in detail view).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<serde_json::Value>,
}

/// Response body for agent list.
#[derive(Debug, Serialize)]
pub struct AgentListResponse {
    /// All registered agents.
    pub agents: Vec<AgentResponse>,
}

/// Standard error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

/// Convert a `ProcessState` to its lowercase string representation.
fn state_to_string(state: &ProcessState) -> String {
    match state {
        ProcessState::Initializing => "initializing".to_string(),
        ProcessState::Active => "active".to_string(),
        ProcessState::Degraded => "degraded".to_string(),
        ProcessState::Stopping => "stopping".to_string(),
        ProcessState::Stopped => "stopped".to_string(),
    }
}

/// Load an agent manifest via the [`AgentLoader`] trait (ADR-019).
///
/// Delegates to the concrete loader injected in `AppState`.
/// Returns a structured error response on failure.
fn load_manifest(
    loader: &dyn AgentLoader,
    agent_path: &str,
) -> Result<AgentManifest, (StatusCode, Json<ErrorResponse>)> {
    let path = Path::new(agent_path);
    loader.load_and_validate(path).map_err(|reason| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("failed to load agent from '{agent_path}': {reason}"),
            }),
        )
    })
}

/// Convert a registry error to an HTTP error response.
fn registry_error_to_response(err: AgentRegistryError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, message) = match &err {
        AgentRegistryError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        AgentRegistryError::InvalidTransition { .. } => (StatusCode::CONFLICT, err.to_string()),
        AgentRegistryError::ActorDead => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };
    (status, Json(ErrorResponse { error: message }))
}

/// Handler for `GET /api/v1/agents`.
///
/// Lists all registered agents with their current state.
pub async fn list_agents<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<AgentListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let entries = state
        .registry_handle
        .list_agents()
        .await
        .map_err(registry_error_to_response)?;

    let agents = entries
        .into_iter()
        .map(|entry| AgentResponse {
            agent_id: entry.id.to_string(),
            name: Some(entry.manifest.name.clone()),
            state: state_to_string(&entry.process_state),
            manifest: None,
        })
        .collect();

    Ok(Json(AgentListResponse { agents }))
}

/// Handler for `POST /api/v1/agents`.
///
/// Loads the Python agent module via [`AgentLoader`] (ADR-019), validates
/// AIP duck typing, registers the agent with its real manifest, transitions
/// to Active (or Degraded if optional tools are missing), and creates an
/// [`ExecutionCoordinator`] registered with the [`TaskRouter`].
///
/// Returns 201 Created with the agent_id and state.
/// Returns 400 Bad Request if the Python module is invalid.
pub async fn start_agent<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Json(req): Json<StartAgentRequest>,
) -> Result<(StatusCode, Json<AgentResponse>), (StatusCode, Json<ErrorResponse>)> {
    let manifest = load_manifest(state.agent_loader.as_ref(), &req.agent_path)?;

    let has_missing_optional = match &state.tool_registry_handle {
        Some(registry) => {
            let mut missing = false;
            for name in &manifest.tools_optional {
                if registry.get(name).await.ok().flatten().is_none() {
                    missing = true;
                    break;
                }
            }
            missing
        }
        // No registry available — treat all optional tools as missing.
        None => !manifest.tools_optional.is_empty(),
    };
    let max_concurrent = manifest.max_concurrent_tasks;
    // Clone manifest before consuming it in register() — needed for factory below.
    let manifest_for_factory = manifest.clone();

    let agent_id = state
        .registry_handle
        .register(manifest)
        .await
        .map_err(registry_error_to_response)?;

    let final_state = if has_missing_optional {
        // optional tools listed but not resolved yet -> DEGRADED
        state
            .registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .map_err(registry_error_to_response)?;
        state
            .registry_handle
            .update_state(agent_id.as_str(), ProcessState::Degraded)
            .await
            .map_err(registry_error_to_response)?;
        "degraded"
    } else {
        state
            .registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .map_err(registry_error_to_response)?;
        "active"
    };

    // Create and register an ExecutionCoordinator for this agent.
    // Production: factory creates a real AIPBridge backend, converted to B via From<DynBackend>.
    // Tests: factory is None — use state.backend directly (already type B).
    let agent_backend: B = match &state.backend_factory {
        Some(factory) => {
            let dyn_backend =
                factory.create_for_agent(Path::new(&req.agent_path), &manifest_for_factory);
            B::from(dyn_backend)
        }
        None => state.backend.clone(),
    };
    let mut coordinator = ExecutionCoordinator::new(
        agent_id.clone(),
        max_concurrent,
        state.event_sender.clone(),
        agent_backend,
    )
    .with_agent_name(manifest_for_factory.name.clone());
    if let Some(ref repo) = state.task_repository {
        coordinator = coordinator.with_task_repository(Arc::clone(repo), state.obs_config.clone());
    }
    // Fire-and-forget: if registration fails the task submission will return NoCoordinator.
    let _ = state
        .router_handle
        .register_coordinator(agent_id.clone(), coordinator)
        .await;

    Ok((
        StatusCode::CREATED,
        Json(AgentResponse {
            agent_id: agent_id.to_string(),
            name: Some(manifest_for_factory.name.clone()),
            state: final_state.to_string(),
            manifest: None,
        }),
    ))
}

/// Resolves `id_or_name` — either a UUID or a human-readable agent name — to its
/// [`AgentEntry`].
///
/// Resolution order:
/// 1. Direct UUID lookup via [`AgentRegistryHandle::get_agent`].
/// 2. Name index lookup via [`AgentRegistryHandle::find_by_name`], then UUID lookup.
///
/// Returns `None` if neither lookup finds a match.
async fn resolve_agent<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    id_or_name: &str,
) -> Result<Option<crate::registry::AgentEntry>, (StatusCode, Json<ErrorResponse>)> {
    // 1. Direct lookup — works when id_or_name is a UUID.
    let entry = state
        .registry_handle
        .get_agent(id_or_name)
        .await
        .map_err(registry_error_to_response)?;

    if entry.is_some() {
        return Ok(entry);
    }

    // 2. Name-based lookup — resolves human-readable identifiers like "apollia-reviewer".
    let resolved_id = state
        .registry_handle
        .find_by_name(id_or_name)
        .await
        .map_err(registry_error_to_response)?;

    match resolved_id {
        None => Ok(None),
        Some(id) => state
            .registry_handle
            .get_agent(id.as_str())
            .await
            .map_err(registry_error_to_response),
    }
}

/// Handler for `GET /api/v1/agents/{id}`.
///
/// Returns the detail of a single agent including its manifest.
/// Accepts both a UUID and a human-readable agent name (e.g. `apollia-reviewer`).
/// Returns 404 if the agent does not exist.
pub async fn get_agent<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    AxumPath(id_or_name): AxumPath<String>,
) -> Result<Json<AgentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let entry = resolve_agent(&state, &id_or_name).await?.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("agent not found: {id_or_name}"),
            }),
        )
    })?;

    let manifest_json = serde_json::to_value(&entry.manifest).ok();
    Ok(Json(AgentResponse {
        agent_id: entry.id.to_string(),
        name: Some(entry.manifest.name.clone()),
        state: state_to_string(&entry.process_state),
        manifest: manifest_json,
    }))
}

/// Handler for `DELETE /api/v1/agents/{id}`.
///
/// Performs a full graceful shutdown of the agent:
/// 1. Transitions to `Stopping` (emits `AgentStopping` event for observers).
/// 2. Unregisters the `ExecutionCoordinator` from the `TaskRouter` so no new
///    tasks can be submitted to this agent.
/// 3. Transitions to `Stopped` (emits `AgentStopped` event).
///
/// This mirrors the sequence performed by [`ShutdownController::stop_agents`]
/// during full system shutdown. The `Stopping` intermediate state is preserved
/// so that event-bus subscribers (dashboard, SSE streams) observe the correct
/// lifecycle.
///
/// Accepts both a UUID and a human-readable agent name (e.g. `apollia-reviewer`).
/// Returns 409 Conflict if the agent is already stopped or stopping.
/// Returns 404 if the agent does not exist.
pub async fn stop_agent<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    AxumPath(id_or_name): AxumPath<String>,
) -> Result<Json<AgentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let entry = resolve_agent(&state, &id_or_name).await?.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("agent not found: {id_or_name}"),
            }),
        )
    })?;

    if entry.process_state == ProcessState::Stopped {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: format!("agent already stopped: {}", entry.id),
            }),
        ));
    }

    if entry.process_state == ProcessState::Stopping {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: format!("agent already stopping: {}", entry.id),
            }),
        ));
    }

    // Use the canonical AgentId — never the raw user input which may be a name.
    let agent_id = entry.id.clone();

    // Step 1: signal drain (emits AgentStopping on the EventBus).
    state
        .registry_handle
        .update_state(agent_id.as_str(), ProcessState::Stopping)
        .await
        .map_err(registry_error_to_response)?;

    // Step 2: remove coordinator so the TaskRouter rejects any new task submissions.
    // Fire-and-forget: if the router is already dead, this is a no-op.
    let _ = state.router_handle.unregister_coordinator(&agent_id).await;

    // Step 3: complete the lifecycle (emits AgentStopped on the EventBus).
    state
        .registry_handle
        .update_state(agent_id.as_str(), ProcessState::Stopped)
        .await
        .map_err(registry_error_to_response)?;

    Ok(Json(AgentResponse {
        agent_id: agent_id.to_string(),
        name: None,
        state: "stopped".to_string(),
        manifest: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::{AgentRegistry, AgentRegistryHandle};
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AgentManifest, ProcessState, TaskStatus};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockBackend;

    impl From<DynBackend> for MockBackend {
        fn from(_: DynBackend) -> Self {
            MockBackend
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: apollia_core::AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// Mock AgentLoader that builds a manifest from the file path stem.
    struct MockAgentLoader;

    impl AgentLoader for MockAgentLoader {
        fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .replace('_', "-");
            Ok(AgentManifest {
                name,
                version: "1.0.0".to_string(),
                description: "mock agent".to_string(),
                tools_required: vec![],
                tools_optional: vec![],
                supports_streaming: false,
                supports_a2a: false,
                memory_namespace: None,
                shared_memory_namespaces: vec![],
                max_concurrent_tasks: 1,
                step_budget: None,
                network_allowlist: None,
                dangerous_tools_allowed: false,
                tags: vec![],
                skills: vec![],
                execution_mode: "auto".to_string(),
                system_prompt: None,
                tools_requiring_approval: vec![],
                llm_backend: None,
            })
        }
    }

    /// Mock AgentLoader that always fails (for error path tests).
    struct FailingAgentLoader;

    impl AgentLoader for FailingAgentLoader {
        fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
            Err(format!("syntax error in '{}'", path.display()))
        }
    }

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
        }
    }

    fn test_router() -> (Router, AgentRegistryHandle) {
        test_router_with_loader(Arc::new(MockAgentLoader))
    }

    fn test_router_with_loader(loader: Arc<dyn AgentLoader>) -> (Router, AgentRegistryHandle) {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle: registry_handle.clone(),
            event_sender: event_tx,
            agent_loader: loader,
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            notification_config: None,
            pipeline_engine: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            pipeline_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            config_writer: None,
            llm_backend_repo: None,
        };
        let router = Router::new()
            .route(
                "/api/v1/agents",
                get(list_agents::<MockBackend>).post(start_agent::<MockBackend>),
            )
            .route(
                "/api/v1/agents/:id",
                get(get_agent::<MockBackend>).delete(stop_agent::<MockBackend>),
            )
            .with_state(state);
        (router, registry_handle)
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .expect("read body");
        serde_json::from_slice(&bytes).expect("parse json")
    }

    #[tokio::test]
    async fn test_list_agents_returns_all() {
        // GIVEN 2 agents enregistres
        let (router, registry) = test_router();
        let id1 = registry
            .register(test_manifest("hello-agent"))
            .await
            .expect("register");
        registry
            .update_state(id1.as_str(), ProcessState::Active)
            .await
            .expect("activate");
        let id2 = registry
            .register(test_manifest("crm-agent"))
            .await
            .expect("register");
        registry
            .update_state(id2.as_str(), ProcessState::Active)
            .await
            .expect("activate");
        registry
            .update_state(id2.as_str(), ProcessState::Stopping)
            .await
            .expect("stopping");
        registry
            .update_state(id2.as_str(), ProcessState::Stopped)
            .await
            .expect("stopped");

        // WHEN GET /api/v1/agents
        let req = Request::builder()
            .uri("/api/v1/agents")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec 2 agents
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let agents = json["agents"].as_array().expect("agents array");
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        // GIVEN aucun agent
        let (router, _) = test_router();

        // WHEN GET /api/v1/agents
        let req = Request::builder()
            .uri("/api/v1/agents")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec []
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let agents = json["agents"].as_array().expect("agents array");
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_start_agent_returns_201() {
        // GIVEN un router vide
        let (router, _) = test_router();

        // WHEN POST /api/v1/agents avec agent_path
        let body = serde_json::json!({"agent_path": "/path/to/hello_agent.py"});
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/agents")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 201 Created avec agent_id et state "active"
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp).await;
        assert_eq!(json["state"], "active");
        assert!(json["agent_id"].is_string());
        assert!(!json["agent_id"].as_str().expect("agent_id str").is_empty());
    }

    #[tokio::test]
    async fn test_get_agent_detail() {
        // GIVEN un agent "hello-agent" ACTIVE
        let (router, registry) = test_router();
        let agent_id = registry
            .register(test_manifest("hello-agent"))
            .await
            .expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate");

        // WHEN GET /api/v1/agents/{id}
        let req = Request::builder()
            .uri(format!("/api/v1/agents/{agent_id}"))
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec detail complet incluant manifest
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "active");
        assert!(json["manifest"].is_object());
        assert_eq!(json["manifest"]["name"], "hello-agent");
    }

    #[tokio::test]
    async fn test_get_agent_by_name() {
        // GIVEN un agent enregistré sous le nom "my-agent"
        let (router, registry) = test_router();
        let agent_id = registry
            .register(test_manifest("my-agent"))
            .await
            .expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate");

        // WHEN GET /api/v1/agents/my-agent (nom humain, pas UUID)
        let req = Request::builder()
            .uri("/api/v1/agents/my-agent")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec le bon agent_id et le manifest
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "active");
        assert_eq!(json["manifest"]["name"], "my-agent");
    }

    #[tokio::test]
    async fn test_get_agent_not_found() {
        // GIVEN aucun agent "ghost"
        let (router, _) = test_router();

        // WHEN GET /api/v1/agents/ghost
        let req = Request::builder()
            .uri("/api/v1/agents/ghost")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 404
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = body_json(resp).await;
        assert!(json["error"]
            .as_str()
            .expect("error str")
            .contains("agent not found"));
    }

    #[tokio::test]
    async fn test_stop_agent_active() {
        // GIVEN un agent ACTIVE
        let (router, registry) = test_router();
        let agent_id = registry
            .register(test_manifest("hello-agent"))
            .await
            .expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate");

        // WHEN DELETE /api/v1/agents/{id}
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/agents/{agent_id}"))
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec state "stopped" — cycle Stopping→Stopped complété atomiquement
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "stopped");
    }

    #[tokio::test]
    async fn test_stop_agent_by_name() {
        // GIVEN un agent ACTIVE enregistré sous le nom "my-agent"
        let (router, registry) = test_router();
        let agent_id = registry
            .register(test_manifest("my-agent"))
            .await
            .expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate");

        // WHEN DELETE /api/v1/agents/my-agent (nom humain, pas UUID)
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/agents/my-agent")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec le canonical UUID et state "stopped"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "stopped");
    }

    #[tokio::test]
    async fn test_stop_agent_registry_state_is_stopped() {
        // GIVEN un agent ACTIVE
        let (router, registry) = test_router();
        let agent_id = registry
            .register(test_manifest("hello-agent"))
            .await
            .expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate");

        // WHEN DELETE /api/v1/agents/{id}
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/agents/{agent_id}"))
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");
        assert_eq!(resp.status(), StatusCode::OK);

        // THEN l'etat dans le registre est bien Stopped (pas Stopping)
        let entry = registry
            .get_agent(agent_id.as_str())
            .await
            .expect("registry call")
            .expect("agent exists");
        assert_eq!(entry.process_state, ProcessState::Stopped);
    }

    #[tokio::test]
    async fn test_stop_agent_already_stopped() {
        // GIVEN un agent STOPPED
        let (router, registry) = test_router();
        let agent_id = registry
            .register(test_manifest("hello-agent"))
            .await
            .expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Stopping)
            .await
            .expect("stopping");
        registry
            .update_state(agent_id.as_str(), ProcessState::Stopped)
            .await
            .expect("stopped");

        // WHEN DELETE /api/v1/agents/{id}
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/agents/{agent_id}"))
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 409 Conflict
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let json = body_json(resp).await;
        assert!(json["error"]
            .as_str()
            .expect("error str")
            .contains("already stopped"));
    }

    #[tokio::test]
    async fn test_start_agent_invalid_python_returns_400() {
        // GIVEN un loader qui echoue toujours
        let (router, _) = test_router_with_loader(Arc::new(FailingAgentLoader));

        // WHEN POST /api/v1/agents avec un fichier invalide
        let body = serde_json::json!({"agent_path": "/path/to/broken.py"});
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/agents")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 400 avec erreur claire
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = body_json(resp).await;
        let error = json["error"].as_str().expect("error str");
        assert!(error.contains("failed to load agent"));
        assert!(error.contains("syntax error"));
    }

    #[tokio::test]
    async fn test_start_agent_with_optional_tools_degraded() {
        // GIVEN un loader qui retourne un manifest avec tools_optional
        struct DegradedLoader;
        impl AgentLoader for DegradedLoader {
            fn load_and_validate(&self, _path: &Path) -> Result<AgentManifest, String> {
                Ok(AgentManifest {
                    name: "degraded-agent".to_string(),
                    version: "1.0.0".to_string(),
                    description: "agent with optional tools".to_string(),
                    tools_required: vec![],
                    tools_optional: vec!["mcp_erp".to_string()],
                    supports_streaming: false,
                    supports_a2a: false,
                    memory_namespace: None,
                    shared_memory_namespaces: vec![],
                    max_concurrent_tasks: 1,
                    step_budget: None,
                    network_allowlist: None,
                    dangerous_tools_allowed: false,
                    tags: vec![],
                    skills: vec![],
                    execution_mode: "auto".to_string(),
                    system_prompt: None,
                    tools_requiring_approval: vec![],
                    llm_backend: None,
                })
            }
        }
        let (router, _) = test_router_with_loader(Arc::new(DegradedLoader));

        // WHEN POST /api/v1/agents
        let body = serde_json::json!({"agent_path": "/path/to/degraded_agent.py"});
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/agents")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 201 avec state "degraded"
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp).await;
        assert_eq!(json["state"], "degraded");
    }
}
