//! Commandes IPC Tauri pour le chat hybride.
//!
//! Chaque commande délègue intégralement au `ChatSessionManagerHandle` —
//! zéro logique métier dans cette couche. Si le chat n'est pas disponible
//! (runtime sans LLM, erreur SQLite), une erreur explicite est retournée.

use apollia_runtime::chat::{
    ChatMode, SessionDetail, SessionInfo, SessionMetrics, SessionStatus, ToolDecision,
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
        .create_session(
            mode,
            request.agent_name,
            request.system_prompt,
            request.tools,
            request.project_id,
        )
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

/// Closes a chat session.
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

/// System prompt instructing the LLM to produce a short conversation title.
const TITLE_PROMPT: &str = "Tu génères des titres courts pour des conversations à partir de la \
requête de l'utilisateur. Le titre doit décrire l'intention de l'utilisateur en 3 à 5 mots. \
Exemples : « Aide rédaction CV », « Bug import CSV Pandas », « Idée nom d'agent IA ». \
Réponds UNIQUEMENT avec le titre — pas de guillemets, pas de ponctuation finale, pas \
d'introduction du type « Voici le titre : ».";

/// Maximum tokens the LLM may produce for a session title.
///
/// Generous enough that reasoning models (DeepSeek R1, o1-style, …) can finish
/// their `<think>` block before emitting the few-word answer; the post-process
/// step strips the reasoning block.
const TITLE_MAX_TOKENS: u32 = 1024;

/// Maximum character length of the persisted title.
const TITLE_MAX_CHARS: usize = 60;

/// Generates a short title for a chat session from its first user message.
///
/// Calls the configured LLM router with a dedicated prompt, sanitises the
/// response (trim, strip surrounding quotes, drop trailing punctuation,
/// truncate to [`TITLE_MAX_CHARS`]), then persists it via the existing
/// `rename_session` path.
///
/// Returns the generated title.
#[tauri::command]
pub async fn generate_chat_session_name(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    first_message: String,
) -> Result<String, String> {
    use apollia_llm::types::ChatMessage as LlmChatMessage;
    use apollia_llm::{CompletionRequest, ObservabilityConfig};

    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    let llm = state
        .llm_router
        .as_ref()
        .ok_or_else(|| "no LLM router configured".to_string())?;

    if first_message.trim().is_empty() {
        return Err("first_message must not be empty".to_string());
    }

    let request = CompletionRequest {
        messages: vec![
            LlmChatMessage::system(TITLE_PROMPT.to_string()),
            LlmChatMessage::user(first_message),
        ],
        max_tokens: Some(TITLE_MAX_TOKENS),
        ..CompletionRequest::default()
    };
    let obs = ObservabilityConfig::default();
    let response = llm
        .complete_with_observability(None, request, None, &obs)
        .await
        .map_err(|e| format!("LLM call failed: {e}"))?;

    let title = sanitize_session_title(&response.content);
    if title.is_empty() {
        return Err("LLM returned an empty title".to_string());
    }

    manager
        .rename_session(session_id, title.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(title)
}

/// Cleans a raw LLM response into a usable session title.
///
/// - Removes `<think>…</think>` and `<reasoning>…</reasoning>` blocks emitted
///   by reasoning models (DeepSeek R1, o1-style, …) — including unterminated
///   blocks when the response was truncated by `max_tokens`.
/// - Drops common preambles like "Voici le titre :" / "Title:".
/// - Keeps only the first non-empty line (titles are single-line).
/// - Trims whitespace.
/// - Strips a single pair of surrounding ASCII or French quotes.
/// - Removes trailing punctuation (`. , ! ? ; :` and French equivalents).
/// - Truncates to [`TITLE_MAX_CHARS`] characters (not bytes).
fn sanitize_session_title(raw: &str) -> String {
    let stripped = strip_reasoning_blocks(raw);
    // Take the first non-empty line — titles never span multiple lines.
    let line = stripped
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut s = strip_preamble(line).trim().to_string();

    // Strip a single pair of surrounding quotes (ASCII / French / typographic).
    if let Some(rest) = s
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
        .or_else(|| s.strip_prefix('«').and_then(|r| r.strip_suffix('»')))
        .or_else(|| s.strip_prefix('“').and_then(|r| r.strip_suffix('”')))
    {
        s = rest.trim().to_string();
    }

    let trailing: &[char] = &['.', ',', '!', '?', ';', ':', '。', '…'];
    while let Some(c) = s.chars().last() {
        if trailing.contains(&c) || c.is_whitespace() {
            s.pop();
        } else {
            break;
        }
    }
    s.chars().take(TITLE_MAX_CHARS).collect::<String>()
}

/// Removes `<think>…</think>` and `<reasoning>…</reasoning>` blocks.
///
/// Handles the common reasoning-model truncation case: if the response is
/// cut off inside an unterminated `<think>` block, everything from that tag
/// onward is dropped (yielding an empty string — caller will reject it).
fn strip_reasoning_blocks(raw: &str) -> String {
    fn drop_block(text: &str, open: &str, close: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find(open) {
            out.push_str(&rest[..start]);
            let after_open = &rest[start + open.len()..];
            match after_open.find(close) {
                Some(end) => rest = &after_open[end + close.len()..],
                None => {
                    // Unterminated block — drop everything from the opening tag.
                    return out;
                }
            }
        }
        out.push_str(rest);
        out
    }
    let s = drop_block(raw, "<think>", "</think>");
    drop_block(&s, "<reasoning>", "</reasoning>")
}

/// Strips common preambles models add despite the system prompt.
fn strip_preamble(line: &str) -> &str {
    let lower = line.to_lowercase();
    for prefix in [
        "voici le titre :",
        "voici le titre:",
        "titre :",
        "titre:",
        "title:",
        "title :",
    ] {
        if let Some(stripped) = lower.strip_prefix(prefix) {
            let consumed = line.len() - stripped.len();
            return line[consumed..].trim_start();
        }
    }
    line
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

/// Resolves a pending tool approval in a chat session.
///
/// `reason` is forwarded to the agent when `decision == "refuse"` so it can
/// adapt its plan; it is ignored for `accept`. `scope` is only meaningful for
/// `always_accept` and defaults to [`AlwaysAcceptScope::ThisSession`] — the
/// safest sticky option (cf. `ApprovalCardV2`).
#[tauri::command]
pub async fn authorize_chat_tool(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    message_id: String,
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

    manager
        .resolve_tool(session_id, message_id, tool_name, tool_decision)
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

/// Orphans all chat sessions linked to a project (called on project deletion).
#[tauri::command]
pub async fn orphan_project_chats(
    state: State<'_, RuntimeHandle>,
    project_id: String,
) -> Result<(), String> {
    if let Some(manager) = state.chat_manager.as_ref() {
        manager.orphan_project_sessions(project_id).await;
    }
    Ok(())
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
    }
}

/// Writes exported conversation content to disk.
///
/// Thin IPC wrapper around [`std::fs::write`] — the frontend owns the
/// formatting (Markdown / JSON / Markdown-with-tools) via
/// `lib/chat/exportConversation.ts`, then calls this to persist at the
/// path chosen via the native save dialog.
///
/// `_mime` is accepted for telemetry parity with the frontend caller but
/// is not currently recorded; the extension in `dest_path` is authoritative.
#[tauri::command]
pub async fn export_conversation(
    dest_path: String,
    content: String,
    #[allow(unused_variables)] mime: Option<String>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || std::fs::write(&dest_path, content.as_bytes()))
        .await
        .map_err(|e| format!("export_conversation join: {e}"))?
        .map_err(|e| format!("export_conversation write: {e}"))
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

/// Aggregated A2A skill telemetry returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ASkillTelemetryView {
    pub skill_name: String,
    pub version: String,
    pub invocations: u64,
    pub avg_latency_ms: u64,
    pub success_rate: f64,
    pub tokens_consumed: u64,
}

/// Step-level provenance entry for drill-down from TimelineGlobal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AStepProvenanceView {
    pub step_id: String,
    pub input_excerpt: String,
    pub output_excerpt: Option<String>,
    pub agent_from: String,
    pub agent_to: String,
    pub parent_step: Option<String>,
    pub skill_id: String,
    pub timestamp_ms: u64,
}

/// Compatibility warning returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ACompatibilityWarningView {
    pub skill_id: String,
    pub agent_name: String,
    pub required_version: String,
    pub advertised_version: String,
    /// `"warning"` or `"incompatible"`.
    pub severity: String,
    pub message: String,
    pub alternative_agent: Option<String>,
}

/// Lists aggregated telemetry for each A2A skill (rolling window).
#[tauri::command]
pub async fn list_a2a_skill_telemetry(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<A2ASkillTelemetryView>, String> {
    let Some(manager) = state.chat_manager.as_ref() else {
        return Ok(Vec::new());
    };
    let rows = manager.list_a2a_skill_telemetry().await;
    Ok(rows
        .into_iter()
        .map(|t| A2ASkillTelemetryView {
            skill_name: t.skill_name,
            version: t.version,
            invocations: t.invocations,
            avg_latency_ms: t.avg_latency_ms,
            success_rate: t.success_rate,
            tokens_consumed: t.tokens_consumed,
        })
        .collect())
}

/// Lists A2A step provenance entries, optionally filtered by skill id.
#[tauri::command]
pub async fn list_a2a_step_provenance(
    state: State<'_, RuntimeHandle>,
    skill_id: Option<String>,
) -> Result<Vec<A2AStepProvenanceView>, String> {
    let Some(manager) = state.chat_manager.as_ref() else {
        return Ok(Vec::new());
    };
    let rows = manager.list_a2a_step_provenance(skill_id).await;
    Ok(rows
        .into_iter()
        .map(|s| A2AStepProvenanceView {
            step_id: s.step_id,
            input_excerpt: s.input_excerpt,
            output_excerpt: s.output_excerpt,
            agent_from: s.agent_from,
            agent_to: s.agent_to,
            parent_step: s.parent_step,
            skill_id: s.skill_id,
            timestamp_ms: s.timestamp_ms,
        })
        .collect())
}

/// Checks semver compatibility between the director's required version and
/// the active worker advertised version for a given skill.
#[tauri::command]
pub async fn check_a2a_compatibility(
    state: State<'_, RuntimeHandle>,
    skill_id: String,
    required_version: String,
) -> Result<Option<A2ACompatibilityWarningView>, String> {
    let Some(manager) = state.chat_manager.as_ref() else {
        return Ok(None);
    };
    let out = manager
        .check_a2a_compatibility(skill_id, required_version)
        .await;
    Ok(out.map(|w| A2ACompatibilityWarningView {
        skill_id: w.skill_id,
        agent_name: w.agent_name,
        required_version: w.required_version,
        advertised_version: w.advertised_version,
        severity: match w.severity {
            apollia_runtime::a2a::CompatSeverity::Warning => "warning".to_string(),
            apollia_runtime::a2a::CompatSeverity::Incompatible => "incompatible".to_string(),
        },
        message: w.message,
        alternative_agent: w.alternative_agent,
    }))
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
    fn test_sanitize_session_title_strips_quotes_and_punct() {
        // GIVEN a raw LLM response with surrounding quotes and trailing punctuation
        // WHEN sanitised
        // THEN both wrappers are removed
        assert_eq!(sanitize_session_title("  \"Plan de migration.\"  "), "Plan de migration");
        assert_eq!(sanitize_session_title("«Idée d'article!»"), "Idée d'article");
        assert_eq!(sanitize_session_title("Refonte sidebar"), "Refonte sidebar");
    }

    #[test]
    fn test_sanitize_session_title_truncates_to_max_chars() {
        // GIVEN a long response with many chars
        let long: String = "a".repeat(120);
        // WHEN sanitised
        let out = sanitize_session_title(&long);
        // THEN it is capped at TITLE_MAX_CHARS chars (not bytes)
        assert_eq!(out.chars().count(), TITLE_MAX_CHARS);
    }

    #[test]
    fn test_sanitize_session_title_empty_input() {
        assert_eq!(sanitize_session_title("   "), "");
        assert_eq!(sanitize_session_title("…"), "");
    }

    #[test]
    fn test_sanitize_session_title_drops_think_block() {
        // GIVEN a reasoning-model response with a closed <think> block
        let raw = "<think>Okay, the user wants me to generate a title for a CV \
                   request, so let me think… Aide rédaction CV is concise.</think>\n\
                   Aide rédaction CV";
        // WHEN sanitised
        // THEN only the post-think title remains
        assert_eq!(sanitize_session_title(raw), "Aide rédaction CV");
    }

    #[test]
    fn test_sanitize_session_title_truncated_unterminated_think() {
        // GIVEN a response cut off inside an unterminated <think> block
        let raw = "<think>Okay, the user wants me to generate a short title fo";
        // WHEN sanitised
        // THEN the result is empty (caller will reject it and fall back)
        assert_eq!(sanitize_session_title(raw), "");
    }

    #[test]
    fn test_sanitize_session_title_drops_preamble() {
        assert_eq!(sanitize_session_title("Titre : Aide rédaction CV"), "Aide rédaction CV");
        assert_eq!(sanitize_session_title("Voici le titre : Bug import CSV"), "Bug import CSV");
        assert_eq!(sanitize_session_title("Title: Idée nom agent"), "Idée nom agent");
    }

    #[test]
    fn test_sanitize_session_title_keeps_only_first_line() {
        let raw = "<think>blah</think>\nAide rédaction CV\n\nNote: alternative title.";
        assert_eq!(sanitize_session_title(raw), "Aide rédaction CV");
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
