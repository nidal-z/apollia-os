//! BuiltInChatAgent, Rust-native ReAct loop for Chat Libre mode.
//!
//! Implements the core reasoning loop: LLM, tool call, approval, result, LLM.
//! Protected by [`StepBudget`] (the runtime step safeguard) and integrated with
//! the HITL approval flow via [`PendingChatApprovals`].
//!
//! Uses `LlmRouter.stream()` for token-by-token streaming.
//! Each token emits a `ChatToken` RuntimeEvent on the EventBus so the SSE
//! stream can forward it to the client in real time.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tracing::{info, warn};

use apollia_core::{
    AutonomyLevel, AutonomyLevelConfig, CeilingAction, ORIAConfig, RunId, RuntimeEvent,
};
use apollia_llm::routing_level::{EscalationSignal, LlmRoutingLevel};
use apollia_llm::types::{
    ChatMessage as LlmChatMessage, CompletionModel, CompletionRequest, StreamChunk, TokenUsage,
    ToolCall, ToolSpec,
};
use apollia_llm::{LlmRouter, MetaOrchestratorHandle, ObservabilityConfig, ToolInvoker};
use apollia_mcp::tool_search::{tool_search_input_schema, ToolIndexSnapshot};
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::budget::StepBudget;
use apollia_oria::context_manager::ContextManager;
use apollia_oria::verification::{
    CheckFailure, CheckInvoker, CheckOutcome, Correction, CriticPass, CriticReport,
    VerificationLoop,
};
use apollia_tools::ToolRegistryHandle;

use super::types::{
    ApprovalTimeoutParams, ChatError, ChatMessage, ChatRole, PendingChatApprovals, ToolCallRecord,
    ToolCallStatus, ToolDecision,
};
use crate::a2a::A2AInvoker;
use crate::chat::a2a_tools::generate_a2a_tool_specs;
use crate::chat::todo_handle::TodoHandle;
use crate::chat::todo_tool::{run_todo_write, todo_write_spec, TODO_WRITE_TOOL_NAME};
use crate::eventbus::EventBusSender;
use crate::hooks::executor::{HookDecision, HookExecutor};

/// Default timeout for chat tool approval requests (5 minutes).
const CHAT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// Maximum number of read-only tool calls executed concurrently within a single
/// agent turn. Write calls and read-only calls awaiting approval stay
/// sequential regardless of this cap. Mirrors the ORIA batch cap.
const MAX_CONCURRENT_READONLY_TOOL_CALLS: usize = 10;

/// Default number of recent messages in the sliding context window.
pub const DEFAULT_CONTEXT_WINDOW_SIZE: usize = 20;

/// Prefix of the reminder message re-injected after a context compaction so the
/// agent keeps its task list in view once the history is truncated.
const TODO_REMINDER_PREFIX: &str =
    "[System reminder] Your current task list after context compaction:";

// NativeChatToolInvoker: production tool execution

/// Parameters for attaching HITL filesystem support to a `NativeChatToolInvoker`.
pub(crate) struct HitlInvokerParams {
    pub session_id: String,
    pub event_bus: crate::eventbus::EventBusSender,
    pub pending_fs: super::types::PendingFilesystemApprovals,
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
    sandbox_root: std::path::PathBuf,
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
    pending_fs: Option<super::types::PendingFilesystemApprovals>,
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
            .unwrap_or_else(super::types::FsHitlDecision::deny);

        match decision {
            super::types::FsHitlDecision::Approve => Ok(()),
            super::types::FsHitlDecision::Deny { reason } => {
                Err(reason.unwrap_or_else(|| "User denied filesystem operation".to_string()))
            }
            super::types::FsHitlDecision::AlwaysAllow {
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
    // reached via `fallback_dispatcher`. No inline HITL, so the
    // dispatcher's permission engine stays in charge.

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

    /// Execute `python_executor` with the given JSON arguments.
    ///
    /// Runs Python code in a dedicated `chat-libre` virtualenv at
    /// `~/.apollia/venvs/chat-libre/venv/`. The venv is lazily created on first
    /// invocation, no packages are pre-installed (the LLM can only use stdlib).
    #[allow(
        dead_code,
        reason = "replaced by HitlFilesystemGuard(PythonExecutor) via fallback_dispatcher"
    )]
    async fn invoke_python(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::python_executor::{PythonExecutor, PythonInput};

        let venv_base = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(".apollia")
            .join("venvs");
        let executor = PythonExecutor::new("chat-libre", &venv_base).map_err(|e| e.to_string())?;

        // Lazily set up the venv on first call (idempotent, skips if already exists).
        executor
            .setup_venv(&[])
            .await
            .map_err(|e| format!("python_executor: venv setup failed: {e}"))?;

        let code = arguments
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or("python_executor: missing 'code' field")?
            .to_string();
        let timeout_secs = arguments
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let output = executor
            .run(PythonInput { code, timeout_secs })
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "stdout": output.stdout,
            "stderr": output.stderr,
            "exit_code": output.exit_code,
            "duration_ms": output.duration_ms,
        })
        .to_string())
    }

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

        let base_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
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
        // cases, a single permission engine + audit trail path across
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

/// Maximum number of characters for input/output previews in events.
const PREVIEW_MAX_LEN: usize = 200;

/// Maximum number of characters for tool output injected into LLM context.
/// Outputs longer than this are truncated with a notice so the LLM knows
/// results were cut and can refine its command.
const TOOL_OUTPUT_MAX_LEN: usize = 4000;

/// Default system prompt used when no custom prompt is provided.
pub const DEFAULT_SYSTEM_PROMPT: &str = "\
Tu es un assistant IA qui aide l'utilisateur en exécutant des actions concrètes via ses outils. \
Réponds de manière concise et naturelle.

## Comportement

- **Agis d'abord** : quand l'utilisateur demande quelque chose de faisable avec tes outils, \
exécute-le immédiatement. Ne demande pas de précisions sauf si la requête est réellement ambiguë.
- **Langage naturel** : parle comme un humain, pas comme une machine. N'expose jamais de \
détails techniques internes (chemins système, noms d'outils, limitations techniques).
- **Autonomie** : si une première approche échoue, essaie une alternative avant de signaler un \
problème à l'utilisateur.
- **Limites honnêtes** : si une tâche dépasse réellement les capacités de tes outils disponibles, \
dis-le clairement et propose ce que tu peux faire. Ne refuse jamais d'utiliser un outil qui figure \
dans ta liste - vérifie d'abord ta liste avant de déclarer une capacité absente.

## Principes d'utilisation des outils

1. **Contexte d'abord** : vérifie si l'information est déjà dans le contexte de la conversation \
avant d'exécuter un outil.
2. **Commande minimale** : choisis l'approche la plus rapide et ciblée. Ne scanne jamais un \
filesystem entier quand un scope restreint suffit.
3. **Timeout proportionnel** : adapte `timeout_secs` à la complexité réelle de la commande.
4. **Résilience** : si une commande échoue ou timeout, analyse la cause et essaie une approche \
différente plutôt que de relancer la même commande.
5. **Jamais de valeurs fictives** : n'invente jamais un paramètre inconnu (clé API, token, URL, \
chemin, identifiant). Si une information requise est absente, demande-la explicitement à \
l'utilisateur avant d'appeler l'outil. Utiliser un placeholder comme `YOUR_API_KEY` ou \
`<TOKEN>` dans un appel réel est interdit.
6. **Résolution d'identifiants par nom** : quand l'utilisateur référence un fichier, document, \
feuille, présentation ou dossier par son **titre** sans fournir d'ID, **NE DEMANDE PAS l'ID** - \
recherche-le toi-même via un outil de listing approprié (`gdrive.find_by_name` pour Google Drive, \
ou son équivalent), puis enchaîne l'opération demandée. Demander un ID alphanumérique à un \
utilisateur est une mauvaise expérience que tu dois éviter.
7. **Jamais de succès hallucinés** : tu ne dois JAMAIS prétendre qu'une opération a réussi sans \
avoir vu un résultat d'outil explicitement positif dans l'historique de la conversation. Si ton \
dernier appel d'outil a échoué, n'a pas répondu, ou si tu n'arrives plus à formuler un tool call \
valide après plusieurs tentatives : annonce clairement à l'utilisateur que l'opération n'a pas \
abouti, explique brièvement ce qui a été tenté, et arrête-toi. Reformuler le même appel en boucle \
sans nouveau résultat est interdit.
8. **Découverte des noms d'onglets Sheets** : pour Google Sheets, le **titre du spreadsheet** \
(visible en haut) ≠ le **titre d'un onglet** (l'onglet par défaut s'appelle souvent `Sheet1`, mais \
en français Google le nomme `Feuille 1`). Les paramètres `range` (`gsheets.read_values`, \
`gsheets.update_values`, `gsheets.append_values`) attendent le **nom de l'onglet**, pas du \
spreadsheet. Quand le nom contient un espace, encadre-le de guillemets simples : \
`'Feuille 1'!A1:C1`. Si tu n'es pas certain du nom de l'onglet, appelle `gsheets.list_sheets` \
avant d'écrire.

## Pattern d'enchaînement obligatoire - Google par titre

Quand l'utilisateur référence un asset Google par son **titre** (jamais par un ID alphanumérique), \
tu DOIS enchaîner SANS DEMANDE INTERMÉDIAIRE :

> Utilisateur : « lis la plage A1:C1 de la feuille Apollia Test »
>
> 1. `gdrive.find_by_name(name=\"Apollia Test\", mime_type_filter=\"spreadsheet\")` → récupère \
spreadsheet_id depuis `matches[0].id`.
> 2. `gsheets.list_sheets(spreadsheet_id=<id>)` → récupère le titre de l'onglet par défaut.
> 3. `gsheets.read_values(spreadsheet_id=<id>, range=\"'<onglet>'!A1:C1\")`.
> 4. Réponse à l'utilisateur avec le contenu.

Même pattern pour `gdocs.*` (`gdrive.find_by_name(mime_type_filter=\"document\")` puis \
`gdocs.read_text` / `gdocs.append_text`), `gslides.*` (`mime_type_filter=\"presentation\"`), \
et tout autre asset Google identifié par titre. **Demander l'ID alphanumérique à \
l'utilisateur est un échec - tu as les outils pour le résoudre seul.**
";

/// System prompt variant selected for autonomous tiers (supervised,
/// bounded_autonomous, long_autonomous).
///
/// Instructs the agent to continue until the objective is complete and verified,
/// to prefer sub-agents for research tasks, and to batch independent tool calls.
/// The assisted tier keeps the reactive [`DEFAULT_SYSTEM_PROMPT`] instead.
pub const PERSEVERANCE_SYSTEM_PROMPT: &str = "\
Tu es un agent IA autonome qui execute des taches complexes jusqu'a leur completion totale \
et leur verification. Tu n'interromps pas ta progression pour demander des confirmations \
intermediaires sauf si une decision irreversible ou une information bloquante est requise.

## Doctrine de completion

- **Persevere jusqu'a l'objectif** : continue d'agir tant que la tache n'est pas accomplie \
et verifiee. Un resultat partiel n'est pas un succes.
- **Verifie avant de conclure** : avant de declarer la tache terminee, execute les verifications \
disponibles (tests, lint, relecture du resultat) et corrige les problemes detectes.
- **Ne jamais simuler un succes** : si tu n'as pas vu un resultat positif explicite dans l'historique \
des outils, tu n'as pas reussi. Annonce clairement un echec plutot que d'inventer un succes.

## Utilisation des outils

1. **Regroupe les appels independants** : si plusieurs informations peuvent etre collectees \
en parallele, formule plusieurs appels d'outils dans le meme tour plutot que de les sequencer \
inutilement.
2. **Prefere les sous-agents pour la recherche** : delege les taches de recherche ou d'analyse \
parallelisables a des sous-agents specialises quand ils sont disponibles.
3. **Contexte d'abord** : verifie si l'information est deja dans le contexte avant d'appeler \
un outil.
4. **Commande minimale** : choisis l'approche la plus rapide et la plus ciblee. Ne scanne pas \
un filesystem entier quand un scope restreint suffit.
5. **Resilience** : si une approche echoue, analyse la cause et essaie une alternative \
differente plutot que de relancer la meme commande.

## Limites et transparence

- **Limites honnetes** : si une tache depasse reellement les capacites des outils disponibles, \
dis-le clairement. Ne refuse jamais d'utiliser un outil qui figure dans ta liste.
- **Budget conscient** : si tu approches les limites de ton budget d'execution, notifie \
l'utilisateur et propose de continuer dans une nouvelle session avec l'etat acquis.
- **Jamais de valeurs fictives** : n'invente jamais un parametre inconnu. Si une information \
requise est absente, demande-la explicitement avant d'appeler un outil.
";

/// Response produced by a complete chat exchange.
#[derive(Debug, Clone)]
pub struct ChatAgentResponse {
    /// Final text content from the LLM.
    pub content: String,
    /// All tool calls made during the exchange.
    pub tool_calls: Vec<ToolCallRecord>,
    /// Tool names newly added to the session allowlist (via AlwaysAccept).
    pub newly_authorized: Vec<String>,
    /// Cumulative token usage across all LLM calls in the exchange.
    pub tokens_used: TokenUsage,
    /// Concatenated thinking/reasoning blocks extracted from `<think>...</think>` tags.
    pub thinking_trace: Option<String>,
    /// Present when verification ran: at supervised and above, or at the assisted
    /// tier when the agent declares check commands. `None` when verification is
    /// skipped (assisted tier with no declared checks).
    pub verification_report: Option<ConsolidatedVerificationReport>,
    /// True when an escalation was requested during this exchange but the hybrid
    /// cost ceiling kept the step local. The caller may surface a notice to the
    /// user. Stays `false` when hybrid routing is not configured.
    pub frontier_ceiling_reached: bool,
}

/// Consolidated result of the full post-run verification pass (checks + critic).
#[derive(Debug, Clone)]
pub struct ConsolidatedVerificationReport {
    /// True when all checks passed and the critic found no corrections.
    pub passed: bool,
    /// Failures from the programmed check commands.
    pub check_failures: Vec<CheckFailure>,
    /// Corrections proposed by the LLM critic.
    pub corrections: Vec<Correction>,
    /// Number of retry iterations performed (0 when verification passed first time).
    pub retry_iterations: u32,
}

/// Owned state threaded through the verification retry loop.
///
/// Carrying the conversation buffer and the latest response by value (rather
/// than borrowing them in the retry closure) keeps the closure's future free of
/// borrowed locals, so the spawned execute future stays `Send`.
struct RetryCarry {
    /// The running LLM message buffer, appended with each correction turn.
    messages: Vec<LlmChatMessage>,
    /// The most recent terminal response from the ReAct loop.
    last_response: ChatAgentResponse,
}

/// Maximum number of verification retry iterations per run.
///
/// Bounded to a small number so a failing check cannot loop indefinitely; each
/// retry still consumes from the shared [`StepBudget`].
const VERIFICATION_MAX_RETRIES: u32 = 2;

/// Number of consecutive tool failures before the ReAct loop emits an
/// escalation signal toward the frontier backend.
///
/// Conservative surface heuristic: it counts consecutive failed tool calls
/// (execution error, non-zero exit code, or operator refusal) and resets on the
/// first success. A richer signal based on a model confidence score is out of
/// scope for this iteration.
const ESCALATION_FAILURE_THRESHOLD: u32 = 3;

/// A [`CheckInvoker`] that never executes anything.
///
/// Chat Libre declares no manifest check commands, so the [`VerificationLoop`]
/// resolves an empty command list and never calls the invoker. This placeholder
/// satisfies the generic bound without spawning processes.
struct NoopCheckInvoker;

impl CheckInvoker for NoopCheckInvoker {
    async fn invoke_check(&self, _command: &str) -> Result<CheckOutcome, String> {
        Ok(CheckOutcome {
            exit_code: 0,
            stderr: String::new(),
        })
    }
}

/// Run the optional critic pass, treating an absent critic as a skipped success.
async fn run_critic_pass(
    critic: Option<&CriticPass>,
    objective: &str,
    output: &str,
) -> CriticReport {
    match critic {
        Some(critic) => critic.run(objective, output).await,
        None => CriticReport {
            passed: true,
            corrections: Vec::new(),
            skipped: true,
        },
    }
}

/// Build the correction message injected into the LLM context for a retry turn.
///
/// Emits an XML-like, English block listing the failed checks and the critic
/// corrections, followed by an instruction to address them. The format is meant
/// to be parsed by the model, not displayed to the user.
fn correction_message(check_failures: &[CheckFailure], corrections: &[Correction]) -> String {
    let mut msg = String::from("<verification_feedback>\n  <check_failures>\n");
    for failure in check_failures {
        msg.push_str(&format!(
            "    <check command=\"{}\" exit_code=\"{}\">{}</check>\n",
            failure.command, failure.exit_code, failure.stderr
        ));
    }
    msg.push_str("  </check_failures>\n  <corrections>\n");
    for correction in corrections {
        msg.push_str(&format!(
            "    <correction kind=\"{}\">\n      <description>{}</description>\n      \
             <suggestion>{}</suggestion>\n    </correction>\n",
            correction.kind, correction.description, correction.suggestion
        ));
    }
    msg.push_str("  </corrections>\n");
    msg.push_str(
        "  <instruction>Please address the issues above and provide a corrected \
         output.</instruction>\n",
    );
    msg.push_str("</verification_feedback>");
    msg
}

