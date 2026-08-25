//! Timeline API, `GET /api/v1/tasks/{id}/timeline`.
//!
//! Aggregates execution data from 5 SQLite sources into a chronologically
//! ordered timeline for a given task. All data is read server-side in a single
//! `spawn_blocking` call per source, then merged and sorted by timestamp.
//!
//! Sources:
//! - `tasks.transitions_json` (hitl.db) → [`TimelineEvent::TaskTransition`]
//! - `plan_steps` (plans.db) → [`TimelineEvent::StepStarted`] + [`TimelineEvent::StepCompleted`]
//! - `llm_calls` (llm.db) → [`TimelineEvent::LlmCall`]
//! - `tool_invocations` (audit.db) → [`TimelineEvent::ToolCall`]
//! - `task_approvals` (hitl.db) → [`TimelineEvent::HitlSuspended`] + [`TimelineEvent::HitlResolved`]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use rusqlite::params;
use serde::Serialize;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Maximum length for input preview in step events (chars).
const MAX_INPUT_PREVIEW: usize = 200;
/// Maximum length for output preview in task completed events (chars).
const MAX_OUTPUT_PREVIEW: usize = 500;
/// Maximum length for tool call args_json preview (chars).
const MAX_TOOL_INPUT_PREVIEW: usize = 300;
/// Maximum length for tool call stdout/stderr output preview (chars).
const MAX_TOOL_OUTPUT_PREVIEW: usize = 500;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// A single event in a task's execution timeline.
///
/// Each variant maps to a kind of action recorded during execution. The JSON
/// `type` tag uses `snake_case`.
#[derive(Debug, Serialize, Clone, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimelineEvent {
    /// Task state transition (submitted -> running -> completed, etc.).
    TaskTransition {
        /// Target status name.
        status: String,
        /// ISO 8601 timestamp of the transition.
        timestamp: String,
    },
    /// Start of an ORIA step (orchestrated mode).
    StepStarted {
        /// Step identifier.
        step_id: String,
        /// Tool used or suggested.
        tool: Option<String>,
        /// Input preview (truncated to 200 chars).
        input_preview: Option<String>,
        /// ISO 8601 timestamp.
        timestamp: String,
    },
    /// Completion of an ORIA step.
    StepCompleted {
        /// Step identifier.
        step_id: String,
        /// Execution duration in milliseconds.
        duration_ms: Option<i64>,
        /// `true` if the step finished successfully.
        success: bool,
        /// ISO 8601 timestamp.
        timestamp: String,
    },
    /// Recorded LLM call.
    LlmCall {
        /// Backend name (e.g. `"anthropic"`, `"local"`).
        backend: String,
        /// Model identifier.
        model: String,
        /// Prompt tokens.
        prompt_tokens: Option<i64>,
        /// Completion tokens.
        completion_tokens: Option<i64>,
        /// Estimated cost in USD.
        cost_usd: Option<f64>,
        /// Latency in milliseconds.
        latency_ms: Option<i64>,
        /// ISO 8601 timestamp.
        timestamp: String,
    },
    /// Invocation of a native tool.
    ToolCall {
        /// Tool name.
        tool_name: String,
        /// Duration in milliseconds.
        duration_ms: Option<i64>,
        /// Process exit code (bash, python).
        exit_code: Option<i64>,
        /// `true` if the data was truncated.
        truncated: bool,
        /// Input preview (args_json), truncated to 300 chars.
        #[serde(skip_serializing_if = "Option::is_none")]
        input_preview: Option<String>,
        /// Output preview (stdout + stderr), truncated to 500 chars.
        #[serde(skip_serializing_if = "Option::is_none")]
        output_preview: Option<String>,
        /// ISO 8601 timestamp.
        timestamp: String,
    },
    /// HITL suspension: the agent requests human approval.
    HitlSuspended {
        /// Prompt shown to the operator.
        prompt: String,
        /// ISO 8601 timestamp of the suspension.
        timestamp: String,
    },
    /// HITL resolution: the operator has responded.
    HitlResolved {
        /// `true` if approved, `false` if rejected.
        approved: bool,
        /// Reason provided by the operator.
        reason: Option<String>,
        /// Wait duration in milliseconds.
        wait_ms: Option<i64>,
        /// ISO 8601 timestamp of the response.
        timestamp: String,
    },
    /// Task completion (terminal event).
    TaskCompleted {
        /// Output preview (truncated to 500 chars).
        output_preview: Option<String>,
        /// Total duration in milliseconds.
        duration_ms: Option<i64>,
        /// ISO 8601 timestamp.
        timestamp: String,
    },
}

/// Timeline response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TimelineResponse {
    /// Task identifier.
    pub task_id: String,
    /// Events sorted by timestamp ascending.
    pub events: Vec<TimelineEvent>,
}

