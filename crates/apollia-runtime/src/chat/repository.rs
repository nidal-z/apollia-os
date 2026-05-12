//! SQLite-backed repository for chat sessions and messages.
//!
//! Operates on `~/.apollia/chat.db` — one database per runtime instance.
//! All writes are synchronous (rusqlite); callers should wrap in
//! `spawn_blocking` for async contexts.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection};

use super::types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, PastSessionSummary,
    RecentSessionSummary, SessionStatus, ToolCallRecord,
};

/// SQL migration applied on first open.
const MIGRATION_SQL: &str = include_str!("../../migrations/001_chat_tables.sql");

/// Raw row from the `chat_sessions` table.
#[derive(Debug, Clone)]
pub struct SessionRow {
    /// Session identifier.
    pub id: String,
    /// Mode string (`"libre"` or `"agent"`).
    pub mode: String,
    /// Agent name (nullable).
    pub agent_name: Option<String>,
    /// System prompt.
    pub system_prompt: String,
    /// Status string (`"active"`, `"processing"`, `"closed"`).
    pub status: String,
    /// JSON array of available tool names.
    pub available_tools: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 close timestamp (nullable).
    pub closed_at: Option<String>,
    /// Preferred LLM backend name (nullable — uses runtime default when None).
    pub llm_backend: Option<String>,
    /// Conversation summary produced by the summarizer (nullable).
    pub summary: Option<String>,
    /// User-defined display title (nullable — falls back to agent_name or mode).
    pub title: Option<String>,
    /// Parent session identifier when this session is a fork (nullable).
    pub parent_session_id: Option<String>,
    /// Fork depth: 0 for root sessions, N+1 for a fork of a depth-N session.
    pub fork_depth: i64,
    /// Project this session belongs to (application-level link, no FK).
    pub project_id: Option<String>,
}

/// Raw row from the `chat_messages` table.
#[derive(Debug, Clone)]
pub struct MessageRow {
    /// Message identifier.
    pub id: String,
    /// Owning session identifier.
    pub session_id: String,
    /// Role string (`"user"`, `"assistant"`, `"system"`, `"tool"`).
    pub role: String,
    /// Text content.
    pub content: String,
    /// JSON-encoded tool calls (nullable).
    pub tool_calls_json: Option<String>,
    /// Tool name for tool-role messages (nullable).
    pub tool_name: Option<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// Sequence number within the session.
    pub seq: u32,
    /// JSON-encoded metadata (nullable).
    pub metadata: Option<String>,
}

/// Row returned by [`ChatSessionRepository::list_tool_approval_history`].
pub struct ChatApprovalLogRow {
    /// Session identifier.
    pub session_id: String,
    /// Message identifier the approval was attached to.
    pub message_id: String,
    /// Tool that required approval.
    pub tool_name: String,
    /// Decision taken (`accept`, `refuse`, `always_accept`).
    pub decision: String,
    /// ISO-8601 timestamp of the resolution.
    pub resolved_at: String,
    /// Operator-provided refusal reason (only set for `refuse` decisions).
    pub reason: Option<String>,
}

/// Parameters for appending a message to a chat session.
pub struct AppendMessageParams<'a> {
    /// Unique message identifier.
    pub id: &'a str,
    /// Owning session identifier.
    pub session_id: &'a str,
    /// Role of the message sender.
    pub role: &'a ChatRole,
    /// Text content.
    pub content: &'a str,
    /// JSON-encoded tool calls (optional).
    pub tool_calls_json: Option<&'a str>,
    /// Tool name for tool-role messages (optional).
    pub tool_name: Option<&'a str>,
    /// ISO-8601 creation timestamp.
    pub created_at: &'a str,
    /// JSON-encoded metadata (optional, e.g. thinking trace).
    pub metadata: Option<&'a str>,
}

/// CRUD repository for chat sessions, messages, and tool authorizations.
///
/// Wraps a single SQLite connection to `chat.db`.
/// Thread-safety is achieved by the caller (typically `Arc<Mutex<ChatSessionRepository>>`).
pub struct ChatSessionRepository {
    conn: Connection,
}

impl ChatSessionRepository {
    /// Open (or create) the chat database at the given path and run migrations.
    pub fn open(path: &Path) -> Result<Self, ChatError> {
        let conn = Connection::open(path)
            .map_err(|e| ChatError::InternalError(format!("failed to open chat.db: {e}")))?;

        conn.execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(|e| ChatError::InternalError(format!("WAL pragma failed: {e}")))?;

        conn.execute_batch(MIGRATION_SQL)
            .map_err(|e| ChatError::InternalError(format!("migration failed: {e}")))?;

        // v2 migration: add llm_backend column for existing databases.
        let _ = conn.execute_batch("ALTER TABLE chat_sessions ADD COLUMN llm_backend TEXT");

        // v3 migration: add summary column for conversation summarization.
        let _ = conn.execute_batch("ALTER TABLE chat_sessions ADD COLUMN summary TEXT");

        // v4 migration: FTS5 index on session summaries for cross-session recall.
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS chat_sessions_fts USING fts5(
                session_id UNINDEXED,
                created_at UNINDEXED,
                summary
            );",
        )
        .map_err(|e| ChatError::InternalError(format!("FTS5 migration failed: {e}")))?;

        // v5 migration: add title column for user-defined session names.
        let _ = conn.execute_batch("ALTER TABLE chat_sessions ADD COLUMN title TEXT");