/// Run the post-loop verification (checks + critic) with a bounded retry.
///
/// Returns `None` when the tier is assisted or when no [`VerificationLoop`] is
/// configured. Otherwise it runs the checks and the optional critic on the
/// initial output; on failure it injects a correction and re-runs the loop via
/// `retry_fn`, up to `max_retries` times, stopping early when the budget is
/// exhausted. The budget is the hard ceiling: no retry starts once it is spent.
///
/// The retry state `state` is threaded by value through `retry_fn`, which always
/// returns it back alongside the new output (or an error). Owning the state
/// avoids capturing borrowed locals in the retry closure, which keeps the
/// returned future `Send` for `tokio::spawn`. The second tuple element is the
/// final state so the caller can recover the latest run's response.
#[allow(clippy::too_many_arguments)]
async fn run_verification_with_retry<I, S, F, Fut>(
    autonomy: &AutonomyLevel,
    verification: Option<&VerificationLoop>,
    critic: Option<&CriticPass>,
    invoker: &I,
    objective: &str,
    agent_output: &str,
    budget: &StepBudget,
    max_retries: u32,
    initial_state: S,
    mut retry_fn: F,
) -> (Option<ConsolidatedVerificationReport>, S)
where
    I: CheckInvoker,
    F: FnMut(S, String) -> Fut,
    Fut: std::future::Future<Output = (Result<String, ChatError>, S)>,
{
    let Some(verification) = verification else {
        return (None, initial_state);
    };
    // At the assisted tier, run only the deterministic checks the agent declared,
    // with no LLM critic and no retries: declared checks count by default at no
    // extra cost, and an agent that declares none is left untouched.
    let assisted = matches!(autonomy, AutonomyLevel::Assisted);
    if assisted && !verification.has_commands() {
        return (None, initial_state);
    }
    let critic = if assisted { None } else { critic };
    let max_retries = if assisted { 0 } else { max_retries };

    let mut state = initial_state;
    let mut current_output = agent_output.to_string();
    let mut retry_iterations = 0;

    let mut check_report = verification.run(invoker).await;
    let mut critic_report = run_critic_pass(critic, objective, &current_output).await;
    let mut passed = check_report.passed && critic_report.passed;

    while !passed && retry_iterations < max_retries && !budget.is_exhausted() {
        let message = correction_message(&check_report.failures, &critic_report.corrections);
        let (result, next_state) = retry_fn(state, message).await;
        state = next_state;
        match result {
            Ok(new_output) => current_output = new_output,
            Err(error) => {
                tracing::warn!(error = %error, "chat.verification.retry_failed");
                break;
            }
        }
        retry_iterations += 1;
        check_report = verification.run(invoker).await;
        critic_report = run_critic_pass(critic, objective, &current_output).await;
        passed = check_report.passed && critic_report.passed;
    }

    let report = ConsolidatedVerificationReport {
        passed,
        check_failures: check_report.failures,
        corrections: critic_report.corrections,
        retry_iterations,
    };
    (Some(report), state)
}

/// Dependencies required to construct a [`BuiltInChatAgent`].
pub struct BuiltInChatAgentDeps {
    pub llm_router: Arc<LlmRouter>,
    pub tool_registry: ToolRegistryHandle,
    pub tool_invoker: Arc<dyn ToolInvoker>,
    pub event_bus: EventBusSender,
    pub user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    pub a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Optional per-session todo store. When present, the `todo_write` built-in
    /// tool is advertised to the LLM and handled inside the ReAct loop.
    pub todo: Option<TodoHandle>,
}

/// Rust-native chat agent implementing a ReAct loop for Chat Libre mode.
///
/// Stateless, all mutable state is passed as parameters to [`execute`](Self::execute).
/// Tool execution is delegated to a [`ToolInvoker`].
pub struct BuiltInChatAgent {
    /// LLM router for completion calls.
    llm_router: Arc<LlmRouter>,
    /// Tool registry for resolving tool descriptors into LLM-compatible specs.
    tool_registry: ToolRegistryHandle,
    /// Tool invoker for actual tool execution.
    tool_invoker: Arc<dyn ToolInvoker>,
    /// Event bus for emitting chat lifecycle events.
    event_bus: EventBusSender,
    /// Optional user memory repository for injecting user context into the system prompt.
    user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Optional A2A invoker for discovering worker agent skills as virtual tools.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Context window manager: compacts `llm_messages` inside the ReAct loop
    /// when accumulated messages exceed the model's window threshold.
    context_manager: ContextManager,
    /// Optional handle to the `MetaLlmOrchestrator`, used to produce the
    /// `ToolCallRationale` narrated before each tool execution.
    /// Absent by default for backward compatibility; injected by the manager
    /// when the "Explain tool calls" main toggle is active.
    meta_handle: Option<MetaOrchestratorHandle>,
    /// Workspace directory injected into the system prompt so the LLM knows its
    /// effective working directory (project workspace or ~/.apollia/ for free chat).
    workspace_path: Option<std::path::PathBuf>,
    /// Aggregated MCP tool index for deferred mode.
    ///
    /// `Some` only when the session runs in deferred mode: `build_tool_specs`
    /// then injects the synthetic `tool_search` spec and omits the individual MCP
    /// schemas. `None` keeps the eager spec path unchanged.
    mcp_index: Option<Vec<ToolIndexSnapshot>>,
    /// Maximum `limit` advertised for the synthetic `tool_search` tool.
    tool_search_limit: usize,
    /// Optional per-session todo store. `None` disables the `todo_write` tool.
    todo: Option<TodoHandle>,
    /// Optional lifecycle hook executor shared across sessions. `None` means no
    /// hooks are configured: the ReAct loop behaves exactly as before, with no
    /// interception and zero overhead.
    hook_executor: Option<Arc<HookExecutor>>,
}

/// Mutable accumulators threaded through one ReAct turn's tool-call handling.
struct ReactAccumulators {
    all_tool_calls: Vec<ToolCallRecord>,
    newly_authorized: Vec<String>,
    authorized: HashSet<String>,
}

/// Owned/borrowed state needed to build the terminal [`ChatAgentResponse`]
/// (final text or stream-error path).
struct ResponseContext<'a> {
    acc: ReactAccumulators,
    total_usage: TokenUsage,
    session_id: &'a str,
    message_id: &'a str,
    run_id: &'a RunId,
    frontier_ceiling_reached: bool,
}

/// Borrowed context for processing a single tool call inside the ReAct loop.
struct ToolCallContext<'a> {
    session_id: &'a str,
    message_id: &'a str,
    call: &'a ToolCall,
    pending_approvals: &'a PendingChatApprovals,
}

/// Borrowed identifiers shared by every tool call in a single ReAct turn
/// (the per-call [`ToolCall`] is supplied separately while iterating).
#[derive(Clone, Copy)]
struct ToolCallContextIds<'a> {
    session_id: &'a str,
    message_id: &'a str,
    run_id: &'a RunId,
    pending_approvals: &'a PendingChatApprovals,
}

/// Borrowed read-only inputs for [`BuiltInChatAgent::record_tool_turn`]:
/// the raw LLM output, the parsed tool calls, the step budget, and the
/// per-turn identifiers. The mutable accumulators are passed separately.
struct RecordTurnInput<'a> {
    accumulated_text: &'a str,
    tool_calls: &'a [ToolCall],
    budget: &'a StepBudget,
    ids: ToolCallContextIds<'a>,
}

/// Borrowed identifiers locating a single tool call being executed
/// (session + message scope plus the call itself).
struct ToolExecTarget<'a> {
    session_id: &'a str,
    message_id: &'a str,
    call: &'a ToolCall,
}

/// Outcome of running the `PreToolUse` hooks over one turn's tool calls.
///
/// `calls` is the working set to execute: borrowed (no hook, no change) or owned
/// with any `Rewrite` applied. `denied[i]` carries the refusal reason when call
/// `i` was blocked; it is index-aligned with `calls`.
struct PreToolUseOutcome<'a> {
    calls: std::borrow::Cow<'a, [ToolCall]>,
    denied: Vec<Option<String>>,
}

impl BuiltInChatAgent {
    /// Create a new agent with the given dependencies.
    pub fn new(deps: BuiltInChatAgentDeps) -> Self {
        Self {
            llm_router: deps.llm_router,
            tool_registry: deps.tool_registry,
            tool_invoker: deps.tool_invoker,
            event_bus: deps.event_bus,
            user_memory: deps.user_memory,
            a2a_invoker: deps.a2a_invoker,
            context_manager: ContextManager::from_config(&ORIAConfig::default()),
            meta_handle: None,
            workspace_path: None,
            mcp_index: None,
            // Overwritten by `with_mcp_index` in deferred mode; unused otherwise.
            tool_search_limit: 20,
            todo: deps.todo,
            hook_executor: None,
        }
    }

    /// Configure the deferred MCP tool index for this agent.
    ///
    /// `Some(index)` switches `build_tool_specs` to the deferred path: it injects
    /// the synthetic `tool_search` spec (capped at `tool_search_limit`) and omits
    /// the individual MCP schemas. `None` keeps the eager path unchanged.
    pub fn with_mcp_index(
        mut self,
        mcp_index: Option<Vec<ToolIndexSnapshot>>,
        tool_search_limit: usize,
    ) -> Self {
        self.mcp_index = mcp_index;
        self.tool_search_limit = tool_search_limit;
        self
    }

    /// Set the workspace path for this agent (used in system prompt and bash CWD).
    pub fn with_workspace_path(mut self, path: Option<std::path::PathBuf>) -> Self {
        self.workspace_path = path;
        self
    }

    /// Attach a `MetaOrchestratorHandle` to generate `ToolCallRationale`s.
    /// No-op when `None`.
    pub fn with_meta_handle(mut self, handle: Option<MetaOrchestratorHandle>) -> Self {
        self.meta_handle = handle;
        self
    }

    /// Attach the shared lifecycle hook executor. No-op when `None`: the loop
    /// runs without any hook interception.
    pub fn with_hook_executor(mut self, executor: Option<Arc<HookExecutor>>) -> Self {
        self.hook_executor = executor;
        self
    }

    /// Build the effective system prompt for the given autonomy level.
    ///
    /// When `custom_prompt` is a non-empty string it is used as the base
    /// (preserving the behavior for agents with a personalized prompt).
    /// Otherwise the base is selected by tier: the assisted tier uses
    /// [`DEFAULT_SYSTEM_PROMPT`], every autonomous tier uses
    /// [`PERSEVERANCE_SYSTEM_PROMPT`].
    ///
    /// Then prepends the authoritative temporal/environment block at the **top**
    /// of the prompt so the LLM treats current date + time + timezone as ground
    /// truth, not as one fact among its priors. The user persona block is
    /// appended only when `inject_memory` is true and a memory repository is
    /// configured (memory stays at the agent's initiative, gated by the tier).
    pub fn build_system_prompt(
        &self,
        custom_prompt: Option<&str>,
        level: AutonomyLevel,
        inject_memory: bool,
    ) -> String {
        let base_prompt = match custom_prompt {
            Some(custom) if !custom.is_empty() => custom,
            _ => match level {
                AutonomyLevel::Assisted => DEFAULT_SYSTEM_PROMPT,
                AutonomyLevel::Supervised
                | AutonomyLevel::BoundedAutonomous
                | AutonomyLevel::LongAutonomous => PERSEVERANCE_SYSTEM_PROMPT,
            },
        };
        let mut prompt = apollia_core::temporal_context::prepend_temporal_context(base_prompt);

        if inject_memory {
            if let Some(ref repo_mutex) = self.user_memory {
                match repo_mutex.lock() {
                    Ok(repo) => match repo.recall_persona_brief(30) {
                        Ok(block) if !block.is_empty() => {
                            prompt.push_str(
                                "\n\n## User Persona\n\
                                 Follow the adaptation instructions below to personalize every \
                                 response. Do not repeat this information back to the user \
                                 unless asked.\n\n",
                            );
                            prompt.push_str(&block);
                        }
                        Ok(_) => {} // empty, nothing to inject
                        Err(e) => {
                            warn!(error = %e, "Failed to read user memory for injection, skipping");
                        }
                    },
                    Err(e) => {
                        warn!(error = %e, "User memory mutex poisoned, skipping injection");
                    }
                }
            }
        }

        prompt
    }

