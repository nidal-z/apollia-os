//! ChatSessionManager — Tokio actor managing chat session lifecycle.
//!
//! Central entry point for the Sprint 18 chat subsystem. Handles session
//! creation, message routing, tool approval resolution, and lifecycle events.
//! Persists in SQLite via [`ChatSessionRepository`] and emits
//! [`RuntimeEvent`] on the EventBus.
//!
//! The chat path does NOT go through the `TaskRouter` — it has its own
//! execution path (ADR-034).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use apollia_core::{RuntimeEvent, StepBudgetConfig};
use apollia_llm::{LlmRouter, ToolInvoker};
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::budget::StepBudget;
use apollia_tools::{ProjectRepository, ToolRegistryHandle};

use super::a2a_tools::CompositeToolInvoker;
use super::agent_chat::{AgentChatExecutor, ChatAgentRunner};
use super::builtin_agent::{
    BuiltInChatAgent, ChatAgentResponse, NativeChatToolInvoker, DEFAULT_CONTEXT_WINDOW_SIZE,
};
use super::extractor::UserMemoryExtractor;
use super::repository::{AppendMessageParams, ChatSessionRepository};
use super::types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, ExchangeState, MessageId,
    PendingChatApprovals, PendingFilesystemApprovals, ProjectContextProvider, RecentSessionSummary,
    SessionDetail, SessionId, SessionInfo, SessionStatus, ToolCallRecord, ToolDecision,
};
use crate::a2a::A2AInvoker;
use crate::api::routes_agents::AgentLoader;
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;

/// Maximum number of past sessions to inject as cross-session context.
const MAX_PAST_SESSIONS: usize = 3;

/// Minimum length (in bytes) of the first message to trigger cross-session recall.
///
/// Short greetings like "bonjour" or "hello" are filtered out to avoid
/// injecting irrelevant context from past sessions.
const MIN_MESSAGE_LENGTH_FOR_RECALL: usize = 20;

