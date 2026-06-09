//! Clonable handle to the [`TodoActor`](super::todo_actor::TodoActor).
//!
//! The handle is the only way to read or write session todo state. It speaks to
//! the actor over a bounded `mpsc` channel, so the actor serializes every access
//! and upholds the at-most-one-`InProgress` invariant without a shared lock.

use apollia_core::todo::{TodoError, TodoItem};
use tokio::sync::{mpsc, oneshot};

/// Internal protocol between [`TodoHandle`] and the todo actor.
pub(crate) enum TodoMessage {
    /// Replace the full todo list for a session. The invariant is checked
    /// before any write; the reply carries the error when it is violated.
    SetItems {
        /// Session whose list is being replaced.
        session_id: String,
        /// Full replacement list.
        items: Vec<TodoItem>,
        /// Reply channel for the write outcome.
        reply: oneshot::Sender<Result<(), TodoError>>,
    },
    /// Return the current todo list for a session, ordered by insertion.
    GetItems {
        /// Session to read.
        session_id: String,
        /// Reply channel for the read outcome.
        reply: oneshot::Sender<Result<Vec<TodoItem>, TodoError>>,
    },
    /// Stop the actor loop.
    Shutdown,
}

/// Clonable handle to the todo actor.
///
/// All methods are async and communicate via a bounded `mpsc` channel. Cloning
/// is cheap (it clones the sender); there is no `Arc<Mutex>`.
#[derive(Clone)]
pub struct TodoHandle {
    pub(crate) tx: mpsc::Sender<TodoMessage>,
}

impl TodoHandle {
    /// Replace the full todo list for `session_id`.
    ///
    /// The list is validated and persisted atomically: on success the prior
    /// list for the session is fully replaced; on error the database is left
    /// unchanged.
    ///
    /// # Errors
    ///
    /// - [`TodoError::MultipleInProgress`] when more than one item has
    ///   `status == InProgress`.
    /// - [`TodoError::Sqlite`] when the underlying write fails.
    /// - [`TodoError::ActorGone`] when the actor has stopped.
    pub async fn set_items(
        &self,
        session_id: &str,
        items: Vec<TodoItem>,
    ) -> Result<(), TodoError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(TodoMessage::SetItems {
                session_id: session_id.to_string(),
                items,
                reply,
            })
            .await
            .map_err(|_| TodoError::ActorGone)?;
        rx.await.map_err(|_| TodoError::ActorGone)?
    }

    /// Return the current todo list for `session_id`, ordered by insertion.
    ///
    /// Returns an empty `Vec` when the session has no items yet.
    ///
    /// # Errors
    ///
    /// - [`TodoError::Sqlite`] when the underlying read fails.
    /// - [`TodoError::ActorGone`] when the actor has stopped.
    pub async fn get_items(&self, session_id: &str) -> Result<Vec<TodoItem>, TodoError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(TodoMessage::GetItems {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| TodoError::ActorGone)?;
        rx.await.map_err(|_| TodoError::ActorGone)?
    }

    /// Signal the actor to stop. Best-effort; ignores a closed channel.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(TodoMessage::Shutdown).await;
    }
}
