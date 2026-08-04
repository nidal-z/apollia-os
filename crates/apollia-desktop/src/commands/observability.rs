//! Tauri IPC commands for the Observability view.
//!
//! Three commands covering the view's three tabs:
//! - `get_global_timeline`: runtime events aggregated across tasks
//! - `get_tool_audit_trail`: tool invocations with details
//! - `get_llm_daily_costs`: LLM costs broken down by day and backend

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{http_get_json, http_post_json};

// ---------------------------------------------------------------------------
// Global Timeline
// ---------------------------------------------------------------------------

/// Global timeline event for display.
#[derive(Debug, Serialize)]
pub struct GlobalTimelineEvent {
    /// Event type: task, tool, llm, trigger, hitl.
    pub event_type: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Event summary.
    pub summary: String,
    /// Expandable JSON details.
    pub detail: serde_json::Value,
}

/// Parameters for `get_global_timeline`.
#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    /// Time window in minutes (30, 60, 360, 720, 1440).
    pub window_minutes: u32,
}

/// Fetches an exhaustive global timeline by scanning the SQLite databases by
/// time window, **regardless of the originating task_id**.
///
/// The previous task-centric scan missed any operation not attached to a
/// persisted task: chat, triggers, runtime events outside a task. This version
/// queries each source directly by timestamp, surfacing 100% of the activity
/// visible in the window.
///
/// Scanned sources:
/// - `audit.db tool_invocations` → tool
/// - `llm_calls.db llm_calls` → llm
/// - `hitl.db tasks` (transitions_json) → task
/// - `hitl.db task_approvals` → hitl
/// - `chat.db chat_sessions` + `chat_approval_log` → task / hitl
/// - `triggers.db trigger_history` → task (trigger fire)
/// - `runtime_events.db runtime_events` (kinds: thought, agent_log, action_parse_error) → memory / task / error
#[tauri::command]
pub async fn get_global_timeline(
    state: State<'_, RuntimeHandle>,
    params: TimelineParams,
) -> Result<Vec<GlobalTimelineEvent>, String> {
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(i64::from(params.window_minutes));
    // ISO 8601 (UTC, no fractional secs), compatible with the canonical format
    // stored across all our SQLite tables and lexicographically comparable.
    let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Resolve data_dir the same way the desktop bootstrapper does (main.rs).
    let data_dir = {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
        std::path::PathBuf::from(home).join(".apollia")
    };

    // Build agent_id/name → human-readable label map once. Used to humanise the
    // [prefix] of each event summary (e.g. `[veille-ia-agent]` instead of UUID).
    // We map both UUID id → name AND name → name so source rows storing either
    // form resolve identically.
    let mut agent_labels: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Ok(agents) = state.registry_handle.list_agents().await {
        for entry in agents {
            let name = entry.manifest.name.clone();
            agent_labels.insert(entry.id.to_string(), name.clone());
            agent_labels.insert(name.clone(), name);
        }
    }

    // SQLite is sync, so push the entire scan onto a blocking thread.
    let result = tokio::task::spawn_blocking(move || {
        let mut events = Vec::<GlobalTimelineEvent>::new();
        scan_audit_db(&data_dir, &cutoff_str, &agent_labels, &mut events);
        scan_llm_calls_db(&data_dir, &cutoff_str, &agent_labels, &mut events);
        scan_hitl_tasks(&data_dir, &cutoff_str, &mut events);
        scan_hitl_approvals(&data_dir, &cutoff_str, &mut events);
        scan_chat_sessions(&data_dir, &cutoff_str, &mut events);
        scan_chat_approvals(&data_dir, &cutoff_str, &mut events);
        scan_trigger_history(&data_dir, &cutoff_str, &mut events);
        scan_runtime_events(&data_dir, &cutoff_str, &agent_labels, &mut events);
        events
    })
    .await
    .map_err(|e| format!("join error: {e}"))?;

    // Sort DESC by timestamp (most recent first).
    let mut sorted = result;
    sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(sorted)
}

/// Resolves a human-readable label for a raw identifier (agent_id, name, ...).
fn label_for(agent_key: &str, labels: &std::collections::HashMap<String, String>) -> String {
    labels
        .get(agent_key)
        .cloned()
        .unwrap_or_else(|| agent_key.to_string())
}

/// Cleanly truncates a string for summaries (max `max` chars plus an ellipsis).
fn trim_for_summary(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn scan_audit_db(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    labels: &std::collections::HashMap<String, String>,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join("audit.db");
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return;
    };
    let mut stmt = match conn.prepare(
        "SELECT id, agent_id, task_id, tool_name, started_at, duration_ms, exit_code, success, error_code
         FROM tool_invocations
         WHERE started_at >= ?1
         ORDER BY started_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, Option<String>>(8)?,
        ))
    });
    let Ok(iter) = rows else { return };
    for r in iter.flatten() {
        let (id, agent_id, task_id, tool_name, ts, dur, exit_code, success, error_code) = r;
        let agent_label = label_for(&agent_id, labels);
        let dur_label = dur.map(|ms| format!(" ({ms}ms)")).unwrap_or_default();
        let success_marker = if success == 0 { " ⚠" } else { "" };
        let summary = format!("[{agent_label}] Tool: {tool_name}{dur_label}{success_marker}");
        let event_type = if success == 0 { "error" } else { "tool" };
        events.push(GlobalTimelineEvent {
            event_type: event_type.to_string(),
            timestamp: ts,
            summary,
            detail: serde_json::json!({
                "source": "audit.db",
                "id": id,
                "task_id": task_id,
                "agent_id": agent_id,
                "tool_name": tool_name,
                "duration_ms": dur,
                "exit_code": exit_code,
                "success": success != 0,
                "error_code": error_code,
            }),
        });
    }
}

