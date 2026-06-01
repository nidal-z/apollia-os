//! `EventPersistor`, an append-only SQLite actor for `runtime_events.db`.
//!
//! Follows the same pattern as `apollia_tools::audit::AuditTrailHandle`:
//! - bounded `tokio::sync::mpsc` inbox, with a dedicated
//!   `tokio::task::spawn_blocking` owning the non-`Sync` `rusqlite` connection,
//! - fire-and-forget writes so the agent thread never blocks,
//! - versioned schema loaded at startup (idempotent).
//!
//! The persistor maps each `RuntimeEvent` variant that participates in the
//! event log to a persistable record; variants that do not participate are
//! skipped.

use std::path::{Path, PathBuf};

use apollia_core::events::RuntimeEvent;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use super::repository::RuntimeEventRecord;
use crate::eventbus::EventBusSender;

const CHANNEL_CAPACITY: usize = 1024;

/// Inline SQL schema, mirror of the `runtime_events` migration.
///
/// Pulling it in via `include_str!` would have required reorganizing the
/// `apollia-tools` crate, so the schema is duplicated here in a controlled
/// way until the two are consolidated.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS runtime_events (
    event_id         TEXT PRIMARY KEY,
    task_id          TEXT NOT NULL,
    agent_id         TEXT NOT NULL,
    parent_event_id  TEXT,
    correlation_id   TEXT,
    step_num         INTEGER,
    kind             TEXT NOT NULL,
    payload_json     TEXT NOT NULL,
    ts               TEXT NOT NULL,
    created_at_unix  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_events_task_ts
    ON runtime_events(task_id, ts);
CREATE INDEX IF NOT EXISTS idx_runtime_events_parent
    ON runtime_events(parent_event_id);
CREATE INDEX IF NOT EXISTS idx_runtime_events_correlation
    ON runtime_events(correlation_id);
CREATE INDEX IF NOT EXISTS idx_runtime_events_created_at
    ON runtime_events(created_at_unix);

CREATE TRIGGER IF NOT EXISTS runtime_events_no_update
BEFORE UPDATE ON runtime_events
BEGIN
    SELECT RAISE(ABORT, 'runtime_events is append-only');
END;
"#;

/// Errors raised while opening or initializing the persistor.
#[derive(Debug, Error)]
pub enum EventPersistorError {
    /// Failed to open the SQLite file.
    #[error("failed to open runtime_events database at {path}: {source}")]
    OpenFailed {
        /// Path that was attempted.
        path: PathBuf,
        /// Underlying cause.
        source: rusqlite::Error,
    },
    /// Failed to create the schema (table, indexes, trigger).
    #[error("failed to initialize runtime_events schema: {0}")]
    SchemaInitFailed(String),
    /// The init channel closed prematurely (the worker thread crashed).
    #[error("event persistor init channel disconnected")]
    InitDisconnected,
}

/// Internal messages sent to the actor.
enum PersistorMessage {
    /// Fire-and-forget append of an event.
    Append(Box<RuntimeEventRecord>),
    /// Clean shutdown after the queue has been drained.
    Shutdown,
}

/// Internal actor that exclusively owns the SQLite `Connection`.
struct EventPersistor {
    conn: rusqlite::Connection,
    receiver: mpsc::Receiver<PersistorMessage>,
}

impl EventPersistor {
    fn run(mut self) {
        while let Some(msg) = self.receiver.blocking_recv() {
            match msg {
                PersistorMessage::Append(record) => {
                    if let Err(e) = Self::insert(&self.conn, &record) {
                        tracing::error!(
                            error = %e,
                            event_id = %record.event_id,
                            kind = %record.kind,
                            "runtime_events insert failed",
                        );
                    }
                }
                PersistorMessage::Shutdown => break,
            }
        }
    }

    fn insert(conn: &rusqlite::Connection, r: &RuntimeEventRecord) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO runtime_events \
             (event_id, task_id, agent_id, parent_event_id, correlation_id, \
              step_num, kind, payload_json, ts, created_at_unix) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                r.event_id,
                r.task_id,
                r.agent_id,
                r.parent_event_id,
                r.correlation_id,
                r.step_num,
                r.kind,
                r.payload_json,
                r.ts,
                r.created_at_unix,
            ],
        )?;
        Ok(())
    }
}

