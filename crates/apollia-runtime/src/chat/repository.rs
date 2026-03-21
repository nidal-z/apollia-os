//! SQLite-backed repository for chat sessions and messages (STORY-198).
//!
//! Operates on `~/.apollia/chat.db` — one database per runtime instance.
//! All writes are synchronous (rusqlite); callers should wrap in
//! `spawn_blocking` for async contexts.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection};

use super::types::{ChatError, ChatMode, ChatRole, SessionStatus};

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

        Ok(Self { conn })
    }

    /// Open an in-memory database (for tests).
    #[cfg(test)]
    fn open_in_memory() -> Result<Self, ChatError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ChatError::InternalError(format!("in-memory open failed: {e}")))?;

        conn.execute_batch(MIGRATION_SQL)
            .map_err(|e| ChatError::InternalError(format!("migration failed: {e}")))?;

        Ok(Self { conn })
    }

    /// Persist a new chat session.
    pub fn create_session(
        &self,
        id: &str,
        mode: &ChatMode,
        agent_name: Option<&str>,
        system_prompt: &str,
        available_tools: &[String],
        created_at: &str,
        llm_backend: Option<&str>,
    ) -> Result<(), ChatError> {
        let tools_json = serde_json::to_string(available_tools)
            .map_err(|e| ChatError::InternalError(format!("tools serialization: {e}")))?;

        self.conn
            .execute(
                "INSERT INTO chat_sessions (id, mode, agent_name, system_prompt, available_tools, created_at, llm_backend)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, mode.as_sql(), agent_name, system_prompt, tools_json, created_at, llm_backend],
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

    /// Retrieve a session by ID, or `None` if not found.
    pub fn get_session(&self, id: &str) -> Result<Option<SessionRow>, ChatError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at, closed_at, llm_backend
                 FROM chat_sessions WHERE id = ?1",
            )
            .map_err(|e| ChatError::InternalError(format!("get_session prepare: {e}")))?;

        let row = stmt
            .query_row(params![id], |row| {
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
                })
            })
            .optional()
            .map_err(|e| ChatError::InternalError(format!("get_session query: {e}")))?;

        Ok(row)
    }

    /// List sessions, optionally filtered by status.
    pub fn list_sessions(&self, status: Option<&str>) -> Result<Vec<SessionRow>, ChatError> {
        let (sql, param): (&str, Option<&str>) = match status {
            Some(s) => (
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at, closed_at, llm_backend
                 FROM chat_sessions WHERE status = ?1 ORDER BY created_at DESC",
                Some(s),
            ),
            None => (
                "SELECT id, mode, agent_name, system_prompt, status, available_tools, created_at, closed_at, llm_backend
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
                "INSERT INTO chat_messages (id, session_id, role, content, tool_calls_json, tool_name, created_at, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    params.id,
                    params.session_id,
                    params.role.as_sql(),
                    params.content,
                    params.tool_calls_json,
                    params.tool_name,
                    params.created_at,
                    next_seq
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
                        "SELECT id, session_id, role, content, tool_calls_json, tool_name, created_at, seq
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
                        "SELECT id, session_id, role, content, tool_calls_json, tool_name, created_at, seq
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
    })
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
        )
        .expect("create");

        // THEN the tools are stored as JSON
        let session = repo.get_session("s1").expect("get").expect("exists");
        let tools: Vec<String> =
            serde_json::from_str(&session.available_tools).expect("parse tools");
        assert_eq!(tools, vec!["bash_executor", "file_io"]);
    }
}
