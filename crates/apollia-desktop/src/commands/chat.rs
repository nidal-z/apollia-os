//! Commandes IPC Tauri pour le chat hybride (STORY-204).
//!
//! Chaque commande délègue intégralement au `ChatSessionManagerHandle` —
//! zéro logique métier dans cette couche. Si le chat n'est pas disponible
//! (runtime sans LLM, erreur SQLite), une erreur explicite est retournée.

use apollia_runtime::chat::{
    ChatMode, ChatSession, SessionDetail, SessionInfo, SessionStatus, ToolDecision,
};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Request payload for creating a new chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// Chat mode — `"libre"` or `"agent"`.
    pub mode: String,
    /// Agent name (required when `mode == "agent"`).
    pub agent_name: Option<String>,
    /// Optional system prompt override.
    pub system_prompt: Option<String>,
    /// Tools to make available in this session.
    #[serde(default)]
    pub tools: Vec<String>,
}

/// Summary of a chat session for list responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionSummary {
    /// Session identifier.
    pub id: String,
    /// Chat mode (`"libre"` or `"agent"`).
    pub mode: String,
    /// Agent name (if agent mode).
    pub agent_name: Option<String>,
    /// Current status (`"active"`, `"processing"`, `"closed"`).
    pub status: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

/// Detailed view of a chat session including messages and authorized tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionDetail {
    /// Full session data.
    pub session: ChatSession,
    /// Total number of messages in the session.
    pub message_count: u32,
    /// Tools permanently authorized for this session.
    pub authorized_tools: Vec<String>,
}

/// Creates a new chat session (Libre or Agent mode).
///
/// Delegates to `ChatSessionManagerHandle::create_session()`.
///
/// # Errors
///
/// Returns an error if:
/// - The chat subsystem is not available
/// - The mode string is unrecognized
/// - The agent (for Agent mode) does not exist
#[tauri::command]
pub async fn create_chat_session(
    state: State<'_, RuntimeHandle>,
    request: CreateSessionRequest,
) -> Result<ChatSessionSummary, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let mode = parse_chat_mode(&request.mode)?;

    let info = manager
        .create_session(
            mode,
            request.agent_name,
            request.system_prompt,
            request.tools,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(session_info_to_summary(&info))
}

/// Lists chat sessions, optionally filtered by status.
///
/// Delegates to `ChatSessionManagerHandle::list_sessions()`.
/// Returns an empty list if the chat subsystem is not available.
#[tauri::command]
pub async fn list_chat_sessions(
    state: State<'_, RuntimeHandle>,
    status: Option<String>,
) -> Result<Vec<ChatSessionSummary>, String> {
    let manager = match state.chat_manager.as_ref() {
        Some(m) => m,
        None => return Ok(Vec::new()),
    };

    let status_filter = match status.as_deref() {
        Some(s) => Some(parse_session_status(s)?),
        None => None,
    };

    let sessions = manager.list_sessions(status_filter).await;
    Ok(sessions
        .into_iter()
        .map(|s| session_info_to_summary(&s))
        .collect())
}

/// Gets a single chat session with full message history.
///
/// Delegates to `ChatSessionManagerHandle::get_session()`.
///
/// # Errors
///
/// Returns an error if:
/// - The chat subsystem is not available
/// - The session does not exist
#[tauri::command]
pub async fn get_chat_session(
    state: State<'_, RuntimeHandle>,
    session_id: String,
) -> Result<ChatSessionDetail, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let detail = manager
        .get_session(session_id)
        .await
        .ok_or_else(|| "session not found".to_string())?;

    Ok(session_detail_to_response(detail))
}

