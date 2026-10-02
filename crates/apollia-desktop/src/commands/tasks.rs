//! Tauri IPC commands for task management.
//!
//! `list_tasks` and `submit_task` delegate to the runtime handles.
//! `get_task_timeline` calls the internal REST API `GET /api/v1/tasks/{id}/timeline`
//! to avoid duplicating the aggregation logic. The list, the launch, the
//! detail and the cancellation also serve a desktop attached to a daemon,
//! through the routes of `/api/v1/tasks`.

use apollia_core::{AIPInput, AIPPart, TextPart};
use apollia_runtime::api::routes_tasks::summaries::{task_summaries, PERSISTED_TASK_LIMIT};
use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use tauri::State;

use super::http_get_json;
use crate::attached::{segment, RuntimeLink};

/// Optional filter for the task list, the one the daemon route reads.
pub use apollia_runtime::api::routes_tasks::summaries::TaskFilter;

/// Summary of a task for display in the UI, the one the daemon route answers.
pub use apollia_runtime::api::routes_tasks::summaries::TaskSummary;

/// The snake_case name of a status, as the API spells it.
use apollia_runtime::api::routes_tasks::summaries::status_label as status_to_string;

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
/// to a daemon.
///
/// `allowed_tools` and `disallowed_tools` are accepted for compatibility with
/// the CLI (`--allowed-tools` / `--disallowed-tools`) but are not yet propagated
/// to the runtime; they will be passed through the AIP metadata later.
#[tauri::command]
pub async fn submit_task(
    link: State<'_, RuntimeLink>,
    agent_id: String,
    input: String,
    allowed_tools: Option<Vec<String>>,
    disallowed_tools: Option<Vec<String>>,
) -> Result<String, String> {
    // Tool filtering is not yet implemented in the AIPInput type.
    // Log the intent so the UI can see it was received.
    if let Some(ref tools) = allowed_tools {
        tracing::debug!(
            allowed_tools = ?tools,
            detail = "not enforced yet",
            "task.allowed_tools.ignored"
        );
    }
    if let Some(ref tools) = disallowed_tools {
        tracing::debug!(
            disallowed_tools = ?tools,
            detail = "not enforced yet",
            "task.disallowed_tools.ignored"
        );
    }

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
#[tauri::command]
pub async fn delete_task(state: State<'_, RuntimeHandle>, task_id: String) -> Result<bool, String> {
    let Some(repo) = state.task_repository.as_ref() else {
        return Err("task persistence is not available".to_string());
    };
    repo.delete_task(&task_id).await.map_err(|e| e.to_string())
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

/// Node of the A2A delegation tree returned to the frontend.
#[derive(Debug, Serialize)]
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
    pub children: Vec<DelegationTreeNode>,
}

/// Fetches the A2A delegation tree for a task.
///
/// The root is the parent task; the children are the delegations recorded in
/// `task_sidechains` via `GET /api/v1/tasks/{id}/sidechains`.
///
/// If no delegation was recorded for the task (404 or empty list), returns a
/// root tree with no children.
#[tauri::command]
pub async fn get_delegation_tree(
    state: State<'_, RuntimeHandle>,
    task_id: String,
) -> Result<DelegationTreeNode, String> {
    // 1. Resolve the root (agent_id, agent_name, status, started_at).
    let mut root = resolve_runtime_root(&state, &task_id).await;

    // b) Fallback: look in the persisted tasks.
    if root.agent_name.is_empty() {
        apply_persisted_root_fallback(&state, &task_id, &mut root).await;
    }

    if root.agent_name.is_empty() {
        root.agent_name = task_id.clone();
    }

    // 2. Fetch the delegations via the internal REST API.
    let path = format!("/api/v1/tasks/{task_id}/sidechains");
    let children = match http_get_json(state.api_port, &path).await {
        Ok(json) => parse_sidechains(json),
        Err(e) if e.contains("404") => Vec::new(),
        Err(e) => return Err(e),
    };

    Ok(DelegationTreeNode {
        agent_id: if root.agent_id.is_empty() {
            task_id
        } else {
            root.agent_id
        },
        agent_name: root.agent_name,
        status: root.status,
        started_at: root.started_at,
        children,
    })
}

/// Partially-resolved root node fields (before sidechains are attached).
struct RootInfo {
    agent_id: String,
    agent_name: String,
    status: String,
    started_at: Option<String>,
}

/// Resolve the root node from the in-memory runtime (active task), if present.
async fn resolve_runtime_root(state: &RuntimeHandle, task_id: &str) -> RootInfo {
    let mut root = RootInfo {
        agent_id: String::new(),
        agent_name: String::new(),
        status: String::from("unknown"),
        started_at: None,
    };

    let Ok(all) = state.router_handle.all_tasks().await else {
        return root;
    };
    let Some((_, agent_id, status)) = all.iter().find(|(tid, _, _)| tid.as_str() == task_id) else {
        return root;
    };
    root.agent_id = agent_id.to_string();
    root.status = status_to_string(status);
    if let Ok(Some(entry)) = state.registry_handle.get_agent(agent_id.as_str()).await {
        root.agent_name = entry.manifest.name.clone();
    }
    root
}

/// Fill missing root fields from the persisted task store.
async fn apply_persisted_root_fallback(state: &RuntimeHandle, task_id: &str, root: &mut RootInfo) {
    let Some(repo) = state.task_repository.as_ref() else {
        return;
    };
    let Ok(persisted) = repo.list_recent_tasks(PERSISTED_TASK_LIMIT).await else {
        return;
    };
    let Some(row) = persisted.into_iter().find(|r| r.task_id == task_id) else {
        return;
    };
    if root.agent_name.is_empty() {
        root.agent_name = row.agent_name;
    }
    if root.status == "unknown" {
        root.status = row.status;
    }
    root.started_at = Some(row.created_at);
}

/// Converts the `/sidechains` JSON response into child nodes.
fn parse_sidechains(json: serde_json::Value) -> Vec<DelegationTreeNode> {
    let arr = match json.as_array() {
        Some(arr) => arr,
        None => return Vec::new(),
    };
    arr.iter()
        .map(|row| {
            let agent_name = row
                .get("agent_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let status = row
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let started_at = row
                .get("started_at")
                .and_then(|v| v.as_str())
                .map(String::from);
            DelegationTreeNode {
                agent_id: agent_name.clone(),
                agent_name,
                status,
                started_at,
                children: Vec::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::TaskStatus;

    #[test]
    fn test_parse_sidechains_extracts_fields() {
        // GIVEN a JSON array of sidechain rows
        let json = serde_json::json!([
            {
                "sidechain_n": 1,
                "agent_name": "agent-recherche",
                "status": "completed",
                "input_summary": "...",
                "output_summary": "...",
                "started_at": "2026-04-30T10:00:00Z",
                "completed_at": "2026-04-30T10:00:02Z"
            },
            {
                "sidechain_n": 2,
                "agent_name": "agent-synthese",
                "status": "running",
                "started_at": "2026-04-30T10:00:03Z"
            }
        ]);

        // WHEN parsed
        let nodes = parse_sidechains(json);

        // THEN one node per row, with agent_name/status/started_at populated
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].agent_name, "agent-recherche");
        assert_eq!(nodes[0].status, "completed");
        assert_eq!(nodes[0].started_at.as_deref(), Some("2026-04-30T10:00:00Z"));
        assert!(nodes[0].children.is_empty());
        assert_eq!(nodes[1].status, "running");
    }

    #[test]
    fn test_parse_sidechains_handles_non_array() {
        // GIVEN a non-array JSON value
        let json = serde_json::json!({ "error": "boom" });
        // WHEN parsed
        let nodes = parse_sidechains(json);
        // THEN no children produced
        assert!(nodes.is_empty());
    }

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