/// Structured error response.
#[derive(Debug, Serialize)]
pub struct TimelineErrorResponse {
    /// Error message.
    pub error: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────────────────────────────────────

/// `GET /api/v1/tasks/{id}/timeline`: full timeline for a task.
///
/// Aggregates data from 5 SQLite sources (hitl.db, plans.db, llm.db, audit.db)
/// into a unified timeline sorted by timestamp ascending. Returns 404 when the
/// task is unknown to all sources.
#[utoipa::path(
    get,
    path = "/api/v1/tasks/{id}/timeline",
    tag = "tasks",
    params(("id" = String, Path, description = "Task id")),
    responses(
        (status = 200, description = "Aggregated execution timeline", body = TimelineResponse),
        (status = 404, description = "Task unknown to all sources", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Internal error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_task_timeline<B: ExecutionBackend + Clone>(
    Path(task_id): Path<String>,
    State(state): State<AppState<B>>,
) -> Result<Json<TimelineResponse>, (StatusCode, Json<TimelineErrorResponse>)> {
    let data_dir = resolve_data_dir(&state);

    let hitl_db = data_dir.join(apollia_core::paths::DataFile::Hitl.file_name());
    let plans_db = data_dir.join(apollia_core::paths::DataFile::Plans.file_name());
    let llm_db = data_dir.join(apollia_core::paths::DataFile::LlmCalls.file_name());
    let audit_db = data_dir.join(apollia_core::paths::DataFile::Audit.file_name());

    let tid = task_id.clone();

    // Read all sources in a single spawn_blocking to avoid multiple thread hops.
    let result = tokio::task::spawn_blocking(move || {
        collect_timeline_events(
            &TimelineDbPaths {
                hitl_db,
                plans_db,
                llm_db,
                audit_db,
            },
            &tid,
        )
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TimelineErrorResponse {
                error: format!("internal error: {e}"),
            }),
        )
    })?;

    match result {
        Ok(events) => Ok(Json(TimelineResponse { task_id, events })),
        Err(msg) => Err((
            StatusCode::NOT_FOUND,
            Json(TimelineErrorResponse { error: msg }),
        )),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers, data reading
// ─────────────────────────────────────────────────────────────────────────────

/// Paths to the SQLite sources aggregated into a task timeline.
struct TimelineDbPaths {
    hitl_db: std::path::PathBuf,
    plans_db: std::path::PathBuf,
    llm_db: std::path::PathBuf,
    audit_db: std::path::PathBuf,
}

/// Raw events gathered from every source before sorting and finalization.
struct GatheredEvents {
    events: Vec<(String, TimelineEvent)>,
    task_found: bool,
    completion_data: Option<(Option<String>, Option<i64>)>,
}

/// Read every source and build the sorted, finalized list of timeline events.
///
/// Returns `Err` with a not-found message when the task is unknown to all
/// sources.
fn collect_timeline_events(dbs: &TimelineDbPaths, tid: &str) -> Result<Vec<TimelineEvent>, String> {
    let GatheredEvents {
        mut events,
        task_found,
        completion_data,
    } = gather_timeline_events(dbs, tid);

    if !task_found && events.is_empty() {
        return Err(format!("task not found: {tid}"));
    }

    // Sort by timestamp ASC (ISO 8601 string comparison works)
    events.sort_by(|a, b| a.0.cmp(&b.0));

    // Append TaskCompleted as the terminal event (always last)
    if let Some((output_text, duration_ms)) = completion_data {
        let last_ts = events.last().map(|(ts, _)| ts.clone()).unwrap_or_default();
        events.push((
            last_ts.clone(),
            TimelineEvent::TaskCompleted {
                output_preview: output_text.map(|t| truncate_preview(&t, MAX_OUTPUT_PREVIEW).0),
                duration_ms,
                timestamp: last_ts,
            },
        ));
    }

    Ok(events.into_iter().map(|(_, e)| e).collect())
}

/// Gather raw (timestamp, event) pairs from the 5 SQLite sources.
fn gather_timeline_events(dbs: &TimelineDbPaths, tid: &str) -> GatheredEvents {
    let mut events: Vec<(String, TimelineEvent)> = Vec::new();
    let mut task_found = false;
    let mut completion_data: Option<(Option<String>, Option<i64>)> = None;

    // Source 1: transitions_json + output/duration from hitl.db (tasks table)
    if let Ok(conn) = rusqlite::Connection::open(&dbs.hitl_db) {
        if let Some((transitions, output_text, duration_ms, status)) = read_task_data(&conn, tid) {
            task_found = true;
            parse_transitions(&transitions, &mut events);

            // Defer TaskCompleted until after all events are collected
            if status == "completed" || status == "working" {
                completion_data = Some((output_text, duration_ms));
            }
        }

        // Source 5: task_approvals from hitl.db
        read_approvals(&conn, tid, &mut events);
    }

    // Source 2: plan_steps from plans.db
    if let Ok(conn) = rusqlite::Connection::open(&dbs.plans_db) {
        read_plan_steps(&conn, tid, &mut events);
        if !task_found {
            task_found = has_plan_for_task(&conn, tid);
        }
    }

    // Source 3: llm_calls from llm.db
    if let Ok(conn) = rusqlite::Connection::open(&dbs.llm_db) {
        read_llm_calls(&conn, tid, &mut events);
    }

    // Source 4: tool_invocations from audit.db
    if let Ok(conn) = rusqlite::Connection::open(&dbs.audit_db) {
        read_tool_calls(&conn, tid, &mut events);
    }

    GatheredEvents {
        events,
        task_found,
        completion_data,
    }
}

/// Resolve the runtime data directory from AppState.
fn resolve_data_dir<B: ExecutionBackend + Clone>(_state: &AppState<B>) -> std::path::PathBuf {
    apollia_core::paths::data_dir().unwrap_or_else(|| std::env::temp_dir().join("apollia"))
}

/// Data extracted from the `tasks` table for timeline construction.
type TaskData = (Option<String>, Option<String>, Option<i64>, String);

/// Read task data from the `tasks` table: (transitions_json, output_text, duration_ms, status).
fn read_task_data(conn: &rusqlite::Connection, task_id: &str) -> Option<TaskData> {
    conn.query_row(
        "SELECT transitions_json, output_text, duration_ms, status \
         FROM tasks WHERE task_id = ?1",
        params![task_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )
    .ok()
}

/// Parse the `transitions_json` field into `TaskTransition` events.
fn parse_transitions(json: &Option<String>, events: &mut Vec<(String, TimelineEvent)>) {
    let Some(json_str) = json.as_deref() else {
        return;
    };
    if json_str.is_empty() {
        return;
    }
    let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(json_str) else {
        return;
    };
    for entry in arr {
        let status = entry["status"].as_str().unwrap_or("unknown").to_string();
        let ts = entry["ts"].as_str().unwrap_or("").to_string();
        if !ts.is_empty() {
            events.push((
                ts.clone(),
                TimelineEvent::TaskTransition {
                    status,
                    timestamp: ts,
                },
            ));
        }
    }
}

/// Read HITL approvals from the `task_approvals` table.
fn read_approvals(
    conn: &rusqlite::Connection,
    task_id: &str,
    events: &mut Vec<(String, TimelineEvent)>,
) {
    let Ok(mut stmt) = conn.prepare(
        "SELECT prompt, suspended_at, approved, reason, responded_at, wait_duration_ms \
         FROM task_approvals WHERE task_id = ?1 \
         ORDER BY requested_at",
    ) else {
        return;
    };

    let Ok(rows) = stmt.query_map(params![task_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<i32>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    }) else {
        return;
    };

    for row_result in rows {
        let Ok((prompt, suspended_at, approved, reason, responded_at, wait_ms)) = row_result else {
            continue;
        };

        if let Some(ref ts) = suspended_at {
            events.push((
                ts.clone(),
                TimelineEvent::HitlSuspended {
                    prompt: prompt.clone(),
                    timestamp: ts.clone(),
                },
            ));
        }

        if let Some(ref ts) = responded_at {
            if let Some(approved_val) = approved {
                events.push((
                    ts.clone(),
                    TimelineEvent::HitlResolved {
                        approved: approved_val != 0,
                        reason,
                        wait_ms,
                        timestamp: ts.clone(),
                    },
                ));
            }
        }
    }
}

/// Read plan steps for a task from `execution_plans` + `plan_steps`.
fn read_plan_steps(
    conn: &rusqlite::Connection,
    task_id: &str,
    events: &mut Vec<(String, TimelineEvent)>,
) {
    let Ok(mut stmt) = conn.prepare(
        "SELECT ps.step_id, ps.tool_hint, ps.tool_used, ps.input_rendered, \
                ps.status, ps.started_at, ps.completed_at, ps.duration_ms \
         FROM plan_steps ps \
         JOIN execution_plans ep ON ps.plan_id = ep.plan_id \
         WHERE ep.task_id = ?1 \
         ORDER BY ps.started_at NULLS LAST",
    ) else {
        return;
    };

    let Ok(rows) = stmt.query_map(params![task_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<i64>>(7)?,
        ))
    }) else {
        return;
    };

    for row_result in rows {
        let Ok((
            step_id,
            tool_hint,
            tool_used,
            input_rendered,
            status,
            started_at,
            completed_at,
            duration_ms,
        )) = row_result
        else {
            continue;
        };

        let tool = tool_used.or(tool_hint);

        if let Some(ref ts) = started_at {
            events.push((
                ts.clone(),
                TimelineEvent::StepStarted {
                    step_id: step_id.clone(),
                    tool: tool.clone(),
                    input_preview: input_rendered
                        .map(|t| truncate_preview(&t, MAX_INPUT_PREVIEW).0),
                    timestamp: ts.clone(),
                },
            ));
        }

        if let Some(ref ts) = completed_at {
            let success = status == "completed";
            events.push((
                ts.clone(),
                TimelineEvent::StepCompleted {
                    step_id,
                    duration_ms,
                    success,
                    timestamp: ts.clone(),
                },
            ));
        }
    }
}

/// Check if a plan exists for this task in `execution_plans`.
fn has_plan_for_task(conn: &rusqlite::Connection, task_id: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM execution_plans WHERE task_id = ?1",
        params![task_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}

/// Read LLM calls for a task from `llm_calls`.
fn read_llm_calls(
    conn: &rusqlite::Connection,
    task_id: &str,
    events: &mut Vec<(String, TimelineEvent)>,
) {
    let Ok(mut stmt) = conn.prepare(
        "SELECT backend, model, prompt_tokens, completion_tokens, \
                cost_usd, latency_ms, created_at \
         FROM llm_calls WHERE task_id = ?1 \
         ORDER BY created_at",
    ) else {
        return;
    };

    let Ok(rows) = stmt.query_map(params![task_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<f64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, String>(6)?,
        ))
    }) else {
        return;
    };

    for row_result in rows {
        let Ok((
            backend,
            model,
            prompt_tokens,
            completion_tokens,
            cost_usd,
            latency_ms,
            created_at,
        )) = row_result
        else {
            continue;
        };

        events.push((
            created_at.clone(),
            TimelineEvent::LlmCall {
                backend,
                model,
                prompt_tokens,
                completion_tokens,
                cost_usd,
                latency_ms,
                timestamp: created_at,
            },
        ));
    }
}

/// Read tool invocations for a task from `tool_invocations`.
fn read_tool_calls(
    conn: &rusqlite::Connection,
    task_id: &str,
    events: &mut Vec<(String, TimelineEvent)>,
) {
    let Ok(mut stmt) = conn.prepare(
        "SELECT tool_name, duration_ms, exit_code, started_at, args_json, stdout, stderr \
         FROM tool_invocations WHERE task_id = ?1 \
         ORDER BY started_at",
    ) else {
        return;
    };

    let Ok(rows) = stmt.query_map(params![task_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    }) else {
        return;
    };

    for row_result in rows {
        let Ok((tool_name, duration_ms, exit_code, started_at, args_json, stdout, stderr)) =
            row_result
        else {
            continue;
        };

        let raw_input = args_json.as_deref().unwrap_or("");
        let (input_preview, input_truncated): (Option<String>, bool) = if raw_input.is_empty() {
            (None, false)
        } else {
            let (p, t) = truncate_preview(raw_input, MAX_TOOL_INPUT_PREVIEW);
            (Some(p), t)
        };

        let combined_output = match (stdout.as_deref(), stderr.as_deref()) {
            (Some(out), Some(err)) if !out.is_empty() && !err.is_empty() => {
                format!("{out}\n--- stderr ---\n{err}")
            }
            (Some(out), _) if !out.is_empty() => out.to_string(),
            (_, Some(err)) if !err.is_empty() => err.to_string(),
            _ => String::new(),
        };
        let (output_preview, output_truncated): (Option<String>, bool) =
            if combined_output.is_empty() {
                (None, false)
            } else {
                let (p, t) = truncate_preview(&combined_output, MAX_TOOL_OUTPUT_PREVIEW);
                (Some(p), t)
            };

        let truncated = input_truncated || output_truncated;

        events.push((
            started_at.clone(),
            TimelineEvent::ToolCall {
                tool_name,
                duration_ms,
                exit_code,
                truncated,
                input_preview,
                output_preview,
                timestamp: started_at,
            },
        ));
    }
}

/// Truncate a string to `max_chars` characters (UTF-8 safe), appending `"..."`
/// if truncated. Returns `(rendered, was_truncated)`.
///
/// Slicing by byte index would panic when `max_chars` falls inside a
/// multi-byte codepoint (e.g. `'à'`, `'â'`, `'é'`), so we walk the
/// `chars()` iterator in a single pass.
fn truncate_preview(text: &str, max_chars: usize) -> (String, bool) {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        (format!("{head}..."), true)
    } else {
        (head, false)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ────────────────────────────────────────────────────────────

    /// Create a temporary data directory with all required DBs initialized.
    fn setup_test_dbs(dir: &std::path::Path) {
        // hitl.db, tasks + task_approvals tables
        let conn =
            rusqlite::Connection::open(dir.join(apollia_core::paths::DataFile::Hitl.file_name()))
                .expect("open hitl.db");
        conn.execute_batch(include_str!(
            "../../../apollia-tools/migrations/005_hitl_tables.sql"
        ))
        .expect("hitl migration");
        // Add observability columns
        for col in &[
            "input_text TEXT",
            "input_truncated INTEGER NOT NULL DEFAULT 0",
            "output_text TEXT",
            "output_truncated INTEGER NOT NULL DEFAULT 0",
            "duration_ms INTEGER",
            "transitions_json TEXT",
        ] {
            let _ = conn.execute_batch(&format!("ALTER TABLE tasks ADD COLUMN {col}"));
        }
        for col in &["suspended_at TEXT", "wait_duration_ms INTEGER"] {
            let _ = conn.execute_batch(&format!("ALTER TABLE task_approvals ADD COLUMN {col}"));
        }

        // plans.db, execution_plans + plan_steps tables
        let conn =
            rusqlite::Connection::open(dir.join(apollia_core::paths::DataFile::Plans.file_name()))
                .expect("open plans.db");
        conn.execute_batch(include_str!(
            "../../../apollia-tools/migrations/004_execution_plans.sql"
        ))
        .expect("plans migration");
        for col in &[
            "input_rendered TEXT",
            "input_truncated INTEGER NOT NULL DEFAULT 0",
            "output_text TEXT",
            "output_truncated INTEGER NOT NULL DEFAULT 0",
            "tool_used TEXT",
            "error_detail TEXT",
            "duration_ms INTEGER",
        ] {
            let _ = conn.execute_batch(&format!("ALTER TABLE plan_steps ADD COLUMN {col}"));
        }

        // llm_calls.db
        let conn = rusqlite::Connection::open(
            dir.join(apollia_core::paths::DataFile::LlmCalls.file_name()),
        )
        .expect("open llm.db");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS llm_calls (
                id TEXT PRIMARY KEY,
                task_id TEXT,
                step_id TEXT,
                backend TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_tokens INTEGER,
                completion_tokens INTEGER,
                cost_usd REAL,
                latency_ms INTEGER,
                prompt_text TEXT,
                completion_text TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )
        .expect("llm migration");

        // audit.db
        let conn =
            rusqlite::Connection::open(dir.join(apollia_core::paths::DataFile::Audit.file_name()))
                .expect("open audit.db");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tool_invocations (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                input_hash TEXT NOT NULL,
                sandbox_profile TEXT NOT NULL,
                started_at TEXT NOT NULL,
                duration_ms INTEGER,
                exit_code INTEGER,
                success INTEGER NOT NULL,
                error_code TEXT,
                resources_used TEXT,
                args_json TEXT,
                stdout TEXT,
                stderr TEXT
            );",
        )
        .expect("audit migration");
    }

