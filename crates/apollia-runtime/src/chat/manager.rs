//! ChatSessionManager, Tokio actor managing chat session lifecycle.
//!
//! Central entry point for the chat subsystem. Handles session
//! creation, message routing, tool approval resolution, and lifecycle events.
//! Persists in SQLite via [`ChatSessionRepository`] and emits
//! [`RuntimeEvent`] on the EventBus.
//!
//! The chat path does NOT go through the `TaskRouter`, it has its own
//! execution path.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use apollia_core::todo::TodoItem;
use apollia_core::{
    AutonomyConfig, AutonomyLevel, AutonomyLevelConfig, RunId, RuntimeEvent, StepBudgetConfig,
};
use apollia_llm::{LlmRouter, ToolInvoker};
use apollia_mcp::session::LoadingMode;
use apollia_mcp::tool_search::{ToolIndexSnapshot, ToolSearchExecutor};
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::budget::StepBudget;
use apollia_oria::verification::{CriticPass, VerificationLoop};
use apollia_permissions::prefix_rule_engine::RuleAction;
use apollia_permissions::PrefixRuleEngine;
use apollia_tools::chat_libre_config::ChatLibreConfigRepository;
use apollia_tools::governance_db::GOVERNANCE_DB_FILENAME;
use apollia_tools::{ProjectRepository, ToolRegistryHandle};

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

/// Merge the session's authorized tools with the live Chat-Libre overrides
/// (additive: never removes an in-session authorization). Overrides are only
/// applied for [`ChatMode::Libre`]; other modes return the base set as-is.
fn merge_live_authorized_tools(
    base: &std::collections::HashSet<String>,
    mode: &ChatMode,
) -> std::collections::HashSet<String> {
    let mut authorized_tools = base.clone();
    if *mode == ChatMode::Libre {
        let live = load_chat_libre_overrides();
        for tool in live.pre_authorized_tools {
            authorized_tools.insert(tool);
        }
    }
    authorized_tools
}

fn load_chat_libre_overrides() -> ChatLibreOverrides {
    let mut out = ChatLibreOverrides::default();
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return out,
    };
    let base_dir = std::path::PathBuf::from(home).join(".apollia");
    let db_path = base_dir.join(GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return out;
    }

    apply_chat_libre_config(&mut out, &db_path);
    apply_chat_prefix_allow_rules(&mut out, &db_path);

    out
}

/// Apply the free-chat config (`chat_libre_config`) to the overrides.
/// Silent fallback if the database is absent or unreadable.
fn apply_chat_libre_config(out: &mut ChatLibreOverrides, db_path: &std::path::Path) {
    let Ok(repo) = ChatLibreConfigRepository::open(db_path) else {
        return;
    };
    let Ok(cfg) = repo.load() else {
        return;
    };
    if !cfg.system_prompt.trim().is_empty() {
        out.system_prompt = Some(cfg.system_prompt);
    }
    // chat_libre_config.allowed_tools are the auto-authorized tools (HITL
    // skipped), matching the UX label "tools allowed without confirmation".
    // They go into pre_authorized_tools, not available_tools: the LLM still
    // sees the whole registry, but these tools no longer trigger a popup.
    for tool in cfg.allowed_tools {
        out.pre_authorized_tools.insert(tool);
    }
    if let Some(b) = cfg.llm_backend {
        if !b.trim().is_empty() {
            out.llm_backend = Some(b);
        }
    }
}

/// Add to the overrides the tools auto-authorized via `allow` rules scoped to
/// the `apollia:chat` agent. Silent fallback on error.
fn apply_chat_prefix_allow_rules(out: &mut ChatLibreOverrides, db_path: &std::path::Path) {
    let Ok(engine) = PrefixRuleEngine::new(db_path) else {
        return;
    };
    let Ok(rules) = engine.list_rules_for_agent(APOLLIA_CHAT_AGENT_ID) else {
        return;
    };
    for r in rules {
        if matches!(r.action, RuleAction::Allow) {
            out.pre_authorized_tools.insert(r.tool_name);
        }
    }
}

#[derive(Debug, Default)]
struct ChatLibreOverrides {
    system_prompt: Option<String>,
    llm_backend: Option<String>,
    pre_authorized_tools: std::collections::HashSet<String>,
}

/// Per-session defaults derived from the Libre-mode governance overrides.
#[derive(Debug, Default)]
struct LibreSessionDefaults {
    llm_backend: Option<String>,
    pre_authorized: std::collections::HashSet<String>,
}

/// Apply Libre-mode governance overrides to the session prompt and return the
/// derived session defaults.
///
/// When `is_libre` is `false`, leaves `prompt` untouched and returns the empty
/// defaults (legacy behavior).
///
/// - `system_prompt` : prepended to the caller's prompt when set.
/// - `llm_backend`   : recorded on the session for downstream routing.
/// - `pre_authorized`: agent-scoped allow rules seed authorized_tools so the
///   chat ReAct loop skips HITL for them.
fn apply_libre_overrides(is_libre: bool, prompt: &mut String) -> LibreSessionDefaults {
    if !is_libre {
        return LibreSessionDefaults::default();
    }
    let overrides = load_chat_libre_overrides();
    if let Some(sp) = overrides.system_prompt {
        if prompt.trim().is_empty() {
            *prompt = sp;
        } else {
            *prompt = format!("{sp}\n\n{prompt}");
        }
    }
    LibreSessionDefaults {
        llm_backend: overrides.llm_backend,
        pre_authorized: overrides.pre_authorized_tools,
    }
}

/// Accumulate the metrics of one completed exchange into `entry`.
fn accumulate_exchange_metrics(
    entry: &mut SessionMetrics,
    tokens_used: &apollia_llm::types::TokenUsage,
    session: &ChatSession,
    max_steps: u32,
) {
    let now_ts = now_rfc3339();
    if entry.started_at.is_none() {
        entry.started_at = Some(now_ts.clone());
    }
    entry.updated_at = Some(now_ts);
    entry.prompt_tokens = entry
        .prompt_tokens
        .saturating_add(tokens_used.prompt_tokens);
    entry.completion_tokens = entry
        .completion_tokens
        .saturating_add(tokens_used.completion_tokens);
    entry.cache_read_input_tokens = entry
        .cache_read_input_tokens
        .saturating_add(tokens_used.cache_read_input_tokens);
    entry.cache_write_input_tokens = entry
        .cache_write_input_tokens
        .saturating_add(tokens_used.cache_write_input_tokens);
    entry.cost_usd = match (entry.cost_usd, tokens_used.cost_usd) {
        (Some(a), Some(b)) => Some(a + b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    entry.budget_max_steps = max_steps;
    entry.steps_used = entry.steps_used.saturating_add(1);
    entry.context_window_size = super::builtin_agent::DEFAULT_CONTEXT_WINDOW_SIZE as u32;
    entry.messages_in_history = session.history.len() as u32;
    entry.exchanges_count = entry.exchanges_count.saturating_add(1);
    entry.record_tool_calls(
        &session
            .history
            .last()
            .and_then(|m| m.tool_calls.clone())
            .unwrap_or_default(),
    );
    if entry.llm_backend.is_none() {
        entry.llm_backend = session.llm_backend.clone();
    }
}

/// Enrich `system_prompt` with the project context block when injection is
/// enabled and both a project id and a provider are available.
async fn maybe_inject_project_context(
    system_prompt: String,
    should_inject: bool,
    session_project_id: &Option<String>,
    project_ctx: &Option<Arc<dyn ProjectContextProvider>>,
) -> String {
    if !should_inject {
        return system_prompt;
    }
    let (Some(pid), Some(provider)) = (session_project_id, project_ctx) else {
        return system_prompt;
    };
    match provider.build_context(pid).await {
        Some(ctx) => {
            let mut enriched = system_prompt;
            enriched.push_str("\n\n");
            enriched.push_str(&ctx);
            enriched
        }
        None => system_prompt,
    }
}

/// Inputs for [`resolve_exchange_summary`].
struct ExchangeSummaryParams<'a> {
    history: &'a [ChatMessage],
    context_window_size: usize,
    stored_summary: Option<String>,
    llm_for_summarize: &'a Option<Arc<LlmRouter>>,
    tx: &'a mpsc::Sender<ChatCommand>,
    sid: &'a str,
}

/// Scope-resolution inputs for `persist_always_accept_scope`, captured before
/// the session borrow is released so governance persistence can use a disjoint
/// `&self`.
struct AlwaysAcceptScopeCtx<'a> {
    scope: &'a super::types::AlwaysAcceptScope,
    session_mode: ChatMode,
    session_agent_name: Option<String>,
    session_project_id: Option<&'a str>,
    session_id: &'a str,
    tool_name: &'a str,
}

