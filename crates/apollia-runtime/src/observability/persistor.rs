//! `EventPersistor`, actor SQLite append-only pour `runtime_events.db`.
//!
//! Pattern copié de `apollia_tools::audit::AuditTrailHandle` :
//! - `tokio::sync::mpsc` borné en entrée, `tokio::task::spawn_blocking`
//!   dédié à la connexion `rusqlite` (non-`Sync`),
//! - écritures fire-and-forget pour ne jamais bloquer le thread d'agent,
//! - schéma versionné via `migrations/006_runtime_events.sql` chargé au
//!   démarrage (idempotent).
//!
//! En Lot 1 le persistor accepte un seul kind d'événement (`AgentLog`).
//! Les Lots suivants ajouteront thoughts, tool_call_*, llm_call_*, etc.

use std::path::{Path, PathBuf};

use apollia_core::events::RuntimeEvent;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use super::repository::RuntimeEventRecord;
use crate::eventbus::EventBusSender;

const CHANNEL_CAPACITY: usize = 1024;

/// Schéma SQL inline, miroir de `migrations/006_runtime_events.sql`.
///
/// Inclus en `include_str!` aurait nécessité de réorganiser le crate
/// `apollia-tools` ; on duplique le schéma ici de façon contrôlée jusqu'à
/// la consolidation (Lot 5).
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

/// Erreurs d'ouverture / initialisation du persistor.
#[derive(Debug, Error)]
pub enum EventPersistorError {
    /// Échec d'ouverture du fichier SQLite.
    #[error("failed to open runtime_events database at {path}: {source}")]
    OpenFailed {
        /// Chemin tenté.
        path: PathBuf,
        /// Cause sous-jacente.
        source: rusqlite::Error,
    },
    /// Échec de création du schéma (table, index, trigger).
    #[error("failed to initialize runtime_events schema: {0}")]
    SchemaInitFailed(String),
    /// Le canal d'initialisation s'est fermé prématurément (thread crashé).
    #[error("event persistor init channel disconnected")]
    InitDisconnected,
}

/// Messages internes envoyés à l'acteur.
enum PersistorMessage {
    /// Append fire-and-forget d'un événement.
    Append(Box<RuntimeEventRecord>),
    /// Arrêt propre de l'acteur après vidage de la file.
    Shutdown,
}

/// Acteur interne, détient la `Connection` SQLite en exclusivité.
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

/// Handle clonable vers l'acteur.
///
/// Toutes les méthodes sont thread-safe ; plusieurs handles peuvent émettre
/// vers le même acteur. Les écritures sont fire-and-forget : si le canal est
/// saturé, l'enregistrement est abandonné avec un `warn!` (backpressure).
#[derive(Clone)]
pub struct EventPersistorHandle {
    sender: mpsc::Sender<PersistorMessage>,
}

impl EventPersistorHandle {
    /// Ouvre `db_path`, applique le schéma idempotent, et démarre l'acteur
    /// en arrière-plan. Mode WAL activé pour la concurrence lecture/écriture
    /// (lectures via `RuntimeEventsRepository` parallèlement aux insertions).
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

    /// Append fire-and-forget d'un événement.
    ///
    /// Si le canal est saturé (>1024 events en attente), l'enregistrement est
    /// abandonné avec un `warn!` plutôt que de bloquer le thread d'agent.
    /// L'événement spécial `bus_lagged` du Lot 4 surfacera la perte côté UI.
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

    /// Arrête l'acteur après vidage de la file.
    pub async fn shutdown(&self) {
        let _ = self.sender.send(PersistorMessage::Shutdown).await;
    }
}

/// Spawn un subscriber EventBus → `EventPersistor`.
///
/// Filtre les `RuntimeEvent` mappables sur un `EventKind` persistable et
/// route le payload sérialisé vers le persistor. En Lot 1 seul `AgentLog`
/// est mappé ; les Lots 2+ ajouteront thoughts, tool_call_*, llm_call_*…
///
/// Le `JoinHandle` retourné peut être attendu pour vérifier la sortie
/// propre (le subscriber s'arrête quand l'`EventBus` est fermé).
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

/// Mappe un `RuntimeEvent` vers un `RuntimeEventRecord` persistable.
///
/// Retourne `None` pour les variantes qui ne participent pas (encore) au
/// log d'événements. Le mapping s'élargit au fil des Lots, chaque ajout
/// est une simple branche supplémentaire dans le `match`.
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

        // ── Lot 2 ────────────────────────────────────────────────
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
        // Mapping du LlmCallFailed *existant* (pré-Lot 2), porte une
        // ErrorAnalysis structurée plus riche que les champs ad hoc qu'on
        // aurait redéfinis. Le payload conserve l'analyse complète pour
        // que l'UI puisse afficher `category`, `severity`, etc.
        RuntimeEvent::LlmCallFailed {
            backend,
            model,
            task_id,
            step_id,
            error,
            analysis,
        } => {
            // Sans task_id on ne sait pas où raccrocher l'événement -
            // ignorer (perdu vs `tracing::*` qui aura logué l'erreur).
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
            // L'event_id vient du producteur pour permettre au companion
            // ToolCallCompleted de le réutiliser comme parent_event_id.
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
            // L'agent_id du completed est celui du caller, il est
            // déduit du started via parent_event_id côté UI.
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

        // Variantes hors observability, ignorées.
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

    /// Lot 2, chaîne tool_call_started → tool_call_completed avec
    /// `parent_event_id` correctement reliés.
    #[tokio::test]
    async fn end_to_end_tool_call_pair_links_via_parent_event_id() {
        // GIVEN un persistor + son subscriber connecté à un EventBus
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");
        let (bus, _rx_keepalive) = EventBus::new();
        let _join = spawn_runtime_events_subscriber(handle.clone(), &bus);

        // WHEN un agent émet un tool_call_started suivi du completed
        // partageant le même event_id en parent_event_id
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

        // THEN les deux événements sont persistés et chaînés via
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

        // Le started conserve son event_id explicite (générateur côté ToolProxy).
        assert_eq!(started.event_id, started_id);
        // Le completed pointe via parent_event_id vers le started.
        assert_eq!(
            completed.parent_event_id.as_deref(),
            Some(started_id.as_str())
        );
        // Le payload contient l'output et le succès.
        assert!(completed.payload_json.contains("\"success\":true"));
        assert!(completed.payload_json.contains("example.com"));
    }

    /// Lot 2, Thought, Retry et ActionParseError persistent leur step_num.
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

    /// Smoke test bout-en-bout (Lot 1) :
    /// RuntimeEvent::AgentLog publié sur l'EventBus → subscriber → persistor
    /// → repository.list_for_task récupère le record persisté.
    #[tokio::test]
    async fn end_to_end_agent_log_flows_through_event_bus() {
        // GIVEN un persistor + son subscriber connecté à un EventBus
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("runtime_events.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");
        let (bus, _rx_keepalive) = EventBus::new();
        let _join = spawn_runtime_events_subscriber(handle.clone(), &bus);

        // WHEN un agent émet ctx.log via RuntimeEvent::AgentLog
        bus.send(RuntimeEvent::AgentLog {
            task_id: "task-smoke".into(),
            agent_id: "agent-smoke".into(),
            level: "warn".into(),
            message: "veille-ia: research tool unavailable".into(),
            extra_fields_json: None,
        })
        .expect("bus send");

        // Laisser le subscriber + l'acteur de persistance traiter.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        handle.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN le repository renvoie l'événement avec le bon kind et payload
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