/// Clonable handle to the actor.
///
/// All methods are thread-safe; several handles may emit to the same actor.
/// Writes are fire-and-forget: when the channel is full, the record is
/// dropped with a `warn!` (backpressure).
#[derive(Clone)]
pub struct EventPersistorHandle {
    sender: mpsc::Sender<PersistorMessage>,
}

impl EventPersistorHandle {
    /// Opens `db_path`, applies the idempotent schema, and starts the actor
    /// in the background. WAL mode is enabled for read/write concurrency
    /// (reads via `RuntimeEventsRepository` run alongside inserts).
    pub async fn open(db_path: &Path) -> Result<Self, EventPersistorError> {
        let db_path = db_path.to_path_buf();
        let (sender, receiver) = mpsc::channel::<PersistorMessage>(CHANNEL_CAPACITY);
        let (init_tx, init_rx) = oneshot::channel::<Result<(), EventPersistorError>>();

        let path_for_init = db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = match rusqlite::Connection::open(&path_for_init) {
                Ok(c) => c,
                Err(e) => {
                    let _ = init_tx.send(Err(EventPersistorError::OpenFailed {
                        path: path_for_init.clone(),
                        source: e,
                    }));
                    return;
                }
            };

            if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL;") {
                let _ = init_tx.send(Err(EventPersistorError::SchemaInitFailed(e.to_string())));
                return;
            }

            if let Err(e) = conn.execute_batch(SCHEMA) {
                let _ = init_tx.send(Err(EventPersistorError::SchemaInitFailed(e.to_string())));
                return;
            }

            let _ = init_tx.send(Ok(()));

            EventPersistor { conn, receiver }.run();
        });

        init_rx
            .await
            .map_err(|_| EventPersistorError::InitDisconnected)??;

        Ok(Self { sender })
    }

    /// Fire-and-forget append of an event.
    ///
    /// When the channel is full (>1024 events pending), the record is dropped
    /// with a `warn!` rather than blocking the agent thread. A dedicated
    /// `bus_lagged` event surfaces the loss in the UI.
    pub fn append(&self, record: RuntimeEventRecord) {
        match self
            .sender
            .try_send(PersistorMessage::Append(Box::new(record)))
        {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("runtime_events persistor channel full, event dropped");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("runtime_events persistor disconnected, event dropped");
            }
        }
    }

    /// Stops the actor after the queue has been drained.
    pub async fn shutdown(&self) {
        let _ = self.sender.send(PersistorMessage::Shutdown).await;
    }
}

/// Spawns an EventBus subscriber feeding the `EventPersistor`.
///
/// Filters the `RuntimeEvent`s that map to a persistable kind and routes the
/// serialized payload to the persistor.
///
/// The returned `JoinHandle` can be awaited to confirm a clean exit (the
/// subscriber stops when the `EventBus` is closed).
pub fn spawn_runtime_events_subscriber(
    handle: EventPersistorHandle,
    event_bus: &EventBusSender,
) -> tokio::task::JoinHandle<()> {
    let mut rx = event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(record) = event_to_record(event) {
                        handle.append(record);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        skipped = n,
                        "runtime_events subscriber lagged - events dropped (Lot 4 will surface as bus_lagged)",
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("runtime_events subscriber: event bus closed, exiting");
                    break;
                }
            }
        }
    })
}

