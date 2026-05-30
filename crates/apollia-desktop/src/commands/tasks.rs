//! Tauri IPC commands for task management.
//!
//! `list_tasks` and `submit_task` delegate to the runtime handles.
//! `get_task_timeline` calls the internal REST API `GET /api/v1/tasks/{id}/timeline`
//! to avoid duplicating the aggregation logic.

use apollia_core::{AIPInput, AIPPart, TaskStatus, TextPart};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_get_json;

/// Optional filter for the task list.
#[derive(Debug, Deserialize)]
pub struct TaskFilter {
    /// Filter by status (submitted, working, completed, failed, etc.).
    pub status: Option<String>,
    /// Filter by agent identifier.
    pub agent_id: Option<String>,
}

/// Summary of a task for display in the UI.
#[derive(Debug, Serialize)]
pub struct TaskSummary {
    /// Unique task identifier.
    pub id: String,
    /// Identifier of the assigned agent.
    pub agent_id: String,
    /// Name of the assigned agent.
    pub agent_name: String,
    /// Current status.
    pub status: String,
    /// Preview of the input text (truncated).
    pub input_preview: String,
    /// Full output text (possibly truncated by observability).
    pub output_text: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// ISO8601 creation date.
    pub created_at: String,
}

/// Converts a `TaskStatus` into a snake_case string for the frontend.
fn status_to_string(status: &TaskStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{status:?}"))
}

/// Maximum number of historical tasks to load from SQLite.
const PERSISTED_TASK_LIMIT: usize = 50;

/// Lists all tasks with optional filtering by status or agent.
///
/// Merges two sources:
/// 1. **Runtime** (in-memory TaskRouter): active tasks for the current session.
/// 2. **SQLite** (TaskRepository): historical tasks persisted across restarts.
///
/// Runtime tasks take priority: if a task exists in both sources, only the
/// runtime version is kept.
#[tauri::command]
pub async fn list_tasks(
    state: State<'_, RuntimeHandle>,
    filter: Option<TaskFilter>,
) -> Result<Vec<TaskSummary>, String> {
    let all = state
        .router_handle
        .all_tasks()
        .await
        .map_err(|e| e.to_string())?;

    // Resolve the agent name once to filter the persisted tasks (which lack the
    // runtime UUID but carry the agent name).
    let filter_agent_name = resolve_filter_agent_name(&state, filter.as_ref()).await;

    let mut summaries = Vec::with_capacity(all.len());
    let mut seen_task_ids = std::collections::HashSet::new();

    // 1. Runtime tasks (current session, in-memory).
    append_runtime_tasks(
        &state,
        &all,
        filter.as_ref(),
        &mut summaries,
        &mut seen_task_ids,
    )
    .await;

    // 2. Persisted tasks from SQLite (historical, survived restart).
    append_persisted_tasks(
        &state,
        filter.as_ref(),
        filter_agent_name.as_deref(),
        &mut summaries,
        &mut seen_task_ids,
    )
    .await;

    Ok(summaries)
}

/// Resolve the filter's agent UUID to the agent *name* used by persisted rows.
///
/// Returns `None` when no agent filter is set or the agent cannot be resolved.
async fn resolve_filter_agent_name(
    state: &RuntimeHandle,
    filter: Option<&TaskFilter>,
) -> Option<String> {
    let agent_id = filter.and_then(|f| f.agent_id.as_ref())?;
    state
        .registry_handle
        .get_agent(agent_id.as_str())
        .await
        .ok()
        .flatten()
        .map(|e| e.manifest.name.clone())
}

/// Append the in-memory runtime tasks (current session) that pass the filter.
async fn append_runtime_tasks(
    state: &RuntimeHandle,
    all: &[(apollia_core::TaskId, apollia_core::AgentId, TaskStatus)],
    filter: Option<&TaskFilter>,
    summaries: &mut Vec<TaskSummary>,
    seen_task_ids: &mut std::collections::HashSet<String>,
) {
    for (task_id, agent_id, status) in all {
        let status_str = status_to_string(status);

        if !runtime_task_matches(filter, agent_id.as_str(), &status_str) {
            continue;
        }

        let agent_name = state
            .registry_handle
            .get_agent(agent_id.as_str())
            .await
            .ok()
            .flatten()
            .map(|e| e.manifest.name.clone())
            .unwrap_or_default();

        let (input_preview, output_text, duration_ms, created_at) =
            fetch_runtime_task_fields(state, task_id.as_str()).await;

        seen_task_ids.insert(task_id.to_string());

        summaries.push(TaskSummary {
            id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            agent_name,
            status: status_str,
            input_preview,
            output_text,
            duration_ms,
            created_at,
        });
    }
}

