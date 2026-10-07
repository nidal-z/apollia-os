//! `GET /api/v1/tasks/summaries`, the task list an operator reads.
//!
//! `GET /api/v1/tasks` answers what the router holds in memory. This view adds
//! what survived a restart: the tasks of the current process first, then the
//! recent rows of the task store not already listed, each with its agent name,
//! an input preview, its output and its duration. The desktop builds it from
//! its own handles when the runtime runs in its process and reads it here when
//! it attached to a daemon, so both go through [`task_summaries`].

use std::collections::HashSet;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::{AgentId, TaskId, TaskStatus};
use apollia_tools::{PersistedTaskSummary, TaskRepository};

use super::{submit_error_to_response, ErrorResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::registry::AgentRegistryHandle;
use crate::router::{SubmitError, TaskRouterHandle};

/// How many rows of the task store the view reads.
pub const PERSISTED_TASK_LIMIT: usize = 50;

/// How many characters of the input the preview keeps.
const PREVIEW_CHARS: usize = 120;

/// Optional filter of the view.
#[derive(Debug, Default, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaskFilter {
    /// Keep only the tasks in this status (`working`, `completed`...).
    pub status: Option<String>,
    /// Keep only the tasks of this agent, by runtime identifier.
    pub agent_id: Option<String>,
}

/// One task of the view.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TaskSummary {
    /// Unique task identifier.
    pub id: String,
    /// Runtime identifier of the agent, empty for a task read from the store.
    pub agent_id: String,
    /// Name of the agent.
    pub agent_name: String,
    /// Current status.
    pub status: String,
    /// The first characters of the input.
    pub input_preview: String,
    /// Output text, as the store kept it.
    pub output_text: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// ISO 8601 creation date, empty when the store has no row yet.
    pub created_at: String,
}

/// The snake_case name of a status, as the API spells it.
pub fn status_label(status: &TaskStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{status:?}"))
}

/// The tasks of the current process, then the recent stored ones, filtered.
///
/// # Errors
///
/// [`SubmitError::ActorDead`] when the router does not answer.
pub async fn task_summaries<B: ExecutionBackend + Clone>(
    router: &TaskRouterHandle<B>,
    registry: &AgentRegistryHandle,
    repo: Option<&TaskRepository>,
    filter: &TaskFilter,
) -> Result<Vec<TaskSummary>, SubmitError> {
    let live = router.all_tasks().await?;
    let mut seen = HashSet::new();
    let mut summaries = Vec::with_capacity(live.len());
    for task in &live {
        if let Some(summary) = live_summary(registry, repo, filter, task).await {
            seen.insert(summary.id.clone());
            summaries.push(summary);
        }
    }

    // Stored rows carry the agent name, not the runtime identifier.
    let filter_name = match &filter.agent_id {
        Some(id) => agent_name(registry, id).await,
        None => None,
    };
    let Some(repo) = repo else {
        return Ok(summaries);
    };
    let Ok(rows) = repo.list_recent_tasks(PERSISTED_TASK_LIMIT).await else {
        return Ok(summaries);
    };
    for row in rows {
        if seen.contains(&row.task_id) || !stored_matches(filter, filter_name.as_deref(), &row) {
            continue;
        }
        seen.insert(row.task_id.clone());
        summaries.push(TaskSummary {
            id: row.task_id,
            agent_id: String::new(),
            agent_name: row.agent_name,
            status: row.status,
            input_preview: row.input_preview,
            output_text: row.output_text,
            duration_ms: row.duration_ms.map(|ms| ms as u64),
            created_at: row.created_at,
        });
    }
    Ok(summaries)
}

async fn agent_name(registry: &AgentRegistryHandle, agent_id: &str) -> Option<String> {
    registry
        .get_agent(agent_id)
        .await
        .ok()
        .flatten()
        .map(|e| e.manifest.name)
}

async fn live_summary(
    registry: &AgentRegistryHandle,
    repo: Option<&TaskRepository>,
    filter: &TaskFilter,
    (task_id, agent_id, status): &(TaskId, AgentId, TaskStatus),
) -> Option<TaskSummary> {
    let status = status_label(status);
    if filter.status.as_ref().is_some_and(|s| *s != status)
        || filter
            .agent_id
            .as_ref()
            .is_some_and(|a| a != agent_id.as_str())
    {
        return None;
    }
    let detail = match repo {
        Some(repo) => repo.get_task_detail(task_id.as_str()).await.ok().flatten(),
        None => None,
    };
    let (input_preview, output_text, duration_ms, created_at) = match detail {
        Some(d) => (
            d.input_text
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(PREVIEW_CHARS)
                .collect(),
            d.output_text,
            d.duration_ms.map(|ms| ms as u64),
            d.created_at,
        ),
        None => (String::new(), None, None, String::new()),
    };
    Some(TaskSummary {
        id: task_id.to_string(),
        agent_id: agent_id.to_string(),
        agent_name: agent_name(registry, agent_id.as_str())
            .await
            .unwrap_or_default(),
        status,
        input_preview,
        output_text,
        duration_ms,
        created_at,
    })
}

fn stored_matches(filter: &TaskFilter, name: Option<&str>, row: &PersistedTaskSummary) -> bool {
    filter.status.as_ref().is_none_or(|s| *s == row.status)
        && name.is_none_or(|n| n == row.agent_name)
}

/// Handler for `GET /api/v1/tasks/summaries`.
#[utoipa::path(
    get,
    path = "/api/v1/tasks/summaries",
    tag = "tasks",
    params(TaskFilter),
    responses(
        (status = 200, description = "Tasks of the running process, then the recent stored ones", body = Vec<TaskSummary>),
        (status = 500, description = "Task router unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_task_summaries<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(filter): Query<TaskFilter>,
) -> Result<Json<Vec<TaskSummary>>, (StatusCode, Json<ErrorResponse>)> {
    task_summaries(
        &state.router_handle,
        &state.registry_handle,
        state.task_repository.as_deref(),
        &filter,
    )
    .await
    .map(Json)
    .map_err(submit_error_to_response)
}

#[cfg(test)]
mod tests {

    use axum::http::StatusCode;

    use super::*;
    use crate::api::test_support::{app_state, call};

    #[test]
    fn test_a_stored_row_is_kept_by_its_agent_name_and_status() {
        // GIVEN a stored row of agent "veille", completed
        let row = PersistedTaskSummary {
            task_id: "t-1".to_owned(),
            agent_name: "veille".to_owned(),
            status: "completed".to_owned(),
            input_preview: String::new(),
            output_text: None,
            duration_ms: None,
            created_at: String::new(),
        };
        let completed = TaskFilter {
            status: Some("completed".to_owned()),
            agent_id: None,
        };

        // WHEN it is filtered by status and by the resolved agent name
        // THEN it passes on a match and is left out otherwise
        assert!(stored_matches(&completed, Some("veille"), &row));
        assert!(!stored_matches(&completed, Some("docx"), &row));
        assert!(!stored_matches(
            &TaskFilter {
                status: Some("failed".to_owned()),
                agent_id: None
            },
            None,
            &row
        ));
    }

    #[tokio::test]
    async fn test_the_summaries_route_answers_the_view_without_a_store() {
        // GIVEN a daemon without a task store and no task submitted
        let data = tempfile::tempdir().unwrap();
        let state = app_state(data.path().to_path_buf());

        // WHEN the view is read through the router, filter included
        let (status, body) =
            call(state, "GET", "/api/v1/tasks/summaries?status=working", None).await;

        // THEN the static path wins over /tasks/{id} and the list is empty
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, serde_json::json!([]));
    }
}