/// Resolve the conversation summary for the exchange.
///
/// Reuses `stored_summary` when present or the history fits the context
/// window. Otherwise summarizes the overflow via the LLM and persists the
/// result (best-effort) through `tx`.
async fn resolve_exchange_summary(params: ExchangeSummaryParams<'_>) -> Option<String> {
    let ExchangeSummaryParams {
        history,
        context_window_size,
        stored_summary,
        llm_for_summarize,
        tx,
        sid,
    } = params;
    if history.len() <= context_window_size || stored_summary.is_some() {
        return stored_summary;
    }
    let Some(llm) = llm_for_summarize else {
        return None;
    };
    let older = &history[..history.len() - context_window_size];
    match super::summarizer::summarize(older, llm).await {
        Ok(s) => {
            let _ = tx
                .send(ChatCommand::PersistSummary {
                    session_id: sid.to_string(),
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
}

/// Owned, `Send` inputs for a spawned Libre/Companion exchange.
///
/// All values are cloned out of [`ChatSessionManager`] before the task is
/// spawned so the spawned future never captures the non-`Send` `&self`
/// (the manager owns a `RefCell` via rusqlite).
struct LibreExchangeParams {
    llm_router: Arc<LlmRouter>,
    tool_registry: ToolRegistryHandle,
    event_bus: EventBusSender,
    a2a_for_agent: Option<Arc<A2AInvoker>>,
    session_user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    pending_approvals: PendingChatApprovals,
    budget: StepBudget,
    autonomy_level: AutonomyLevel,
    level_config: AutonomyLevelConfig,
    verification: Option<VerificationLoop>,
    critic: Option<CriticPass>,
    history: Vec<ChatMessage>,
    available_tools: Vec<String>,
    authorized_tools: std::collections::HashSet<String>,
    system_prompt: String,
    inject_project_context: bool,
    is_companion: bool,
    context_window_size: usize,
    stored_summary: Option<String>,
    llm_for_summarize: Option<Arc<LlmRouter>>,
    project_ctx: Option<Arc<dyn ProjectContextProvider>>,
    session_project_id: Option<String>,
    project_repo_for_session: Option<Arc<ProjectRepository>>,
    pending_user_inputs_for_session: apollia_tools::tools::ask_user::PendingUserInputs,
    mcp_handle_for_session: Option<apollia_mcp::manager::McpClientManagerHandle>,
    chat_tools_config_for_session: Option<Arc<ChatToolsConfig>>,
    mcp_loading: LoadingMode,
    tool_search_limit: usize,
    session_id_str: String,
    hitl_params: HitlInvokerParams,
    sid: String,
    mid: String,
    run_id: RunId,
    user_msg: String,
    tx: mpsc::Sender<ChatCommand>,
    todo: Option<TodoHandle>,
}

/// Run a spawned Libre/Companion exchange end-to-end and report the result back
/// to the actor via `tx`.
///
/// Extracted as a free `async fn` (rather than an inline `async move` block) so
/// it captures only owned `Send` data and keeps `dispatch_libre_exchange`
/// linear.
async fn run_libre_exchange(params: LibreExchangeParams) {
    let LibreExchangeParams {
        llm_router,
        tool_registry,
        event_bus,
        a2a_for_agent,
        session_user_memory,
        pending_approvals,
        budget,
        autonomy_level,
        level_config,
        verification,
        critic,
        history,
        available_tools,
        authorized_tools,
        system_prompt,
        inject_project_context,
        is_companion,
        context_window_size,
        stored_summary,
        llm_for_summarize,
        project_ctx,
        session_project_id,
        project_repo_for_session,
        pending_user_inputs_for_session,
        mcp_handle_for_session,
        chat_tools_config_for_session,
        mcp_loading,
        tool_search_limit,
        session_id_str,
        hitl_params,
        sid,
        mid,
        run_id,
        user_msg,
        tx,
        todo,
    } = params;

    // In deferred mode, snapshot the aggregated tool index once. The synthetic
    // `tool_search` tool is built from it twice: as a dispatcher executor (so a
    // discovered tool can be invoked) and as an LLM spec (so the agent sees it
    // instead of every MCP schema). Eager mode keeps an empty index.
    let mcp_index: Vec<ToolIndexSnapshot> = match (mcp_loading, &mcp_handle_for_session) {
        (LoadingMode::Deferred, Some(handle)) => handle.get_tool_index().await,
        (LoadingMode::Deferred, None) => {
            warn!(
                mode = "deferred",
                "MCP deferred mode active but no manager handle is configured; \
                 tool_search will be exposed over an empty index"
            );
            Vec::new()
        }
        (LoadingMode::Eager, _) => Vec::new(),
    };

    // Resolve per-session sandbox root from project workspace_path.
    // On error (project not found) surface as ExchangeError, no panic.
    let session_invoker = match build_session_invoker(
        WorkspaceResolutionParams {
            project_id: session_project_id.clone(),
            project_repo: project_repo_for_session,
            hitl: Some(hitl_params),
            pending_user_inputs: Some(pending_user_inputs_for_session),
            mcp_handle: mcp_handle_for_session,
            chat_tools_config: chat_tools_config_for_session,
            session_id: session_id_str,
            mcp_loading,
            mcp_index: mcp_index.clone(),
            tool_search_limit,
        },
        &a2a_for_agent,
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

    // `Some` only in deferred mode, so `build_tool_specs` injects `tool_search`.
    // Eager keeps `None` and the legacy spec path.
    let agent_mcp_index = match mcp_loading {
        LoadingMode::Deferred => Some(mcp_index),
        LoadingMode::Eager => None,
    };
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router,
        tool_registry,
        tool_invoker: session_invoker.invoker,
        event_bus,
        user_memory: session_user_memory,
        a2a_invoker: a2a_for_agent,
        todo,
    })
    .with_workspace_path(session_invoker.workspace)
    .with_mcp_index(agent_mcp_index, tool_search_limit);

    // Inject project context on first message OR right after the
    // session was linked to a project (consumed flag).
    tracing::info!(
        session_id = %sid,
        inject_project_context,
        has_project = session_project_id.is_some(),
        has_provider = project_ctx.is_some(),
        "Chat send: project-context gate"
    );
    let system_prompt = maybe_inject_project_context(
        system_prompt,
        inject_project_context && !is_companion,
        &session_project_id,
        &project_ctx,
    )
    .await;
    let summary = resolve_exchange_summary(ExchangeSummaryParams {
        history: &history,
        context_window_size,
        stored_summary,
        llm_for_summarize: &llm_for_summarize,
        tx: &tx,
        sid: &sid,
    })
    .await;

    let result = agent
        .execute(
            &sid,
            &mid,
            &run_id,
            &user_msg,
            &history,
            &system_prompt,
            &available_tools,
            &authorized_tools,
            &pending_approvals,
            &budget,
            summary.as_deref(),
            context_window_size,
            Some(&autonomy_level),
            verification.as_ref(),
            critic.as_ref(),
            Some(&level_config),
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
}

/// Resolved tool invoker plus the agent workspace path for a session.
struct ResolvedSessionInvoker {
    invoker: Arc<dyn ToolInvoker>,
    workspace: Option<std::path::PathBuf>,
}

/// Resolve the native invoker for a session and wrap it in a composite invoker
/// when an A2A invoker is configured.
async fn build_session_invoker(
    workspace_params: WorkspaceResolutionParams,
    a2a_for_agent: &Option<Arc<A2AInvoker>>,
) -> Result<ResolvedSessionInvoker, ChatError> {
    let native_invoker = resolve_workspace_for_session(workspace_params).await?;
    let workspace = native_invoker.workspace_path().map(|p| p.to_path_buf());
    let invoker: Arc<dyn ToolInvoker> = if let Some(a2a) = a2a_for_agent {
        Arc::new(CompositeToolInvoker::new(native_invoker, a2a.clone()))
    } else {
        Arc::new(native_invoker)
    };
    Ok(ResolvedSessionInvoker { invoker, workspace })
}

/// Trace-log the enriched approval metadata (reject reason / always-accept
/// scope) without touching the `log_tool_approval` SQL schema.
fn log_resolution_metadata(
    decision: &ToolDecision,
    session_id: &str,
    message_id: &str,
    tool_name: &str,
) {
    match decision {
        ToolDecision::Refuse { reason: Some(r) } => {
            tracing::info!(
                session_id,
                message_id,
                tool_name,
                reject_reason = %r,
                "chat tool rejected with reason"
            );
        }
        ToolDecision::AlwaysAccept { scope } => {
            tracing::info!(
                session_id,
                message_id,
                tool_name,
                always_accept_scope = ?scope,
                "chat tool always-accept rule installed"
            );
        }
        _ => {}
    }
}

/// Persist a scoped `allow` rule in `governance.db` for `tool_name`.
///
/// Driven by the chat "Always allow" button according to the scope the
/// operator picked. Best-effort: logs and continues on failure (the in-memory
/// authorization in `session.authorized_tools` stays in place).
fn persist_chat_allow_rule(
    scope: apollia_permissions::PermissionScope,
    project_path: Option<std::path::PathBuf>,
    agent_id: Option<String>,
    tool_name: &str,
) {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(e) => {
            warn!(error = %e, "HOME not set; skipping governance.db rule persistence");
            return;
        }
    };
    let base_dir = std::path::PathBuf::from(home).join(".apollia");

    if let Err(e) = apollia_tools::governance_db::GovernanceDb::open(&base_dir) {
        warn!(error = %e, "failed to open governance.db for chat rule persistence");
        return;
    }
    let db_path = base_dir.join(GOVERNANCE_DB_FILENAME);

    let mut engine = match PrefixRuleEngine::new(&db_path) {
        Ok(e) => e,
        Err(e) => {
            warn!(error = %e, "failed to open prefix rule engine for chat rule persistence");
            return;
        }
    };

    let rule = apollia_permissions::PrefixRule {
        tool_name: tool_name.to_string(),
        arg_prefix: None,
        action: RuleAction::Allow,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        scope,
        project_path,
        agent_id,
        ..apollia_permissions::PrefixRule::default()
    };

    match engine.add_rule(&rule) {
        Ok(rule_id) => info!(
            rule_id,
            scope = %scope.as_str(),
            tool = %tool_name,
            "persisted scoped allow rule from chat AlwaysAccept"
        ),
        Err(e) => warn!(error = %e, "failed to persist scoped allow rule"),
    }
}

use super::a2a_tools::CompositeToolInvoker;
use super::agent_chat::{AgentChatExecutor, AgentChatRequest, ChatAgentRunner};
use super::builtin_agent::{
    BuiltInChatAgent, BuiltInChatAgentDeps, ChatAgentResponse, HitlInvokerParams,
    NativeChatToolInvoker, DEFAULT_CONTEXT_WINDOW_SIZE,
};
use super::extractor::UserMemoryExtractor;
use super::repository::{AppendMessageParams, ChatSessionRepository, ToolApprovalLogEntry};
use super::todo_actor::spawn_todo_actor;
use super::todo_handle::TodoHandle;
use super::types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, ExchangeState, MessageId,
    PendingChatApprovals, PendingFilesystemApprovals, ProjectContextProvider, RecentSessionSummary,
    SessionDetail, SessionId, SessionInfo, SessionMetrics, SessionStatus, ToolCallRecord,
    ToolDecision,
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
    /// Read the todo list for a session.
    GetSessionTodo {
        /// Target session.
        session_id: SessionId,
        /// Response channel: `SessionNotFound` for an unknown session, an empty
        /// list for a known session with no items.
        reply: oneshot::Sender<Result<Vec<TodoItem>, ChatError>>,
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
        /// New LLM backend (if Some, inner None means "use default").
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
    /// List in-memory tool authorizations across all active sessions
    /// (used by the desktop Settings > Permissions page to surface
    /// session-only auths that don't live in `governance.db`).
    ListSessionAuthorizations {
        /// Response channel.
        reply: oneshot::Sender<Vec<SessionAuthorizationView>>,
    },
    /// Remove an in-memory tool authorization from a specific session.
    RevokeSessionAuthorization {
        /// Target session.
        session_id: SessionId,
        /// Tool name to remove from `authorized_tools`.
        tool_name: String,
        /// Response channel, `true` if the entry existed and was removed.
        reply: oneshot::Sender<Result<bool, ChatError>>,
    },
    /// List all A2A skills available from active worker agents.
    ListA2ASkills {
        /// Response channel.
        reply: oneshot::Sender<Vec<crate::a2a::SkillListing>>,
    },
    /// Snapshot A2A skill telemetry.
    ListA2ASkillTelemetry {
        /// Response channel.
        reply: oneshot::Sender<Vec<crate::a2a::A2ASkillTelemetry>>,
    },
    /// Retrieve A2A step provenance entries, optionally filtered by skill id.
    ListA2AStepProvenance {
        /// Optional skill id filter.
        skill_id: Option<String>,
        /// Response channel.
        reply: oneshot::Sender<Vec<crate::a2a::A2AStepProvenance>>,
    },
    /// Check compatibility of a skill against a required semver version.
    CheckA2ACompatibility {
        /// Skill id to check.
        skill_id: String,
        /// Required semver version (e.g. `"1.5.0"`).
        required_version: String,
        /// Response channel.
        reply: oneshot::Sender<Option<crate::a2a::A2ACompatibilityWarning>>,
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
        /// Chat session that triggered the ask_user call.
        session_id: String,
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
    /// Reject a pending `ask_user` request with a mandatory reason.
    RejectUserInput {
        /// Unique request identifier.
        request_id: String,
        /// Operator-provided reason (non-empty, enforced by the frontend).
        reason: String,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// List all currently pending `ask_user` requests (for inbox / reconnection).
    ListPendingUserInputs {
        /// Response channel.
        reply: oneshot::Sender<Vec<PendingUserInputView>>,
    },
    /// Fetch aggregated metrics for a session (tokens, cost, tool stats, context window).
    GetSessionMetrics {
        /// Target session.
        session_id: SessionId,
        /// Response channel (returns `None` when session is unknown).
        reply: oneshot::Sender<Option<SessionMetrics>>,
    },
    /// Shut down the actor.
    Shutdown,
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
struct RegisterUserInputReplyParams {
    request_id: String,
    session_id: String,
    questions_json: String,
    context: Option<String>,
    reply_tx: tokio::sync::oneshot::Sender<apollia_tools::tools::ask_user::AskUserOutput>,
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
struct PendingUserInputMeta {
    session_id: String,
    questions_json: String,
    context: Option<String>,
    created_at: String,
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
    /// Pending filesystem HITL approval channels.
    pending_fs_approvals: PendingFilesystemApprovals,
    /// Optional user memory repository for system prompt enrichment.
    user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Stateful extractor for passive memory enrichment from conversations.
    enrichment_extractor: Option<Arc<tokio::sync::Mutex<UserMemoryExtractor>>>,
    /// Sender clone for spawned tasks to send commands back to the actor.
    tx: mpsc::Sender<ChatCommand>,
    /// Optional A2A invoker, when present, worker agent skills are exposed as virtual tools.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Optional project context provider for injecting project context into system prompts.
    project_context: Option<Arc<dyn ProjectContextProvider>>,
    /// Optional project repository for resolving workspace_path per session.
    project_repo: Option<Arc<ProjectRepository>>,
    /// Pending user input registry for the `ask_user` tool.
    pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs,
    /// MCP manager handle, populated when the supervisor's MCP bootstrap
    /// succeeds. Propagated to `NativeChatToolInvoker` per session so
    /// chat-libre invocations of `mcp:<server>/<tool>` can be routed
    /// through the manager.
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// Operator config bundled at supervisor boot (data dir, brave key,
    /// tools_config) for the chat dispatcher. `None` in tests / minimal
    /// runtimes, falls back to the minimal dispatcher.
    chat_tools_config: Option<Arc<ChatToolsConfig>>,
    /// Map of pending `ask_user` entries, keyed by request_id.
    /// Populated by the background drain task, resolved by `ResolveUserInput`.
    pending_user_replies: HashMap<
        String,
        (
            PendingUserInputMeta,
            tokio::sync::oneshot::Sender<apollia_tools::tools::ask_user::AskUserOutput>,
        ),
    >,
    /// Per-session aggregated metrics.
    ///
    /// In-memory only, rebuilt from history on session resume. Not persisted.
    metrics: HashMap<SessionId, SessionMetrics>,
    /// MCP tool loading strategy applied to every chat exchange.
    ///
    /// In [`LoadingMode::Deferred`] the LLM receives the synthetic `tool_search`
    /// tool instead of the individual MCP schemas. In [`LoadingMode::Eager`] the
    /// existing behavior is preserved (MCP schemas resolved from the registry).
    mcp_loading: LoadingMode,
    /// Maximum `limit` accepted by the synthetic `tool_search` tool, sourced from
    /// the `mcp.tool_search_limit` setting.
    tool_search_limit: usize,
    /// Clonable handle to the per-runtime todo actor, cloned into each exchange
    /// so the agent's `todo_write` tool persists session task state.
    todo_handle: Option<TodoHandle>,
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
                        .handle_create_session(CreateSessionParams {
                            mode,
                            agent_name,
                            system_prompt,
                            tools,
                            project_id,
                        })
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
                ChatCommand::GetSessionTodo { session_id, reply } => {
                    // Resolve synchronously so no SQLite borrow is held across
                    // the await, then read through the owned handle clone.
                    let result = match self.resolve_todo_handle(&session_id) {
                        Ok(Some(todo)) => todo.get_items(&session_id).await.map_err(|e| {
                            ChatError::InternalError(format!("todo read failed: {e}"))
                        }),
                        Ok(None) => Ok(Vec::new()),
                        Err(e) => Err(e),
                    };
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
                    self.handle_orphan_project_sessions(&project_id);
                }
                ChatCommand::ListSessionAuthorizations { reply } => {
                    let _ = reply.send(self.handle_list_session_authorizations());
                }
                ChatCommand::RevokeSessionAuthorization {
                    session_id,
                    tool_name,
                    reply,
                } => {
                    let _ = reply
                        .send(self.handle_revoke_session_authorization(&session_id, &tool_name));
                }
                ChatCommand::ListA2ASkills { reply } => {
                    self.handle_list_a2a_skills(reply);
                }
                ChatCommand::ListA2ASkillTelemetry { reply } => {
                    self.handle_list_a2a_skill_telemetry(reply);
                }
                ChatCommand::ListA2AStepProvenance { skill_id, reply } => {
                    self.handle_list_a2a_step_provenance(skill_id, reply);
                }
                ChatCommand::CheckA2ACompatibility {
                    skill_id,
                    required_version,
                    reply,
                } => {
                    self.handle_check_a2a_compatibility(skill_id, required_version, reply);
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
                    let _ = reply.send(self.handle_resolve_fs_hitl(&request_id, decision));
                }
                ChatCommand::ListApprovalHistory { limit, days, reply } => {
                    let result = self.repository.list_tool_approval_history(limit, days);
                    let _ = reply.send(result);
                }
                ChatCommand::RegisterUserInputReply {
                    request_id,
                    session_id,
                    questions_json,
                    context,
                    reply_tx,
                } => {
                    self.handle_register_user_input_reply(RegisterUserInputReplyParams {
                        request_id,
                        session_id,
                        questions_json,
                        context,
                        reply_tx,
                    });
                }
                ChatCommand::ResolveUserInput {
                    request_id,
                    answers,
                    reply,
                } => {
                    let result = self.resolve_user_input_internal(&request_id, answers);
                    let _ = reply.send(result);
                }
                ChatCommand::RejectUserInput {
                    request_id,
                    reason,
                    reply,
                } => {
                    let result = self.reject_user_input_internal(&request_id, reason);
                    let _ = reply.send(result);
                }
                ChatCommand::ListPendingUserInputs { reply } => {
                    let views = self
                        .pending_user_replies
                        .iter()
                        .map(|(req_id, (meta, _))| PendingUserInputView {
                            request_id: req_id.clone(),
                            session_id: meta.session_id.clone(),
                            questions_json: meta.questions_json.clone(),
                            context: meta.context.clone(),
                            created_at: meta.created_at.clone(),
                        })
                        .collect();
                    let _ = reply.send(views);
                }
                ChatCommand::GetSessionMetrics { session_id, reply } => {
                    let result = self.metrics.get(&session_id).cloned();
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

    /// Clear the `project_id` of cached sessions after their project is deleted.
    fn handle_orphan_project_sessions(&mut self, project_id: &str) {
        match self.repository.orphan_project_sessions(project_id) {
            Ok(count) => {
                if count > 0 {
                    info!(project_id = %project_id, count, "Orphaned chat sessions after project deletion");
                    // Also update in-memory cache
                    for session in self.sessions.values_mut() {
                        if session.project_id.as_deref() == Some(project_id) {
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

    /// Collect the per-session tool authorizations for all open sessions.
    fn handle_list_session_authorizations(&self) -> Vec<SessionAuthorizationView> {
        let mut out: Vec<SessionAuthorizationView> = Vec::new();
        for s in self.sessions.values() {
            if matches!(s.status, SessionStatus::Closed) {
                continue;
            }
            for tool in &s.authorized_tools {
                out.push(SessionAuthorizationView {
                    session_id: s.id.clone(),
                    session_title: s.title.clone(),
                    mode: s.mode.as_sql().to_string(),
                    tool_name: tool.clone(),
                });
            }
        }
        out.sort_by(|a, b| {
            a.session_id
                .cmp(&b.session_id)
                .then_with(|| a.tool_name.cmp(&b.tool_name))
        });
        out
    }

    /// Revoke a session-scoped tool authorization. Returns whether it existed.
    fn handle_revoke_session_authorization(
        &mut self,
        session_id: &str,
        tool_name: &str,
    ) -> Result<bool, ChatError> {
        match self.sessions.get_mut(session_id) {
            Some(s) => Ok(s.authorized_tools.remove(tool_name)),
            None => Err(ChatError::SessionNotFound(session_id.to_string())),
        }
    }

    /// List A2A skills available from active worker agents (async, off-actor).
    fn handle_list_a2a_skills(&self, reply: oneshot::Sender<Vec<crate::a2a::SkillListing>>) {
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

    /// Snapshot A2A skill telemetry (async, off-actor).
    fn handle_list_a2a_skill_telemetry(
        &self,
        reply: oneshot::Sender<Vec<crate::a2a::A2ASkillTelemetry>>,
    ) {
        if let Some(ref a2a) = self.a2a_invoker {
            let a2a = a2a.clone();
            tokio::spawn(async move {
                let out = match a2a.telemetry() {
                    Some(t) => t.all_telemetry().await,
                    None => Vec::new(),
                };
                let _ = reply.send(out);
            });
        } else {
            let _ = reply.send(Vec::new());
        }
    }

    /// Retrieve A2A step provenance entries, optionally filtered by skill id.
    fn handle_list_a2a_step_provenance(
        &self,
        skill_id: Option<String>,
        reply: oneshot::Sender<Vec<crate::a2a::A2AStepProvenance>>,
    ) {
        if let Some(ref a2a) = self.a2a_invoker {
            let a2a = a2a.clone();
            tokio::spawn(async move {
                let out = match a2a.telemetry() {
                    Some(t) => t.steps_for(skill_id.as_deref()).await,
                    None => Vec::new(),
                };
                let _ = reply.send(out);
            });
        } else {
            let _ = reply.send(Vec::new());
        }
    }

    /// Check compatibility of a skill against a required semver version.
    fn handle_check_a2a_compatibility(
        &self,
        skill_id: String,
        required_version: String,
        reply: oneshot::Sender<Option<crate::a2a::A2ACompatibilityWarning>>,
    ) {
        if let Some(ref a2a) = self.a2a_invoker {
            let a2a = a2a.clone();
            tokio::spawn(async move {
                let out = a2a
                    .check_skill_compatibility(&skill_id, &required_version)
                    .await
                    .ok()
                    .flatten();
                let _ = reply.send(out);
            });
        } else {
            let _ = reply.send(None);
        }
    }

    /// Resolve a pending filesystem HITL request.
    fn handle_resolve_fs_hitl(
        &self,
        request_id: &str,
        decision: super::types::FsHitlDecision,
    ) -> Result<(), ChatError> {
        if self.pending_fs_approvals.resolve(request_id, decision) {
            Ok(())
        } else {
            Err(ChatError::InternalError(format!(
                "no pending fs HITL request for id '{request_id}'"
            )))
        }
    }

    /// Register a pending `ask_user` reply channel and emit the input-required event.
    fn handle_register_user_input_reply(&mut self, params: RegisterUserInputReplyParams) {
        let RegisterUserInputReplyParams {
            request_id,
            session_id,
            questions_json,
            context,
            reply_tx,
        } = params;
        let created_at = chrono::Utc::now().to_rfc3339();
        let meta = PendingUserInputMeta {
            session_id: session_id.clone(),
            questions_json: questions_json.clone(),
            context: context.clone(),
            created_at,
        };
        self.pending_user_replies
            .insert(request_id.clone(), (meta, reply_tx));
        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::ChatUserInputRequired {
                request_id,
                session_id,
                message_id: String::new(),
                questions_json,
                context,
            });
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

    /// Validate a session-creation request against the registry and LLM config.
    ///
    /// Takes the borrowed dependencies explicitly rather than `&self`: holding a
    /// `&ChatSessionManager` (which owns a `RefCell` via rusqlite) across the
    /// `find_by_name` await would make `run`'s future non-`Send`. Borrowing only
    /// the `Send + Sync` registry handle keeps the future spawnable.
    async fn validate_create_request(
        registry_handle: &AgentRegistryHandle,
        llm_configured: bool,
        mode: ChatMode,
        agent_name: Option<&str>,
    ) -> Result<(), ChatError> {
        // Agent mode requires an agent name
        if mode == ChatMode::Agent && agent_name.is_none() {
            return Err(ChatError::AgentNotFound(
                "agent_name is required for Agent mode".into(),
            ));
        }

        // Validate agent exists in the registry if agent mode
        if mode == ChatMode::Agent {
            if let Some(name) = agent_name {
                let found = registry_handle.find_by_name(name).await.map_err(|e| {
                    ChatError::InternalError(format!("registry lookup failed: {e}"))
                })?;
                if found.is_none() {
                    return Err(ChatError::AgentNotFound(name.to_string()));
                }
            }
        }

        // Libre mode requires an LLM
        if mode == ChatMode::Libre && !llm_configured {
            return Err(ChatError::NoLlmConfigured);
        }

        Ok(())
    }

    /// Create a new chat session.
    async fn handle_create_session(
        &mut self,
        params: CreateSessionParams,
    ) -> Result<SessionInfo, ChatError> {
        let CreateSessionParams {
            mode,
            agent_name,
            system_prompt,
            tools,
            project_id,
        } = params;
        Self::validate_create_request(
            &self.registry_handle,
            self.llm_router.is_some(),
            mode.clone(),
            agent_name.as_deref(),
        )
        .await?;

        // When no tools are explicitly specified, default to all tools in the registry.
        // Users can override by passing an explicit tools list when creating the session.
        let resolved_tools = if tools.is_empty() {
            let all =
                self.tool_registry.list().await.map_err(|e| {
                    ChatError::InternalError(format!("tool registry list failed: {e}"))
                })?;
            all.into_iter().map(|d| d.name).collect()
        } else {
            tools
        };

        let session_id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let mut prompt = system_prompt.unwrap_or_default();

        // ── Libre mode: pull persisted overrides from governance.db ──────────
        // Silent fallback when the DB is absent / empty / unreadable: legacy behavior.
        let LibreSessionDefaults {
            llm_backend: libre_llm_backend,
            pre_authorized,
        } = apply_libre_overrides(mode == ChatMode::Libre, &mut prompt);

        // Persist to SQLite
        self.repository.create_session(
            &session_id,
            &mode,
            agent_name.as_deref(),
            &prompt,
            &resolved_tools,
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
            authorized_tools: pre_authorized,
            available_tools: resolved_tools,
            created_at: now.clone(),
            active_exchange: None,
            llm_backend: libre_llm_backend,
            title: None,
            parent_session_id: None,
            fork_depth: 0,
            project_id,
            force_project_context_inject: false,
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

        // Set session to Processing. A fresh run_id correlates every event
        // emitted during this exchange (one user turn, one response cycle).
        session.status = SessionStatus::Processing;
        let run_id = RunId::new();
        session.active_exchange = Some(ExchangeState {
            message_id: message_id.clone(),
            started_at: now,
            run_id: run_id.clone(),
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

        // The agent path clones the full session, which already carries `run_id`
        // in `active_exchange`; only the Libre path needs it threaded explicitly.
        if session.mode == ChatMode::Libre || session.mode == ChatMode::Companion {
            self.dispatch_libre_exchange(session_id, &message_id, content, &run_id)?;
        } else {
            self.dispatch_agent_exchange(session_id, &message_id, content)?;
        }

        Ok(message_id)
    }

    /// Build the system prompt for a Libre/Companion exchange, optionally
    /// enriched with cross-session context on the first message.
    fn build_libre_system_prompt(&self, base_prompt: &str, content: &str, enrich: bool) -> String {
        let mut prompt = base_prompt.to_string();
        if enrich {
            if let Some(block) = self.build_cross_session_context(content) {
                prompt.push_str("\n\n");
                prompt.push_str(&block);
            }
        }
        prompt
    }

    /// Spawn the BuiltInChatAgent background task for a Libre/Companion exchange.
    fn dispatch_libre_exchange(
        &mut self,
        session_id: &str,
        message_id: &str,
        content: &str,
        run_id: &RunId,
    ) -> Result<(), ChatError> {
        let message_id = message_id.to_string();
        {
            let llm_router = self.llm_router.clone().ok_or(ChatError::NoLlmConfigured)?;
            // Read and consume the late-link injection flag, it triggers a
            // single re-injection of the project context on the next message
            // after a session was linked to a project mid-conversation.
            let force_inject_project_context = {
                let s_mut = self
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;
                let v = s_mut.force_project_context_inject;
                s_mut.force_project_context_inject = false;
                v
            };
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;

            // Companion sessions are intentionally isolated from user memory and
            // cross-session history (memory stays at the agent's initiative).
            // The companion helps with the platform, not the user's personal context.
            let session_user_memory = if session.mode == ChatMode::Companion {
                None
            } else {
                self.user_memory.clone()
            };

            let history = session.history.clone();
            let is_first_message = history.len() == 1;
            let is_companion = session.mode == ChatMode::Companion;
            let inject_project_context = is_first_message || force_inject_project_context;
            // On the first message, enrich the system prompt with cross-session context.
            // Companion sessions are excluded, they must not inherit personal history.
            let system_prompt = self.build_libre_system_prompt(
                &session.system_prompt,
                content,
                is_first_message && !is_companion,
            );
            let available_tools = session.available_tools.clone();
            // Live merge: governance.db is re-read on every message so Apollia
            // Chat config changes (auto-authorized tools, agent-scoped rules)
            // apply to already-open Libre sessions without closing/reopening the
            // conversation. The merge is purely additive: an authorization
            // granted during the session is never removed, only added to if the
            // config has been enriched since.
            let authorized_tools =
                merge_live_authorized_tools(&session.authorized_tools, &session.mode);
            let pending_approvals = self.pending_chat_approvals.clone();

            // Resolve the autonomy tier and build the effective budget from it.
            // `runtime_budget` is the ceiling: `from_capped` guarantees no tier
            // raises the budget above it (principle #7). The verification loop and
            // critic are constructed only when the tier requests verification;
            // Chat Libre declares no manifest check commands, so the loop is
            // critic-driven (empty command list).
            let autonomy_config = AutonomyConfig::default();
            let autonomy_level = autonomy_config.default_level;
            let level_config = autonomy_config.level_config(autonomy_level);
            let budget = StepBudget::from_capped(&level_config.budget, &self.runtime_budget);
            let (verification, critic) = if level_config.run_verification {
                (
                    Some(VerificationLoop::new(Vec::new(), Vec::new())),
                    Some(CriticPass::new(llm_router.clone())),
                )
            } else {
                (None, None)
            };
            let sid = session_id.to_string();
            let mid = message_id.clone();
            let user_msg = content.to_string();
            let tx = self.tx.clone();
            let context_window_size = DEFAULT_CONTEXT_WINDOW_SIZE;

            let stored_summary = self.repository.get_summary(session_id).unwrap_or(None);
            let llm_for_summarize = self.llm_router.clone();

            // Capture project context and repo for async injection in spawned task.
            // The invoker is created per-session inside the task.
            let project_ctx = self.project_context.clone();
            let session_project_id = session.project_id.clone();
            let project_repo_for_session = self.project_repo.clone();
            let a2a_for_agent = self.a2a_invoker.clone();
            let tool_registry = self.tool_registry.clone();
            let event_bus = self.event_bus.clone();

            let pending_user_inputs_for_session = self.pending_user_inputs.clone();
            let mcp_handle_for_session = self.mcp_handle.clone();
            let chat_tools_config_for_session = self.chat_tools_config.clone();
            let mcp_loading = self.mcp_loading;
            let tool_search_limit = self.tool_search_limit;
            let session_id_str = session_id.to_string();

            // Capture HITL filesystem params for the invoker.
            let hitl_params = HitlInvokerParams {
                session_id: session_id.to_string(),
                event_bus: self.event_bus.clone(),
                pending_fs: self.pending_fs_approvals.clone(),
                fs_allow_rules: std::sync::Arc::clone(&session.fs_allow_rules),
                risk_config: apollia_core::FilesystemRiskConfig::default(),
            };

            tokio::spawn(run_libre_exchange(LibreExchangeParams {
                llm_router,
                tool_registry,
                event_bus,
                a2a_for_agent,
                session_user_memory,
                pending_approvals,
                budget,
                autonomy_level,
                level_config,
                verification,
                critic,
                history,
                available_tools,
                authorized_tools,
                system_prompt,
                inject_project_context,
                is_companion,
                context_window_size,
                stored_summary,
                llm_for_summarize,
                project_ctx,
                session_project_id,
                project_repo_for_session,
                pending_user_inputs_for_session,
                mcp_handle_for_session,
                chat_tools_config_for_session,
                mcp_loading,
                tool_search_limit,
                session_id_str,
                hitl_params,
                sid,
                mid,
                run_id: run_id.clone(),
                user_msg,
                tx,
                todo: self.todo_handle.clone(),
            }));
        }

        Ok(())
    }

    /// Dispatch an Agent-mode exchange to the [`AgentChatExecutor`].
    fn dispatch_agent_exchange(
        &mut self,
        session_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;

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
                    "no ChatAgentRunner configured - Agent mode unavailable".into(),
                ));
            }
        };

        let executor = AgentChatExecutor::new(agent_runner, self.event_bus.clone());
        let session_clone = session.clone();
        let authorized = session.authorized_tools.clone();
        let pending = self.pending_chat_approvals.clone();
        let sid = session_id.to_string();
        let mid = message_id.to_string();
        let user_msg = content.to_string();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            let result = executor
                .execute(AgentChatRequest {
                    session: &session_clone,
                    user_message: &user_msg,
                    message_id: &mid,
                    authorized_tools: &authorized,
                    pending_approvals: &pending,
                })
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

        Ok(())
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
        let tokens_used = response.tokens_used.clone();
        let max_steps = self.runtime_budget.max_steps;

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
            tokens = tokens_used.prompt_tokens + tokens_used.completion_tokens,
            "Chat exchange complete"
        );

        // ── accumulate session metrics ─────────────────────
        let entry = self
            .metrics
            .entry(session_id.to_string())
            .or_insert_with(|| SessionMetrics::new(session_id.to_string()));
        accumulate_exchange_metrics(entry, &tokens_used, session, max_steps);

        // Emit a lightweight runtime event so the UI can trigger a refetch.
        // The event bridge forwards it as a generic `runtime-event` with
        // `event_type = "ChatResponseCompleted"`, and the frontend's metrics
        // store throttles subsequent `chat_session_metrics` calls to max 2/s.
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
        let run_id = self
            .sessions
            .get(session_id)
            .and_then(|s| s.active_exchange.as_ref())
            .map(|e| e.run_id.clone());
        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: format!("[Erreur : {error}]"),
            run_id,
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

        // AlwaysAccept: dispatch persistence by scope.
        // - ThisTool / ThisSession: in-memory only (session.authorized_tools).
        // - ThisAgent  : scope='agent' rule in governance.db (agent_id derived from the mode).
        // - ThisProject: scope='project' rule in governance.db (workspace_path of the current project).
        // - Global     : scope='global' rule in governance.db.
        if let ToolDecision::AlwaysAccept { scope } = &decision {
            // Always update the current session (immediate authorization).
            session.authorized_tools.insert(tool_name.to_string());

            // Capture the scope-resolution inputs before releasing the session
            // borrow, so governance.db persistence can use a disjoint &self.
            let session_mode = session.mode.clone();
            let session_agent_name = session.agent_name.clone();
            let session_project_id = session.project_id.clone();

            // chat.db.authorized_tools: written to preserve the authorization if
            // the runtime crashes mid-session. Kept for the ThisTool/ThisSession
            // scopes (otherwise they would be lost on restart). For the
            // persistent scopes it is redundant with governance.db but has no
            // side effect, to be cleaned up later.
            let now = now_rfc3339();
            if let Err(e) = self.repository.authorize_tool(session_id, tool_name, &now) {
                warn!(error = %e, "Failed to persist tool authorization");
            }

            self.persist_always_accept_scope(AlwaysAcceptScopeCtx {
                scope,
                session_mode,
                session_agent_name,
                session_project_id: session_project_id.as_deref(),
                session_id,
                tool_name,
            });
        }

        // Trace-log the enriched metadata (reason / scope) without breaking
        // the existing `log_tool_approval` SQL schema.
        log_resolution_metadata(&decision, session_id, message_id, tool_name);

        let decision_str = decision.as_str();
        let reason: Option<&str> = match &decision {
            ToolDecision::Refuse { reason: Some(r) } => Some(r.as_str()),
            _ => None,
        };

        let resolved_at = now_rfc3339();

        // Persist decision in approval log for history view.
        if let Err(e) = self.repository.log_tool_approval(ToolApprovalLogEntry {
            session_id,
            message_id,
            tool_name,
            decision: decision_str,
            resolved_at: &resolved_at,
            reason,
        }) {
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

    /// Persist an `AlwaysAccept` rule in governance.db according to its scope.
    ///
    /// `ThisTool` / `ThisSession` are in-session only (no governance write).
    /// `ThisAgent` derives the agent id from the session mode; `ThisProject`
    /// resolves the workspace path from the project repository.
    fn persist_always_accept_scope(&self, ctx: AlwaysAcceptScopeCtx<'_>) {
        let AlwaysAcceptScopeCtx {
            scope,
            session_mode,
            session_agent_name,
            session_project_id,
            session_id,
            tool_name,
        } = ctx;
        use super::types::AlwaysAcceptScope;
        match scope {
            AlwaysAcceptScope::ThisTool | AlwaysAcceptScope::ThisSession => {
                // No governance.db persistence, purely in-session.
            }
            AlwaysAcceptScope::ThisAgent => {
                let agent_id = match session_mode {
                    ChatMode::Libre | ChatMode::Companion => {
                        Some(APOLLIA_CHAT_AGENT_ID.to_string())
                    }
                    ChatMode::Agent => session_agent_name,
                };
                if let Some(aid) = agent_id {
                    persist_chat_allow_rule(
                        apollia_permissions::PermissionScope::Agent,
                        None,
                        Some(aid),
                        tool_name,
                    );
                }
            }
            AlwaysAcceptScope::ThisProject => {
                match self.resolve_project_workspace(session_project_id) {
                    Some(ws) => {
                        persist_chat_allow_rule(
                            apollia_permissions::PermissionScope::Project,
                            Some(ws),
                            None,
                            tool_name,
                        );
                    }
                    None => {
                        warn!(
                            session_id,
                            tool_name,
                            "ThisProject scope requested but session has no resolvable workspace_path; falling back to session-only authorization"
                        );
                    }
                }
            }
            AlwaysAcceptScope::Global => {
                persist_chat_allow_rule(
                    apollia_permissions::PermissionScope::Global,
                    None,
                    None,
                    tool_name,
                );
            }
        }
    }

    /// Resolve a session's project id to its workspace path, if any.
    fn resolve_project_workspace(&self, project_id: Option<&str>) -> Option<std::path::PathBuf> {
        let pid = project_id?;
        let repo = self.project_repo.as_ref()?;
        repo.get_project(pid)
            .ok()
            .and_then(|d| d.workspace_path.map(std::path::PathBuf::from))
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
    /// Resolve the todo handle for a session, enforcing 404 semantics.
    ///
    /// Returns [`ChatError::SessionNotFound`] for an unknown session, `Ok(None)`
    /// for a known session when no todo store is attached, and `Ok(Some(handle))`
    /// otherwise. Kept synchronous so the actor loop never holds a borrow of the
    /// SQLite connection across an `await`; the caller reads through the cloned
    /// handle (principle: one actor, one responsibility).
    fn resolve_todo_handle(&self, session_id: &str) -> Result<Option<TodoHandle>, ChatError> {
        let known = self.sessions.contains_key(session_id)
            || self.repository.get_session(session_id)?.is_some();
        if !known {
            return Err(ChatError::SessionNotFound(session_id.to_string()));
        }
        Ok(self.todo_handle.clone())
    }

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
            force_project_context_inject: false,
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

    /// Fork a session, create a child with a copy of the parent's history.
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
                // Fall back to repository count, session may not be in memory.
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

        // Update in-memory cache and flag the next user message so the
        // project-context provider re-runs (the initial-injection path is
        // gated on `is_first_message`, which is false for already-active
        // sessions).
        if let Some(session) = self.sessions.get_mut(session_id) {
            let new_pid = project_id.map(|s| s.to_string());
            if session.project_id != new_pid {
                session.force_project_context_inject = new_pid.is_some();
            }
            session.project_id = new_pid;
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
        let (meta, reply_tx) = self
            .pending_user_replies
            .remove(request_id)
            .ok_or_else(|| {
                ChatError::InternalError(format!(
                    "no pending ask_user request with id '{request_id}'"
                ))
            })?;

        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::ChatUserInputResolved {
                request_id: request_id.to_string(),
                session_id: meta.session_id,
            });

        let output = apollia_tools::tools::ask_user::AskUserOutput { answers };
        reply_tx.send(output).map_err(|_| {
            ChatError::InternalError(
                "ask_user reply channel closed (agent may have timed out)".into(),
            )
        })
    }

    /// Reject a pending `ask_user` request, sends skipped answers to unblock the agent.
    fn reject_user_input_internal(
        &mut self,
        request_id: &str,
        _reason: String,
    ) -> Result<(), ChatError> {
        let (meta, reply_tx) = self
            .pending_user_replies
            .remove(request_id)
            .ok_or_else(|| {
                ChatError::InternalError(format!(
                    "no pending ask_user request with id '{request_id}'"
                ))
            })?;

        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::ChatUserInputResolved {
                request_id: request_id.to_string(),
                session_id: meta.session_id,
            });

        // Deliver all-skipped answers to unblock the agent loop.
        let output = apollia_tools::tools::ask_user::AskUserOutput { answers: vec![] };
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
            let mut authorized_tools = self
                .repository
                .get_authorized_tools(&row.id)
                .unwrap_or_default();
            // Libre sessions: also seed from governance.db agent-scoped allow
            // rules so cross-session "always allow" decisions survive.
            if mode == ChatMode::Libre {
                let overrides = load_chat_libre_overrides();
                for tool in overrides.pre_authorized_tools {
                    authorized_tools.insert(tool);
                }
            }
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
                force_project_context_inject: false,
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
/// Returns a [`NativeChatToolInvoker`] configured with:
/// - the project's `workspace_path` when the session is linked to a project;
/// - `~/.apollia/` for free-chat sessions (no project), if that directory exists;
/// - `None` as a last resort (bash will then inherit the process CWD).
///
/// When `hitl` is provided, HITL filesystem support is enabled on the returned invoker.
async fn resolve_workspace_for_session(
    params: WorkspaceResolutionParams,
) -> Result<NativeChatToolInvoker, ChatError> {
    let WorkspaceResolutionParams {
        project_id,
        project_repo,
        hitl,
        pending_user_inputs,
        mcp_handle,
        chat_tools_config,
        session_id,
        mcp_loading,
        mcp_index,
        tool_search_limit,
    } = params;
    let session_id = session_id.as_str();
    let workspace_path = resolve_workspace_path(&project_id, &project_repo).await?;
    let mut invoker = NativeChatToolInvoker::new_unrestricted(workspace_path.clone());
    if let Some(pending) = pending_user_inputs.clone() {
        invoker = invoker.with_ask_user_support(pending);
    }

    // Convergence point: every tool outside the native hardcoded fast path,
    // MCP, Google connectors, read-only natives, future providers, is
    // resolved through a single `ToolDispatcher`.
    // The dispatcher applies the same permission engine + audit trail as
    // the Agent-mode pipeline. HITL-sensitive natives (file_write/edit,
    // bash, python_executor, notebook_edit) stay in the fast path because
    // their inline approval flow is not yet ported to events.
    let mut extra_executors: Vec<Box<dyn apollia_tools::executor::ToolExecutor>> = Vec::new();

    // MCP executors. Eager: one per tool registered by a connected server.
    // Deferred: the synthetic `tool_search` tool plus one executor per indexed
    // tool, so a tool found via search is invocable without a preloaded schema.
    collect_mcp_executors(
        mcp_handle,
        mcp_loading,
        &mcp_index,
        tool_search_limit,
        &mut extra_executors,
    )
    .await;

    // SaaS connectors: Google + Microsoft 365.
    extra_executors.extend(crate::connectors_bridge::build_google_executors());
    extra_executors.extend(crate::connectors_bridge::build_microsoft_executors());

    let sandbox_root = workspace_path.clone().unwrap_or_else(std::env::temp_dir);

    // When the supervisor handed us a `ChatToolsConfig`, build the full native
    // dispatcher (same factory the Agent-mode + Triggers pipeline uses). This
    // pulls in web_search, web_read, http_fetch, memory_search,
    // permission_rule_* and ask_user with the operator's `apollia.toml` cfg.
    // HITL-sensitive natives (bash, python_executor, file_write/edit,
    // notebook_edit) stay disabled from the dispatcher so the fast path
    // continues to drive their inline approval flow; migrating that flow to
    // EventBus events is tracked separately.
    let dispatcher = if let Some(cfg) = chat_tools_config.as_ref() {
        build_full_chat_dispatcher(FullDispatcherParams {
            cfg,
            session_id,
            sandbox_root: &sandbox_root,
            workspace_path: &workspace_path,
            pending_user_inputs: &pending_user_inputs,
            hitl: hitl.as_ref(),
            extra_executors,
        })
    } else {
        build_fallback_chat_dispatcher(&sandbox_root, extra_executors)
    };
    invoker = invoker.with_fallback_dispatcher(std::sync::Arc::new(
        apollia_tools::dispatcher_invoker::DispatcherToolInvoker::new(std::sync::Arc::new(
            dispatcher,
        )),
    ));
    if let Some(p) = hitl {
        Ok(invoker.with_hitl_support(p))
    } else {
        Ok(invoker)
    }
}

/// Resolve the sandbox root path for a chat session.
async fn resolve_workspace_path(
    project_id: &Option<String>,
    project_repo: &Option<Arc<ProjectRepository>>,
) -> Result<Option<std::path::PathBuf>, ChatError> {
    match project_id {
        None => {
            // Free chat: default to ~/.apollia/ so bash commands don't inherit the Tauri process CWD.
            Ok(std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".apollia"))
                .filter(|p| p.is_dir()))
        }
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
                    "project has no workspace_path configured - falling back to current_dir()"
                );
            }
            Ok(detail.workspace_path.map(std::path::PathBuf::from))
        }
    }
}

/// Register the MCP executors for a chat session.
///
/// In [`LoadingMode::Eager`] this pushes one [`McpToolExecutor`] per tool
/// advertised with a full schema by a connected server (the legacy behavior).
///
/// In [`LoadingMode::Deferred`] it pushes the synthetic `tool_search` executor
/// plus one [`McpToolExecutor`] per indexed tool, so a tool discovered through
/// `tool_search` can be invoked even though its schema was never preloaded. The
/// individual schemas are not sent to the LLM; they are fetched on demand at
/// call time.
///
/// [`McpToolExecutor`]: apollia_mcp::executor::McpToolExecutor
async fn collect_mcp_executors(
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    mcp_loading: LoadingMode,
    mcp_index: &[ToolIndexSnapshot],
    tool_search_limit: usize,
    extra_executors: &mut Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
) {
    let Some(handle) = mcp_handle else {
        return;
    };
    // Agent-initiative resource tools: the two read-only resource tools route
    // through the same MCP client manager handle as the per-server tools.
    extra_executors.extend(apollia_mcp::mcp_resources::build_mcp_resource_executors(
        &Some(handle.clone()),
    ));

    match mcp_loading {
        LoadingMode::Eager => {
            for status in handle.status().await {
                if !status.connected {
                    continue;
                }
                let Some(detail) = handle.server_detail(&status.name).await else {
                    continue;
                };
                for tool in detail.tools {
                    extra_executors.push(Box::new(apollia_mcp::executor::McpToolExecutor::new(
                        handle.clone(),
                        status.name.clone(),
                        tool.local_name.clone(),
                    )));
                }
            }
        }
        LoadingMode::Deferred => {
            // The synthetic search tool is the single MCP entry point exposed to
            // the LLM. It is registered even when the index is empty.
            extra_executors.push(Box::new(ToolSearchExecutor::new(
                mcp_index.to_vec(),
                tool_search_limit,
            )));
            for entry in mcp_index {
                extra_executors.push(Box::new(apollia_mcp::executor::McpToolExecutor::new(
                    handle.clone(),
                    entry.server_name.clone(),
                    entry.tool_name.clone(),
                )));
            }
        }
    }
}

/// Parameters for [`build_full_chat_dispatcher`].
struct FullDispatcherParams<'a> {
    cfg: &'a Arc<ChatToolsConfig>,
    session_id: &'a str,
    sandbox_root: &'a std::path::Path,
    workspace_path: &'a Option<std::path::PathBuf>,
    pending_user_inputs: &'a Option<apollia_tools::tools::ask_user::PendingUserInputs>,
    hitl: Option<&'a HitlInvokerParams>,
    extra_executors: Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
}

/// Build the full chat dispatcher (every native flows through the dispatcher,
/// HITL-sensitive natives wrapped, dynamic http_fetch).
fn build_full_chat_dispatcher(
    params: FullDispatcherParams<'_>,
) -> apollia_tools::executor::ToolDispatcher {
    let FullDispatcherParams {
        cfg,
        session_id,
        sandbox_root,
        workspace_path,
        pending_user_inputs,
        hitl,
        mut extra_executors,
    } = params;
    // We disable the dispatcher's default versions of:
    //   - file_write / file_edit / notebook_edit / bash / python:
    //     re-added below wrapped in `HitlFilesystemGuard`.
    //   - http_fetch: re-added as `DynamicAllowlistHttpFetch`.
    //
    // The remaining natives (file_read/list/glob/grep, notebook_read,
    // web_search, web_read, memory_search, ask_user, permission_rule_*)
    // are built untouched by `build_dispatcher_with`.
    const WRAPPED_NATIVES: &[&str] = &[
        "bash_executor",
        "python_executor",
        "file_write",
        "file_edit",
        "notebook_edit",
        "http_fetch",
    ];
    let mut disabled = cfg.tools_config.disabled.clone();
    for name in WRAPPED_NATIVES {
        if !disabled.iter().any(|d| d == name) {
            disabled.push((*name).to_string());
        }
    }

    let native_cfg = apollia_tools::NativeDispatcherConfig {
        sandbox_root: sandbox_root.to_path_buf(),
        agent_id: format!("apollia:chat:{session_id}"),
        venv_base_dir: cfg.data_dir.join("venvs"),
        // Per-session memory namespace so `memory_search` reads/writes
        // the chat session's own slice. Other namespaces could be added
        // here as shared read-only later.
        memory_namespace: Some(format!("apollia:chat:{session_id}")),
        memory_shared_namespaces: Vec::new(),
        memory_base_dir: cfg.data_dir.join("memory"),
        // Native http_fetch uses None, the wrapper below provides
        // dynamic allowlist behaviour preserving Chat Libre UX.
        http_allowlist: None,
        pending_user_inputs: pending_user_inputs.clone(),
        disabled_tools: disabled,
        brave_api_key: cfg.brave_api_key.clone(),
        web_search_config: cfg.tools_config.web_search.clone(),
        web_read_config: cfg.tools_config.web_read.clone(),
        governance_db_path: Some(cfg.data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME)),
    };

    // Wrap the HITL-sensitive natives with the approval-flow guard.
    if let Some(p) = hitl {
        push_hitl_natives(
            &mut extra_executors,
            p,
            workspace_path,
            sandbox_root,
            &native_cfg,
        );
    }

    // Dynamic-allowlist http_fetch (preserves the per-call host
    // injection that the legacy fast path did).
    extra_executors.push(Box::new(
        crate::chat::native_wrappers::DynamicAllowlistHttpFetch::new(),
    ));

    apollia_tools::build_dispatcher_with(&native_cfg, extra_executors)
}

/// Wrap and append the HITL-sensitive native executors (file write/edit,
/// notebook edit, bash, python) behind the filesystem approval guard.
fn push_hitl_natives(
    extra_executors: &mut Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
    p: &HitlInvokerParams,
    workspace_path: &Option<std::path::PathBuf>,
    sandbox_root: &std::path::Path,
    native_cfg: &apollia_tools::NativeDispatcherConfig,
) {
    let hitl_ctx = crate::chat::native_wrappers::HitlFilesystemContext {
        event_bus: p.event_bus.clone(),
        pending_fs: p.pending_fs.clone(),
        fs_allow_rules: p.fs_allow_rules.clone(),
        session_id: p.session_id.clone(),
        workspace_path: workspace_path.clone(),
        sandbox_root: sandbox_root.to_path_buf(),
        risk_config: p.risk_config.clone(),
    };

    let push_hitl = |execs: &mut Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
                     inner: Box<dyn apollia_tools::executor::ToolExecutor>,
                     op: apollia_tools::FilesystemOp| {
        execs.push(Box::new(
            crate::chat::native_wrappers::HitlFilesystemGuard::new(inner, op, hitl_ctx.clone()),
        ));
    };

    if let Ok(t) = apollia_tools::tools::file_write::FileWrite::new(sandbox_root.to_path_buf()) {
        push_hitl(
            extra_executors,
            Box::new(t),
            apollia_tools::FilesystemOp::Write,
        );
    }
    if let Ok(t) = apollia_tools::tools::file_edit::FileEdit::new(sandbox_root.to_path_buf()) {
        push_hitl(
            extra_executors,
            Box::new(t),
            apollia_tools::FilesystemOp::Write,
        );
    }
    if let Ok(t) =
        apollia_tools::tools::notebook_edit::NotebookEdit::new(sandbox_root.to_path_buf())
    {
        push_hitl(
            extra_executors,
            Box::new(t),
            apollia_tools::FilesystemOp::Write,
        );
    }
    push_hitl(
        extra_executors,
        Box::new(apollia_tools::tools::bash_executor::BashExecutor::new()),
        apollia_tools::FilesystemOp::Write,
    );
    if let Ok(t) = apollia_tools::tools::python_executor::PythonExecutor::new(
        &native_cfg.agent_id,
        &native_cfg.venv_base_dir,
    ) {
        push_hitl(
            extra_executors,
            Box::new(t),
            apollia_tools::FilesystemOp::Write,
        );
    }
}

/// Build the fallback dispatcher used when no `ChatToolsConfig` was provided
/// (e.g. tests): connector + MCP + read-only file natives only.
fn build_fallback_chat_dispatcher(
    sandbox_root: &std::path::Path,
    mut extra_executors: Vec<Box<dyn apollia_tools::executor::ToolExecutor>>,
) -> apollia_tools::executor::ToolDispatcher {
    if let Ok(t) = apollia_tools::tools::file_read::FileRead::new(sandbox_root.to_path_buf()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_list::FileList::new(sandbox_root.to_path_buf()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_glob::FileGlob::new(sandbox_root.to_path_buf()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) = apollia_tools::tools::file_grep::FileGrep::new(sandbox_root.to_path_buf()) {
        extra_executors.push(Box::new(t));
    }
    if let Ok(t) =
        apollia_tools::tools::notebook_read::NotebookRead::new(sandbox_root.to_path_buf())
    {
        extra_executors.push(Box::new(t));
    }
    apollia_tools::executor::ToolDispatcher::new(extra_executors)
}

/// Parameters for [`resolve_workspace_for_session`].
struct WorkspaceResolutionParams {
    project_id: Option<String>,
    project_repo: Option<Arc<ProjectRepository>>,
    hitl: Option<HitlInvokerParams>,
    pending_user_inputs: Option<apollia_tools::tools::ask_user::PendingUserInputs>,
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    chat_tools_config: Option<Arc<ChatToolsConfig>>,
    session_id: String,
    /// MCP tool loading strategy for this exchange.
    mcp_loading: LoadingMode,
    /// Aggregated deferred tool index, empty in eager mode.
    mcp_index: Vec<ToolIndexSnapshot>,
    /// Maximum `limit` accepted by the synthetic `tool_search` tool.
    tool_search_limit: usize,
}

/// Clonable handle for communicating with the [`ChatSessionManager`] actor.
///
/// All methods are async and return the result via oneshot channels.
/// This handle is `Clone + Send + Sync`.
#[derive(Clone)]
pub struct ChatSessionManagerHandle {
    tx: mpsc::Sender<ChatCommand>,
    /// Shared `ask_user` request registry, cloned by chat agent runners so
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
            mcp_handle,
            chat_tools_config,
            pending_user_replies: HashMap::new(),
            metrics: HashMap::new(),
            mcp_loading,
            tool_search_limit,
            todo_handle,
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
                                    session_id: pending_input.session_id.unwrap_or_default(),
                                    questions_json,
                                    context: pending_input.context,
                                    reply_tx: pending_input.reply_tx,
                                })
                                .await;
                        }
                        None => break, // Channel closed, manager shutting down.
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

    /// List aggregated A2A skill telemetry.
    pub async fn list_a2a_skill_telemetry(&self) -> Vec<crate::a2a::A2ASkillTelemetry> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::ListA2ASkillTelemetry { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// List A2A step provenance entries, optionally filtered by skill id.
    pub async fn list_a2a_step_provenance(
        &self,
        skill_id: Option<String>,
    ) -> Vec<crate::a2a::A2AStepProvenance> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::ListA2AStepProvenance {
                skill_id,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Check compatibility of a skill against a required semver version.
    pub async fn check_a2a_compatibility(
        &self,
        skill_id: String,
        required_version: String,
    ) -> Option<crate::a2a::A2ACompatibilityWarning> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::CheckA2ACompatibility {
                skill_id,
                required_version,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return None;
        }
        reply_rx.await.unwrap_or(None)
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

    /// Fetch aggregated metrics for a session.
    ///
    /// Returns `None` when the session is unknown or no exchange has completed
    /// yet. Accumulated in-memory from each [`ChatAgentResponse`], cleared on
    /// actor restart.
    pub async fn get_session_metrics(&self, session_id: SessionId) -> Option<SessionMetrics> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::GetSessionMetrics {
                session_id,
                reply: reply_tx,
            })
            .await;
        if sent.is_err() {
            return None;
        }
        reply_rx.await.ok().flatten()
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
                agent_class: None,
                user_memory_write: false,
                datasources: vec![],
                templates: vec![],
                secrets: vec![],
                check_commands: vec![],
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
            None, // no mcp handle in basic tests
            None, // no chat tools config in basic tests
            LoadingMode::Eager,
            20,
        )
        .expect("spawn manager")
    }

    fn fake_llm_router() -> Option<Arc<LlmRouter>> {
        Some(Arc::new(LlmRouter::empty()))
    }

    /// Minimal Libre-mode session params used by most manager tests.
    fn libre_session_params() -> CreateSessionParams {
        CreateSessionParams {
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: None,
            tools: vec![],
            project_id: None,
        }
    }

    #[tokio::test]
    async fn test_create_session_libre() {
        // GIVEN a ChatSessionManager with LLM configured
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        // WHEN create_session mode=Libre
        let info = handle
            .create_session(CreateSessionParams {
                mode: ChatMode::Libre,
                agent_name: None,
                system_prompt: None,
                tools: vec!["bash_executor".into()],
                project_id: None,
            })
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
            .create_session(CreateSessionParams {
                mode: ChatMode::Agent,
                agent_name: None,
                system_prompt: None,
                tools: vec![],
                project_id: None,
            })
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
        let result = handle.create_session(libre_session_params()).await;

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
            .create_session(libre_session_params())
            .await
            .expect("create 1");
        handle
            .create_session(libre_session_params())
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
            .create_session(libre_session_params())
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
    async fn test_get_session_todo_empty_and_unknown() {
        // GIVEN a freshly created session with no todo items
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));
        let info = handle
            .create_session(libre_session_params())
            .await
            .expect("create");

        // WHEN reading the todo for the known but empty session
        let items = handle
            .get_session_todo(info.id.clone())
            .await
            .expect("known session");

        // THEN an empty list is returned (200 semantics)
        assert!(items.is_empty());

        // WHEN reading the todo for an unknown session
        let unknown = handle.get_session_todo("does-not-exist".to_string()).await;

        // THEN SessionNotFound is returned (404 semantics)
        assert!(matches!(unknown, Err(ChatError::SessionNotFound(_))));

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
            None,
            None,
            LoadingMode::Eager,
            20,
        )
        .expect("spawn");

        let info = handle
            .create_session(libre_session_params())
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
            .create_session(libre_session_params())
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
            .create_session(libre_session_params())
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
            mcp_handle: None,
            chat_tools_config: None,
            pending_user_replies: HashMap::new(),
            metrics: HashMap::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
            a2a_invoker: None,
            project_context: None,
            project_repo: None,
            mcp_loading: LoadingMode::Eager,
            tool_search_limit: 20,
            todo_handle: None,
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
            force_project_context_inject: false,
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

        // THEN the actor stops, subsequent sends fail gracefully
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let result = handle.create_session(libre_session_params()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_close_already_closed_session() {
        // GIVEN a closed session
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = spawn_test_manager(&dir, fake_llm_router(), Arc::new(AlwaysOkLoader));

        let info = handle
            .create_session(libre_session_params())
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
            mcp_handle: None,
            chat_tools_config: None,
            pending_user_replies: HashMap::new(),
            metrics: HashMap::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
            a2a_invoker: None,
            project_context: None,
            project_repo: None,
            mcp_loading: LoadingMode::Eager,
            tool_search_limit: 20,
            todo_handle: None,
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
            mcp_handle: None,
            chat_tools_config: None,
            pending_user_replies: HashMap::new(),
            metrics: HashMap::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
            a2a_invoker: None,
            project_context: None,
            project_repo: None,
            mcp_loading: LoadingMode::Eager,
            tool_search_limit: 20,
            todo_handle: None,
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
            mcp_handle: None,
            chat_tools_config: None,
            pending_user_replies: HashMap::new(),
            metrics: HashMap::new(),
            user_memory: None,
            enrichment_extractor: None,
            tx,
            a2a_invoker: None,
            project_context: None,
            project_repo: None,
            mcp_loading: LoadingMode::Eager,
            tool_search_limit: 20,
            todo_handle: None,
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
            .create_session(CreateSessionParams {
                mode: ChatMode::Agent,
                agent_name: Some("nonexistent".into()),
                system_prompt: None,
                tools: vec![],
                project_id: None,
            })
            .await;

        // THEN Err(ChatError::AgentNotFound)
        assert!(matches!(result, Err(ChatError::AgentNotFound(_))));

        handle.shutdown().await;
    }
}
