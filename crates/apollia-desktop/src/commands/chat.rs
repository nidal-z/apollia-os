//! Tauri IPC commands for the hybrid chat.
//!
//! Each command delegates entirely to the `ChatSessionManagerHandle`; no
//! business logic in this layer. If the chat is unavailable (runtime without an
//! LLM, SQLite error), an explicit error is returned.

use apollia_runtime::chat::{
    ChatMode, SessionDetail, SessionInfo, SessionMetrics, SessionStatus, ToolDecision,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// Automatic session naming lives in `naming`, the on-disk export in `export`,
/// the daemon routes of the conversations in `conversations`, the images of
/// the chat in `images`.
pub(crate) mod conversations;
pub mod export;
pub mod folders;
pub mod images;
pub mod naming;
mod remote;
pub mod targets;
pub mod trash;
pub mod turn_calls;

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
    /// The model chosen for the session: the identifier of a language model
    /// of the model index, or `<server>/<model>`. Absent keeps the model of
    /// the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    /// Whether the session runs in first-class conversational plan mode.
    pub plan_mode: bool,
    /// Current plan-mode lifecycle phase as a stable lowercase string
    /// (`discovery`, `drafting`, `awaiting_approval`, `executing`, `done`).
    pub plan_phase: String,
    /// Profile the conversation designates; `None` follows the default.
    pub profile_id: Option<String>,
    /// Profile switch of the conversation: whether the built-in assistant
    /// receives the profile in its prompt.
    pub use_profile: bool,
}

/// Request payload for updating session configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionRequest {
    pub system_prompt: Option<String>,
    pub tools: Option<Vec<String>>,
    pub llm_backend: Option<Option<String>>,
    /// Profile of the conversation: absent keeps it, `null` returns the
    /// conversation to the current default profile.
    #[serde(
        default,
        deserialize_with = "apollia_runtime::api::routes_chat::conversations::present_or_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub profile_id: Option<Option<String>>,
    /// Profile switch of the conversation; absent keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_profile: Option<bool>,
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
    app: tauri::AppHandle,
    link: State<'_, RuntimeLink>,
    request: CreateSessionRequest,
) -> Result<ChatSessionSummary, String> {
    let mode = parse_chat_mode(&request.mode)?;
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            let info = remote.create_session(&request).await?;
            return Ok(session_info_to_summary(&info));
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    let model = match request.model.filter(|m| !m.trim().is_empty()) {
        Some(model) => Some(targets::embedded_session_route(&app, state, model).await),
        None => None,
    };

    let info = manager
        .create_session(apollia_runtime::chat::manager::CreateSessionParams {
            mode,
            agent_name: request.agent_name,
            system_prompt: request.system_prompt,
            tools: request.tools,
            model,
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(session_info_to_summary(&info))
}

/// Lists chat sessions, optionally filtered by status.
#[tauri::command]
pub async fn list_chat_sessions(
    link: State<'_, RuntimeLink>,
    status: Option<String>,
) -> Result<Vec<ChatSessionSummary>, String> {
    let status_filter = match status.as_deref() {
        Some(s) => Some(parse_session_status(s)?),
        None => None,
    };
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            let sessions = remote
                .list_sessions(status_filter.as_ref().map(SessionStatus::as_sql))
                .await?;
            return Ok(sessions.iter().map(session_info_to_summary).collect());
        }
    };
    let manager = match state.chat_manager.as_ref() {
        Some(m) => m,
        None => return Ok(Vec::new()),
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
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<ChatSessionDetail, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return Ok(session_detail_to_flat(
                remote.get_session(&session_id).await?,
            ));
        }
    };
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
/// render the panel with empty values instead of an error. Attached, the
/// daemon answers the same record over [`metrics_path`].
#[tauri::command]
pub async fn chat_session_metrics(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<SessionMetrics, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => return Ok(remote.get(&metrics_path(&session_id)).await?),
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    Ok(manager.session_metrics_or_empty(session_id).await)
}

/// The daemon route of the metrics of `session_id`.
pub(crate) fn metrics_path(session_id: &str) -> String {
    format!("/api/v1/sessions/{}/metrics", segment(session_id))
}

/// Updates session configuration (instructions, tools, LLM provider).
#[tauri::command]
pub async fn update_chat_session(
    app: tauri::AppHandle,
    link: State<'_, RuntimeLink>,
    session_id: String,
    update: UpdateSessionRequest,
) -> Result<(), String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return conversations::update(remote, &session_id, &update).await;
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    let llm_backend = match update.llm_backend {
        Some(Some(model)) if !model.trim().is_empty() => Some(Some(
            targets::embedded_session_route(&app, state, model).await,
        )),
        other => other,
    };

    manager
        .update_session(
            session_id,
            apollia_runtime::chat::SessionConfigUpdate {
                system_prompt: update.system_prompt,
                available_tools: update.tools,
                llm_backend,
                profile_id: update.profile_id,
                use_profile: update.use_profile,
            },
        )
        .await
        .map_err(|e| e.to_string())
}