/// Commands sent to the [`ChatSessionManager`] actor.
pub enum ChatCommand {
    /// Create a new chat session.
    CreateSession {
        /// Session mode (Libre or Agent).
        mode: ChatMode,
        /// Agent name (required for Agent mode).
        agent_name: Option<String>,
        /// Custom system prompt override.
        system_prompt: Option<String>,
        /// Tool names available in this session.
        tools: Vec<String>,
        /// Project to link this session to (None = standalone).
        project_id: Option<String>,
        /// Response channel.
        reply: oneshot::Sender<Result<SessionInfo, ChatError>>,
    },
    /// Send a user message in a session.
    SendMessage {
        /// Target session.
        session_id: SessionId,
        /// Text content.
        content: String,
        /// Response channel.
        reply: oneshot::Sender<Result<MessageId, ChatError>>,
    },
    /// Resolve a pending tool approval.
    ResolveTool {
        /// Target session.
        session_id: SessionId,
        /// Message that triggered the tool call.
        message_id: MessageId,
        /// Name of the tool.
        tool_name: String,
        /// User decision.
        decision: ToolDecision,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// List all sessions (optionally filtered by status).
    ListSessions {
        /// Optional status filter.
        status_filter: Option<SessionStatus>,
        /// Response channel.
        reply: oneshot::Sender<Vec<SessionInfo>>,
    },
    /// Get detailed session info.
    GetSession {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Option<SessionDetail>>,
    },
    /// Close a session.
    CloseSession {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Update session configuration (system_prompt, tools, llm_backend).
    UpdateSession {
        /// Target session.
        session_id: SessionId,
        /// New system prompt (if Some).
        system_prompt: Option<String>,
        /// New available tools (if Some).
        available_tools: Option<Vec<String>>,
        /// New LLM backend (if Some — inner None means "use default").
        llm_backend: Option<Option<String>>,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Delete a session and all its data.
    DeleteSession {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Rename a session (set a user-defined title).
    RenameSession {
        /// Target session.
        session_id: SessionId,
        /// New display title.
        title: String,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Internal: ReAct exchange completed successfully (sent by spawned task).
    ExchangeComplete {
        /// Target session.
        session_id: SessionId,
        /// User message that triggered the exchange.
        message_id: MessageId,
        /// Agent response.
        response: ChatAgentResponse,
    },
    /// Internal: ReAct exchange failed (sent by spawned task).
    ExchangeError {
        /// Target session.
        session_id: SessionId,
        /// User message that triggered the exchange.
        message_id: MessageId,
        /// Error description.
        error: String,
    },
    /// Internal: persist a conversation summary computed by the spawned task.
    PersistSummary {
        /// Target session.
        session_id: SessionId,
        /// Summary text to store.
        summary: String,
    },
    /// List the N most recent sessions with their first message content.
    GetRecentSummaries {
        /// Maximum number of sessions to return.
        limit: usize,
        /// Response channel.
        reply: oneshot::Sender<Vec<RecentSessionSummary>>,
    },
    /// Load a session from SQLite into memory (if not already loaded) and reset
    /// Processing status to Active.
    ResumeSession {
        /// Target session identifier.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<SessionDetail, ChatError>>,
    },
    /// Hot-reload the LLM router (e.g. after onboarding setup).
    ReloadLlm {
        /// New router to use for subsequent requests.
        router: Option<Arc<LlmRouter>>,
    },
    /// Fork a session, creating a child with a copy of the history.
    ForkSession {
        /// Parent session to fork from.
        session_id: SessionId,
        /// Number of messages to copy (None = all).
        up_to_index: Option<usize>,
        /// Response channel.
        reply: oneshot::Sender<Result<SessionInfo, ChatError>>,
    },
    /// List all child sessions of a parent (forks).
    ListChildren {
        /// Parent session identifier.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Vec<SessionInfo>>,
    },
    /// Link or unlink a session to a project.
    LinkSessionToProject {
        /// Target session.
        session_id: SessionId,
        /// Project ID (None to unlink).
        project_id: Option<String>,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// List sessions belonging to a specific project.
    ListSessionsByProject {
        /// Project identifier.
        project_id: String,
        /// Response channel.
        reply: oneshot::Sender<Vec<SessionInfo>>,
    },
    /// Unlink all sessions from a project (orphan them on project deletion).
    OrphanProjectSessions {
        /// Project identifier.
        project_id: String,
    },
    /// List all A2A skills available from active worker agents.
    ListA2ASkills {
        /// Response channel.
        reply: oneshot::Sender<Vec<crate::a2a::SkillListing>>,
    },
    /// Resolve a pending filesystem HITL request.
    ///
    /// Called by the `respond_hitl_filesystem` Tauri command when the user
    /// makes a decision in `HitlFilesystemModal`.
    ResolveFsHitl {
        /// Unique request identifier emitted in `HitlFilesystemRequired`.
        request_id: String,
        /// User decision.
        decision: super::types::FsHitlDecision,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// List recently resolved chat tool approvals.
    ListApprovalHistory {
        /// Maximum number of entries to return.
        limit: i64,
        /// Number of days to look back.
        days: i64,
        /// Response channel.
        reply: oneshot::Sender<Result<Vec<super::repository::ChatApprovalLogRow>, ChatError>>,
    },
    /// Internal: register a pending `ask_user` reply channel from the background drain task.
    RegisterUserInputReply {
        /// Unique request identifier.
        request_id: String,
        /// Questions JSON for event emission.
        questions_json: String,
        /// Agent context for the questions.
        context: Option<String>,
        /// Oneshot sender to deliver answers back to the executor.
        reply_tx: tokio::sync::oneshot::Sender<apollia_tools::tools::ask_user::AskUserOutput>,
    },
    /// Resolve a pending `ask_user` request with user answers.
    ResolveUserInput {
        /// Unique request identifier emitted in `ChatUserInputRequired`.
        request_id: String,
        /// User answers to the questions.
        answers: Vec<apollia_tools::tools::ask_user::UserAnswer>,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Shut down the actor.
    Shutdown,
}

/// Internal state of the [`ChatSessionManager`] actor.
struct ChatSessionManager {
    /// In-memory session cache.
    sessions: HashMap<SessionId, ChatSession>,
    /// SQLite persistence.
    repository: ChatSessionRepository,
    /// LLM router for free-form / agent conversations.
    llm_router: Option<Arc<LlmRouter>>,
    /// Tool registry for tool descriptor resolution.
    tool_registry: ToolRegistryHandle,
    /// Agent registry for resolving agent names to IDs.
    registry_handle: AgentRegistryHandle,
    /// Agent runner for Chat Agent mode. `None` disables Agent mode.
    agent_runner: Option<Arc<dyn ChatAgentRunner>>,
    /// Event bus sender for runtime events.
    event_bus: EventBusSender,
    /// Runtime-level step budget configuration.
    runtime_budget: StepBudgetConfig,
    /// Pending tool approval channels (operator HITL).
    pending_chat_approvals: PendingChatApprovals,
    /// Pending filesystem HITL approval channels (ADR-069 Couche 2).
    pending_fs_approvals: PendingFilesystemApprovals,
    /// Optional user memory repository for system prompt enrichment.
    user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Stateful extractor for passive memory enrichment from conversations.
    enrichment_extractor: Option<Arc<tokio::sync::Mutex<UserMemoryExtractor>>>,
    /// Sender clone for spawned tasks to send commands back to the actor.
    tx: mpsc::Sender<ChatCommand>,
    /// Optional A2A invoker — when present, worker agent skills are exposed as virtual tools.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Optional project context provider for injecting project context into system prompts.
    project_context: Option<Arc<dyn ProjectContextProvider>>,
    /// Optional project repository for resolving workspace_path per session (ADR-069).
    project_repo: Option<Arc<ProjectRepository>>,
    /// Pending user input registry for the `ask_user` tool.
    pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs,
    /// Map of pending `ask_user` reply channels, keyed by request_id.
    /// Populated by the background drain task, resolved by `ResolveUserInput`.
    pending_user_replies: HashMap<
        String,
        tokio::sync::oneshot::Sender<apollia_tools::tools::ask_user::AskUserOutput>,
    >,
}

impl ChatSessionManager {
    /// Process incoming commands until Shutdown or channel close.
    async fn run(mut self, mut rx: mpsc::Receiver<ChatCommand>) {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ChatCommand::CreateSession {
                    mode,
                    agent_name,
                    system_prompt,
                    tools,
                    project_id,
                    reply,
                } => {
                    let result = self
                        .handle_create_session(mode, agent_name, system_prompt, tools, project_id)
                        .await;
                    let _ = reply.send(result);
                }
                ChatCommand::SendMessage {
                    session_id,
                    content,
                    reply,
                } => {
                    let result = self.handle_send_message(&session_id, &content);
                    let _ = reply.send(result);
                }
                ChatCommand::ResolveTool {
                    session_id,
                    message_id,
                    tool_name,
                    decision,
                    reply,
                } => {
                    let result =
                        self.handle_resolve_tool(&session_id, &message_id, &tool_name, decision);
                    let _ = reply.send(result);
                }
                ChatCommand::ListSessions {
                    status_filter,
                    reply,
                } => {
                    let result = self.handle_list_sessions(status_filter.as_ref());
                    let _ = reply.send(result);
                }
                ChatCommand::GetSession { session_id, reply } => {
                    let result = self.handle_get_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::CloseSession { session_id, reply } => {
                    let result = self.handle_close_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::DeleteSession { session_id, reply } => {
                    let result = self.handle_delete_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::RenameSession {
                    session_id,
                    title,
                    reply,
                } => {
                    let result = self.handle_rename_session(&session_id, &title);
                    let _ = reply.send(result);
                }
                ChatCommand::UpdateSession {
                    session_id,
                    system_prompt,
                    available_tools,
                    llm_backend,
                    reply,
                } => {
                    let result = self.handle_update_session(
                        &session_id,
                        system_prompt.as_deref(),
                        available_tools.as_deref(),
                        llm_backend.as_ref(),
                    );
                    let _ = reply.send(result);
                }
                ChatCommand::ExchangeComplete {
                    session_id,
                    message_id,
                    response,
                } => {
                    self.handle_exchange_complete(&session_id, &message_id, response);
                }
                ChatCommand::ExchangeError {
                    session_id,
                    message_id,
                    error,
                } => {
                    self.handle_exchange_error(&session_id, &message_id, &error);
                }
                ChatCommand::PersistSummary {
                    session_id,
                    summary,
                } => {
                    if let Err(e) = self.repository.update_summary(&session_id, &summary) {
                        warn!(session_id = %session_id, error = %e, "Failed to persist conversation summary");
                    }
                }
                ChatCommand::GetRecentSummaries { limit, reply } => {
                    let result = self.handle_get_recent_summaries(limit);
                    let _ = reply.send(result);
                }
                ChatCommand::ResumeSession { session_id, reply } => {
                    let result = self.handle_resume_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::ForkSession {
                    session_id,
                    up_to_index,
                    reply,
                } => {
                    let result = self.handle_fork_session(&session_id, up_to_index);
                    let _ = reply.send(result);
                }
                ChatCommand::ListChildren { session_id, reply } => {
                    let result = self.handle_list_children(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::LinkSessionToProject {
                    session_id,
                    project_id,
                    reply,
                } => {
                    let result =
                        self.handle_link_session_to_project(&session_id, project_id.as_deref());
                    let _ = reply.send(result);
                }
                ChatCommand::ListSessionsByProject { project_id, reply } => {
                    let result = self.handle_list_sessions_by_project(&project_id);
                    let _ = reply.send(result);
                }
                ChatCommand::OrphanProjectSessions { project_id } => {
                    match self.repository.orphan_project_sessions(&project_id) {
                        Ok(count) => {
                            if count > 0 {
                                info!(project_id = %project_id, count, "Orphaned chat sessions after project deletion");
                                // Also update in-memory cache
                                for session in self.sessions.values_mut() {
                                    if session.project_id.as_deref() == Some(&project_id) {
                                        session.project_id = None;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!(project_id = %project_id, error = %e, "Failed to orphan sessions");
                        }
                    }
                }
                ChatCommand::ListA2ASkills { reply } => {
                    if let Some(ref a2a) = self.a2a_invoker {
                        let a2a = a2a.clone();
                        tokio::spawn(async move {
                            let skills = a2a.list_skills().await.unwrap_or_default();
                            let _ = reply.send(skills);
                        });
                    } else {
                        let _ = reply.send(Vec::new());
                    }
                }
                ChatCommand::ReloadLlm { router } => {
                    info!("ChatSessionManager: LLM router reloaded");
                    self.llm_router = router;
                }
                ChatCommand::ResolveFsHitl {
                    request_id,
                    decision,
                    reply,
                } => {
                    let resolved = self.pending_fs_approvals.resolve(&request_id, decision);
                    let result = if resolved {
                        Ok(())
                    } else {
                        Err(ChatError::InternalError(format!(
                            "no pending fs HITL request for id '{request_id}'"
                        )))
                    };
                    let _ = reply.send(result);
                }
                ChatCommand::ListApprovalHistory { limit, days, reply } => {
                    let result = self.repository.list_tool_approval_history(limit, days);
                    let _ = reply.send(result);
                }
                ChatCommand::RegisterUserInputReply {
                    request_id,
                    questions_json,
                    context,
                    reply_tx,
                } => {
                    // Store the reply channel for later resolution.
                    self.pending_user_replies
                        .insert(request_id.clone(), reply_tx);
                    // Emit event so the UI can render the AskUserCard.
                    let _ = self.event_bus.send(
                        apollia_core::RuntimeEvent::ChatUserInputRequired {
                            request_id,
                            session_id: String::new(), // TODO: associate with session
                            message_id: String::new(),
                            questions_json,
                            context,
                        },
                    );
                }
                ChatCommand::ResolveUserInput {
                    request_id,
                    answers,
                    reply,
                } => {
                    // The PendingUserInputs registry is consumed by the executor
                    // in a background task. We need to find the pending request
                    // and deliver the answers. Since the executor is already
                    // listening on the oneshot, we use a dedicated resolution map.
                    let result = self.resolve_user_input_internal(&request_id, answers);
                    let _ = reply.send(result);
                }
                ChatCommand::Shutdown => {
                    info!("ChatSessionManager: shutting down");
                    break;
                }
            }
        }
        info!("ChatSessionManager: actor stopped");
    }

    /// Build a cross-session context block from past session summaries.
    ///
    /// Returns `None` if the first message is too short (trivial greeting)
    /// or no relevant past sessions are found.
    fn build_cross_session_context(&self, first_message: &str) -> Option<String> {
        if first_message.len() < MIN_MESSAGE_LENGTH_FOR_RECALL {
            return None;
        }

        let sessions = self
            .repository
            .find_relevant_sessions(first_message, MAX_PAST_SESSIONS)
            .ok()?;

        if sessions.is_empty() {
            return None;
        }

        let mut block = String::from("## Previous conversations (for reference)\n");
        for session in &sessions {
            block.push_str(&format!("- [{}] {}\n", session.created_at, session.summary));
        }

        Some(block)
    }

    /// Create a new chat session.
    async fn handle_create_session(
        &mut self,
        mode: ChatMode,
        agent_name: Option<String>,
        system_prompt: Option<String>,
        tools: Vec<String>,
        project_id: Option<String>,
    ) -> Result<SessionInfo, ChatError> {
        // Agent mode requires an agent name
        if mode == ChatMode::Agent && agent_name.is_none() {
            return Err(ChatError::AgentNotFound(
                "agent_name is required for Agent mode".into(),
            ));
        }

        // Validate agent exists in the registry if agent mode
        if mode == ChatMode::Agent {
            if let Some(ref name) = agent_name {
                let found = self.registry_handle.find_by_name(name).await.map_err(|e| {
                    ChatError::InternalError(format!("registry lookup failed: {e}"))
                })?;
                if found.is_none() {
                    return Err(ChatError::AgentNotFound(name.clone()));
                }
            }
        }

        // Libre mode requires an LLM
        if mode == ChatMode::Libre && self.llm_router.is_none() {
            return Err(ChatError::NoLlmConfigured);
        }

        let session_id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let prompt = system_prompt.unwrap_or_default();

        // Persist to SQLite
        self.repository.create_session(
            &session_id,
            &mode,
            agent_name.as_deref(),
            &prompt,
            &tools,
            &now,
            None,
            project_id.as_deref(),
        )?;

        // Build in-memory session
        let session = ChatSession {
            id: session_id.clone(),
            mode: mode.clone(),
            agent_name: agent_name.clone(),
            system_prompt: prompt,
            status: SessionStatus::Active,
            history: Vec::new(),
            authorized_tools: std::collections::HashSet::new(),
            available_tools: tools,
            created_at: now.clone(),
            active_exchange: None,
            llm_backend: None,
            title: None,
            parent_session_id: None,
            fork_depth: 0,
            project_id,
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
        };

        let info = session_to_info(&session);
        self.sessions.insert(session_id.clone(), session);

        // Emit event
        let _ = self.event_bus.send(RuntimeEvent::ChatSessionCreated {
            session_id,
            mode: mode.as_sql().to_string(),
            agent_name,
        });

        Ok(info)
    }

    /// Update session configuration.
    fn handle_update_session(
        &mut self,
        session_id: &str,
        system_prompt: Option<&str>,
        available_tools: Option<&[String]>,
        llm_backend: Option<&Option<String>>,
    ) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        if session.status == SessionStatus::Closed {
            return Err(ChatError::SessionClosed(session_id.to_string()));
        }

        if session.status == SessionStatus::Processing {
            return Err(ChatError::SessionBusy(session_id.to_string()));
        }

        // Persist to SQLite
        self.repository.update_session_config(
            session_id,
            system_prompt,
            available_tools,
            llm_backend.map(|b| b.as_deref()),
        )?;

        // Update in-memory session
        if let Some(prompt) = system_prompt {
            session.system_prompt = prompt.to_string();
        }
        if let Some(tools) = available_tools {
            session.available_tools = tools.to_vec();
        }
        if let Some(backend) = llm_backend {
            session.llm_backend = backend.clone();
        }

        info!(session_id = %session_id, "chat session config updated");
        Ok(())
    }

    /// Send a user message in a session.
    fn handle_send_message(
        &mut self,
        session_id: &str,
        content: &str,
    ) -> Result<MessageId, ChatError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        if session.status == SessionStatus::Closed {
            return Err(ChatError::SessionClosed(session_id.to_string()));
        }

        if session.status == SessionStatus::Processing {
            return Err(ChatError::SessionBusy(session_id.to_string()));
        }

        let message_id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();

        // Persist message to SQLite
        let seq = self.repository.append_message(&AppendMessageParams {
            id: &message_id,
            session_id,
            role: &ChatRole::User,
            content,
            tool_calls_json: None,
            tool_name: None,
            created_at: &now,
            metadata: None,
        })?;

        // Add to in-memory history
        let msg = ChatMessage {
            id: message_id.clone(),
            role: ChatRole::User,
            content: content.to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: now.clone(),
            seq,
            metadata: None,
        };
        session.history.push(msg);

        // Set session to Processing
        session.status = SessionStatus::Processing;
        session.active_exchange = Some(ExchangeState {
            message_id: message_id.clone(),
            started_at: now,
        });
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Processing)
        {
            warn!(error = %e, "Failed to update session status to Processing in SQLite");
        }

        // Emit event
        let _ = self.event_bus.send(RuntimeEvent::ChatMessageSent {
            session_id: session_id.to_string(),
            message_id: message_id.clone(),
        });

        // Launch BuiltInChatAgent in a background task for Libre mode.
        // For Agent mode, a different path will be used.
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;

        if session.mode == ChatMode::Libre || session.mode == ChatMode::Companion {
            let llm_router = self.llm_router.clone().ok_or(ChatError::NoLlmConfigured)?;

            // Companion sessions are intentionally isolated from user memory and
            // cross-session history (Principle #6 — memory at agent initiative).
            // The companion helps with the platform, not the user's personal context.
            let session_user_memory = if session.mode == ChatMode::Companion {
                None
            } else {
                self.user_memory.clone()
            };

            let history = session.history.clone();
            let is_first_message = history.len() == 1;
            let is_companion = session.mode == ChatMode::Companion;
            // On the first message, enrich the system prompt with cross-session context.
            // Companion sessions are excluded — they must not inherit personal history.
            let system_prompt = if is_first_message && !is_companion {
                let mut prompt = session.system_prompt.clone();
                if let Some(block) = self.build_cross_session_context(content) {
                    prompt.push_str("\n\n");
                    prompt.push_str(&block);
                }
                prompt
            } else {
                session.system_prompt.clone()
            };
            let available_tools = session.available_tools.clone();
            let authorized_tools = session.authorized_tools.clone();
            let pending_approvals = self.pending_chat_approvals.clone();
            let budget = StepBudget::new(&self.runtime_budget);
            let sid = session_id.to_string();
            let mid = message_id.clone();
            let user_msg = content.to_string();
            let tx = self.tx.clone();
            let context_window_size = DEFAULT_CONTEXT_WINDOW_SIZE;

            let stored_summary = self.repository.get_summary(session_id).unwrap_or(None);
            let llm_for_summarize = self.llm_router.clone();

            // Capture project context and repo for async injection in spawned task.
            // The invoker is created per-session inside the task (ADR-069).
            let project_ctx = self.project_context.clone();
            let session_project_id = session.project_id.clone();
            let project_repo_for_session = self.project_repo.clone();
            let a2a_for_agent = self.a2a_invoker.clone();
            let tool_registry = self.tool_registry.clone();
            let event_bus = self.event_bus.clone();

            let pending_user_inputs_for_session = self.pending_user_inputs.clone();

            // Capture HITL filesystem params for the invoker.
            let hitl_params = HitlInvokerParams {
                session_id: session_id.to_string(),
                event_bus: self.event_bus.clone(),
                pending_fs: self.pending_fs_approvals.clone(),
                fs_allow_rules: std::sync::Arc::clone(&session.fs_allow_rules),
                risk_config: apollia_core::FilesystemRiskConfig::default(),
            };

            tokio::spawn(async move {
                // Resolve per-session sandbox root from project workspace_path.
                // On error (project not found) surface as ExchangeError — no panic.
                let native_invoker = match resolve_workspace_for_session(
                    &session_project_id,
                    &project_repo_for_session,
                    Some(hitl_params),
                    Some(pending_user_inputs_for_session),
                )
                .await
                {
                    Ok(inv) => inv,
                    Err(e) => {
                        let _ = tx
                            .send(ChatCommand::ExchangeError {
                                session_id: sid,
                                message_id: mid,
                                error: e.to_string(),
                            })
                            .await;
                        return;
                    }
                };
                let session_invoker: Arc<dyn ToolInvoker> = if let Some(ref a2a) = a2a_for_agent {
                    Arc::new(CompositeToolInvoker::new(native_invoker, a2a.clone()))
                } else {
                    Arc::new(native_invoker)
                };

                let agent = BuiltInChatAgent::new(
                    llm_router,
                    tool_registry,
                    session_invoker,
                    event_bus,
                    session_user_memory,
                    a2a_for_agent,
                );

                // On the first message, inject project context if the session belongs to a project.
                let system_prompt = if is_first_message && !is_companion {
                    if let (Some(ref pid), Some(ref provider)) = (&session_project_id, &project_ctx)
                    {
                        match provider.build_context(pid).await {
                            Some(ctx) => {
                                let mut enriched = system_prompt;
                                enriched.push_str("\n\n");
                                enriched.push_str(&ctx);
                                enriched
                            }
                            None => system_prompt,
                        }
                    } else {
                        system_prompt
                    }
                } else {
                    system_prompt
                };
                let summary = if history.len() > context_window_size && stored_summary.is_none() {
                    if let Some(ref llm) = llm_for_summarize {
                        let older = &history[..history.len() - context_window_size];
                        match super::summarizer::summarize(older, llm).await {
                            Ok(s) => {
                                let _ = tx
                                    .send(ChatCommand::PersistSummary {
                                        session_id: sid.clone(),
                                        summary: s.clone(),
                                    })
                                    .await;
                                Some(s)
                            }
                            Err(e) => {
                                warn!(error = %e, "Context window summarization failed, proceeding without summary");
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    stored_summary
                };

                let result = agent
                    .execute(
                        &sid,
                        &mid,
                        &user_msg,
                        &history,
                        &system_prompt,
                        &available_tools,
                        &authorized_tools,
                        &pending_approvals,
                        &budget,
                        summary.as_deref(),
                        context_window_size,
                    )
                    .await;

                let cmd = match result {
                    Ok(response) => ChatCommand::ExchangeComplete {
                        session_id: sid,
                        message_id: mid,
                        response,
                    },
                    Err(err) => ChatCommand::ExchangeError {
                        session_id: sid,
                        message_id: mid,
                        error: err.to_string(),
                    },
                };
                let _ = tx.send(cmd).await;
            });
        } else {
            // Agent mode: dispatch to AgentChatExecutor.
            let agent_runner = match self.agent_runner.clone() {
                Some(r) => r,
                None => {
                    warn!(session_id = %session_id, "Agent mode requested but no ChatAgentRunner configured");
                    if let Some(s) = self.sessions.get_mut(session_id) {
                        s.status = SessionStatus::Active;
                        s.active_exchange = None;
                    }
                    if let Err(e) = self
                        .repository
                        .update_status(session_id, &SessionStatus::Active)
                    {
                        warn!(error = %e, "Failed to reset session status to Active in SQLite");
                    }
                    return Err(ChatError::AgentLoadFailed(
                        "no ChatAgentRunner configured — Agent mode unavailable".into(),
                    ));
                }
            };

            let executor = AgentChatExecutor::new(agent_runner, self.event_bus.clone());
            let session_clone = session.clone();
            let authorized = session.authorized_tools.clone();
            let pending = self.pending_chat_approvals.clone();
            let sid = session_id.to_string();
            let mid = message_id.clone();
            let user_msg = content.to_string();
            let tx = self.tx.clone();

            tokio::spawn(async move {
                let result = executor
                    .execute(&session_clone, &user_msg, &mid, &authorized, &pending)
                    .await;

                let cmd = match result {
                    Ok(response) => ChatCommand::ExchangeComplete {
                        session_id: sid,
                        message_id: mid,
                        response,
                    },
                    Err(err) => ChatCommand::ExchangeError {
                        session_id: sid,
                        message_id: mid,
                        error: err.to_string(),
                    },
                };
                let _ = tx.send(cmd).await;
            });
        }

        Ok(message_id)
    }

    /// Handle successful completion of a ReAct exchange.
    fn handle_exchange_complete(
        &mut self,
        session_id: &str,
        _message_id: &str,
        response: ChatAgentResponse,
    ) {
        let session = match self.sessions.get_mut(session_id) {
            Some(s) => s,
            None => {
                warn!(session_id = %session_id, "ExchangeComplete for unknown session");
                return;
            }
        };

        let now = now_rfc3339();
        let assistant_msg_id = uuid::Uuid::new_v4().to_string();

        // Serialize tool calls for SQLite
        let tool_calls_json = if response.tool_calls.is_empty() {
            None
        } else {
            serde_json::to_string(&response.tool_calls).ok()
        };

        // Serialize thinking trace as JSON metadata.
        let metadata_json = response
            .thinking_trace
            .as_ref()
            .map(|t| serde_json::json!({ "thinking_trace": t }).to_string());

        // Persist assistant response message
        match self.repository.append_message(&AppendMessageParams {
            id: &assistant_msg_id,
            session_id,
            role: &ChatRole::Assistant,
            content: &response.content,
            tool_calls_json: tool_calls_json.as_deref(),
            tool_name: None,
            created_at: &now,
            metadata: metadata_json.as_deref(),
        }) {
            Ok(seq) => {
                let metadata_value = metadata_json
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok());
                session.history.push(ChatMessage {
                    id: assistant_msg_id,
                    role: ChatRole::Assistant,
                    content: response.content,
                    tool_calls: if response.tool_calls.is_empty() {
                        None
                    } else {
                        Some(response.tool_calls)
                    },
                    tool_name: None,
                    created_at: now,
                    seq,
                    metadata: metadata_value,
                });
            }
            Err(e) => {
                error!(error = %e, "Failed to persist assistant message to SQLite");
            }
        }

        // Persist newly authorized tools
        for tool_name in &response.newly_authorized {
            let auth_now = now_rfc3339();
            if let Err(e) = self
                .repository
                .authorize_tool(session_id, tool_name, &auth_now)
            {
                warn!(error = %e, tool = %tool_name, "Failed to persist tool authorization");
            }
            session.authorized_tools.insert(tool_name.clone());
        }

        // Reset session to Active
        session.status = SessionStatus::Active;
        session.active_exchange = None;
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Active)
        {
            warn!(error = %e, "Failed to reset session status to Active in SQLite");
        }

        info!(
            session_id = %session_id,
            tokens = response.tokens_used.prompt_tokens + response.tokens_used.completion_tokens,
            "Chat exchange complete"
        );
    }

    /// Handle a failed ReAct exchange.
    fn handle_exchange_error(&mut self, session_id: &str, message_id: &str, error: &str) {
        error!(
            session_id = %session_id,
            message_id = %message_id,
            error = %error,
            "Chat exchange failed"
        );

        let _ = self.event_bus.send(RuntimeEvent::ChatError {
            session_id: session_id.to_string(),
            message_id: Some(message_id.to_string()),
            error: error.to_string(),
        });

        // Always emit ChatResponseCompleted so the frontend exits the "generating"
        // state even when the exchange fails.  Without this the UI stays blocked
        // indefinitely because it waits for ChatResponseCompleted to clear the
        // typing indicator.
        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: format!("[Erreur : {error}]"),
        });

        // Reset session to Active so it can accept new messages
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.status = SessionStatus::Active;
            session.active_exchange = None;
        }
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Active)
        {
            warn!(error = %e, "Failed to reset session status to Active after error");
        }
    }

    /// Resolve a pending tool approval.
    fn handle_resolve_tool(
        &mut self,
        session_id: &str,
        message_id: &str,
        tool_name: &str,
        decision: ToolDecision,
    ) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        let key = format!("{session_id}::{message_id}::{tool_name}");
        let resolved = self.pending_chat_approvals.resolve(&key, decision.clone());

        if !resolved {
            return Err(ChatError::SessionNotFound(format!(
                "no pending approval for {key}"
            )));
        }

        // If AlwaysAccept, persist authorization
        if decision == ToolDecision::AlwaysAccept {
            let now = now_rfc3339();
            if let Err(e) = self.repository.authorize_tool(session_id, tool_name, &now) {
                warn!(error = %e, "Failed to persist tool authorization");
            }
            session.authorized_tools.insert(tool_name.to_string());
        }

        let decision_str = match &decision {
            ToolDecision::Accept => "accept",
            ToolDecision::Refuse => "refuse",
            ToolDecision::AlwaysAccept => "always_accept",
        };

        let resolved_at = now_rfc3339();

        // Persist decision in approval log for history view.
        if let Err(e) = self.repository.log_tool_approval(
            session_id,
            message_id,
            tool_name,
            decision_str,
            &resolved_at,
        ) {
            warn!(error = %e, "Failed to persist chat approval log");
        }

        let _ = self.event_bus.send(RuntimeEvent::ChatApprovalResolved {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: tool_name.to_string(),
            decision: decision_str.to_string(),
        });

        Ok(())
    }

    /// List sessions with optional status filter.
    fn handle_list_sessions(&self, status_filter: Option<&SessionStatus>) -> Vec<SessionInfo> {
        let filter_sql = status_filter.map(|s| s.as_sql());
        match self.repository.list_sessions(filter_sql) {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    let mode = ChatMode::from_sql(&row.mode)?;
                    let status = SessionStatus::from_sql(&row.status)?;
                    Some(SessionInfo {
                        id: row.id,
                        mode,
                        agent_name: row.agent_name,
                        status,
                        created_at: row.created_at,
                        title: row.title,
                        project_id: row.project_id,
                    })
                })
                .collect(),
            Err(e) => {
                error!(error = %e, "Failed to list sessions from SQLite");
                Vec::new()
            }
        }
    }

    /// Get session detail.
    fn handle_get_session(&self, session_id: &str) -> Option<SessionDetail> {
        // Try in-memory first
        if let Some(session) = self.sessions.get(session_id) {
            return Some(SessionDetail {
                session: session.clone(),
                message_count: session.history.len() as u32,
            });
        }

        // Fall back to SQLite
        let row = match self.repository.get_session(session_id) {
            Ok(Some(row)) => row,
            _ => return None,
        };

        let messages = self
            .repository
            .get_messages(session_id, None)
            .unwrap_or_default();
        let authorized_tools = self
            .repository
            .get_authorized_tools(session_id)
            .unwrap_or_default();

        let mode = ChatMode::from_sql(&row.mode)?;
        let status = SessionStatus::from_sql(&row.status)?;
        let available_tools: Vec<String> =
            serde_json::from_str(&row.available_tools).unwrap_or_default();

        let history: Vec<ChatMessage> = messages
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

        let message_count = history.len() as u32;
        let session = ChatSession {
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
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
        };

        Some(SessionDetail {
            session,
            message_count,
        })
    }

    /// Close a session.
    fn handle_close_session(&mut self, session_id: &str) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        if session.status == SessionStatus::Closed {
            return Err(ChatError::SessionClosed(session_id.to_string()));
        }

        // Capture history before mutating session state
        let history = session.history.clone();

        let now = now_rfc3339();

        // If an exchange is in progress, cancel it
        session.active_exchange = None;
        session.status = SessionStatus::Closed;

        self.repository.close_session(session_id, &now)?;

        let _ = self.event_bus.send(RuntimeEvent::ChatSessionClosed {
            session_id: session_id.to_string(),
        });

        // Fire-and-forget passive memory enrichment with rate limiting and deduplication
        if let Some(extractor) = &self.enrichment_extractor {
            UserMemoryExtractor::spawn_enrichment(Arc::clone(extractor), history);
        }

        Ok(())
    }

    /// Delete a session and all its data from SQLite and in-memory cache.
    fn handle_delete_session(&mut self, session_id: &str) -> Result<(), ChatError> {
        // Remove from SQLite (messages, authorizations, FTS, session row).
        self.repository.delete_session(session_id)?;

        // Remove from in-memory cache.
        self.sessions.remove(session_id);

        let _ = self.event_bus.send(RuntimeEvent::ChatSessionClosed {
            session_id: session_id.to_string(),
        });

        info!(session_id = %session_id, "chat session deleted");
        Ok(())
    }

    /// Rename a session by setting a user-defined title.
    fn handle_rename_session(&mut self, session_id: &str, title: &str) -> Result<(), ChatError> {
        // Persist to SQLite.
        self.repository.rename_session(session_id, title)?;

        // Update in-memory cache if present.
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.title = Some(title.to_string());
        }

        info!(session_id = %session_id, title = %title, "chat session renamed");
        Ok(())
    }

    /// Return a lightweight summary of the N most recent sessions.
    ///
    /// Calls the repository and logs on error, returning an empty vec on failure.
    fn handle_get_recent_summaries(&self, limit: usize) -> Vec<RecentSessionSummary> {
        match self.repository.list_recent_summaries(limit) {
            Ok(summaries) => summaries,
            Err(e) => {
                error!(error = %e, "Failed to list recent session summaries from SQLite");
                Vec::new()
            }
        }
    }

    /// Load a session from SQLite into memory (if not already there) and return its detail.
    ///
    /// If the session is already in memory, its current state is returned immediately.
    /// If the loaded session has status `Processing`, it is reset to `Active` both in
    /// memory and in SQLite before being returned.
    fn handle_resume_session(&mut self, session_id: &str) -> Result<SessionDetail, ChatError> {
        // If already in memory, return current state directly.
        if let Some(session) = self.sessions.get(session_id) {
            let message_count = session.history.len() as u32;
            return Ok(SessionDetail {
                session: session.clone(),
                message_count,
            });
        }

        // Load from SQLite.
        let mut session = self.repository.load_session_with_history(session_id)?;

        // If the session was left in Processing state, reset it to Active.
        if session.status == SessionStatus::Processing {
            if let Err(e) = self
                .repository
                .update_status(session_id, &SessionStatus::Active)
            {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "Failed to reset Processing session to Active in SQLite during resume"
                );
            }
            session.status = SessionStatus::Active;
            session.active_exchange = None;
        }

        let message_count = session.history.len() as u32;
        let detail = SessionDetail {
            session: session.clone(),
            message_count,
        };

        self.sessions.insert(session_id.to_string(), session);

        info!(session_id = %session_id, "chat session resumed from SQLite");

        Ok(detail)
    }

    /// Fork a session — create a child with a copy of the parent's history.
    ///
    /// The parent may be in any non-Closed state; a Closed parent can also be
    /// forked (useful for branching from an archived conversation). The child
    /// inherits mode, system prompt, available tools, and LLM backend from the
    /// parent. Messages up to `up_to_index` are copied (all if `None`).
    fn handle_fork_session(
        &mut self,
        parent_id: &str,
        up_to_index: Option<usize>,
    ) -> Result<SessionInfo, ChatError> {
        // Resolve the message count to copy from in-memory history when available,
        // so the caller can pass an index relative to the current in-memory state.
        let resolved_count = if let Some(n) = up_to_index {
            let parent_len = if let Some(s) = self.sessions.get(parent_id) {
                s.history.len()
            } else {
                // Fall back to repository count — session may not be in memory.
                self.repository
                    .get_messages(parent_id, None)
                    .unwrap_or_default()
                    .len()
            };
            Some(n.min(parent_len))
        } else {
            None
        };

        let child_id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();

        let child =
            self.repository
                .create_fork_session(&child_id, parent_id, resolved_count, &now)?;

        let messages_copied = child.history.len();

        tracing::info!(
            parent_id = %parent_id,
            child_id = %child_id,
            messages_copied = messages_copied,
            "session forked"
        );

        let _ = self.event_bus.send(RuntimeEvent::ChatSessionCreated {
            session_id: child_id.clone(),
            mode: child.mode.as_sql().to_string(),
            agent_name: child.agent_name.clone(),
        });

        let info = session_to_info(&child);
        self.sessions.insert(child_id, child);

        Ok(info)
    }

    /// List direct child sessions (forks) of the given parent.
    fn handle_list_children(&self, parent_id: &str) -> Vec<SessionInfo> {
        match self.repository.list_children(parent_id) {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    let mode = ChatMode::from_sql(&row.mode)?;
                    let status = SessionStatus::from_sql(&row.status)?;
                    Some(SessionInfo {
                        id: row.id,
                        mode,
                        agent_name: row.agent_name,
                        status,
                        created_at: row.created_at,
                        title: row.title,
                        project_id: row.project_id,
                    })
                })
                .collect(),
            Err(e) => {
                error!(parent_id = %parent_id, error = %e, "Failed to list session children");
                Vec::new()
            }
        }
    }

    /// Link or unlink a session to a project.
    fn handle_link_session_to_project(
        &mut self,
        session_id: &str,
        project_id: Option<&str>,
    ) -> Result<(), ChatError> {
        self.repository
            .set_session_project(session_id, project_id)?;

        // Update in-memory cache
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.project_id = project_id.map(|s| s.to_string());
        }
        Ok(())
    }

