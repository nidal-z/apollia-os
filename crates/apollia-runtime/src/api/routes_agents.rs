//! REST routes for agent management, GET/POST/DELETE `/api/v1/agents`.
//!
//! These routes expose agent lifecycle management via the API. They delegate
//! to [`AgentRegistryHandle`] for state reads and transitions.
//!
//! An agent is a definition stored in `agents.db`: starting one registers it
//! from that definition, and nothing is imported.

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::agents::available::{make_available, Availability};
use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::registry::AgentRegistryError;

use apollia_core::ProcessState;

/// The installed agents of the machine and their install route.
pub mod installed;

/// Query parameters for `GET /api/v1/agents`.
#[derive(Debug, Deserialize)]
pub struct AgentListQuery {
    /// When present and `true`, restrict results to agents declaring `supports_a2a = true`.
    pub supports_a2a: Option<bool>,
}

/// Abridged skill descriptor included in the agent list response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SkillDto {
    /// Unique skill identifier (e.g. `"read-excel"`).
    pub id: String,
    /// Human-readable skill name.
    pub name: String,
}

/// Request body for `POST /api/v1/agents`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StartAgentRequest {
    /// Identifier of the stored agent to make available.
    pub agent: String,
}

/// Response body for agent operations.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentResponse {
    /// Agent identifier (UUID v4).
    pub agent_id: String,
    /// Agent name from manifest (always present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Agent version from manifest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Current process state as string.
    pub state: String,
    /// Whether this agent supports A2A inter-agent communication.
    pub supports_a2a: bool,
    /// Skills declared by this agent (populated in list and detail responses).
    pub skills: Vec<SkillDto>,
    /// Agent manifest (present in detail view).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub manifest: Option<serde_json::Value>,
}

/// Response body for agent list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
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
/// Lists registered agents. When `?supports_a2a=true` is present, only agents
/// that declare `supports_a2a = true` in their manifest are returned, and each
/// entry includes the agent's `skills` and `version`.
#[utoipa::path(
    get,
    path = "/api/v1/agents",
    tag = "agents",
    params(("supports_a2a" = Option<bool>, Query, description = "Restrict to agents declaring supports_a2a = true")),
    responses(
        (status = 200, description = "Agent list", body = AgentListResponse),
        (status = 500, description = "Registry error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_agents<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<AgentListQuery>,
) -> Result<Json<AgentListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let entries = state
        .registry_handle
        .list_agents()
        .await
        .map_err(registry_error_to_response)?;

    let agents = entries
        .into_iter()
        .filter(|entry| query.supports_a2a != Some(true) || entry.manifest.supports_a2a)
        .map(|entry| {
            let skills = entry
                .manifest
                .skills
                .iter()
                .map(|s| SkillDto {
                    id: s.id.clone(),
                    name: s.name.clone(),
                })
                .collect();
            AgentResponse {
                agent_id: entry.id.to_string(),
                name: Some(entry.manifest.name.clone()),
                version: Some(entry.manifest.version.clone()),
                state: state_to_string(&entry.process_state),
                supports_a2a: entry.manifest.supports_a2a,
                skills,
                manifest: None,
            }
        })
        .collect();

    Ok(Json(AgentListResponse { agents }))
}

