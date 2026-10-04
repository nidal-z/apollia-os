//! Tauri IPC commands for task management.
//!
//! `list_tasks` and `submit_task` delegate to the runtime handles.
//! `get_task_timeline` calls the internal REST API `GET /api/v1/tasks/{id}/timeline`
//! to avoid duplicating the aggregation logic. The list, the launch, the
//! detail, the cancellation, the deletion and the delegation tree also serve
//! a desktop attached to a daemon, through the routes of `/api/v1/tasks`.

use apollia_core::{AIPInput, AIPPart, TextPart};
use apollia_runtime::api::routes_tasks::record::TaskRecordDeleted;
use apollia_runtime::api::routes_tasks::summaries::task_summaries;
use tauri::State;

use super::http_get_json;
use crate::attached::{segment, RuntimeLink};

/// Optional filter for the task list, the one the daemon route reads.
pub use apollia_runtime::api::routes_tasks::summaries::TaskFilter;

/// Summary of a task for display in the UI, the one the daemon route answers.
pub use apollia_runtime::api::routes_tasks::summaries::TaskSummary;

/// Node of the A2A delegation tree, the one the runtime route answers.
pub use apollia_runtime::api::routes_tasks::delegation::DelegationTreeNode;

/// Lists all tasks with optional filtering by status or agent.
///
/// Merges two sources:
/// 1. **Runtime** (in-memory TaskRouter): active tasks for the current session.
/// 2. **SQLite** (TaskRepository): historical tasks persisted across restarts.
///
/// Runtime tasks take priority: if a task exists in both sources, only the
/// runtime version is kept. Attached to a daemon, the daemon builds the same
/// list with the same function.
#[tauri::command]
pub async fn list_tasks(
    link: State<'_, RuntimeLink>,
    filter: Option<TaskFilter>,
) -> Result<Vec<TaskSummary>, String> {
    let filter = filter.unwrap_or_default();
    match link.inner() {
        RuntimeLink::Embedded(state) => task_summaries(
            &state.router_handle,
            &state.registry_handle,
            state.task_repository.as_deref(),
            &filter,
        )
        .await
        .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => {
            let mut query = Vec::new();
            if let Some(status) = &filter.status {
                query.push(format!("status={}", segment(status)));
            }
            if let Some(agent_id) = &filter.agent_id {
                query.push(format!("agent_id={}", segment(agent_id)));
            }
            let path = match query.is_empty() {
                true => "/api/v1/tasks/summaries".to_string(),
                false => format!("/api/v1/tasks/summaries?{}", query.join("&")),
            };
            Ok(remote.get(&path).await?)
        }
    }
}

/// Submits a task to an agent and returns the generated `TaskId`.
///
/// Builds an `AIPInput` from the raw text provided by the frontend and submits
/// it via `TaskRouterHandle::submit()`, or `POST /api/v1/tasks` when attached
/// to a daemon. The tools the run may call are the access list of the agent.
#[tauri::command]
pub async fn submit_task(
    link: State<'_, RuntimeLink>,
    agent_id: String,
    input: String,
) -> Result<String, String> {
    let aip_input = AIPInput {
        parts: vec![AIPPart::Text(TextPart { text: input })],
    };

    let state = match link.inner() {
        RuntimeLink::Embedded(state) => state,
        RuntimeLink::Attached(remote) => {
            let body = serde_json::json!({ "agent_id": agent_id, "input": aip_input });
            let accepted: serde_json::Value = remote.post("/api/v1/tasks", &body).await?;
            return accepted
                .get("task_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| "missing task_id in response".to_string());
        }
    };
    let task_id = state
        .router_handle
        .submit(&agent_id, aip_input)
        .await
        .map_err(|e| e.to_string())?;

    Ok(task_id.to_string())
}

/// Cancels a running task.
///
/// Calls `DELETE /api/v1/tasks/{id}` via the internal REST API, or the
/// daemon's when attached.
/// Returns `true` if the task was cancelled, `false` if already finished or not found.
#[tauri::command]
pub async fn cancel_task(link: State<'_, RuntimeLink>, task_id: String) -> Result<bool, String> {
    let path = format!("/api/v1/tasks/{}", segment(&task_id));
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            match super::http_delete_json(state.api_port, &path).await {
                Ok(_) => Ok(true),
                Err(e) if e.contains("404") => Ok(false),
                Err(e) => Err(e),
            }
        }
        RuntimeLink::Attached(remote) => match remote.delete::<serde_json::Value>(&path).await {
            Ok(_) => Ok(true),
            Err(crate::attached::AttachedError::Refused { status: 404, .. }) => Ok(false),
            Err(e) => Err(e.into()),
        },
    }
}

/// Hard-deletes a persisted task record (and its approval rows) from the store.
///
/// Distinct from [`cancel_task`], which cancels an in-flight task but keeps its
/// history: this removes the record entirely, used by the "delete" affordance
/// on a finished or cancelled task. Returns `true` when a record was removed,
/// `false` when no persisted task matched (already gone, or runtime-only with
/// nothing persisted). Errors only when the persistence layer is unavailable.
/// Served by `DELETE /api/v1/tasks/{id}/record`, in this process or on the
/// daemon.
#[tauri::command]
pub async fn delete_task(link: State<'_, RuntimeLink>, task_id: String) -> Result<bool, String> {
    delete_task_on(link.inner(), &task_id).await
}