    /// Execute a complete exchange: user message, LLM stream, tool calls, response.
    ///
    /// Uses `LlmRouter.stream()` to produce tokens one by one, emitting a
    /// [`RuntimeEvent::ChatToken`] for each token received. The ReAct loop
    /// continues until the LLM produces a final text response (no tool calls)
    /// or the [`StepBudget`] is exhausted.
    ///
    /// The autonomy tier governs the prompt variant, memory injection, and the
    /// post-run verification. The `budget` is built by the manager via
    /// `StepBudget::from_capped`, so it is already the capped ceiling; this method
    /// never raises it. `level_config` carries the resolved tier flags
    /// (`inject_memory`, `run_verification`); when `None` the call behaves as the
    /// assisted tier (no memory injection, no verification).
    ///
    /// # Errors
    ///
    /// - [`ChatError::BudgetExhausted`] if the step budget is exceeded
    /// - [`ChatError::InternalError`] for LLM backend failures
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        session_id: &str,
        message_id: &str,
        run_id: &RunId,
        user_message: &str,
        history: &[ChatMessage],
        system_prompt: &str,
        available_tools: &[String],
        authorized_tools: &HashSet<String>,
        pending_approvals: &PendingChatApprovals,
        budget: &StepBudget,
        summary: Option<&str>,
        context_window_size: usize,
        autonomy: Option<&AutonomyLevel>,
        verification: Option<&VerificationLoop>,
        critic: Option<&CriticPass>,
        level_config: Option<&AutonomyLevelConfig>,
    ) -> Result<ChatAgentResponse, ChatError> {
        let custom_prompt = if system_prompt.is_empty() {
            None
        } else {
            Some(system_prompt)
        };
        let level = autonomy.copied().unwrap_or(AutonomyLevel::Assisted);
        let inject_memory = level_config.is_some_and(|c| c.inject_memory);
        let run_verification = level_config.is_some_and(|c| c.run_verification);

        // Auditable trace of the applied tier. The budget is already capped by
        // the runtime ceiling at construction (principle #7); this only records
        // the effective values.
        tracing::info!(
            autonomy_level = %level.as_str(),
            inject_memory,
            run_verification,
            max_steps = budget.max_steps,
            max_tool_calls = budget.max_tool_calls,
            wall_clock_secs = budget.wall_clock_limit.as_secs(),
            "chat.autonomy_tier.applied"
        );

        let effective_prompt = self.build_system_prompt(custom_prompt, level, inject_memory);

        let mut tool_specs = build_tool_specs(
            available_tools,
            &self.tool_registry,
            self.mcp_index.as_deref(),
            self.tool_search_limit,
        )
        .await;
        if let Some(ref a2a) = self.a2a_invoker {
            tool_specs.extend(generate_a2a_tool_specs(a2a).await);
        }
        // Advertise the todo_write built-in whenever a todo store is attached.
        if self.todo.is_some() {
            tool_specs.push(todo_write_spec());
        }
        info!(
            session_id = %session_id,
            available = available_tools.len(),
            resolved = tool_specs.len(),
            tool_names = ?tool_specs.iter().map(|s| &s.name).collect::<Vec<_>>(),
            "Chat ReAct loop: tool specs resolved"
        );
        let mut llm_messages = build_llm_messages(
            &effective_prompt,
            history,
            user_message,
            summary,
            context_window_size,
        );

        let ids = ToolCallContextIds {
            session_id,
            message_id,
            run_id,
            pending_approvals,
        };
        let first = self
            .run_react_loop(
                &mut llm_messages,
                &tool_specs,
                authorized_tools,
                budget,
                ids,
            )
            .await?;

        // Post-run verification with bounded retry, gated by the autonomy tier.
        // The verification loop and critic are injected by the manager; when the
        // tier does not request verification, or neither is configured, this is a
        // no-op and the first response is returned unchanged.
        let Some(level) = autonomy.filter(|_| run_verification) else {
            return Ok(first);
        };
        let invoker = NoopCheckInvoker;
        let initial_output = first.content.clone();
        let tool_specs_ref: &[ToolSpec] = &tool_specs;
        let carry = RetryCarry {
            messages: llm_messages,
            last_response: first,
        };
        let (report, carry) = run_verification_with_retry(
            level,
            verification,
            critic,
            &invoker,
            user_message,
            &initial_output,
            budget,
            VERIFICATION_MAX_RETRIES,
            carry,
            move |mut state: RetryCarry, correction: String| async move {
                state.messages.push(LlmChatMessage::user(correction));
                match self
                    .run_react_loop(
                        &mut state.messages,
                        tool_specs_ref,
                        authorized_tools,
                        budget,
                        ids,
                    )
                    .await
                {
                    Ok(next) => {
                        let output = next.content.clone();
                        state.last_response = next;
                        (Ok(output), state)
                    }
                    Err(error) => (Err(error), state),
                }
            },
        )
        .await;
        let mut response = carry.last_response;
        response.verification_report = report;
        Ok(response)
    }

    /// Run the ReAct loop to completion and return the terminal response.
    ///
    /// Drives the stream/tool-call cycle until the LLM produces a final text
    /// response, the stream errors, or the [`StepBudget`] is exhausted. The
    /// caller owns `llm_messages`, so it can append a correction turn and call
    /// this again for a bounded verification retry.
    ///
    /// # Errors
    ///
    /// - [`ChatError::BudgetExhausted`] if the step budget is exceeded.
    /// - [`ChatError::InternalError`] for LLM backend failures.
    async fn run_react_loop(
        &self,
        llm_messages: &mut Vec<LlmChatMessage>,
        tool_specs: &[ToolSpec],
        authorized_tools: &HashSet<String>,
        budget: &StepBudget,
        ids: ToolCallContextIds<'_>,
    ) -> Result<ChatAgentResponse, ChatError> {
        let session_id = ids.session_id;
        let message_id = ids.message_id;
        let run_id = ids.run_id;
        let total_usage = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
            ..Default::default()
        };
        let mut acc = ReactAccumulators {
            all_tool_calls: Vec::new(),
            newly_authorized: Vec::new(),
            authorized: authorized_tools.clone(),
        };
        let obs = ObservabilityConfig::default();
        let mut reasoning_fragments: Vec<String> = Vec::new();
        // Conservative escalation heuristic: count consecutive failed tool calls
        // and, past the threshold, ask the router to escalate the next LLM call to
        // the frontier backend. Reset to 0 on the first successful tool call.
        let mut consecutive_tool_failures: u32 = 0;
        // Set when an escalation was requested but the cost ceiling kept the step
        // local. Surfaced on the terminal response so the caller can warn.
        let mut frontier_ceiling_reached = false;

        loop {
            // Step safeguard: budget check before every LLM call
            if budget.is_exhausted() {
                let reason = budget
                    .exhaustion_reason()
                    .unwrap_or_else(|| "unknown".into());
                tracing::warn!(
                    %reason,
                    session_id = %session_id,
                    "chat step budget exhausted"
                );
                return Err(ChatError::BudgetExhausted);
            }
            budget.increment_steps();

            // Compact context if messages approach the model's context limit.
            // When a compaction drops the history, re-inject the todo list so
            // the agent never loses track of pending work (principle: memory at
            // the agent's initiative; nothing is injected when the list is empty).
            let was_compacted = self.maybe_compact_context(llm_messages, session_id).await;
            if was_compacted {
                if let Some(todo) = self.todo.as_ref() {
                    Self::inject_todo_after_compaction(todo, session_id, llm_messages).await;
                }
            }

            let request = CompletionRequest {
                messages: llm_messages.clone(),
                tools: tool_specs.to_vec(),
                ..Default::default()
            };

            // Emit ChatResponseStarted before the first token
            let _ = self.event_bus.send(RuntimeEvent::ChatResponseStarted {
                session_id: session_id.to_string(),
                message_id: message_id.to_string(),
                run_id: Some(run_id.clone()),
            });

            // Derive the escalation signal from the consecutive-failure counter.
            // Only when it escalates do we route through the frontier policy; the
            // non-escalated path keeps the default backend, so behavior without
            // hybrid routing or below threshold is byte-for-byte unchanged. Routing
            // and ceiling policy stay in `LlmRouter`; the loop only detects and
            // observes. The escalated backend is streamed directly (the router map
            // owns it), so this does not depend on a name round-trip.
            let signal = if consecutive_tool_failures >= ESCALATION_FAILURE_THRESHOLD {
                EscalationSignal::RepeatedStepFailure {
                    consecutive_failures: consecutive_tool_failures,
                }
            } else {
                EscalationSignal::None
            };
            let stream_result = if signal.is_escalation() {
                let backend = self
                    .llm_router
                    .route_with_escalation(signal, LlmRoutingLevel::Precise);
                if self.llm_router.is_ceiling_reached() {
                    frontier_ceiling_reached = true;
                    // Hard stop: when configured, stop the run cleanly instead of
                    // continuing on the degraded local backend. The router owns the
                    // threshold decision; the loop only applies the configured action.
                    if self.llm_router.ceiling_action() == CeilingAction::HardStop {
                        let cost_usd = self.llm_router.session_cost_usd();
                        let ceiling_usd = self.llm_router.cost_ceiling_usd().unwrap_or(0.0);
                        let _ = self.event_bus.send(RuntimeEvent::CostCeilingReached {
                            session_id: session_id.to_string(),
                            cost_usd,
                            ceiling_usd,
                        });
                        tracing::warn!(
                            session_id = %session_id,
                            cost_usd,
                            ceiling_usd,
                            ceiling_action = "hard_stop",
                            "chat.cost_ceiling.hard_stop"
                        );
                        return Err(ChatError::CostCeilingExceeded {
                            cost_usd,
                            ceiling_usd,
                        });
                    }
                }
                tracing::info!(
                    consecutive_failures = consecutive_tool_failures,
                    backend = %backend.backend_name(),
                    ceiling_reached = frontier_ceiling_reached,
                    session_id = %session_id,
                    "chat.escalation.requested"
                );
                backend.stream(request).await
            } else {
                self.llm_router
                    .stream_with_observability(None, request, &obs)
                    .await
            };
            let stream = stream_result.map_err(|e| ChatError::InternalError(e.to_string()))?;

            // Consume stream, emit ChatToken per token, accumulate text
            let mut accumulated_text = String::new();
            let stream_result = self
                .consume_stream(stream, session_id, message_id, &mut accumulated_text)
                .await;

            match stream_result {
                Ok(tool_calls) if tool_calls.is_empty() => {
                    return Ok(self.finalize_text_response(
                        &accumulated_text,
                        &mut reasoning_fragments,
                        ResponseContext {
                            acc,
                            total_usage,
                            session_id,
                            message_id,
                            run_id,
                            frontier_ceiling_reached,
                        },
                    ));
                }
                Ok(tool_calls) => {
                    self.record_tool_turn(
                        RecordTurnInput {
                            accumulated_text: &accumulated_text,
                            tool_calls: &tool_calls,
                            budget,
                            ids,
                        },
                        &mut reasoning_fragments,
                        llm_messages,
                        &mut acc,
                        &mut consecutive_tool_failures,
                    )
                    .await;
                }
                Err(err) => {
                    return Ok(self.stream_error_response(
                        err,
                        accumulated_text,
                        ResponseContext {
                            acc,
                            total_usage,
                            session_id,
                            message_id,
                            run_id,
                            frontier_ceiling_reached,
                        },
                    ));
                }
            }
        }
    }

    /// Build the final [`ChatAgentResponse`] when the LLM produced no tool calls.
    ///
    /// Combines the accumulated reasoning fragments with the final thinking
    /// trace and emits [`RuntimeEvent::ChatResponseCompleted`].
    fn finalize_text_response(
        &self,
        accumulated_text: &str,
        reasoning_fragments: &mut Vec<String>,
        ctx: ResponseContext<'_>,
    ) -> ChatAgentResponse {
        let ResponseContext {
            acc,
            total_usage,
            session_id,
            message_id,
            run_id,
            frontier_ceiling_reached,
        } = ctx;
        // Extract thinking trace before stripping.
        let final_thinking = Self::extract_think_blocks(accumulated_text);
        let clean = Self::strip_think_blocks(accumulated_text);
        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: clean.clone(),
            run_id: Some(run_id.clone()),
        });

        // Combine accumulated reasoning fragments with final thinking.
        if let Some(ft) = &final_thinking {
            reasoning_fragments.push(ft.clone());
        }
        let thinking_trace = if reasoning_fragments.is_empty() {
            None
        } else {
            Some(reasoning_fragments.join("\n\n---\n\n"))
        };

        tracing::info!(
            fragment_count = reasoning_fragments.len(),
            has_trace = thinking_trace.is_some(),
            trace_len = thinking_trace.as_ref().map(|t| t.len()).unwrap_or(0),
            session_id = %session_id,
            "ReAct complete: thinking_trace summary"
        );

        ChatAgentResponse {
            content: clean,
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace,
            verification_report: None,
            frontier_ceiling_reached,
        }
    }

    /// Record one ReAct turn that produced tool calls: capture reasoning,
    /// append the assistant message, and dispatch each tool call.
    ///
    /// Updates `consecutive_tool_failures` per call: incremented on a failed
    /// call (execution error, non-zero exit code, or operator refusal), reset to
    /// 0 on the first success, so the loop can derive an escalation signal.
    async fn record_tool_turn(
        &self,
        input: RecordTurnInput<'_>,
        reasoning_fragments: &mut Vec<String>,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
        consecutive_tool_failures: &mut u32,
    ) {
        let RecordTurnInput {
            accumulated_text,
            tool_calls,
            budget,
            ids,
        } = input;
        // Capture reasoning text emitted before tool calls.
        let clean_reasoning = Self::strip_think_blocks(accumulated_text);
        let reasoning_with_think = Self::extract_think_blocks(accumulated_text);
        let reasoning_text = reasoning_with_think.unwrap_or_else(|| clean_reasoning.clone());
        tracing::info!(
            accumulated_len = accumulated_text.len(),
            reasoning_len = reasoning_text.trim().len(),
            tool_count = tool_calls.len(),
            session_id = %ids.session_id,
            "ReAct turn: captured reasoning before tool calls"
        );
        if !reasoning_text.trim().is_empty() {
            reasoning_fragments.push(reasoning_text.trim().to_string());
        }

        // Strip think blocks before re-injecting into the LLM context
        // so reasoning tokens don't pollute future turns.
        let clean_for_context = clean_reasoning;
        llm_messages.push(LlmChatMessage::assistant_with_calls(
            &clean_for_context,
            tool_calls,
        ));

        let session_id = ids.session_id;
        let message_id = ids.message_id;

        // PreToolUse hooks (blocking): resolve a decision per call before any
        // tool runs, so a `deny` truly prevents the invocation, including the
        // read-only calls that would otherwise execute in the parallel phase
        // below. `effective_calls` carries any rewritten arguments; `denied[i]`
        // holds the refusal reason when call `i` was blocked. With no hook
        // configured this is a borrow of the original calls with no denials, so
        // the loop behaves exactly as before.
        let pre = self.apply_pre_tool_use(tool_calls, session_id).await;
        let effective_calls: &[ToolCall] = pre.calls.as_ref();
        let denied = &pre.denied;

        // Determine read-only status for each call via the tool registry. A call
        // runs concurrently only when its tool is read-only AND already
        // authorized: execute_tool_call then touches neither llm_messages nor acc,
        // so the slow invocations overlap while results are applied in order.
        // Unknown tools (absent from the registry, e.g. hardcoded-false MCP specs)
        // are treated as write, the conservative default.
        let mut read_only: Vec<bool> = Vec::with_capacity(effective_calls.len());
        for call in effective_calls.iter() {
            let ro = self
                .tool_registry
                .describe(&call.name)
                .await
                .map(|d| d.is_read_only)
                .unwrap_or(false);
            read_only.push(ro);
        }

        // Phase A: invoke the parallel-safe calls concurrently, keyed by index.
        // Denied calls never run, even when read-only.
        let mut precomputed: std::collections::HashMap<usize, (ToolCallRecord, String, bool)> = {
            use futures::stream::{self, StreamExt};
            let parallel = (0..effective_calls.len())
                .filter(|&i| {
                    denied[i].is_none()
                        && read_only[i]
                        && acc.authorized.contains(&effective_calls[i].name)
                })
                .map(|i| async move {
                    let outcome = self
                        .execute_tool_call(session_id, message_id, &effective_calls[i])
                        .await;
                    (i, outcome)
                });
            stream::iter(parallel)
                .buffered(MAX_CONCURRENT_READONLY_TOOL_CALLS)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect()
        };

        // Phase B: apply every call in original order. Parallel-safe calls reuse
        // their precomputed result; everything else (write tools, read-only calls
        // awaiting HITL approval) goes through the sequential path.
        for (i, call) in effective_calls.iter().enumerate() {
            budget.increment_tool_calls();
            // PreToolUse deny: the tool is not invoked. Inject a synthetic tool
            // result so the model can react to the refusal on its next turn, and
            // do not count it as a tool failure (a deny is a policy decision, not
            // an execution failure).
            if let Some(reason) = &denied[i] {
                let synthetic = format!("tool denied: {reason}");
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &synthetic));
                acc.all_tool_calls.push(ToolCallRecord {
                    tool_name: call.name.clone(),
                    input: call.arguments.clone(),
                    output: Some(synthetic),
                    status: ToolCallStatus::Refused,
                    rationale: None,
                    retry_attempts: Vec::new(),
                });
                tracing::warn!(
                    tool_name = %call.name,
                    decision = "deny",
                    reason = %reason,
                    session_id = %session_id,
                    "hook.pretooluse.deny: tool call blocked"
                );
                continue;
            }
            let failed = match (call.name.as_str(), self.todo.as_ref()) {
                // todo_write is a safe built-in handled in-loop: it never goes
                // through the registry, the parallel partition, or HITL approval.
                (TODO_WRITE_TOOL_NAME, Some(todo)) => {
                    Self::handle_todo_write(todo, session_id, call, llm_messages, acc).await
                }
                _ => match precomputed.remove(&i) {
                    Some((record, tool_result, success)) => {
                        llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                        acc.all_tool_calls.push(record);
                        !success
                    }
                    None => {
                        self.process_tool_call(
                            ToolCallContext {
                                session_id,
                                message_id,
                                call,
                                pending_approvals: ids.pending_approvals,
                            },
                            llm_messages,
                            acc,
                        )
                        .await
                    }
                },
            };
            *consecutive_tool_failures = next_failure_count(*consecutive_tool_failures, failed);
        }
    }

    /// Run the blocking `PreToolUse` hooks over every call in a turn.
    ///
    /// Returns the working set to execute (with any `Rewrite` applied) plus a
    /// per-call refusal reason. When no hook executor is attached, or no
    /// `PreToolUse` handler is registered, this borrows the original calls and
    /// reports no denials, so the loop incurs no extra work. Decisions are
    /// traced with structured fields: `allow` at debug, `rewrite` at info; the
    /// `deny` warn is emitted at the blocking site in the loop.
    async fn apply_pre_tool_use<'a>(
        &self,
        tool_calls: &'a [ToolCall],
        session_id: &str,
    ) -> PreToolUseOutcome<'a> {
        let no_op = || PreToolUseOutcome {
            calls: std::borrow::Cow::Borrowed(tool_calls),
            denied: vec![None; tool_calls.len()],
        };
        let Some(executor) = self.hook_executor.as_ref() else {
            return no_op();
        };
        if executor
            .registry()
            .handlers_for(apollia_core::HookEventKind::PreToolUse)
            .is_empty()
        {
            return no_op();
        }

        let mut calls = tool_calls.to_vec();
        let mut denied: Vec<Option<String>> = vec![None; tool_calls.len()];
        for (i, call) in tool_calls.iter().enumerate() {
            match executor
                .run_pre_tool_use(&call.name, &call.arguments, session_id)
                .await
            {
                HookDecision::Allow => {
                    tracing::debug!(
                        tool_name = %call.name,
                        decision = "allow",
                        session_id = %session_id,
                        "hook.pretooluse.decision"
                    );
                }
                HookDecision::Rewrite { arguments } => {
                    tracing::info!(
                        tool_name = %call.name,
                        decision = "rewrite",
                        original_args = %truncate_preview(
                            &serde_json::to_string(&call.arguments).unwrap_or_default()
                        ),
                        rewritten_args = %truncate_preview(
                            &serde_json::to_string(&arguments).unwrap_or_default()
                        ),
                        session_id = %session_id,
                        "hook.pretooluse.decision"
                    );
                    calls[i].arguments = arguments;
                }
                HookDecision::Deny { reason } => {
                    denied[i] = Some(reason);
                }
            }
        }
        PreToolUseOutcome {
            calls: std::borrow::Cow::Owned(calls),
            denied,
        }
    }

    /// Run the `todo_write` built-in tool inside the ReAct loop.
    ///
    /// Persists the agent-provided list via the [`TodoHandle`] and injects the
    /// JSON result as the tool message. Returns `true` when the write failed
    /// (invariant violation or malformed payload) so the loop counts it toward
    /// escalation; the loop itself never stops on a todo error.
    async fn handle_todo_write(
        todo: &TodoHandle,
        session_id: &str,
        call: &ToolCall,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) -> bool {
        let result = run_todo_write(todo, session_id, &call.arguments).await;
        let item_count = result.count.unwrap_or(0);
        tracing::info!(
            session_id = %session_id,
            item_count,
            ok = result.ok,
            "chat.todo_write.applied"
        );
        let tool_result = serde_json::to_string(&result)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"todo result serialization failed"}"#.to_string());
        llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
        acc.all_tool_calls.push(ToolCallRecord {
            tool_name: call.name.clone(),
            input: call.arguments.clone(),
            output: Some(tool_result),
            status: ToolCallStatus::Executed,
            rationale: None,
            retry_attempts: Vec::new(),
        });
        !result.ok
    }

    /// Build the partial [`ChatAgentResponse`] returned when the stream was
    /// interrupted, emitting [`RuntimeEvent::ChatError`] and the completion event.
    fn stream_error_response(
        &self,
        err: String,
        accumulated_text: String,
        ctx: ResponseContext<'_>,
    ) -> ChatAgentResponse {
        let ResponseContext {
            acc,
            total_usage,
            session_id,
            message_id,
            run_id,
            frontier_ceiling_reached,
        } = ctx;
        // Stream interrupted: emit ChatError, return partial content
        let _ = self.event_bus.send(RuntimeEvent::ChatError {
            session_id: session_id.to_string(),
            message_id: Some(message_id.to_string()),
            error: err.clone(),
        });

        // Return partial content so the caller can save what was received
        let content = if accumulated_text.is_empty() {
            format!("[erreur streaming : {err}]")
        } else {
            accumulated_text
        };

        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: content.clone(),
            run_id: Some(run_id.clone()),
        });

        ChatAgentResponse {
            content,
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace: None,
            verification_report: None,
            frontier_ceiling_reached,
        }
    }

    /// Compact the LLM message buffer in place when it approaches the context
    /// limit, emitting [`RuntimeEvent::ContextCompacted`] on success.
    ///
    /// Returns `true` when a compaction actually occurred, so the caller can
    /// re-inject any state (such as the todo list) that the truncation dropped.
    async fn maybe_compact_context(
        &self,
        llm_messages: &mut Vec<LlmChatMessage>,
        session_id: &str,
    ) -> bool {
        let (compacted, was_compacted) = self
            .context_manager
            .maybe_compact(llm_messages, &self.llm_router)
            .await;
        if !was_compacted {
            return false;
        }
        let summary_chars = compacted
            .get(1)
            .map(apollia_oria::context_manager::message_char_len)
            .unwrap_or(0);
        let original_messages = llm_messages.len();
        *llm_messages = compacted;
        tracing::info!(
            summary_chars,
            original_messages,
            session_id = %session_id,
            "ReAct context compacted before LLM call"
        );
        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::ContextCompacted {
                summary_chars,
                original_messages,
            });
        true
    }

    /// Re-inject the current todo list as a reminder message after a context
    /// compaction dropped the conversation history.
    ///
    /// Called only when a compaction actually occurred and a todo store is
    /// attached. Reads the list through the [`TodoHandle`] and, when non-empty,
    /// appends a single user-role reminder message enumerating each item with
    /// its status. The agent never loses track of pending work even when the
    /// history is truncated. Errors are logged and swallowed: the loop continues
    /// without the reminder rather than failing.
    ///
    /// The Markdown rendering is intentionally simple here; a structured
    /// JSON or XML format may replace it in a later iteration.
    async fn inject_todo_after_compaction(
        todo: &TodoHandle,
        session_id: &str,
        messages: &mut Vec<LlmChatMessage>,
    ) {
        let items = match todo.get_items(session_id).await {
            Ok(items) => items,
            Err(e) => {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "todo.reinject.failed"
                );
                return;
            }
        };
        if items.is_empty() {
            return;
        }
        let mut reminder = String::from(TODO_REMINDER_PREFIX);
        for item in &items {
            reminder.push_str(&format!("\n- [{}] {}", item.status.as_str(), item.content));
        }
        messages.push(LlmChatMessage::user(reminder));
        tracing::info!(
            session_id = %session_id,
            todo_count = items.len(),
            compaction_triggered = true,
            "chat.todo.reinjected_after_compaction"
        );
    }

    /// Consume a token stream, emitting [`RuntimeEvent::ChatToken`] for each token
    /// and accumulating text in `accumulated_text`.
    ///
    /// Returns the list of tool calls found in the stream (empty if none).
    /// On stream error, returns the error message; the caller can use the
    /// partially accumulated text.
    async fn consume_stream(
        &self,
        mut stream: std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<StreamChunk, apollia_llm::LlmError>> + Send>,
        >,
        session_id: &str,
        message_id: &str,
        accumulated_text: &mut String,
    ) -> Result<Vec<ToolCall>, String> {
        let mut tool_calls = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(StreamChunk::Text(token)) => {
                    // Emit ChatToken and accumulate
                    let _ = self.event_bus.send(RuntimeEvent::ChatToken {
                        session_id: session_id.to_string(),
                        message_id: message_id.to_string(),
                        token: token.clone(),
                    });
                    accumulated_text.push_str(&token);
                }
                Ok(StreamChunk::ToolCall(call)) => {
                    // Tool call detected in stream
                    tool_calls.push(call);
                }
                Err(e) => {
                    // Stream interrupted
                    warn!(
                        session_id = %session_id,
                        error = %e,
                        "LLM stream interrupted"
                    );
                    return Err(e.to_string());
                }
            }
        }

        // Tool calls now arrive as structured `StreamChunk::ToolCall` from every
        // backend: cloud providers emit them natively, and the local runner
        // decodes them through the GGUF's own chat-template parser (common_chat)
        // before returning. No text-level `<tool_call>` scraping is needed.
        Ok(tool_calls)
    }

    /// Extracts the content of `<think>...</think>` blocks from reasoning models.
    ///
    /// Returns the concatenated thinking text if any blocks are found, or `None`.
    /// Called before [`strip_think_blocks`] to capture reasoning for metadata.
    fn extract_think_blocks(text: &str) -> Option<String> {
        let tag_open = "<think>";
        let tag_close = "</think>";
        let mut blocks = Vec::new();
        let mut cursor = 0;

        while let Some(start) = text[cursor..].find(tag_open) {
            let after_open = cursor + start + tag_open.len();
            if let Some(end) = text[after_open..].find(tag_close) {
                let block = text[after_open..after_open + end].trim();
                if !block.is_empty() {
                    blocks.push(block.to_string());
                }
                cursor = after_open + end + tag_close.len();
            } else {
                // Unclosed <think> tag, capture remaining as partial thinking.
                let block = text[after_open..].trim();
                if !block.is_empty() {
                    blocks.push(block.to_string());
                }
                break;
            }
        }

        if blocks.is_empty() {
            None
        } else {
            Some(blocks.join("\n\n"))
        }
    }

    /// Strips `<think>...</think>` blocks emitted by reasoning models (e.g. Qwen3).
    ///
    /// Called before re-injecting the assistant's turn into `llm_messages` and before
    /// returning the final content to the user. This prevents thinking tokens from
    /// polluting the context window across turns.
    fn strip_think_blocks(text: &str) -> String {
        let tag_open = "<think>";
        let tag_close = "</think>";
        let mut result = String::with_capacity(text.len());
        let mut cursor = 0;

        while let Some(start) = text[cursor..].find(tag_open) {
            result.push_str(&text[cursor..cursor + start]);
            let after_open = cursor + start + tag_open.len();
            if let Some(end) = text[after_open..].find(tag_close) {
                cursor = after_open + end + tag_close.len();
            } else {
                // Unclosed <think> tag, discard everything after it.
                break;
            }
        }
        result.push_str(&text[cursor..]);
        result.trim().to_string()
    }

    /// Build the [`ErrorAnalysis`] attached to a `ChatToolCallCompleted` event.
    ///
    /// On failure, classifies the raw output via [`crate::analyzers::classify_tool_error`]
    /// and, if the user has opted in, enriches the message via the meta-LLM.
    /// On success, runs only the hallucination heuristic (zero-cost) and
    /// returns `Some(...)` only when the heuristic flags the output.
    async fn build_error_analysis(
        &self,
        session_id: &str,
        tool_name: &str,
        output: &str,
        success: bool,
    ) -> Option<apollia_core::ErrorAnalysis> {
        use crate::analyzers::hallucination_detector::analysis_from_report;
        use crate::analyzers::{classify_tool_error, detect_hallucination, enrich_with_llm};

        if !success {
            let analysis = classify_tool_error(output);
            let analysis = if let Some(handle) = self.meta_handle.as_ref() {
                let context = format!("tool={tool_name}");
                enrich_with_llm(handle, analysis, &context, session_id).await
            } else {
                analysis
            };
            return Some(analysis);
        }

        // Success path: only flag if the heuristic fires (no schema
        // validators are wired up yet; those come with the per-tool registry).
        let report = detect_hallucination(output, None);
        if report.is_suspect() {
            Some(analysis_from_report(&report, output))
        } else {
            None
        }
    }

    /// Execute a single tool call via the [`ToolInvoker`], emitting events.
    /// Process one tool call: run it directly when authorized, otherwise go
    /// through the HITL approval flow. Mutates `llm_messages` and `acc`.
    ///
    /// Returns `true` when the call failed (execution error, non-zero exit code,
    /// or operator refusal), so the loop can update its escalation counter.
    async fn process_tool_call(
        &self,
        ctx: ToolCallContext<'_>,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) -> bool {
        let ToolCallContext {
            session_id,
            message_id,
            call,
            pending_approvals,
        } = ctx;

        if acc.authorized.contains(&call.name) {
            let (record, tool_result, success) =
                self.execute_tool_call(session_id, message_id, call).await;
            llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
            acc.all_tool_calls.push(record);
            return !success;
        }

        // HITL approval
        let key = format!("{session_id}::{message_id}::{}", call.name);
        let input_preview =
            truncate_preview(&serde_json::to_string(&call.arguments).unwrap_or_default());

        let _ = self.event_bus.send(RuntimeEvent::ChatApprovalRequired {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: call.name.clone(),
            prompt: format!(
                "L'outil '{}' demande à être exécuté avec: {}",
                call.name, input_preview
            ),
        });

        let rx = pending_approvals.register(key.clone());
        pending_approvals.start_timeout(ApprovalTimeoutParams {
            key,
            duration: CHAT_APPROVAL_TIMEOUT,
            event_bus: self.event_bus.clone(),
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: call.name.clone(),
        });
        let decision = rx.await.unwrap_or(ToolDecision::refuse());

        self.apply_tool_decision(
            ToolExecTarget {
                session_id,
                message_id,
                call,
            },
            decision,
            llm_messages,
            acc,
        )
        .await
    }

    /// Apply the operator's HITL decision for an unauthorized tool call.
    ///
    /// Returns `true` when the call failed: an execution failure on accept, or a
    /// refusal (the operator declined, which the loop counts toward escalation).
    async fn apply_tool_decision(
        &self,
        target: ToolExecTarget<'_>,
        decision: ToolDecision,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) -> bool {
        let ToolExecTarget {
            session_id,
            message_id,
            call,
        } = target;
        match decision {
            ToolDecision::Accept => {
                let (record, tool_result, success) =
                    self.execute_tool_call(session_id, message_id, call).await;
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                acc.all_tool_calls.push(record);
                !success
            }
            ToolDecision::AlwaysAccept { .. } => {
                acc.authorized.insert(call.name.clone());
                acc.newly_authorized.push(call.name.clone());
                let (record, tool_result, success) =
                    self.execute_tool_call(session_id, message_id, call).await;
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                acc.all_tool_calls.push(record);
                !success
            }
            ToolDecision::Refuse { reason } => {
                // The reason carries the operator's intent (e.g. "wrong
                // directory"), surface it to the LLM so it can correct course
                // on the next iteration instead of retrying blind.
                let refusal = match &reason {
                    Some(r) => format!("Outil refusé par l'utilisateur. Raison : {r}"),
                    None => "Outil refusé par l'utilisateur".to_string(),
                };
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &refusal));
                acc.all_tool_calls.push(ToolCallRecord {
                    tool_name: call.name.clone(),
                    input: call.arguments.clone(),
                    output: Some(refusal),
                    status: ToolCallStatus::Refused,
                    rationale: None,
                    retry_attempts: Vec::new(),
                });
                true
            }
        }
    }

    async fn execute_tool_call(
        &self,
        session_id: &str,
        message_id: &str,
        call: &apollia_llm::types::ToolCall,
    ) -> (ToolCallRecord, String, bool) {
        let input_preview =
            truncate_preview(&serde_json::to_string(&call.arguments).unwrap_or_default());

        // Generate the opt-in rationale *before* execution so the UI can
        // surface it immediately. Falls back to `None` when the meta handle
        // is absent, the routine is disabled, the budget is exhausted, or
        // the call fails / times out (see MetaOrchestratorHandle docs).
        let rationale = if let Some(handle) = self.meta_handle.as_ref() {
            handle
                .generate_tool_call_rationale(
                    &call.name,
                    &call.arguments,
                    "",
                    session_id.to_string(),
                )
                .await
        } else {
            None
        };

        let _ = self.event_bus.send(RuntimeEvent::ChatToolCallStarted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: call.name.clone(),
            input_preview,
            rationale: rationale.clone(),
        });

        let result = self.tool_invoker.invoke(&call.name, &call.arguments).await;
        let (output, success) = match result {
            Ok(s) => {
                // Detect tool-reported failures (e.g. bash_executor with exit_code != 0)
                let tool_failed = serde_json::from_str::<serde_json::Value>(&s)
                    .ok()
                    .and_then(|v| v.get("exit_code")?.as_i64())
                    .is_some_and(|code| code != 0);
                (s, !tool_failed)
            }
            Err(e) => {
                warn!(tool = %call.name, error = %e, "Tool call failed");
                (format!("tool error: {e}"), false)
            }
        };

        let output_preview = truncate_preview(&output);

        // Static analysis (always-on): run the static error classifier (on
        // failure) and the hallucination heuristic (on every output).
        // Opt-in: when the
        // analysis falls back to `Unknown`, ask the meta-LLM to humanise
        // the message via `MetaRoutine::GenerateErrorExplanation`.
        let analysis = self
            .build_error_analysis(session_id, &call.name, &output, success)
            .await;
        let _ = self.event_bus.send(RuntimeEvent::ChatToolCallCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: call.name.clone(),
            success,
            output_preview: Some(output_preview),
            analysis,
        });

        let record = ToolCallRecord {
            tool_name: call.name.clone(),
            input: call.arguments.clone(),
            output: Some(output.clone()),
            status: ToolCallStatus::Executed,
            rationale,
            retry_attempts: Vec::new(),
        };

        // Truncate output for LLM context to avoid flooding the context window.
        // The full output is preserved in the ToolCallRecord for history/UI.
        let llm_output = truncate_tool_output(&output);

        (record, llm_output, success)
    }
}