fn scan_llm_calls_db(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    labels: &std::collections::HashMap<String, String>,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join("llm_calls.db");
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return;
    };
    let mut stmt = match conn.prepare(
        "SELECT id, task_id, backend, model, prompt_tokens, completion_tokens, cost_usd, latency_ms, created_at
         FROM llm_calls
         WHERE created_at >= ?1
         ORDER BY created_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<f64>>(6)?,
            row.get::<_, Option<i64>>(7)?,
            row.get::<_, String>(8)?,
        ))
    });
    let Ok(iter) = rows else { return };
    for r in iter.flatten() {
        let (
            id,
            task_id,
            backend,
            model,
            prompt_tokens,
            completion_tokens,
            cost_usd,
            latency_ms,
            ts,
        ) = r;
        let cost_label = cost_usd
            .map(|c| {
                if c >= 0.01 {
                    format!(" · ${c:.2}")
                } else {
                    format!(" · ${c:.4}")
                }
            })
            .unwrap_or_default();
        let label = task_id
            .as_deref()
            .and_then(|tid| labels.get(tid).cloned())
            .unwrap_or_else(|| backend.clone());
        let summary = format!("[{label}] LLM: {model}{cost_label}");
        events.push(GlobalTimelineEvent {
            event_type: "llm".to_string(),
            timestamp: ts,
            summary,
            detail: serde_json::json!({
                "source": "llm_calls.db",
                "id": id,
                "task_id": task_id,
                "backend": backend,
                "model": model,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "cost_usd": cost_usd,
                "latency_ms": latency_ms,
            }),
        });
    }
}

fn scan_hitl_tasks(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join("hitl.db");
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return;
    };
    let mut stmt = match conn.prepare(
        "SELECT task_id, agent_name, transitions_json, created_at, updated_at, duration_ms
         FROM tasks
         WHERE updated_at >= ?1 OR created_at >= ?1
         ORDER BY created_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    });
    let Ok(iter) = rows else { return };
    for r in iter.flatten() {
        let (task_id, agent_name, transitions, _created, _updated, duration_ms) = r;

        // Parse transitions_json: each entry { status, ts } emits one task event.
        let Some(json) = transitions else { continue };
        let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&json) else {
            continue;
        };
        for tr in arr {
            push_transition_event(&tr, cutoff_str, &task_id, &agent_name, duration_ms, events);
        }
    }
}

/// Emit one task-timeline event for a single `{ status, ts }` transition entry.
///
/// Entries older than `cutoff_str` are skipped.
// Timeline builder helper: the row plus cutoff, ids, duration and the events
// accumulator exceed 5 by design; a struct would not clarify this internal call.
#[allow(clippy::too_many_arguments)]
fn push_transition_event(
    tr: &serde_json::Value,
    cutoff_str: &str,
    task_id: &str,
    agent_name: &str,
    duration_ms: Option<i64>,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let status = tr
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let ts = tr.get("ts").and_then(|v| v.as_str()).unwrap_or("");
    if ts < cutoff_str {
        return;
    }
    let dur = if status == "completed" {
        duration_ms
            .map(|ms| format!(" · {ms}ms"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    events.push(GlobalTimelineEvent {
        event_type: "task".to_string(),
        timestamp: ts.to_string(),
        summary: format!("[{agent_name}] Tâche → {status}{dur}"),
        detail: serde_json::json!({
            "source": "hitl.db/tasks",
            "task_id": task_id,
            "agent_name": agent_name,
            "status": status,
            "duration_ms": duration_ms,
        }),
    });
}

fn scan_hitl_approvals(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join("hitl.db");
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return;
    };
    // The schema typically includes suspended_at + resolved_at.
    let mut stmt = match conn.prepare(
        "SELECT task_id, prompt, suspended_at, resolved_at, decision, reason
         FROM task_approvals
         WHERE suspended_at >= ?1 OR resolved_at >= ?1
         ORDER BY suspended_at DESC",
    ) {
        Ok(s) => s,
        // Table may not exist on older installs, so silently skip.
        Err(_) => return,
    };
    let rows = stmt.query_map([cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    });
    let Ok(iter) = rows else { return };
    for r in iter.flatten() {
        let (task_id, prompt, suspended_at, resolved_at, decision, reason) = r;
        if suspended_at.as_str() >= cutoff_str {
            let preview = trim_for_summary(prompt.as_deref().unwrap_or(""), 80);
            events.push(GlobalTimelineEvent {
                event_type: "hitl".to_string(),
                timestamp: suspended_at.clone(),
                summary: format!("HITL pending: {preview}"),
                detail: serde_json::json!({
                    "source": "hitl.db/task_approvals",
                    "task_id": task_id,
                    "prompt": prompt,
                }),
            });
        }
        if let Some(ts) = resolved_at {
            if ts.as_str() >= cutoff_str {
                let verdict = decision.as_deref().unwrap_or("resolved");
                events.push(GlobalTimelineEvent {
                    event_type: "hitl".to_string(),
                    timestamp: ts,
                    summary: format!("HITL → {verdict}"),
                    detail: serde_json::json!({
                        "source": "hitl.db/task_approvals",
                        "task_id": task_id,
                        "decision": decision,
                        "reason": reason,
                    }),
                });
            }
        }
    }
}

fn scan_chat_sessions(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join("chat.db");
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return;
    };
    let mut stmt = match conn.prepare(
        "SELECT id, mode, agent_name, status, created_at, closed_at, title
         FROM chat_sessions
         WHERE created_at >= ?1 OR closed_at >= ?1
         ORDER BY created_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    });
    let Ok(iter) = rows else { return };
    for r in iter.flatten() {
        let (id, mode, agent_name, status, created_at, closed_at, title) = r;
        let label = agent_name.clone().unwrap_or_else(|| format!("chat-{mode}"));
        let title_label = title
            .as_deref()
            .map(|t| trim_for_summary(t, 60))
            .unwrap_or_else(|| "(sans titre)".to_string());
        if created_at.as_str() >= cutoff_str {
            events.push(GlobalTimelineEvent {
                event_type: "task".to_string(),
                timestamp: created_at.clone(),
                summary: format!("[{label}] Chat ouvert · {title_label}"),
                detail: serde_json::json!({
                    "source": "chat.db/chat_sessions",
                    "session_id": id,
                    "mode": mode,
                    "agent_name": agent_name,
                    "status": status,
                    "title": title,
                }),
            });
        }
        if let Some(ts) = closed_at {
            if ts.as_str() >= cutoff_str {
                events.push(GlobalTimelineEvent {
                    event_type: "task".to_string(),
                    timestamp: ts,
                    summary: format!("[{label}] Chat clos · {title_label}"),
                    detail: serde_json::json!({
                        "source": "chat.db/chat_sessions",
                        "session_id": id,
                        "mode": mode,
                        "agent_name": agent_name,
                        "status": status,
                    }),
                });
            }
        }
    }
}

