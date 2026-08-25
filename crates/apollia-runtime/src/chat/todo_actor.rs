//! Tokio actor owning the per-session todo store.
//!
//! The actor is the single owner of the `session_todos` SQLite table. It
//! serializes every read and write over a bounded `mpsc` channel and enforces
//! the at-most-one-`InProgress` invariant before any write. Callers interact
//! through a clonable [`TodoHandle`].

use apollia_core::todo::{count_in_progress, TodoError, TodoItem, TodoStatus};
use apollia_core::RuntimeEvent;
use rusqlite::{params, Connection};
use tokio::sync::mpsc;
use tracing::warn;

use super::todo_handle::{TodoHandle, TodoMessage};
use crate::eventbus::EventBusSender;

/// Bounded capacity of the todo actor's command channel.
///
/// Todo writes are infrequent (one agent turn at a time per session), so a
/// small buffer absorbs bursts without unbounded growth.
const TODO_CHANNEL_CAPACITY: usize = 64;

/// Actor owning the SQLite connection backing the todo store.
///
/// Created via [`spawn_todo_actor`]; never constructed directly by callers.
pub struct TodoActor {
    conn: Connection,
    /// Optional event bus for emitting [`RuntimeEvent::TodoUpdated`] after a
    /// successful write. `None` in tests that do not assert on events.
    event_bus: Option<EventBusSender>,
}