/// Build LLM messages from system prompt, chat history, and current user message.
///
/// Applies a sliding window over history: only the last `context_window_size`
/// messages are included. When a conversation summary is available, it is
/// injected as a system message between the system prompt and the windowed
/// history to preserve context from older messages.
///
/// Message order: system prompt, [summary], windowed history, user message.
fn build_llm_messages(
    system_prompt: &str,
    history: &[ChatMessage],
    user_message: &str,
    summary: Option<&str>,
    context_window_size: usize,
) -> Vec<LlmChatMessage> {
    let window_size = if context_window_size == 0 {
        DEFAULT_CONTEXT_WINDOW_SIZE
    } else {
        context_window_size
    };

    let windowed_history = if history.len() > window_size {
        let start = history.len() - window_size;
        &history[start..]
    } else {
        history
    };

    let mut messages = Vec::with_capacity(windowed_history.len() + 4);

    messages.push(LlmChatMessage::system(system_prompt));

    if let Some(summary_text) = summary {
        if !summary_text.is_empty() {
            messages.push(LlmChatMessage::system(format!(
                "Previous context summary:\n{summary_text}"
            )));
        }
    }

    for msg in windowed_history {
        match msg.role {
            ChatRole::User => messages.push(LlmChatMessage::user(&msg.content)),
            ChatRole::Assistant => messages.push(LlmChatMessage::assistant(&msg.content)),
            ChatRole::Tool => {
                let call_id = msg.tool_name.as_deref().unwrap_or("unknown");
                messages.push(LlmChatMessage::tool_result(call_id, &msg.content));
            }
            ChatRole::System => {
                // System messages from history are skipped, we already have the prompt
            }
        }
    }

    messages.push(LlmChatMessage::user(user_message));
    messages
}