    /// List sessions belonging to a specific project.
    fn handle_list_sessions_by_project(&self, project_id: &str) -> Vec<SessionInfo> {
        match self.repository.list_sessions_by_project(project_id) {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    let mode = ChatMode::from_sql(&row.mode)?;
                    let status = SessionStatus::from_sql(&row.status)?;
                    Some(SessionInfo {
                        id: row.id,
                        mode,
                        agent_name: row.agent_name,
                        status,
                        created_at: row.created_at,
                        title: row.title,
                        project_id: row.project_id,
                    })
                })
                .collect(),
            Err(e) => {
                error!(project_id = %project_id, error = %e, "Failed to list sessions by project");
                Vec::new()
            }
        }
    }

    /// Restore active sessions from SQLite at boot.
    /// Resolve a pending `ask_user` request by delivering the user's answers.
    fn resolve_user_input_internal(
        &mut self,
        request_id: &str,
        answers: Vec<apollia_tools::tools::ask_user::UserAnswer>,
    ) -> Result<(), ChatError> {
        let reply_tx = self
            .pending_user_replies
            .remove(request_id)
            .ok_or_else(|| {
                ChatError::InternalError(format!(
                    "no pending ask_user request with id '{request_id}'"
                ))
            })?;

        let output = apollia_tools::tools::ask_user::AskUserOutput { answers };
        reply_tx.send(output).map_err(|_| {
            ChatError::InternalError(
                "ask_user reply channel closed (agent may have timed out)".into(),
            )
        })
    }

    fn restore_sessions(&mut self) {
        let rows = match self.repository.list_sessions(Some("active")) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Failed to restore active sessions from SQLite");
                return;
            }
        };

        for row in rows {
            let mode = match ChatMode::from_sql(&row.mode) {
                Some(m) => m,
                None => continue,
            };
            let status = match SessionStatus::from_sql(&row.status) {
                Some(s) => s,
                None => continue,
            };
            let available_tools: Vec<String> =
                serde_json::from_str(&row.available_tools).unwrap_or_default();
            let authorized_tools = self
                .repository
                .get_authorized_tools(&row.id)
                .unwrap_or_default();
            let messages = self
                .repository
                .get_messages(&row.id, None)
                .unwrap_or_default();
            let history: Vec<ChatMessage> = messages
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

            let session = ChatSession {
                id: row.id.clone(),
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
                fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                    std::collections::HashSet::new(),
                )),
            };
            self.sessions.insert(row.id, session);
        }

        if !self.sessions.is_empty() {
            info!(
                count = self.sessions.len(),
                "ChatSessionManager: restored active sessions from SQLite"
            );
        }
    }
}

