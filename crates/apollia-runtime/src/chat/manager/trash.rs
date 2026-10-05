//! The trash of the conversations, as the actor serves it: send a
//! conversation there, list what it holds, take one back out.
//!
//! A conversation in the trash is out of the actor's memory: its exchange in
//! flight is cancelled the way a deletion cancels it, and it is neither
//! listed nor resumed until it is restored.

use super::*;
use crate::chat::repository::TrashedSession;

impl ChatSessionManager {
    /// Send a session to the trash and drop what the actor holds of it.
    pub(in crate::chat::manager) fn handle_trash_session(
        &mut self,
        session_id: &str,
    ) -> Result<(), ChatError> {
        self.repository.trash_session(session_id, &now_rfc3339())?;

        self.purge_session_runtime_state(session_id);
        self.sessions.remove(session_id);

        let _ = self.event_bus.send(RuntimeEvent::ChatSessionClosed {
            session_id: session_id.to_string(),
        });

        info!(session_id = %session_id, "chat.session.trashed");
        Ok(())
    }

    /// Take a session out of the trash and load it back the way a resume
    /// does, so the next message reaches it.
    pub(in crate::chat::manager) fn handle_restore_session(
        &mut self,
        session_id: &str,
    ) -> Result<SessionDetail, ChatError> {
        self.repository.restore_session(session_id)?;
        let detail = self.handle_resume_session(session_id)?;
        info!(session_id = %session_id, "chat.session.restored");
        Ok(detail)
    }

    /// The sessions in the trash, the last removed first.
    pub(in crate::chat::manager) fn handle_list_trashed_sessions(
        &self,
    ) -> Result<Vec<TrashedSession>, ChatError> {
        self.repository.list_trashed_sessions()
    }
}

impl ChatSessionManagerHandle {
    /// Send a session to the trash: it leaves the list and keeps its
    /// messages.
    ///
    /// # Errors
    ///
    /// [`ChatError::SessionNotFound`] when no session has this id.
    pub async fn trash_session(&self, session_id: SessionId) -> Result<(), ChatError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(ChatCommand::TrashSession { session_id, reply })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;
        answer
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Take a session out of the trash, back into the list with its
    /// messages.
    ///
    /// # Errors
    ///
    /// [`ChatError::SessionNotFound`] when the trash holds no session with
    /// this id.
    pub async fn restore_session(&self, session_id: SessionId) -> Result<SessionDetail, ChatError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(ChatCommand::RestoreSession { session_id, reply })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;
        answer
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// The sessions in the trash, the last removed first.
    ///
    /// # Errors
    ///
    /// [`ChatError::InternalError`] when the actor is gone or the database
    /// refuses the read.
    pub async fn list_trashed_sessions(&self) -> Result<Vec<TrashedSession>, ChatError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(ChatCommand::ListTrashedSessions { reply })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;
        answer
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }
}
