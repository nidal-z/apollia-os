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
use apollia_tools::ToolRegistryHandle;

use super::agent_chat::{AgentChatExecutor, ChatAgentRunner};
use super::builtin_agent::{BuiltInChatAgent, ChatAgentResponse, DEFAULT_CONTEXT_WINDOW_SIZE};
use super::extractor::UserMemoryExtractor;
use super::repository::{AppendMessageParams, ChatSessionRepository};
use super::types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, ExchangeState, MessageId,
    PendingChatApprovals, SessionDetail, SessionId, SessionInfo, SessionStatus, ToolCallRecord,
    ToolDecision,
};
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
    /// Hot-reload the LLM router (e.g. after onboarding setup).
    ReloadLlm {
        /// New router to use for subsequent requests.
        router: Option<Arc<LlmRouter>>,
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
    /// Tool invoker for actual tool execution (ADR-015).
    tool_invoker: Arc<dyn ToolInvoker>,
    /// Agent registry for resolving agent names to IDs.
    registry_handle: AgentRegistryHandle,
    /// Agent runner for Chat Agent mode. `None` disables Agent mode.
    agent_runner: Option<Arc<dyn ChatAgentRunner>>,
    /// Event bus sender for runtime events.
    event_bus: EventBusSender,
    /// Runtime-level step budget configuration.
    runtime_budget: StepBudgetConfig,
    /// Pending tool approval channels.
    pending_chat_approvals: PendingChatApprovals,
    /// Optional user memory repository for system prompt enrichment.
    user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Stateful extractor for passive memory enrichment from conversations.
    enrichment_extractor: Option<Arc<tokio::sync::Mutex<UserMemoryExtractor>>>,
    /// Sender clone for spawned tasks to send commands back to the actor.
    tx: mpsc::Sender<ChatCommand>,
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
                    reply,
                } => {
                    let result = self
                        .handle_create_session(mode, agent_name, system_prompt, tools)
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
                ChatCommand::ReloadLlm { router } => {
                    info!("ChatSessionManager: LLM router reloaded");
                    self.llm_router = router;
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

        if session.mode == ChatMode::Libre {
            let llm_router = self.llm_router.clone().ok_or(ChatError::NoLlmConfigured)?;

            let agent = BuiltInChatAgent::new(
                llm_router,
                self.tool_registry.clone(),
                Arc::clone(&self.tool_invoker),
                self.event_bus.clone(),
                self.user_memory.clone(),
            );

            let history = session.history.clone();
            // On the first message, enrich the system prompt with cross-session context
            let system_prompt = if history.len() == 1 {
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

            tokio::spawn(async move {
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

        // Persist assistant response message
        match self.repository.append_message(&AppendMessageParams {
            id: &assistant_msg_id,
            session_id,
            role: &ChatRole::Assistant,
            content: &response.content,
            tool_calls_json: tool_calls_json.as_deref(),
            tool_name: None,
            created_at: &now,
        }) {
            Ok(seq) => {
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
                    metadata: None,
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
                ChatMessage {
                    id: m.id,
                    role,
                    content: m.content,
                    tool_calls,
                    tool_name: m.tool_name,
                    created_at: m.created_at,
                    seq: m.seq,
                    metadata: None,
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

    /// Restore active sessions from SQLite at boot.
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
                    ChatMessage {
                        id: m.id,
                        role,
                        content: m.content,
                        tool_calls,
                        tool_name: m.tool_name,
                        created_at: m.created_at,
                        seq: m.seq,
                        metadata: None,
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

/// Clonable handle for communicating with the [`ChatSessionManager`] actor.
///
/// All methods are async and return the result via oneshot channels.
/// This handle is `Clone + Send + Sync`.
#[derive(Clone)]
pub struct ChatSessionManagerHandle {
    tx: mpsc::Sender<ChatCommand>,
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
        tool_invoker: Arc<dyn ToolInvoker>,
        _agent_loader: Arc<dyn AgentLoader>,
        agent_runner: Option<Arc<dyn ChatAgentRunner>>,
        event_bus: EventBusSender,
        runtime_budget: StepBudgetConfig,
        user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
        registry_handle: AgentRegistryHandle,
    ) -> Result<Self, ChatError> {
        let repository = ChatSessionRepository::open(db_path)?;
        let pending_chat_approvals = PendingChatApprovals::new();

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
            tool_invoker,
            registry_handle,
            agent_runner,
            event_bus,
            runtime_budget,
            pending_chat_approvals,
            user_memory,
            enrichment_extractor,
            tx: tx.clone(),
        };

        // Restore active sessions from SQLite before entering the actor loop
        manager.restore_sessions();

        tokio::spawn(manager.run(rx));

        Ok(Self { tx })
    }

    /// Create a new chat session.
    pub async fn create_session(
        &self,
        mode: ChatMode,
        agent_name: Option<String>,
        system_prompt: Option<String>,
        tools: Vec<String>,
    ) -> Result<SessionInfo, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::CreateSession {
                mode,
                agent_name,
                system_prompt,
                tools,
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

    /// Stub ToolInvoker for manager tests (tool execution tested in builtin_agent).
    struct NoopTestInvoker;

    #[async_trait::async_trait]
    impl ToolInvoker for NoopTestInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            Ok("ok".into())
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
        let tool_invoker: Arc<dyn ToolInvoker> = Arc::new(NoopTestInvoker);
        let registry_handle = crate::registry::AgentRegistry::spawn(event_tx.clone());
        ChatSessionManagerHandle::spawn(
            &db_path,
            llm_router,
            tool_registry,
            tool_invoker,
            agent_loader,
            None, // no agent runner in basic tests
            event_tx,
            StepBudgetConfig::default(),
            None, // no user memory in basic tests
            registry_handle,
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
            .create_session(ChatMode::Libre, None, None, vec!["bash_executor".into()])
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
            .create_session(ChatMode::Agent, None, None, vec![])
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
            .create_session(ChatMode::Libre, None, None, vec![])
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
            .create_session(ChatMode::Libre, None, None, vec![])
            .await
            .expect("create 1");
        handle
            .create_session(ChatMode::Libre, None, None, vec![])
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
            .create_session(ChatMode::Libre, None, None, vec![])
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
        let tool_invoker: Arc<dyn ToolInvoker> = Arc::new(NoopTestInvoker);
        let registry_handle = crate::registry::AgentRegistry::spawn(event_tx.clone());
        let handle = ChatSessionManagerHandle::spawn(
            &db_path,
            fake_llm_router(),
            tool_registry,
            tool_invoker,
            Arc::new(AlwaysOkLoader),
            None,
            event_tx,
            StepBudgetConfig::default(),
            None,
            registry_handle,
        )
        .expect("spawn");

        let info = handle
            .create_session(ChatMode::Libre, None, None, vec![])
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
            .create_session(ChatMode::Libre, None, None, vec![])
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
            .create_session(ChatMode::Libre, None, None, vec![])
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
        let tool_invoker: Arc<dyn ToolInvoker> = Arc::new(NoopTestInvoker);
        // Manually build manager to inject pending_chat_approvals
        let mut manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router: fake_llm_router(),
            tool_registry,
            tool_invoker,
            registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
            agent_runner: None,
            event_bus: event_tx,
            runtime_budget: StepBudgetConfig::default(),
            pending_chat_approvals: pending,
            user_memory: None,
            enrichment_extractor: None,
            tx,
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
            .create_session(ChatMode::Libre, None, None, vec![])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_close_already_closed_session() {
        // GIVEN a closed session
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        let info = handle
            .create_session(ChatMode::Libre, None, None, vec![])
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
        let tool_invoker: Arc<dyn ToolInvoker> = Arc::new(NoopTestInvoker);
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
                .create_session(id, &ChatMode::Libre, None, "", &[], ts, None)
                .expect("create");
            repository.close_session(id, ts).expect("close");
            repository.update_summary(id, summary).expect("summary");
        }

        let manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router: fake_llm_router(),
            tool_registry,
            tool_invoker,
            registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
            agent_runner: None,
            event_bus: event_tx,
            runtime_budget: StepBudgetConfig::default(),
            pending_chat_approvals: PendingChatApprovals::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
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
        let tool_invoker: Arc<dyn ToolInvoker> = Arc::new(NoopTestInvoker);
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
            tool_invoker,
            registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
            agent_runner: None,
            event_bus: event_tx,
            runtime_budget: StepBudgetConfig::default(),
            pending_chat_approvals: PendingChatApprovals::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
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
        let tool_invoker: Arc<dyn ToolInvoker> = Arc::new(NoopTestInvoker);
        let (tx, _rx) = mpsc::channel(256);
        let repository = ChatSessionRepository::open(&db_path).expect("open");

        let manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router: fake_llm_router(),
            tool_registry,
            tool_invoker,
            registry_handle: crate::registry::AgentRegistry::spawn(event_tx.clone()),
            agent_runner: None,
            event_bus: event_tx,
            runtime_budget: StepBudgetConfig::default(),
            pending_chat_approvals: PendingChatApprovals::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
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
            .create_session(ChatMode::Agent, Some("nonexistent".into()), None, vec![])
            .await;

        // THEN Err(ChatError::AgentNotFound)
        assert!(matches!(result, Err(ChatError::AgentNotFound(_))));

        handle.shutdown().await;
    }
}
