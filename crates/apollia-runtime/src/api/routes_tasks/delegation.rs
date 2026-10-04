//! `GET /api/v1/tasks/{id}/delegation-tree`, the A2A delegation tree of a task.
//!
//! The root is the task itself, named from the router while it runs and from
//! the task store once it is over; the children are the delegations recorded in
//! `task_sidechains`. The desktop reads it through the listener of the runtime
//! in its process, or through the daemon's when it attached to one.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_tools::TaskRepository;

use super::summaries::{status_label, PERSISTED_TASK_LIMIT};
use super::ErrorResponse;
use crate::a2a::SidechainRow;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::registry::AgentRegistryHandle;
use crate::router::TaskRouterHandle;

/// One node of the delegation tree.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DelegationTreeNode {
    /// Agent identifier (runtime UUID, or name if resolution fails).
    pub agent_id: String,
    /// Human-readable agent name.
    pub agent_name: String,
    /// Current status: `submitted`, `working`, `completed`, `failed`, `running`, etc.
    pub status: String,
    /// ISO 8601 start timestamp (None for the root if unknown).
    pub started_at: Option<String>,
    /// Child delegations (A2A subtasks).
    #[schema(no_recursion)]
    pub children: Vec<DelegationTreeNode>,
}

/// The tree of `task_id`: its root resolved from the router, then from the
/// task store, then named after the task itself, with one child per recorded
/// delegation.
pub async fn delegation_tree<B: ExecutionBackend + Clone>(
    router: &TaskRouterHandle<B>,
    registry: &AgentRegistryHandle,
    repo: Option<&TaskRepository>,
    task_id: &str,
    delegations: Vec<SidechainRow>,
) -> DelegationTreeNode {
    let mut root = DelegationTreeNode {
        agent_id: String::new(),
        agent_name: String::new(),
        status: String::from("unknown"),
        started_at: None,
        children: delegations.into_iter().map(child).collect(),
    };
    if let Ok(all) = router.all_tasks().await {
        if let Some((_, agent_id, status)) = all.iter().find(|(tid, _, _)| tid.as_str() == task_id)
        {
            root.agent_id = agent_id.to_string();
            root.status = status_label(status);
            if let Ok(Some(entry)) = registry.get_agent(agent_id.as_str()).await {
                root.agent_name = entry.manifest.name;
            }
        }
    }
    if root.agent_name.is_empty() {
        if let Some(repo) = repo {
            apply_stored_root(repo, task_id, &mut root).await;
        }
    }
    if root.agent_name.is_empty() {
        root.agent_name = task_id.to_owned();
    }
    if root.agent_id.is_empty() {
        root.agent_id = task_id.to_owned();
    }
    root
}

/// Fills the missing root fields from the task store.
async fn apply_stored_root(repo: &TaskRepository, task_id: &str, root: &mut DelegationTreeNode) {
    let Ok(persisted) = repo.list_recent_tasks(PERSISTED_TASK_LIMIT).await else {
        return;
    };
    let Some(row) = persisted.into_iter().find(|r| r.task_id == task_id) else {
        return;
    };
    root.agent_name = row.agent_name;
    if root.status == "unknown" {
        root.status = row.status;
    }
    root.started_at = Some(row.created_at);
}

fn child(row: SidechainRow) -> DelegationTreeNode {
    DelegationTreeNode {
        agent_id: row.agent_name.clone(),
        agent_name: row.agent_name,
        status: row.status,
        started_at: row.started_at,
        children: Vec::new(),
    }
}

fn unavailable(error: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: error.to_owned(),
        }),
    )
}

/// Handler for `GET /api/v1/tasks/{id}/delegation-tree`.
///
/// A task with no recorded delegation answers its root with no children.
#[utoipa::path(
    get,
    path = "/api/v1/tasks/{id}/delegation-tree",
    tag = "a2a",
    params(("id" = String, Path, description = "Parent task id")),
    responses(
        (status = 200, description = "The task and its delegations", body = DelegationTreeNode),
        (status = 500, description = "The delegations cannot be read", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Sidechain logging not initialized", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_delegation_tree<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(task_id): Path<String>,
) -> Result<Json<DelegationTreeNode>, (StatusCode, Json<ErrorResponse>)> {
    let invoker = state
        .a2a_invoker
        .as_ref()
        .ok_or_else(|| unavailable("A2A invoker not initialized"))?;
    let logger = invoker
        .sidechain_logger()
        .ok_or_else(|| unavailable("sidechain logging not initialized"))?;
    let delegations = logger.list_by_parent(&task_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;
    Ok(Json(
        delegation_tree(
            &state.router_handle,
            &state.registry_handle,
            state.task_repository.as_deref(),
            &task_id,
            delegations,
        )
        .await,
    ))
}

#[cfg(test)]
mod tests;
