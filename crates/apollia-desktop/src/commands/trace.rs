//! Tauri IPC command for the event-sourced execution trace.
//!
//! Delegates to the REST endpoint `/api/v1/tasks/:id/trace` exposed by
//! `apollia-runtime`; no direct read of `runtime_events.db` here, to keep a
//! single entry point (future ACL, deprecation header).
//!
//! The frontend calls `invoke("get_task_trace", { taskId, since, limit })` and
//! receives a `TraceResponse` with the paginated list of events.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_get_json;

/// TS-friendly representation of a `RuntimeEventRecord` (camelCase).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEventDto {
    /// UUID v7, lexicographically orderable.
    pub event_id: String,
    /// Task.
    pub task_id: String,
    /// Emitting agent.
    pub agent_id: String,
    /// Parent link (tool_call_completed → started, etc.).
    pub parent_event_id: Option<String>,
    /// ID shared across an A2A chain.
    pub correlation_id: Option<String>,
    /// ReAct turn (NULL outside the loop).
    pub step_num: Option<i64>,
    /// Discriminant, e.g. `tool_call_started`, `thought`, `agent_log`.
    pub kind: String,
    /// Payload typed by kind, kept as `Value` to pass raw to the front.
    pub payload: serde_json::Value,
    /// ISO 8601 RFC 3339 milliseconds.
    pub ts: String,
}

/// Paginated response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceResponse {
    /// Task concerned (echo).
    pub task_id: String,
    /// Events ordered chronologically (UUIDv7 ASC).
    pub events: Vec<RuntimeEventDto>,
    /// Cursor to pass as `since` on the next call.
    pub next_cursor: Option<String>,
}

/// Call parameters.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTraceParams {
    /// Target task.
    pub task_id: String,
    /// Pagination cursor (event_id UUIDv7).
    #[serde(default)]
    pub since: Option<String>,
    /// Maximum number of events to return (default 500, max 5000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Fetches a task's execution trace.
///
/// Delegates to `GET /api/v1/tasks/:task_id/trace?since=...&limit=...` exposed
/// by the runtime. Parses the JSON response into `TraceResponse` (the server's
/// raw `payload_json` is deserialized into `serde_json::Value` here to give the
/// front an object rather than a string to re-parse).
#[tauri::command]
pub async fn get_task_trace(
    state: State<'_, RuntimeHandle>,
    params: GetTraceParams,
) -> Result<TraceResponse, String> {
    let port = state.api_port;

    let mut path = format!("/api/v1/tasks/{}/trace", params.task_id);
    let mut query: Vec<String> = Vec::new();
    if let Some(since) = params.since.as_deref() {
        query.push(format!("since={}", urlencoding::encode(since)));
    }
    if let Some(limit) = params.limit {
        query.push(format!("limit={limit}"));
    }
    if !query.is_empty() {
        path.push('?');
        path.push_str(&query.join("&"));
    }

    let raw = http_get_json(port, &path).await?;

    // The server returns `payload_json: String`; we parse it into a Value to
    // give the TS side a usable object. If the parse fails (it should not), we
    // keep the string as a Value::String.
    let task_id = raw
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&params.task_id)
        .to_string();

    let next_cursor = raw
        .get("next_cursor")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let events_raw = raw
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut events = Vec::with_capacity(events_raw.len());
    for ev in events_raw {
        let payload_str = ev
            .get("payload_json")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");
        let payload: serde_json::Value = serde_json::from_str(payload_str)
            .unwrap_or_else(|_| serde_json::Value::String(payload_str.to_string()));

        events.push(RuntimeEventDto {
            event_id: ev
                .get("event_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            task_id: ev
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            agent_id: ev
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            parent_event_id: ev
                .get("parent_event_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            correlation_id: ev
                .get("correlation_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            step_num: ev.get("step_num").and_then(|v| v.as_i64()),
            kind: ev
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            payload,
            ts: ev
                .get("ts")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        });
    }

    Ok(TraceResponse {
        task_id,
        events,
        next_cursor,
    })
}