/// Append the historical SQLite-persisted tasks that pass the filter and were
/// not already produced by the runtime source.
async fn append_persisted_tasks(
    state: &RuntimeHandle,
    filter: Option<&TaskFilter>,
    filter_agent_name: Option<&str>,
    summaries: &mut Vec<TaskSummary>,
    seen_task_ids: &mut std::collections::HashSet<String>,
) {
    let Some(repo) = state.task_repository.as_ref() else {
        return;
    };
    let Ok(persisted) = repo.list_recent_tasks(PERSISTED_TASK_LIMIT).await else {
        return;
    };
    for row in persisted {
        if seen_task_ids.contains(&row.task_id) {
            continue;
        }
        if !persisted_task_matches(filter, filter_agent_name, &row) {
            continue;
        }

        seen_task_ids.insert(row.task_id.clone());

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
}

/// Whether a runtime task passes the optional status/agent filter.
fn runtime_task_matches(filter: Option<&TaskFilter>, agent_id: &str, status_str: &str) -> bool {
    let Some(f) = filter else { return true };
    if let Some(ref filter_status) = f.status {
        if status_str != filter_status {
            return false;
        }
    }
    if let Some(ref filter_agent) = f.agent_id {
        if agent_id != filter_agent.as_str() {
            return false;
        }
    }
    true
}

/// Whether a persisted task row passes the optional status/agent filter.
///
/// `filter_agent_name` is the resolved agent *name* (persisted rows carry the
/// name, not the runtime UUID).
fn persisted_task_matches(
    filter: Option<&TaskFilter>,
    filter_agent_name: Option<&str>,
    row: &apollia_tools::PersistedTaskSummary,
) -> bool {
    let Some(f) = filter else { return true };
    if let Some(ref filter_status) = f.status {
        if &row.status != filter_status {
            return false;
        }
    }
    if let Some(name) = filter_agent_name {
        if row.agent_name != name {
            return false;
        }
    }
    true
}

/// Fetch the input preview / output / duration / created_at for a runtime task
/// from the persisted observability store, defaulting to empties when absent.
async fn fetch_runtime_task_fields(
    state: &RuntimeHandle,
    task_id: &str,
) -> (String, Option<String>, Option<u64>, String) {
    let Some(repo) = state.task_repository.as_ref() else {
        return (String::new(), None, None, String::new());
    };
    match repo.get_task_detail(task_id).await {
        Ok(Some(detail)) => {
            let preview = detail
                .input_text
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(120)
                .collect::<String>();
            let dur = detail.duration_ms.map(|ms| ms as u64);
            (preview, detail.output_text, dur, detail.created_at)
        }
        _ => (String::new(), None, None, String::new()),
    }
}

/// Submits a task to an agent and returns the generated `TaskId`.
///
/// Builds an `AIPInput` from the raw text provided by the frontend and submits
/// it via `TaskRouterHandle::submit()`.
///
/// `allowed_tools` and `disallowed_tools` are accepted for compatibility with
/// the CLI (`--allowed-tools` / `--disallowed-tools`) but are not yet propagated
/// to the runtime; they will be passed through the AIP metadata later.
#[tauri::command]
pub async fn submit_task(
    state: State<'_, RuntimeHandle>,
    agent_id: String,
    input: String,
    allowed_tools: Option<Vec<String>>,
    disallowed_tools: Option<Vec<String>>,
) -> Result<String, String> {
    // Tool filtering is not yet implemented in the AIPInput type.
    // Log the intent so the UI can see it was received.
    if let Some(ref tools) = allowed_tools {
        tracing::debug!(allowed_tools = ?tools, "submit_task: allowed_tools received (not yet enforced)");
    }
    if let Some(ref tools) = disallowed_tools {
        tracing::debug!(disallowed_tools = ?tools, "submit_task: disallowed_tools received (not yet enforced)");
    }

    let aip_input = AIPInput {
        parts: vec![AIPPart::Text(TextPart { text: input })],
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
/// Calls `DELETE /api/v1/tasks/{id}` via the internal REST API.
/// Returns `true` if the task was cancelled, `false` if already finished or not found.
#[tauri::command]
pub async fn cancel_task(state: State<'_, RuntimeHandle>, task_id: String) -> Result<bool, String> {
    let path = format!("/api/v1/tasks/{task_id}");
    match super::http_delete_json(state.api_port, &path).await {
        Ok(_) => Ok(true),
        Err(e) if e.contains("404") => Ok(false),
        Err(e) => Err(e),
    }
}

/// Fetches a task's timeline via the internal REST API.
///
/// Calls `GET /api/v1/tasks/{id}/timeline`, which aggregates events from 5
/// SQLite sources (transitions, plans, LLM calls, tool calls, HITL). The result
/// is returned as-is to the frontend.
#[tauri::command]
pub async fn get_task_timeline(
    state: State<'_, RuntimeHandle>,
    task_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let path = format!("/api/v1/tasks/{task_id}/timeline");
    let json = http_get_json(state.api_port, &path).await?;

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
