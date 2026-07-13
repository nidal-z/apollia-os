use super::*;

/// Logical identifier of the system agent "Apollia Chat".
///
/// Used as the key for `scope = 'agent'` permission rules and for the
/// free-chat config persisted in `governance.db`.
pub const APOLLIA_CHAT_AGENT_ID: &str = "apollia:chat";

/// Subset of the runtime's native-tool config the chat dispatcher needs to
/// instantiate config-dependent natives (web_search, web_read, http_fetch,
/// memory_search, permission_rule_*). Cloned per session, then completed
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
    /// Default working directory for free-chat sessions (`[chat]
    /// default_workspace`). `None` falls back to `~/.apollia`.
    pub default_workspace: Option<std::path::PathBuf>,
    /// Temperature applied to a chat turn that advertises tools (`[chat]
    /// tool_turn_temperature`). `None` resolves to the agent default.
    pub tool_turn_temperature: Option<f32>,
}

/// Snapshot of a tool-level `scope=session` authorization for an active session.
///
/// Returned by `ChatHandle::list_session_authorizations` so the desktop can
/// display in-memory authorizations in Settings > Permissions (`scope =
/// 'session'` rules are never persisted).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionAuthorizationView {
    /// Unique session identifier.
    pub session_id: String,
    /// Session title (None for untitled sessions).
    pub session_title: Option<String>,
    /// Session mode (`"libre"` | `"agent"` | `"companion"`).
    pub mode: String,
    /// Name of the tool auto-authorized during this session.
    pub tool_name: String,
}

#[derive(Debug, Default)]
pub(in crate::chat::manager) struct ChatLibreOverrides {
    pub(in crate::chat::manager) system_prompt: Option<String>,
    pub(in crate::chat::manager) llm_backend: Option<String>,
    pub(in crate::chat::manager) pre_authorized_tools: std::collections::HashSet<String>,
}

/// Per-session defaults derived from the Libre-mode governance overrides.
#[derive(Debug, Default)]
pub(in crate::chat::manager) struct LibreSessionDefaults {
    pub(in crate::chat::manager) llm_backend: Option<String>,
    pub(in crate::chat::manager) pre_authorized: std::collections::HashSet<String>,
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

/// Scope-resolution inputs for `persist_always_accept_scope`, captured before
/// the session borrow is released so governance persistence can use a disjoint
/// `&self`.
pub(in crate::chat::manager) struct AlwaysAcceptScopeCtx<'a> {
    pub(in crate::chat::manager) scope: &'a super::super::types::AlwaysAcceptScope,
    pub(in crate::chat::manager) session_mode: ChatMode,
    pub(in crate::chat::manager) session_agent_name: Option<String>,
    pub(in crate::chat::manager) session_project_id: Option<&'a str>,
    pub(in crate::chat::manager) session_id: &'a str,
    pub(in crate::chat::manager) tool_name: &'a str,
}

/// Owned, `Send` inputs for a spawned Libre/Companion exchange.
///
/// All values are cloned out of [`ChatSessionManager`] before the task is
/// spawned so the spawned future never captures the non-`Send` `&self`
/// (the manager owns a `RefCell` via rusqlite).
pub(in crate::chat::manager) struct LibreExchangeParams {
    pub(in crate::chat::manager) llm_router: Arc<LlmRouter>,
    pub(in crate::chat::manager) tool_registry: ToolRegistryHandle,
    pub(in crate::chat::manager) event_bus: EventBusSender,
    pub(in crate::chat::manager) a2a_for_agent: Option<Arc<A2AInvoker>>,
    pub(in crate::chat::manager) session_user_memory:
        Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    pub(in crate::chat::manager) pending_approvals: PendingChatApprovals,
    pub(in crate::chat::manager) budget: StepBudget,
    pub(in crate::chat::manager) autonomy_level: AutonomyLevel,
    pub(in crate::chat::manager) level_config: AutonomyLevelConfig,
    pub(in crate::chat::manager) verification: Option<VerificationLoop>,
    pub(in crate::chat::manager) critic: Option<CriticPass>,
    pub(in crate::chat::manager) history: Vec<ChatMessage>,
    pub(in crate::chat::manager) available_tools: Vec<String>,
    pub(in crate::chat::manager) authorized_tools: std::collections::HashSet<String>,
    pub(in crate::chat::manager) system_prompt: String,
    pub(in crate::chat::manager) inject_project_context: bool,
    pub(in crate::chat::manager) is_companion: bool,
    pub(in crate::chat::manager) context_window_size: usize,
    pub(in crate::chat::manager) stored_summary: Option<String>,
    pub(in crate::chat::manager) llm_for_summarize: Option<Arc<LlmRouter>>,
    pub(in crate::chat::manager) project_ctx: Option<Arc<dyn ProjectContextProvider>>,
    pub(in crate::chat::manager) session_project_id: Option<String>,
    pub(in crate::chat::manager) project_repo_for_session: Option<Arc<ProjectRepository>>,
    pub(in crate::chat::manager) pending_user_inputs_for_session:
        apollia_tools::tools::ask_user::PendingUserInputs,
    pub(in crate::chat::manager) mcp_handle_for_session:
        Option<apollia_mcp::manager::McpClientManagerHandle>,
    pub(in crate::chat::manager) chat_tools_config_for_session: Option<Arc<ChatToolsConfig>>,
    pub(in crate::chat::manager) mcp_loading: LoadingMode,
    pub(in crate::chat::manager) tool_search_limit: usize,
    pub(in crate::chat::manager) session_id_str: String,
    pub(in crate::chat::manager) hitl_params: HitlInvokerParams,
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
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatPlanSnapshot {
    /// Current plan, or `None` when the session has produced none.
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
    pub project_id: Option<String>,
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    pub(in crate::chat::manager) sandbox_root: &'a std::path::Path,
    pub(in crate::chat::manager) workspace_path: &'a Option<std::path::PathBuf>,
    pub(in crate::chat::manager) pending_user_inputs:
        &'a Option<apollia_tools::tools::ask_user::PendingUserInputs>,
    pub(in crate::chat::manager) hitl: Option<&'a HitlInvokerParams>,
    pub(in crate::chat::manager) extra_executors:
        Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
}

/// Parameters for [`resolve_workspace_for_session`].
pub(in crate::chat::manager) struct WorkspaceResolutionParams {
    pub(in crate::chat::manager) project_id: Option<String>,
    pub(in crate::chat::manager) project_repo: Option<Arc<ProjectRepository>>,
    pub(in crate::chat::manager) hitl: Option<HitlInvokerParams>,
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
}
