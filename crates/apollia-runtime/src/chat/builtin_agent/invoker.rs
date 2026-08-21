use super::*;

// NativeChatToolInvoker: production tool execution

/// Parameters for attaching HITL filesystem support to a `NativeChatToolInvoker`.
pub struct HitlInvokerParams {
    pub session_id: String,
    pub event_bus: crate::eventbus::EventBusSender,
    pub pending_fs: super::super::types::PendingFilesystemApprovals,
    pub fs_allow_rules: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    pub risk_config: apollia_core::FilesystemRiskConfig,
}

/// Production [`ToolInvoker`] that dispatches to native Apollia tools.
///
/// Used by [`BuiltInChatAgent`] to execute tools in Chat Libre mode.
/// Each tool invocation is fully async (no `block_in_place`).
///
/// The sandbox root scopes all file operations to a specific directory.
/// It is resolved once per session from the project's `workspace_path`.
///
/// When HITL filesystem support is enabled (via [`NativeChatToolInvoker::with_hitl_support`]),
/// write operations are classified by risk level before execution. Operations with
/// `RiskLevel::Medium` or higher suspend the tool call and wait for user approval
/// via `HitlFilesystemModal` in the desktop UI.
pub struct NativeChatToolInvoker {
    // The fields below used to back the hardcoded `invoke_*` fast path.
    // After convergence, all tools (including HITL-sensitive ones) flow
    // through `fallback_dispatcher` and the executors carry their own
    // per-session context. The fields are retained for backward
    // compatibility with existing builder methods (`with_hitl_support`,
    // `with_ask_user_support`, etc.); their values are ignored by
    // `invoke()`. To be removed in a follow-up refactor.
    #[allow(
        dead_code,
        reason = "back-compat field, written by with_hitl_support/with_ask_user_support, removed in the follow-up invoker refactor"
    )]
    pub(in crate::chat::builtin_agent) sandbox_root: std::path::PathBuf,
    /// Original workspace path for risk classification (may differ from sandbox_root
    /// when sandbox_root has been resolved via fallback).
    workspace_path: Option<std::path::PathBuf>,
    /// EventBus sender for emitting `HitlFilesystemRequired` events.
    #[allow(
        dead_code,
        reason = "back-compat field, written by with_hitl_support, removed in the follow-up invoker refactor"
    )]
    event_bus: Option<crate::eventbus::EventBusSender>,
    /// Pending filesystem HITL approvals store.
    #[allow(
        dead_code,
        reason = "back-compat field, written by with_hitl_support, removed in the follow-up invoker refactor"
    )]
    pending_fs: Option<super::super::types::PendingFilesystemApprovals>,
    /// Session-level filesystem allow rules (shared Arc, not persisted).
    #[allow(
        dead_code,
        reason = "back-compat field, written by with_hitl_support, removed in the follow-up invoker refactor"
    )]
    fs_allow_rules: Option<std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>>,
    /// Session identifier for HITL events.
    #[allow(
        dead_code,
        reason = "back-compat field, written by with_hitl_support, removed in the follow-up invoker refactor"
    )]
    session_id: Option<String>,
    /// Filesystem risk configuration (path lists for system/credential paths).
    #[allow(
        dead_code,
        reason = "back-compat field, written by with_hitl_support, removed in the follow-up invoker refactor"
    )]
    risk_config: apollia_core::FilesystemRiskConfig,
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
            event_bus: None,
            pending_fs: None,
            fs_allow_rules: None,
            session_id: None,
            risk_config: apollia_core::FilesystemRiskConfig::default(),
            pending_user_inputs: None,
            fallback_dispatcher: None,
        }
    }

    /// Create a new invoker with unrestricted filesystem access.
    ///
    /// The sandbox root is set to `/` so file tools can access any path on the machine.
    /// `workspace_path` is kept solely for HITL risk classification: paths inside the
    /// workspace are rated Safe/Low, paths outside trigger Medium/High approval modals.
    ///
    /// Use this for Chat Libre sessions where the HITL layer is the security boundary,
    /// not the sandbox.
    pub fn new_unrestricted(workspace_path: Option<std::path::PathBuf>) -> Self {
        Self {
            sandbox_root: std::path::PathBuf::from("/"),
            workspace_path,
            event_bus: None,
            pending_fs: None,
            fs_allow_rules: None,
            session_id: None,
            risk_config: apollia_core::FilesystemRiskConfig::default(),
            pending_user_inputs: None,
            fallback_dispatcher: None,
        }
    }

    /// Attach a generic fallback dispatcher that handles any tool not
    /// covered by the hardcoded native fast path. The fast path stays in
    /// place (Chat Libre keeps inline HITL filesystem semantics for
    /// `file_write` / `file_edit` etc.); the fallback is consulted only
    /// for unknown names, so MCP, connector, and future-provider tools
    /// flow through it uniformly without per-family special cases.
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
    pub fn with_ask_user_support(
        mut self,
        pending: apollia_tools::tools::ask_user::PendingUserInputs,
    ) -> Self {
        self.pending_user_inputs = Some(pending);
        self
    }

    /// Attach HITL filesystem support to this invoker.
    ///
    /// When enabled, write and edit operations are classified by risk level before
    /// execution. Operations at `RiskLevel::Medium` or above are suspended pending
    /// user approval via `HitlFilesystemModal`.
    pub fn with_hitl_support(mut self, params: HitlInvokerParams) -> Self {
        self.session_id = Some(params.session_id);
        self.event_bus = Some(params.event_bus);
        self.pending_fs = Some(params.pending_fs);
        self.fs_allow_rules = Some(params.fs_allow_rules);
        self.risk_config = params.risk_config;
        self
    }

    /// Check if a filesystem write operation needs HITL approval.
    ///
    /// Returns `Ok(())` if the operation can proceed (Safe/Low risk or already approved).
    /// Returns `Err(String)` with a human-readable reason if denied.
    /// Awaits user decision if the operation is Medium or above and not in allow rules.
    ///
    /// Superseded by [`crate::chat::native_wrappers::HitlFilesystemGuard`].
    #[allow(
        dead_code,
        reason = "kept as the reference implementation behind the invoke_* methods listed below until they are deleted"
    )]
    async fn check_fs_hitl(
        &self,
        op: apollia_tools::FilesystemOp,
        resolved_path: &std::path::Path,
        preview: apollia_core::FilesystemPreview,
    ) -> Result<(), String> {
        use apollia_tools::RiskLevel;

        let level = apollia_tools::RiskClassifier::classify_filesystem(
            op,
            resolved_path,
            self.workspace_path.as_deref(),
            &self.risk_config,
        );

        // Safe and Low: no friction needed.
        if level < RiskLevel::Medium {
            return Ok(());
        }

        // Check session allow rules.
        let rule_key = format!("{}:{}", op.as_str(), level.as_str());
        if let Some(ref rules) = self.fs_allow_rules {
            let guard = rules.lock().expect("fs_allow_rules lock poisoned");
            if guard.contains(&rule_key) {
                return Ok(());
            }
        }

        // No pending store: fall back to approve (invoker running without HITL support).
        let (event_bus, pending_fs, session_id) = match (
            self.event_bus.as_ref(),
            self.pending_fs.as_ref(),
            self.session_id.as_deref(),
        ) {
            (Some(eb), Some(pf), Some(sid)) => (eb, pf, sid),
            _ => return Ok(()),
        };

        let request_id = uuid::Uuid::new_v4().to_string();

        // Emit the HITL event so the desktop UI can show the modal.
        let _ = event_bus.send(apollia_core::RuntimeEvent::HitlFilesystemRequired {
            request_id: request_id.clone(),
            session_id: session_id.to_string(),
            level: level.as_str().to_string(),
            op: op.as_str().to_string(),
            path: resolved_path.to_string_lossy().to_string(),
            preview,
        });

        // Register and await the decision (5 minute timeout).
        let rx = pending_fs.register(request_id.clone());

        let decision = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_else(super::super::types::FsHitlDecision::deny);

        match decision {
            super::super::types::FsHitlDecision::Approve => Ok(()),
            super::super::types::FsHitlDecision::Deny { reason } => {
                Err(reason.unwrap_or_else(|| "User denied filesystem operation".to_string()))
            }
            super::super::types::FsHitlDecision::AlwaysAllow {
                scope: _,
                op: rule_op,
                level: rule_level,
            } => {
                // The current session always stores a local rule regardless of
                // the requested scope, broader scopes (project, global) are
                // persisted by the desktop layer before this point is reached.
                if let Some(ref rules) = self.fs_allow_rules {
                    let mut guard = rules.lock().expect("fs_allow_rules lock poisoned");
                    guard.insert(format!("{rule_op}:{rule_level}"));
                }
                Ok(())
            }
        }
    }

    /// Execute `bash_executor` with the given JSON arguments.
    #[allow(
        dead_code,
        reason = "replaced by HitlFilesystemGuard(BashExecutor) via fallback_dispatcher"
    )]
    async fn invoke_bash(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::bash_executor::{BashExecutor, BashInput};

        let command = arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or("bash_executor: missing 'command' field")?
            .to_string();
        let timeout_secs = arguments
            .get("timeout_seconds")
            .or_else(|| arguments.get("timeout_secs"))
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let result = BashExecutor::new()
            .run(BashInput {
                command,
                timeout_secs,
                working_dir: self.workspace_path.clone(),
            })
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
            "duration_ms": result.duration_ms,
        })
        .to_string())
    }

    // `file_read` migrated to the shared ToolDispatcher: the executor is
    // registered by `chat::manager::resolve_workspace_for_session` and
    // reached via `fallback_dispatcher`. No inline HITL: a read-only tool
    // is gated by the loop's authorization set like any other.

    /// Execute `file_write` with the given JSON arguments.
    #[allow(
        dead_code,
        reason = "replaced by HitlFilesystemGuard(FileWrite) via fallback_dispatcher"
    )]
    async fn invoke_file_write(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::file_write::{FileWrite, FileWriteInput};
        use apollia_tools::FilesystemOp;

        let input: FileWriteInput = serde_json::from_value(arguments.clone())
            .map_err(|e| format!("file_write: invalid arguments: {e}"))?;

        // Resolve path for risk classification (before creating the tool).
        let resolved_path = self
            .sandbox_root
            .join(&input.path)
            .canonicalize()
            .unwrap_or_else(|_| self.sandbox_root.join(&input.path));

        // Build a diff preview: read existing file content as "before".
        let before = tokio::fs::read_to_string(&resolved_path)
            .await
            .unwrap_or_default();
        let after = input.content.clone();
        const PREVIEW_LIMIT: usize = 4096;
        let (before_trunc, after_trunc, truncated) =
            if before.len() > PREVIEW_LIMIT || after.len() > PREVIEW_LIMIT {
                (
                    before.chars().take(PREVIEW_LIMIT).collect::<String>(),
                    after.chars().take(PREVIEW_LIMIT).collect::<String>(),
                    true,
                )
            } else {
                (before, after, false)
            };

        let preview = apollia_core::FilesystemPreview::Diff {
            before: before_trunc,
            after: after_trunc,
            truncated,
        };

        self.check_fs_hitl(FilesystemOp::Write, &resolved_path, preview)
            .await?;

        let tool = FileWrite::new(self.sandbox_root.clone()).map_err(|e| e.to_string())?;
        tool.run(input).await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"written": true}).to_string())
    }

    // `file_list` migrated to the shared dispatcher. Reached via fallback dispatcher.

    /// Execute `file_edit` with the given JSON arguments.
    #[allow(
        dead_code,
        reason = "replaced by HitlFilesystemGuard(FileEdit) via fallback_dispatcher"
    )]
    async fn invoke_file_edit(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::file_edit::{FileEdit, FileEditInput};
        use apollia_tools::FilesystemOp;

        let input: FileEditInput = serde_json::from_value(arguments.clone())
            .map_err(|e| format!("file_edit: invalid arguments: {e}"))?;

        // Resolve path for risk classification.
        let resolved_path = self
            .sandbox_root
            .join(&input.path)
            .canonicalize()
            .unwrap_or_else(|_| self.sandbox_root.join(&input.path));

        // Build diff preview from old_text/new_text fields.
        const PREVIEW_LIMIT: usize = 4096;
        let before: String = input.old_text.chars().take(PREVIEW_LIMIT).collect();
        let after: String = input.new_text.chars().take(PREVIEW_LIMIT).collect();
        let truncated =
            input.old_text.len() > PREVIEW_LIMIT || input.new_text.len() > PREVIEW_LIMIT;

        let preview = apollia_core::FilesystemPreview::Diff {
            before,
            after,
            truncated,
        };

        self.check_fs_hitl(FilesystemOp::Write, &resolved_path, preview)
            .await?;

        let tool = FileEdit::new(self.sandbox_root.clone()).map_err(|e| e.to_string())?;
        let output = tool.run(input).await.map_err(|e| e.to_string())?;
        serde_json::to_string(&output).map_err(|e| e.to_string())
    }

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
    // is wrapped in `HitlFilesystemGuard` and creates its virtualenv on the
    // first execution. The dead copy that used to live here was the only place
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

        let base_dir = apollia_core::paths::home_dir_or_temp()
            .join(".apollia")
            .join("memory");
        let tool = MemorySearchTool::new("user".to_string(), vec![], base_dir);
        let input: MemorySearchInput = serde_json::from_value(arguments.clone())
            .map_err(|e| format!("memory_search: invalid arguments: {e}"))?;
        let output = tool.run(input).await.map_err(|e| e.to_string())?;
        serde_json::to_string(&output).map_err(|e| e.to_string())
    }

    // `notebook_read` migrated to the shared dispatcher.

    /// Execute `notebook_edit` with the given JSON arguments.
    ///
    /// Applies a sequence of atomic cell operations to a Jupyter `.ipynb` notebook,
    /// writing the modified notebook back to disk. Only nbformat v4 is supported.
    #[allow(
        dead_code,
        reason = "replaced by HitlFilesystemGuard(NotebookEdit) via fallback_dispatcher"
    )]
    async fn invoke_notebook_edit(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::notebook_edit::{NotebookEdit, NotebookEditInput};

        let tool = NotebookEdit::new(self.sandbox_root.clone()).map_err(|e| e.to_string())?;
        let input: NotebookEditInput = serde_json::from_value(arguments.clone())
            .map_err(|e| format!("notebook_edit: invalid arguments: {e}"))?;
        let output = tool.run(input).await.map_err(|e| e.to_string())?;
        serde_json::to_string(&output).map_err(|e| e.to_string())
    }

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
        // `fallback_dispatcher`. HITL-sensitive ones are wrapped in
        // `HitlFilesystemGuard`, `http_fetch` via `DynamicAllowlistHttpFetch`,
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