/// Deletes a chat session and all its messages, for good: nothing goes to
/// the trash. `trash::trash_chat_session` is the removal one comes back from.
#[tauri::command]
pub async fn delete_chat_session(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<(), String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => return conversations::erase(remote, &session_id).await,
    };
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
    link: State<'_, RuntimeLink>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return conversations::rename(remote, &session_id, &title).await;
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    apollia_runtime::chat::title::rename_session(manager, session_id, &title)
        .await
        .map_err(|e| e.to_string())
}

/// Sends a user message and launches the async response generation.
///
/// `images` travel after the text, for a model that reads images; the runtime
/// refuses them when the model of the conversation does not. `documents` are
/// read by the runtime on reception: their text joins the message.
#[tauri::command]
pub async fn send_chat_message(
    link: State<'_, RuntimeLink>,
    session_id: String,
    content: String,
    images: Option<Vec<apollia_runtime::chat::images::ImageAttachment>>,
    documents: Option<Vec<apollia_runtime::chat::documents::DocumentAttachment>>,
) -> Result<String, String> {
    let images = images.unwrap_or_default();
    let documents = documents.unwrap_or_default();
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return Ok(remote
                .send_message(&session_id, &content, &images, &documents)
                .await?);
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let decoded =
        apollia_runtime::chat::images::decode_attachments(&images).map_err(|e| e.to_string())?;
    let decoded_documents = apollia_runtime::chat::documents::decode_attachments(&documents)
        .map_err(|e| e.to_string())?;
    let message_id = manager
        .send_message_with_attachments(session_id, content, decoded, decoded_documents)
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
    link: State<'_, RuntimeLink>,
    session_id: String,
    message_id: String,
) -> Result<(), String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return Ok(remote::regenerate(remote, &session_id, &message_id).await?);
        }
    };
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
    link: State<'_, RuntimeLink>,
    session_id: String,
    message_id: String,
    content: String,
) -> Result<String, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return Ok(remote::edit_and_resend(remote, &session_id, &message_id, &content).await?);
        }
    };
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
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<(), String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => return Ok(remote.pause_session(&session_id).await?),
    };
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
/// adapt its next step; it is ignored otherwise. `always_accept` takes no
/// scope: it grants the asking agent the action family of the tool.
#[tauri::command]
// Tauri command: the session/message/tool identifiers plus the decision and
// reason exceed 5 by design; they map one-to-one onto the IPC call.
// REASON: Tauri command: each parameter is one invoke key or injected State; a struct would change the IPC contract.
#[allow(clippy::too_many_arguments)]
pub async fn authorize_chat_tool(
    link: State<'_, RuntimeLink>,
    session_id: String,
    message_id: String,
    tool_call_id: Option<String>,
    tool_name: String,
    decision: String,
    reason: Option<String>,
) -> Result<(), String> {
    let tool_decision = parse_tool_decision(&decision, reason.clone())?;

    // Fall back to the tool name when the caller omits the id (matches the
    // pre-correlation key), so single-approval turns keep resolving.
    let tool_call_id = tool_call_id.unwrap_or_else(|| tool_name.clone());
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            let body = remote::AuthorizeBody {
                message_id,
                tool_call_id,
                tool_name,
                decision,
                reason,
            };
            return Ok(remote::authorize(remote, &session_id, &body).await?);
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
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
    link: State<'_, RuntimeLink>,
    request_id: String,
    answers: Vec<apollia_tools::tools::ask_user::UserAnswer>,
) -> Result<(), String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            let path = format!("/api/v1/user-inputs/{}/answer", segment(&request_id));
            let body = serde_json::json!({ "answers": answers });
            let _: serde_json::Value = remote.post(&path, &body).await?;
            return Ok(());
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .resolve_user_input(request_id, answers)
        .await
        .map_err(|e| e.to_string())
}

