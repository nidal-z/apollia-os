use super::*;

/// Logical identifier of the system agent "Apollia Chat".
///
/// Used as the key for `scope = 'agent'` permission rules, for the standing
/// consents of Apollia, and as the name of the Apollia agent in `agents.db`.
/// Defined once, in `apollia-permissions`.
pub const APOLLIA_CHAT_AGENT_ID: &str = apollia_permissions::APOLLIA_AGENT_ID;

/// Subset of the runtime's native-tool config the chat dispatcher needs to
/// instantiate config-dependent natives (web_search, web_read, http_fetch,
/// memory_search). Cloned per session, then completed
/// with per-session `sandbox_root` + `agent_id` inside
/// [`resolve_workspace_for_session`].
///
/// Supervisor populates this once at boot from `apollia.toml` + `data_dir`.
/// Part of converging free-chat native tool execution with the Agent mode
/// and Triggers pipeline.
#[derive(Clone)]
pub struct ChatToolsConfig {
    /// Root data directory (`~/.apollia` typical), used to derive
    /// `governance_db_path`, `memory_base_dir`, and `venv_base_dir`.
    pub data_dir: std::path::PathBuf,
    /// Resolved Brave Search API key (read from `governance.db` credentials
    /// or the `BRAVE_SEARCH_API_KEY` env var). `None` falls back to env var.
    pub brave_api_key: Option<String>,
    /// Native-tools subsection of `apollia.toml` (web cfg, http allowlist,
    /// disabled list). Defaults are sensible when no file is loaded.
    pub tools_config: apollia_core::ToolsConfig,
    /// Temperature applied to a chat turn that advertises tools (`[chat]
    /// tool_turn_temperature`). `None` resolves to the agent default.
    pub tool_turn_temperature: Option<f32>,
    /// The registry of sources, through which `source_search` reads the
    /// indexes of the sources an agent checked. `None` leaves every source
    /// but the memory unsearchable.
    pub source_registry: Option<apollia_sources::SourceRegistryHandle>,
    /// The governance actor the disabled tools are read from. `None` reads
    /// `governance.db` under `data_dir` directly.
    pub governance: Option<crate::governance::GovernanceHandle>,
}

#[derive(Debug, Default)]
pub(in crate::chat::manager) struct ChatLibreOverrides {
    pub(in crate::chat::manager) system_prompt: Option<String>,
    pub(in crate::chat::manager) llm_backend: Option<String>,
}

/// Per-session defaults derived from the Libre-mode governance overrides.
#[derive(Debug, Default)]
pub(in crate::chat::manager) struct LibreSessionDefaults {
    pub(in crate::chat::manager) llm_backend: Option<String>,
}

/// Inputs for [`resolve_exchange_summary`].
pub(in crate::chat::manager) struct ExchangeSummaryParams<'a> {
    pub(in crate::chat::manager) history: &'a [ChatMessage],
    pub(in crate::chat::manager) context_window_size: usize,
    pub(in crate::chat::manager) stored_summary: Option<String>,
    pub(in crate::chat::manager) llm_for_summarize: &'a Option<Arc<LlmRouter>>,
    pub(in crate::chat::manager) tx: &'a mpsc::Sender<ChatCommand>,
    pub(in crate::chat::manager) sid: &'a str,
}