impl TodoActor {
    /// Replace the full todo list for a session inside a single transaction.
    ///
    /// Validates the at-most-one-`InProgress` invariant before touching the
    /// database; on violation the table is left unchanged.
    fn set_items(&mut self, session_id: &str, items: &[TodoItem]) -> Result<(), TodoError> {
        let in_progress = count_in_progress(items);
        if in_progress > 1 {
            return Err(TodoError::MultipleInProgress { count: in_progress });
        }

        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM session_todos WHERE session_id = ?1",
            params![session_id],
        )?;
        for item in items {
            let depends_on =
                serde_json::to_string(&item.depends_on).unwrap_or_else(|_| "[]".to_string());
            tx.execute(
                "INSERT INTO session_todos (id, session_id, content, status, depends_on)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    item.id,
                    session_id,
                    item.content,
                    item.status.as_str(),
                    depends_on
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Return the todo list for a session, ordered by insertion (rowid).
    fn get_items(&self, session_id: &str) -> Result<Vec<TodoItem>, TodoError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, status, depends_on FROM session_todos
             WHERE session_id = ?1 ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let status_raw: String = row.get(2)?;
            let depends_raw: String = row.get(3)?;
            // The CHECK constraint guarantees a valid status string on write;
            // an unexpected value degrades to Pending rather than failing a read.
            let status = TodoStatus::parse(&status_raw).unwrap_or(TodoStatus::Pending);
            let depends_on: Vec<String> = serde_json::from_str(&depends_raw).unwrap_or_default();
            Ok(TodoItem {
                id,
                content,
                status,
                depends_on,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    /// Drive the actor until the channel closes or `Shutdown` is received.
    async fn run(mut self, mut rx: mpsc::Receiver<TodoMessage>) {
        while let Some(msg) = rx.recv().await {
            match msg {
                TodoMessage::SetItems {
                    session_id,
                    items,
                    reply,
                } => {
                    let result = self.set_items(&session_id, &items);
                    // Emit the snapshot only after a successful write, so a
                    // rejected payload (invariant violation) produces no event.
                    if result.is_ok() {
                        if let Some(bus) = &self.event_bus {
                            let _ = bus.send(RuntimeEvent::TodoUpdated {
                                session_id: session_id.clone(),
                                items: items.clone(),
                            });
                        }
                    }
                    let _ = reply.send(result);
                }
                TodoMessage::GetItems { session_id, reply } => {
                    let result = self.get_items(&session_id);
                    let _ = reply.send(result);
                }
                TodoMessage::Shutdown => break,
            }
        }
        warn!("todo.actor.stopped");
    }
}

/// Apply the migration on `conn`, spawn the [`TodoActor`], and return its handle.
///
/// The migration runs synchronously before the actor task is spawned, so a
/// schema failure surfaces at startup (principle: fail fast) instead of on the
/// first write.
///
/// `event_bus` receives a [`RuntimeEvent::TodoUpdated`] snapshot after every
/// successful write; pass `None` to disable event emission.
///
/// # Errors
///
/// - [`apollia_core::schema::SchemaError`] when the chat schema cannot be
///   brought to the current version (the `session_todos` table lives in the
///   chat database, whose whole migration list is applied here, fail fast).
pub fn spawn_todo_actor(
    conn: Connection,
    event_bus: Option<EventBusSender>,
) -> Result<TodoHandle, apollia_core::schema::SchemaError> {
    super::schema::migrate(&conn)?;
    let (tx, rx) = mpsc::channel(TODO_CHANNEL_CAPACITY);
    let actor = TodoActor { conn, event_bus };
    tokio::spawn(actor.run(rx));
    Ok(TodoHandle { tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> TodoHandle {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        spawn_todo_actor(conn, None).expect("spawn actor")
    }

    fn item(id: &str, content: &str, status: TodoStatus) -> TodoItem {
        TodoItem {
            id: id.into(),
            content: content.into(),
            status,
            depends_on: vec![],
        }
    }

    #[tokio::test]
    async fn test_set_and_get_single_pending_item() {
        // GIVEN a handle backed by an in-memory db
        let h = handle();

        // WHEN persisting one pending item
        h.set_items(
            "s1",
            vec![item("t1", "Analyser le fichier", TodoStatus::Pending)],
        )
        .await
        .expect("set ok");

        // THEN get_items returns the same item with the same status
        let got = h.get_items("s1").await.expect("get ok");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "t1");
        assert_eq!(got[0].content, "Analyser le fichier");
        assert_eq!(got[0].status, TodoStatus::Pending);
    }

    #[tokio::test]
    async fn test_multiple_in_progress_rejected() {
        // GIVEN a session with one in_progress item already persisted
        let h = handle();
        h.set_items("s1", vec![item("t1", "first", TodoStatus::InProgress)])
            .await
            .expect("seed ok");

        // WHEN setting a list with two in_progress items
        let result = h
            .set_items(
                "s1",
                vec![
                    item("t1", "first", TodoStatus::InProgress),
                    item("t2", "second", TodoStatus::InProgress),
                ],
            )
            .await;

        // THEN the invariant error is returned
        assert!(matches!(
            result,
            Err(TodoError::MultipleInProgress { count: 2 })
        ));

        // AND the prior list is unchanged
        let got = h.get_items("s1").await.expect("get ok");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "t1");
        assert_eq!(got[0].status, TodoStatus::InProgress);
    }

    #[tokio::test]
    async fn test_transition_pending_to_in_progress() {
        // GIVEN a pending item persisted
        let h = handle();
        h.set_items("s1", vec![item("t1", "work", TodoStatus::Pending)])
            .await
            .expect("seed ok");

        // WHEN the same item is set to in_progress with no other in_progress
        h.set_items("s1", vec![item("t1", "work", TodoStatus::InProgress)])
            .await
            .expect("transition ok");

        // THEN get_items reports in_progress without error
        let got = h.get_items("s1").await.expect("get ok");
        assert_eq!(got[0].status, TodoStatus::InProgress);
    }

    #[tokio::test]
    async fn test_single_in_progress_accepted() {
        // GIVEN a list with one in_progress and two pending items
        let h = handle();

        // WHEN persisting the list
        h.set_items(
            "s1",
            vec![
                item("t1", "a", TodoStatus::InProgress),
                item("t2", "b", TodoStatus::Pending),
                item("t3", "c", TodoStatus::Pending),
            ],
        )
        .await
        .expect("set ok");

        // THEN all three items are stored in insertion order
        let got = h.get_items("s1").await.expect("get ok");
        let ids: Vec<&str> = got.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["t1", "t2", "t3"]);
    }

    #[tokio::test]
    async fn test_session_isolation() {
        // GIVEN items persisted only in session s1
        let h = handle();
        h.set_items("s1", vec![item("t1", "a", TodoStatus::InProgress)])
            .await
            .expect("set ok");

        // WHEN reading another session s2
        let got = h.get_items("s2").await.expect("get ok");

        // THEN the list is empty (no cross-session leak)
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn test_get_items_after_actor_shutdown_is_actor_gone() {
        // GIVEN a handle whose actor has been told to stop
        let h = handle();
        h.shutdown().await;
        // Give the actor a moment to drain the shutdown message.
        tokio::task::yield_now().await;

        // WHEN reading after the channel is closed
        let result = h.get_items("s1").await;

        // THEN the call reports the actor is gone (graceful, no panic)
        assert!(matches!(result, Err(TodoError::ActorGone)));
    }

    #[tokio::test]
    async fn test_todo_updated_event_emitted_on_success() {
        // GIVEN an actor wired to an event bus
        let (bus, mut rx) = tokio::sync::broadcast::channel(16);
        let h = spawn_todo_actor(Connection::open_in_memory().expect("open"), Some(bus))
            .expect("spawn");

        // WHEN one valid item is persisted
        h.set_items("s1", vec![item("t1", "work", TodoStatus::InProgress)])
            .await
            .expect("set ok");

        // THEN a TodoUpdated event carries the same session and items
        let event = rx.recv().await.expect("event received");
        assert!(matches!(
            &event,
            RuntimeEvent::TodoUpdated { session_id, items }
                if session_id == "s1" && items.len() == 1 && items[0].id == "t1"
        ));
    }

    #[tokio::test]
    async fn test_no_event_on_invariant_violation() {
        // GIVEN an actor wired to an event bus
        let (bus, mut rx) = tokio::sync::broadcast::channel(16);
        let h = spawn_todo_actor(Connection::open_in_memory().expect("open"), Some(bus))
            .expect("spawn");

        // WHEN a write violating the invariant is rejected
        let result = h
            .set_items(
                "s1",
                vec![
                    item("t1", "a", TodoStatus::InProgress),
                    item("t2", "b", TodoStatus::InProgress),
                ],
            )
            .await;
        assert!(matches!(
            result,
            Err(TodoError::MultipleInProgress { count: 2 })
        ));

        // THEN no event is emitted for the rejected write
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn test_depends_on_round_trips() {
        // GIVEN an item with dependencies
        let h = handle();
        let mut it = item("t2", "depends", TodoStatus::Pending);
        it.depends_on = vec!["t1".into()];

        // WHEN persisting and reading it back
        h.set_items("s1", vec![it]).await.expect("set ok");
        let got = h.get_items("s1").await.expect("get ok");

        // THEN depends_on is preserved verbatim
        assert_eq!(got[0].depends_on, vec!["t1".to_string()]);
    }

    #[tokio::test]
    async fn test_spawn_todo_actor_refuses_newer_database() {
        // GIVEN a chat database stamped one version above this binary
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.pragma_update(
            None,
            "user_version",
            super::super::schema::SCHEMA_VERSION + 1,
        )
        .expect("stamp");

        // WHEN spawning the todo actor on it
        let result = spawn_todo_actor(conn, None);

        // THEN the spawn is refused instead of misreading the newer schema
        assert!(result.is_err());
    }
}