        // v6 migration: conversation forking support.
        let _ = conn.execute_batch(
            "ALTER TABLE chat_sessions ADD COLUMN parent_session_id TEXT REFERENCES chat_sessions(id)",
        );
        let _ = conn.execute_batch(
            "ALTER TABLE chat_sessions ADD COLUMN fork_depth INTEGER NOT NULL DEFAULT 0",
        );
        let _ = conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_sessions_parent ON chat_sessions(parent_session_id)",
        );

        // v7 migration: project linkage (application-level, no FK — separate databases).
        let _ = conn.execute_batch("ALTER TABLE chat_sessions ADD COLUMN project_id TEXT");
        let _ = conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_chat_sessions_project ON chat_sessions(project_id)",
        );

        // v8 migration: metadata column on messages (thinking trace, etc.).
        let _ = conn.execute_batch("ALTER TABLE chat_messages ADD COLUMN metadata TEXT");

        // v9 migration: chat approval log for resolved tool approvals history.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chat_approval_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL,
                message_id  TEXT NOT NULL,
                tool_name   TEXT NOT NULL,
                decision    TEXT NOT NULL CHECK (decision IN ('accept', 'refuse', 'always_accept')),
                resolved_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_approval_log_resolved
                ON chat_approval_log(resolved_at DESC);",
        )
        .map_err(|e| ChatError::InternalError(format!("v9 migration failed: {e}")))?;

        // v11 migration (pre-v10 path): store the operator-provided refusal
        // reason. NULL for accept / always_accept.
        let _ = conn.execute_batch("ALTER TABLE chat_approval_log ADD COLUMN reason TEXT");

        // v10 migration: add 'companion' to the mode CHECK constraint.
        // SQLite doesn't support ALTER TABLE DROP CONSTRAINT, so we recreate the table.
        conn.execute_batch(
            "PRAGMA foreign_keys = OFF;
            BEGIN;
            CREATE TABLE IF NOT EXISTS chat_sessions_new (
                id              TEXT PRIMARY KEY,
                mode            TEXT NOT NULL CHECK (mode IN ('libre', 'agent', 'companion')),
                agent_name      TEXT,
                system_prompt   TEXT NOT NULL DEFAULT '',
                status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'processing', 'closed')),
                available_tools TEXT NOT NULL DEFAULT '[]',
                created_at      TEXT NOT NULL,
                closed_at       TEXT,
                llm_backend     TEXT,
                summary         TEXT,
                title           TEXT,
                parent_session_id TEXT REFERENCES chat_sessions_new(id),
                fork_depth      INTEGER NOT NULL DEFAULT 0,
                project_id      TEXT
            );
            INSERT OR IGNORE INTO chat_sessions_new
                SELECT id, mode, agent_name, system_prompt, status, available_tools,
                       created_at, closed_at, llm_backend, summary, title,
                       parent_session_id, fork_depth, project_id
                FROM chat_sessions;
            DROP TABLE chat_sessions;
            ALTER TABLE chat_sessions_new RENAME TO chat_sessions;
            CREATE INDEX IF NOT EXISTS idx_chat_sessions_status ON chat_sessions(status);
            CREATE INDEX IF NOT EXISTS idx_sessions_parent ON chat_sessions(parent_session_id);
            CREATE INDEX IF NOT EXISTS idx_chat_sessions_project ON chat_sessions(project_id);
            COMMIT;
            PRAGMA foreign_keys = ON;",
        )
        .map_err(|e| ChatError::InternalError(format!("v10 migration failed: {e}")))?;

        Ok(Self { conn })
    }

    /// Open an in-memory database (for tests).
    #[cfg(test)]
    fn open_in_memory() -> Result<Self, ChatError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ChatError::InternalError(format!("in-memory open failed: {e}")))?;

        conn.execute_batch(MIGRATION_SQL)
            .map_err(|e| ChatError::InternalError(format!("migration failed: {e}")))?;

        // v3 migration: summary column (already in CREATE TABLE for fresh DBs,
        // but needed here since MIGRATION_SQL predates this column).
        let _ = conn.execute_batch("ALTER TABLE chat_sessions ADD COLUMN summary TEXT");

        // v4 migration: FTS5 index on session summaries for cross-session recall.
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS chat_sessions_fts USING fts5(
                session_id UNINDEXED,
                created_at UNINDEXED,
                summary
            );",
        )
        .map_err(|e| ChatError::InternalError(format!("FTS5 migration failed: {e}")))?;

        // v5 migration: title column.
        let _ = conn.execute_batch("ALTER TABLE chat_sessions ADD COLUMN title TEXT");

        // v6 migration: conversation forking support.
        let _ = conn.execute_batch(
            "ALTER TABLE chat_sessions ADD COLUMN parent_session_id TEXT REFERENCES chat_sessions(id)",
        );
        let _ = conn.execute_batch(
            "ALTER TABLE chat_sessions ADD COLUMN fork_depth INTEGER NOT NULL DEFAULT 0",
        );
        let _ = conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_sessions_parent ON chat_sessions(parent_session_id)",
        );

        // v7 migration: project linkage.
        let _ = conn.execute_batch("ALTER TABLE chat_sessions ADD COLUMN project_id TEXT");
        let _ = conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_chat_sessions_project ON chat_sessions(project_id)",
        );

        // v8 migration: metadata column on messages.
        let _ = conn.execute_batch("ALTER TABLE chat_messages ADD COLUMN metadata TEXT");

        // v9 migration: chat approval log.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chat_approval_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL,
                message_id  TEXT NOT NULL,
                tool_name   TEXT NOT NULL,
                decision    TEXT NOT NULL CHECK (decision IN ('accept', 'refuse', 'always_accept')),
                resolved_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_approval_log_resolved
                ON chat_approval_log(resolved_at DESC);",
        )
        .map_err(|e| ChatError::InternalError(format!("v9 migration failed: {e}")))?;

        // v11 migration: store the operator-provided refusal reason so it can
        // be displayed in the inbox history view. NULL for accept / always_accept.
        let _ = conn.execute_batch("ALTER TABLE chat_approval_log ADD COLUMN reason TEXT");

        Ok(Self { conn })
    }

    /// Persist a new chat session.
    #[allow(clippy::too_many_arguments)]
    pub fn create_session(
        &self,
        id: &str,
        mode: &ChatMode,
        agent_name: Option<&str>,
        system_prompt: &str,
        available_tools: &[String],
        created_at: &str,
        llm_backend: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<(), ChatError> {
        let tools_json = serde_json::to_string(available_tools)
            .map_err(|e| ChatError::InternalError(format!("tools serialization: {e}")))?;

        self.conn
            .execute(
                "INSERT INTO chat_sessions (id, mode, agent_name, system_prompt, available_tools, created_at, llm_backend, project_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, mode.as_sql(), agent_name, system_prompt, tools_json, created_at, llm_backend, project_id],
            )
            .map_err(|e| ChatError::InternalError(format!("create_session: {e}")))?;

        Ok(())
    }

    /// Update session configuration (system_prompt, available_tools, llm_backend).
    ///
    /// Only updates fields that are `Some`.
    pub fn update_session_config(
        &self,
        id: &str,
        system_prompt: Option<&str>,
        available_tools: Option<&[String]>,
        llm_backend: Option<Option<&str>>,
    ) -> Result<(), ChatError> {
        let mut parts = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(prompt) = system_prompt {
            parts.push("system_prompt = ?");
            values.push(Box::new(prompt.to_string()));
        }
        if let Some(tools) = available_tools {
            let tools_json = serde_json::to_string(tools)
                .map_err(|e| ChatError::InternalError(format!("tools serialization: {e}")))?;
            parts.push("available_tools = ?");
            values.push(Box::new(tools_json));
        }
        if let Some(backend) = llm_backend {
            parts.push("llm_backend = ?");
            values.push(Box::new(backend.map(|s| s.to_string())));
        }

        if parts.is_empty() {
            return Ok(());
        }

        let set_clause = parts.join(", ");
        let sql = format!("UPDATE chat_sessions SET {set_clause} WHERE id = ?");
        values.push(Box::new(id.to_string()));

        let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let updated = self
            .conn
            .execute(&sql, params.as_slice())
            .map_err(|e| ChatError::InternalError(format!("update_session_config: {e}")))?;

        if updated == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Delete a session and all its messages and tool authorizations.
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn delete_session(&self, id: &str) -> Result<(), ChatError> {
        // Delete related data first (foreign-key-like cleanup).
        self.conn
            .execute(
                "DELETE FROM chat_messages WHERE session_id = ?1",
                params![id],
            )
            .map_err(|e| ChatError::InternalError(format!("delete_session messages: {e}")))?;

        self.conn
            .execute(
                "DELETE FROM chat_tool_authorizations WHERE session_id = ?1",
                params![id],
            )
            .map_err(|e| ChatError::InternalError(format!("delete_session authorizations: {e}")))?;

        // Clean up FTS5 index.
        let _ = self.conn.execute(
            "DELETE FROM chat_sessions_fts WHERE session_id = ?1",
            params![id],
        );

        let deleted = self
            .conn
            .execute("DELETE FROM chat_sessions WHERE id = ?1", params![id])
            .map_err(|e| ChatError::InternalError(format!("delete_session: {e}")))?;

        if deleted == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Rename a session by setting a user-defined title.
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn rename_session(&self, id: &str, title: &str) -> Result<(), ChatError> {
        let updated = self
            .conn
            .execute(
                "UPDATE chat_sessions SET title = ?1 WHERE id = ?2",
                params![title, id],
            )
            .map_err(|e| ChatError::InternalError(format!("rename_session: {e}")))?;

        if updated == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Retrieve a session by ID, or `None` if not found.
    pub fn get_session(&self, id: &str) -> Result<Option<SessionRow>, ChatError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at,
                        closed_at, llm_backend, summary, title, parent_session_id, fork_depth, project_id
                 FROM chat_sessions WHERE id = ?1",
            )
            .map_err(|e| ChatError::InternalError(format!("get_session prepare: {e}")))?;

        let row = stmt
            .query_row(params![id], row_to_session)
            .optional()
            .map_err(|e| ChatError::InternalError(format!("get_session query: {e}")))?;

        Ok(row)
    }

    /// List sessions, optionally filtered by status.
    pub fn list_sessions(&self, status: Option<&str>) -> Result<Vec<SessionRow>, ChatError> {
        let (sql, param): (&str, Option<&str>) = match status {
            Some(s) => (
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at,
                        closed_at, llm_backend, summary, title, parent_session_id, fork_depth, project_id
                 FROM chat_sessions WHERE status = ?1 ORDER BY created_at DESC",
                Some(s),
            ),
            None => (
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at,
                        closed_at, llm_backend, summary, title, parent_session_id, fork_depth, project_id
                 FROM chat_sessions ORDER BY created_at DESC",
                None,
            ),
        };

        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| ChatError::InternalError(format!("list_sessions prepare: {e}")))?;

        let rows = if let Some(p) = param {
            stmt.query_map(params![p], row_to_session)
        } else {
            stmt.query_map([], row_to_session)
        }
        .map_err(|e| ChatError::InternalError(format!("list_sessions query: {e}")))?;

        let mut result = Vec::new();
        for r in rows {
            result
                .push(r.map_err(|e| ChatError::InternalError(format!("list_sessions row: {e}")))?);
        }
        Ok(result)
    }

    /// Update session status (e.g. Active → Processing or Processing → Active).
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn update_status(&self, id: &str, status: &SessionStatus) -> Result<(), ChatError> {
        let updated = self
            .conn
            .execute(
                "UPDATE chat_sessions SET status = ?1 WHERE id = ?2",
                params![status.as_sql(), id],
            )
            .map_err(|e| ChatError::InternalError(format!("update_status: {e}")))?;

        if updated == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Close a session — sets `status='closed'` and records `closed_at`.
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn close_session(&self, id: &str, closed_at: &str) -> Result<(), ChatError> {
        let updated = self
            .conn
            .execute(
                "UPDATE chat_sessions SET status = ?1, closed_at = ?2 WHERE id = ?3",
                params![SessionStatus::Closed.as_sql(), closed_at, id],
            )
            .map_err(|e| ChatError::InternalError(format!("close_session: {e}")))?;

        if updated == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Append a message to a session with auto-incremented `seq`.
    pub fn append_message(&self, params: &AppendMessageParams<'_>) -> Result<u32, ChatError> {
        // Compute next seq for this session
        let next_seq: u32 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_messages WHERE session_id = ?1",
                rusqlite::params![params.session_id],
                |row| row.get(0),
            )
            .map_err(|e| ChatError::InternalError(format!("append_message seq: {e}")))?;

        self.conn
            .execute(
                "INSERT INTO chat_messages (id, session_id, role, content, tool_calls_json, tool_name, created_at, seq, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    params.id,
                    params.session_id,
                    params.role.as_sql(),
                    params.content,
                    params.tool_calls_json,
                    params.tool_name,
                    params.created_at,
                    next_seq,
                    params.metadata
                ],
            )
            .map_err(|e| ChatError::InternalError(format!("append_message insert: {e}")))?;

        Ok(next_seq)
    }

    /// Retrieve messages for a session, ordered by `seq` ascending.
    ///
    /// If `limit` is `Some(n)`, only the last `n` messages are returned.
    pub fn get_messages(
        &self,
        session_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<MessageRow>, ChatError> {
        let rows = match limit {
            Some(n) => {
                let mut stmt = self
                    .conn
                    .prepare(
                        "SELECT id, session_id, role, content, tool_calls_json, tool_name, created_at, seq, metadata
                         FROM chat_messages WHERE session_id = ?1
                         ORDER BY seq DESC LIMIT ?2",
                    )
                    .map_err(|e| {
                        ChatError::InternalError(format!("get_messages prepare: {e}"))
                    })?;

                let mapped = stmt
                    .query_map(params![session_id, n], row_to_message)
                    .map_err(|e| ChatError::InternalError(format!("get_messages query: {e}")))?;

                let mut result = Vec::new();
                for r in mapped {
                    result.push(
                        r.map_err(|e| ChatError::InternalError(format!("get_messages row: {e}")))?,
                    );
                }
                // Reverse to restore ascending seq order
                result.reverse();
                result
            }
            None => {
                let mut stmt = self
                    .conn
                    .prepare(
                        "SELECT id, session_id, role, content, tool_calls_json, tool_name, created_at, seq, metadata
                         FROM chat_messages WHERE session_id = ?1
                         ORDER BY seq ASC",
                    )
                    .map_err(|e| {
                        ChatError::InternalError(format!("get_messages prepare: {e}"))
                    })?;

                let mapped = stmt
                    .query_map(params![session_id], row_to_message)
                    .map_err(|e| ChatError::InternalError(format!("get_messages query: {e}")))?;

                let mut result = Vec::new();
                for r in mapped {
                    result.push(
                        r.map_err(|e| ChatError::InternalError(format!("get_messages row: {e}")))?,
                    );
                }
                result
            }
        };

        Ok(rows)
    }

    /// Authorize a tool for a session (idempotent — `INSERT OR IGNORE`).
    pub fn authorize_tool(
        &self,
        session_id: &str,
        tool_name: &str,
        authorized_at: &str,
    ) -> Result<(), ChatError> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO chat_tool_authorizations (session_id, tool_name, authorized_at)
                 VALUES (?1, ?2, ?3)",
                params![session_id, tool_name, authorized_at],
            )
            .map_err(|e| ChatError::InternalError(format!("authorize_tool: {e}")))?;

        Ok(())
    }

    /// Store or replace the conversation summary for a session.
    ///
    /// Also updates the FTS5 index used for cross-session recall.
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn update_summary(&self, session_id: &str, summary: &str) -> Result<(), ChatError> {
        let updated = self
            .conn
            .execute(
                "UPDATE chat_sessions SET summary = ?1 WHERE id = ?2",
                params![summary, session_id],
            )
            .map_err(|e| ChatError::InternalError(format!("update_summary: {e}")))?;

        if updated == 0 {
            return Err(ChatError::SessionNotFound(session_id.to_string()));
        }

        // Fetch created_at for the FTS5 index
        let created_at: String = self
            .conn
            .query_row(
                "SELECT created_at FROM chat_sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .map_err(|e| ChatError::InternalError(format!("update_summary created_at: {e}")))?;

        // Replace FTS5 entry (delete then insert, since FTS5 doesn't support REPLACE natively)
        self.conn
            .execute(
                "DELETE FROM chat_sessions_fts WHERE session_id = ?1",
                params![session_id],
            )
            .map_err(|e| ChatError::InternalError(format!("update_summary fts delete: {e}")))?;
        self.conn
            .execute(
                "INSERT INTO chat_sessions_fts (session_id, created_at, summary) VALUES (?1, ?2, ?3)",
                params![session_id, created_at, summary],
            )
            .map_err(|e| ChatError::InternalError(format!("update_summary fts insert: {e}")))?;

        Ok(())
    }

    /// Retrieve the conversation summary for a session, if any.
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn get_summary(&self, session_id: &str) -> Result<Option<String>, ChatError> {
        let session = self
            .get_session(session_id)?
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
        Ok(session.summary)
    }

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

    /// Load a session and its full message history from SQLite.
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if no session with `session_id` exists.
    /// Authorized tools and message history are eagerly fetched and embedded into the
    /// returned [`ChatSession`].
    pub fn load_session_with_history(&self, session_id: &str) -> Result<ChatSession, ChatError> {
        let row = self
            .get_session(session_id)?
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        let message_rows = self.get_messages(session_id, None)?;
        let authorized_tools = self.get_authorized_tools(session_id)?;

        let mode = ChatMode::from_sql(&row.mode)
            .ok_or_else(|| ChatError::InternalError(format!("unknown mode: {}", row.mode)))?;
        let status = SessionStatus::from_sql(&row.status)
            .ok_or_else(|| ChatError::InternalError(format!("unknown status: {}", row.status)))?;
        let available_tools: Vec<String> =
            serde_json::from_str(&row.available_tools).unwrap_or_default();

        let history: Vec<ChatMessage> = message_rows
            .into_iter()
            .map(|m| {
                let role = ChatRole::from_sql(&m.role).unwrap_or(ChatRole::User);
                let tool_calls: Option<Vec<ToolCallRecord>> = m
                    .tool_calls_json
                    .as_deref()
                    .and_then(|j| serde_json::from_str(j).ok());
                let metadata: Option<serde_json::Value> = m
                    .metadata
                    .as_deref()
                    .and_then(|j| serde_json::from_str(j).ok());
                ChatMessage {
                    id: m.id,
                    role,
                    content: m.content,
                    tool_calls,
                    tool_name: m.tool_name,
                    created_at: m.created_at,
                    seq: m.seq,
                    metadata,
                }
            })
            .collect();

        Ok(ChatSession {
            id: row.id,
            mode,
            agent_name: row.agent_name,
            system_prompt: row.system_prompt,
            status,
            history,
            authorized_tools,
            available_tools,
            created_at: row.created_at,
            active_exchange: None,
            llm_backend: row.llm_backend,
            title: row.title,
            parent_session_id: row.parent_session_id,
            fork_depth: row.fork_depth,
            project_id: row.project_id,
            force_project_context_inject: false,
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
        })
    }

    /// Create a child session that forks from `parent_id`, copying the first
    /// `up_to_count` messages (or all messages when `None`).
    ///
    /// The child inherits the parent's mode, system prompt, available tools, and
    /// LLM backend. Its `parent_session_id` is set to `parent_id` and its
    /// `fork_depth` is `parent.fork_depth + 1`.
    ///
    /// Returns the fully-populated [`ChatSession`] for the new child.
    pub fn create_fork_session(
        &self,
        child_id: &str,
        parent_id: &str,
        up_to_count: Option<usize>,
        created_at: &str,
    ) -> Result<ChatSession, ChatError> {
        let parent_row = self
            .get_session(parent_id)?
            .ok_or_else(|| ChatError::SessionNotFound(parent_id.to_string()))?;

        let child_fork_depth = parent_row.fork_depth + 1;
        let tools_json = &parent_row.available_tools;

        // Persist child session row (inherits parent's project_id).
        self.conn
            .execute(
                "INSERT INTO chat_sessions
                    (id, mode, agent_name, system_prompt, available_tools, created_at,
                     llm_backend, parent_session_id, fork_depth, project_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    child_id,
                    parent_row.mode,
                    parent_row.agent_name,
                    parent_row.system_prompt,
                    tools_json,
                    created_at,
                    parent_row.llm_backend,
                    parent_id,
                    child_fork_depth,
                    parent_row.project_id,
                ],
            )
            .map_err(|e| ChatError::InternalError(format!("create_fork_session insert: {e}")))?;

        // Copy messages from parent to child with fresh IDs and sequential seq.
        let parent_messages = self.get_messages(parent_id, None)?;
        let message_slice: &[MessageRow] = match up_to_count {
            Some(n) => {
                let end = n.min(parent_messages.len());
                &parent_messages[..end]
            }
            None => &parent_messages,
        };

        for (idx, msg) in message_slice.iter().enumerate() {
            let new_msg_id = uuid::Uuid::new_v4().to_string();
            let seq = (idx + 1) as u32;
            self.conn
                .execute(
                    "INSERT INTO chat_messages
                        (id, session_id, role, content, tool_calls_json, tool_name, created_at, seq, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        new_msg_id,
                        child_id,
                        msg.role,
                        msg.content,
                        msg.tool_calls_json,
                        msg.tool_name,
                        msg.created_at,
                        seq,
                        msg.metadata,
                    ],
                )
                .map_err(|e| {
                    ChatError::InternalError(format!("create_fork_session copy message: {e}"))
                })?;
        }

        // Reload the child with full history.
        self.load_session_with_history(child_id)
    }

    /// List all sessions that are direct children (forks) of the given parent.
    ///
    /// Results are ordered by `created_at` ascending.
    pub fn list_children(&self, parent_id: &str) -> Result<Vec<SessionRow>, ChatError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at,
                        closed_at, llm_backend, summary, title, parent_session_id, fork_depth, project_id
                 FROM chat_sessions
                 WHERE parent_session_id = ?1
                 ORDER BY created_at ASC",
            )
            .map_err(|e| ChatError::InternalError(format!("list_children prepare: {e}")))?;

        let rows = stmt
            .query_map(params![parent_id], row_to_session)
            .map_err(|e| ChatError::InternalError(format!("list_children query: {e}")))?;

        let mut result = Vec::new();
        for r in rows {
            result
                .push(r.map_err(|e| ChatError::InternalError(format!("list_children row: {e}")))?);
        }
        Ok(result)
    }

    /// List sessions belonging to a specific project.
    pub fn list_sessions_by_project(&self, project_id: &str) -> Result<Vec<SessionRow>, ChatError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at,
                        closed_at, llm_backend, summary, title, parent_session_id, fork_depth, project_id
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

    /// Persist a resolved chat tool approval decision in the log.
    ///
    /// `reason` carries the operator-provided refusal explanation (or `None`
    /// for accept / always_accept). Stored verbatim so the inbox history view
    /// can surface it.
    pub fn log_tool_approval(
        &self,
        session_id: &str,
        message_id: &str,
        tool_name: &str,
        decision: &str,
        resolved_at: &str,
        reason: Option<&str>,
    ) -> Result<(), ChatError> {
        self.conn
            .execute(
                "INSERT INTO chat_approval_log
                    (session_id, message_id, tool_name, decision, resolved_at, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![session_id, message_id, tool_name, decision, resolved_at, reason],
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

/// Helper: map a rusqlite row to `SessionRow`.
fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        mode: row.get(1)?,
        agent_name: row.get(2)?,
        system_prompt: row.get(3)?,
        status: row.get(4)?,
        available_tools: row.get(5)?,
        created_at: row.get(6)?,
        closed_at: row.get(7)?,
        llm_backend: row.get(8)?,
        summary: row.get(9)?,
        title: row.get(10)?,
        parent_session_id: row.get(11)?,
        fork_depth: row.get(12)?,
        project_id: row.get(13)?,
    })
}