/// Owned, `Send` inputs for a spawned Libre/Companion exchange.
///
/// All values are cloned out of [`ChatSessionManager`] before the task is
/// spawned so the spawned future never captures the non-`Send` `&self`
/// (the manager owns a `RefCell` via rusqlite).
pub(in crate::chat::manager) struct LibreExchangeParams {
    /// The agent the exchange runs: Apollia, or the declarative agent of an
    /// agent session. Its model, connector accounts and journal entries are
    /// resolved under this identifier.
    pub(in crate::chat::manager) agent_id: String,
    pub(in crate::chat::manager) llm_router: Arc<LlmRouter>,
    pub(in crate::chat::manager) tool_registry: ToolRegistryHandle,
    pub(in crate::chat::manager) event_bus: EventBusSender,
    pub(in crate::chat::manager) a2a_for_agent: Option<Arc<A2AInvoker>>,
    /// The principle 6 injections gathered for the session (user memory,
    /// past sessions block).
    pub(in crate::chat::manager) apollia_context: ApolliaContext,
    pub(in crate::chat::manager) pending_approvals: PendingChatApprovals,
    /// Autonomy tier of the exchange; the executor derives the budget, the
    /// verification and the critic from it.
    pub(in crate::chat::manager) autonomy_level: AutonomyLevel,
    /// Runtime ceiling the tier's budget is capped by.
    pub(in crate::chat::manager) runtime_budget: StepBudgetConfig,
    pub(in crate::chat::manager) history: Vec<ChatMessage>,
    /// The access of the agent: a list, or the reading family read when the
    /// exchange starts.
    pub(in crate::chat::manager) available_tools: apollia_tools::AgentAccess,
    pub(in crate::chat::manager) authorized_tools: std::collections::HashSet<String>,
    /// The mission of the agent, read when the message is sent.
    pub(in crate::chat::manager) system_prompt: String,
    /// What the conversation changes from its agent, for itself alone.
    pub(in crate::chat::manager) session_settings: SessionSettings,
    pub(in crate::chat::manager) context_window_size: usize,
    pub(in crate::chat::manager) stored_summary: Option<String>,
    pub(in crate::chat::manager) llm_for_summarize: Option<Arc<LlmRouter>>,
    pub(in crate::chat::manager) pending_user_inputs_for_session:
        apollia_tools::tools::ask_user::PendingUserInputs,
    pub(in crate::chat::manager) mcp_handle_for_session:
        Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// Executors of the package tools, built for the agent of the exchange.
    pub(in crate::chat::manager) package_tools: Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
    /// Runs the agents and workflows the Apollia agent was granted.
    pub(in crate::chat::manager) composer: Option<Arc<dyn crate::composition::Composer>>,
    /// The governance every tool call of the exchange is decided by.
    pub(in crate::chat::manager) governance: Option<crate::governance::GovernanceHandle>,
    pub(in crate::chat::manager) chat_tools_config_for_session: Option<Arc<ChatToolsConfig>>,
    pub(in crate::chat::manager) mcp_loading: LoadingMode,
    pub(in crate::chat::manager) tool_search_limit: usize,
    pub(in crate::chat::manager) session_id_str: String,
    pub(in crate::chat::manager) sid: String,
    pub(in crate::chat::manager) mid: String,
    pub(in crate::chat::manager) run_id: RunId,
    pub(in crate::chat::manager) user_msg: String,
    pub(in crate::chat::manager) tx: mpsc::Sender<ChatCommand>,
    pub(in crate::chat::manager) todo: Option<TodoHandle>,
    pub(in crate::chat::manager) plan: Option<PlanHandle>,
    pub(in crate::chat::manager) session_plan_mode: bool,
    /// Plan phase the session is in when the turn starts. A turn entered while
    /// the session is in [`PlanPhase::AwaitingApproval`] is a revision turn (the
    /// soft gate is open), so it does not reopen discovery.
    pub(in crate::chat::manager) session_plan_phase: PlanPhase,
    pub(in crate::chat::manager) hook_executor: Option<Arc<HookExecutor>>,
    /// Cooperative pause token for this turn, cloned from the manager's per-session
    /// registry. Cancelling it stops the ReAct loop at its next checkpoint.
    pub(in crate::chat::manager) cancel: CancellationToken,
    /// Operator instruction to consume on a resume turn, when this exchange is a
    /// resume carrying a queued injection. `None` for ordinary turns.
    pub(in crate::chat::manager) pending_injection: Option<InjectedInstruction>,
    /// The answer the exchange continues, when the user asked for the last
    /// answer to be continued. `None` for every other turn.
    pub(in crate::chat::manager) answer_prefill: Option<String>,
    /// Backend the session prefers (the free chat's stored choice), tried
    /// before the default at each call of the turn.
    pub(in crate::chat::manager) preferred_backend: Option<String>,
    /// The turn the exchange answers: the identifier its answer is persisted
    /// under.
    pub(in crate::chat::manager) turn_id: Option<String>,
    /// Whether the conversation is the one that creates an agent: Apollia is
    /// then offered `propose_agent` and nothing else, under the instructions
    /// of that conversation.
    pub(in crate::chat::manager) agent_draft: bool,
    /// Images of the message answered and of the earlier user messages in
    /// the window, read back from disk.
    pub(in crate::chat::manager) turn_images: crate::chat::images::TurnImages,
}

/// Resolved tool invoker plus the agent workspace path for a session.
pub(in crate::chat::manager) struct ResolvedSessionInvoker {
    pub(in crate::chat::manager) invoker: Arc<dyn ToolInvoker>,
    pub(in crate::chat::manager) workspace: Option<std::path::PathBuf>,
}

/// Error surface for cooperative pause and resume operations on a chat session.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PauseError {
    /// No session matches the requested identifier.
    #[error("session {session_id} is unknown")]
    UnknownSession {
        /// The unknown session identifier.
        session_id: String,
    },
}

/// Error surface for natural-language instruction injection.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InjectError {
    /// No session matches the requested identifier.
    #[error("session {session_id} is unknown")]
    UnknownSession {
        /// The unknown session identifier.
        session_id: String,
    },
    /// The session is not paused; injection is only valid while paused.
    #[error("session {session_id} is not paused; inject is only valid while paused")]
    NotPaused {
        /// The session identifier that is not paused.
        session_id: String,
    },
    /// The instruction text was empty or whitespace-only.
    #[error("instruction text is empty")]
    EmptyInstruction,
}

