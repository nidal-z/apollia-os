use super::*;

// NativeChatToolInvoker: production tool execution

/// Production [`ToolInvoker`] that dispatches to native Apollia tools.
///
/// Used by [`BuiltInChatAgent`] to execute tools in Chat Libre mode.
/// Each tool invocation is fully async (no `block_in_place`).
///
/// The sandbox root scopes all file operations to a specific directory.
/// It is resolved once per session from the project's `workspace_path`.
///
/// Consent is not decided here: the loop asks governance before every call,
/// by action family, and only an allowed or accepted call reaches `invoke`.
pub struct NativeChatToolInvoker {
    // The fields below used to back the hardcoded `invoke_*` fast path.
    // After convergence, every tool flows through `fallback_dispatcher` and
    // the executors carry their own per-session context. The fields are
    // retained for the builder methods that still write them
    // (`with_ask_user_support`); their values are ignored by `invoke()`.
    #[allow(
        dead_code,
        reason = "back-compat field, written by the constructors, removed in the follow-up invoker refactor"
    )]
    pub(in crate::chat::builtin_agent) sandbox_root: std::path::PathBuf,
    /// Workspace path of the session, when it has one.
    workspace_path: Option<std::path::PathBuf>,
    /// Session identifier, sent with every `ask_user` request.
    #[allow(
        dead_code,
        reason = "back-compat field, written by with_ask_user_support, removed in the follow-up invoker refactor"
    )]
    session_id: Option<String>,
    /// Pending user input registry for the `ask_user` tool.
    #[allow(
        dead_code,
        reason = "back-compat field, written by with_ask_user_support, removed in the follow-up invoker refactor"
    )]
    pending_user_inputs: Option<apollia_tools::tools::ask_user::PendingUserInputs>,
    /// Generic fallback for any tool that isn't in the hardcoded native
    /// match. When present, MCP + connector + future-tool calls are all
    /// resolved through a single [`ToolDispatcher`] wrapped in
    /// [`apollia_tools::dispatcher_invoker::DispatcherToolInvoker`], or
    /// any provider-specific [`ToolInvoker`] (e.g.
    /// `GoogleChatToolInvoker`). This is the convergence path that
    /// replaces the per-family special-case fields previously bolted on
    /// the invoker.
    fallback_dispatcher: Option<Arc<dyn ToolInvoker>>,
}