/// Resolves the sandbox root for a chat session based on its project association.
///
/// Called once per message, inside the async tokio task spawned by `handle_send_message`.
/// Returns a [`NativeChatToolInvoker`] configured with the project's `workspace_path` when
/// available, or falling back to `current_dir()` when the session has no project or the
/// project has no workspace set yet. Never falls back to `$HOME`.
///
/// When `hitl` is provided, HITL filesystem support is enabled on the returned invoker.
async fn resolve_workspace_for_session(
    project_id: &Option<String>,
    project_repo: &Option<Arc<ProjectRepository>>,
    hitl: Option<HitlInvokerParams>,
    pending_user_inputs: Option<apollia_tools::tools::ask_user::PendingUserInputs>,
) -> Result<NativeChatToolInvoker, ChatError> {
    let workspace_path = match project_id {
        None => None,
        Some(pid) => {
            let repo = project_repo
                .as_ref()
                .ok_or_else(|| ChatError::ProjectNotFound(pid.clone()))?;
            let detail = repo
                .get_project_async(pid.clone())
                .await
                .map_err(|_| ChatError::ProjectNotFound(pid.clone()))?;
            if detail.workspace_path.is_none() {
                warn!(
                    project_id = %pid,
                    "project has no workspace_path configured — falling back to current_dir()"
                );
            }
            detail.workspace_path.map(std::path::PathBuf::from)
        }
    };
    let mut invoker = NativeChatToolInvoker::new_unrestricted(workspace_path);
    if let Some(pending) = pending_user_inputs {
        invoker = invoker.with_ask_user_support(pending);
    }
    if let Some(p) = hitl {
        Ok(invoker.with_hitl_support(
            p.session_id,
            p.event_bus,
            p.pending_fs,
            p.fs_allow_rules,
            p.risk_config,
        ))
    } else {
        Ok(invoker)
    }
}