/// Extract the hostname from a URL string for use as an http_fetch allowlist entry.
///
/// Handles common shapes: `https://host/path`, `http://host:port/path`.
/// Returns `None` for malformed URLs or those without a hostname.
fn extract_hostname(url: &str) -> Option<String> {
    let rest = url.find("://").map(|i| &url[i + 3..])?;
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host_and_port = &rest[..host_end];
    // Strip port if present (skip IPv6 brackets)
    let host = if !host_and_port.starts_with('[') {
        if let Some(colon) = host_and_port.rfind(':') {
            if host_and_port[colon + 1..]
                .chars()
                .all(|c| c.is_ascii_digit())
            {
                &host_and_port[..colon]
            } else {
                host_and_port
            }
        } else {
            host_and_port
        }
    } else {
        host_and_port
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Convert available tool names to LLM-compatible [`ToolSpec`]s via the registry.
///
/// In eager mode (`mcp_index` is `None`) this resolves every entry in
/// `available_tools` from the registry, exactly as before this change.
///
/// In deferred mode (`mcp_index` is `Some`) the individual `mcp:` names are
/// skipped and a single synthetic `tool_search` spec is appended instead, so the
/// LLM discovers MCP tools by intent rather than receiving every schema up front.
/// `tool_search_limit` is the upper bound advertised in that spec's description.
async fn build_tool_specs(
    available_tools: &[String],
    tool_registry: &ToolRegistryHandle,
    mcp_index: Option<&[ToolIndexSnapshot]>,
    tool_search_limit: usize,
) -> Vec<ToolSpec> {
    let deferred = mcp_index.is_some();
    let mut specs = Vec::with_capacity(available_tools.len());
    for name in available_tools {
        // In deferred mode the individual MCP schemas are never sent to the LLM:
        // the synthetic `tool_search` tool (appended below) is the only entry
        // point. Native tools are resolved normally in both modes.
        if deferred && name.starts_with("mcp:") {
            continue;
        }
        match tool_registry.get(name).await {
            Ok(Some(descriptor)) => {
                specs.push(ToolSpec {
                    name: descriptor.name,
                    description: descriptor.description,
                    parameters: descriptor.input_schema,
                });
            }
            Ok(None) => {
                info!(tool = %name, "Tool not found in registry, skipping");
            }
            Err(e) => {
                warn!(tool = %name, error = %e, "Failed to get tool descriptor, skipping");
            }
        }
    }
    if deferred {
        specs.push(tool_search_spec(tool_search_limit));
    }
    specs
}

/// Build the synthetic `tool_search` [`ToolSpec`] injected in deferred MCP mode.
///
/// `max_limit` is the configured upper bound for the `limit` argument, surfaced
/// in the description so the model picks a valid value.
fn tool_search_spec(max_limit: usize) -> ToolSpec {
    ToolSpec {
        name: "tool_search".to_string(),
        description: format!(
            "Search the connected MCP tools by intent and return matching tools \
             with their fully qualified `mcp:server/tool` names. Call this before \
             invoking any MCP tool: the returned `full_name` is the exact name to \
             call. Takes an optional `query` substring (empty returns the top \
             results) and an optional `limit` between 1 and {max_limit}."
        ),
        parameters: tool_search_input_schema(),
    }
}

/// Truncate a string to a maximum length, appending "..." if truncated.
fn truncate_preview(s: &str) -> String {
    truncate_to(s, PREVIEW_MAX_LEN)
}

/// Next value of the consecutive-tool-failure counter.
///
/// Increments on a failed call and resets to 0 on success, so a run of failures
/// accumulates toward [`ESCALATION_FAILURE_THRESHOLD`] while any success clears it.
fn next_failure_count(current: u32, failed: bool) -> u32 {
    if failed {
        current.saturating_add(1)
    } else {
        0
    }
}

/// Truncate tool output for LLM context injection.
///
/// When the raw output exceeds [`TOOL_OUTPUT_MAX_LEN`], this function attempts
/// a smarter strategy: it parses the JSON result, prioritizes user-relevant
/// lines in stdout (lines under the user's home directory), and rebuilds a
/// compact result. Falls back to raw truncation if parsing fails.
fn truncate_tool_output(s: &str) -> String {
    if s.len() <= TOOL_OUTPUT_MAX_LEN {
        return s.to_string();
    }

    // Try to parse as the JSON shape returned by bash_executor / file_io
    if let Some(compacted) = compact_json_stdout(s) {
        return compacted;
    }

    // Fallback: raw truncation
    let truncated = truncate_to(s, TOOL_OUTPUT_MAX_LEN);
    format!(
        "{truncated}\n\n[Output truncated - {total} chars total. \
         Refine the command to produce less output.]",
        total = s.len()
    )
}

/// Compact the `stdout` field of a JSON tool result, prioritizing user-space
/// lines. Returns `None` when `s` is not the expected JSON shape.
fn compact_json_stdout(s: &str) -> Option<String> {
    let mut val = serde_json::from_str::<serde_json::Value>(s).ok()?;
    let stdout = val
        .get("stdout")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let lines: Vec<&str> = stdout.lines().collect();
    let total_lines = lines.len();

    // Partition: user-space lines first, then the rest
    let home = std::env::var("HOME").unwrap_or_default();
    let (user_lines, system_lines): (Vec<&str>, Vec<&str>) = if home.is_empty() {
        (lines.clone(), Vec::new())
    } else {
        lines.iter().partition(|l| l.starts_with(&home))
    };

    // Build compact output: user lines have priority, fill remaining budget
    let mut kept = Vec::new();
    let mut budget = TOOL_OUTPUT_MAX_LEN / 2; // reserve half for JSON overhead + notice
    for line in user_lines.iter().chain(system_lines.iter()) {
        if line.len() + 1 > budget {
            break;
        }
        budget -= line.len() + 1;
        kept.push(*line);
    }

    let compact_stdout = kept.join("\n");
    val["stdout"] = serde_json::Value::String(compact_stdout);

    let result = val.to_string();
    if kept.len() < total_lines {
        return Some(format!(
            "{result}\n\n[Output filtered - showing {kept}/{total} lines, \
             user paths prioritized. Refine the command for more precise results.]",
            kept = kept.len(),
            total = total_lines,
        ));
    }
    Some(result)
}

/// Truncate a string to `max_len` characters at a valid UTF-8 boundary.
fn truncate_to(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = s
            .char_indices()
            .take_while(|(i, _)| *i < max_len.saturating_sub(3))
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}...", &s[..boundary])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::types::{
        CompletionModel, CompletionRequest, CompletionResponse, FinishReason as LlmFinishReason,
        StreamChunk as LlmStreamChunk, ToolCall as LlmToolCall,
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    // ── Mock CompletionModel: streams text tokens then stops ─────────────

    struct MockStopModel {
        /// Tokens to emit (each becomes a StreamChunk::Text).
        tokens: Vec<String>,
    }

    impl MockStopModel {
        fn with_content(content: &str) -> Self {
            Self {
                tokens: split_tokens(content),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockStopModel {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
            Ok(CompletionResponse {
                content: self.tokens.join(""),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: LlmFinishReason::Stop,
                latency_ms: 1,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                        + Send,
                >,
            >,
            apollia_llm::types::LlmError,
        > {
            let chunks: Vec<Result<LlmStreamChunk, apollia_llm::types::LlmError>> = self
                .tokens
                .iter()
                .map(|t| Ok(LlmStreamChunk::Text(t.clone())))
                .collect();
            Ok(Box::pin(futures::stream::iter(chunks)))
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-stop"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel: streams tool calls then text ───────────────

    struct MockReActModel {
        calls: Vec<LlmToolCall>,
        final_tokens: Vec<String>,
        iteration: AtomicU32,
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockReActModel {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
            let current = self.iteration.load(Ordering::SeqCst);
            if current == 0 {
                Ok(CompletionResponse {
                    content: String::new(),
                    tool_calls: self.calls.clone(),
                    usage: TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        cost_usd: None,
                        ..Default::default()
                    },
                    finish_reason: LlmFinishReason::ToolCalls,
                    latency_ms: 1,
                    ttft_ms: None,
                })
            } else {
                Ok(CompletionResponse {
                    content: self.final_tokens.join(""),
                    tool_calls: vec![],
                    usage: TokenUsage {
                        prompt_tokens: 15,
                        completion_tokens: 8,
                        cost_usd: None,
                        ..Default::default()
                    },
                    finish_reason: LlmFinishReason::Stop,
                    latency_ms: 1,
                    ttft_ms: None,
                })
            }
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                        + Send,
                >,
            >,
            apollia_llm::types::LlmError,
        > {
            let current = self.iteration.fetch_add(1, Ordering::SeqCst);
            if current == 0 {
                // First iteration: emit tool calls
                let chunks: Vec<Result<LlmStreamChunk, apollia_llm::types::LlmError>> = self
                    .calls
                    .iter()
                    .map(|c| Ok(LlmStreamChunk::ToolCall(c.clone())))
                    .collect();
                Ok(Box::pin(futures::stream::iter(chunks)))
            } else {
                // Subsequent iterations: emit text tokens
                let chunks: Vec<Result<LlmStreamChunk, apollia_llm::types::LlmError>> = self
                    .final_tokens
                    .iter()
                    .map(|t| Ok(LlmStreamChunk::Text(t.clone())))
                    .collect();
                Ok(Box::pin(futures::stream::iter(chunks)))
            }
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-react"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel: always streams tool calls (infinite loop) ──

    struct MockInfiniteToolCallModel;

    #[async_trait::async_trait]
    impl CompletionModel for MockInfiniteToolCallModel {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
            Ok(CompletionResponse {
                content: String::new(),
                tool_calls: vec![LlmToolCall {
                    id: "c1".into(),
                    name: "bash_executor".into(),
                    arguments: serde_json::json!({"command": "echo"}),
                }],
                usage: TokenUsage {
                    prompt_tokens: 5,
                    completion_tokens: 3,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: LlmFinishReason::ToolCalls,
                latency_ms: 1,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                        + Send,
                >,
            >,
            apollia_llm::types::LlmError,
        > {
            let chunks = vec![Ok(LlmStreamChunk::ToolCall(LlmToolCall {
                id: "c1".into(),
                name: "bash_executor".into(),
                arguments: serde_json::json!({"command": "echo"}),
            }))];
            Ok(Box::pin(futures::stream::iter(chunks)))
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-infinite"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel: emits a tool call for the first `tool_turns`
    //    iterations, then final text. Lets a test drive the consecutive-failure
    //    counter past the escalation threshold and still terminate with a
    //    response (the tool calls fail via a failing invoker).
    struct MockFailingThenStopModel {
        tool_turns: u32,
        final_tokens: Vec<String>,
        iteration: AtomicU32,
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockFailingThenStopModel {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
            unimplemented!("streaming path only")
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                        + Send,
                >,
            >,
            apollia_llm::types::LlmError,
        > {
            let current = self.iteration.fetch_add(1, Ordering::SeqCst);
            if current < self.tool_turns {
                let chunks = vec![Ok(LlmStreamChunk::ToolCall(LlmToolCall {
                    id: format!("c{current}"),
                    name: "bash_executor".into(),
                    arguments: serde_json::json!({"command": "false"}),
                }))];
                Ok(Box::pin(futures::stream::iter(chunks)))
            } else {
                let chunks: Vec<Result<LlmStreamChunk, apollia_llm::types::LlmError>> = self
                    .final_tokens
                    .iter()
                    .map(|t| Ok(LlmStreamChunk::Text(t.clone())))
                    .collect();
                Ok(Box::pin(futures::stream::iter(chunks)))
            }
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-fail-then-stop"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    /// Split content into word-boundary tokens for mock streaming.
    fn split_tokens(content: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        for ch in content.chars() {
            if ch == ' ' {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push(" ".to_string());
            } else {
                current.push(ch);
            }
        }
        if !current.is_empty() {
            tokens.push(current);
        }
        tokens
    }

    // ── Mock ToolInvoker ─────────────────────────────────────────────────

    struct MockToolInvoker {
        result: String,
    }

    impl MockToolInvoker {
        fn new(result: impl Into<String>) -> Self {
            Self {
                result: result.into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl ToolInvoker for MockToolInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            Ok(self.result.clone())
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────

    fn make_router(model: Arc<dyn CompletionModel>) -> Arc<LlmRouter> {
        let mut backends = std::collections::HashMap::new();
        backends.insert("default".to_string(), model);
        Arc::new(LlmRouter::with_backends(backends, "default"))
    }

    fn make_event_bus() -> EventBusSender {
        let (tx, _rx) = tokio::sync::broadcast::channel(128);
        tx
    }

    fn make_budget(max_steps: u32) -> StepBudget {
        StepBudget::with_max(max_steps)
    }

    // ── Tests ────────────────────────────────────────────────────────────

    /// Simple text response without tool calls (streamed).
    #[tokio::test]
    async fn test_simple_text_response() {
        // GIVEN a model that streams text tokens without tool calls
        let model = Arc::new(MockStopModel::with_content("Bonjour !"));
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute with a simple user message
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "Salut",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN response contains the text, no tool calls
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Bonjour !");
        assert!(resp.tool_calls.is_empty());
        assert!(resp.newly_authorized.is_empty());
        // AND no hybrid routing was configured, so the ceiling was never hit.
        assert!(!resp.frontier_ceiling_reached);

        tool_registry.shutdown().await;
    }

    // next_failure_count: a failed call increments, a success resets to 0.
    #[test]
    fn test_next_failure_count_increments_and_resets() {
        // GIVEN a counter below the threshold
        let mut count = 2u32;

        // WHEN a tool call fails
        count = next_failure_count(count, true);
        // THEN the counter increments
        assert_eq!(count, 3);

        // WHEN a tool call then succeeds
        count = next_failure_count(count, false);
        // THEN the counter resets to 0
        assert_eq!(count, 0);

        // WHEN a success arrives with no prior failures
        assert_eq!(next_failure_count(0, false), 0);
    }

    /// Without a hybrid routing config, a run that fails tool calls past the
    /// escalation threshold still reports `frontier_ceiling_reached == false`:
    /// the escalation attempt against a non-hybrid router stays local.
    #[tokio::test]
    async fn test_no_hybrid_config_leaves_ceiling_flag_false() {
        // GIVEN a model that emits a failing tool call for 4 turns (past the
        //   threshold of 3), then final text, and a router with no hybrid section.
        let model = Arc::new(MockFailingThenStopModel {
            tool_turns: 4,
            final_tokens: split_tokens("Terminé"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        // Invoker returns a non-zero exit code: every tool call is a failure.
        let invoker: Arc<dyn ToolInvoker> =
            Arc::new(MockToolInvoker::new(r#"{"exit_code": 1, "stdout": ""}"#));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(20);
        let approvals = PendingChatApprovals::new();
        let mut authorized = HashSet::new();
        authorized.insert("bash_executor".to_string());

        // WHEN execute runs to a final text response
        let result = agent
            .execute(
                "sess-no-hybrid",
                "msg-1",
                &RunId::new(),
                "go",
                &[],
                "",
                &[],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN the loop crossed the escalation threshold but, with no hybrid
        // config, stayed local and never set the ceiling flag.
        let resp = result.expect("should produce a final response");
        assert_eq!(resp.content, "Terminé");
        assert!(!resp.frontier_ceiling_reached);

        tool_registry.shutdown().await;
    }

    /// Build a router with a single backend, a hybrid section configured with
    /// `action`, and a seeded session cost.
    fn make_hybrid_router(
        model: Arc<dyn CompletionModel>,
        ceiling: f64,
        session_cost: f64,
        action: CeilingAction,
    ) -> Arc<LlmRouter> {
        let mut backends = std::collections::HashMap::new();
        backends.insert("default".to_string(), model);
        let router =
            LlmRouter::with_backends(backends, "default").with_routing(apollia_core::LlmRoutingConfig {
                precise: "default".to_owned(),
                fast: "default".to_owned(),
                hybrid: Some(apollia_core::HybridRoutingConfig {
                    frontier: "default".to_owned(),
                    cost_ceiling_usd: ceiling,
                    ceiling_action: action,
                }),
            });
        router.seed_session_cost_usd(session_cost);
        Arc::new(router)
    }

    /// Helper: run a failing-then-stop exchange against `router`, returning the
    /// execute result and every event observed on the bus.
    async fn run_ceiling_exchange(
        router: Arc<LlmRouter>,
    ) -> (Result<ChatAgentResponse, ChatError>, Vec<RuntimeEvent>) {
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> =
            Arc::new(MockToolInvoker::new(r#"{"exit_code": 1, "stdout": ""}"#));
        let event_bus = make_event_bus();
        let mut rx = event_bus.subscribe();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });
        let budget = make_budget(20);
        let approvals = PendingChatApprovals::new();
        let mut authorized = HashSet::new();
        authorized.insert("bash_executor".to_string());

        let result = agent
            .execute(
                "sess-ceiling",
                "msg-1",
                &RunId::new(),
                "go",
                &[],
                "",
                &[],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        let mut events = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(ev) => events.push(ev),
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
        tool_registry.shutdown().await;
        (result, events)
    }

    fn failing_then_stop_model() -> Arc<MockFailingThenStopModel> {
        Arc::new(MockFailingThenStopModel {
            tool_turns: 4,
            final_tokens: split_tokens("done"),
            iteration: AtomicU32::new(0),
        })
    }

    #[tokio::test]
    async fn test_hard_stop_returns_error_on_ceiling() {
        // GIVEN a HardStop hybrid router with a session cost above the ceiling
        let router = make_hybrid_router(failing_then_stop_model(), 0.001, 1.0, CeilingAction::HardStop);

        // WHEN the loop escalates and detects the ceiling
        let (result, _events) = run_ceiling_exchange(router).await;

        // THEN the run stops cleanly with CostCeilingExceeded (no panic)
        assert!(matches!(
            result,
            Err(ChatError::CostCeilingExceeded { cost_usd, ceiling_usd })
                if cost_usd >= ceiling_usd
        ));
    }

    #[tokio::test]
    async fn test_hard_stop_emits_cost_ceiling_reached_event() {
        // GIVEN the same HardStop conditions
        let router = make_hybrid_router(failing_then_stop_model(), 0.001, 1.0, CeilingAction::HardStop);

        // WHEN the run hard-stops
        let (_result, events) = run_ceiling_exchange(router).await;

        // THEN a CostCeilingReached event carries the budget figures
        let found = events.iter().any(|ev| {
            matches!(
                ev,
                RuntimeEvent::CostCeilingReached { cost_usd, ceiling_usd, .. }
                    if (*ceiling_usd - 0.001).abs() < 1e-9 && *cost_usd >= *ceiling_usd
            )
        });
        assert!(found, "expected a CostCeilingReached event");
    }

    #[tokio::test]
    async fn test_stay_local_continues_without_error() {
        // GIVEN a StayLocal hybrid router with a session cost above the ceiling
        let router = make_hybrid_router(failing_then_stop_model(), 0.001, 1.0, CeilingAction::StayLocal);

        // WHEN the loop escalates and detects the ceiling
        let (result, events) = run_ceiling_exchange(router).await;

        // THEN the run continues to a final response, flags the ceiling, and
        // emits no CostCeilingReached event (no regression vs the prior behavior)
        let resp = result.expect("should produce a final response");
        assert_eq!(resp.content, "done");
        assert!(resp.frontier_ceiling_reached);
        assert!(!events
            .iter()
            .any(|ev| matches!(ev, RuntimeEvent::CostCeilingReached { .. })));
    }

    #[tokio::test]
    async fn test_hard_stop_below_ceiling_continues() {
        // GIVEN a HardStop hybrid router with a session cost below the ceiling
        let router = make_hybrid_router(failing_then_stop_model(), 10.0, 0.0, CeilingAction::HardStop);

        // WHEN the loop escalates but the ceiling is not reached
        let (result, events) = run_ceiling_exchange(router).await;

        // THEN the run continues normally and never flags or emits the ceiling
        let resp = result.expect("should produce a final response");
        assert_eq!(resp.content, "done");
        assert!(!resp.frontier_ceiling_reached);
        assert!(!events
            .iter()
            .any(|ev| matches!(ev, RuntimeEvent::CostCeilingReached { .. })));
    }

    /// Tool call authorized: direct execution (via streaming).
    #[tokio::test]
    async fn test_tool_call_authorized() {
        // GIVEN a model that streams a tool call, then text
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "bash_executor".into(),
                arguments: serde_json::json!({"command": "echo hello"}),
            }],
            final_tokens: split_tokens("Commande exécutée"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("hello\n"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();
        let mut authorized = HashSet::new();
        authorized.insert("bash_executor".to_string());

        // WHEN execute with "bash_executor" in authorized_tools
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "Execute echo",
                &[],
                "Tu es un assistant.",
                &["bash_executor".to_string()],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN tool was executed, response contains final text
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Commande exécutée");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].tool_name, "bash_executor");
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Executed);
        assert!(resp.tool_calls[0].output.is_some());

        tool_registry.shutdown().await;
    }

    /// Tool call not authorized, HITL Accept.
    #[tokio::test]
    async fn test_tool_call_hitl_accept() {
        // GIVEN a model with tool call "file_read" NOT in authorized_tools
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "file_read".into(),
                arguments: serde_json::json!({"path": "test.txt"}),
            }],
            final_tokens: split_tokens("Fichier lu"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("file content"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // Pre-resolve the approval to Accept before execute (simulates user action)
        let key = "sess-1::msg-1::file_read".to_string();
        tokio::spawn({
            let approvals = approvals.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                approvals.resolve(&key, ToolDecision::Accept);
            }
        });

        // WHEN execute
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "Read file",
                &[],
                "assistant",
                &["file_read".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN tool was executed after approval
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Fichier lu");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Executed);
        assert!(resp.newly_authorized.is_empty());

        tool_registry.shutdown().await;
    }

    /// Tool call HITL Refuse: refusal message injected.
    #[tokio::test]
    async fn test_tool_call_hitl_refuse() {
        // GIVEN a model with unauthorized tool, decision = Refuse
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "file_read".into(),
                arguments: serde_json::json!({}),
            }],
            final_tokens: split_tokens("Ok, pas de souci."),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("unused"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        let key = "sess-1::msg-1::file_read".to_string();
        tokio::spawn({
            let approvals = approvals.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                approvals.resolve(&key, ToolDecision::refuse());
            }
        });

        // WHEN execute
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "Read",
                &[],
                "assistant",
                &["file_read".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN refusal recorded, LLM sees it and produces final text
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Ok, pas de souci.");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Refused);
        assert_eq!(
            resp.tool_calls[0].output.as_deref(),
            Some("Outil refusé par l'utilisateur")
        );

        tool_registry.shutdown().await;
    }

    /// Tool call HITL AlwaysAccept: tool allowlisted.
    #[tokio::test]
    async fn test_tool_call_hitl_always_accept() {
        // GIVEN unauthorized tool, decision = AlwaysAccept
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "file_read".into(),
                arguments: serde_json::json!({}),
            }],
            final_tokens: split_tokens("Done"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        let key = "sess-1::msg-1::file_read".to_string();
        tokio::spawn({
            let approvals = approvals.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                approvals.resolve(&key, ToolDecision::always_accept_default());
            }
        });

        // WHEN execute
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "Read",
                &[],
                "assistant",
                &["file_read".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN tool executed AND newly_authorized contains "file_read"
        let resp = result.expect("should succeed");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Executed);
        assert_eq!(resp.newly_authorized, vec!["file_read".to_string()]);

        tool_registry.shutdown().await;
    }

    /// Budget exhausted returns error.
    #[tokio::test]
    async fn test_budget_exhausted() {
        // GIVEN a model that always returns tool calls + budget max_steps=1
        let model = Arc::new(MockInfiniteToolCallModel);
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(1);
        let mut authorized = HashSet::new();
        authorized.insert("bash_executor".to_string());
        let approvals = PendingChatApprovals::new();

        // WHEN execute, first iteration uses the budget, second checks and fails
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "Loop",
                &[],
                "assistant",
                &["bash_executor".to_string()],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN BudgetExhausted error
        assert!(
            matches!(result, Err(ChatError::BudgetExhausted)),
            "expected BudgetExhausted, got: {result:?}"
        );

        tool_registry.shutdown().await;
    }

    /// build_llm_messages constructs messages in correct order.
    #[test]
    fn test_build_llm_messages() {
        // GIVEN system prompt, 3 history messages, and a user message
        let history = vec![
            ChatMessage {
                id: "m1".into(),
                role: ChatRole::User,
                content: "Hello".into(),
                tool_calls: None,
                tool_name: None,
                created_at: String::new(),
                seq: 1,
                metadata: None,
            },
            ChatMessage {
                id: "m2".into(),
                role: ChatRole::Assistant,
                content: "Hi there".into(),
                tool_calls: None,
                tool_name: None,
                created_at: String::new(),
                seq: 2,
                metadata: None,
            },
            ChatMessage {
                id: "m3".into(),
                role: ChatRole::User,
                content: "How are you?".into(),
                tool_calls: None,
                tool_name: None,
                created_at: String::new(),
                seq: 3,
                metadata: None,
            },
        ];

        // WHEN building LLM messages
        let messages = build_llm_messages(
            "You are helpful.",
            &history,
            "Final question",
            None,
            DEFAULT_CONTEXT_WINDOW_SIZE,
        );

        // THEN 5 messages in order: system, h1 (user), h2 (assistant), h3 (user), current user
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[0].role, apollia_llm::types::Role::System);
        assert_eq!(messages[1].role, apollia_llm::types::Role::User);
        assert_eq!(messages[2].role, apollia_llm::types::Role::Assistant);
        assert_eq!(messages[3].role, apollia_llm::types::Role::User);
        assert_eq!(messages[4].role, apollia_llm::types::Role::User);
    }

    /// Events emitted in correct order (including ChatToken).
    #[tokio::test]
    async fn test_events_emitted_in_order() {
        // GIVEN a model that streams one tool call then text "Done"
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({}),
            }],
            final_tokens: split_tokens("Done"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("output"));
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus: event_tx,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let mut authorized = HashSet::new();
        authorized.insert("bash".to_string());
        let approvals = PendingChatApprovals::new();

        // WHEN execute completes
        let _resp = agent
            .execute(
                "s1",
                "m1",
                &RunId::new(),
                "Go",
                &[],
                "prompt",
                &["bash".to_string()],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("should succeed");

        // THEN events are: ResponseStarted (tool iteration), ToolCallStarted,
        // ToolCallCompleted, ResponseStarted (text iteration), Token("Done"),
        // ResponseCompleted
        let mut event_names = Vec::new();
        while let Ok(evt) = event_rx.try_recv() {
            let name = match evt {
                RuntimeEvent::ChatResponseStarted { .. } => "ResponseStarted",
                RuntimeEvent::ChatToken { .. } => "Token",
                RuntimeEvent::ChatToolCallStarted { .. } => "ToolCallStarted",
                RuntimeEvent::ChatToolCallCompleted { .. } => "ToolCallCompleted",
                RuntimeEvent::ChatResponseCompleted { .. } => "ResponseCompleted",
                RuntimeEvent::LlmCallCompleted { .. } => continue,
                _ => "other",
            };
            event_names.push(name);
        }

        assert_eq!(
            event_names,
            vec![
                "ResponseStarted",
                "ToolCallStarted",
                "ToolCallCompleted",
                "ResponseStarted",
                "Token",
                "ResponseCompleted"
            ]
        );

        tool_registry.shutdown().await;
    }

    #[test]
    fn test_truncate_preview_short() {
        // GIVEN a string shorter than PREVIEW_MAX_LEN
        let s = "short string";
        // WHEN truncating
        let result = truncate_preview(s);
        // THEN unchanged
        assert_eq!(result, s);
    }

    #[test]
    fn test_truncate_preview_long() {
        // GIVEN a string longer than PREVIEW_MAX_LEN
        let s = "a".repeat(300);
        // WHEN truncating
        let result = truncate_preview(&s);
        // THEN truncated with "..."
        assert!(result.len() <= PREVIEW_MAX_LEN);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_default_system_prompt_used_when_empty() {
        // GIVEN empty system_prompt
        let messages = build_llm_messages("", &[], "Hello", None, DEFAULT_CONTEXT_WINDOW_SIZE);

        // THEN first message is the empty string we passed (caller decides default)
        assert_eq!(messages.len(), 2);
    }

    // ── Streaming-specific tests ──────────────────────────────────────────

    /// Each token emits a ChatToken event.
    #[tokio::test]
    async fn test_stream_tokens_emitted() {
        // GIVEN a model that streams ["Bon", "jour", " ", "!"]
        let model = Arc::new(MockStopModel {
            tokens: vec!["Bon".into(), "jour".into(), " ".into(), "!".into()],
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus: event_tx,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "Salut",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("should succeed");

        // THEN 4 ChatToken events emitted, content is "Bonjour !"
        assert_eq!(resp.content, "Bonjour !");

        let mut tokens = Vec::new();
        while let Ok(evt) = event_rx.try_recv() {
            if let RuntimeEvent::ChatToken { token, .. } = evt {
                tokens.push(token);
            }
        }
        assert_eq!(tokens, vec!["Bon", "jour", " ", "!"]);

        tool_registry.shutdown().await;
    }

    /// Accumulated text from stream matches final content.
    #[tokio::test]
    async fn test_stream_accumulation() {
        // GIVEN a model that streams ["Hello", " ", "world"]
        let model = Arc::new(MockStopModel {
            tokens: vec!["Hello".into(), " ".into(), "world".into()],
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "test",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("should succeed");

        // THEN accumulated text is "Hello world"
        assert_eq!(resp.content, "Hello world");

        tool_registry.shutdown().await;
    }

    /// Stream interruption returns partial content.
    #[tokio::test]
    async fn test_stream_interrupted() {
        // GIVEN a model whose stream returns 2 tokens then an error
        struct InterruptedModel;

        #[async_trait::async_trait]
        impl CompletionModel for InterruptedModel {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
                unimplemented!()
            }

            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<
                std::pin::Pin<
                    Box<
                        dyn futures::Stream<
                                Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>,
                            > + Send,
                    >,
                >,
                apollia_llm::types::LlmError,
            > {
                let chunks = vec![
                    Ok(LlmStreamChunk::Text("Par".into())),
                    Ok(LlmStreamChunk::Text("tial".into())),
                    Err(apollia_llm::types::LlmError::InferenceError(
                        "connection reset".into(),
                    )),
                ];
                Ok(Box::pin(futures::stream::iter(chunks)))
            }

            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                "mock-interrupted"
            }
            fn model_id(&self) -> &str {
                "mock"
            }
        }

        let model: Arc<dyn CompletionModel> = Arc::new(InterruptedModel);
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus: event_tx,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "test",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("should return partial content, not error");

        // THEN partial content is saved
        assert_eq!(resp.content, "Partial");

        // AND ChatError event was emitted
        let mut has_error = false;
        while let Ok(evt) = event_rx.try_recv() {
            if let RuntimeEvent::ChatError { error, .. } = evt {
                assert!(error.contains("connection reset"));
                has_error = true;
            }
        }
        assert!(has_error, "ChatError event should have been emitted");

        tool_registry.shutdown().await;
    }

    /// Stream with tool call: text tokens emitted, then tool executed.
    #[tokio::test]
    async fn test_stream_with_tool_call() {
        // GIVEN a model that streams text + tool_call on first iteration,
        // then only text on second iteration
        struct TextThenToolModel {
            iteration: AtomicU32,
        }

        #[async_trait::async_trait]
        impl CompletionModel for TextThenToolModel {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
                unimplemented!()
            }

            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<
                std::pin::Pin<
                    Box<
                        dyn futures::Stream<
                                Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>,
                            > + Send,
                    >,
                >,
                apollia_llm::types::LlmError,
            > {
                let current = self.iteration.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    let chunks = vec![
                        Ok(LlmStreamChunk::Text("Je ".into())),
                        Ok(LlmStreamChunk::Text("vais ".into())),
                        Ok(LlmStreamChunk::Text("lire".into())),
                        Ok(LlmStreamChunk::ToolCall(LlmToolCall {
                            id: "c1".into(),
                            name: "file_read".into(),
                            arguments: serde_json::json!({"path": "data.txt"}),
                        })),
                    ];
                    Ok(Box::pin(futures::stream::iter(chunks)))
                } else {
                    let chunks = vec![
                        Ok(LlmStreamChunk::Text("Fichier ".into())),
                        Ok(LlmStreamChunk::Text("lu.".into())),
                    ];
                    Ok(Box::pin(futures::stream::iter(chunks)))
                }
            }

            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                "mock-text-tool"
            }
            fn model_id(&self) -> &str {
                "mock"
            }
        }

        let model: Arc<dyn CompletionModel> = Arc::new(TextThenToolModel {
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("file content"));
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus: event_tx,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();
        let mut authorized = HashSet::new();
        authorized.insert("file_read".to_string());

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                &RunId::new(),
                "lis le fichier",
                &[],
                "",
                &["file_read".to_string()],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("should succeed");

        // THEN final content from second iteration
        assert_eq!(resp.content, "Fichier lu.");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].tool_name, "file_read");
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Executed);

        // AND tokens from both iterations were emitted
        let mut tokens = Vec::new();
        while let Ok(evt) = event_rx.try_recv() {
            if let RuntimeEvent::ChatToken { token, .. } = evt {
                tokens.push(token);
            }
        }
        // First iteration text tokens + second iteration text tokens
        assert_eq!(tokens, vec!["Je ", "vais ", "lire", "Fichier ", "lu."]);

        tool_registry.shutdown().await;
    }

    // ── User memory injection tests ─────────────────────────────────────

    fn make_user_memory_repo(
        entries: &[(&str, &str, &str)],
    ) -> Arc<std::sync::Mutex<UserMemoryRepository>> {
        use apollia_memory::user_memory::WrittenBy;

        let dir = std::env::temp_dir().join(format!("apollia_test_um_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let db_path = dir.join("user_memory.db");
        let repo = UserMemoryRepository::new(&db_path).expect("open user memory db");

        // Categories from the legacy `(category, key, value)` test fixtures
        // are ignored; storage is flat.
        for (_category, key, value) in entries {
            repo.set(key, value, WrittenBy::User).expect("set entry");
        }

        Arc::new(std::sync::Mutex::new(repo))
    }

    #[tokio::test]
    async fn test_build_system_prompt_with_non_empty_user_memory() {
        // GIVEN a BuiltInChatAgent with 3 user memory entries
        let repo = make_user_memory_repo(&[
            ("preferences", "langue", "francais"),
            ("preferences", "format", "markdown"),
            ("context", "projet", "apollia"),
        ]);
        let router = make_router(Arc::new(MockStopModel::with_content("ok")));
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry,
            tool_invoker: invoker,
            event_bus,
            user_memory: Some(repo),
            a2a_invoker: None,
            todo: None,
        });

        // WHEN building the system prompt
        let prompt = agent.build_system_prompt(Some("Base prompt."), AutonomyLevel::Assisted, true);

        // THEN the prompt opens with the authoritative environment block
        // (temporal context now leads the prompt) and still carries the
        // base prompt + user persona section.
        assert!(prompt.starts_with("## CURRENT ENVIRONMENT"));
        assert!(prompt.contains("Base prompt."));
        assert!(prompt.contains("## User Persona"));
        assert!(prompt.contains("francais"));
        assert!(prompt.contains("markdown"));
        assert!(prompt.contains("apollia"));
    }

    // with a populated repo, a tier whose `inject_memory` is
    // false must NOT inject the persona block, while a tier with it true must.
    #[tokio::test]
    async fn test_inject_memory_flag_gates_persona_block() {
        // GIVEN an agent with a non-empty user memory repository
        let repo = make_user_memory_repo(&[("preferences", "langue", "francais")]);
        let router = make_router(Arc::new(MockStopModel::with_content("ok")));
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry,
            tool_invoker: invoker,
            event_bus,
            user_memory: Some(repo),
            a2a_invoker: None,
            todo: None,
        });

        // WHEN inject_memory is false (e.g. supervised tier)
        let without = agent.build_system_prompt(None, AutonomyLevel::Supervised, false);
        // THEN the persona block is absent despite the populated repo
        assert!(!without.contains("## User Persona"));
        assert!(!without.contains("francais"));

        // WHEN inject_memory is true (e.g. long autonomous tier)
        let with = agent.build_system_prompt(None, AutonomyLevel::LongAutonomous, true);
        // THEN the persona block is injected
        assert!(with.contains("## User Persona"));
        assert!(with.contains("francais"));
    }

    // the effective budget is the tier budget capped by the
    // runtime ceiling, never above it.
    #[test]
    fn test_from_capped_applies_runtime_ceiling() {
        // GIVEN the long-autonomous tier (500 steps) and a 200-step ceiling
        let config = apollia_core::AutonomyConfig::default();
        let ceiling = apollia_core::StepBudgetConfig {
            max_steps: 200,
            max_tool_calls: 400,
            wall_clock_secs: 3600,
        };
        let lc = config.level_config(AutonomyLevel::LongAutonomous);
        let budget = StepBudget::from_capped(&lc.budget, &ceiling);

        // THEN the ceiling caps max_steps and the tier flags are active
        assert_eq!(budget.max_steps, 200);
        assert!(lc.inject_memory);
        assert!(lc.run_verification);

        // AND the assisted tier stays at its own 100-step budget under the ceiling
        let assisted = config.level_config(AutonomyLevel::Assisted);
        let assisted_budget = StepBudget::from_capped(&assisted.budget, &ceiling);
        assert_eq!(assisted_budget.max_steps, 100);
        assert!(!assisted.inject_memory);
        assert!(!assisted.run_verification);
    }

    #[tokio::test]
    async fn test_build_system_prompt_with_empty_user_memory() {
        // GIVEN a BuiltInChatAgent with an empty user memory repository
        let repo = make_user_memory_repo(&[]);
        let router = make_router(Arc::new(MockStopModel::with_content("ok")));
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry,
            tool_invoker: invoker,
            event_bus,
            user_memory: Some(repo),
            a2a_invoker: None,
            todo: None,
        });

        // WHEN building the system prompt
        let prompt = agent.build_system_prompt(Some("Base prompt."), AutonomyLevel::Assisted, true);

        // THEN the prompt does NOT contain the user persona block
        assert!(!prompt.contains("User Persona"));
    }

    #[tokio::test]
    async fn test_build_system_prompt_without_repository() {
        // GIVEN a BuiltInChatAgent with no user memory repository (None)
        let router = make_router(Arc::new(MockStopModel::with_content("ok")));
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry,
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        });

        // WHEN building the system prompt
        let prompt = agent.build_system_prompt(Some("Base prompt."), AutonomyLevel::Assisted, true);

        // THEN the prompt does NOT contain the user persona block
        assert!(!prompt.contains("User Persona"));
    }

    // ── Level-aware prompt selection (story 549) ─────────────────────────

    /// Build a `BuiltInChatAgent` with no user memory repository, for prompt
    /// selection tests that do not exercise persona injection.
    fn make_agent_no_memory() -> BuiltInChatAgent {
        let router = make_router(Arc::new(MockStopModel::with_content("ok")));
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry,
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        })
    }

    #[tokio::test]
    async fn test_assisted_uses_default_prompt() {
        // GIVEN an agent with no user memory
        let agent = make_agent_no_memory();

        // WHEN building the prompt for the assisted tier
        let prompt = agent.build_system_prompt(None, AutonomyLevel::Assisted, false);

        // THEN it carries the reactive default marker, not the perseverance one
        assert!(prompt.contains("Agis d'abord"));
        assert!(!prompt.contains("Persevere jusqu'a l'objectif"));
    }

    #[tokio::test]
    async fn test_bounded_autonomous_uses_perseverance_prompt() {
        // GIVEN an agent with no user memory
        let agent = make_agent_no_memory();

        // WHEN building the prompt for a bounded-autonomous tier
        let prompt = agent.build_system_prompt(None, AutonomyLevel::BoundedAutonomous, false);

        // THEN it carries the perseverance marker, not the reactive default
        assert!(prompt.contains("Persevere jusqu'a l'objectif"));
        assert!(!prompt.contains("Agis d'abord"));
    }

    #[tokio::test]
    async fn test_long_autonomous_uses_perseverance_prompt() {
        // GIVEN an agent with no user memory
        let agent = make_agent_no_memory();

        // WHEN building the prompt for the long-autonomous tier
        let prompt = agent.build_system_prompt(None, AutonomyLevel::LongAutonomous, false);

        // THEN it carries the perseverance marker
        assert!(prompt.contains("Persevere jusqu'a l'objectif"));
    }

    #[tokio::test]
    async fn test_custom_prompt_preserved_for_assisted() {
        // GIVEN an agent and a custom base prompt
        let agent = make_agent_no_memory();
        let custom = "Mon prompt personnalise";

        // WHEN building the prompt with the custom base
        let prompt = agent.build_system_prompt(Some(custom), AutonomyLevel::Assisted, false);

        // THEN the custom prompt is used verbatim
        assert!(prompt.contains("Mon prompt personnalise"));
    }

    #[tokio::test]
    async fn test_all_autonomy_levels_no_panic() {
        // GIVEN an agent and every tier
        let agent = make_agent_no_memory();

        // WHEN / THEN building the prompt for each tier never panics
        for level in AutonomyLevel::ALL {
            let _ = agent.build_system_prompt(None, level, false);
        }
    }

    #[tokio::test]
    async fn test_temporal_context_always_prepended() {
        // GIVEN an agent with no user memory
        let agent = make_agent_no_memory();

        // WHEN building both the assisted and an autonomous prompt
        let p_assisted = agent.build_system_prompt(None, AutonomyLevel::Assisted, false);
        let p_auto = agent.build_system_prompt(None, AutonomyLevel::LongAutonomous, false);

        // THEN both are longer than the bare constant (temporal block prepended)
        assert!(p_assisted.len() > DEFAULT_SYSTEM_PROMPT.len());
        assert!(p_auto.len() > PERSEVERANCE_SYSTEM_PROMPT.len());
    }

    // ── Context window management tests ─────────────────────────────────

    fn make_history(count: usize) -> Vec<ChatMessage> {
        (0..count)
            .map(|i| ChatMessage {
                id: format!("msg-{i}"),
                role: if i % 2 == 0 {
                    ChatRole::User
                } else {
                    ChatRole::Assistant
                },
                content: format!("message {i}"),
                tool_calls: None,
                tool_name: None,
                created_at: "2026-03-24T10:00:00Z".to_string(),
                seq: i as u32 + 1,
                metadata: None,
            })
            .collect()
    }

    /// Extract text from a MessageContent, panicking if it's not Text.
    fn text_of(msg: &apollia_llm::types::ChatMessage) -> &str {
        match &msg.content {
            apollia_llm::types::MessageContent::Text(s) => s.as_str(),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn test_context_window_short_conversation_includes_all() {
        // GIVEN a conversation shorter than window_size (10 messages, window=20)
        let history = make_history(10);

        // WHEN building context without summary
        let messages = build_llm_messages("system", &history, "new msg", None, 20);

        // THEN all 10 history messages are included (+ system + user = 12)
        assert_eq!(messages.len(), 12);
        assert_eq!(messages[0].role, apollia_llm::types::Role::System);
        assert_eq!(messages[11].role, apollia_llm::types::Role::User);
    }

    #[test]
    fn test_context_window_long_conversation_with_summary() {
        // GIVEN 40 messages, window_size=20, and a stored summary
        let history = make_history(40);
        let summary = "The user discussed project setup and deployment.";

        // WHEN building context with summary
        let messages = build_llm_messages("system", &history, "new msg", Some(summary), 20);

        // THEN: system + summary + 20 windowed messages + user = 23
        assert_eq!(messages.len(), 23);
        // First message is system prompt
        assert_eq!(messages[0].role, apollia_llm::types::Role::System);
        // Second message is the summary (system role)
        assert_eq!(messages[1].role, apollia_llm::types::Role::System);
        let summary_text = text_of(&messages[1]);
        assert!(summary_text.contains("Previous context summary:"));
        assert!(summary_text.contains(summary));
        // Last message is the current user message
        assert_eq!(messages[22].role, apollia_llm::types::Role::User);
        assert_eq!(text_of(&messages[22]), "new msg");
        // Windowed messages start from index 20 (history[20..40])
        assert_eq!(text_of(&messages[2]), "message 20");
    }

    #[test]
    fn test_context_window_long_conversation_without_summary() {
        // GIVEN 40 messages, window_size=20, no summary
        let history = make_history(40);

        // WHEN building context without summary
        let messages = build_llm_messages("system", &history, "new msg", None, 20);

        // THEN: system + 20 windowed messages + user = 22 (no summary message)
        assert_eq!(messages.len(), 22);
        assert_eq!(messages[0].role, apollia_llm::types::Role::System);
        // First windowed message is history[20]
        assert_eq!(text_of(&messages[1]), "message 20");
        // Last message is current user message
        assert_eq!(messages[21].role, apollia_llm::types::Role::User);
        assert_eq!(text_of(&messages[21]), "new msg");
    }

    // ── NativeChatToolInvoker constructor tests ──────────────────────────

    #[test]
    fn new_with_workspace_some_valid_dir_uses_it() {
        // GIVEN an existing temporary directory
        let tmp = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).expect("create tmp dir");

        // WHEN creating the invoker with that path
        let invoker = NativeChatToolInvoker::new_with_workspace(Some(tmp.clone()));

        // THEN sandbox_root equals the provided directory
        assert_eq!(invoker.sandbox_root, tmp);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn new_with_workspace_none_uses_current_dir_not_home() {
        // GIVEN no workspace path
        // WHEN creating the invoker with None
        let invoker = NativeChatToolInvoker::new_with_workspace(None);

        // THEN sandbox_root equals current_dir()
        let cwd = std::env::current_dir().expect("cwd must be available");
        assert_eq!(invoker.sandbox_root, cwd);

        // AND sandbox_root is never $HOME
        if let Ok(home) = std::env::var("HOME") {
            assert_ne!(
                invoker.sandbox_root,
                std::path::PathBuf::from(home),
                "sandbox root must not fall back to $HOME"
            );
        }
    }

    #[test]
    fn new_with_workspace_some_invalid_dir_falls_back() {
        // GIVEN a path that does not exist on disk
        let ghost = std::path::PathBuf::from("/nonexistent/apollia/ghost-dir");

        // WHEN creating the invoker with that path
        let invoker = NativeChatToolInvoker::new_with_workspace(Some(ghost.clone()));

        // THEN sandbox_root is not the ghost path (filter rejects non-existent dirs)
        assert_ne!(invoker.sandbox_root, ghost);
    }

    // ── build_tool_specs: eager vs deferred MCP ────────────────────────────

    /// Build a minimal registry descriptor for an `mcp:server/tool` name.
    fn mcp_descriptor(full_name: &str) -> apollia_tools::descriptor::ToolDescriptor {
        use apollia_tools::descriptor::{ToolDescriptor, ToolKind};
        ToolDescriptor {
            name: full_name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("MCP tool {full_name}"),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            sandbox_profile: apollia_core::SandboxProfile::NetworkRestricted,
            tags: vec!["mcp".to_string()],
            dangerous: false,
            is_read_only: false,
            risk_score: 3,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    fn snapshot(server: &str, tool: &str) -> ToolIndexSnapshot {
        ToolIndexSnapshot {
            server_name: server.to_string(),
            tool_name: tool.to_string(),
            description: Some(format!("{tool} description")),
            tags: vec![],
        }
    }

    // ── Parallel read-only tool-call partition ─────────────────────────────

    /// A read-only tool descriptor (eligible for concurrent execution).
    fn ro_descriptor(name: &str) -> apollia_tools::descriptor::ToolDescriptor {
        let mut d = mcp_descriptor(name);
        d.is_read_only = true;
        d
    }

    /// Tool invoker that tracks peak concurrency and echoes the tool name.
    struct ConcurrencyInvoker {
        concurrent: AtomicU32,
        peak: AtomicU32,
        delay_ms: u64,
    }

    impl ConcurrencyInvoker {
        fn new(delay_ms: u64) -> Self {
            Self {
                concurrent: AtomicU32::new(0),
                peak: AtomicU32::new(0),
                delay_ms,
            }
        }
        fn peak(&self) -> u32 {
            self.peak.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl ToolInvoker for ConcurrencyInvoker {
        async fn invoke(&self, tool_name: &str, _: &serde_json::Value) -> Result<String, String> {
            let cur = self.concurrent.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(cur, Ordering::SeqCst);
            if self.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            }
            self.concurrent.fetch_sub(1, Ordering::SeqCst);
            Ok(tool_name.to_string())
        }
    }

    fn agent_with(
        registry: ToolRegistryHandle,
        invoker: Arc<dyn ToolInvoker>,
    ) -> BuiltInChatAgent {
        BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: make_router(Arc::new(MockStopModel::with_content("x"))),
            tool_registry: registry,
            tool_invoker: invoker,
            event_bus: make_event_bus(),
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        })
    }

    fn tool_call(i: usize, name: &str) -> ToolCall {
        ToolCall {
            id: format!("c{i}"),
            name: name.to_string(),
            arguments: serde_json::json!({}),
        }
    }

    /// Runs `record_tool_turn` for `calls`, authorizing every named tool so none
    /// hits the HITL path, and returns the resulting tool-call order plus the
    /// invoker peak concurrency.
    async fn run_turn(
        agent: &BuiltInChatAgent,
        invoker: &ConcurrencyInvoker,
        calls: &[ToolCall],
    ) -> Vec<String> {
        let mut acc = ReactAccumulators {
            all_tool_calls: vec![],
            newly_authorized: vec![],
            authorized: calls.iter().map(|c| c.name.clone()).collect(),
        };
        let budget = make_budget(100);
        let approvals = PendingChatApprovals::new();
        let mut reasoning = Vec::new();
        let mut msgs = Vec::new();
        let mut failures = 0u32;
        let _ = invoker; // peak read by the caller after the turn
        agent
            .record_tool_turn(
                RecordTurnInput {
                    accumulated_text: "",
                    tool_calls: calls,
                    budget: &budget,
                    ids: ToolCallContextIds {
                        session_id: "s",
                        message_id: "m",
                        run_id: &RunId::new(),
                        pending_approvals: &approvals,
                    },
                },
                &mut reasoning,
                &mut msgs,
                &mut acc,
                &mut failures,
            )
            .await;
        acc.all_tool_calls
            .iter()
            .map(|r| r.tool_name.clone())
            .collect()
    }

    /// Authorized read-only calls run concurrently and keep their input order.
    #[tokio::test]
    async fn test_readonly_calls_run_in_parallel_preserving_order() {
        // GIVEN four authorized read-only tools
        let registry = ToolRegistryHandle::start();
        for n in ["ro_a", "ro_b", "ro_c", "ro_d"] {
            registry.register(ro_descriptor(n)).await.unwrap();
        }
        let invoker = Arc::new(ConcurrencyInvoker::new(30));
        let agent = agent_with(registry.clone(), invoker.clone());
        let calls = vec![
            tool_call(0, "ro_a"),
            tool_call(1, "ro_b"),
            tool_call(2, "ro_c"),
            tool_call(3, "ro_d"),
        ];

        // WHEN the turn runs
        let order = run_turn(&agent, &invoker, &calls).await;

        // THEN results keep input order and the invocations overlapped
        assert_eq!(order, vec!["ro_a", "ro_b", "ro_c", "ro_d"]);
        assert!(
            invoker.peak() >= 2,
            "expected concurrent read-only execution, peak was {}",
            invoker.peak()
        );
        registry.shutdown().await;
    }

    /// A mixed turn keeps global order: writes and unknown tools stay sequential,
    /// read-only authorized tools run concurrently, results merge in input order.
    #[tokio::test]
    async fn test_mixed_calls_preserve_global_order() {
        // GIVEN a registered write tool, two read-only tools, and one unknown tool
        let registry = ToolRegistryHandle::start();
        registry.register(mcp_descriptor("w_x")).await.unwrap(); // is_read_only = false
        registry.register(ro_descriptor("ro_a")).await.unwrap();
        registry.register(ro_descriptor("ro_b")).await.unwrap();
        // "w_y" is intentionally not registered: unknown status is treated as write.
        let invoker = Arc::new(ConcurrencyInvoker::new(0));
        let agent = agent_with(registry.clone(), invoker.clone());
        let calls = vec![
            tool_call(0, "w_x"),
            tool_call(1, "ro_a"),
            tool_call(2, "ro_b"),
            tool_call(3, "w_y"),
            tool_call(4, "ro_a"),
        ];

        // WHEN the turn runs
        let order = run_turn(&agent, &invoker, &calls).await;

        // THEN the final order matches the input order exactly
        assert_eq!(order, vec!["w_x", "ro_a", "ro_b", "w_y", "ro_a"]);
        registry.shutdown().await;
    }

    /// Concurrency stays bounded by the read-only cap.
    #[tokio::test]
    async fn test_readonly_concurrency_cap_respected() {
        // GIVEN 15 authorized read-only tools with a slow invoker
        let registry = ToolRegistryHandle::start();
        for i in 0..15 {
            registry
                .register(ro_descriptor(&format!("ro_{i}")))
                .await
                .unwrap();
        }
        let invoker = Arc::new(ConcurrencyInvoker::new(30));
        let agent = agent_with(registry.clone(), invoker.clone());
        let calls: Vec<ToolCall> = (0..15).map(|i| tool_call(i, &format!("ro_{i}"))).collect();

        // WHEN the turn runs
        let order = run_turn(&agent, &invoker, &calls).await;

        // THEN all 15 complete in order and concurrency respects the cap
        assert_eq!(order.len(), 15);
        for (i, name) in order.iter().enumerate() {
            assert_eq!(name, &format!("ro_{i}"));
        }
        assert!(invoker.peak() <= MAX_CONCURRENT_READONLY_TOOL_CALLS as u32);
        assert!(invoker.peak() >= 2, "expected some concurrency");
        registry.shutdown().await;
    }

    /// Tool invoker that fails for one configured tool name and echoes the rest.
    struct FailingInvoker {
        failing: String,
    }

    #[async_trait::async_trait]
    impl ToolInvoker for FailingInvoker {
        async fn invoke(&self, tool_name: &str, _: &serde_json::Value) -> Result<String, String> {
            if tool_name == self.failing {
                Err("boom".to_string())
            } else {
                Ok(tool_name.to_string())
            }
        }
    }

    /// Runs `record_tool_turn` against an external budget and returns the ordered
    /// tool-call records plus the final `consecutive_tool_failures` count, so tests
    /// can assert per-position outcomes and budget accounting.
    async fn run_turn_full(
        agent: &BuiltInChatAgent,
        budget: &StepBudget,
        calls: &[ToolCall],
    ) -> (Vec<ToolCallRecord>, u32) {
        let mut acc = ReactAccumulators {
            all_tool_calls: vec![],
            newly_authorized: vec![],
            authorized: calls.iter().map(|c| c.name.clone()).collect(),
        };
        let approvals = PendingChatApprovals::new();
        let mut reasoning = Vec::new();
        let mut msgs = Vec::new();
        let mut failures = 0u32;
        agent
            .record_tool_turn(
                RecordTurnInput {
                    accumulated_text: "",
                    tool_calls: calls,
                    budget,
                    ids: ToolCallContextIds {
                        session_id: "s",
                        message_id: "m",
                        run_id: &RunId::new(),
                        pending_approvals: &approvals,
                    },
                },
                &mut reasoning,
                &mut msgs,
                &mut acc,
                &mut failures,
            )
            .await;
        (acc.all_tool_calls, failures)
    }

    /// All-write turns stay sequential: no two writes overlap, order is preserved.
    #[tokio::test]
    async fn test_all_write_sequential_order() {
        // GIVEN three registered write tools (is_read_only = false) and a slow
        //   invoker so any overlap would be observed by the concurrency counter
        let registry = ToolRegistryHandle::start();
        for n in ["w_a", "w_b", "w_c"] {
            registry.register(mcp_descriptor(n)).await.unwrap();
        }
        let invoker = Arc::new(ConcurrencyInvoker::new(20));
        let agent = agent_with(registry.clone(), invoker.clone());
        let calls = vec![
            tool_call(0, "w_a"),
            tool_call(1, "w_b"),
            tool_call(2, "w_c"),
        ];

        // WHEN the turn runs
        let order = run_turn(&agent, &invoker, &calls).await;

        // THEN results keep input order and concurrency never exceeded one
        assert_eq!(order, vec!["w_a", "w_b", "w_c"]);
        assert_eq!(invoker.peak(), 1, "writes must run sequentially");
        registry.shutdown().await;
    }

    /// An isolated read-only failure is confined to its own position and does not
    /// interrupt the rest of the turn.
    #[tokio::test]
    async fn test_readonly_failure_does_not_poison_other_calls() {
        // GIVEN three authorized read-only tools where the middle one fails
        let registry = ToolRegistryHandle::start();
        for n in ["ro_0", "ro_1", "ro_2"] {
            registry.register(ro_descriptor(n)).await.unwrap();
        }
        let invoker: Arc<dyn ToolInvoker> = Arc::new(FailingInvoker {
            failing: "ro_1".to_string(),
        });
        let agent = agent_with(registry.clone(), invoker);
        let budget = make_budget(100);
        let calls = vec![
            tool_call(0, "ro_0"),
            tool_call(1, "ro_1"),
            tool_call(2, "ro_2"),
        ];

        // WHEN the turn runs
        let (records, failures) = run_turn_full(&agent, &budget, &calls).await;

        // THEN all three records land at their positions, only the middle failed,
        //   the turn completed, and a later success reset the ordered failure count
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].tool_name, "ro_0");
        assert_eq!(records[1].tool_name, "ro_1");
        assert_eq!(records[2].tool_name, "ro_2");
        let failed = |r: &ToolCallRecord| r.output.as_deref().unwrap_or("").contains("tool error");
        assert!(failed(&records[1]), "the middle call must report a failure");
        assert!(!failed(&records[0]), "the first call must succeed");
        assert!(!failed(&records[2]), "the last call must succeed");
        assert_eq!(failures, 0, "a success after the failure resets the count");
        registry.shutdown().await;
    }

    /// The step budget is charged exactly once per tool call, whichever path
    /// (parallel read-only or sequential write) the call takes.
    #[tokio::test]
    async fn test_budget_incremented_once_per_call() {
        // GIVEN a mixed turn of seven calls (three read-only, four write)
        let registry = ToolRegistryHandle::start();
        for n in ["ro_a", "ro_b", "ro_c"] {
            registry.register(ro_descriptor(n)).await.unwrap();
        }
        for n in ["w_a", "w_b", "w_c", "w_d"] {
            registry.register(mcp_descriptor(n)).await.unwrap();
        }
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let agent = agent_with(registry.clone(), invoker);
        let budget = make_budget(100);
        let calls = vec![
            tool_call(0, "ro_a"),
            tool_call(1, "w_a"),
            tool_call(2, "ro_b"),
            tool_call(3, "w_b"),
            tool_call(4, "ro_c"),
            tool_call(5, "w_c"),
            tool_call(6, "w_d"),
        ];
        let before = budget.tool_calls_left();

        // WHEN the turn runs
        let (records, _failures) = run_turn_full(&agent, &budget, &calls).await;

        // THEN every call produced a record and the budget was charged exactly seven times
        assert_eq!(records.len(), 7);
        assert_eq!(before - budget.tool_calls_left(), 7);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_build_tool_specs_eager_includes_mcp_schemas() {
        // GIVEN a registry with a native tool and an MCP tool, eager mode
        let registry = ToolRegistryHandle::start();
        registry
            .register(apollia_tools::tools::bash_executor::BashExecutor::descriptor())
            .await
            .unwrap();
        registry
            .register(mcp_descriptor("mcp:notion/search_pages"))
            .await
            .unwrap();
        let available = vec![
            "bash_executor".to_string(),
            "mcp:notion/search_pages".to_string(),
        ];
        // WHEN build_tool_specs runs with no index (eager)
        let specs = build_tool_specs(&available, &registry, None, 20).await;
        // THEN both the native and the MCP schema are present, tool_search absent
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"bash_executor"));
        assert!(names.contains(&"mcp:notion/search_pages"));
        assert!(!names.contains(&"tool_search"));
    }

    #[tokio::test]
    async fn test_build_tool_specs_deferred_injects_tool_search() {
        // GIVEN the same registry, but deferred mode with a one-tool index
        let registry = ToolRegistryHandle::start();
        registry
            .register(apollia_tools::tools::bash_executor::BashExecutor::descriptor())
            .await
            .unwrap();
        registry
            .register(mcp_descriptor("mcp:notion/search_pages"))
            .await
            .unwrap();
        let available = vec![
            "bash_executor".to_string(),
            "mcp:notion/search_pages".to_string(),
        ];
        let index = vec![snapshot("notion", "search_pages")];
        // WHEN build_tool_specs runs with the index (deferred)
        let specs = build_tool_specs(&available, &registry, Some(&index), 20).await;
        // THEN the native tool stays, the MCP schema is gone, tool_search is present
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"bash_executor"));
        assert!(!names.contains(&"mcp:notion/search_pages"));
        assert!(names.contains(&"tool_search"));
    }

    #[tokio::test]
    async fn test_build_tool_specs_deferred_empty_index_still_has_tool_search() {
        // GIVEN a registry with only a native tool, deferred mode, empty index
        let registry = ToolRegistryHandle::start();
        registry
            .register(apollia_tools::tools::bash_executor::BashExecutor::descriptor())
            .await
            .unwrap();
        let available = vec!["bash_executor".to_string()];
        // WHEN build_tool_specs runs with an empty index
        let specs = build_tool_specs(&available, &registry, Some(&[]), 20).await;
        // THEN the native tool and tool_search are present, no panic, valid schema
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"bash_executor"));
        let ts = specs
            .iter()
            .find(|s| s.name == "tool_search")
            .expect("tool_search spec present");
        assert_eq!(ts.parameters["type"], "object");
    }

    #[tokio::test]
    async fn test_tool_search_executor_returns_connected_tool() {
        use apollia_mcp::tool_search::ToolSearchExecutor;
        use apollia_tools::executor::ToolExecutor;
        // GIVEN a tool_search executor over a notion index
        let executor = ToolSearchExecutor::new(vec![snapshot("notion", "search_pages")], 20);
        // WHEN it is invoked with a matching query
        let out = executor
            .execute(serde_json::json!({"query": "search"}))
            .await
            .unwrap();
        // THEN the returned full_name is the directly-invocable identifier
        assert_eq!(out["matches"][0]["full_name"], "mcp:notion/search_pages");
    }

    // ── PreToolUse blocking hook (loop integration) ──────────────────────

    /// Tool invoker that counts how many times a tool was actually invoked.
    struct CountingToolInvoker {
        count: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ToolInvoker for CountingToolInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(r#"{"exit_code": 0, "stdout": "ran"}"#.to_string())
        }
    }

    /// Writes an executable hook script returning the given decision JSON.
    fn write_hook_script(dir: &std::path::Path, name: &str, decision_json: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\nprintf '{decision_json}'\n"))
            .expect("write hook script");
        let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
        path.to_string_lossy().into_owned()
    }

    /// Builds a hook executor with a single PreToolUse command handler.
    fn pre_tool_use_executor(script: String) -> Arc<HookExecutor> {
        let registry = crate::hooks::HookRegistry::from_config(&apollia_core::HooksConfig {
            handlers: vec![apollia_core::HookHandlerConfig {
                events: vec![apollia_core::HookEventKind::PreToolUse],
                kind: apollia_core::HookHandlerKind::Command {
                    command: vec![script],
                },
                timeout_ms: 5_000,
            }],
        });
        Arc::new(HookExecutor::new(Arc::new(registry)))
    }

    fn bash_call_model() -> Arc<MockReActModel> {
        Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "bash_executor".into(),
                arguments: serde_json::json!({"command": "rm -rf /"}),
            }],
            final_tokens: split_tokens("done"),
            iteration: AtomicU32::new(0),
        })
    }

    /// AC-2: a deny decision blocks the invocation and records a refusal,
    /// without ever calling the tool invoker.
    #[tokio::test]
    async fn test_pretooluse_deny_blocks_invocation() {
        // GIVEN a model that emits one authorized bash_executor call
        let model = bash_call_model();
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let invoker: Arc<dyn ToolInvoker> = Arc::new(CountingToolInvoker {
            count: count.clone(),
        });
        let event_bus = make_event_bus();

        // AND a PreToolUse hook that denies every call
        let dir = tempfile::tempdir().expect("tempdir");
        let script = write_hook_script(
            dir.path(),
            "deny.sh",
            r#"{"decision":"deny","reason":"blocked by policy"}"#,
        );
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        })
        .with_hook_executor(Some(pre_tool_use_executor(script)));

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();
        let mut authorized = HashSet::new();
        authorized.insert("bash_executor".to_string());

        // WHEN execute runs to completion
        let result = agent
            .execute(
                "sess-deny",
                "msg-1",
                &RunId::new(),
                "go",
                &[],
                "",
                &[],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN the tool was never invoked and the call is recorded as refused
        let resp = result.expect("final response");
        assert_eq!(
            count.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "a denied tool call must not reach the invoker"
        );
        assert!(
            resp.tool_calls
                .iter()
                .any(|t| matches!(t.status, ToolCallStatus::Refused)),
            "the blocked call must be recorded as refused"
        );

        tool_registry.shutdown().await;
    }

    /// AC-1: an allow decision lets the invocation proceed normally.
    #[tokio::test]
    async fn test_pretooluse_allow_lets_tool_run() {
        // GIVEN a model that emits one authorized bash_executor call
        let model = bash_call_model();
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let invoker: Arc<dyn ToolInvoker> = Arc::new(CountingToolInvoker {
            count: count.clone(),
        });
        let event_bus = make_event_bus();

        // AND a PreToolUse hook that allows every call
        let dir = tempfile::tempdir().expect("tempdir");
        let script = write_hook_script(dir.path(), "allow.sh", r#"{"decision":"allow"}"#);
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: router,
            tool_registry: tool_registry.clone(),
            tool_invoker: invoker,
            event_bus,
            user_memory: None,
            a2a_invoker: None,
            todo: None,
        })
        .with_hook_executor(Some(pre_tool_use_executor(script)));

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();
        let mut authorized = HashSet::new();
        authorized.insert("bash_executor".to_string());

        // WHEN execute runs to completion
        let result = agent
            .execute(
                "sess-allow",
                "msg-1",
                &RunId::new(),
                "go",
                &[],
                "",
                &[],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
                None,
                None,
                None,
                None,
            )
            .await;

        // THEN the tool was invoked exactly once
        result.expect("final response");
        assert_eq!(
            count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "an allowed tool call must reach the invoker"
        );

        tool_registry.shutdown().await;
    }
}

#[cfg(test)]
mod verification_wire_tests {
    use super::*;
    use apollia_core::StepBudgetConfig;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc as StdArc, Mutex as StdMutex};

    /// Mock invoker driven by a fixed response sequence; counts every call.
    struct CountingInvoker {
        responses: StdArc<StdMutex<Vec<Result<CheckOutcome, String>>>>,
        call_count: StdArc<AtomicU32>,
    }

    impl CountingInvoker {
        fn with_sequence(seq: Vec<Result<CheckOutcome, String>>) -> Self {
            Self {
                responses: StdArc::new(StdMutex::new(seq)),
                call_count: StdArc::new(AtomicU32::new(0)),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl CheckInvoker for CountingInvoker {
        async fn invoke_check(&self, _command: &str) -> Result<CheckOutcome, String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut seq = self.responses.lock().unwrap_or_else(|e| e.into_inner());
            if seq.is_empty() {
                Ok(CheckOutcome {
                    exit_code: 0,
                    stderr: String::new(),
                })
            } else {
                seq.remove(0)
            }
        }
    }

    fn ok_check() -> Result<CheckOutcome, String> {
        Ok(CheckOutcome {
            exit_code: 0,
            stderr: String::new(),
        })
    }

    fn failed_check() -> Result<CheckOutcome, String> {
        Ok(CheckOutcome {
            exit_code: 1,
            stderr: "echec".into(),
        })
    }

    // supervised tier, checks pass, no retry.
    #[tokio::test]
    async fn test_supervised_checks_pass_no_retry() {
        // GIVEN a passing check and a disabled critic at the supervised tier
        let invoker = CountingInvoker::with_sequence(vec![ok_check()]);
        let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
        let critic = CriticPass::disabled();
        let budget = StepBudget::new(&StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        });
        let autonomy = AutonomyLevel::Supervised;

        // WHEN running verification with retry
        let (report, ()) = run_verification_with_retry(
            &autonomy,
            Some(&loop_),
            Some(&critic),
            &invoker,
            "objectif",
            "sortie",
            &budget,
            VERIFICATION_MAX_RETRIES,
            (),
            |(), _correction: String| async { (Ok("sortie corrigee".to_string()), ()) },
        )
        .await;

        // THEN it passes on the first run, no retry, one invocation
        let report = report.expect("report attendu pour palier supervised");
        assert!(report.passed);
        assert_eq!(report.retry_iterations, 0);
        assert_eq!(invoker.call_count(), 1);
    }

    // budget exhausted before retry, report returned cleanly.
    #[tokio::test]
    async fn test_budget_exhausted_no_retry() {
        // GIVEN a failing check and a budget with no steps left
        let budget = StepBudget::new(&StepBudgetConfig {
            max_steps: 0,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        });
        let invoker = CountingInvoker::with_sequence(vec![failed_check()]);
        let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
        let critic = CriticPass::disabled();
        let autonomy = AutonomyLevel::Supervised;

        // WHEN running verification with retry
        let (report, ()) = run_verification_with_retry(
            &autonomy,
            Some(&loop_),
            Some(&critic),
            &invoker,
            "objectif",
            "sortie",
            &budget,
            VERIFICATION_MAX_RETRIES,
            (),
            |(), _correction: String| async { (Ok("sortie".to_string()), ()) },
        )
        .await;

        // THEN no retry is attempted and a failing report is returned, not an error
        let report = report.expect("report attendu meme quand budget epuise");
        assert!(!report.passed);
        assert_eq!(report.retry_iterations, 0);
    }

    // At the assisted tier, declared checks run once, without critic or retries.
    #[tokio::test]
    async fn test_assisted_runs_declared_checks() {
        // GIVEN the assisted tier with a declared check command
        let invoker = CountingInvoker::with_sequence(vec![ok_check()]);
        let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
        let critic = CriticPass::disabled();
        let budget = StepBudget::new(&StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        });
        let autonomy = AutonomyLevel::Assisted;

        // WHEN running verification with retry
        let (report, ()) = run_verification_with_retry(
            &autonomy,
            Some(&loop_),
            Some(&critic),
            &invoker,
            "objectif",
            "sortie",
            &budget,
            VERIFICATION_MAX_RETRIES,
            (),
            |(), _correction: String| async { (Ok("sortie".to_string()), ()) },
        )
        .await;

        // THEN the declared check runs once, with no retries
        let report = report.expect("declared checks run at the assisted tier");
        assert!(report.passed);
        assert_eq!(report.retry_iterations, 0);
        assert_eq!(invoker.call_count(), 1);
    }

    // At the assisted tier with no declared checks, verification is skipped.
    #[tokio::test]
    async fn test_assisted_without_checks_skips_verification() {
        // GIVEN the assisted tier and a verification loop with no commands
        let invoker = CountingInvoker::with_sequence(vec![]);
        let loop_ = VerificationLoop::new(vec![], vec![]);
        let critic = CriticPass::disabled();
        let budget = StepBudget::new(&StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        });
        let autonomy = AutonomyLevel::Assisted;

        // WHEN running verification with retry
        let (report, ()) = run_verification_with_retry(
            &autonomy,
            Some(&loop_),
            Some(&critic),
            &invoker,
            "objectif",
            "sortie",
            &budget,
            VERIFICATION_MAX_RETRIES,
            (),
            |(), _correction: String| async { (Ok("sortie".to_string()), ()) },
        )
        .await;

        // THEN nothing runs and no report is produced
        assert!(report.is_none(), "no declared checks means no verification");
        assert_eq!(invoker.call_count(), 0);
    }

    // persistent failures stop at the retry bound.
    #[tokio::test]
    async fn test_max_retries_bounded() {
        // GIVEN checks that always fail and ample budget
        let invoker = CountingInvoker::with_sequence(vec![
            failed_check(),
            failed_check(),
            failed_check(),
            failed_check(),
        ]);
        let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
        let critic = CriticPass::disabled();
        let budget = StepBudget::new(&StepBudgetConfig {
            max_steps: 50,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        });
        let autonomy = AutonomyLevel::Supervised;

        // WHEN running verification with retry
        let (report, ()) = run_verification_with_retry(
            &autonomy,
            Some(&loop_),
            Some(&critic),
            &invoker,
            "objectif",
            "sortie",
            &budget,
            VERIFICATION_MAX_RETRIES,
            (),
            |(), _correction: String| async { (Ok("sortie".to_string()), ()) },
        )
        .await;

        // THEN exactly max_retries iterations ran (initial + 2 = 3 invocations)
        let report = report.expect("report attendu");
        assert!(!report.passed);
        assert_eq!(report.retry_iterations, VERIFICATION_MAX_RETRIES);
        assert_eq!(invoker.call_count(), VERIFICATION_MAX_RETRIES + 1);
    }

    // a failure on the first run that the retry resolves.
    #[tokio::test]
    async fn test_retry_resolves_failure() {
        // GIVEN a check that fails once then passes
        let invoker = CountingInvoker::with_sequence(vec![failed_check(), ok_check()]);
        let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
        let critic = CriticPass::disabled();
        let budget = StepBudget::new(&StepBudgetConfig {
            max_steps: 50,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        });
        let autonomy = AutonomyLevel::Supervised;
        let retry_calls = StdArc::new(AtomicU32::new(0));
        let retry_calls_inner = StdArc::clone(&retry_calls);

        // WHEN running verification with retry
        let (report, ()) = run_verification_with_retry(
            &autonomy,
            Some(&loop_),
            Some(&critic),
            &invoker,
            "objectif",
            "sortie initiale",
            &budget,
            VERIFICATION_MAX_RETRIES,
            (),
            move |(), _correction: String| {
                let counter = StdArc::clone(&retry_calls_inner);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    (Ok("sortie corrigee".to_string()), ())
                }
            },
        )
        .await;

        // THEN the retry ran once and the final report passes
        let report = report.expect("report attendu");
        assert!(report.passed);
        assert_eq!(report.retry_iterations, 1);
        assert_eq!(retry_calls.load(Ordering::SeqCst), 1);
        assert_eq!(invoker.call_count(), 2);
    }

    // correction_message embeds both failed checks and critic corrections.
    #[test]
    fn test_correction_message_contains_failures_and_corrections() {
        // GIVEN one check failure and one critic correction
        let failures = vec![CheckFailure {
            command: "cargo test".into(),
            exit_code: 1,
            stderr: "boom".into(),
        }];
        let corrections = vec![Correction {
            kind: "missing_file".into(),
            description: "fichier absent".into(),
            suggestion: "creer le fichier".into(),
        }];

        // WHEN building the correction message
        let message = correction_message(&failures, &corrections);

        // THEN it carries both pieces and the instruction wrapper
        assert!(message.contains("cargo test"));
        assert!(message.contains("boom"));
        assert!(message.contains("missing_file"));
        assert!(message.contains("creer le fichier"));
        assert!(message.contains("<verification_feedback>"));
        assert!(message.contains("Please address the issues"));
    }
}