/// Snapshot of a session's plan plus its current plan-mode phase.
///
/// The phase is the authoritative gate state: a persisted plan keeps its
/// `awaiting_approval` status forever, but once approved the session moves to
/// `executing` then `done`. The desktop hydrates the plan tab from `plan` and
/// decides whether to show the approval card from `phase`, never from the
/// plan's own status.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ChatPlanSnapshot {
    /// Current plan, or `None` when the session has produced none.
    #[schema(value_type = Option<Object>)]
    pub plan: Option<Plan>,
    /// Session plan-mode phase: `discovery`, `drafting`, `awaiting_approval`,
    /// `executing`, or `done`.
    pub phase: String,
}

/// Parameters for [`ChatSessionManager::handle_create_session`].
pub struct CreateSessionParams {
    pub mode: ChatMode,
    pub agent_name: Option<String>,
    pub system_prompt: Option<String>,
    pub tools: Vec<String>,
    /// The model chosen for the session, by the name the router routes by.
    /// `None` keeps the model of the agent, else the default.
    pub model: Option<String>,
}

/// Parameters for [`ChatSessionManager::handle_register_user_input_reply`].
pub(in crate::chat::manager) struct RegisterUserInputReplyParams {
    pub(in crate::chat::manager) request_id: String,
    pub(in crate::chat::manager) session_id: String,
    pub(in crate::chat::manager) questions_json: String,
    pub(in crate::chat::manager) context: Option<String>,
    pub(in crate::chat::manager) reply_tx:
        tokio::sync::oneshot::Sender<apollia_tools::tools::ask_user::AskUserOutput>,
}

/// Snapshot of a pending `ask_user` request, safe to send across the IPC boundary.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct PendingUserInputView {
    /// Unique request identifier.
    pub request_id: String,
    /// Chat session that owns this request.
    pub session_id: String,
    /// Questions serialised as JSON (Vec<UserQuestion>).
    pub questions_json: String,
    /// Optional context string from the agent.
    pub context: Option<String>,
    /// ISO-8601 timestamp when the request was registered.
    pub created_at: String,
}

/// Internal metadata kept alongside the oneshot sender for each pending `ask_user`.
pub(in crate::chat::manager) struct PendingUserInputMeta {
    pub(in crate::chat::manager) session_id: String,
    pub(in crate::chat::manager) questions_json: String,
    pub(in crate::chat::manager) context: Option<String>,
    pub(in crate::chat::manager) created_at: String,
}

/// Parameters for [`build_full_chat_dispatcher`].
pub(in crate::chat::manager) struct FullDispatcherParams<'a> {
    pub(in crate::chat::manager) cfg: &'a Arc<ChatToolsConfig>,
    pub(in crate::chat::manager) session_id: &'a str,
    /// The folders granted to the agent, the roots of its file tools.
    pub(in crate::chat::manager) folders: &'a apollia_tools::FolderRoots,
    pub(in crate::chat::manager) pending_user_inputs:
        &'a Option<apollia_tools::tools::ask_user::PendingUserInputs>,
    pub(in crate::chat::manager) extra_executors:
        Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
    /// The sources the agent of the run has checked.
    pub(in crate::chat::manager) sources: apollia_tools::SourceAccess,
    /// The memory namespace `memory_search` reads when the memory is checked.
    pub(in crate::chat::manager) memory_namespace: String,
}

/// Parameters for [`resolve_workspace_for_session`].
pub(in crate::chat::manager) struct WorkspaceResolutionParams {
    /// The folders granted to the agent of the run, the roots of its file
    /// tools; the first anchors a relative path.
    pub(in crate::chat::manager) folders: apollia_tools::FolderRoots,
    pub(in crate::chat::manager) pending_user_inputs:
        Option<apollia_tools::tools::ask_user::PendingUserInputs>,
    pub(in crate::chat::manager) mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    pub(in crate::chat::manager) chat_tools_config: Option<Arc<ChatToolsConfig>>,
    pub(in crate::chat::manager) session_id: String,
    /// MCP tool loading strategy for this exchange.
    pub(in crate::chat::manager) mcp_loading: LoadingMode,
    /// Aggregated deferred tool index, empty in eager mode.
    pub(in crate::chat::manager) mcp_index: Vec<ToolIndexSnapshot>,
    /// Maximum `limit` accepted by the synthetic `tool_search` tool.
    pub(in crate::chat::manager) tool_search_limit: usize,
    /// Executors of the package tools, built for the agent of the run.
    pub(in crate::chat::manager) package_tools: Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
    /// The sources the agent of the run has checked.
    pub(in crate::chat::manager) sources: apollia_tools::SourceAccess,
    /// The memory namespace `memory_search` reads when the memory is checked.
    pub(in crate::chat::manager) memory_namespace: String,
}
