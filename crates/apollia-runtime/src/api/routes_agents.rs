//! REST routes for agent management — GET/POST/DELETE `/api/v1/agents`.
//!
//! These routes expose agent lifecycle management via the API. They delegate
//! to [`AgentRegistryHandle`] for state reads and transitions.

use std::path::Path;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::registry::AgentRegistryError;

use apollia_core::{AgentManifest, ProcessState};

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

/// Extract agent name from a file path (stem without extension).
///
/// Falls back to the full path string if no stem is found.
fn agent_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .replace('_', "-")
}

/// Build a minimal manifest from an agent path (MVP simplification).
///
/// Full AIP loading/validation is out of scope for this story.
fn manifest_from_path(path: &str) -> AgentManifest {
    AgentManifest {
        name: agent_name_from_path(path),
        version: "0.0.0".to_string(),
        description: format!("Agent loaded from {path}"),
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
/// Lists all registered agents with their current state.
pub async fn list_agents<B: ExecutionBackend>(
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
            state: state_to_string(&entry.process_state),
            manifest: None,
        })
        .collect();

    Ok(Json(AgentListResponse { agents }))
}

/// Handler for `POST /api/v1/agents`.
///
/// Registers a new agent from the given path. Returns 201 Created with
/// the generated agent_id and initial state "initializing".
///
/// MVP simplification: creates a manifest from the path without actually
/// loading the Python module via AIP.
pub async fn start_agent<B: ExecutionBackend>(
    State(state): State<AppState<B>>,
    Json(req): Json<StartAgentRequest>,
) -> Result<(StatusCode, Json<AgentResponse>), (StatusCode, Json<ErrorResponse>)> {
    let manifest = manifest_from_path(&req.agent_path);

    let agent_id = state
        .registry_handle
        .register(manifest)
        .await
        .map_err(registry_error_to_response)?;

    Ok((
        StatusCode::CREATED,
        Json(AgentResponse {
            agent_id: agent_id.to_string(),
            state: "initializing".to_string(),
            manifest: None,
        }),
    ))
}

/// Handler for `GET /api/v1/agents/{id}`.
///
/// Returns the detail of a single agent including its manifest.
/// Returns 404 if the agent does not exist.
pub async fn get_agent<B: ExecutionBackend>(
    State(state): State<AppState<B>>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<AgentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let entry = state
        .registry_handle
        .get_agent(&agent_id)
        .await
        .map_err(registry_error_to_response)?;

    match entry {
        Some(e) => {
            let manifest_json = serde_json::to_value(&e.manifest).ok();
            Ok(Json(AgentResponse {
                agent_id: e.id.to_string(),
                state: state_to_string(&e.process_state),
                manifest: manifest_json,
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("agent not found: {agent_id}"),
            }),
        )),
    }
}

/// Handler for `DELETE /api/v1/agents/{id}`.
///
/// Initiates agent shutdown by transitioning to `Stopping`.
/// Returns 409 Conflict if the agent is already stopped.
/// Returns 404 if the agent does not exist.
pub async fn stop_agent<B: ExecutionBackend>(
    State(state): State<AppState<B>>,
    AxumPath(agent_id): AxumPath<String>,
) -> Result<Json<AgentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let entry = state
        .registry_handle
        .get_agent(&agent_id)
        .await
        .map_err(registry_error_to_response)?;

    let entry = entry.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("agent not found: {agent_id}"),
            }),
        )
    })?;

    if entry.process_state == ProcessState::Stopped {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: format!("agent already stopped: {agent_id}"),
            }),
        ));
    }

    if entry.process_state == ProcessState::Stopping {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: format!("agent already stopping: {agent_id}"),
            }),
        ));
    }

    state
        .registry_handle
        .update_state(agent_id.as_str(), ProcessState::Stopping)
        .await
        .map_err(registry_error_to_response)?;

    Ok(Json(AgentResponse {
        agent_id,
        state: "stopping".to_string(),
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
                })
            })
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
        }
    }

    fn test_router() -> (Router, AgentRegistryHandle) {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle: registry_handle.clone(),
            event_sender: event_tx,
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

        // THEN 201 Created avec agent_id et state "initializing"
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp).await;
        assert_eq!(json["state"], "initializing");
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

        // THEN 200 avec state "stopping"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["agent_id"], agent_id.as_str());
        assert_eq!(json["state"], "stopping");
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
}
