use super::*;

/// Clonable handle for communicating with the [`ChatSessionManager`] actor.
///
/// All methods are async and return the result via oneshot channels.
/// This handle is `Clone + Send + Sync`.
#[derive(Clone)]
pub struct ChatSessionManagerHandle {
    pub(in crate::chat::manager) tx: mpsc::Sender<ChatCommand>,
    /// Shared `ask_user` request registry, cloned by chat agent runners so
    /// their tool dispatcher can register an `AskUserExecutor` whose replies
    /// are routed to this manager's background drainer task.
    pub(in crate::chat::manager) pending_user_inputs:
        apollia_tools::tools::ask_user::PendingUserInputs,
    /// Shared lifecycle hook executor, exposed so the API can list the active
    /// handlers. `None` when no hooks are configured.
    pub(in crate::chat::manager) hook_executor: Option<Arc<HookExecutor>>,
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

    /// Return a summary of every registered lifecycle hook handler, in
    /// declaration order. Empty when no hooks are configured.
    pub fn hook_summaries(&self) -> Vec<crate::hooks::HookHandlerSummary> {
        self.hook_executor
            .as_ref()
            .map(|exec| exec.registry().list_all())
            .unwrap_or_default()
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
        mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
        // Supervisor-built config so the chat dispatcher can include
        // config-dependent natives (web_search, web_read, http_fetch,
        // memory_search, permission_rule_*) alongside MCP + connectors.
        // `None` keeps the minimal behaviour (only read-only file tools in
        // the dispatcher, the rest in the fast path).
        chat_tools_config: Option<Arc<ChatToolsConfig>>,
        // MCP tool loading strategy and the synthetic `tool_search` result cap,
        // sourced from the `[mcp]` section of `apollia.toml`.
        mcp_loading: LoadingMode,
        tool_search_limit: usize,
        // Shared lifecycle hook executor (PreToolUse blocking, plus best-effort
        // hooks). `None` disables hooks: the ReAct loop runs unchanged.
        hook_executor: Option<Arc<HookExecutor>>,
        // Default plan-mode state applied to every new session at creation,
        // sourced from the `[chat] plan_mode_default` config key.
        plan_mode_default: bool,
    ) -> Result<Self, ChatError> {
        let repository = ChatSessionRepository::open(db_path)?;

        // Spawn the todo actor on its own connection to the same chat database.
        // The migration runs synchronously here so a schema failure stops the
        // runtime at startup (fail fast) rather than on the first todo write.
        let todo_conn = rusqlite::Connection::open(db_path)
            .map_err(|e| ChatError::InternalError(format!("failed to open todo db: {e}")))?;
        let todo_handle = Some(
            spawn_todo_actor(todo_conn, Some(event_bus.clone()))
                .map_err(|e| ChatError::InternalError(format!("todo migration failed: {e}")))?,
        );

        // Spawn the plan actor on its own connection to the same chat database.
        // The migration runs synchronously here (fail fast), exactly like the
        // todo actor above.
        let plan_conn = rusqlite::Connection::open(db_path)
            .map_err(|e| ChatError::InternalError(format!("failed to open plan db: {e}")))?;
        let plan_handle = Some(
            spawn_plan_actor(plan_conn, Some(event_bus.clone()))
                .map_err(|e| ChatError::InternalError(format!("plan migration failed: {e}")))?,
        );

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

        // Keep a handle-side clone so the API can list active hook handlers.
        let hook_executor_for_handle = hook_executor.clone();

        let mut manager = ChatSessionManager {
            sessions: HashMap::new(),
            repository,
            llm_router,
            tool_registry,
            registry_handle,
            agent_runner,
            event_bus,
            runtime_budget,
            plan_mode_default,
            pending_chat_approvals,
            pending_fs_approvals,
            user_memory,
            enrichment_extractor,
            tx: tx.clone(),
            a2a_invoker,
            project_context,
            project_repo,
            pending_user_inputs: pending_user_inputs.clone(),
            mcp_handle,
            chat_tools_config,
            pending_user_replies: HashMap::new(),
            metrics: HashMap::new(),
            mcp_loading,
            tool_search_limit,
            todo_handle,
            plan_handle,
            hook_executor,
            pause_tokens: HashMap::new(),
            pause_states: HashMap::new(),
            pending_injections: HashMap::new(),
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
                // Exits when the channel closes and the manager shuts down.
                while let Some((request_id, pending_input)) = pending.next_pending().await {
                    let questions_json = serde_json::to_string(&pending_input.questions)
                        .unwrap_or_else(|_| "[]".to_string());
                    let _ = cmd_tx
                        .send(ChatCommand::RegisterUserInputReply {
                            request_id,
                            session_id: pending_input.session_id.unwrap_or_default(),
                            questions_json,
                            context: pending_input.context,
                            reply_tx: pending_input.reply_tx,
                        })
                        .await;
                }
            });
        }

        Ok(Self {
            tx,
            pending_user_inputs,
            hook_executor: hook_executor_for_handle,
        })
    }

    /// Create a new chat session.
    pub async fn create_session(
        &self,
        params: CreateSessionParams,
    ) -> Result<SessionInfo, ChatError> {
        let CreateSessionParams {
            mode,
            agent_name,
            system_prompt,
            tools,
            project_id,
        } = params;
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
        decision: super::super::types::FsHitlDecision,
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
    // Session/message/tool-call/tool identifiers plus the decision map one-to-one
    // onto the actor command; a struct would only add indirection.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_tool(
        &self,
        session_id: SessionId,
        message_id: MessageId,
        tool_call_id: String,
        tool_name: String,
        decision: ToolDecision,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ResolveTool {
                session_id,
                message_id,
                tool_call_id,
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

    /// Reject a pending `ask_user` request with a mandatory reason.
    pub async fn reject_user_input(
        &self,
        request_id: String,
        reason: String,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::RejectUserInput {
                request_id,
                reason,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// List all currently pending `ask_user` requests.
    pub async fn list_pending_user_inputs(&self) -> Result<Vec<PendingUserInputView>, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ListPendingUserInputs { reply: reply_tx })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))
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

    /// List the in-memory (scope=session) authorizations on active sessions.
    /// Returns an empty vector if the actor does not respond.
    pub async fn list_session_authorizations(&self) -> Vec<SessionAuthorizationView> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::ListSessionAuthorizations { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Remove a `scope=session` authorization from an active session.
    /// Returns `true` if the entry existed and was removed.
    pub async fn revoke_session_authorization(
        &self,
        session_id: String,
        tool_name: String,
    ) -> Result<bool, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::RevokeSessionAuthorization {
                session_id,
                tool_name,
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

    /// Resolve the effective working directory for a session.
    ///
    /// Mirrors the sandbox root the agent uses for its file tools, so a caller
    /// can reveal a relative path against the exact directory the agent used.
    /// Returns `None` when the actor is unreachable or no directory resolves;
    /// the caller then reveals the raw path best-effort.
    pub async fn resolve_session_workspace(
        &self,
        session_id: SessionId,
    ) -> Option<std::path::PathBuf> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::ResolveSessionWorkspace {
                session_id,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return None;
        }
        reply_rx.await.ok().flatten()
    }

    /// Read the todo list for a session.
    ///
    /// # Errors
    ///
    /// - [`ChatError::SessionNotFound`] when the session is unknown.
    /// - [`ChatError::InternalError`] when the manager or todo actor is gone.
    pub async fn get_session_todo(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<TodoItem>, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::GetSessionTodo {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("chat manager unavailable".into()))?;
        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("chat manager unavailable".into()))?
    }

    /// Read the ordered plan mutation history for a session.
    ///
    /// Mutations come back in insertion order (oldest first), including removal
    /// tombstones, so the desktop scrubber can replay the plan construction
    /// revision by revision. A known session with no plan actor or no recorded
    /// mutations yields an empty list.
    ///
    /// # Errors
    ///
    /// - [`ChatError::SessionNotFound`] when the session is unknown.
    /// - [`ChatError::InternalError`] when the manager or plan actor is gone.
    pub async fn read_plan_mutations(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<PlanMutation>, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ReadPlanMutations {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("chat manager unavailable".into()))?;
        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("chat manager unavailable".into()))?
    }

    /// Read the current plan snapshot for a session.
    ///
    /// Returns `None` for a known session that has not produced a plan yet, so
    /// the desktop can hydrate the plan tab on mount without waiting for a live
    /// event.
    ///
    /// # Errors
    ///
    /// - [`ChatError::SessionNotFound`] when the session is unknown.
    /// - [`ChatError::InternalError`] when the manager or plan actor is gone.
    pub async fn get_plan(&self, session_id: SessionId) -> Result<ChatPlanSnapshot, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::GetPlan {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("chat manager unavailable".into()))?;
        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("chat manager unavailable".into()))?
    }
}
