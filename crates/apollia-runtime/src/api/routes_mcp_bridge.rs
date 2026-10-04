//! REST routes of the MCP bridge: what `apollia-os mcp server` serves to an
//! MCP client, and which agents and workflows the user exposes.
//!
//! The bridge serves the agents and workflows marked exposed
//! ([`apollia_tools::ExposedItemRepository`]) and nothing else. It is off by
//! default: until `[mcp] server_enabled` is turned on in `apollia.toml`, the
//! two bridge routes refuse with `403` and the code `bridge_disabled`. The
//! setting is read again on every request, so turning it on or off applies
//! to the next bridge that opens, without a restart.
//!
//! An agent becomes the tool `agent__<id>`, which takes one message; a
//! workflow `<package>/<workflow>` becomes `workflow__<package>_<workflow>`,
//! which takes the inputs its schema declares. Tool names keep to
//! `[A-Za-z0-9_-]` and 64 characters, which the model APIs require; two items
//! that would share a name are both left out rather than one shadowing the
//! other. A call runs the item to its end and answers its final result.

mod catalogue;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_core::config::{resolve_setting, PolicySettings};
use apollia_tools::{ExposedItemRepository, ExposedKind};

use crate::agents::task::AgentTaskLauncher;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::workflows::WorkflowError;

use catalogue::{catalogue, BridgeItem, Target};

/// The named setting that turns the bridge on.
pub const BRIDGE_SETTING: &str = "mcp.server_enabled";

/// Error body of the MCP bridge routes.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct McpBridgeErrorBody {
    /// What went wrong, in words a person can act on.
    pub error: String,
    /// Stable code: `bridge_disabled`, `unknown_tool`, `invalid_arguments`,
    /// `invalid_kind`, `unknown_item`, `unavailable` or `internal`.
    pub code: String,
}

type RouteError = (StatusCode, Json<McpBridgeErrorBody>);

fn error(status: StatusCode, code: &str, message: impl ToString) -> RouteError {
    (
        status,
        Json(McpBridgeErrorBody {
            error: message.to_string(),
            code: code.to_string(),
        }),
    )
}

/// One tool of the bridge, an MCP tool descriptor.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpBridgeTool {
    /// `agent__<id>` or `workflow__<package>_<workflow>`.
    pub name: String,
    /// What the agent or the workflow does.
    pub description: String,
    /// JSON Schema of the arguments.
    #[schema(value_type = Object)]
    pub input_schema: serde_json::Value,
}

/// The tools the bridge serves.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct McpBridgeToolList {
    pub tools: Vec<McpBridgeTool>,
}

/// Body of `POST /api/v1/mcp/bridge/call`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct McpBridgeCall {
    /// The tool, as `GET /api/v1/mcp/bridge/tools` names it.
    pub name: String,
    /// The arguments: `{"message": "..."}` for an agent, the inputs for a
    /// workflow.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub arguments: serde_json::Value,
}

/// The end of a bridge call.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpBridgeCallResult {
    /// The answer of the agent, or what the workflow returned; the failure
    /// when the run did not complete.
    pub text: String,
    /// `true` when the run failed or was stopped.
    pub is_error: bool,
    /// `completed`, `failed` or `canceled`.
    pub status: String,
    /// `agent` or `workflow`.
    pub kind: String,
    /// The agent name, or `<package>/<workflow>`.
    pub item_id: String,
}

/// One item marked exposed.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpExposedItem {
    /// `agent` or `workflow`.
    pub kind: String,
    /// The agent name, or `<package>/<workflow>`.
    pub item_id: String,
}

/// The items marked exposed.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct McpExposedItemList {
    pub items: Vec<McpExposedItem>,
}

/// Body of `PUT /api/v1/mcp/exposed`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpExposure {
    /// `agent` or `workflow`.
    pub kind: String,
    /// The agent name, or `<package>/<workflow>`.
    pub item_id: String,
    /// Whether the bridge serves the item.
    pub exposed: bool,
}

/// Refuses unless the user turned the bridge on, reading `apollia.toml` now.
async fn require_enabled<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<(), RouteError> {
    let disabled = || {
        error(
            StatusCode::FORBIDDEN,
            "bridge_disabled",
            format!(
                "the MCP bridge is turned off; turn it on with \
                 `apollia-os config set {BRIDGE_SETTING} true`"
            ),
        )
    };
    let Some(path) = state.config_path.clone() else {
        return Err(disabled());
    };
    let resolved = tokio::task::spawn_blocking(move || {
        resolve_setting(&path, BRIDGE_SETTING, &PolicySettings::none())
    })
    .await
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?;
    if resolved.value == serde_json::Value::Bool(true) {
        Ok(())
    } else {
        tracing::event!(tracing::Level::INFO, "mcp.bridge.refused");
        Err(disabled())
    }
}

