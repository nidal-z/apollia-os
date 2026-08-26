//! Tauri IPC commands for the hybrid chat.
//!
//! Each command delegates entirely to the `ChatSessionManagerHandle`; no
//! business logic in this layer. If the chat is unavailable (runtime without an
//! LLM, SQLite error), an explicit error is returned.

use apollia_runtime::chat::{
    ChatMode, SessionDetail, SessionInfo, SessionMetrics, SessionStatus, ToolDecision,
};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Automatic session naming lives in `title`, the on-disk export in `export`.
pub mod export;
pub mod title;

/// Request payload for creating a new chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// Chat mode - `"libre"` or `"agent"`.
    pub mode: String,
    /// Agent name (required when `mode == "agent"`).
    pub agent_name: Option<String>,
    /// Optional system prompt override.
    pub system_prompt: Option<String>,
    /// Tools to make available in this session.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Project to link this session to (None = standalone).
    pub project_id: Option<String>,
}

/// Summary of a chat session for list responses (flat structure for frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionSummary {
    pub id: String,
    pub mode: String,
    pub agent_name: Option<String>,
    pub status: String,
    pub last_message_preview: Option<String>,
    pub message_count: u32,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub title: Option<String>,
    pub project_id: Option<String>,
}

/// Detailed view of a chat session (flat structure for frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionDetail {
    pub id: String,
    pub mode: String,
    pub agent_name: Option<String>,
    pub system_prompt: String,
    pub status: String,
    pub available_tools: Vec<String>,
    pub authorized_tools: Vec<String>,
    pub messages: Vec<ChatMessageView>,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub llm_backend: Option<String>,
    pub title: Option<String>,
    pub project_id: Option<String>,
    /// Whether the session runs in first-class conversational plan mode.
    pub plan_mode: bool,
    /// Current plan-mode lifecycle phase as a stable lowercase string
    /// (`discovery`, `drafting`, `awaiting_approval`, `executing`, `done`).
    pub plan_phase: String,
}

/// Request payload for updating session configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionRequest {
    pub system_prompt: Option<String>,
    pub tools: Option<Vec<String>>,
    pub llm_backend: Option<Option<String>>,
}

/// Individual message view for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageView {
    pub id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<serde_json::Value>>,
    pub tool_name: Option<String>,
    pub seq: u32,
    pub created_at: String,
    /// Optional key-value metadata (e.g. cross-session markers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Creates a new chat session (Libre or Agent mode).
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
        .create_session(apollia_runtime::chat::manager::CreateSessionParams {
            mode,
            agent_name: request.agent_name,
            system_prompt: request.system_prompt,
            tools: request.tools,
            project_id: request.project_id,
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(session_info_to_summary(&info))
}

/// Lists chat sessions, optionally filtered by status.
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

    Ok(session_detail_to_flat(detail))
}

/// Returns aggregated session metrics for the metrics panel.
///
/// The backend accumulates metrics in-memory on each exchange. Returns a
/// zero-filled default when no exchange has yet completed so the UI can
/// render the panel with empty values instead of an error.
#[tauri::command]
pub async fn chat_session_metrics(
    state: State<'_, RuntimeHandle>,
    session_id: String,
) -> Result<SessionMetrics, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    Ok(manager
        .get_session_metrics(session_id.clone())
        .await
        .unwrap_or_else(|| SessionMetrics::new(session_id)))
}

/// Updates session configuration (instructions, tools, LLM provider).
#[tauri::command]
pub async fn update_chat_session(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    update: UpdateSessionRequest,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .update_session(
            session_id,
            update.system_prompt,
            update.tools,
            update.llm_backend,
        )
        .await
        .map_err(|e| e.to_string())
}

/// Deletes a chat session and all its messages.
#[tauri::command]
pub async fn delete_chat_session(
    state: State<'_, RuntimeHandle>,
    session_id: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .delete_session(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Renames a chat session (sets a user-defined title).
#[tauri::command]
pub async fn rename_chat_session(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("title must not be empty".to_string());
    }
    let cleaned: String = trimmed.chars().take(100).collect();

    manager
        .rename_session(session_id, cleaned)
        .await
        .map_err(|e| e.to_string())
}

/// Sends a user message and launches the async response generation.
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

/// Regenerates the assistant reply to the last user turn (truncate-in-place).
///
/// `message_id` is the assistant message to regenerate. The turn re-runs in the
/// same session. Rejected while a turn is already in flight.
#[tauri::command]
pub async fn regenerate_chat_response(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    message_id: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .regenerate_response(session_id, message_id)
        .await
        .map_err(|e| e.to_string())
}

/// Replaces a user message and re-runs the conversation from it.
///
/// `message_id` is the user message to edit; `content` is its new text. Returns
/// the id of the newly appended user message. Rejected while a turn is in flight.
#[tauri::command]
pub async fn edit_and_resend_chat_message(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    message_id: String,
    content: String,
) -> Result<String, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .edit_and_resend(session_id, message_id, content)
        .await
        .map_err(|e| e.to_string())
}