fn scan_chat_approvals(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join("chat.db");
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return;
    };
    let mut stmt = match conn.prepare(
        "SELECT session_id, message_id, tool_name, decision, resolved_at, reason
         FROM chat_approval_log
         WHERE resolved_at >= ?1
         ORDER BY resolved_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    });
    let Ok(iter) = rows else { return };
    for r in iter.flatten() {
        let (session_id, message_id, tool_name, decision, ts, reason) = r;
        events.push(GlobalTimelineEvent {
            event_type: "hitl".to_string(),
            timestamp: ts,
            summary: format!("Chat HITL · {tool_name} → {decision}"),
            detail: serde_json::json!({
                "source": "chat.db/chat_approval_log",
                "session_id": session_id,
                "message_id": message_id,
                "tool_name": tool_name,
                "decision": decision,
                "reason": reason,
            }),
        });
    }
}

fn scan_trigger_history(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join("triggers.db");
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return;
    };
    let mut stmt = match conn.prepare(
        "SELECT id, trigger_id, agent_name, fired_at, task_id, status, reason
         FROM trigger_history
         WHERE fired_at >= ?1
         ORDER BY fired_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    });
    let Ok(iter) = rows else { return };
    for r in iter.flatten() {
        let (id, trigger_id, agent_name, ts, task_id, status, reason) = r;
        let event_type = if status == "error" { "error" } else { "task" };
        let suffix = trigger_status_label(&status);
        events.push(GlobalTimelineEvent {
            event_type: event_type.to_string(),
            timestamp: ts,
            summary: format!("[{agent_name}] Trigger {suffix}"),
            detail: serde_json::json!({
                "source": "triggers.db/trigger_history",
                "id": id,
                "trigger_id": trigger_id,
                "agent_name": agent_name,
                "task_id": task_id,
                "status": status,
                "reason": reason,
            }),
        });
    }
}

/// Human label for a `trigger_history.status` value, as shown in the timeline.
///
/// Timeline summaries are built here, not translated in the frontend, so they
/// follow the codebase language rather than the interface language. An unknown
/// status is passed through untouched: it is a raw value from the database and
/// inventing a label for it would hide a schema drift.
fn trigger_status_label(status: &str) -> &str {
    match status {
        "fired" => "fired",
        "skipped" => "skipped",
        "error" => "in error",
        other => other,
    }
}

/// Timeline event type and summary for a `runtime_events.kind`.
///
/// Split out of the scan so the mapping can be asserted without a database.
/// An unknown kind falls back to the task lane with the raw kind in the
/// summary, which surfaces a new event kind instead of swallowing it.
fn runtime_event_label(kind: &str, agent_label: &str) -> (&'static str, String) {
    match kind {
        "thought" => ("task", format!("[{agent_label}] Reasoning")),
        "agent_log" => ("task", format!("[{agent_label}] Log")),
        "action_parse_error" => ("error", format!("[{agent_label}] Parse error")),
        "tool_call_denied" => ("hitl", format!("[{agent_label}] Tool denied")),
        "memory_write" => ("memory", format!("[{agent_label}] Memory written")),
        "memory_read" => ("memory", format!("[{agent_label}] Memory read")),
        "a2a_delegate" => ("a2a", format!("[{agent_label}] A2A delegation")),
        "a2a_response" => ("a2a", format!("[{agent_label}] A2A response")),
        other => ("task", format!("[{agent_label}] {other}")),
    }
}

fn scan_runtime_events(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    labels: &std::collections::HashMap<String, String>,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join("runtime_events.db");
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return;
    };
    // Only surface kinds that aren't already covered by audit/llm/hitl scans
    // (else we'd double-count tool/LLM events).
    let mut stmt = match conn.prepare(
        "SELECT event_id, task_id, agent_id, kind, payload_json, ts
         FROM runtime_events
         WHERE ts >= ?1
           AND kind IN ('thought', 'agent_log', 'action_parse_error', 'tool_call_denied', 'memory_write', 'memory_read', 'a2a_delegate', 'a2a_response')
         ORDER BY ts DESC",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    });
    let Ok(iter) = rows else { return };
    for r in iter.flatten() {
        let (event_id, task_id, agent_id, kind, payload_json, ts) = r;
        let agent_label = label_for(&agent_id, labels);
        let (event_type, summary) = runtime_event_label(&kind, &agent_label);
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json).unwrap_or(serde_json::json!({}));
        events.push(GlobalTimelineEvent {
            event_type: event_type.to_string(),
            timestamp: ts,
            summary,
            detail: serde_json::json!({
                "source": "runtime_events.db",
                "event_id": event_id,
                "task_id": task_id,
                "agent_id": agent_id,
                "kind": kind,
                "payload": payload,
            }),
        });
    }
}