/// Parameters for attaching HITL filesystem support to a `NativeChatToolInvoker`.
struct HitlInvokerParams {
    session_id: String,
    event_bus: EventBusSender,
    pending_fs: super::types::PendingFilesystemApprovals,
    fs_allow_rules: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    risk_config: apollia_core::FilesystemRiskConfig,
}

/// Clonable handle for communicating with the [`ChatSessionManager`] actor.
///
/// All methods are async and return the result via oneshot channels.
/// This handle is `Clone + Send + Sync`.
#[derive(Clone)]
pub struct ChatSessionManagerHandle {
    tx: mpsc::Sender<ChatCommand>,
    /// Shared `ask_user` request registry — cloned by chat agent runners so
    /// their tool dispatcher can register an `AskUserExecutor` whose replies
    /// are routed to this manager's background drainer task.
    pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs,
}

impl ChatSessionManagerHandle {
    /// Return a handle to the shared `ask_user` pending-input registry.
    ///
    /// Chat agent runners pass this to
    /// [`apollia_tools::build_native_dispatcher`] so Python agents in Agent
    /// mode can use the `ask_user` tool with the same HITL loop as the Libre
    /// mode built-in agent.
    pub fn pending_user_inputs(&self) -> apollia_tools::tools::ask_user::PendingUserInputs {
        self.pending_user_inputs.clone()
    }
}