/// Handler for `POST /api/v1/agents`.
///
/// Makes a stored agent available: it is registered from the definition
/// `agents.db` holds, moved to Active, and given an [`ExecutionCoordinator`]
/// in the [`TaskRouter`]. An agent is data: nothing is imported, and what it
/// may reach is read from its resolved access when a run starts.
///
/// Returns 201 Created with the agent_id and state.
/// Returns 404 Not Found when no stored agent carries the identifier.
/// Returns 409 Conflict for a disabled agent, an agent already available,
/// and the Apollia agent, which is always there.
///
/// [`ExecutionCoordinator`]: crate::coordinator::ExecutionCoordinator
/// [`TaskRouter`]: crate::router::TaskRouter
#[utoipa::path(
    post,
    path = "/api/v1/agents",
    tag = "agents",
    request_body = StartAgentRequest,
    responses(
        (status = 201, description = "Agent available", body = AgentResponse),
        (status = 404, description = "No stored agent carries this identifier", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Agent disabled, already available, or built into Apollia", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "agents.db cannot be read", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn start_agent<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Json(req): Json<StartAgentRequest>,
) -> Result<(StatusCode, Json<AgentResponse>), (StatusCode, Json<ErrorResponse>)> {
    let refusal = |status: StatusCode, error: String| (status, Json(ErrorResponse { error }));
    if apollia_tools::is_built_in_agent(&req.agent) {
        return Err(refusal(
            StatusCode::CONFLICT,
            format!(
                "agent '{}' is built into Apollia and is always available",
                req.agent
            ),
        ));
    }
    let stored = stored_agent(&state.data_dir, &req.agent)
        .await
        .map_err(|e| refusal(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| {
            refusal(
                StatusCode::NOT_FOUND,
                format!("agent '{}' is not installed", req.agent),
            )
        })?;
    if !stored.enabled {
        return Err(refusal(
            StatusCode::CONFLICT,
            format!("agent '{}' is disabled", stored.name),
        ));
    }
    let registered = state
        .registry_handle
        .find_by_name(&stored.name)
        .await
        .map_err(registry_error_to_response)?;
    if let Some(id) = registered {
        let entry = state
            .registry_handle
            .get_agent(id.as_str())
            .await
            .map_err(registry_error_to_response)?;
        if entry.is_some_and(|e| !matches!(e.process_state, ProcessState::Stopped)) {
            return Err(refusal(
                StatusCode::CONFLICT,
                format!("agent '{}' is already available", stored.name),
            ));
        }
    }

    let agent_id = make_available(
        &stored,
        &Availability {
            registry: &state.registry_handle,
            router: &state.router_handle,
            event_sender: &state.event_sender,
            backend: &state.backend,
            task_repository: state.task_repository.as_ref(),
            observability: state.observability_live(),
        },
    )
    .await
    .map_err(registry_error_to_response)?;

    Ok((
        StatusCode::CREATED,
        Json(AgentResponse {
            agent_id: agent_id.to_string(),
            name: Some(stored.name.clone()),
            version: Some(stored.version.clone()),
            state: "active".to_string(),
            supports_a2a: false,
            skills: Vec::new(),
            manifest: None,
        }),
    ))
}

/// Registers every stored, enabled agent the registry of `state` does not
/// hold yet: what an installation or a creation calls so that the agent is
/// available at once, with no start.
pub(crate) async fn register_missing_agents<B: ExecutionBackend + Clone>(state: &AppState<B>) {
    crate::agents::available::register_missing(
        &state.data_dir,
        &Availability {
            registry: &state.registry_handle,
            router: &state.router_handle,
            event_sender: &state.event_sender,
            backend: &state.backend,
            task_repository: state.task_repository.as_ref(),
            observability: state.observability_live(),
        },
    )
    .await;
}

/// The stored agent `name` of `<data_dir>/agents.db`, read off the async
/// threads. `None` when the store does not exist or does not hold it.
async fn stored_agent(
    data_dir: &std::path::Path,
    name: &str,
) -> Result<Option<apollia_tools::StoredAgent>, String> {
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    let name = name.to_owned();
    tokio::task::spawn_blocking(move || {
        if !db.exists() {
            return Ok(None);
        }
        apollia_tools::AgentRepository::open(&db)
            .and_then(|repo| repo.stored(&name))
            .map_err(|e| format!("cannot read agents.db: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Resolves `id_or_name`, either a UUID or a human-readable agent name, to its
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
    // 1. Direct lookup, works when id_or_name is a UUID.
    let entry = state
        .registry_handle
        .get_agent(id_or_name)
        .await
        .map_err(registry_error_to_response)?;

    if entry.is_some() {
        return Ok(entry);
    }

    // 2. Name-based lookup, resolves human-readable identifiers like "apollia-reviewer".
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
#[utoipa::path(
    get,
    path = "/api/v1/agents/{id}",
    tag = "agents",
    params(("id" = String, Path, description = "Agent id or name")),
    responses(
        (status = 200, description = "Agent detail", body = AgentResponse),
        (status = 404, description = "Agent not found", body = crate::api::openapi::ApiErrorBody),
    )
)]
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

    let detail_skills = entry
        .manifest
        .skills
        .iter()
        .map(|s| SkillDto {
            id: s.id.clone(),
            name: s.name.clone(),
        })
        .collect();
    let manifest_json = serde_json::to_value(&entry.manifest).ok();
    Ok(Json(AgentResponse {
        agent_id: entry.id.to_string(),
        name: Some(entry.manifest.name.clone()),
        version: Some(entry.manifest.version.clone()),
        state: state_to_string(&entry.process_state),
        supports_a2a: entry.manifest.supports_a2a,
        skills: detail_skills,
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
#[utoipa::path(
    delete,
    path = "/api/v1/agents/{id}",
    tag = "agents",
    params(("id" = String, Path, description = "Agent id or name")),
    responses(
        (status = 200, description = "Agent stopped", body = AgentResponse),
        (status = 404, description = "Agent not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Agent already stopped or stopping", body = crate::api::openapi::ApiErrorBody),
    )
)]
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

    // Use the canonical AgentId, never the raw user input which may be a name.
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
        version: None,
        state: "stopped".to_string(),
        supports_a2a: false,
        skills: vec![],
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

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            autonomy_level: None,
            format_version: 1,
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            model: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        }
    }

    fn test_router() -> (Router, AgentRegistryHandle) {
        test_router_over(std::path::PathBuf::new())
    }

    /// A stored agent named `name` in the store of `data_dir`.
    fn store_agent(data_dir: &std::path::Path, name: &str, enabled: bool) {
        let repo = apollia_tools::AgentRepository::open(
            &apollia_core::paths::DataFile::Agents.path(data_dir),
        )
        .expect("open agents.db");
        repo.save_definition(&apollia_tools::StoredAgent {
            name: name.to_string(),
            origin: apollia_tools::AgentOrigin::Package,
            package: None,
            version: "1.0.0".to_string(),
            definition: serde_json::json!({
                "id": name, "name": name, "mission": "Says hello.", "defaults": {},
            }),
            enabled,
            installed_at: "2026-10-01T00:00:00Z".to_string(),
            updated_at: "2026-10-01T00:00:00Z".to_string(),
        })
        .expect("store the agent");
    }

    async fn start(router: &Router, agent: &str) -> axum::response::Response {
        let body = serde_json::json!({ "agent": agent });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/agents")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        router.clone().oneshot(req).await.expect("request failed")
    }

    fn test_router_over(data_dir: std::path::PathBuf) -> (Router, AgentRegistryHandle) {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle: registry_handle.clone(),
            event_sender: event_tx,
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            connection_governance: Default::default(),
            governance: None,
            inference_governance: Default::default(),
            obs_config: apollia_core::ObservabilityConfig::default(),
            config_reload: None,
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir,
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
            mlx_engine: None,
            secret_store: crate::inference_servers::default_store_opener(),
            batches: None,
            workflows: None,
            compat_server: None,
            source_registry: None,
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
        // GIVEN 2 registered agents
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

        // THEN 200 with 2 agents
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let agents = json["agents"].as_array().expect("agents array");
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        // GIVEN no agents
        let (router, _) = test_router();

        // WHEN GET /api/v1/agents
        let req = Request::builder()
            .uri("/api/v1/agents")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 with []
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let agents = json["agents"].as_array().expect("agents array");
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_get_agent_detail() {
        // GIVEN a "hello-agent" in ACTIVE state
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

        // THEN 200 with full detail including manifest
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "active");
        assert!(json["manifest"].is_object());
        assert_eq!(json["manifest"]["name"], "hello-agent");
    }

    #[tokio::test]
    async fn test_get_agent_by_name() {
        // GIVEN an agent registered under the name "my-agent"
        let (router, registry) = test_router();
        let agent_id = registry
            .register(test_manifest("my-agent"))
            .await
            .expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate");

        // WHEN GET /api/v1/agents/my-agent (human name, not UUID)
        let req = Request::builder()
            .uri("/api/v1/agents/my-agent")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 with the correct agent_id and the manifest
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "active");
        assert_eq!(json["manifest"]["name"], "my-agent");
    }

    #[tokio::test]
    async fn test_get_agent_not_found() {
        // GIVEN no "ghost" agent
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
        // GIVEN an ACTIVE agent
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

        // THEN 200 with state "stopped", Stopping->Stopped cycle completed atomically
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "stopped");
    }

    #[tokio::test]
    async fn test_stop_agent_by_name() {
        // GIVEN an ACTIVE agent registered under the name "my-agent"
        let (router, registry) = test_router();
        let agent_id = registry
            .register(test_manifest("my-agent"))
            .await
            .expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate");

        // WHEN DELETE /api/v1/agents/my-agent (human name, not UUID)
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/agents/my-agent")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 with the canonical UUID and state "stopped"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "stopped");
    }

    #[tokio::test]
    async fn test_stop_agent_registry_state_is_stopped() {
        // GIVEN an ACTIVE agent
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

        // THEN the registry state is Stopped (not Stopping)
        let entry = registry
            .get_agent(agent_id.as_str())
            .await
            .expect("registry call")
            .expect("agent exists");
        assert_eq!(entry.process_state, ProcessState::Stopped);
    }

    #[tokio::test]
    async fn test_stop_agent_already_stopped() {
        // GIVEN a STOPPED agent
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

    #[test]
    fn test_agent_list_query_deserializes_supports_a2a() {
        // GIVEN a JSON object with supports_a2a = true (mirrors URL-decoded query)
        // WHEN deserialized into AgentListQuery
        let with_flag: AgentListQuery =
            serde_json::from_value(serde_json::json!({ "supports_a2a": true }))
                .expect("deserialize");
        // THEN supports_a2a == Some(true)
        assert_eq!(with_flag.supports_a2a, Some(true));

        // AND an absent field gives None (no filtering)
        let without_flag: AgentListQuery =
            serde_json::from_value(serde_json::json!({})).expect("deserialize");
        assert_eq!(without_flag.supports_a2a, None);
    }

    #[tokio::test]
    async fn test_agent_list_filter_a2a_only() {
        // GIVEN 3 agents: 2 A2A + 1 non-A2A
        let (router, registry) = test_router();

        let mut m1 = test_manifest("excel-worker");
        m1.supports_a2a = true;
        let id1 = registry.register(m1).await.expect("register excel-worker");
        registry
            .update_state(id1.as_str(), ProcessState::Active)
            .await
            .expect("activate excel-worker");

        let mut m2 = test_manifest("csv-data-worker");
        m2.supports_a2a = true;
        let id2 = registry
            .register(m2)
            .await
            .expect("register csv-data-worker");
        registry
            .update_state(id2.as_str(), ProcessState::Active)
            .await
            .expect("activate csv-data-worker");

        let m3 = test_manifest("sdk-demo");
        let id3 = registry.register(m3).await.expect("register sdk-demo");
        registry
            .update_state(id3.as_str(), ProcessState::Active)
            .await
            .expect("activate sdk-demo");

        // WHEN GET /api/v1/agents?supports_a2a=true
        let req = Request::builder()
            .uri("/api/v1/agents?supports_a2a=true")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN only 2 A2A agents are returned, sdk-demo is absent
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let agents = json["agents"].as_array().expect("agents array");
        assert_eq!(agents.len(), 2);
        assert!(agents
            .iter()
            .all(|a| a["supports_a2a"].as_bool().unwrap_or(false)));
        let names: Vec<&str> = agents.iter().filter_map(|a| a["name"].as_str()).collect();
        assert!(names.contains(&"excel-worker"));
        assert!(names.contains(&"csv-data-worker"));
        assert!(!names.contains(&"sdk-demo"));
    }

    #[tokio::test]
    async fn test_start_agent_registers_a_stored_agent() {
        // GIVEN a store holding one enabled agent
        let data = tempfile::tempdir().expect("tempdir");
        store_agent(data.path(), "hello-agent", true);
        let (router, registry) = test_router_over(data.path().to_path_buf());

        // WHEN POST /api/v1/agents names it
        let resp = start(&router, "hello-agent").await;

        // THEN 201, the agent is active in the registry under its mission,
        // and nothing but its definition was read
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp).await;
        assert_eq!(json["state"], "active");
        assert_eq!(json["name"], "hello-agent");
        let id = json["agent_id"].as_str().expect("agent_id");
        let entry = registry
            .get_agent(id)
            .await
            .expect("registry call")
            .expect("agent registered");
        assert_eq!(entry.process_state, ProcessState::Active);
        assert_eq!(entry.manifest.description, "Says hello.");
    }

    #[tokio::test]
    async fn test_start_agent_refuses_what_is_not_a_stored_enabled_agent() {
        // GIVEN a store holding a disabled agent and an agent already started
        let data = tempfile::tempdir().expect("tempdir");
        store_agent(data.path(), "asleep", false);
        store_agent(data.path(), "awake", true);
        let (router, registry) = test_router_over(data.path().to_path_buf());
        assert_eq!(start(&router, "awake").await.status(), StatusCode::CREATED);

        // WHEN an unknown name, the disabled agent, the started one, the
        // Apollia agent and a Python path are each started
        let unknown = start(&router, "nobody").await;
        let disabled = start(&router, "asleep").await;
        let twice = start(&router, "awake").await;
        let built_in = start(&router, apollia_tools::APOLLIA_AGENT_ID).await;
        let path = start(&router, "/path/to/hello_agent.py").await;

        // THEN each is refused with its own status, and the registry holds
        // the one agent that was started
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
        assert_eq!(disabled.status(), StatusCode::CONFLICT);
        assert_eq!(twice.status(), StatusCode::CONFLICT);
        assert_eq!(built_in.status(), StatusCode::CONFLICT);
        assert_eq!(path.status(), StatusCode::NOT_FOUND);
        let listed = registry.list_agents().await.expect("list");
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn test_a_stopped_agent_is_started_again() {
        // GIVEN a stored agent started, then stopped
        let data = tempfile::tempdir().expect("tempdir");
        store_agent(data.path(), "hello-agent", true);
        let (router, registry) = test_router_over(data.path().to_path_buf());
        assert_eq!(
            start(&router, "hello-agent").await.status(),
            StatusCode::CREATED
        );
        let stop = Request::builder()
            .method("DELETE")
            .uri("/api/v1/agents/hello-agent")
            .body(Body::empty())
            .expect("build request");
        let stopped = router.clone().oneshot(stop).await.expect("request failed");
        assert_eq!(stopped.status(), StatusCode::OK);

        // WHEN it is started again
        let again = start(&router, "hello-agent").await;

        // THEN it is active once more, under one name
        assert_eq!(again.status(), StatusCode::CREATED);
        let id = registry
            .find_by_name("hello-agent")
            .await
            .expect("registry call")
            .expect("registered");
        let entry = registry
            .get_agent(id.as_str())
            .await
            .expect("registry call")
            .expect("agent registered");
        assert_eq!(entry.process_state, ProcessState::Active);
    }
}
