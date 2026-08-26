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

use apollia_core::events::subscribe_resilient;
use apollia_core::events::RuntimeEvent;
use apollia_core::ObservabilityConfig;
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

/// Current schema version of the runtime-events store (a single step).
const SCHEMA_VERSION: u32 = 1;

/// The ordered migration list applied through
/// [`apollia_core::schema::open_versioned`].
const MIGRATIONS: [apollia_core::schema::Migration; SCHEMA_VERSION as usize] = [migrate_v1];

fn migrate_v1(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(SCHEMA)
}

/// Errors raised while opening or initializing the persistor.
#[derive(Debug, Error)]
#[non_exhaustive]
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
    /// Delete every event older than `cutoff_unix`, and report how many went.
    Purge {
        /// Unix seconds. Rows strictly older than this are deleted.
        cutoff_unix: i64,
        /// Receives the number of deleted rows, or the SQLite error.
        reply: oneshot::Sender<Result<usize, String>>,
    },
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
                PersistorMessage::Purge { cutoff_unix, reply } => {
                    let outcome = Self::purge(&self.conn, cutoff_unix).map_err(|e| e.to_string());
                    let _ = reply.send(outcome);
                }
                PersistorMessage::Shutdown => break,
            }
        }
    }

    /// Delete every row strictly older than `cutoff_unix`.
    ///
    /// Scoped to `runtime_events` by construction: this actor owns the only
    /// connection to `runtime_events.db` and opens nothing else. The audit trail
    /// and the signed audit journal live in separate databases and are never
    /// reachable from here, which matters because the journal is a hash chain
    /// that `audit verify` walks: deleting a link would break verification for
    /// every entry after it.
    fn purge(conn: &rusqlite::Connection, cutoff_unix: i64) -> rusqlite::Result<usize> {
        conn.execute(
            "DELETE FROM runtime_events WHERE created_at_unix < ?1",
            rusqlite::params![cutoff_unix],
        )
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

            if let Err(e) = apollia_core::schema::open_versioned(
                &conn,
                apollia_core::paths::DataFile::RuntimeEvents.file_name(),
                SCHEMA_VERSION,
                &MIGRATIONS,
            ) {
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

    /// Apply the retention policy: delete events older than `retention_days`.
    ///
    /// `retention_days == 0` means **never purge** and returns `Ok(0)` without
    /// touching the database. Any other value deletes rows whose
    /// `created_at_unix` is strictly older than the cutoff, and logs the count
    /// at `INFO`: a deletion the operator did not watch happen should still be
    /// findable afterwards.
    ///
    /// Only `runtime_events.db` is affected. The audit trail and the signed
    /// audit journal are separate databases with their own lifecycle, and the
    /// journal in particular is a hash chain that `audit verify` walks end to
    /// end, so it is never purged on a timer.
    ///
    /// # Errors
    ///
    /// Returns the SQLite error as a string when the delete fails, or a message
    /// when the actor is gone.
    pub async fn purge_older_than(
        &self,
        retention_days: u32,
        now_unix: i64,
    ) -> Result<usize, String> {
        if retention_days == 0 {
            tracing::debug!("runtime_events.retention_disabled");
            return Ok(0);
        }

        let cutoff_unix = now_unix - i64::from(retention_days) * 86_400;
        let (reply, rx) = oneshot::channel();
        self.sender
            .send(PersistorMessage::Purge { cutoff_unix, reply })
            .await
            .map_err(|_| "runtime_events persistor disconnected".to_string())?;
        let deleted = rx
            .await
            .map_err(|_| "runtime_events persistor dropped the purge reply".to_string())??;

        tracing::info!(
            deleted,
            retention_days,
            cutoff_unix,
            "runtime_events.purged"
        );
        Ok(deleted)
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
    obs_config: ObservabilityConfig,
) -> tokio::task::JoinHandle<()> {
    let mut rx = subscribe_resilient(event_bus, "observability.runtime_events");
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Some(record) = event_to_record(event, &obs_config) {
                handle.append(record);
            }
        }
    })
}

