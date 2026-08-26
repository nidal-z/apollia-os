//! Tool-approval ledger for chat sessions.
//!
//! Records every human decision on a tool call, and reads back the tools a
//! session has authorized for the rest of its life.

use std::collections::HashSet;

use rusqlite::params;

use crate::chat::repository::{ChatApprovalLogRow, ChatSessionRepository, ToolApprovalLogEntry};
use crate::chat::types::ChatError;

impl ChatSessionRepository {
    /// Persist a resolved chat tool approval decision in the log.
    ///
    /// `reason` carries the operator-provided refusal explanation (or `None`
    /// for accept / always_accept). Stored verbatim so the inbox history view
    /// can surface it.
    pub fn log_tool_approval(&self, entry: ToolApprovalLogEntry<'_>) -> Result<(), ChatError> {
        self.conn
            .execute(
                "INSERT INTO chat_approval_log
                    (session_id, message_id, tool_name, decision, resolved_at, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    entry.session_id,
                    entry.message_id,
                    entry.tool_name,
                    entry.decision,
                    entry.resolved_at,
                    entry.reason
                ],
            )
            .map_err(|e| ChatError::InternalError(format!("log_tool_approval: {e}")))?;
        Ok(())
    }

    /// List recently resolved chat tool approvals for the history view.
    pub fn list_tool_approval_history(
        &self,
        limit: i64,
        days: i64,
    ) -> Result<Vec<ChatApprovalLogRow>, ChatError> {
        let cutoff = format!("-{days} days");
        let mut stmt = self
            .conn
            .prepare(
                "SELECT session_id, message_id, tool_name, decision, resolved_at, reason
                 FROM chat_approval_log
                 WHERE resolved_at >= datetime('now', ?1)
                 ORDER BY resolved_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| {
                ChatError::InternalError(format!("list_tool_approval_history prepare: {e}"))
            })?;

        let rows = stmt
            .query_map(params![cutoff, limit], |row| {
                Ok(ChatApprovalLogRow {
                    session_id: row.get(0)?,
                    message_id: row.get(1)?,
                    tool_name: row.get(2)?,
                    decision: row.get(3)?,
                    resolved_at: row.get(4)?,
                    reason: row.get(5)?,
                })
            })
            .map_err(|e| {
                ChatError::InternalError(format!("list_tool_approval_history query: {e}"))
            })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(|e| {
                ChatError::InternalError(format!("list_tool_approval_history row: {e}"))
            })?);
        }
        Ok(result)
    }

    /// Get the set of authorized tool names for a session.
    pub fn get_authorized_tools(&self, session_id: &str) -> Result<HashSet<String>, ChatError> {
        let mut stmt = self
            .conn
            .prepare("SELECT tool_name FROM chat_tool_authorizations WHERE session_id = ?1")
            .map_err(|e| ChatError::InternalError(format!("get_authorized_tools prepare: {e}")))?;

        let mapped = stmt
            .query_map(params![session_id], |row| row.get::<_, String>(0))
            .map_err(|e| ChatError::InternalError(format!("get_authorized_tools query: {e}")))?;

        let mut result = HashSet::new();
        for r in mapped {
            result.insert(
                r.map_err(|e| ChatError::InternalError(format!("get_authorized_tools row: {e}")))?,
            );
        }
        Ok(result)
    }
}