/// Kept for potential backward compatibility; no longer used.
#[allow(
    dead_code,
    reason = "kept as rollback target for the timeline event normalisation pipeline, exercised by the inline tests below"
)]
fn classify_event_type(raw: &str) -> String {
    match raw {
        "task_transition" | "task_completed" => "task".to_string(),
        "tool_call" => "tool".to_string(),
        "llm_call" => "llm".to_string(),
        "step_started" | "step_completed" => "task".to_string(),
        "hitl_suspended" | "hitl_resolved" => "hitl".to_string(),
        other => other.to_string(),
    }
}

/// Builds a human-readable summary from a raw timeline event.
#[allow(
    dead_code,
    reason = "kept as rollback target for the timeline event normalisation pipeline, exercised by the inline tests below"
)]
fn build_event_summary(event_type: &str, event: &serde_json::Value, agent_id: &str) -> String {
    match event_type {
        "task_transition" => {
            let status = event
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("[{agent_id}] Task → {status}")
        }
        "task_completed" => {
            let dur = event
                .get("duration_ms")
                .and_then(|v| v.as_i64())
                .map(|ms| format!(" in {ms}ms"))
                .unwrap_or_default();
            format!("[{agent_id}] Task completed{dur}")
        }
        "tool_call" => {
            let tool = event
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let dur = event
                .get("duration_ms")
                .and_then(|v| v.as_i64())
                .map(|ms| format!(" ({ms}ms)"))
                .unwrap_or_default();
            format!("[{agent_id}] Tool: {tool}{dur}")
        }
        "llm_call" => {
            let model = event
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let cost = event
                .get("cost_usd")
                .and_then(|v| v.as_f64())
                .map(|c| format!(" ${c:.4}"))
                .unwrap_or_default();
            format!("[{agent_id}] LLM: {model}{cost}")
        }
        "step_started" => {
            let step_id = event.get("step_id").and_then(|v| v.as_str()).unwrap_or("?");
            let tool = event
                .get("tool")
                .and_then(|v| v.as_str())
                .map(|t| format!(" - {t}"))
                .unwrap_or_default();
            format!("[{agent_id}] Step {step_id} started{tool}")
        }
        "step_completed" => {
            let step_id = event.get("step_id").and_then(|v| v.as_str()).unwrap_or("?");
            let success = event
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let icon = if success { "completed" } else { "failed" };
            format!("[{agent_id}] Step {step_id} {icon}")
        }
        "hitl_suspended" => {
            let prompt = event.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
            let preview = if prompt.len() > 60 {
                let cut = apollia_core::floor_char_boundary(prompt, 60);
                format!("{}...", &prompt[..cut])
            } else {
                prompt.to_string()
            };
            format!("[{agent_id}] HITL: {preview}")
        }
        "hitl_resolved" => {
            let approved = event
                .get("approved")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let verdict = if approved { "Approved" } else { "Rejected" };
            format!("[{agent_id}] HITL: {verdict}")
        }
        _ => format!("[{agent_id}] {event_type}"),
    }
}

// ---------------------------------------------------------------------------
// Audit Trail
// ---------------------------------------------------------------------------

/// Audit trail entry for display in the UI.
#[derive(Debug, Serialize)]
pub struct AuditTrailEntry {
    /// Unique invocation identifier.
    pub id: String,
    /// Name of the invoked tool.
    pub tool_name: String,
    /// Agent UUID (used for filtering).
    pub agent_id: String,
    /// Human-readable agent name, resolved from the registry (e.g. "standup-scribe").
    /// Falls back to agent_id if the agent is no longer registered.
    pub agent_name: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Execution duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Process exit code.
    pub exit_code: Option<i32>,
    /// Full JSON arguments of the invocation.
    pub args_json: Option<String>,
    /// Tool standard output.
    pub stdout: Option<String>,
    /// Tool error output.
    pub stderr: Option<String>,
}

