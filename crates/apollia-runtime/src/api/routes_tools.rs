//! REST routes for the tool catalogue, `GET /api/v1/tools` and `GET /api/v1/tools/:name`.
//!
//! Exposes the [`ToolRegistry`](apollia_tools::ToolRegistryHandle) actor over HTTP
//! so the CLI (`apollia-os tools list|describe`) and other clients can introspect
//! the registered tools without needing direct access to the actor handle.
//!
//! Routes return `503 Service Unavailable` when the runtime was started without
//! a [`ToolRegistryHandle`] in `AppState` (e.g. in certain unit-test contexts).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::api::server::AppState;
use crate::coordinator::DynBackend;
use crate::coordinator::ExecutionBackend;

/// Response body for `GET /api/v1/tools`.
#[derive(Serialize)]
pub struct ToolListResponse {
    /// All descriptors currently registered in the tool catalogue.
    tools: Vec<apollia_tools::ToolDescriptor>,
}

/// Response body for `GET /api/v1/tools/:name`.
#[derive(Serialize)]
pub struct ToolDetailResponse {
    /// The requested tool descriptor.
    #[serde(flatten)]
    tool: apollia_tools::ToolDescriptor,
}

/// Error response body.
#[derive(Serialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

/// `GET /api/v1/tools`, return all registered tool descriptors.
///
/// Returns `200 OK` with the list, even when empty.
/// Returns `503` if the tool registry is not available in the current runtime context.
/// Returns `500` if the registry actor has terminated unexpectedly.
pub async fn list_tools<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
) -> Result<(StatusCode, Json<ToolListResponse>), (StatusCode, Json<ErrorResponse>)> {
    let registry = state.tool_registry_handle.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "tool registry not available".into(),
            }),
        )
    })?;

    let tools = registry.list().await.map_err(|e| {
        tracing::error!(error = %e, "tool registry actor error in list_tools");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "tool registry actor error".into(),
            }),
        )
    })?;

    Ok((StatusCode::OK, Json(ToolListResponse { tools })))
}

/// `GET /api/v1/tools/:name`, return the descriptor for a specific tool.
///
/// Returns `200 OK` with the descriptor, `404 Not Found` if the tool is not registered,
/// `503` if the tool registry is not available, or `500` on actor error.
pub async fn describe_tool<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<(StatusCode, Json<ToolDetailResponse>), (StatusCode, Json<ErrorResponse>)> {
    let registry = state.tool_registry_handle.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "tool registry not available".into(),
            }),
        )
    })?;

    let tool = registry.get(&name).await.map_err(|e| {
        tracing::error!(error = %e, tool = %name, "tool registry actor error in describe_tool");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "tool registry actor error".into(),
            }),
        )
    })?;

    match tool {
        Some(descriptor) => Ok((
            StatusCode::OK,
            Json(ToolDetailResponse { tool: descriptor }),
        )),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("tool not found: {name}"),
            }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::server::AppState;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use apollia_tools::ToolRegistryHandle;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use std::future::Future;
    use std::pin::Pin;
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
            _task: AIPTask,
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

    /// Build a test router with a real ToolRegistry populated with all 10 native tools.
    async fn test_router_with_tools() -> Router {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let tool_registry = ToolRegistryHandle::start();

        let descriptors = [
            apollia_tools::tools::bash_executor::BashExecutor::descriptor(),
            apollia_tools::tools::python_executor::PythonExecutor::descriptor(),
            apollia_tools::tools::file_read::FileRead::descriptor(),
            apollia_tools::tools::file_write::FileWrite::descriptor(),
            apollia_tools::tools::file_edit::FileEdit::descriptor(),
            apollia_tools::tools::file_list::FileList::descriptor(),
            apollia_tools::tools::file_glob::FileGlob::descriptor(),
            apollia_tools::tools::file_grep::FileGrep::descriptor(),
            apollia_tools::tools::http_fetch::HttpFetch::descriptor(),
            apollia_tools::tools::memory_search::MemorySearchTool::descriptor(),
        ];
        for descriptor in descriptors {
            tool_registry
                .register(descriptor)
                .await
                .expect("register failed");
        }

        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: Some(tool_registry),
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        };

        Router::new()
            .route("/api/v1/tools", get(list_tools::<MockBackend>))
            .route("/api/v1/tools/:name", get(describe_tool::<MockBackend>))
            .with_state(state)
    }

    /// Build a test router without a ToolRegistry (simulates missing handle).
    fn test_router_no_registry() -> Router {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);

        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        };

        Router::new()
            .route("/api/v1/tools", get(list_tools::<MockBackend>))
            .route("/api/v1/tools/:name", get(describe_tool::<MockBackend>))
            .with_state(state)
    }

    #[tokio::test]
    async fn api_tools_list_returns_10_tools() {
        // GIVEN a registry with all 10 native tools registered
        let router = test_router_with_tools().await;

        // WHEN GET /api/v1/tools
        let req = Request::builder()
            .uri("/api/v1/tools")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 + exactly 10 tools
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["tools"].as_array().expect("tools must be array");
        assert_eq!(tools.len(), 10);
    }

    #[tokio::test]
    async fn api_tools_get_by_name_returns_descriptor() {
        // GIVEN a registry with all 10 native tools registered
        let router = test_router_with_tools().await;

        // WHEN GET /api/v1/tools/file_read
        let req = Request::builder()
            .uri("/api/v1/tools/file_read")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 + complete descriptor with all expected fields
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "file_read");
        assert_eq!(json["version"], "1.0.0");
        assert!(
            !json["input_schema"].is_null(),
            "input_schema must be present"
        );
        assert!(
            !json["output_schema"].is_null(),
            "output_schema must be present"
        );
        assert!(
            !json["sandbox_profile"].is_null(),
            "sandbox_profile must be present"
        );
    }

    #[tokio::test]
    async fn api_tools_response_includes_tags() {
        // GIVEN a registry with all 10 native tools registered
        let router = test_router_with_tools().await;

        // WHEN GET /api/v1/tools
        let req = Request::builder()
            .uri("/api/v1/tools")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN every tool has a non-empty tags array
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["tools"].as_array().expect("tools must be array");
        for tool in tools {
            let name = tool["name"].as_str().unwrap_or("<unnamed>");
            let tags = tool["tags"]
                .as_array()
                .unwrap_or_else(|| panic!("tool '{name}' is missing 'tags' field"));
            assert!(!tags.is_empty(), "tool '{name}' has empty tags");
        }
    }

    #[tokio::test]
    async fn test_describe_tool_unknown_returns_404() {
        // GIVEN a registry with all 10 native tools, "unknown_tool" is not among them
        let router = test_router_with_tools().await;

        // WHEN GET /api/v1/tools/unknown_tool
        let req = Request::builder()
            .uri("/api/v1/tools/unknown_tool")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 404 with error message naming the missing tool
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("unknown_tool"));
    }

    #[tokio::test]
    async fn test_list_tools_without_registry_returns_503() {
        // GIVEN an AppState with no tool_registry_handle
        let router = test_router_no_registry();

        // WHEN GET /api/v1/tools
        let req = Request::builder()
            .uri("/api/v1/tools")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 503
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_describe_tool_without_registry_returns_503() {
        // GIVEN an AppState with no tool_registry_handle
        let router = test_router_no_registry();

        // WHEN GET /api/v1/tools/anything
        let req = Request::builder()
            .uri("/api/v1/tools/anything")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 503
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