impl ChatSessionManagerHandle {
    /// Spawn the [`ChatSessionManager`] actor and return a handle.
    ///
    /// Opens the SQLite database at `db_path`, restores active sessions,
    /// and starts the actor loop in a background `tokio::spawn`.
    ///
    /// `agent_runner` enables Chat Agent mode. When `None`,
    /// Agent mode sessions will return an error at message time.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        db_path: &Path,
        llm_router: Option<Arc<LlmRouter>>,
        tool_registry: ToolRegistryHandle,
        _agent_loader: Arc<dyn AgentLoader>,
        agent_runner: Option<Arc<dyn ChatAgentRunner>>,
        event_bus: EventBusSender,
        runtime_budget: StepBudgetConfig,
        user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
        registry_handle: AgentRegistryHandle,
        a2a_invoker: Option<Arc<A2AInvoker>>,
        project_context: Option<Arc<dyn ProjectContextProvider>>,
        project_repo: Option<Arc<ProjectRepository>>,
    ) -> Result<Self, ChatError> {
        let repository = ChatSessionRepository::open(db_path)?;
        let pending_chat_approvals = PendingChatApprovals::new();
        let pending_fs_approvals = PendingFilesystemApprovals::new();
        let pending_user_inputs = apollia_tools::tools::ask_user::PendingUserInputs::new();

        let enrichment_extractor = match (&llm_router, &user_memory) {
            (Some(llm), Some(mem)) => {
                let ext = UserMemoryExtractor::new(Arc::clone(llm), Arc::clone(mem));
                Some(Arc::new(tokio::sync::Mutex::new(ext)))
            }
            _ => None,
        };

        let (tx, rx) = mpsc::channel(256);

        let mut manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router,
            tool_registry,
            registry_handle,
            agent_runner,
            event_bus,
            runtime_budget,
            pending_chat_approvals,
            pending_fs_approvals,
            user_memory,
            enrichment_extractor,
            tx: tx.clone(),
            a2a_invoker,
            project_context,
            project_repo,
            pending_user_inputs: pending_user_inputs.clone(),
            pending_user_replies: HashMap::new(),
        };

        // Restore active sessions from SQLite before entering the actor loop
        manager.restore_sessions();

        tokio::spawn(manager.run(rx));

        // Background task: drain PendingUserInputs and route to the actor.
        // When the `ask_user` executor posts a request, this task picks it up,
        // forwards the reply channel to the manager, and emits the event.
        {
            let pending = pending_user_inputs.clone();
            let cmd_tx = tx.clone();
            tokio::spawn(async move {
                loop {
                    match pending.next_pending().await {
                        Some((request_id, pending_input)) => {
                            let questions_json = serde_json::to_string(&pending_input.questions)
                                .unwrap_or_else(|_| "[]".to_string());
                            let _ = cmd_tx
                                .send(ChatCommand::RegisterUserInputReply {
                                    request_id,
                                    questions_json,
                                    context: pending_input.context,
                                    reply_tx: pending_input.reply_tx,
                                })
                                .await;
                        }
                        None => break, // Channel closed — manager shutting down.
                    }
                }
            });
        }

        Ok(Self {
            tx,
            pending_user_inputs,
        })
    }

    /// Create a new chat session.
    pub async fn create_session(
        &self,
        mode: ChatMode,
        agent_name: Option<String>,
        system_prompt: Option<String>,
        tools: Vec<String>,
        project_id: Option<String>,
    ) -> Result<SessionInfo, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::CreateSession {
                mode,
                agent_name,
                system_prompt,
                tools,
                project_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Send a user message.
    pub async fn send_message(
        &self,
        session_id: SessionId,
        content: String,
    ) -> Result<MessageId, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::SendMessage {
                session_id,
                content,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Resolve a pending filesystem HITL request.
    ///
    /// Called by the `respond_hitl_filesystem` Tauri command.
    pub async fn resolve_fs_hitl(
        &self,
        request_id: String,
        decision: super::types::FsHitlDecision,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ResolveFsHitl {
                request_id,
                decision,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Resolve a pending tool approval.
    pub async fn resolve_tool(
        &self,
        session_id: SessionId,
        message_id: MessageId,
        tool_name: String,
        decision: ToolDecision,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ResolveTool {
                session_id,
                message_id,
                tool_name,
                decision,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Deliver user answers to a pending `ask_user` request.
    pub async fn resolve_user_input(
        &self,
        request_id: String,
        answers: Vec<apollia_tools::tools::ask_user::UserAnswer>,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ResolveUserInput {
                request_id,
                answers,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Update session configuration (system_prompt, tools, llm_backend).
    pub async fn update_session(
        &self,
        session_id: SessionId,
        system_prompt: Option<String>,
        available_tools: Option<Vec<String>>,
        llm_backend: Option<Option<String>>,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::UpdateSession {
                session_id,
                system_prompt,
                available_tools,
                llm_backend,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// List sessions with optional status filter.
    pub async fn list_sessions(&self, status_filter: Option<SessionStatus>) -> Vec<SessionInfo> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::ListSessions {
                status_filter,
                reply: reply_tx,
            })
            .await;

        if sent.is_err() {
            return Vec::new();
        }

        reply_rx.await.unwrap_or_default()
    }

    /// Get detailed session info.
    pub async fn get_session(&self, session_id: SessionId) -> Option<SessionDetail> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::GetSession {
                session_id,
                reply: reply_tx,
            })
            .await;

        if sent.is_err() {
            return None;
        }

        reply_rx.await.ok().flatten()
    }

    /// Close a session.
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::CloseSession {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Delete a session and all its data.
    pub async fn delete_session(&self, session_id: SessionId) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::DeleteSession {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Rename a session (set a user-defined title).
    pub async fn rename_session(
        &self,
        session_id: SessionId,
        title: String,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::RenameSession {
                session_id,
                title,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Signal the actor to shut down.
    /// Hot-reload the LLM router used by the chat subsystem.
    ///
    /// Called after the user configures a new LLM backend (e.g. during
    /// onboarding). The new router is used for all subsequent requests.
    pub async fn reload_llm(&self, router: Option<Arc<LlmRouter>>) {
        let _ = self.tx.send(ChatCommand::ReloadLlm { router }).await;
    }

    /// List the N most recent sessions with their first user message.
    ///
    /// Returns an empty vec if the actor is unreachable or the query fails.
    pub async fn list_recent_summaries(&self, limit: usize) -> Vec<RecentSessionSummary> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::GetRecentSummaries {
                limit,
                reply: reply_tx,
            })
            .await;

        if sent.is_err() {
            return Vec::new();
        }

        reply_rx.await.unwrap_or_default()
    }

    /// Load a session from SQLite (if not already in memory) and return its full detail.
    ///
    /// Resets `Processing` status to `Active` so the session can immediately accept
    /// new messages. Returns `Err(ChatError::SessionNotFound)` if the ID is unknown.
    pub async fn resume_session(&self, session_id: SessionId) -> Result<SessionDetail, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ResumeSession {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Fork an existing session, producing a new child session.
    ///
    /// `up_to_index` controls how many messages are copied: `None` copies
    /// the full history, `Some(n)` copies the first `n` messages.
    pub async fn fork_session(
        &self,
        session_id: SessionId,
        up_to_index: Option<usize>,
    ) -> Result<SessionInfo, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ForkSession {
                session_id,
                up_to_index,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// List all direct child sessions (forks) of the given parent.
    ///
    /// Returns an empty vec if the actor is unreachable or the query fails.
    pub async fn list_children(&self, session_id: SessionId) -> Vec<SessionInfo> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::ListChildren {
                session_id,
                reply: reply_tx,
            })
            .await;

        if sent.is_err() {
            return Vec::new();
        }

        reply_rx.await.unwrap_or_default()
    }

    /// List all A2A skills available from active worker agents.
    ///
    /// Returns an empty vec when A2A is not wired or the actor is unreachable.
    /// Link or unlink a session to a project.
    pub async fn link_session_to_project(
        &self,
        session_id: SessionId,
        project_id: Option<String>,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::LinkSessionToProject {
                session_id,
                project_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// List sessions belonging to a specific project.
    pub async fn list_sessions_by_project(&self, project_id: String) -> Vec<SessionInfo> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::ListSessionsByProject {
                project_id,
                reply: reply_tx,
            })
            .await;

        if sent.is_err() {
            return Vec::new();
        }

        reply_rx.await.unwrap_or_default()
    }

    /// Orphan all sessions linked to a project (called on project deletion).
    pub async fn orphan_project_sessions(&self, project_id: String) {
        let _ = self
            .tx
            .send(ChatCommand::OrphanProjectSessions { project_id })
            .await;
    }

    pub async fn list_a2a_skills(&self) -> Vec<crate::a2a::SkillListing> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::ListA2ASkills { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// List recently resolved chat tool approvals from the approval log.
    pub async fn list_approval_history(
        &self,
        limit: i64,
        days: i64,
    ) -> Result<Vec<super::repository::ChatApprovalLogRow>, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ListApprovalHistory {
                limit,
                days,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor unavailable".into()))?;
        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    pub async fn shutdown(&self) {
        let _ = self.tx.send(ChatCommand::Shutdown).await;
    }
}

/// Convert a [`ChatSession`] into a lightweight [`SessionInfo`].
fn session_to_info(session: &ChatSession) -> SessionInfo {
    SessionInfo {
        id: session.id.clone(),
        mode: session.mode.clone(),
        agent_name: session.agent_name.clone(),
        status: session.status.clone(),
        created_at: session.created_at.clone(),
        title: session.title.clone(),
        project_id: session.project_id.clone(),
    }
}

/// Return the current time as an RFC-3339/ISO-8601 string.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::AgentManifest;
    use std::path::PathBuf;

    /// Stub AgentLoader that always succeeds.
    struct AlwaysOkLoader;
    impl AgentLoader for AlwaysOkLoader {
        fn load_and_validate(&self, _path: &Path) -> Result<AgentManifest, String> {
            Ok(AgentManifest {
                name: "test-agent".into(),
                version: "0.1.0".into(),
                description: "test".into(),
                tools_required: vec![],
                tools_optional: vec![],
                supports_streaming: false,
                supports_a2a: false,
                memory_namespace: None,
                shared_memory_namespaces: vec![],
                max_concurrent_tasks: 1,
                step_budget: None,
                network_allowlist: None,
                dangerous_tools_allowed: false,
                tags: vec![],
                skills: vec![],
                execution_mode: "auto".into(),
                system_prompt: None,
                tools_requiring_approval: vec![],
                llm_backend: None,
                packages: vec![],
                memory_config: None,
                agent_type: None,
                examples: vec![],
                limitations: vec![],
                setup_notes: None,
            })
        }
    }

    /// Stub AgentLoader that always fails.
    struct AlwaysFailLoader;
    impl AgentLoader for AlwaysFailLoader {
        fn load_and_validate(&self, _path: &Path) -> Result<AgentManifest, String> {
            Err("agent not found".into())
        }
    }

    /// Spawn a ChatSessionManager backed by a temp SQLite database.
    fn spawn_test_manager(
        dir: &tempfile::TempDir,
        llm_router: Option<Arc<LlmRouter>>,
        agent_loader: Arc<dyn AgentLoader>,
    ) -> ChatSessionManagerHandle {
        let db_path = dir.path().join("chat.db");
        let (event_tx, _) = tokio::sync::broadcast::channel(128);
        let tool_registry = ToolRegistryHandle::start();
        let registry_handle = crate::registry::AgentRegistry::spawn(event_tx.clone());
        ChatSessionManagerHandle::spawn(
            &db_path,
            llm_router,
            tool_registry,
            agent_loader,
            None, // no agent runner in basic tests
            event_tx,
            StepBudgetConfig::default(),
            None, // no user memory in basic tests
            registry_handle,
            None, // no A2A invoker in basic tests
            None, // no project context in basic tests
            None, // no project repo in basic tests
        )
        .expect("spawn manager")
    }

    fn fake_llm_router() -> Option<Arc<LlmRouter>> {
        Some(Arc::new(LlmRouter::empty()))
    }

    #[tokio::test]
    async fn test_create_session_libre() {
        // GIVEN a ChatSessionManager with LLM configured
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        // WHEN create_session mode=Libre
        let info = handle
            .create_session(
                ChatMode::Libre,
                None,
                None,
                vec!["bash_executor".into()],
                None,
            )
            .await
            .expect("create_session");

        // THEN Ok(SessionInfo) with status=Active
        assert_eq!(info.mode, ChatMode::Libre);
        assert_eq!(info.status, SessionStatus::Active);
        assert!(info.agent_name.is_none());

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_create_session_agent_without_name() {
        // GIVEN a ChatSessionManager
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        // WHEN create_session mode=Agent, agent_name=None
        let result = handle
            .create_session(ChatMode::Agent, None, None, vec![], None)
            .await;

        // THEN Err(ChatError::AgentNotFound)
        assert!(matches!(result, Err(ChatError::AgentNotFound(_))));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_create_session_no_llm() {
        // GIVEN a ChatSessionManager without LLM
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, None, Arc::new(AlwaysOkLoader));

        // WHEN create_session mode=Libre
        let result = handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await;

        // THEN Err(ChatError::NoLlmConfigured)
        assert!(matches!(result, Err(ChatError::NoLlmConfigured)));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_list_sessions() {
        // GIVEN 2 sessions created
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await
            .expect("create 1");
        handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await
            .expect("create 2");

        // WHEN list_sessions
        let sessions = handle.list_sessions(None).await;

        // THEN 2 sessions returned
        assert_eq!(sessions.len(), 2);

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_get_session_detail() {
        // GIVEN session with 3 messages
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        let info = handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await
            .expect("create");

        for i in 0..3 {
            handle
                .send_message(info.id.clone(), format!("message {i}"))
                .await
                .expect("send");
        }

        // WHEN get_session
        let detail = handle.get_session(info.id.clone()).await;

        // THEN SessionDetail with 3 messages
        let detail = detail.expect("should exist");
        assert_eq!(detail.message_count, 3);
        assert_eq!(detail.session.history.len(), 3);

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_close_session() {
        // GIVEN session active
        let dir = tempfile::tempdir().expect("tempdir");
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let db_path = dir.path().join("chat.db");
        let tool_registry = ToolRegistryHandle::start();
        let registry_handle = crate::registry::AgentRegistry::spawn(event_tx.clone());
        let handle = ChatSessionManagerHandle::spawn(
            &db_path,
            fake_llm_router(),
            tool_registry,
            Arc::new(AlwaysOkLoader),
            None,
            event_tx,
            StepBudgetConfig::default(),
            None,
            registry_handle,
            None,
            None,
            None,
        )
        .expect("spawn");

        let info = handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await
            .expect("create");

        // Drain the ChatSessionCreated event
        let _ = event_rx.recv().await;

        // WHEN close_session
        handle.close_session(info.id.clone()).await.expect("close");

        // THEN ChatSessionClosed event is emitted
        let event = event_rx.recv().await.expect("event");
        assert!(matches!(event, RuntimeEvent::ChatSessionClosed { .. }));

        // AND session detail shows Closed
        let detail = handle.get_session(info.id.clone()).await.expect("detail");
        assert_eq!(detail.session.status, SessionStatus::Closed);

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_send_message_to_closed_session() {
        // GIVEN session closed
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        let info = handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await
            .expect("create");
        handle.close_session(info.id.clone()).await.expect("close");

        // WHEN send_message
        let result = handle.send_message(info.id.clone(), "hello".into()).await;

        // THEN Err(ChatError::SessionClosed)
        assert!(matches!(result, Err(ChatError::SessionClosed(_))));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_send_message_returns_message_id() {
        // GIVEN active session
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        let info = handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await
            .expect("create");

        // WHEN send_message
        let msg_id = handle
            .send_message(info.id.clone(), "Bonjour".into())
            .await
            .expect("send");

        // THEN a valid message ID is returned
        assert!(!msg_id.is_empty());

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_resolve_tool_approval() {
        // GIVEN a manager with a registered pending approval
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("chat.db");
        let (event_tx, _) = tokio::sync::broadcast::channel(128);
        let tool_registry = ToolRegistryHandle::start();
        let repository = ChatSessionRepository::open(&db_path).expect("open");
        let pending = PendingChatApprovals::new();
        let rx = pending.register("sess-1::msg-1::bash".to_string());

        let (tx, _rx) = mpsc::channel(256);
        // Manually build manager to inject pending_chat_approvals
        let mut manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router: fake_llm_router(),
            tool_registry,
            registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
            agent_runner: None,
            event_bus: event_tx,
            runtime_budget: StepBudgetConfig::default(),
            pending_chat_approvals: pending,
            pending_fs_approvals: PendingFilesystemApprovals::new(),
            pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
            pending_user_replies: HashMap::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
            a2a_invoker: None,
            project_context: None,
            project_repo: None,
        };

        // Insert a dummy session so the lookup succeeds
        let session = ChatSession {
            id: "sess-1".into(),
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: String::new(),
            status: SessionStatus::Processing,
            history: vec![],
            authorized_tools: std::collections::HashSet::new(),
            available_tools: vec!["bash".into()],
            created_at: "2026-03-20T10:00:00Z".into(),
            active_exchange: None,
            llm_backend: None,
            title: None,
            parent_session_id: None,
            fork_depth: 0,
            project_id: None,
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
        };
        manager.sessions.insert("sess-1".into(), session);

        // WHEN resolve_tool Accept
        let result = manager.handle_resolve_tool("sess-1", "msg-1", "bash", ToolDecision::Accept);

        // THEN ok, approval resolved
        assert!(result.is_ok());

        // AND the receiver gets the decision
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::Accept);
    }

    #[tokio::test]
    async fn test_shutdown() {
        // GIVEN a ChatSessionManager spawned
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        // WHEN shutdown
        handle.shutdown().await;

        // THEN the actor stops — subsequent sends fail gracefully
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let result = handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_close_already_closed_session() {
        // GIVEN a closed session
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        let info = handle
            .create_session(ChatMode::Libre, None, None, vec![], None)
            .await
            .expect("create");
        handle.close_session(info.id.clone()).await.expect("close");

        // WHEN close_session again
        let result = handle.close_session(info.id.clone()).await;

        // THEN Err(ChatError::SessionClosed)
        assert!(matches!(result, Err(ChatError::SessionClosed(_))));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_cross_session_context_substantive_message() {
        // GIVEN 3 past sessions with summaries
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("chat.db");

        let (event_tx, _) = tokio::sync::broadcast::channel(128);
        let tool_registry = ToolRegistryHandle::start();
        let (tx, _rx) = mpsc::channel(256);
        let repository = ChatSessionRepository::open(&db_path).expect("open");

        // Seed past sessions with summaries on the same repository instance
        for (id, summary, ts) in [
            (
                "past-1",
                "Discussion about data migration project using batch processing",
                "2026-03-20T10:00:00Z",
            ),
            (
                "past-2",
                "Review of API design for user management endpoints",
                "2026-03-18T10:00:00Z",
            ),
            (
                "past-3",
                "Setup of CI/CD pipeline with GitHub Actions",
                "2026-03-15T10:00:00Z",
            ),
        ] {
            repository
                .create_session(id, &ChatMode::Libre, None, "", &[], ts, None, None)
                .expect("create");
            repository.close_session(id, ts).expect("close");
            repository.update_summary(id, summary).expect("summary");
        }

        let manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router: fake_llm_router(),
            tool_registry,
            registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
            agent_runner: None,
            event_bus: event_tx,
            runtime_budget: StepBudgetConfig::default(),
            pending_chat_approvals: PendingChatApprovals::new(),
            pending_fs_approvals: PendingFilesystemApprovals::new(),
            pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
            pending_user_replies: HashMap::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
            a2a_invoker: None,
            project_context: None,
            project_repo: None,
        };

        // WHEN building cross-session context with a substantive first message
        let context =
            manager.build_cross_session_context("data migration project batch processing");

        // THEN a context block with past sessions is returned
        let block = context.expect("should have cross-session context");
        assert!(block.starts_with("## Previous conversations (for reference)\n"));
        assert!(block.contains("migration"));
    }

    #[tokio::test]
    async fn test_cross_session_context_trivial_message() {
        // GIVEN past sessions with summaries
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("chat.db");

        let (event_tx, _) = tokio::sync::broadcast::channel(128);
        let tool_registry = ToolRegistryHandle::start();
        let (tx, _rx) = mpsc::channel(256);
        let repository = ChatSessionRepository::open(&db_path).expect("open");

        repository
            .create_session(
                "past-1",
                &ChatMode::Libre,
                None,
                "",
                &[],
                "2026-03-20T10:00:00Z",
                None,
                None,
            )
            .expect("create");
        repository
            .close_session("past-1", "2026-03-20T12:00:00Z")
            .expect("close");
        repository
            .update_summary("past-1", "Discussion about data migration")
            .expect("summary");

        let manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router: fake_llm_router(),
            tool_registry,
            registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
            agent_runner: None,
            event_bus: event_tx,
            runtime_budget: StepBudgetConfig::default(),
            pending_chat_approvals: PendingChatApprovals::new(),
            pending_fs_approvals: PendingFilesystemApprovals::new(),
            pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
            pending_user_replies: HashMap::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
            a2a_invoker: None,
            project_context: None,
            project_repo: None,
        };

        // WHEN building cross-session context with a trivial message
        let context = manager.build_cross_session_context("bonjour");

        // THEN None is returned (message too short)
        assert!(context.is_none());
    }

    #[tokio::test]
    async fn test_cross_session_context_no_relevant_sessions() {
        // GIVEN a repository with no sessions (empty)
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("chat.db");

        let (event_tx, _) = tokio::sync::broadcast::channel(128);
        let tool_registry = ToolRegistryHandle::start();
        let (tx, _rx) = mpsc::channel(256);
        let repository = ChatSessionRepository::open(&db_path).expect("open");

        let manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router: fake_llm_router(),
            tool_registry,
            registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
            agent_runner: None,
            event_bus: event_tx,
            runtime_budget: StepBudgetConfig::default(),
            pending_chat_approvals: PendingChatApprovals::new(),
            pending_fs_approvals: PendingFilesystemApprovals::new(),
            pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs::new(),
            pending_user_replies: HashMap::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
            a2a_invoker: None,
            project_context: None,
            project_repo: None,
        };

        // WHEN building cross-session context with a substantive message but no past sessions
        let context =
            manager.build_cross_session_context("data migration project batch processing");

        // THEN None is returned (no relevant sessions found)
        assert!(context.is_none());
    }

    #[tokio::test]
    async fn test_create_session_agent_invalid_agent() {
        // GIVEN a manager with a loader that always fails
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysFailLoader));

        // WHEN create_session mode=Agent with an agent name
        let result = handle
            .create_session(
                ChatMode::Agent,
                Some("nonexistent".into()),
                None,
                vec![],
                None,
            )
            .await;

        // THEN Err(ChatError::AgentNotFound)
        assert!(matches!(result, Err(ChatError::AgentNotFound(_))));

        handle.shutdown().await;
    }
}