/// Maps a `RuntimeEvent` to a persistable `RuntimeEventRecord`.
///
/// Returns `None` for variants that do not participate in the event log.
/// Each new persisted event type is a single extra arm in the `match`.
fn event_to_record(event: RuntimeEvent) -> Option<RuntimeEventRecord> {
    let now_unix = chrono::Utc::now().timestamp();
    let now_iso = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    match event {
        RuntimeEvent::AgentLog {
            task_id,
            agent_id,
            level,
            message,
            extra_fields_json,
        } => {
            let payload = serde_json::json!({
                "level": level,
                "message": message,
                "extra_fields_json": extra_fields_json,
            })
            .to_string();
            Some(RuntimeEventRecord {
                event_id: uuid::Uuid::now_v7().to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                parent_event_id: None,
                correlation_id: None,
                step_num: None,
                kind: "agent_log".to_string(),
                payload_json: payload,
                ts: now_iso,
                created_at_unix: now_unix,
            })
        }

        RuntimeEvent::Thought {
            task_id,
            agent_id,
            step_num,
            text,
        } => Some(RuntimeEventRecord {
            event_id: uuid::Uuid::now_v7().to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            parent_event_id: None,
            correlation_id: None,
            step_num: Some(step_num as i64),
            kind: "thought".to_string(),
            payload_json: serde_json::json!({ "text": text }).to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),
        RuntimeEvent::LlmCallStarted {
            task_id,
            agent_id,
            step_id,
            backend,
            model,
            messages_count,
            prompt_chars,
        } => Some(RuntimeEventRecord {
            event_id: uuid::Uuid::now_v7().to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            parent_event_id: None,
            correlation_id: None,
            step_num: None,
            kind: "llm_call_started".to_string(),
            payload_json: serde_json::json!({
                "step_id": step_id,
                "backend": backend,
                "model": model,
                "messages_count": messages_count,
                "prompt_chars": prompt_chars,
            })
            .to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),
        // This variant carries a structured ErrorAnalysis, richer than the ad
        // hoc fields we would otherwise have redefined. The payload keeps the
        // full analysis so the UI can display `category`, `severity`, etc.
        RuntimeEvent::LlmCallFailed {
            backend,
            model,
            task_id,
            step_id,
            error,
            analysis,
        } => {
            // Without a task_id there is nothing to attach the event to, so
            // skip it (the error is still logged via `tracing::*`).
            let task_id = task_id?;
            Some(RuntimeEventRecord {
                event_id: uuid::Uuid::now_v7().to_string(),
                task_id,
                agent_id: String::new(),
                parent_event_id: None,
                correlation_id: None,
                step_num: None,
                kind: "llm_call_failed".to_string(),
                payload_json: serde_json::json!({
                    "step_id": step_id,
                    "backend": backend,
                    "model": model,
                    "error": error,
                    "analysis": analysis,
                })
                .to_string(),
                ts: now_iso,
                created_at_unix: now_unix,
            })
        }
        RuntimeEvent::ToolCallStarted {
            event_id,
            task_id,
            agent_id,
            tool_name,
            args_json,
        } => Some(RuntimeEventRecord {
            // The event_id comes from the producer so the companion
            // ToolCallCompleted can reuse it as its parent_event_id.
            event_id,
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            parent_event_id: None,
            correlation_id: None,
            step_num: None,
            kind: "tool_call_started".to_string(),
            payload_json: serde_json::json!({
                "tool_name": tool_name,
                "args_json": args_json,
            })
            .to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),
        RuntimeEvent::ToolCallCompleted {
            parent_event_id,
            task_id,
            agent_id,
            tool_name,
            output_json,
            exit_code,
            duration_ms,
            success,
        } => Some(RuntimeEventRecord {
            event_id: uuid::Uuid::now_v7().to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            parent_event_id: Some(parent_event_id),
            correlation_id: None,
            step_num: None,
            kind: "tool_call_completed".to_string(),
            payload_json: serde_json::json!({
                "tool_name": tool_name,
                "output_json": output_json,
                "exit_code": exit_code,
                "duration_ms": duration_ms,
                "success": success,
            })
            .to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),
        RuntimeEvent::ToolCallDenied {
            parent_event_id,
            task_id,
            agent_id,
            tool_name,
            reason,
            detail,
        } => Some(RuntimeEventRecord {
            event_id: uuid::Uuid::now_v7().to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            parent_event_id: Some(parent_event_id),
            correlation_id: None,
            step_num: None,
            kind: "tool_call_denied".to_string(),
            payload_json: serde_json::json!({
                "tool_name": tool_name,
                "reason": reason,
                "detail": detail,
            })
            .to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),
        RuntimeEvent::A2AInvokeStarted {
            event_id,
            correlation_id,
            task_id,
            caller_agent_id,
            skill_id,
            child_task_id,
        } => Some(RuntimeEventRecord {
            event_id,
            task_id: task_id.to_string(),
            agent_id: caller_agent_id.to_string(),
            parent_event_id: None,
            correlation_id: Some(correlation_id),
            step_num: None,
            kind: "a2a_invoke_started".to_string(),
            payload_json: serde_json::json!({
                "skill_id": skill_id,
                "child_task_id": child_task_id.map(|t| t.to_string()),
            })
            .to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),
        RuntimeEvent::A2AInvokeCompleted {
            parent_event_id,
            task_id,
            skill_id,
            success,
            output_summary,
            duration_ms,
        } => Some(RuntimeEventRecord {
            event_id: uuid::Uuid::now_v7().to_string(),
            task_id: task_id.to_string(),
            // The completed event's agent_id is the caller's; the UI derives
            // it from the started event via parent_event_id.
            agent_id: String::new(),
            parent_event_id: Some(parent_event_id),
            correlation_id: None,
            step_num: None,
            kind: "a2a_invoke_completed".to_string(),
            payload_json: serde_json::json!({
                "skill_id": skill_id,
                "success": success,
                "output_summary": output_summary,
                "duration_ms": duration_ms,
            })
            .to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),
        RuntimeEvent::Retry {
            task_id,
            agent_id,
            step_num,
            cause,
            attempt,
        } => Some(RuntimeEventRecord {
            event_id: uuid::Uuid::now_v7().to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            parent_event_id: None,
            correlation_id: None,
            step_num: Some(step_num as i64),
            kind: "retry".to_string(),
            payload_json: serde_json::json!({
                "cause": cause,
                "attempt": attempt,
            })
            .to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),
        RuntimeEvent::ActionParseError {
            task_id,
            agent_id,
            step_num,
            raw_content,
            repair_attempted,
        } => Some(RuntimeEventRecord {
            event_id: uuid::Uuid::now_v7().to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            parent_event_id: None,
            correlation_id: None,
            step_num: Some(step_num as i64),
            kind: "action_parse_error".to_string(),
            payload_json: serde_json::json!({
                "raw_content": raw_content,
                "repair_attempted": repair_attempted,
            })
            .to_string(),
            ts: now_iso,
            created_at_unix: now_unix,
        }),

        // Variants outside observability are ignored.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventbus::EventBus;
    use crate::observability::repository::RuntimeEventsRepository;
    use apollia_core::events::RuntimeEvent;
    use tempfile::tempdir;

    #[tokio::test]
    async fn opens_and_creates_schema() {
        // GIVEN a fresh tempdir path
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");

        // WHEN we open the persistor
        let handle = EventPersistorHandle::open(&db).await.expect("open");

        // THEN the file is created and the schema is in place, verifiable
        // via a direct rusqlite open: the `runtime_events` table must exist.
        let conn = rusqlite::Connection::open(&db).expect("reopen");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='runtime_events'",
                [],
                |row| row.get(0),
            )
            .expect("query");
        assert_eq!(count, 1, "runtime_events table should exist");

        handle.shutdown().await;
    }

    /// tool_call_started -> tool_call_completed chain, correctly linked via
    /// `parent_event_id`.
    #[tokio::test]
    async fn end_to_end_tool_call_pair_links_via_parent_event_id() {
        // GIVEN a persistor and its subscriber connected to an EventBus
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");
        let (bus, _rx_keepalive) = EventBus::new();
        let _join = spawn_runtime_events_subscriber(handle.clone(), &bus);

        // WHEN an agent emits a tool_call_started followed by the completed
        // event sharing the same event_id as parent_event_id
        let started_id = uuid::Uuid::now_v7().to_string();
        bus.send(RuntimeEvent::ToolCallStarted {
            event_id: started_id.clone(),
            task_id: "task-pair".into(),
            agent_id: "agent-pair".into(),
            tool_name: "web_search".into(),
            args_json: Some("{\"query\":\"hello\"}".into()),
        })
        .expect("bus send started");

        bus.send(RuntimeEvent::ToolCallCompleted {
            parent_event_id: started_id.clone(),
            task_id: "task-pair".into(),
            agent_id: "agent-pair".into(),
            tool_name: "web_search".into(),
            output_json: Some("{\"results\":[{\"url\":\"https://example.com\"}]}".into()),
            exit_code: None,
            duration_ms: 412,
            success: true,
        })
        .expect("bus send completed");

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        handle.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN both events are persisted and chained via
        // parent_event_id == started.event_id
        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let rows = repo
            .list_for_task("task-pair", None, 10)
            .expect("list_for_task");
        assert_eq!(rows.len(), 2);

        let started = rows
            .iter()
            .find(|r| r.kind == "tool_call_started")
            .expect("started");
        let completed = rows
            .iter()
            .find(|r| r.kind == "tool_call_completed")
            .expect("completed");

        // The started event keeps its explicit event_id (generated by ToolProxy).
        assert_eq!(started.event_id, started_id);
        // The completed event points back to the started one via parent_event_id.
        assert_eq!(
            completed.parent_event_id.as_deref(),
            Some(started_id.as_str())
        );
        // The payload carries the output and the success flag.
        assert!(completed.payload_json.contains("\"success\":true"));
        assert!(completed.payload_json.contains("example.com"));
    }

    /// Thought, Retry and ActionParseError each persist their step_num.
    #[tokio::test]
    async fn end_to_end_thought_retry_parse_error_record_step_num() {
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");
        let (bus, _rx_keepalive) = EventBus::new();
        let _join = spawn_runtime_events_subscriber(handle.clone(), &bus);

        bus.send(RuntimeEvent::Thought {
            task_id: "T".into(),
            agent_id: "A".into(),
            step_num: 2,
            text: "I should call web_search.".into(),
        })
        .expect("send");
        bus.send(RuntimeEvent::ActionParseError {
            task_id: "T".into(),
            agent_id: "A".into(),
            step_num: 3,
            raw_content: "{not json".into(),
            repair_attempted: true,
        })
        .expect("send");
        bus.send(RuntimeEvent::Retry {
            task_id: "T".into(),
            agent_id: "A".into(),
            step_num: 3,
            cause: "action_parse_error".into(),
            attempt: 1,
        })
        .expect("send");

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        handle.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let rows = repo.list_for_task("T", None, 10).expect("list");

        assert_eq!(rows.len(), 3);
        let thought = rows.iter().find(|r| r.kind == "thought").expect("thought");
        assert_eq!(thought.step_num, Some(2));
        assert!(thought.payload_json.contains("web_search"));

        let parse_err = rows
            .iter()
            .find(|r| r.kind == "action_parse_error")
            .expect("parse");
        assert_eq!(parse_err.step_num, Some(3));
        assert!(parse_err.payload_json.contains("repair_attempted"));

        let retry = rows.iter().find(|r| r.kind == "retry").expect("retry");
        assert_eq!(retry.step_num, Some(3));
        assert!(retry.payload_json.contains("action_parse_error"));
    }

    /// End-to-end smoke test:
    /// RuntimeEvent::AgentLog published on the EventBus -> subscriber ->
    /// persistor -> repository.list_for_task retrieves the persisted record.
    #[tokio::test]
    async fn end_to_end_agent_log_flows_through_event_bus() {
        // GIVEN a persistor and its subscriber connected to an EventBus
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");
        let (bus, _rx_keepalive) = EventBus::new();
        let _join = spawn_runtime_events_subscriber(handle.clone(), &bus);

        // WHEN an agent emits ctx.log via RuntimeEvent::AgentLog
        bus.send(RuntimeEvent::AgentLog {
            task_id: "task-smoke".into(),
            agent_id: "agent-smoke".into(),
            level: "warn".into(),
            message: "veille-ia: research tool unavailable".into(),
            extra_fields_json: None,
        })
        .expect("bus send");

        // Give the subscriber and the persistence actor time to process.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        handle.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN the repository returns the event with the right kind and payload
        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let rows = repo
            .list_for_task("task-smoke", None, 10)
            .expect("list_for_task");
        assert_eq!(rows.len(), 1, "expected exactly one persisted event");
        assert_eq!(rows[0].kind, "agent_log");
        assert_eq!(rows[0].agent_id, "agent-smoke");
        assert!(rows[0].payload_json.contains("research tool unavailable"));
        assert!(rows[0].payload_json.contains("\"level\":\"warn\""));
    }

    #[tokio::test]
    async fn appends_and_persists_records() {
        // GIVEN a fresh persistor
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");

        // WHEN we append two records
        let now_unix = chrono::Utc::now().timestamp();
        let now_iso = chrono::Utc::now().to_rfc3339();
        for i in 0..2 {
            handle.append(RuntimeEventRecord {
                event_id: format!("01900000-0000-7000-8000-00000000000{i}"),
                task_id: "task-A".into(),
                agent_id: "agent-X".into(),
                parent_event_id: None,
                correlation_id: None,
                step_num: None,
                kind: "agent_log".into(),
                payload_json: format!("{{\"level\":\"info\",\"message\":\"msg {i}\"}}"),
                ts: now_iso.clone(),
                created_at_unix: now_unix,
            });
        }

        // Drain by shutting down, flushes the queue.
        handle.shutdown().await;
        // Give the actor a moment to drain (shutdown() returns once the
        // message is enqueued, but the actor still needs to consume it).
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN both rows are present.
        let conn = rusqlite::Connection::open(&db).expect("reopen");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM runtime_events WHERE task_id='task-A'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 2);
    }
}