/// Closes a chat session.
///
/// Delegates to `ChatSessionManagerHandle::close_session()`.
///
/// # Errors
///
/// Returns an error if:
/// - The chat subsystem is not available
/// - The session does not exist or is already closed
#[tauri::command]
pub async fn close_chat_session(
    state: State<'_, RuntimeHandle>,
    session_id: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .close_session(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Sends a user message and launches the async response generation.
///
/// Returns the `message_id` immediately — the backend processes the message
/// asynchronously and streams tokens via the `"chat-token"` Tauri event.
///
/// # Errors
///
/// Returns an error if:
/// - The chat subsystem is not available
/// - The session does not exist, is closed, or is busy
#[tauri::command]
pub async fn send_chat_message(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    content: String,
) -> Result<String, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let message_id = manager
        .send_message(session_id, content)
        .await
        .map_err(|e| e.to_string())?;

    Ok(message_id)
}

/// Resolves a pending tool approval in a chat session.
///
/// `decision` must be one of: `"accept"`, `"refuse"`, `"always_accept"`.
///
/// # Errors
///
/// Returns an error if:
/// - The chat subsystem is not available
/// - The decision string is unrecognized
/// - The approval has already been resolved or timed out
#[tauri::command]
pub async fn authorize_chat_tool(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    message_id: String,
    tool_name: String,
    decision: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let tool_decision = parse_tool_decision(&decision)?;

    manager
        .resolve_tool(session_id, message_id, tool_name, tool_decision)
        .await
        .map_err(|e| e.to_string())
}

/// Parses a string into a [`ChatMode`].
fn parse_chat_mode(s: &str) -> Result<ChatMode, String> {
    ChatMode::from_sql(s)
        .ok_or_else(|| format!("invalid chat mode: '{s}' (expected 'libre' or 'agent')"))
}

/// Parses a string into a [`SessionStatus`].
fn parse_session_status(s: &str) -> Result<SessionStatus, String> {
    SessionStatus::from_sql(s).ok_or_else(|| {
        format!("invalid session status: '{s}' (expected 'active', 'processing', or 'closed')")
    })
}

/// Parses a string into a [`ToolDecision`].
fn parse_tool_decision(s: &str) -> Result<ToolDecision, String> {
    match s {
        "accept" => Ok(ToolDecision::Accept),
        "refuse" => Ok(ToolDecision::Refuse),
        "always_accept" => Ok(ToolDecision::AlwaysAccept),
        _ => Err(format!(
            "invalid tool decision: '{s}' (expected 'accept', 'refuse', or 'always_accept')"
        )),
    }
}

/// Converts a [`SessionInfo`] into a [`ChatSessionSummary`].
fn session_info_to_summary(info: &SessionInfo) -> ChatSessionSummary {
    ChatSessionSummary {
        id: info.id.clone(),
        mode: info.mode.as_sql().to_string(),
        agent_name: info.agent_name.clone(),
        status: info.status.as_sql().to_string(),
        created_at: info.created_at.clone(),
    }
}

/// Converts a [`SessionDetail`] into a [`ChatSessionDetail`].
fn session_detail_to_response(detail: SessionDetail) -> ChatSessionDetail {
    let authorized_tools: Vec<String> = detail.session.authorized_tools.iter().cloned().collect();
    ChatSessionDetail {
        session: detail.session,
        message_count: detail.message_count,
        authorized_tools,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chat_mode_valid() {
        // GIVEN valid mode strings
        // WHEN parsed
        // THEN correct ChatMode variants are returned
        assert_eq!(parse_chat_mode("libre").unwrap(), ChatMode::Libre);
        assert_eq!(parse_chat_mode("agent").unwrap(), ChatMode::Agent);
    }

    #[test]
    fn test_parse_chat_mode_invalid() {
        // GIVEN an invalid mode string
        // WHEN parsed
        // THEN an error is returned
        let result = parse_chat_mode("invalid");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid chat mode"));
    }

    #[test]
    fn test_parse_session_status_valid() {
        // GIVEN valid status strings
        // WHEN parsed
        // THEN correct SessionStatus variants are returned
        assert_eq!(
            parse_session_status("active").unwrap(),
            SessionStatus::Active
        );
        assert_eq!(
            parse_session_status("processing").unwrap(),
            SessionStatus::Processing
        );
        assert_eq!(
            parse_session_status("closed").unwrap(),
            SessionStatus::Closed
        );
    }

    #[test]
    fn test_parse_session_status_invalid() {
        // GIVEN an invalid status string
        // WHEN parsed
        // THEN an error is returned
        let result = parse_session_status("unknown");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid session status"));
    }

    #[test]
    fn test_parse_tool_decision_valid() {
        // GIVEN valid decision strings
        // WHEN parsed
        // THEN correct ToolDecision variants are returned
        assert_eq!(parse_tool_decision("accept").unwrap(), ToolDecision::Accept);
        assert_eq!(parse_tool_decision("refuse").unwrap(), ToolDecision::Refuse);
        assert_eq!(
            parse_tool_decision("always_accept").unwrap(),
            ToolDecision::AlwaysAccept
        );
    }

    #[test]
    fn test_parse_tool_decision_invalid() {
        // GIVEN an invalid decision string
        // WHEN parsed
        // THEN an error is returned
        let result = parse_tool_decision("maybe");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid tool decision"));
    }

    #[test]
    fn test_create_session_request_deserialize() {
        // GIVEN a JSON payload with all fields
        let json = serde_json::json!({
            "mode": "agent",
            "agent_name": "review-agent",
            "system_prompt": "You are a code reviewer.",
            "tools": ["bash_executor", "file_io"]
        });
        // WHEN deserialized
        let req: CreateSessionRequest = serde_json::from_value(json).expect("deserialize");
        // THEN all fields are correct
        assert_eq!(req.mode, "agent");
        assert_eq!(req.agent_name.as_deref(), Some("review-agent"));
        assert_eq!(
            req.system_prompt.as_deref(),
            Some("You are a code reviewer.")
        );
        assert_eq!(req.tools, vec!["bash_executor", "file_io"]);
    }

    #[test]
    fn test_create_session_request_deserialize_minimal() {
        // GIVEN a minimal JSON payload (tools defaults to empty)
        let json = serde_json::json!({ "mode": "libre" });
        // WHEN deserialized
        let req: CreateSessionRequest = serde_json::from_value(json).expect("deserialize");
        // THEN defaults are applied
        assert_eq!(req.mode, "libre");
        assert!(req.agent_name.is_none());
        assert!(req.system_prompt.is_none());
        assert!(req.tools.is_empty());
    }

    #[test]
    fn test_chat_session_summary_roundtrip() {
        // GIVEN a ChatSessionSummary
        let summary = ChatSessionSummary {
            id: "sess-42".into(),
            mode: "libre".into(),
            agent_name: None,
            status: "active".into(),
            created_at: "2026-03-20T10:00:00Z".into(),
        };
        // WHEN serialized and deserialized
        let json = serde_json::to_string(&summary).expect("serialize");
        let restored: ChatSessionSummary = serde_json::from_str(&json).expect("deserialize");
        // THEN identical
        assert_eq!(restored.id, "sess-42");
        assert_eq!(restored.mode, "libre");
        assert!(restored.agent_name.is_none());
        assert_eq!(restored.status, "active");
    }

    #[test]
    fn test_session_info_to_summary_conversion() {
        // GIVEN a SessionInfo from the runtime
        let info = SessionInfo {
            id: "sess-1".into(),
            mode: ChatMode::Agent,
            agent_name: Some("test-agent".into()),
            status: SessionStatus::Processing,
            created_at: "2026-03-20T10:00:00Z".into(),
        };
        // WHEN converted to summary
        let summary = session_info_to_summary(&info);
        // THEN fields are correctly mapped
        assert_eq!(summary.id, "sess-1");
        assert_eq!(summary.mode, "agent");
        assert_eq!(summary.agent_name.as_deref(), Some("test-agent"));
        assert_eq!(summary.status, "processing");
    }
}