/// Maps a `RuntimeEvent` to a persistable `RuntimeEventRecord`.
///
/// Returns `None` for variants that do not participate in the event log, and
/// for variants whose content the operator has opted out of capturing.
/// Each new persisted event type is a single extra arm in the `match`.
///
/// The capture switches of [`ObservabilityConfig`] are enforced here, at the
/// single point where a `RuntimeEvent` becomes a row of `runtime_events.db`.
/// Two shapes, matching what the setting means to an operator:
///
/// - an event whose whole reason to exist is the content (`Thought`,
///   `AgentLog`) is dropped entirely, so the timeline shows nothing;
/// - an event that also carries structural facts (`ToolCallStarted` and its
///   companion `ToolCallCompleted`) keeps its row, and only the content field
///   is left out. Dropping the row would break the `parent_event_id` chain that
///   links a call to its result.
fn event_to_record(event: RuntimeEvent, obs: &ObservabilityConfig) -> Option<RuntimeEventRecord> {
    let now_unix = chrono::Utc::now().timestamp();
    let now_iso = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    match event {
        RuntimeEvent::AgentLog { .. } if !obs.capture_agent_logs => None,
        RuntimeEvent::Thought { .. } if !obs.capture_thoughts => None,

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
            ..
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
            ..
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
                "args_json": obs.capture_tool_args.then_some(args_json).flatten(),
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
            ..
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
                "output_json": obs.capture_tool_outputs.then_some(output_json).flatten(),
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
    use crate::test_support::{poll_until, poll_until_async};
    use apollia_core::events::RuntimeEvent;
    use std::time::Duration;
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
        let join =
            spawn_runtime_events_subscriber(handle.clone(), &bus, ObservabilityConfig::default());

        // WHEN an agent emits a tool_call_started followed by the completed
        // event sharing the same event_id as parent_event_id
        let started_id = uuid::Uuid::now_v7().to_string();
        bus.send(RuntimeEvent::ToolCallStarted {
            event_id: started_id.clone(),
            task_id: "task-pair".into(),
            agent_id: "agent-pair".into(),
            tool_name: "web_search".into(),
            args_json: Some("{\"query\":\"hello\"}".into()),
            run_id: None,
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
            run_id: None,
        })
        .expect("bus send completed");

        // Close the bus so the subscriber drains every buffered event and
        // exits, then stop the persistor. The drain is awaited, not slept on.
        drop(bus);
        join.await.expect("subscriber exits cleanly");
        handle.shutdown().await;

        // THEN both events are persisted and chained via
        // parent_event_id == started.event_id
        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let persisted = poll_until_async(Duration::from_secs(5), || async {
            repo.list_for_task("task-pair", None, 10)
                .map(|r| r.len() == 2)
                .unwrap_or(false)
        })
        .await;
        assert!(persisted, "expected 2 persisted events");
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
        let join =
            spawn_runtime_events_subscriber(handle.clone(), &bus, ObservabilityConfig::default());

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

        drop(bus);
        join.await.expect("subscriber exits cleanly");
        handle.shutdown().await;

        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let persisted = poll_until_async(Duration::from_secs(5), || async {
            repo.list_for_task("T", None, 10)
                .map(|r| r.len() == 3)
                .unwrap_or(false)
        })
        .await;
        assert!(persisted, "expected 3 persisted events");
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
        let join =
            spawn_runtime_events_subscriber(handle.clone(), &bus, ObservabilityConfig::default());

        // WHEN an agent emits ctx.log via RuntimeEvent::AgentLog
        bus.send(RuntimeEvent::AgentLog {
            task_id: "task-smoke".into(),
            agent_id: "agent-smoke".into(),
            level: "warn".into(),
            message: "veille-ia: research tool unavailable".into(),
            extra_fields_json: None,
        })
        .expect("bus send");

        drop(bus);
        join.await.expect("subscriber exits cleanly");
        handle.shutdown().await;

        // THEN the repository returns the event with the right kind and payload
        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let persisted = poll_until_async(Duration::from_secs(5), || async {
            repo.list_for_task("task-smoke", None, 10)
                .map(|r| r.len() == 1)
                .unwrap_or(false)
        })
        .await;
        assert!(persisted, "expected exactly one persisted event");
        let rows = repo
            .list_for_task("task-smoke", None, 10)
            .expect("list_for_task");
        assert_eq!(rows.len(), 1, "expected exactly one persisted event");
        assert_eq!(rows[0].kind, "agent_log");
        assert_eq!(rows[0].agent_id, "agent-smoke");
        assert!(rows[0].payload_json.contains("research tool unavailable"));
        assert!(rows[0].payload_json.contains("\"level\":\"warn\""));
    }

    // ── Capture switches ──────────────────────────────────────────────────
    //
    // One test per switch of `[observability]`. Each proves the same thing:
    // with the switch off, the content does not reach `runtime_events.db`.
    // Before these tests the five switches had no reader at all, so turning
    // one off changed nothing while the settings page said otherwise.

    /// Drives events through the bus with the given config and returns the rows
    /// persisted for `task_id`.
    ///
    /// Asserting that a switch suppressed something means asserting a negative,
    /// which a timeout cannot do: "not there yet" and "never written" look the
    /// same. So a sentinel `Retry` event, which no switch gates, is emitted last
    /// and the read waits for it. Bus, subscriber and actor channel are all
    /// FIFO, so once the sentinel is on disk every earlier event has been
    /// processed, and what is missing is missing by decision.
    async fn rows_for(
        obs: ObservabilityConfig,
        task_id: &str,
        events: Vec<RuntimeEvent>,
    ) -> Vec<RuntimeEventRecord> {
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");
        let (bus, _rx_keepalive) = EventBus::new();
        let join = spawn_runtime_events_subscriber(handle.clone(), &bus, obs);
        for event in events {
            bus.send(event).expect("bus send");
        }
        bus.send(RuntimeEvent::Retry {
            task_id: task_id.into(),
            agent_id: "agent-cap".into(),
            step_num: 99,
            cause: "capture-test-sentinel".into(),
            attempt: 1,
        })
        .expect("bus send sentinel");
        drop(bus);
        join.await.expect("subscriber exits cleanly");
        handle.shutdown().await;

        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let landed = poll_until_async(Duration::from_secs(10), || async {
            repo.list_for_task(task_id, None, 50)
                .map(|r| r.iter().any(|row| row.kind == "retry"))
                .unwrap_or(false)
        })
        .await;
        assert!(
            landed,
            "sentinel never landed, the harness itself is broken"
        );

        repo.list_for_task(task_id, None, 50)
            .expect("list_for_task")
            .into_iter()
            .filter(|r| r.kind != "retry")
            .collect()
    }

    fn agent_log(task_id: &str) -> RuntimeEvent {
        RuntimeEvent::AgentLog {
            task_id: task_id.into(),
            agent_id: "agent-cap".into(),
            level: "info".into(),
            message: "SECRET_LOG_LINE".into(),
            extra_fields_json: None,
        }
    }

    fn thought(task_id: &str) -> RuntimeEvent {
        RuntimeEvent::Thought {
            task_id: task_id.into(),
            agent_id: "agent-cap".into(),
            step_num: 1,
            text: "SECRET_THOUGHT".into(),
        }
    }

    fn tool_pair(task_id: &str) -> Vec<RuntimeEvent> {
        let started_id = uuid::Uuid::now_v7().to_string();
        vec![
            RuntimeEvent::ToolCallStarted {
                event_id: started_id.clone(),
                task_id: task_id.into(),
                agent_id: "agent-cap".into(),
                tool_name: "file_read".into(),
                args_json: Some("{\"path\":\"/SECRET_ARG\"}".into()),
                run_id: None,
            },
            RuntimeEvent::ToolCallCompleted {
                parent_event_id: started_id,
                task_id: task_id.into(),
                agent_id: "agent-cap".into(),
                tool_name: "file_read".into(),
                output_json: Some("{\"content\":\"SECRET_OUTPUT\"}".into()),
                exit_code: Some(0),
                duration_ms: 5,
                success: true,
                run_id: None,
            },
        ]
    }

    #[tokio::test]
    async fn capture_agent_logs_false_persists_no_row() {
        // GIVEN capture_agent_logs disabled
        let obs = ObservabilityConfig {
            capture_agent_logs: false,
            ..ObservabilityConfig::default()
        };

        // WHEN an agent emits ctx.log
        let rows = rows_for(
            obs,
            "task-cap-logs-off",
            vec![agent_log("task-cap-logs-off")],
        )
        .await;

        // THEN nothing reaches runtime_events.db
        assert!(rows.is_empty(), "no agent_log row should be persisted");
    }

    #[tokio::test]
    async fn capture_agent_logs_true_persists_the_message() {
        // GIVEN the default (enabled)
        // WHEN the same event is emitted
        let rows = rows_for(
            ObservabilityConfig::default(),
            "task-cap-logs-on",
            vec![agent_log("task-cap-logs-on")],
        )
        .await;

        // THEN the row is there, so the test above proves the switch, not a
        // broken harness
        assert_eq!(rows.len(), 1);
        assert!(rows[0].payload_json.contains("SECRET_LOG_LINE"));
    }

    #[tokio::test]
    async fn capture_thoughts_false_persists_no_row() {
        // GIVEN capture_thoughts disabled
        let obs = ObservabilityConfig {
            capture_thoughts: false,
            ..ObservabilityConfig::default()
        };

        // WHEN the agent emits a ReAct thought
        let rows = rows_for(obs, "task-cap-th-off", vec![thought("task-cap-th-off")]).await;

        // THEN nothing reaches runtime_events.db
        assert!(rows.is_empty(), "no thought row should be persisted");
    }

    #[tokio::test]
    async fn capture_thoughts_true_persists_the_text() {
        let rows = rows_for(
            ObservabilityConfig::default(),
            "task-cap-th-on",
            vec![thought("task-cap-th-on")],
        )
        .await;
        assert_eq!(rows.len(), 1);
        assert!(rows[0].payload_json.contains("SECRET_THOUGHT"));
    }

    #[tokio::test]
    async fn capture_tool_args_false_drops_the_args_but_keeps_the_row() {
        // GIVEN capture_tool_args disabled
        let obs = ObservabilityConfig {
            capture_tool_args: false,
            ..ObservabilityConfig::default()
        };

        // WHEN a tool call runs
        let rows = rows_for(obs, "task-cap-args-off", tool_pair("task-cap-args-off")).await;

        // THEN the call is still traced, but the argument content is gone
        let started = rows
            .iter()
            .find(|r| r.kind == "tool_call_started")
            .expect("the tool call itself stays traced");
        assert!(
            !started.payload_json.contains("SECRET_ARG"),
            "args content must not reach the database: {}",
            started.payload_json
        );
        assert!(
            started.payload_json.contains("file_read"),
            "the structural fact stays: {}",
            started.payload_json
        );
    }

    #[tokio::test]
    async fn capture_tool_args_true_persists_the_args() {
        let rows = rows_for(
            ObservabilityConfig::default(),
            "task-cap-args-on",
            tool_pair("task-cap-args-on"),
        )
        .await;
        let started = rows
            .iter()
            .find(|r| r.kind == "tool_call_started")
            .expect("started row");
        assert!(started.payload_json.contains("SECRET_ARG"));
    }

    #[tokio::test]
    async fn capture_tool_outputs_false_drops_the_output_but_keeps_the_chain() {
        // GIVEN capture_tool_outputs disabled
        let obs = ObservabilityConfig {
            capture_tool_outputs: false,
            ..ObservabilityConfig::default()
        };

        // WHEN a tool call completes
        let rows = rows_for(obs, "task-cap-out-off", tool_pair("task-cap-out-off")).await;

        // THEN the output content is gone, and the parent link survives so the
        // timeline still pairs the call with its result
        let completed = rows
            .iter()
            .find(|r| r.kind == "tool_call_completed")
            .expect("completed row stays");
        assert!(
            !completed.payload_json.contains("SECRET_OUTPUT"),
            "output content must not reach the database: {}",
            completed.payload_json
        );
        assert!(
            completed.parent_event_id.is_some(),
            "dropping content must not break the parent_event_id chain"
        );
    }

    #[tokio::test]
    async fn capture_tool_outputs_true_persists_the_output() {
        let rows = rows_for(
            ObservabilityConfig::default(),
            "task-cap-out-on",
            tool_pair("task-cap-out-on"),
        )
        .await;
        let completed = rows
            .iter()
            .find(|r| r.kind == "tool_call_completed")
            .expect("completed row");
        assert!(completed.payload_json.contains("SECRET_OUTPUT"));
    }

    // ── Retention purge ───────────────────────────────────────────────────

    /// Insert one row at `created_at_unix` directly through the handle.
    async fn append_at(handle: &EventPersistorHandle, task_id: &str, id: &str, at_unix: i64) {
        handle.append(RuntimeEventRecord {
            event_id: id.to_string(),
            task_id: task_id.to_string(),
            agent_id: "agent-purge".into(),
            parent_event_id: None,
            correlation_id: None,
            step_num: None,
            kind: "agent_log".into(),
            payload_json: "{}".into(),
            ts: "2026-01-01T00:00:00.000Z".into(),
            created_at_unix: at_unix,
        });
    }

    async fn count_rows(db: &std::path::Path) -> i64 {
        let conn = rusqlite::Connection::open(db).expect("reopen");
        conn.query_row("SELECT COUNT(*) FROM runtime_events", [], |r| r.get(0))
            .unwrap_or(-1)
    }

    #[tokio::test]
    async fn purge_deletes_only_events_older_than_the_cutoff() {
        // GIVEN one event well past the retention window and one inside it
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");
        let now = 1_800_000_000i64;
        append_at(
            &handle,
            "task-purge",
            "01900000-0000-7000-8000-00000000aa01",
            now - 40 * 86_400,
        )
        .await;
        append_at(
            &handle,
            "task-purge",
            "01900000-0000-7000-8000-00000000aa02",
            now - 2 * 86_400,
        )
        .await;
        let landed = poll_until_async(Duration::from_secs(10), || async {
            count_rows(&db).await == 2
        })
        .await;
        assert!(landed, "both rows should be in before purging");

        // WHEN a 30-day retention is applied
        let deleted = handle
            .purge_older_than(30, now)
            .await
            .expect("purge should succeed");

        // THEN only the old one is gone
        assert_eq!(deleted, 1);
        assert_eq!(count_rows(&db).await, 1);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn retention_days_zero_never_purges() {
        // GIVEN an event far older than any plausible window
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");
        let now = 1_800_000_000i64;
        append_at(
            &handle,
            "task-keep",
            "01900000-0000-7000-8000-00000000bb01",
            0,
        )
        .await;
        let landed = poll_until_async(Duration::from_secs(10), || async {
            count_rows(&db).await == 1
        })
        .await;
        assert!(landed);

        // WHEN retention is 0, which means never purge
        let deleted = handle.purge_older_than(0, now).await.expect("no-op");

        // THEN nothing is deleted. 0 must not be read as "keep zero days".
        assert_eq!(deleted, 0);
        assert_eq!(count_rows(&db).await, 1);
        handle.shutdown().await;
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

        // shutdown() enqueues the stop after the two appends (FIFO) but returns
        // before the actor consumes them, so poll the read until both rows land.
        handle.shutdown().await;

        // THEN both rows are present.
        let conn = rusqlite::Connection::open(&db).expect("reopen");
        let count_task_a = || {
            conn.query_row(
                "SELECT COUNT(*) FROM runtime_events WHERE task_id='task-A'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
        };
        let persisted = poll_until(Duration::from_secs(5), || count_task_a() == 2).await;
        assert!(persisted, "expected 2 persisted rows");
        assert_eq!(count_task_a(), 2);
    }

    /// runtime_events.db as the first shipped binary wrote it, with one row.
    const RUNTIME_EVENTS_V1_FIXTURE: &str =
        include_str!("../../tests/fixtures/schemas/runtime_events_v1.sql");

    #[tokio::test]
    async fn test_open_legacy_v1_database_keeps_rows_and_stamps_version() {
        // GIVEN a runtime_events.db written before the versioned layer
        // (schema v1, user_version 0, one persisted event)
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let seed = rusqlite::Connection::open(&db).expect("open raw");
        seed.execute_batch(RUNTIME_EVENTS_V1_FIXTURE)
            .expect("seed v1");
        drop(seed);

        // WHEN opening it through the persistor (versioned migration)
        let handle = EventPersistorHandle::open(&db)
            .await
            .expect("open migrated");
        handle.shutdown().await;

        // THEN the legacy row survives and the file is stamped
        let conn = rusqlite::Connection::open(&db).expect("reopen");
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM runtime_events", [], |r| r.get(0))
            .expect("count");
        assert_eq!(rows, 1);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("user_version");
        assert_eq!(version, i64::from(SCHEMA_VERSION));
    }

    #[tokio::test]
    async fn test_open_newer_database_is_refused() {
        // GIVEN a runtime_events.db stamped one version above this binary
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let seed = rusqlite::Connection::open(&db).expect("open raw");
        seed.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .expect("stamp");
        drop(seed);

        // WHEN opening it through the persistor
        let result = EventPersistorHandle::open(&db).await;

        // THEN the open is refused instead of misreading the newer schema
        assert!(
            matches!(result, Err(EventPersistorError::SchemaInitFailed(ref m)) if m.contains("newer")),
            "expected a schema refusal"
        );
    }
}