/// Fetches the latest tool invocations via the audit REST API.
///
/// Delegates to `GET /api/v1/audit?limit=N` and returns the parsed entries for
/// display in the AuditTrail table.
#[tauri::command]
pub async fn get_tool_audit_trail(
    state: State<'_, RuntimeHandle>,
    limit: Option<u32>,
) -> Result<Vec<AuditTrailEntry>, String> {
    let l = limit.unwrap_or(50);
    let path = format!("/api/v1/audit?limit={l}");
    let json = http_get_json(state.api_port, &path).await?;

    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Resolve agent names asynchronously from the registry so the UI shows
    // "standup-scribe" instead of a raw UUID. Falls back to the UUID when the
    // agent is no longer registered (e.g. stopped between runs).
    let mut entries: Vec<AuditTrailEntry> = Vec::with_capacity(events.len());
    for e in events {
        let agent_id = e
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let agent_name = state
            .registry_handle
            .get_agent(&agent_id)
            .await
            .ok()
            .flatten()
            .map(|entry| entry.manifest.name.clone())
            .unwrap_or_else(|| agent_id.clone()); // agent_id is already String here

        entries.push(AuditTrailEntry {
            id: e
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            tool_name: e
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            agent_id,
            agent_name,
            timestamp: e
                .get("started_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            duration_ms: e.get("duration_ms").and_then(|v| v.as_u64()),
            exit_code: e
                .get("exit_code")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32),
            args_json: e
                .get("args_json")
                .and_then(|v| v.as_str())
                .map(String::from),
            stdout: e.get("stdout").and_then(|v| v.as_str()).map(String::from),
            stderr: e.get("stderr").and_then(|v| v.as_str()).map(String::from),
        });
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// LLM Daily Costs
// ---------------------------------------------------------------------------

/// Daily per-backend cost entry for the SVG chart.
#[derive(Debug, Serialize)]
pub struct LlmDailyCostEntry {
    /// Date in `YYYY-MM-DD` format.
    pub date: String,
    /// Backend name.
    pub backend: String,
    /// Estimated total cost in USD for that day.
    pub cost_usd: f64,
}

/// Daily LLM costs response.
#[derive(Debug, Serialize)]
pub struct LlmDailyCostsResponse {
    /// Entries per day and backend.
    pub entries: Vec<LlmDailyCostEntry>,
    /// Number of requested days.
    pub days: u32,
}

/// Fetches LLM costs broken down by day and backend.
///
/// Delegates to `GET /api/v1/llm/costs/daily?days=N`.
#[tauri::command]
pub async fn get_llm_daily_costs(
    state: State<'_, RuntimeHandle>,
    days: Option<u32>,
) -> Result<LlmDailyCostsResponse, String> {
    let d = days.unwrap_or(7);
    let path = format!("/api/v1/llm/costs/daily?days={d}");
    let json = http_get_json(state.api_port, &path).await;

    match json {
        Ok(resp) => {
            let entries = resp
                .get("entries")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|e| LlmDailyCostEntry {
                    date: e
                        .get("date")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    backend: e
                        .get("backend")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    cost_usd: e.get("cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })
                .collect();

            Ok(LlmDailyCostsResponse { entries, days: d })
        }
        Err(_) => Ok(LlmDailyCostsResponse {
            entries: vec![],
            days: d,
        }),
    }
}

// ---------------------------------------------------------------------------
// Audit Stats
// ---------------------------------------------------------------------------

/// Fetches the aggregated audit trail statistics.
///
/// Calls `GET /api/v1/audit/stats` and returns the raw JSON to avoid
/// duplicating the data structure on the Tauri side.
#[tauri::command]
pub async fn get_audit_stats(state: State<'_, RuntimeHandle>) -> Result<serde_json::Value, String> {
    http_get_json(state.api_port, "/api/v1/audit/stats").await
}

// ---------------------------------------------------------------------------
// Audit Chain Verification
// ---------------------------------------------------------------------------

/// Integrity verdict for a run's hash-chained audit journal.
///
/// Flattens the runtime `VerifyChainReport` into the shape consumed by the
/// desktop verify panel: a boolean verdict, the broken link identifier (the
/// sequence number of the first tampered entry, `None` when intact), and a
/// short human-readable message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerifyResult {
    /// True when the whole chain verifies (no broken link, signatures valid).
    pub ok: bool,
    /// Sequence number of the first broken entry as a string, `None` when `ok`.
    pub broken_at: Option<String>,
    /// Short status detail, surfaced under the verdict in the UI.
    pub message: String,
}

/// Verifies the hash chain of a run's audit journal.
///
/// Delegates to `GET /api/v1/audit/verify/:run_id` on the in-process runtime
/// and maps the report to [`AuditVerifyResult`]. A missing run surfaces as an
/// `Err` (the HTTP layer answers 404), so the UI shows an explicit error rather
/// than a silent "ok" verdict.
#[tauri::command]
pub async fn verify_audit_run(
    state: State<'_, RuntimeHandle>,
    run_id: String,
) -> Result<AuditVerifyResult, String> {
    let path = format!("/api/v1/audit/verify/{run_id}");
    let json = http_get_json(state.api_port, &path).await?;

    let ok = json.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let broken_at = json
        .get("first_broken_link")
        .and_then(|v| v.get("seq"))
        .and_then(serde_json::Value::as_u64)
        .map(|seq| seq.to_string());
    let entries_checked = json
        .get("entries_checked")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let message = if ok {
        format!("{entries_checked} entries verified")
    } else {
        match &broken_at {
            Some(seq) => format!("integrity broken at entry {seq} of {entries_checked} checked"),
            None => format!("integrity check failed after {entries_checked} entries"),
        }
    };

    Ok(AuditVerifyResult {
        ok,
        broken_at,
        message,
    })
}

// ---------------------------------------------------------------------------
// Lifecycle Hooks
// ---------------------------------------------------------------------------

/// Lists the lifecycle hook handlers registered at startup.
///
/// Calls `GET /api/v1/hooks` and returns the raw JSON array of handler
/// summaries (`{ id, type, events, timeout_ms, target }`) for the Builder hooks
/// view. An empty array is a valid configuration, surfaced as a clean state.
#[tauri::command]
pub async fn get_active_hooks(
    state: State<'_, RuntimeHandle>,
) -> Result<serde_json::Value, String> {
    http_get_json(state.api_port, "/api/v1/hooks").await
}

// ---------------------------------------------------------------------------
// Plan Cache Stats
// ---------------------------------------------------------------------------

/// ORIA plan cache statistics.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlanCacheStatsView {
    /// Total number of cached entries.
    pub total_entries: u32,
    /// Total number of cache hits.
    pub cache_hits: u64,
    /// Total number of cache misses.
    pub cache_misses: u64,
    /// Hit rate as a percentage (0.0 to 100.0).
    pub hit_rate_pct: f64,
    /// Timestamp of the oldest entry, or `null`.
    pub oldest_entry_at: Option<String>,
    /// Timestamp of the newest entry, or `null`.
    pub newest_entry_at: Option<String>,
}

/// Result of the cache clearing operation.
#[derive(Debug, Serialize, Deserialize)]
pub struct ClearCacheResult {
    /// Number of removed entries.
    pub cleared_count: u32,
}

/// Returns the ORIA plan cache statistics.
///
/// Returns zeroed counters if the cache is empty or disabled.
/// Delegates to `GET /api/v1/plan-cache/stats`.
#[tauri::command]
pub async fn get_plan_cache_stats(
    state: State<'_, RuntimeHandle>,
) -> Result<PlanCacheStatsView, String> {
    get_plan_cache_stats_inner(state.api_port).await
}

/// Inner logic for `get_plan_cache_stats`, testable without Tauri State.
async fn get_plan_cache_stats_inner(port: u16) -> Result<PlanCacheStatsView, String> {
    let json = http_get_json(port, "/api/v1/plan-cache/stats")
        .await
        .map_err(|e| format!("get_plan_cache_stats: {e}"))?;

    Ok(PlanCacheStatsView {
        total_entries: json
            .get("total_entries")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        cache_hits: json.get("cache_hits").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_misses: 0,
        hit_rate_pct: json
            .get("hit_rate_pct")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        oldest_entry_at: json
            .get("oldest_entry_at")
            .and_then(|v| v.as_str())
            .map(String::from),
        newest_entry_at: json
            .get("newest_entry_at")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Clears the plan cache and returns the number of removed entries.
///
/// Delegates to `POST /api/v1/plan-cache/clear`.
#[tauri::command]
pub async fn clear_plan_cache(state: State<'_, RuntimeHandle>) -> Result<ClearCacheResult, String> {
    clear_plan_cache_inner(state.api_port).await
}

/// Inner logic for `clear_plan_cache`, testable without Tauri State.
async fn clear_plan_cache_inner(port: u16) -> Result<ClearCacheResult, String> {
    let json = http_post_json(port, "/api/v1/plan-cache/clear", &serde_json::json!({}))
        .await
        .map_err(|e| format!("clear_plan_cache: {e}"))?;

    Ok(ClearCacheResult {
        cleared_count: json
            .get("cleared_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    })
}

// ---------------------------------------------------------------------------
// Mailbox Messages
// ---------------------------------------------------------------------------

/// One inter-agent mailbox message, projected for the observability view.
#[derive(Debug, Serialize)]
pub struct MailboxMessageRow {
    /// Stable message identifier (primary key).
    pub message_id: String,
    /// Sending agent name (or `host:<id>` for a host injection).
    pub from_agent: String,
    /// Receiving agent name.
    pub to_agent: String,
    /// Raw JSON payload as stored, rendered verbatim in the UI.
    pub payload: String,
    /// Send timestamp (RFC 3339).
    pub sent_at: String,
    /// Delivery state: `pending` or `in_flight`.
    pub state: String,
}

/// Errors raised while reading the durable mailbox store.
#[derive(Debug, thiserror::Error)]
pub enum MailboxQueryError {
    /// The blocking read task failed to join.
    #[error("mailbox read task failed: {0}")]
    Join(String),
    /// A SQLite operation against `mailbox.db` failed.
    #[error("mailbox read failed")]
    Sqlite(#[from] rusqlite::Error),
}

/// Lists the most recent inter-agent mailbox messages across every recipient.
///
/// Reads the durable `mailbox.db` store written by the runtime mailbox actor,
/// returning rows newest first (descending `seq`). A missing store, or one whose
/// table has not been created yet, yields an empty list rather than an error:
/// the mailbox is simply empty until the first message is sent.
#[tauri::command]
pub async fn list_mailbox_messages(limit: Option<u32>) -> Result<Vec<MailboxMessageRow>, String> {
    // Resolve data_dir the same way the desktop bootstrapper does (main.rs).
    let data_dir = {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
        std::path::PathBuf::from(home).join(".apollia")
    };
    read_mailbox_messages(data_dir, limit.unwrap_or(100))
        .await
        .map_err(|e| e.to_string())
}

/// Inner reader, decoupled from the HOME lookup so it stays unit-testable with a
/// caller-provided data directory.
async fn read_mailbox_messages(
    data_dir: std::path::PathBuf,
    limit: u32,
) -> Result<Vec<MailboxMessageRow>, MailboxQueryError> {
    tokio::task::spawn_blocking(move || query_mailbox_db(&data_dir, limit))
        .await
        .map_err(|e| MailboxQueryError::Join(e.to_string()))?
}

/// Runs the mailbox SELECT on a blocking thread. Returns an empty list when the
/// store file or its `mailbox_messages` table is absent.
fn query_mailbox_db(
    data_dir: &std::path::Path,
    limit: u32,
) -> Result<Vec<MailboxMessageRow>, MailboxQueryError> {
    let path = data_dir.join("mailbox.db");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = rusqlite::Connection::open(&path)?;
    let table_present = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'mailbox_messages'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !table_present {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT message_id, from_agent, to_agent, payload, sent_at, state
         FROM mailbox_messages
         ORDER BY seq DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(MailboxMessageRow {
            message_id: row.get(0)?,
            from_agent: row.get(1)?,
            to_agent: row.get(2)?,
            payload: row.get(3)?,
            sent_at: row.get(4)?,
            state: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_summaries_are_written_in_one_language() {
        // GIVEN every runtime event kind the timeline surfaces, and every
        // trigger status it labels
        //
        // WHEN their summaries are built
        //
        // THEN each reads in the codebase language. These strings are produced
        // here and rendered verbatim, with no translation layer between, so a
        // French label lands untouched in the English interface. The assertion
        // is on the exact wording rather than on a non-ASCII scan, because
        // "Raisonnement" is pure ASCII and a scan would wave it through.
        let expected = [
            ("thought", "task", "[atlas-scribe] Reasoning"),
            ("agent_log", "task", "[atlas-scribe] Log"),
            ("action_parse_error", "error", "[atlas-scribe] Parse error"),
            ("tool_call_denied", "hitl", "[atlas-scribe] Tool denied"),
            ("memory_write", "memory", "[atlas-scribe] Memory written"),
            ("memory_read", "memory", "[atlas-scribe] Memory read"),
            ("a2a_delegate", "a2a", "[atlas-scribe] A2A delegation"),
            ("a2a_response", "a2a", "[atlas-scribe] A2A response"),
        ];
        for (kind, lane, summary) in expected {
            assert_eq!(
                runtime_event_label(kind, "atlas-scribe"),
                (lane, summary.to_string())
            );
        }

        assert_eq!(trigger_status_label("fired"), "fired");
        assert_eq!(trigger_status_label("skipped"), "skipped");
        assert_eq!(trigger_status_label("error"), "in error");
    }

    #[test]
    fn test_runtime_event_label_routes_kinds_to_their_timeline_lane() {
        // GIVEN the kinds the timeline filter chips split on
        // WHEN each is labelled
        // THEN it lands in the lane its chip filters, and an unknown kind falls
        // back to the task lane carrying its raw name rather than disappearing
        assert_eq!(runtime_event_label("thought", "a").0, "task");
        assert_eq!(runtime_event_label("action_parse_error", "a").0, "error");
        assert_eq!(runtime_event_label("memory_write", "a").0, "memory");
        assert_eq!(runtime_event_label("a2a_delegate", "a").0, "a2a");
        assert_eq!(runtime_event_label("tool_call_denied", "a").0, "hitl");

        let (lane, summary) = runtime_event_label("brand_new_kind", "atlas-scribe");
        assert_eq!(lane, "task");
        assert!(summary.contains("brand_new_kind"), "got {summary}");
    }

    #[test]
    fn test_hitl_prompt_preview_cuts_on_char_boundary() {
        // GIVEN a hitl_suspended event whose prompt exceeds 60 bytes with a
        // multibyte code point straddling the 60-byte cut (€ is 3 bytes)
        let prompt = format!("x{}", "€".repeat(30));
        let event = serde_json::json!({ "prompt": prompt });
        // WHEN building the event summary preview
        let summary = build_event_summary("hitl_suspended", &event, "agent-1");
        // THEN it does not panic and produces valid UTF-8
        assert!(std::str::from_utf8(summary.as_bytes()).is_ok());
        assert!(summary.contains("HITL:"));
    }

    #[test]
    fn test_classify_event_type_task() {
        // GIVEN task-related event types
        // WHEN classified
        // THEN they all map to "task"
        assert_eq!(classify_event_type("task_transition"), "task");
        assert_eq!(classify_event_type("task_completed"), "task");
        assert_eq!(classify_event_type("step_started"), "task");
        assert_eq!(classify_event_type("step_completed"), "task");
    }

    #[test]
    fn test_classify_event_type_tool() {
        // GIVEN a tool_call event type
        // WHEN classified
        // THEN it maps to "tool"
        assert_eq!(classify_event_type("tool_call"), "tool");
    }

    #[test]
    fn test_classify_event_type_llm() {
        // GIVEN an llm_call event type
        // WHEN classified
        // THEN it maps to "llm"
        assert_eq!(classify_event_type("llm_call"), "llm");
    }

    #[test]
    fn test_classify_event_type_hitl() {
        // GIVEN HITL event types
        // WHEN classified
        // THEN they map to "hitl"
        assert_eq!(classify_event_type("hitl_suspended"), "hitl");
        assert_eq!(classify_event_type("hitl_resolved"), "hitl");
    }

    #[test]
    fn test_classify_event_type_unknown() {
        // GIVEN an unknown event type
        // WHEN classified
        // THEN it passes through unchanged
        assert_eq!(classify_event_type("custom_event"), "custom_event");
    }

    #[test]
    fn test_build_event_summary_task_transition() {
        // GIVEN a task_transition event JSON
        let event = serde_json::json!({
            "type": "task_transition",
            "status": "working",
            "timestamp": "2026-03-13T10:00:00Z"
        });

        // WHEN building summary
        let summary = build_event_summary("task_transition", &event, "agent-1");

        // THEN it includes the agent and status
        assert_eq!(summary, "[agent-1] Task → working");
    }

    #[test]
    fn test_build_event_summary_tool_call() {
        // GIVEN a tool_call event JSON
        let event = serde_json::json!({
            "type": "tool_call",
            "tool_name": "bash_executor",
            "duration_ms": 150,
            "timestamp": "2026-03-13T10:00:00Z"
        });

        // WHEN building summary
        let summary = build_event_summary("tool_call", &event, "agent-2");

        // THEN it includes tool name and duration
        assert_eq!(summary, "[agent-2] Tool: bash_executor (150ms)");
    }

    #[test]
    fn test_build_event_summary_llm_call() {
        // GIVEN an llm_call event JSON
        let event = serde_json::json!({
            "type": "llm_call",
            "model": "sonnet",
            "cost_usd": 0.0015,
            "timestamp": "2026-03-13T10:00:00Z"
        });

        // WHEN building summary
        let summary = build_event_summary("llm_call", &event, "agent-3");

        // THEN it includes model and cost
        assert_eq!(summary, "[agent-3] LLM: sonnet $0.0015");
    }

    #[test]
    fn test_audit_trail_entry_serializes() {
        // GIVEN an AuditTrailEntry with both agent_id (UUID) and agent_name
        let entry = AuditTrailEntry {
            id: "inv-001".to_string(),
            tool_name: "file_io".to_string(),
            agent_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            agent_name: "standup-scribe".to_string(),
            timestamp: "2026-03-13T10:00:00Z".to_string(),
            duration_ms: Some(42),
            exit_code: Some(0),
            args_json: Some(r#"{"path": "/tmp/test"}"#.to_string()),
            stdout: Some("ok".to_string()),
            stderr: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN all fields are present including agent_name
        assert_eq!(json["tool_name"], "file_io");
        assert_eq!(json["agent_name"], "standup-scribe");
        assert_eq!(json["duration_ms"], 42);
        assert_eq!(json["exit_code"], 0);
        assert!(json["stderr"].is_null());
    }

    #[test]
    fn test_llm_daily_costs_response_serializes() {
        // GIVEN an LlmDailyCostsResponse
        let resp = LlmDailyCostsResponse {
            entries: vec![
                LlmDailyCostEntry {
                    date: "2026-03-12".to_string(),
                    backend: "anthropic".to_string(),
                    cost_usd: 0.15,
                },
                LlmDailyCostEntry {
                    date: "2026-03-13".to_string(),
                    backend: "local".to_string(),
                    cost_usd: 0.0,
                },
            ],
            days: 7,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN entries and days are correct
        assert_eq!(json["days"], 7);
        let entries = json["entries"].as_array().expect("entries is array");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["date"], "2026-03-12");
        assert_eq!(entries[0]["backend"], "anthropic");
    }

    #[test]
    fn test_global_timeline_event_serializes() {
        // GIVEN a GlobalTimelineEvent
        let event = GlobalTimelineEvent {
            event_type: "task".to_string(),
            timestamp: "2026-03-13T10:00:00Z".to_string(),
            summary: "[agent-1] Task → working".to_string(),
            detail: serde_json::json!({"status": "working"}),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&event).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["event_type"], "task");
        assert_eq!(json["summary"], "[agent-1] Task → working");
        assert_eq!(json["detail"]["status"], "working");
    }

    #[test]
    fn test_plan_cache_stats_view_serializes_populated() {
        // GIVEN a PlanCacheStatsView with data
        let stats = PlanCacheStatsView {
            total_entries: 42,
            cache_hits: 128,
            cache_misses: 30,
            hit_rate_pct: 81.01,
            oldest_entry_at: Some("2026-03-01T10:00:00Z".to_string()),
            newest_entry_at: Some("2026-03-24T15:00:00Z".to_string()),
        };

        // WHEN serialized
        let json = serde_json::to_value(&stats).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["total_entries"], 42);
        assert_eq!(json["cache_hits"], 128);
        assert_eq!(json["cache_misses"], 30);
        assert_eq!(json["hit_rate_pct"], 81.01);
        assert_eq!(json["oldest_entry_at"], "2026-03-01T10:00:00Z");
        assert_eq!(json["newest_entry_at"], "2026-03-24T15:00:00Z");
    }

    #[test]
    fn test_plan_cache_stats_view_serializes_empty() {
        // GIVEN an empty stats view
        let stats = PlanCacheStatsView {
            total_entries: 0,
            cache_hits: 0,
            cache_misses: 0,
            hit_rate_pct: 0.0,
            oldest_entry_at: None,
            newest_entry_at: None,
        };

        // WHEN serialized
        let json = serde_json::to_value(&stats).expect("serialize");

        // THEN zero counters and null dates
        assert_eq!(json["total_entries"], 0);
        assert_eq!(json["cache_hits"], 0);
        assert!(json["oldest_entry_at"].is_null());
        assert!(json["newest_entry_at"].is_null());
    }

    #[test]
    fn test_clear_cache_result_serializes() {
        // GIVEN a clear result
        let result = ClearCacheResult { cleared_count: 15 };

        // WHEN serialized
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN cleared_count is correct
        assert_eq!(json["cleared_count"], 15);
    }

    #[test]
    fn test_mailbox_message_row_serializes() {
        // GIVEN a mailbox row
        let row = MailboxMessageRow {
            message_id: "m-1".to_string(),
            from_agent: "apollia-guide".to_string(),
            to_agent: "seed-classifier".to_string(),
            payload: r#"{"kind":"ping"}"#.to_string(),
            sent_at: "2026-07-01T09:00:00Z".to_string(),
            state: "pending".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&row).expect("serialize");

        // THEN every field is present under its snake_case name
        assert_eq!(json["message_id"], "m-1");
        assert_eq!(json["from_agent"], "apollia-guide");
        assert_eq!(json["to_agent"], "seed-classifier");
        assert_eq!(json["payload"], r#"{"kind":"ping"}"#);
        assert_eq!(json["state"], "pending");
    }

    #[test]
    fn test_query_mailbox_db_missing_file_is_empty() {
        // GIVEN a data dir without a mailbox.db
        let dir = tempfile::tempdir().expect("tempdir");

        // WHEN querying the mailbox store
        let rows = query_mailbox_db(dir.path(), 100).expect("query");

        // THEN the result is empty, not an error
        assert!(rows.is_empty());
    }

    #[test]
    fn test_query_mailbox_db_orders_by_seq_desc() {
        // GIVEN a mailbox.db seeded with two messages of increasing seq
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mailbox.db");
        let conn = rusqlite::Connection::open(&path).expect("open");
        conn.execute_batch(
            "CREATE TABLE mailbox_messages (
                message_id       TEXT PRIMARY KEY,
                to_agent         TEXT    NOT NULL,
                from_agent       TEXT    NOT NULL,
                payload          TEXT    NOT NULL,
                sent_at          TEXT    NOT NULL,
                created_unix     INTEGER NOT NULL,
                state            TEXT    NOT NULL,
                lease_until_unix INTEGER,
                lease_owner      TEXT,
                seq              INTEGER NOT NULL
            );",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO mailbox_messages VALUES \
             ('m-1','b','a','{}','2026-07-01T09:00:00Z',1,'pending',NULL,NULL,1)",
            [],
        )
        .expect("insert 1");
        conn.execute(
            "INSERT INTO mailbox_messages VALUES \
             ('m-2','a','b','{}','2026-07-01T09:01:00Z',2,'pending',NULL,NULL,2)",
            [],
        )
        .expect("insert 2");

        // WHEN querying the mailbox store
        let rows = query_mailbox_db(dir.path(), 100).expect("query");

        // THEN the newest (highest seq) message comes first
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].message_id, "m-2");
        assert_eq!(rows[1].message_id, "m-1");
    }
}
