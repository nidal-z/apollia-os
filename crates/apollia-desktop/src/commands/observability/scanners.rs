//! The readers the global timeline aggregates: one per store it opens, plus
//! the labelling and summarising the view needs to show a row in one language.
//!
//! Every scanner is best-effort by design. A store that is absent, locked or
//! from an older schema yields no rows rather than failing the whole timeline.

use super::{label_for, trim_for_summary, GlobalTimelineEvent};

pub(super) fn scan_audit_db(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    labels: &std::collections::HashMap<String, String>,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join(apollia_core::paths::DataFile::Audit.file_name());
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
                "source": apollia_core::paths::DataFile::Audit.file_name(),
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

pub(super) fn scan_llm_calls_db(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    labels: &std::collections::HashMap<String, String>,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join(apollia_core::paths::DataFile::LlmCalls.file_name());
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
                "source": apollia_core::paths::DataFile::LlmCalls.file_name(),
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

pub(super) fn scan_hitl_tasks(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join(apollia_core::paths::DataFile::Hitl.file_name());
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
// REASON: internal helper pushing one transition row; the arguments are that row's columns.
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
        summary: format!("[{agent_name}] Task → {status}{dur}"),
        detail: serde_json::json!({
            "source": "hitl.db/tasks",
            "task_id": task_id,
            "agent_name": agent_name,
            "status": status,
            "duration_ms": duration_ms,
        }),
    });
}

pub(super) fn scan_hitl_approvals(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join(apollia_core::paths::DataFile::Hitl.file_name());
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

pub(super) fn scan_chat_sessions(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join(apollia_core::paths::DataFile::Chat.file_name());
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
            .unwrap_or_else(|| "(untitled)".to_string());
        if created_at.as_str() >= cutoff_str {
            events.push(GlobalTimelineEvent {
                event_type: "task".to_string(),
                timestamp: created_at.clone(),
                summary: format!("[{label}] Chat opened · {title_label}"),
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
                    summary: format!("[{label}] Chat closed · {title_label}"),
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

pub(super) fn scan_chat_approvals(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join(apollia_core::paths::DataFile::Chat.file_name());
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

pub(super) fn scan_trigger_history(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join(apollia_core::paths::DataFile::Triggers.file_name());
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

pub(super) fn scan_runtime_events(
    data_dir: &std::path::Path,
    cutoff_str: &str,
    labels: &std::collections::HashMap<String, String>,
    events: &mut Vec<GlobalTimelineEvent>,
) {
    let path = data_dir.join(apollia_core::paths::DataFile::RuntimeEvents.file_name());
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
                "source": apollia_core::paths::DataFile::RuntimeEvents.file_name(),
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
}