    // ── Direct-mode timeline, chronological order ─────────────────────────

    #[test]
    fn test_timeline_mode_direct_chronological() {
        // GIVEN a direct-mode task with transitions + 2 tool calls
        let dir = tempfile::tempdir().expect("tempdir");
        setup_test_dbs(dir.path());

        let task_id = "t-direct-1";
        let hitl = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Hitl.file_name()),
        )
        .expect("open");
        hitl.execute(
            "INSERT INTO tasks (task_id, status, transitions_json, output_text, duration_ms) \
             VALUES (?1, 'completed', ?2, 'result', 1500)",
            params![
                task_id,
                r#"[{"status":"submitted","ts":"2026-03-13T10:00:00Z"},{"status":"running","ts":"2026-03-13T10:00:01Z"}]"#,
            ],
        )
        .expect("insert task");

        let audit = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Audit.file_name()),
        )
        .expect("open");
        for (i, ts) in ["2026-03-13T10:00:02Z", "2026-03-13T10:00:03Z"]
            .iter()
            .enumerate()
        {
            audit
                .execute(
                    "INSERT INTO tool_invocations \
                     (id, agent_id, task_id, tool_name, input_hash, sandbox_profile, \
                      started_at, success, duration_ms) \
                     VALUES (?1, 'agent-1', ?2, 'file_io', 'hash', 'default', ?3, 1, 100)",
                    params![format!("inv-{i}"), task_id, ts],
                )
                .expect("insert invocation");
        }

        // WHEN we read the timeline (same logic as handler: defer TaskCompleted)
        let mut events: Vec<(String, TimelineEvent)> = Vec::new();
        let mut completion_data: Option<(Option<String>, Option<i64>)> = None;
        let conn = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Hitl.file_name()),
        )
        .expect("open");
        if let Some((transitions, output_text, duration_ms, status)) =
            read_task_data(&conn, task_id)
        {
            parse_transitions(&transitions, &mut events);
            if status == "completed" {
                completion_data = Some((output_text, duration_ms));
            }
        }
        let audit_conn = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Audit.file_name()),
        )
        .expect("open");
        read_tool_calls(&audit_conn, task_id, &mut events);

        events.sort_by(|a, b| a.0.cmp(&b.0));

        if let Some((output_text, duration_ms)) = completion_data {
            let last_ts = events.last().map(|(ts, _)| ts.clone()).unwrap_or_default();
            events.push((
                last_ts.clone(),
                TimelineEvent::TaskCompleted {
                    output_preview: output_text.map(|t| truncate_preview(&t, MAX_OUTPUT_PREVIEW).0),
                    duration_ms,
                    timestamp: last_ts,
                },
            ));
        }

        // THEN events are in chronological order
        assert!(events.len() >= 5, "got {} events", events.len());

        let types: Vec<String> = events
            .iter()
            .map(|(_, e)| match e {
                TimelineEvent::TaskTransition { .. } => "transition".into(),
                TimelineEvent::ToolCall { .. } => "tool_call".into(),
                TimelineEvent::TaskCompleted { .. } => "completed".into(),
                _ => "other".into(),
            })
            .collect();
        assert_eq!(types[0], "transition"); // submitted
        assert_eq!(types[1], "transition"); // running
        assert_eq!(types[2], "tool_call");
        assert_eq!(types[3], "tool_call");
        assert_eq!(types[4], "completed");
    }

    // ── Orchestrated-mode timeline with steps + LLM calls ─────────────────

    #[test]
    fn test_timeline_orchestrated_with_steps() {
        // GIVEN an orchestrated task with 2 steps + 1 LLM call
        let dir = tempfile::tempdir().expect("tempdir");
        setup_test_dbs(dir.path());

        let task_id = "t-orch-1";

        // Insert plan
        let plans = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Plans.file_name()),
        )
        .expect("open");
        plans
            .execute(
                "INSERT INTO execution_plans (plan_id, task_id, agent_name, status, replan_count) \
                 VALUES ('plan-1', ?1, 'agent-1', 'completed', 0)",
                params![task_id],
            )
            .expect("insert plan");
        plans
            .execute(
                "INSERT INTO plan_steps (step_id, plan_id, description, status, \
                 started_at, completed_at, tool_used, duration_ms) \
                 VALUES ('s1', 'plan-1', 'Step 1', 'completed', \
                 '2026-03-13T10:00:01Z', '2026-03-13T10:00:02Z', 'file_io', 1000)",
                [],
            )
            .expect("insert step 1");
        plans
            .execute(
                "INSERT INTO plan_steps (step_id, plan_id, description, status, \
                 started_at, completed_at, tool_used, duration_ms) \
                 VALUES ('s2', 'plan-1', 'Step 2', 'completed', \
                 '2026-03-13T10:00:03Z', '2026-03-13T10:00:04Z', 'bash', 500)",
                [],
            )
            .expect("insert step 2");

        // Insert LLM call
        let llm = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::LlmCalls.file_name()),
        )
        .expect("open");
        llm.execute(
            "INSERT INTO llm_calls (id, task_id, backend, model, prompt_tokens, \
             completion_tokens, cost_usd, latency_ms, created_at) \
             VALUES ('llm-1', ?1, 'anthropic', 'sonnet', 100, 50, 0.001, 200, \
             '2026-03-13T10:00:00Z')",
            params![task_id],
        )
        .expect("insert llm call");

        // WHEN we read the timeline
        let mut events: Vec<(String, TimelineEvent)> = Vec::new();
        read_plan_steps(&plans, task_id, &mut events);
        read_llm_calls(&llm, task_id, &mut events);
        events.sort_by(|a, b| a.0.cmp(&b.0));

        // THEN it contains LlmCall + StepStarted + StepCompleted in order
        assert!(events.len() >= 5, "got {} events", events.len());

        let types: Vec<&str> = events
            .iter()
            .map(|(_, e)| match e {
                TimelineEvent::LlmCall { .. } => "llm_call",
                TimelineEvent::StepStarted { .. } => "step_started",
                TimelineEvent::StepCompleted { .. } => "step_completed",
                _ => "other",
            })
            .collect();
        assert_eq!(types[0], "llm_call"); // 10:00:00
        assert_eq!(types[1], "step_started"); // s1 10:00:01
        assert_eq!(types[2], "step_completed"); // s1 10:00:02
        assert_eq!(types[3], "step_started"); // s2 10:00:03
        assert_eq!(types[4], "step_completed"); // s2 10:00:04
    }

    // ── HITL suspension + resolution timeline ─────────────────────────────

    #[test]
    fn test_timeline_hitl_suspension_and_resolution() {
        // GIVEN an approval with suspended_at + responded_at
        let dir = tempfile::tempdir().expect("tempdir");
        setup_test_dbs(dir.path());

        let task_id = "t-hitl-1";

        let conn = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Hitl.file_name()),
        )
        .expect("open");
        conn.execute(
            "INSERT INTO tasks (task_id, status) VALUES (?1, 'working')",
            params![task_id],
        )
        .expect("insert task");
        conn.execute(
            "INSERT INTO task_approvals \
             (task_id, prompt, context_json, approved, reason, \
              suspended_at, responded_at, wait_duration_ms) \
             VALUES (?1, 'Confirmer ?', '{}', 1, NULL, \
              '2026-03-13T14:30:00Z', '2026-03-13T14:35:00Z', 300000)",
            params![task_id],
        )
        .expect("insert approval");

        // WHEN we read the approvals
        let mut events: Vec<(String, TimelineEvent)> = Vec::new();
        read_approvals(&conn, task_id, &mut events);
        events.sort_by(|a, b| a.0.cmp(&b.0));

        // THEN it contains HitlSuspended then HitlResolved
        assert_eq!(events.len(), 2, "got {} events", events.len());

        match &events[0].1 {
            TimelineEvent::HitlSuspended { prompt, timestamp } => {
                assert_eq!(prompt, "Confirmer ?");
                assert_eq!(timestamp, "2026-03-13T14:30:00Z");
            }
            other => panic!("expected HitlSuspended, got {other:?}"),
        }

        match &events[1].1 {
            TimelineEvent::HitlResolved {
                approved,
                wait_ms,
                timestamp,
                ..
            } => {
                assert!(*approved);
                assert_eq!(*wait_ms, Some(300000));
                assert_eq!(timestamp, "2026-03-13T14:35:00Z");
            }
            other => panic!("expected HitlResolved, got {other:?}"),
        }
    }

    // ── Task not found -> Err ─────────────────────────────────────────────

    #[test]
    fn test_timeline_task_not_found() {
        // GIVEN empty DBs
        let dir = tempfile::tempdir().expect("tempdir");
        setup_test_dbs(dir.path());

        // WHEN we look up a nonexistent task
        let events: Vec<(String, TimelineEvent)> = Vec::new();
        let mut task_found = false;
        let conn = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Hitl.file_name()),
        )
        .expect("open");
        if read_task_data(&conn, "nonexistent").is_some() {
            task_found = true;
        }

        // THEN task not found
        assert!(!task_found);
        assert!(events.is_empty());
    }

    // ── In-progress task -> partial timeline without TaskCompleted ────────

    #[test]
    fn test_timeline_in_progress_no_task_completed() {
        // GIVEN an in-progress task (status = running)
        let dir = tempfile::tempdir().expect("tempdir");
        setup_test_dbs(dir.path());

        let task_id = "t-running-1";
        let conn = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Hitl.file_name()),
        )
        .expect("open");
        conn.execute(
            "INSERT INTO tasks (task_id, status, transitions_json) \
             VALUES (?1, 'input_required', ?2)",
            params![
                task_id,
                r#"[{"status":"submitted","ts":"2026-03-13T10:00:00Z"},{"status":"running","ts":"2026-03-13T10:00:01Z"}]"#,
            ],
        )
        .expect("insert task");

        // WHEN we read the timeline
        let mut events: Vec<(String, TimelineEvent)> = Vec::new();
        if let Some((transitions, _output_text, _duration_ms, status)) =
            read_task_data(&conn, task_id)
        {
            parse_transitions(&transitions, &mut events);
            // Only add TaskCompleted if status is "completed"
            if status == "completed" || status == "working" {
                // Not reached for "input_required"
                events.push((
                    String::new(),
                    TimelineEvent::TaskCompleted {
                        output_preview: None,
                        duration_ms: None,
                        timestamp: String::new(),
                    },
                ));
            }
        }

        // THEN partial timeline, no TaskCompleted
        assert_eq!(events.len(), 2, "got {} events", events.len());
        let has_completed = events
            .iter()
            .any(|(_, e)| matches!(e, TimelineEvent::TaskCompleted { .. }));
        assert!(!has_completed, "no TaskCompleted for in-progress task");
    }

    // ── truncate_preview ─────────────────────────────────────────────────

    #[test]
    fn test_truncate_preview_short() {
        // GIVEN text shorter than the limit
        // WHEN we truncate
        // THEN the text is returned unchanged and the truncated flag is false
        assert_eq!(truncate_preview("hello", 10), ("hello".to_string(), false));
    }

    #[test]
    fn test_truncate_preview_long() {
        // GIVEN text longer than the limit
        // WHEN we truncate to 200 characters
        // THEN the result ends with "..." and the truncated flag is true
        let long = "x".repeat(300);
        let (rendered, truncated) = truncate_preview(&long, 200);
        assert!(rendered.ends_with("..."));
        assert_eq!(rendered.chars().count(), 203);
        assert!(truncated);
    }

    #[test]
    fn test_truncate_preview_utf8_boundary() {
        // GIVEN accented (multi-byte) text where the limit falls exactly on
        // the boundary of an 'à' (2 bytes in UTF-8)
        // WHEN we truncate by character count, not by bytes
        // THEN no panic, and truncation respects codepoints
        // (regression: an earlier version sliced by byte and panicked with
        //  "byte index N is not a char boundary").
        let text = "à".repeat(300); // 300 chars, 600 bytes
        let (rendered, truncated) = truncate_preview(&text, 200);
        assert!(rendered.ends_with("..."));
        assert_eq!(rendered.chars().count(), 203);
        assert!(truncated);
    }

    #[test]
    fn test_truncate_preview_exact_length() {
        // GIVEN text of length equal to the limit
        // WHEN we truncate
        // THEN no truncation is applied
        let text = "a".repeat(50);
        let (rendered, truncated) = truncate_preview(&text, 50);
        assert_eq!(rendered.chars().count(), 50);
        assert!(!truncated);
    }

    // ── tool_call enrichment (input_preview + output_preview) ────────────

    #[test]
    fn test_ac_tool_call_enrichment_basic() {
        // GIVEN a tool_invocation with args_json + stdout
        let dir = tempfile::tempdir().expect("tempdir");
        setup_test_dbs(dir.path());

        let task_id = "t-enrich-1";
        let audit = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Audit.file_name()),
        )
        .expect("open");
        audit
            .execute(
                "INSERT INTO tool_invocations \
                 (id, agent_id, task_id, tool_name, input_hash, sandbox_profile, \
                  started_at, success, duration_ms, args_json, stdout, stderr) \
                 VALUES ('inv-1', 'agent-1', ?1, 'bash_executor', 'hash', 'default', \
                         '2026-03-13T10:00:02Z', 1, 150, '{\"command\":\"ls -la\"}', 'file1\nfile2', NULL)",
                params![task_id],
            )
            .expect("insert invocation");

        // WHEN we read the tool calls
        let mut events: Vec<(String, TimelineEvent)> = Vec::new();
        read_tool_calls(&audit, task_id, &mut events);

        // THEN the event contains input_preview and output_preview
        assert_eq!(events.len(), 1);
        let (_, event) = &events[0];
        match event {
            TimelineEvent::ToolCall {
                input_preview,
                output_preview,
                truncated,
                ..
            } => {
                assert_eq!(input_preview.as_deref(), Some("{\"command\":\"ls -la\"}"));
                assert_eq!(output_preview.as_deref(), Some("file1\nfile2"));
                assert!(!truncated, "should not be truncated");
            }
            _ => panic!("expected ToolCall event"),
        }
    }

    #[test]
    fn test_ac_tool_call_enrichment_truncation() {
        // GIVEN an args_json of 400 chars (> MAX_TOOL_INPUT_PREVIEW=300)
        let dir = tempfile::tempdir().expect("tempdir");
        setup_test_dbs(dir.path());

        let task_id = "t-enrich-2";
        let long_args = "a".repeat(400);
        let audit = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Audit.file_name()),
        )
        .expect("open");
        audit
            .execute(
                "INSERT INTO tool_invocations \
                 (id, agent_id, task_id, tool_name, input_hash, sandbox_profile, \
                  started_at, success, duration_ms, args_json, stdout, stderr) \
                 VALUES ('inv-2', 'agent-1', ?1, 'file_read', 'hash', 'default', \
                         '2026-03-13T10:00:03Z', 1, 50, ?2, NULL, NULL)",
                params![task_id, long_args],
            )
            .expect("insert invocation");

        let mut events: Vec<(String, TimelineEvent)> = Vec::new();
        read_tool_calls(&audit, task_id, &mut events);

        assert_eq!(events.len(), 1);
        match &events[0].1 {
            TimelineEvent::ToolCall {
                input_preview,
                truncated,
                ..
            } => {
                let preview = input_preview.as_deref().expect("should have preview");
                assert!(preview.ends_with("..."), "should end with ...");
                assert!(*truncated, "should be marked as truncated");
            }
            _ => panic!("expected ToolCall event"),
        }
    }

    #[test]
    fn test_ac_tool_call_enrichment_stderr_combined() {
        // GIVEN non-empty stdout + stderr
        let dir = tempfile::tempdir().expect("tempdir");
        setup_test_dbs(dir.path());

        let task_id = "t-enrich-3";
        let audit = rusqlite::Connection::open(
            dir.path()
                .join(apollia_core::paths::DataFile::Audit.file_name()),
        )
        .expect("open");
        audit
            .execute(
                "INSERT INTO tool_invocations \
                 (id, agent_id, task_id, tool_name, input_hash, sandbox_profile, \
                  started_at, success, duration_ms, args_json, stdout, stderr) \
                 VALUES ('inv-3', 'agent-1', ?1, 'bash_executor', 'hash', 'default', \
                         '2026-03-13T10:00:04Z', 0, 200, NULL, 'out', 'err')",
                params![task_id],
            )
            .expect("insert invocation");

        let mut events: Vec<(String, TimelineEvent)> = Vec::new();
        read_tool_calls(&audit, task_id, &mut events);

        match &events[0].1 {
            TimelineEvent::ToolCall {
                output_preview,
                input_preview,
                ..
            } => {
                assert!(input_preview.is_none(), "no args_json → no input_preview");
                let out = output_preview
                    .as_deref()
                    .expect("should have output_preview");
                assert!(out.contains("out"), "should contain stdout");
                assert!(out.contains("--- stderr ---"), "should contain separator");
                assert!(out.contains("err"), "should contain stderr");
            }
            _ => panic!("expected ToolCall event"),
        }
    }
}