/// Helper: map a rusqlite row to `MessageRow`.
fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        tool_calls_json: row.get(4)?,
        tool_name: row.get(5)?,
        created_at: row.get(6)?,
        seq: row.get(7)?,
        metadata: row.get(8).unwrap_or(None),
    })
}

/// Sanitize a user query for FTS5 MATCH by extracting alphanumeric words
/// and joining them with implicit AND.
///
/// FTS5 special characters (`"`, `*`, `(`, `)`, etc.) are stripped.
/// Returns an empty string if the query contains no searchable terms.
fn sanitize_fts_query(input: &str) -> String {
    input
        .split_whitespace()
        .filter_map(|word| {
            let clean: String = word
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect();
            if clean.is_empty() {
                None
            } else {
                Some(clean)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extension trait to make `query_row` return `Option` instead of error on missing row.
trait OptionalExt<T> {
    /// Convert a `QueryReturnedNoRows` error into `Ok(None)`.
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_session() {
        // GIVEN a repository in memory
        let repo = ChatSessionRepository::open_in_memory().expect("open");

        // WHEN we create a session and retrieve it
        repo.create_session(
            "sess-1",
            &ChatMode::Libre,
            None,
            "You are helpful.",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");

        let session = repo.get_session("sess-1").expect("get");

        // THEN the session is returned with correct fields
        let session = session.expect("should exist");
        assert_eq!(session.id, "sess-1");
        assert_eq!(session.mode, "libre");
        assert!(session.agent_name.is_none());
        assert_eq!(session.system_prompt, "You are helpful.");
        assert_eq!(session.status, "active");
        assert_eq!(session.available_tools, "[]");
        assert!(session.closed_at.is_none());
    }

    #[test]
    fn test_list_sessions_filter_by_status() {
        // GIVEN 2 sessions (one active, one closed)
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create s1");
        repo.create_session(
            "s2",
            &ChatMode::Agent,
            Some("agent-1"),
            "",
            &[],
            "2026-03-20T11:00:00Z",
            None,
            None,
        )
        .expect("create s2");
        repo.close_session("s2", "2026-03-20T12:00:00Z")
            .expect("close s2");

        // WHEN we list only active sessions
        let active = repo.list_sessions(Some("active")).expect("list active");

        // THEN only s1 is returned
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "s1");

        // AND listing all returns both
        let all = repo.list_sessions(None).expect("list all");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_close_session() {
        // GIVEN an active session
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");

        // WHEN we close it
        repo.close_session("s1", "2026-03-20T12:00:00Z")
            .expect("close");

        // THEN status is 'closed' and closed_at is set
        let session = repo.get_session("s1").expect("get").expect("exists");
        assert_eq!(session.status, "closed");
        assert_eq!(session.closed_at.as_deref(), Some("2026-03-20T12:00:00Z"));
    }

    #[test]
    fn test_close_nonexistent_session() {
        // GIVEN no sessions
        let repo = ChatSessionRepository::open_in_memory().expect("open");

        // WHEN we try to close a nonexistent session
        let result = repo.close_session("nonexistent", "2026-03-20T12:00:00Z");

        // THEN we get SessionNotFound
        assert!(matches!(result, Err(ChatError::SessionNotFound(_))));
    }

    #[test]
    fn test_append_and_get_messages_ordered() {
        // GIVEN a session with 5 messages
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");

        for i in 1..=5u32 {
            let msg_id = format!("msg-{i}");
            let content = format!("message {i}");
            let ts = format!("2026-03-20T10:0{i}:00Z");
            let seq = repo
                .append_message(&AppendMessageParams {
                    id: &msg_id,
                    session_id: "s1",
                    role: &ChatRole::User,
                    content: &content,
                    tool_calls_json: None,
                    tool_name: None,
                    created_at: &ts,
                    metadata: None,
                })
                .expect("append");
            assert_eq!(seq, i);
        }

        // WHEN we get all messages
        let messages = repo.get_messages("s1", None).expect("get");

        // THEN they are ordered by seq ascending
        assert_eq!(messages.len(), 5);
        let seqs: Vec<u32> = messages.iter().map(|m| m.seq).collect();
        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_get_messages_with_limit() {
        // GIVEN 5 messages
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");

        for i in 1..=5u32 {
            let msg_id = format!("msg-{i}");
            let content = format!("message {i}");
            let ts = format!("2026-03-20T10:0{i}:00Z");
            repo.append_message(&AppendMessageParams {
                id: &msg_id,
                session_id: "s1",
                role: &ChatRole::User,
                content: &content,
                tool_calls_json: None,
                tool_name: None,
                created_at: &ts,
                metadata: None,
            })
            .expect("append");
        }

        // WHEN we get the last 3 messages
        let messages = repo.get_messages("s1", Some(3)).expect("get");

        // THEN only 3 messages are returned, with seq 3, 4, 5
        assert_eq!(messages.len(), 3);
        let seqs: Vec<u32> = messages.iter().map(|m| m.seq).collect();
        assert_eq!(seqs, vec![3, 4, 5]);
    }

    #[test]
    fn test_authorize_tool_and_get() {
        // GIVEN a session
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Agent,
            Some("agent"),
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");

        // WHEN we authorize a tool and retrieve authorized tools
        repo.authorize_tool("s1", "bash_executor", "2026-03-20T10:01:00Z")
            .expect("authorize");
        let tools = repo.get_authorized_tools("s1").expect("get");

        // THEN the set contains "bash_executor"
        assert!(tools.contains("bash_executor"));
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn test_authorize_tool_idempotent() {
        // GIVEN a tool already authorized
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Agent,
            Some("agent"),
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");
        repo.authorize_tool("s1", "bash_executor", "2026-03-20T10:01:00Z")
            .expect("first authorize");

        // WHEN we authorize the same tool again
        let result = repo.authorize_tool("s1", "bash_executor", "2026-03-20T10:02:00Z");

        // THEN no error (INSERT OR IGNORE)
        assert!(result.is_ok());
        let tools = repo.get_authorized_tools("s1").expect("get");
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn test_restore_authorizations_from_db() {
        // GIVEN a session with 2 authorized tools persisted
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Agent,
            Some("agent"),
            "",
            &["bash_executor".into(), "file_io".into()],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");
        repo.authorize_tool("s1", "bash_executor", "2026-03-20T10:01:00Z")
            .expect("authorize bash");
        repo.authorize_tool("s1", "file_io", "2026-03-20T10:02:00Z")
            .expect("authorize file_io");

        // WHEN get_authorized_tools("s1")
        let tools = repo.get_authorized_tools("s1").expect("get");

        // THEN HashSet contains both tools
        assert_eq!(tools.len(), 2);
        assert!(tools.contains("bash_executor"));
        assert!(tools.contains("file_io"));
    }

    #[test]
    fn test_get_session_nonexistent() {
        // GIVEN an empty repository
        let repo = ChatSessionRepository::open_in_memory().expect("open");

        // WHEN we query a nonexistent session
        let result = repo.get_session("nonexistent").expect("should not error");

        // THEN we get Ok(None)
        assert!(result.is_none());
    }

    #[test]
    fn test_create_session_with_tools() {
        // GIVEN a repository
        let repo = ChatSessionRepository::open_in_memory().expect("open");

        // WHEN we create a session with available tools
        repo.create_session(
            "s1",
            &ChatMode::Agent,
            Some("reviewer"),
            "Review code.",
            &["bash_executor".to_string(), "file_io".to_string()],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");

        // THEN the tools are stored as JSON
        let session = repo.get_session("s1").expect("get").expect("exists");
        let tools: Vec<String> =
            serde_json::from_str(&session.available_tools).expect("parse tools");
        assert_eq!(tools, vec!["bash_executor", "file_io"]);
    }

    #[test]
    fn test_find_relevant_sessions_with_matches() {
        // GIVEN 3 closed sessions with summaries about different topics
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        for (id, summary, ts) in [
            (
                "s1",
                "Discussion about data migration project using batch processing",
                "2026-03-20T10:00:00Z",
            ),
            (
                "s2",
                "Review of API design for user management endpoints",
                "2026-03-18T10:00:00Z",
            ),
            (
                "s3",
                "Setup of CI/CD pipeline with GitHub Actions",
                "2026-03-15T10:00:00Z",
            ),
        ] {
            repo.create_session(id, &ChatMode::Libre, None, "", &[], ts, None, None)
                .expect("create");
            repo.close_session(id, ts).expect("close");
            repo.update_summary(id, summary).expect("update summary");
        }

        // WHEN searching with a query about data migration
        let results = repo
            .find_relevant_sessions("data migration project batch processing", 3)
            .expect("search");

        // THEN at least one session about data migration is returned
        assert!(!results.is_empty());
        assert!(results.iter().any(|s| s.session_id == "s1"));
        assert!(results.len() <= 3);
    }

    #[test]
    fn test_find_relevant_sessions_no_match() {
        // GIVEN sessions with summaries that do not match the query
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");
        repo.close_session("s1", "2026-03-20T12:00:00Z")
            .expect("close");
        repo.update_summary("s1", "Discussion about cooking recipes and ingredients")
            .expect("update summary");

        // WHEN searching for a completely unrelated topic
        let results = repo
            .find_relevant_sessions("kubernetes cluster deployment strategy", 3)
            .expect("search");

        // THEN no sessions are returned
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_relevant_sessions_empty_query() {
        // GIVEN a repository with sessions
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");
        repo.update_summary("s1", "Some summary text")
            .expect("update");

        // WHEN searching with an empty query
        let results = repo.find_relevant_sessions("", 3).expect("search");

        // THEN no results (sanitized query is empty)
        assert!(results.is_empty());
    }

    #[test]
    fn test_list_recent_summaries() {
        // GIVEN 2 sessions, each with a first user message
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create s1");
        repo.append_message(&AppendMessageParams {
            id: "m1",
            session_id: "s1",
            role: &ChatRole::User,
            content: "Hello from s1",
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-03-20T10:01:00Z",
            metadata: None,
        })
        .expect("append s1");

        repo.create_session(
            "s2",
            &ChatMode::Agent,
            Some("my-agent"),
            "",
            &[],
            "2026-03-20T11:00:00Z",
            None,
            None,
        )
        .expect("create s2");
        repo.append_message(&AppendMessageParams {
            id: "m2",
            session_id: "s2",
            role: &ChatRole::User,
            content: "Hello from s2",
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-03-20T11:01:00Z",
            metadata: None,
        })
        .expect("append s2");

        // WHEN we list recent summaries with limit=10
        let summaries = repo.list_recent_summaries(10).expect("list");

        // THEN we get 2 entries ordered DESC (s2 first because 11:00 > 10:00)
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, "s2");
        assert_eq!(summaries[0].first_message.as_deref(), Some("Hello from s2"));
        assert_eq!(summaries[1].id, "s1");
        assert_eq!(summaries[1].first_message.as_deref(), Some("Hello from s1"));
    }

    #[test]
    fn test_load_session_with_history() {
        // GIVEN a session with 2 messages
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        repo.create_session(
            "s1",
            &ChatMode::Libre,
            None,
            "System prompt",
            &["bash_executor".to_string()],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create");
        repo.append_message(&AppendMessageParams {
            id: "m1",
            session_id: "s1",
            role: &ChatRole::User,
            content: "User message",
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-03-20T10:01:00Z",
            metadata: None,
        })
        .expect("m1");
        repo.append_message(&AppendMessageParams {
            id: "m2",
            session_id: "s1",
            role: &ChatRole::Assistant,
            content: "Assistant reply",
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-03-20T10:02:00Z",
            metadata: None,
        })
        .expect("m2");
        repo.authorize_tool("s1", "bash_executor", "2026-03-20T10:03:00Z")
            .expect("authorize");

        // WHEN we load the session with history
        let session = repo.load_session_with_history("s1").expect("load");

        // THEN the session has 2 messages and 1 authorized tool
        assert_eq!(session.id, "s1");
        assert_eq!(session.history.len(), 2);
        assert_eq!(session.history[0].content, "User message");
        assert_eq!(session.history[1].content, "Assistant reply");
        assert!(session.authorized_tools.contains("bash_executor"));
        assert_eq!(session.system_prompt, "System prompt");
    }

    #[test]
    fn test_load_session_not_found() {
        // GIVEN an empty repository
        let repo = ChatSessionRepository::open_in_memory().expect("open");

        // WHEN we try to load a nonexistent session
        let result = repo.load_session_with_history("nonexistent");

        // THEN we get SessionNotFound
        assert!(matches!(result, Err(ChatError::SessionNotFound(_))));
    }

    /// Helper: create a session and append N numbered messages.
    fn make_session_with_messages(repo: &ChatSessionRepository, session_id: &str, n: usize) {
        repo.create_session(
            session_id,
            &ChatMode::Libre,
            None,
            "System prompt",
            &[],
            "2026-03-20T10:00:00Z",
            None,
            None,
        )
        .expect("create session");

        for i in 1..=n {
            repo.append_message(&AppendMessageParams {
                id: &format!("{session_id}-m{i}"),
                session_id,
                role: &ChatRole::User,
                content: &format!("Message {i}"),
                tool_calls_json: None,
                tool_name: None,
                created_at: "2026-03-20T10:01:00Z",
                metadata: None,
            })
            .expect("append message");
        }
    }

    #[test]
    fn fork_none_copies_full_history() {
        // GIVEN a session with 5 messages
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        make_session_with_messages(&repo, "parent", 5);

        // WHEN fork(None)
        let child = repo
            .create_fork_session("child", "parent", None, "2026-03-20T11:00:00Z")
            .expect("fork");

        // THEN new session with 5 messages
        assert_eq!(child.history.len(), 5);
        assert_eq!(child.parent_session_id.as_deref(), Some("parent"));
        assert_eq!(child.fork_depth, 1);
    }

    #[test]
    fn fork_some_copies_partial_history() {
        // GIVEN a session with 5 messages
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        make_session_with_messages(&repo, "parent", 5);

        // WHEN fork(Some(3))
        let child = repo
            .create_fork_session("child", "parent", Some(3), "2026-03-20T11:00:00Z")
            .expect("fork");

        // THEN new session with 3 messages
        assert_eq!(child.history.len(), 3);
    }

    #[test]
    fn fork_child_is_independent() {
        // GIVEN parent with 5 messages and a fork
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        make_session_with_messages(&repo, "parent", 5);
        repo.create_fork_session("child", "parent", None, "2026-03-20T11:00:00Z")
            .expect("fork");

        // WHEN we add a message to the child
        repo.append_message(&AppendMessageParams {
            id: "child-extra",
            session_id: "child",
            role: &ChatRole::User,
            content: "Extra message in child",
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-03-20T11:01:00Z",
            metadata: None,
        })
        .expect("append");

        // THEN parent message count is unchanged
        let parent = repo
            .load_session_with_history("parent")
            .expect("load parent");
        assert_eq!(parent.history.len(), 5);

        let child = repo.load_session_with_history("child").expect("load child");
        assert_eq!(child.history.len(), 6);
    }

    #[test]
    fn fork_parent_session_id_persisted() {
        // GIVEN parent session
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        make_session_with_messages(&repo, "parent-uuid", 2);

        // WHEN fork()
        let child = repo
            .create_fork_session("child-uuid", "parent-uuid", None, "2026-03-20T11:00:00Z")
            .expect("fork");

        // THEN child.parent_session_id == Some("parent-uuid")
        assert_eq!(child.parent_session_id.as_deref(), Some("parent-uuid"));
    }

    #[test]
    fn fork_list_returns_children() {
        // GIVEN parent with 2 forks
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        make_session_with_messages(&repo, "parent", 3);
        repo.create_fork_session("child-1", "parent", None, "2026-03-20T11:00:00Z")
            .expect("fork 1");
        repo.create_fork_session("child-2", "parent", None, "2026-03-20T11:01:00Z")
            .expect("fork 2");

        // WHEN list_children(parent.id)
        let children = repo.list_children("parent").expect("list children");

        // THEN 2 sessions returned with correct parent_session_id
        assert_eq!(children.len(), 2);
        for child in &children {
            assert_eq!(child.parent_session_id.as_deref(), Some("parent"));
        }
    }
}
