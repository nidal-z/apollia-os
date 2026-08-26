//! Project scoping of chat sessions.
//!
//! A session belongs to at most one project; orphaning a project detaches its
//! sessions instead of deleting them.

use rusqlite::params;

use crate::chat::repository::{row_to_session, ChatSessionRepository, SessionRow};
use crate::chat::types::ChatError;

impl ChatSessionRepository {
    /// List sessions belonging to a specific project.
    pub fn list_sessions_by_project(&self, project_id: &str) -> Result<Vec<SessionRow>, ChatError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at,
                        closed_at, llm_backend, summary, title, parent_session_id, fork_depth, project_id,
                        plan_mode, plan_phase
                 FROM chat_sessions WHERE project_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| ChatError::InternalError(format!("list_sessions_by_project prepare: {e}")))?;

        let rows = stmt
            .query_map(params![project_id], row_to_session)
            .map_err(|e| {
                ChatError::InternalError(format!("list_sessions_by_project query: {e}"))
            })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(|e| {
                ChatError::InternalError(format!("list_sessions_by_project row: {e}"))
            })?);
        }
        Ok(result)
    }

    /// Link or unlink a session to a project.
    ///
    /// Pass `None` to unlink the session from any project.
    pub fn set_session_project(
        &self,
        session_id: &str,
        project_id: Option<&str>,
    ) -> Result<(), ChatError> {
        let updated = self
            .conn
            .execute(
                "UPDATE chat_sessions SET project_id = ?1 WHERE id = ?2",
                params![project_id, session_id],
            )
            .map_err(|e| ChatError::InternalError(format!("set_session_project: {e}")))?;

        if updated == 0 {
            return Err(ChatError::SessionNotFound(session_id.to_string()));
        }
        Ok(())
    }

    /// Unlink all sessions from a project (orphan them).
    ///
    /// Called when a project is deleted to avoid dangling project_id references.
    pub fn orphan_project_sessions(&self, project_id: &str) -> Result<usize, ChatError> {
        let count = self
            .conn
            .execute(
                "UPDATE chat_sessions SET project_id = NULL WHERE project_id = ?1",
                params![project_id],
            )
            .map_err(|e| ChatError::InternalError(format!("orphan_project_sessions: {e}")))?;
        Ok(count)
    }
}