/// Requests a cooperative pause of the in-flight turn for a session.
///
/// The ReAct loop stops at its next checkpoint, the partial assistant message
/// is persisted, and no `ChatResponseCompleted` event is emitted. If no turn is
/// active this is a no-op. The frontend finalizes the streamed partial on ack.
#[tauri::command]
pub async fn pause_chat_session(
    state: State<'_, RuntimeHandle>,
    session_id: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .pause_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Resolves a pending tool approval in a chat session.
///
/// `reason` is forwarded to the agent when `decision == "refuse"` so it can
/// adapt its plan; it is ignored for `accept`. `scope` is only meaningful for
/// `always_accept` and defaults to [`AlwaysAcceptScope::ThisSession`] - the
/// safest sticky option (cf. `ApprovalCardV2`).
#[tauri::command]
// Tauri command: the session/message/tool identifiers plus the decision, reason
// and scope exceed 5 by design; they map one-to-one onto the IPC call.
// REASON: Tauri command: each parameter is one invoke key or injected State; a struct would change the IPC contract.
#[allow(clippy::too_many_arguments)]
pub async fn authorize_chat_tool(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    message_id: String,
    tool_call_id: Option<String>,
    tool_name: String,
    decision: String,
    reason: Option<String>,
    scope: Option<apollia_runtime::chat::AlwaysAcceptScope>,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let tool_decision = parse_tool_decision(&decision, reason, scope)?;

    // Fall back to the tool name when the caller omits the id (matches the
    // pre-correlation key), so single-approval turns keep resolving.
    let tool_call_id = tool_call_id.unwrap_or_else(|| tool_name.clone());
    manager
        .resolve_tool(
            session_id,
            message_id,
            tool_call_id,
            tool_name,
            tool_decision,
        )
        .await
        .map_err(|e| e.to_string())
}

/// Deliver user answers to a pending `ask_user` tool request.
#[tauri::command]
pub async fn respond_user_input(
    state: State<'_, RuntimeHandle>,
    request_id: String,
    answers: Vec<apollia_tools::tools::ask_user::UserAnswer>,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .resolve_user_input(request_id, answers)
        .await
        .map_err(|e| e.to_string())
}

/// Reject a pending `ask_user` request with a mandatory reason.
#[tauri::command]
pub async fn respond_user_input_rejected(
    state: State<'_, RuntimeHandle>,
    request_id: String,
    reason: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .reject_user_input(request_id, reason)
        .await
        .map_err(|e| e.to_string())
}

/// List all currently pending `ask_user` requests (for inbox / reconnection).
#[tauri::command]
pub async fn list_pending_user_inputs(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<apollia_runtime::chat::manager::PendingUserInputView>, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .list_pending_user_inputs()
        .await
        .map_err(|e| e.to_string())
}

/// List recently resolved chat tool approvals for the history view.
#[tauri::command]
pub async fn list_chat_approval_history(
    state: State<'_, RuntimeHandle>,
    limit: Option<i64>,
    days: Option<i64>,
) -> Result<Vec<ResolvedChatApproval>, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let rows = manager
        .list_approval_history(limit.unwrap_or(20), days.unwrap_or(7))
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| ResolvedChatApproval {
            session_id: r.session_id,
            message_id: r.message_id,
            tool_name: r.tool_name,
            decision: r.decision,
            resolved_at: r.resolved_at,
            reason: r.reason,
        })
        .collect())
}

/// Resolved chat tool approval entry for the UI history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedChatApproval {
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

/// Links or unlinks a chat session to/from a project.
///
/// Pass `project_id = null` to unlink.
#[tauri::command]
pub async fn link_chat_to_project(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    project_id: Option<String>,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .link_session_to_project(session_id, project_id)
        .await
        .map_err(|e| e.to_string())
}

