//! Full-text search over past chat sessions.
//!
//! Reads the FTS5 index that `update_summary` maintains, and the plain
//! recent-summary listing that feeds the first message of a free chat.

use rusqlite::params;

use crate::chat::repository::{sanitize_fts_query, ChatSessionRepository};
use crate::chat::types::{ChatError, PastSessionSummary, RecentSessionSummary};

impl ChatSessionRepository {
    /// Search past sessions by summary relevance using FTS5 full-text search.
    ///
    /// Returns the top `limit` sessions whose summary matches the query,
    /// sorted by BM25 relevance. Only closed sessions with a non-empty summary
    /// are returned.
    pub fn find_relevant_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<PastSessionSummary>, ChatError> {
        let sanitized = sanitize_fts_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT session_id, created_at, summary
                 FROM chat_sessions_fts
                 WHERE summary MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )
            .map_err(|e| {
                ChatError::InternalError(format!("find_relevant_sessions prepare: {e}"))
            })?;

        let rows = stmt
            .query_map(params![sanitized, limit], |row| {
                Ok(PastSessionSummary {
                    session_id: row.get(0)?,
                    created_at: row.get(1)?,
                    summary: row.get(2)?,
                })
            })
            .map_err(|e| ChatError::InternalError(format!("find_relevant_sessions query: {e}")))?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(|e| {
                ChatError::InternalError(format!("find_relevant_sessions row: {e}"))
            })?);
        }
        Ok(result)
    }

    /// List the N most recent sessions with the content of their first user message.
    ///
    /// Uses a LEFT JOIN between `chat_sessions` and `chat_messages` (seq=1, role='user')
    /// so that sessions with no messages are still included (first_message = None).
    /// Results are ordered by `created_at DESC`, limited to `limit` rows.
    pub fn list_recent_summaries(
        &self,
        limit: usize,
    ) -> Result<Vec<RecentSessionSummary>, ChatError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.id, s.mode, s.status, m.content, s.created_at
                 FROM chat_sessions s
                 LEFT JOIN chat_messages m
                   ON m.session_id = s.id AND m.seq = 1 AND m.role = 'user'
                 ORDER BY s.created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| ChatError::InternalError(format!("list_recent_summaries prepare: {e}")))?;

        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(RecentSessionSummary {
                    id: row.get(0)?,
                    mode: row.get(1)?,
                    status: row.get(2)?,
                    first_message: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| ChatError::InternalError(format!("list_recent_summaries query: {e}")))?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(|e| {
                ChatError::InternalError(format!("list_recent_summaries row: {e}"))
            })?);
        }
        Ok(result)
    }
}