/// The body of [`delete_task`], on either arm of the seam.
pub(crate) async fn delete_task_on(link: &RuntimeLink, task_id: &str) -> Result<bool, String> {
    let path = format!("/api/v1/tasks/{}/record", segment(task_id));
    let answer = link.delete_json(&path).await?;
    let deleted: TaskRecordDeleted =
        serde_json::from_value(answer).map_err(|e| format!("invalid deletion answer: {e}"))?;
    Ok(deleted.deleted)
}

/// Fetches a task's timeline via the internal REST API.
///
/// Calls `GET /api/v1/tasks/{id}/timeline`, which aggregates events from 5
/// SQLite sources (transitions, plans, LLM calls, tool calls, HITL). The result
/// is returned as-is to the frontend.
#[tauri::command]
pub async fn get_task_timeline(
    link: State<'_, RuntimeLink>,
    task_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let path = format!("/api/v1/tasks/{}/timeline", segment(&task_id));
    let json: serde_json::Value = match link.inner() {
        RuntimeLink::Embedded(state) => http_get_json(state.api_port, &path).await?,
        RuntimeLink::Attached(remote) => remote.get(&path).await?,
    };

    match json.get("events").and_then(|v| v.as_array()) {
        Some(events) => Ok(events.clone()),
        None => {
            if let Some(arr) = json.as_array() {
                Ok(arr.clone())
            } else {
                Ok(vec![json])
            }
        }
    }
}

/// Fetches the A2A delegation tree for a task.
///
/// The root is the parent task; the children are the delegations recorded in
/// `task_sidechains`. The runtime builds it (`GET
/// /api/v1/tasks/{id}/delegation-tree`), in this process or on the daemon; a
/// task with no recorded delegation is a root with no children.
#[tauri::command]
pub async fn get_delegation_tree(
    link: State<'_, RuntimeLink>,
    task_id: String,
) -> Result<DelegationTreeNode, String> {
    delegation_tree_on(link.inner(), &task_id).await
}

/// The body of [`get_delegation_tree`], on either arm of the seam.
pub(crate) async fn delegation_tree_on(
    link: &RuntimeLink,
    task_id: &str,
) -> Result<DelegationTreeNode, String> {
    let path = format!("/api/v1/tasks/{}/delegation-tree", segment(task_id));
    let answer = link.get_json(&path).await?;
    serde_json::from_value(answer).map_err(|e| format!("invalid delegation tree: {e}"))
}

#[cfg(test)]
mod attached_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::TaskStatus;
    use apollia_runtime::api::routes_tasks::summaries::status_label as status_to_string;

    #[test]
    fn test_status_to_string_all_variants() {
        // GIVEN all TaskStatus variants
        // WHEN converted to string
        // THEN each produces the expected snake_case representation
        assert_eq!(status_to_string(&TaskStatus::Submitted), "submitted");
        assert_eq!(status_to_string(&TaskStatus::Working), "working");
        assert_eq!(status_to_string(&TaskStatus::Completed), "completed");
        assert_eq!(status_to_string(&TaskStatus::Failed), "failed");
        assert_eq!(
            status_to_string(&TaskStatus::InputRequired),
            "input_required"
        );
        assert_eq!(status_to_string(&TaskStatus::Canceled), "canceled");
    }

    #[test]
    fn test_task_summary_serializes_to_json() {
        // GIVEN a TaskSummary struct
        let summary = TaskSummary {
            id: "task-001".to_string(),
            agent_id: "agent-001".to_string(),
            agent_name: "hello-agent".to_string(),
            status: "completed".to_string(),
            input_preview: "generate report".to_string(),
            output_text: Some("Report generated.".to_string()),
            duration_ms: Some(1200),
            created_at: "2026-03-13T10:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&summary).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["id"], "task-001");
        assert_eq!(json["agent_name"], "hello-agent");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["duration_ms"], 1200);
    }

    #[test]
    fn test_task_filter_deserializes() {
        // GIVEN a JSON filter with status only
        let json = serde_json::json!({ "status": "working" });

        // WHEN deserialized
        let filter: TaskFilter = serde_json::from_value(json).expect("deserialize");

        // THEN the status field is populated
        assert_eq!(filter.status.as_deref(), Some("working"));
        assert!(filter.agent_id.is_none());
    }

    #[test]
    fn test_task_filter_deserializes_with_agent() {
        // GIVEN a JSON filter with both status and agent_id
        let json = serde_json::json!({ "status": "completed", "agent_id": "agent-42" });

        // WHEN deserialized
        let filter: TaskFilter = serde_json::from_value(json).expect("deserialize");

        // THEN both fields are populated
        assert_eq!(filter.status.as_deref(), Some("completed"));
        assert_eq!(filter.agent_id.as_deref(), Some("agent-42"));
    }

    #[test]
    fn test_task_summary_with_null_output_serializes() {
        // GIVEN a TaskSummary with no output (task still running)
        let summary = TaskSummary {
            id: "task-002".to_string(),
            agent_id: "agent-002".to_string(),
            agent_name: "review-agent".to_string(),
            status: "working".to_string(),
            input_preview: "analyze code".to_string(),
            output_text: None,
            duration_ms: None,
            created_at: "2026-03-13T11:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&summary).expect("serialize");

        // THEN output_text and duration_ms are null
        assert!(json["output_text"].is_null());
        assert!(json["duration_ms"].is_null());
        assert_eq!(json["status"], "working");
    }
}