/// Lists chat sessions belonging to a specific project.
#[tauri::command]
pub async fn list_chats_by_project(
    state: State<'_, RuntimeHandle>,
    project_id: String,
) -> Result<Vec<ChatSessionSummary>, String> {
    let manager = match state.chat_manager.as_ref() {
        Some(m) => m,
        None => return Ok(Vec::new()),
    };

    let sessions = manager.list_sessions_by_project(project_id).await;
    Ok(sessions
        .into_iter()
        .map(|s| session_info_to_summary(&s))
        .collect())
}

// ─── Internal helpers ────────────────────────────────────────────────────────

fn parse_chat_mode(s: &str) -> Result<ChatMode, String> {
    ChatMode::from_sql(s)
        .ok_or_else(|| format!("invalid chat mode: '{s}' (expected 'libre' or 'agent')"))
}

fn parse_session_status(s: &str) -> Result<SessionStatus, String> {
    SessionStatus::from_sql(s).ok_or_else(|| {
        format!("invalid session status: '{s}' (expected 'active', 'processing', or 'closed')")
    })
}

fn parse_tool_decision(
    s: &str,
    reason: Option<String>,
    scope: Option<apollia_runtime::chat::AlwaysAcceptScope>,
) -> Result<ToolDecision, String> {
    use apollia_runtime::chat::AlwaysAcceptScope;
    match s {
        "accept" => Ok(ToolDecision::Accept),
        "refuse" => Ok(ToolDecision::Refuse { reason }),
        "always_accept" => Ok(ToolDecision::AlwaysAccept {
            scope: scope.unwrap_or_else(AlwaysAcceptScope::safe_default),
        }),
        _ => Err(format!(
            "invalid tool decision: '{s}' (expected 'accept', 'refuse', or 'always_accept')"
        )),
    }
}

/// Converts a [`SessionInfo`] into a flat [`ChatSessionSummary`].
///
/// `SessionInfo` is lightweight (no messages loaded), so `message_count`
/// defaults to 0 and `last_message_preview` to `None`.
fn session_info_to_summary(info: &SessionInfo) -> ChatSessionSummary {
    ChatSessionSummary {
        id: info.id.clone(),
        mode: info.mode.as_sql().to_string(),
        agent_name: info.agent_name.clone(),
        status: info.status.as_sql().to_string(),
        last_message_preview: None,
        message_count: 0,
        created_at: info.created_at.clone(),
        closed_at: None,
        title: info.title.clone(),
        project_id: info.project_id.clone(),
    }
}

/// Converts a [`SessionDetail`] into a flat [`ChatSessionDetail`].
fn session_detail_to_flat(detail: SessionDetail) -> ChatSessionDetail {
    let session = detail.session;
    let messages: Vec<ChatMessageView> = session
        .history
        .iter()
        .map(|m| ChatMessageView {
            id: m.id.clone(),
            role: role_to_string(&m.role),
            content: m.content.clone(),
            tool_calls: m.tool_calls.as_ref().map(|tc| {
                tc.iter()
                    .map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null))
                    .collect()
            }),
            tool_name: m.tool_name.clone(),
            seq: m.seq,
            created_at: m.created_at.clone(),
            metadata: m.metadata.clone(),
        })
        .collect();

    let authorized_tools: Vec<String> = session.authorized_tools.into_iter().collect();

    ChatSessionDetail {
        id: session.id,
        mode: session.mode.as_sql().to_string(),
        agent_name: session.agent_name,
        system_prompt: session.system_prompt,
        status: session.status.as_sql().to_string(),
        available_tools: session.available_tools,
        authorized_tools,
        messages,
        created_at: session.created_at,
        closed_at: None,
        llm_backend: session.llm_backend,
        title: session.title,
        project_id: session.project_id,
        plan_mode: session.plan_mode,
        plan_phase: session.plan_phase.as_sql().to_string(),
    }
}

/// Flat view of an A2A skill for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ASkillView {
    pub skill_id: String,
    pub agent_name: String,
    pub skill_name: String,
    pub description: String,
}

/// Lists all A2A skills available from active worker agents.
///
/// Returns an empty list when A2A is not wired or no workers are active.
#[tauri::command]
pub async fn list_a2a_skills(state: State<'_, RuntimeHandle>) -> Result<Vec<A2ASkillView>, String> {
    let manager = match state.chat_manager.as_ref() {
        Some(m) => m,
        None => return Ok(Vec::new()),
    };
    let skills = manager.list_a2a_skills().await;
    Ok(skills
        .into_iter()
        .map(|s| A2ASkillView {
            skill_id: s.skill_id,
            agent_name: s.agent_name,
            skill_name: s.skill_name,
            description: s.description,
        })
        .collect())
}