/// `GET /api/v1/mcp/bridge/tools`: the exposed items, as MCP tools.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/bridge/tools",
    tag = "mcp",
    responses(
        (status = 200, description = "The tools the bridge serves", body = McpBridgeToolList),
        (status = 403, description = "The bridge is turned off (`bridge_disabled`)", body = McpBridgeErrorBody),
        (status = 500, description = "A store could not be read", body = McpBridgeErrorBody),
    )
)]
pub async fn list_bridge_tools<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<McpBridgeToolList>, RouteError> {
    require_enabled(&state).await?;
    let items = catalogue(&state).await?;
    tracing::event!(
        tracing::Level::INFO,
        tools = items.len(),
        "mcp.bridge.listed"
    );
    Ok(Json(McpBridgeToolList {
        tools: items
            .into_iter()
            .map(|item| McpBridgeTool {
                name: item.name,
                description: item.description,
                input_schema: item.input_schema,
            })
            .collect(),
    }))
}

/// `POST /api/v1/mcp/bridge/call`: run an exposed item to its end.
#[utoipa::path(
    post,
    path = "/api/v1/mcp/bridge/call",
    tag = "mcp",
    request_body = McpBridgeCall,
    responses(
        (status = 200, description = "The run ended; `isError` says whether it failed", body = McpBridgeCallResult),
        (status = 400, description = "The arguments do not fit the tool (`invalid_arguments`)", body = McpBridgeErrorBody),
        (status = 403, description = "The bridge is turned off (`bridge_disabled`)", body = McpBridgeErrorBody),
        (status = 404, description = "No exposed item carries this name (`unknown_tool`)", body = McpBridgeErrorBody),
        (status = 503, description = "The agent executor or the workflow engine is not running (`unavailable`)", body = McpBridgeErrorBody),
    )
)]
pub async fn call_bridge_tool<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(call): Json<McpBridgeCall>,
) -> Result<Json<McpBridgeCallResult>, RouteError> {
    require_enabled(&state).await?;
    let item = catalogue(&state)
        .await?
        .into_iter()
        .find(|item| item.name == call.name)
        .ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "unknown_tool",
                format!("no exposed agent or workflow is named '{}'", call.name),
            )
        })?;
    tracing::event!(
        tracing::Level::INFO,
        kind = item.kind.as_str(),
        item = %item.item_id,
        "mcp.bridge.call"
    );
    match &item.target {
        Target::Agent(agent) => run_agent(&state, &item, agent, &call.arguments).await,
        Target::Workflow => run_workflow(&state, &item, call.arguments).await,
    }
}

async fn run_agent<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    item: &BridgeItem,
    agent: &apollia_tools::ResolvedAgent,
    arguments: &serde_json::Value,
) -> Result<Json<McpBridgeCallResult>, RouteError> {
    let message = arguments
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "invalid_arguments",
                format!("'{}' takes a non-empty text argument 'message'", item.name),
            )
        })?
        .to_string();
    let Some(chat) = state.chat_manager.clone() else {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "no agent executor in this runtime",
        ));
    };
    let launcher = AgentTaskLauncher {
        router: state.router_handle.clone(),
        chat,
        repository: state.task_repository.clone(),
        event_bus: state.event_sender.clone(),
        observability: state.observability(),
        plan_cache: state.plan_cache.clone(),
    };
    let input = serde_json::json!({ "message": message });
    let (_task, outcome) = launcher
        .launch_watched(agent, &input, message)
        .await
        .map_err(|e| error(StatusCode::SERVICE_UNAVAILABLE, "unavailable", e))?;
    let outcome = outcome.await.map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            format!("the run of agent '{}' was dropped", item.item_id),
        )
    })?;
    Ok(Json(result(item, outcome.status, outcome.output)))
}

async fn run_workflow<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    item: &BridgeItem,
    arguments: serde_json::Value,
) -> Result<Json<McpBridgeCallResult>, RouteError> {
    let Some(engine) = state.workflows.clone() else {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "the workflow engine is not running in this runtime",
        ));
    };
    let inputs = if arguments.is_null() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        arguments
    };
    let row = engine
        .run_to_end(&item.item_id, inputs)
        .await
        .map_err(|e| match e {
            WorkflowError::InvalidInput(input) => error(
                StatusCode::BAD_REQUEST,
                "invalid_arguments",
                format!("input '{}': {}", input.field, input.message),
            ),
            other => error(StatusCode::SERVICE_UNAVAILABLE, "unavailable", other),
        })?;
    let text = if row.status == "completed" {
        match row.output {
            Some(serde_json::Value::String(text)) => text,
            Some(other) => other.to_string(),
            None => String::new(),
        }
    } else {
        row.error.unwrap_or_default()
    };
    Ok(Json(result(item, &row.status, text)))
}

