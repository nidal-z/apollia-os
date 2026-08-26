//! Session hydration and forking.
//!
//! Rebuilds a full [`ChatSession`] from its rows, and derives a child session
//! that replays its parent's history up to a chosen message.

use rusqlite::params;

use crate::chat::repository::{row_to_session, ChatSessionRepository, MessageRow, SessionRow};
use crate::chat::types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, PlanPhase, SessionStatus,
    ToolCallRecord,
};

impl ChatSessionRepository {
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
        let plan_phase = PlanPhase::from_sql(&row.plan_phase).ok_or_else(|| {
            ChatError::InternalError(format!("unknown plan_phase: {}", row.plan_phase))
        })?;
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
            plan_mode: row.plan_mode,
            plan_phase,
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

        // Read the parent messages before opening the transaction: the copy must
        // be all-or-nothing so a crash cannot leave a half-forked child.
        let parent_messages = self.get_messages(parent_id, None)?;
        let message_slice: &[MessageRow] = match up_to_count {
            Some(n) => {
                let end = n.min(parent_messages.len());
                &parent_messages[..end]
            }
            None => &parent_messages,
        };

        // SAFETY: single writer behind `Arc<Mutex<ChatSessionRepository>>`, so
        // `unchecked_transaction` over `&self` is sound; the child row and every
        // copied message commit together or not at all.
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| ChatError::InternalError(format!("create_fork_session begin: {e}")))?;

        // Persist child session row (inherits parent's project_id).
        tx.execute(
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
        for (idx, msg) in message_slice.iter().enumerate() {
            let new_msg_id = uuid::Uuid::new_v4().to_string();
            let seq = (idx + 1) as u32;
            tx.execute(
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

        tx.commit()
            .map_err(|e| ChatError::InternalError(format!("create_fork_session commit: {e}")))?;

        // Reload the child with full history (after commit, so it reflects the
        // persisted state).
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
                        closed_at, llm_backend, summary, title, parent_session_id, fork_depth, project_id,
                        plan_mode, plan_phase
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
}