impl NativeChatToolInvoker {
    /// Create a new invoker with the given workspace as sandbox root.
    ///
    /// If `workspace_path` is `Some(p)` and `p` is an existing directory, it is
    /// used as-is. Otherwise falls back to `std::env::current_dir()`, and finally
    /// to `std::env::temp_dir()`. Never falls back to `$HOME`.
    pub fn new_with_workspace(workspace_path: Option<std::path::PathBuf>) -> Self {
        let sandbox_root = workspace_path
            .clone()
            .filter(|p| p.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(std::env::temp_dir);
        Self {
            sandbox_root,
            workspace_path,
            session_id: None,
            pending_user_inputs: None,
            fallback_dispatcher: None,
        }
    }

    /// Attach the dispatcher every tool call is resolved through, natives,
    /// MCP, connectors and future providers alike.
    pub fn with_fallback_dispatcher(mut self, invoker: Arc<dyn ToolInvoker>) -> Self {
        self.fallback_dispatcher = Some(invoker);
        self
    }

    /// Returns the workspace path associated with this invoker, if any.
    pub fn workspace_path(&self) -> Option<&std::path::Path> {
        self.workspace_path.as_deref()
    }

    /// Attach `ask_user` tool support to this invoker.
    ///
    /// When enabled, the agent can call the `ask_user` tool to pose structured
    /// questions to the user and wait for responses.
    ///
    /// The session id travels with every request the tool registers, so the
    /// inbox can open the conversation that asked.
    pub fn with_ask_user_support(
        mut self,
        pending: apollia_tools::tools::ask_user::PendingUserInputs,
        session_id: impl Into<String>,
    ) -> Self {
        self.pending_user_inputs = Some(pending);
        self.session_id = Some(session_id.into());
        self
    }

    // `file_read` migrated to the shared ToolDispatcher: the executor is
    // registered by `chat::manager::resolve_workspace_for_session` and
    // reached via `fallback_dispatcher`. No inline HITL: a read-only tool
    // is gated by the loop's authorization set like any other.

    // `file_list` migrated to the shared dispatcher. Reached via fallback dispatcher.

    // `file_glob` + `file_grep` migrated to the shared dispatcher.
    // Both reached via fallback dispatcher.

    /// Execute `http_fetch` with the given JSON arguments.
    ///
    /// In libre chat mode, the URL's hostname is dynamically added to the allowlist
    /// since the user explicitly enabled this tool and tool calls are HITL-approved.
    #[allow(
        dead_code,
        reason = "replaced by DynamicAllowlistHttpFetch via fallback_dispatcher"
    )]
    async fn invoke_http_fetch(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::http_fetch::{HttpFetch, HttpFetchInput};

        let input: HttpFetchInput = serde_json::from_value(arguments.clone())
            .map_err(|e| format!("http_fetch: invalid arguments: {e}"))?;

        let hostname = extract_hostname(&input.url)
            .ok_or_else(|| "http_fetch: cannot parse hostname from URL".to_string())?;

        let tool = HttpFetch::new(Some(vec![hostname]));
        let output = tool.run(input).await.map_err(|e| e.to_string())?;
        serde_json::to_string(&output).map_err(|e| e.to_string())
    }

    // `python_executor` migrated to the shared dispatcher, where the executor
    // creates its virtualenv on the first execution. The dead copy that used to live here was the only place
    // still calling `setup_venv` for a chat session.

    /// Execute `memory_search` with the given JSON arguments.
    ///
    /// Searches the user's local memory store (`~/.apollia/memory/user.db`) using
    /// FTS5 full-text search. The namespace is fixed to `"user"` in chat libre mode;
    /// agents have their own namespaced databases.
    #[allow(
        dead_code,
        reason = "replaced by MemorySearchTool with per-session namespace via fallback_dispatcher"
    )]
    async fn invoke_memory_search(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::memory_search::{MemorySearchInput, MemorySearchTool};

        let base_dir = apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp())
            .join("memory");
        let tool = MemorySearchTool::new("user".to_string(), vec![], base_dir);
        let input: MemorySearchInput = serde_json::from_value(arguments.clone())
            .map_err(|e| format!("memory_search: invalid arguments: {e}"))?;
        let output = tool.run(input).await.map_err(|e| e.to_string())?;
        serde_json::to_string(&output).map_err(|e| e.to_string())
    }

    // `notebook_read` migrated to the shared dispatcher.

    /// Ask the user structured questions and wait for responses.
    ///
    /// Posts the questions to the [`PendingUserInputs`] registry and blocks until
    /// the UI delivers the answers through the oneshot channel.
    #[allow(
        dead_code,
        reason = "replaced by AskUserExecutor with session_id via fallback_dispatcher"
    )]
    async fn invoke_ask_user(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::ask_user::AskUserExecutor;

        let pending = self
            .pending_user_inputs
            .as_ref()
            .ok_or("ask_user: tool not available in this session (no pending registry)")?;

        let executor = AskUserExecutor::new_with_session(pending, self.session_id.clone());

        use apollia_tools::executor::ToolExecutor;
        let result = executor
            .execute(arguments.clone())
            .await
            .map_err(|e| format!("ask_user: {e}"))?;

        serde_json::to_string(&result).map_err(|e| format!("ask_user serialization: {e}"))
    }

    // `web_search` + `web_read` migrated to the shared ToolDispatcher; see
    // `chat::manager::resolve_workspace_for_session` for the executor
    // wiring. The dispatcher reads the operator's Brave key + `apollia.toml`
    // web cfg, so Chat Libre, Agent mode and Triggers now share the same
    // backend priority and SSRF settings.
}

#[async_trait::async_trait]
impl ToolInvoker for NativeChatToolInvoker {
    async fn invoke(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, String> {
        // Full convergence: every native tool goes through the
        // `fallback_dispatcher`, `http_fetch` via `DynamicAllowlistHttpFetch`,
        // everything else as stock executors. No fast path, no special
        // cases, a single governed path plus audit trail across
        // Chat Libre / Chat Agent / Triggers.
        match self.fallback_dispatcher.as_ref() {
            Some(invoker) => invoker.invoke(tool_name, arguments).await,
            None => Err(format!(
                "unknown tool: {tool_name} \
                 (no dispatcher attached - invoker built outside chat manager)"
            )),
        }
    }
}