fn result(item: &BridgeItem, status: &str, text: String) -> McpBridgeCallResult {
    McpBridgeCallResult {
        text,
        is_error: status != "completed",
        status: status.to_string(),
        kind: item.kind.as_str().to_string(),
        item_id: item.item_id.clone(),
    }
}

/// Opens the exposure store of `state`, off the async workers.
async fn with_store<B, T, F>(state: &AppState<B>, op: F) -> Result<T, RouteError>
where
    B: ExecutionBackend + Clone,
    T: Send + 'static,
    F: FnOnce(&ExposedItemRepository) -> Result<T, apollia_tools::ExposedItemError>
        + Send
        + 'static,
{
    let path = apollia_core::paths::DataFile::Agents.path(&state.data_dir);
    tokio::task::spawn_blocking(move || {
        let repo = ExposedItemRepository::open(&path)?;
        op(&repo)
    })
    .await
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?
    .map_err(|e| match e {
        apollia_tools::ExposedItemError::EmptyId => {
            error(StatusCode::BAD_REQUEST, "unknown_item", e)
        }
        other => error(StatusCode::INTERNAL_SERVER_ERROR, "internal", other),
    })
}

/// `GET /api/v1/mcp/exposed`: the agents and workflows marked exposed.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/exposed",
    tag = "mcp",
    responses(
        (status = 200, description = "The items marked exposed", body = McpExposedItemList),
        (status = 500, description = "agents.db could not be read", body = McpBridgeErrorBody),
    )
)]
pub async fn list_exposed<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<McpExposedItemList>, RouteError> {
    let items = with_store(&state, |repo| repo.list()).await?;
    Ok(Json(McpExposedItemList {
        items: items
            .into_iter()
            .map(|item| McpExposedItem {
                kind: item.kind.as_str().to_string(),
                item_id: item.item_id,
            })
            .collect(),
    }))
}

/// `PUT /api/v1/mcp/exposed`: mark an agent or a workflow exposed, or not.
///
/// Exposing an item checks that it exists; withdrawing one does not, so an
/// item that has since been removed can still be withdrawn.
#[utoipa::path(
    put,
    path = "/api/v1/mcp/exposed",
    tag = "mcp",
    request_body = McpExposure,
    responses(
        (status = 200, description = "The flag, as stored", body = McpExposedItemState),
        (status = 400, description = "The kind is neither `agent` nor `workflow` (`invalid_kind`), or the identifier is empty (`unknown_item`)", body = McpBridgeErrorBody),
        (status = 404, description = "No agent or workflow by that identifier (`unknown_item`)", body = McpBridgeErrorBody),
        (status = 500, description = "agents.db could not be written", body = McpBridgeErrorBody),
    )
)]
pub async fn set_exposed<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<McpExposure>,
) -> Result<Json<McpExposedItemState>, RouteError> {
    let kind = ExposedKind::parse(&body.kind).ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_kind",
            format!("kind '{}' is neither 'agent' nor 'workflow'", body.kind),
        )
    })?;
    let item_id = body.item_id.trim().to_string();
    if body.exposed && !item_id.is_empty() && !catalogue::exists(&state, kind, &item_id).await? {
        return Err(error(
            StatusCode::NOT_FOUND,
            "unknown_item",
            format!("no {} named '{item_id}'", kind.as_str()),
        ));
    }
    let exposed = body.exposed;
    let stored_id = item_id.clone();
    with_store(&state, move |repo| repo.set(kind, &stored_id, exposed)).await?;
    tracing::event!(
        tracing::Level::INFO,
        kind = kind.as_str(),
        item = %item_id,
        exposed = exposed,
        "mcp.exposure.set"
    );
    Ok(Json(McpExposedItemState {
        kind: kind.as_str().to_string(),
        item_id,
        exposed,
    }))
}

/// The exposure flag of one item, as stored.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpExposedItemState {
    /// `agent` or `workflow`.
    pub kind: String,
    /// The agent name, or `<package>/<workflow>`.
    pub item_id: String,
    /// Whether the bridge serves the item.
    pub exposed: bool,
}

/// The MCP bridge routes.
pub fn mcp_bridge_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/mcp/bridge/tools", get(list_bridge_tools::<B>))
        .route("/api/v1/mcp/bridge/call", post(call_bridge_tool::<B>))
        .route(
            "/api/v1/mcp/exposed",
            get(list_exposed::<B>).put(set_exposed::<B>),
        )
}

#[cfg(test)]
mod tests;