#[cfg(test)]
mod todo_compaction_tests {
    use super::*;
    use crate::chat::todo_actor::spawn_todo_actor;
    use apollia_core::todo::{TodoItem, TodoStatus};
    use apollia_llm::types::{MessageContent, Role};
    use rusqlite::Connection;

    fn todo_handle() -> TodoHandle {
        spawn_todo_actor(Connection::open_in_memory().expect("open"), None).expect("spawn")
    }

    fn item(id: &str, content: &str, status: TodoStatus) -> TodoItem {
        TodoItem {
            id: id.into(),
            content: content.into(),
            status,
            depends_on: vec![],
        }
    }

    fn text_of(msg: &LlmChatMessage) -> String {
        match &msg.content {
            MessageContent::Text(t) => t.clone(),
            other => format!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn test_todo_injected_after_compaction() {
        // GIVEN a session with one in_progress item persisted
        let h = todo_handle();
        h.set_items("s1", vec![item("t1", "Analyser les logs", TodoStatus::InProgress)])
            .await
            .expect("seed");
        let mut messages = vec![LlmChatMessage::system("base")];

        // WHEN the post-compaction injection runs
        BuiltInChatAgent::inject_todo_after_compaction(&h, "s1", &mut messages).await;

        // THEN a user reminder carrying the item content and status is appended
        assert_eq!(messages.len(), 2);
        let last = messages.last().expect("message present");
        assert!(matches!(last.role, Role::User));
        let body = text_of(last);
        assert!(body.contains("Analyser les logs"));
        assert!(body.contains("in_progress"));
    }

    #[tokio::test]
    async fn test_no_injection_when_todo_empty() {
        // GIVEN a session with no todo items
        let h = todo_handle();
        let mut messages = vec![LlmChatMessage::system("base")];

        // WHEN the injection runs
        BuiltInChatAgent::inject_todo_after_compaction(&h, "s1", &mut messages).await;

        // THEN no message is appended
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_multiple_items_all_present_in_reminder() {
        // GIVEN a session with one in_progress, two pending, one completed
        let h = todo_handle();
        h.set_items(
            "s1",
            vec![
                item("t1", "done thing", TodoStatus::Completed),
                item("t2", "current thing", TodoStatus::InProgress),
                item("t3", "next thing", TodoStatus::Pending),
                item("t4", "later thing", TodoStatus::Pending),
            ],
        )
        .await
        .expect("seed");
        let mut messages = vec![LlmChatMessage::system("base")];

        // WHEN the injection runs
        BuiltInChatAgent::inject_todo_after_compaction(&h, "s1", &mut messages).await;

        // THEN all four items appear in creation order
        let body = text_of(messages.last().expect("message present"));
        let p1 = body.find("done thing").expect("t1 present");
        let p2 = body.find("current thing").expect("t2 present");
        let p3 = body.find("next thing").expect("t3 present");
        let p4 = body.find("later thing").expect("t4 present");
        assert!(p1 < p2 && p2 < p3 && p3 < p4);
    }

    #[tokio::test]
    async fn test_get_items_error_is_graceful() {
        // GIVEN a handle whose actor has stopped (channel closed)
        let h = todo_handle();
        h.shutdown().await;
        tokio::task::yield_now().await;
        let mut messages = vec![LlmChatMessage::system("base")];

        // WHEN the injection runs against the dead actor
        BuiltInChatAgent::inject_todo_after_compaction(&h, "s1", &mut messages).await;

        // THEN it degrades gracefully: no panic, no message appended
        assert_eq!(messages.len(), 1);
    }
}