/// Convert a [`ChatRole`] to its string representation.
fn role_to_string(role: &apollia_runtime::chat::ChatRole) -> String {
    match role {
        apollia_runtime::chat::ChatRole::User => "user".to_string(),
        apollia_runtime::chat::ChatRole::Assistant => "assistant".to_string(),
        apollia_runtime::chat::ChatRole::System => "system".to_string(),
        apollia_runtime::chat::ChatRole::Tool => "tool".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chat_mode_valid() {
        assert_eq!(parse_chat_mode("libre").unwrap(), ChatMode::Libre);
        assert_eq!(parse_chat_mode("agent").unwrap(), ChatMode::Agent);
    }

    #[test]
    fn test_parse_chat_mode_invalid() {
        let result = parse_chat_mode("invalid");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid chat mode"));
    }

    #[test]
    fn test_parse_session_status_valid() {
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
        let result = parse_session_status("unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tool_decision_valid() {
        use apollia_runtime::chat::AlwaysAcceptScope;
        assert_eq!(
            parse_tool_decision("accept", None, None).unwrap(),
            ToolDecision::Accept
        );
        assert_eq!(
            parse_tool_decision("refuse", None, None).unwrap(),
            ToolDecision::Refuse { reason: None }
        );
        assert_eq!(
            parse_tool_decision("always_accept", None, None).unwrap(),
            ToolDecision::AlwaysAccept {
                scope: AlwaysAcceptScope::ThisSession
            }
        );
    }

    #[test]
    fn test_parse_tool_decision_carries_reason_and_scope() {
        use apollia_runtime::chat::AlwaysAcceptScope;
        // GIVEN operator rejects with a typed reason
        let d = parse_tool_decision("refuse", Some("out of scope".into()), None).unwrap();
        assert_eq!(
            d,
            ToolDecision::Refuse {
                reason: Some("out of scope".into())
            }
        );

        // GIVEN operator picks a project-wide always-accept
        let d = parse_tool_decision("always_accept", None, Some(AlwaysAcceptScope::ThisProject))
            .unwrap();
        assert_eq!(
            d,
            ToolDecision::AlwaysAccept {
                scope: AlwaysAcceptScope::ThisProject
            }
        );
    }

    #[test]
    fn test_parse_tool_decision_invalid() {
        let result = parse_tool_decision("maybe", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_session_request_deserialize() {
        let json = serde_json::json!({
            "mode": "agent",
            "agent_name": "review-agent",
            "system_prompt": "You are a code reviewer.",
            "tools": ["bash_executor", "file_io"]
        });
        let req: CreateSessionRequest = serde_json::from_value(json).expect("deserialize");
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
        let json = serde_json::json!({ "mode": "libre" });
        let req: CreateSessionRequest = serde_json::from_value(json).expect("deserialize");
        assert_eq!(req.mode, "libre");
        assert!(req.agent_name.is_none());
        assert!(req.system_prompt.is_none());
        assert!(req.tools.is_empty());
    }

    #[test]
    fn test_chat_session_summary_roundtrip() {
        let summary = ChatSessionSummary {
            id: "sess-42".into(),
            mode: "libre".into(),
            agent_name: None,
            status: "active".into(),
            last_message_preview: None,
            message_count: 0,
            created_at: "2026-03-20T10:00:00Z".into(),
            closed_at: None,
            title: None,
            project_id: None,
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        let restored: ChatSessionSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.id, "sess-42");
        assert_eq!(restored.mode, "libre");
        assert!(restored.agent_name.is_none());
        assert_eq!(restored.status, "active");
        assert_eq!(restored.message_count, 0);
    }

    #[test]
    fn test_session_info_to_summary_conversion() {
        let info = SessionInfo {
            id: "sess-1".into(),
            mode: ChatMode::Agent,
            agent_name: Some("test-agent".into()),
            status: SessionStatus::Processing,
            created_at: "2026-03-20T10:00:00Z".into(),
            title: None,
            project_id: None,
        };
        let summary = session_info_to_summary(&info);
        assert_eq!(summary.id, "sess-1");
        assert_eq!(summary.mode, "agent");
        assert_eq!(summary.agent_name.as_deref(), Some("test-agent"));
        assert_eq!(summary.status, "processing");
        assert_eq!(summary.message_count, 0);
    }
}