/// List all currently pending `ask_user` requests (for inbox / reconnection).
#[tauri::command]
pub async fn list_pending_user_inputs(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<apollia_runtime::chat::manager::PendingUserInputView>, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => return Ok(remote.get("/api/v1/user-inputs").await?),
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    manager
        .list_pending_user_inputs()
        .await
        .map_err(|e| e.to_string())
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

fn parse_tool_decision(s: &str, reason: Option<String>) -> Result<ToolDecision, String> {
    match s {
        "accept" => Ok(ToolDecision::Accept),
        "refuse" => Ok(ToolDecision::Refuse { reason }),
        "always_accept" => Ok(ToolDecision::AlwaysAccept),
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
        // The instructions of the conversation, under the name the
        // configuration panel reads them by.
        system_prompt: session.settings.instructions.unwrap_or_default(),
        status: session.status.as_sql().to_string(),
        available_tools: session.available_tools,
        authorized_tools,
        messages,
        created_at: session.created_at,
        closed_at: None,
        llm_backend: session.llm_backend,
        title: session.title,
        plan_mode: session.plan_mode,
        plan_phase: session.plan_phase.as_sql().to_string(),
        profile_id: session.profile_id,
        use_profile: session.use_profile,
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
pub async fn list_a2a_skills(link: State<'_, RuntimeLink>) -> Result<Vec<A2ASkillView>, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => return Ok(remote::a2a_skills(remote).await?),
    };
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
        // GIVEN the two chat mode names the front end sends
        // WHEN each is parsed
        // THEN it maps to its mode
        assert_eq!(parse_chat_mode("libre").unwrap(), ChatMode::Libre);
        assert_eq!(parse_chat_mode("agent").unwrap(), ChatMode::Agent);
    }

    #[test]
    fn test_parse_chat_mode_invalid() {
        // GIVEN a mode name that does not exist
        // WHEN it is parsed
        let result = parse_chat_mode("invalid");
        // THEN the call is rejected with a message naming the offending value
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid chat mode"));
    }

    #[test]
    fn test_parse_session_status_valid() {
        // GIVEN the three session status names the front end sends
        // WHEN each is parsed
        // THEN it maps to its status
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
        // GIVEN a status name that does not exist
        // WHEN it is parsed
        let result = parse_session_status("unknown");
        // THEN the call is rejected instead of defaulting to a status
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tool_decision_valid() {
        // GIVEN the three decision names an approval card can send
        // WHEN each is parsed without a reason
        // THEN each comes back as its decision, always-accept with no scope to pick
        assert_eq!(
            parse_tool_decision("accept", None).unwrap(),
            ToolDecision::Accept
        );
        assert_eq!(
            parse_tool_decision("refuse", None).unwrap(),
            ToolDecision::Refuse { reason: None }
        );
        assert_eq!(
            parse_tool_decision("always_accept", None).unwrap(),
            ToolDecision::AlwaysAccept
        );
    }

    #[test]
    fn test_parse_tool_decision_carries_the_reason() {
        // GIVEN operator rejects with a typed reason
        // WHEN the decision is parsed
        let d = parse_tool_decision("refuse", Some("out of scope".into())).unwrap();
        // THEN the reason travels with it
        assert_eq!(
            d,
            ToolDecision::Refuse {
                reason: Some("out of scope".into())
            }
        );
    }

    #[test]
    fn test_parse_tool_decision_invalid() {
        // GIVEN a decision name that does not exist
        // WHEN it is parsed
        let result = parse_tool_decision("maybe", None);
        // THEN the call is rejected rather than read as a refusal
        assert!(result.is_err());
    }

    #[test]
    fn test_create_session_request_deserialize() {
        // GIVEN the full session creation payload the front end sends
        let json = serde_json::json!({
            "mode": "agent",
            "agent_name": "review-agent",
            "system_prompt": "You are a code reviewer.",
            "tools": ["bash_executor", "file_io"]
        });
        // WHEN it is deserialized
        let req: CreateSessionRequest = serde_json::from_value(json).expect("deserialize");
        // THEN every field arrives, tool list included
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
        // GIVEN a payload carrying only the mode
        let json = serde_json::json!({ "mode": "libre" });
        // WHEN it is deserialized
        let req: CreateSessionRequest = serde_json::from_value(json).expect("deserialize");
        // THEN the optional fields are empty rather than making the call fail
        assert_eq!(req.mode, "libre");
        assert!(req.agent_name.is_none());
        assert!(req.system_prompt.is_none());
        assert!(req.tools.is_empty());
    }

    #[test]
    fn test_chat_session_summary_roundtrip() {
        // GIVEN a session summary as the front end receives it
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
        };
        // WHEN it is serialized and read back
        let json = serde_json::to_string(&summary).expect("serialize");
        let restored: ChatSessionSummary = serde_json::from_str(&json).expect("deserialize");
        // THEN every field survives the round trip
        assert_eq!(restored.id, "sess-42");
        assert_eq!(restored.mode, "libre");
        assert!(restored.agent_name.is_none());
        assert_eq!(restored.status, "active");
        assert_eq!(restored.message_count, 0);
    }

    #[test]
    fn test_session_info_to_summary_conversion() {
        // GIVEN a runtime session in the processing state, bound to an agent
        let info = SessionInfo {
            id: "sess-1".into(),
            mode: ChatMode::Agent,
            agent_name: Some("test-agent".into()),
            status: SessionStatus::Processing,
            created_at: "2026-03-20T10:00:00Z".into(),
            title: None,
        };
        // WHEN it is converted to the summary the front end reads
        let summary = session_info_to_summary(&info);
        // THEN the enums become their wire names and the message count starts at zero
        assert_eq!(summary.id, "sess-1");
        assert_eq!(summary.mode, "agent");
        assert_eq!(summary.agent_name.as_deref(), Some("test-agent"));
        assert_eq!(summary.status, "processing");
        assert_eq!(summary.message_count, 0);
    }
}
