//! BuiltInChatAgent, Rust-native ReAct loop for Chat Libre mode.
//!
//! Implements the core reasoning loop: LLM → tool call → approval → result → LLM.
//! Protected by [`StepBudget`] (Principle #7) and integrated with the HITL
//! approval flow via [`PendingChatApprovals`].
//!
//! Uses `LlmRouter.stream()` for token-by-token streaming.
//! Each token emits a `ChatToken` RuntimeEvent on the EventBus so the SSE
//! stream can forward it to the client in real time.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tracing::{info, warn};

use apollia_core::{ORIAConfig, RuntimeEvent};
use apollia_llm::types::{
    ChatMessage as LlmChatMessage, CompletionRequest, StreamChunk, TokenUsage, ToolCall, ToolSpec,
};
use apollia_llm::{LlmRouter, MetaOrchestratorHandle, ObservabilityConfig, ToolInvoker};
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::budget::StepBudget;
use apollia_oria::context_manager::ContextManager;
use apollia_tools::ToolRegistryHandle;

use super::types::{
    ApprovalTimeoutParams, ChatError, ChatMessage, ChatRole, PendingChatApprovals, ToolCallRecord,
    ToolCallStatus, ToolDecision,
};
use crate::a2a::A2AInvoker;
use crate::chat::a2a_tools::generate_a2a_tool_specs;
use crate::eventbus::EventBusSender;

/// Default timeout for chat tool approval requests (5 minutes).
const CHAT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// Default number of recent messages in the sliding context window.
pub const DEFAULT_CONTEXT_WINDOW_SIZE: usize = 20;

// ─────────────────────────────────────────────
// NativeChatToolInvoker, production tool execution
// ─────────────────────────────────────────────

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
/// It is resolved once per session from the project's `workspace_path` (ADR-069).
///
/// When HITL filesystem support is enabled (via [`NativeChatToolInvoker::with_hitl_support`]),
/// write operations are classified by risk level before execution. Operations with
/// `RiskLevel::Medium` or higher suspend the tool call and wait for user approval
/// via `HitlFilesystemModal` in the desktop UI.
pub struct NativeChatToolInvoker {
    // ADR-096 Phase 4, the fields below used to back the hardcoded
    // `invoke_*` fast path. With the convergence, all tools (including
    // HITL-sensitive ones) flow through `fallback_dispatcher` and the
    // executors carry their own per-session context. The fields are
    // retained for backward compatibility with existing builder methods
    // (`with_hitl_support`, `with_ask_user_support`, …); their values are
    // ignored by `invoke()`. Will be removed in a follow-up refactor.
    #[allow(dead_code)]
    sandbox_root: std::path::PathBuf,
    /// Original workspace path for risk classification (may differ from sandbox_root
    /// when sandbox_root has been resolved via fallback).
    workspace_path: Option<std::path::PathBuf>,
    /// EventBus sender for emitting `HitlFilesystemRequired` events.
    #[allow(dead_code)]
    event_bus: Option<crate::eventbus::EventBusSender>,
    /// Pending filesystem HITL approvals store.
    #[allow(dead_code)]
    pending_fs: Option<super::types::PendingFilesystemApprovals>,
    /// Session-level filesystem allow rules (shared Arc, not persisted).
    #[allow(dead_code)]
    fs_allow_rules: Option<std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>>,
    /// Session identifier for HITL events.
    #[allow(dead_code)]
    session_id: Option<String>,
    /// Filesystem risk configuration (path lists for system/credential paths).
    #[allow(dead_code)]
    risk_config: apollia_core::FilesystemRiskConfig,
    /// Pending user input registry for the `ask_user` tool.
    #[allow(dead_code)]
    pending_user_inputs: Option<apollia_tools::tools::ask_user::PendingUserInputs>,
    /// Generic fallback for any tool that isn't in the hardcoded native
    /// match. When present, MCP + connector + future-tool calls are all
    /// resolved through a single [`ToolDispatcher`] wrapped in
    /// [`apollia_tools::dispatcher_invoker::DispatcherToolInvoker`], or
    /// any provider-specific [`ToolInvoker`] (e.g.
    /// `GoogleChatToolInvoker`). This is the convergence path that
    /// replaces the per-family special-case fields previously bolted on
    /// the invoker (cf. ADR-098).
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
    /// ADR-096 Phase 4, superseded by [`crate::chat::native_wrappers::HitlFilesystemGuard`].
    #[allow(dead_code)]
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

        // Safe and Low → no friction needed.
        if level < RiskLevel::Medium {
            return Ok(());
        }

        // Check session allow rules (AC-5).
        let rule_key = format!("{}:{}", op.as_str(), level.as_str());
        if let Some(ref rules) = self.fs_allow_rules {
            let guard = rules.lock().expect("fs_allow_rules lock poisoned");
            if guard.contains(&rule_key) {
                return Ok(());
            }
        }

        // No pending store → fallback approve (invoker running without HITL support).
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
    #[allow(dead_code)] // ADR-096 P4, replaced by HitlFilesystemGuard(BashExecutor)
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

    // `file_read` migrated to the shared ToolDispatcher (ADR-096 Phase 2) -
    // the executor is registered by `chat::manager::resolve_workspace_for_session`
    // and reached via `fallback_dispatcher`. No HITL inline → safe to leave
    // the dispatcher's permission engine in charge.

    /// Execute `file_write` with the given JSON arguments.
    #[allow(dead_code)] // ADR-096 P4, replaced by HitlFilesystemGuard(FileWrite)
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

    // `file_list` migrated, see Phase 2 ADR-096. Reached via fallback dispatcher.

    /// Execute `file_edit` with the given JSON arguments.
    #[allow(dead_code)] // ADR-096 P4, replaced by HitlFilesystemGuard(FileEdit)
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

    // `file_glob` + `file_grep` migrated, see Phase 2 ADR-096.
    // Both reached via fallback dispatcher.

    /// Execute `http_fetch` with the given JSON arguments.
    ///
    /// In libre chat mode, the URL's hostname is dynamically added to the allowlist
    /// since the user explicitly enabled this tool and tool calls are HITL-approved.
    #[allow(dead_code)] // ADR-096 P4, replaced by DynamicAllowlistHttpFetch
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
    #[allow(dead_code)] // ADR-096 P4, replaced by HitlFilesystemGuard(PythonExecutor)
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
    /// FTS5 full-text search. The namespace is fixed to `"user"` in chat libre mode
    ///, agents have their own namespaced databases.
    #[allow(dead_code)] // ADR-096 P4, dispatcher MemorySearchTool with per-session namespace
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

    // `notebook_read` migrated, see Phase 2 ADR-096.

    /// Execute `notebook_edit` with the given JSON arguments.
    ///
    /// Applies a sequence of atomic cell operations to a Jupyter `.ipynb` notebook,
    /// writing the modified notebook back to disk. Only nbformat v4 is supported.
    #[allow(dead_code)] // ADR-096 P4, replaced by HitlFilesystemGuard(NotebookEdit)
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
    #[allow(dead_code)] // ADR-096 P4, AskUserExecutor in dispatcher with session_id
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

    // `web_search` + `web_read` migrated to the shared ToolDispatcher in
    // ADR-096 Phase 3, see `chat::manager::resolve_workspace_for_session`
    // for the executor wiring. The dispatcher reads the operator's Brave
    // key + `apollia.toml` web cfg, so Chat Libre, Agent mode and Triggers
    // now share the same backend priority and SSRF settings.
}

#[async_trait::async_trait]
impl ToolInvoker for NativeChatToolInvoker {
    async fn invoke(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, String> {
        // ADR-096 Phase 4, full convergence. Every native goes through
        // the `fallback_dispatcher`: HITL-sensitive ones wrapped in
        // `HitlFilesystemGuard`, `http_fetch` via `DynamicAllowlistHttpFetch`,
        // everything else as stock executors. No fast path, no special
        // cases, single permission engine + audit trail path across
        // Chat Libre / Chat Agent / Triggers.
        match self.fallback_dispatcher.as_ref() {
            Some(invoker) => invoker.invoke(tool_name, arguments).await,
            None => Err(format!(
                "unknown tool: {tool_name} \
                 (no dispatcher attached — invoker built outside chat manager)"
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
dans ta liste — vérifie d'abord ta liste avant de déclarer une capacité absente.

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
feuille, présentation ou dossier par son **titre** sans fournir d'ID, **NE DEMANDE PAS l'ID** — \
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

## Pattern d'enchaînement obligatoire — Google par titre

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
l'utilisateur est un échec — tu as les outils pour le résoudre seul.**
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
}

/// Dependencies required to construct a [`BuiltInChatAgent`].
pub struct BuiltInChatAgentDeps {
    pub llm_router: Arc<LlmRouter>,
    pub tool_registry: ToolRegistryHandle,
    pub tool_invoker: Arc<dyn ToolInvoker>,
    pub event_bus: EventBusSender,
    pub user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    pub a2a_invoker: Option<Arc<A2AInvoker>>,
}

/// Rust-native chat agent implementing a ReAct loop for Chat Libre mode.
///
/// Stateless, all mutable state is passed as parameters to [`execute`](Self::execute).
/// Tool execution is delegated to a [`ToolInvoker`] (ADR-015 pattern).
pub struct BuiltInChatAgent {
    /// LLM router for completion calls.
    llm_router: Arc<LlmRouter>,
    /// Tool registry for resolving tool descriptors into LLM-compatible specs.
    tool_registry: ToolRegistryHandle,
    /// Tool invoker for actual tool execution (ADR-015).
    tool_invoker: Arc<dyn ToolInvoker>,
    /// Event bus for emitting chat lifecycle events.
    event_bus: EventBusSender,
    /// Optional user memory repository for injecting user context into the system prompt.
    user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Optional A2A invoker for discovering worker agent skills as virtual tools.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Gestionnaire de fenêtre de contexte, compacte `llm_messages` dans la boucle ReAct
    /// quand les messages accumulés dépassent le seuil de la fenêtre du modèle.
    context_manager: ContextManager,
    /// Optional handle to the `MetaLlmOrchestrator`, used to produce the
    /// `ToolCallRationale` narrated before each tool execution.
    /// Absent by default for backward compatibility; injected by the manager
    /// when the "Explain tool calls" main toggle is active.
    meta_handle: Option<MetaOrchestratorHandle>,
    /// Workspace directory injected into the system prompt so the LLM knows its
    /// effective working directory (project workspace or ~/.apollia/ for free chat).
    workspace_path: Option<std::path::PathBuf>,
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
struct ToolCallContextIds<'a> {
    session_id: &'a str,
    message_id: &'a str,
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
        }
    }

    /// Set the workspace path for this agent (used in system prompt and bash CWD).
    pub fn with_workspace_path(mut self, path: Option<std::path::PathBuf>) -> Self {
        self.workspace_path = path;
        self
    }

    /// Attache un `MetaOrchestratorHandle` pour générer les `ToolCallRationale`
    ///. Noop si `None`.
    pub fn with_meta_handle(mut self, handle: Option<MetaOrchestratorHandle>) -> Self {
        self.meta_handle = handle;
        self
    }

    /// Build the effective system prompt with optional user memory injection.
    ///
    /// Prepends the authoritative temporal/environment block (ADR-096 Step 0)
    /// at the **top** of the prompt so the LLM treats current date + time +
    /// timezone as ground truth, not as one fact among its priors. Then
    /// appends the user persona block when configured.
    pub fn build_system_prompt(&self, base_prompt: &str) -> String {
        let mut prompt = apollia_core::temporal_context::prepend_temporal_context(base_prompt);

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

        prompt
    }

    /// Execute a complete exchange: user message → LLM stream → tool calls → response.
    ///
    /// Uses `LlmRouter.stream()` to produce tokens one by one, emitting a
    /// [`RuntimeEvent::ChatToken`] for each token received. The ReAct loop
    /// continues until the LLM produces a final text response (no tool calls)
    /// or the [`StepBudget`] is exhausted.
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
        user_message: &str,
        history: &[ChatMessage],
        system_prompt: &str,
        available_tools: &[String],
        authorized_tools: &HashSet<String>,
        pending_approvals: &PendingChatApprovals,
        budget: &StepBudget,
        summary: Option<&str>,
        context_window_size: usize,
    ) -> Result<ChatAgentResponse, ChatError> {
        let base_prompt = if system_prompt.is_empty() {
            DEFAULT_SYSTEM_PROMPT
        } else {
            system_prompt
        };
        let effective_prompt = self.build_system_prompt(base_prompt);

        let mut tool_specs = build_tool_specs(available_tools, &self.tool_registry).await;
        if let Some(ref a2a) = self.a2a_invoker {
            tool_specs.extend(generate_a2a_tool_specs(a2a).await);
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

        loop {
            // Principle #7, budget check before every LLM call
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
            self.maybe_compact_context(&mut llm_messages, session_id).await;

            let request = CompletionRequest {
                messages: llm_messages.clone(),
                tools: tool_specs.clone(),
                ..Default::default()
            };

            // Emit ChatResponseStarted before the first token
            let _ = self.event_bus.send(RuntimeEvent::ChatResponseStarted {
                session_id: session_id.to_string(),
                message_id: message_id.to_string(),
            });

            // Use stream() instead of complete()
            let stream = self
                .llm_router
                .stream_with_observability(None, request, &obs)
                .await
                .map_err(|e| ChatError::InternalError(e.to_string()))?;

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
                        },
                    ));
                }
                Ok(tool_calls) => {
                    self.record_tool_turn(
                        RecordTurnInput {
                            accumulated_text: &accumulated_text,
                            tool_calls: &tool_calls,
                            budget,
                            ids: ToolCallContextIds {
                                session_id,
                                message_id,
                                pending_approvals,
                            },
                        },
                        &mut reasoning_fragments,
                        &mut llm_messages,
                        &mut acc,
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
        } = ctx;
        // Extract thinking trace before stripping.
        let final_thinking = Self::extract_think_blocks(accumulated_text);
        let clean = Self::strip_think_blocks(accumulated_text);
        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: clean.clone(),
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
        }
    }

    /// Record one ReAct turn that produced tool calls: capture reasoning,
    /// append the assistant message, and dispatch each tool call.
    async fn record_tool_turn(
        &self,
        input: RecordTurnInput<'_>,
        reasoning_fragments: &mut Vec<String>,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
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

        for call in tool_calls {
            budget.increment_tool_calls();
            self.process_tool_call(
                ToolCallContext {
                    session_id: ids.session_id,
                    message_id: ids.message_id,
                    call,
                    pending_approvals: ids.pending_approvals,
                },
                llm_messages,
                acc,
            )
            .await;
        }
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
        });

        ChatAgentResponse {
            content,
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace: None,
        }
    }

    /// Compact the LLM message buffer in place when it approaches the context
    /// limit, emitting [`RuntimeEvent::ContextCompacted`] on success.
    async fn maybe_compact_context(&self, llm_messages: &mut Vec<LlmChatMessage>, session_id: &str) {
        let (compacted, was_compacted) = self
            .context_manager
            .maybe_compact(llm_messages, &self.llm_router)
            .await;
        if !was_compacted {
            return;
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

        // If no structured ToolCall chunks were received, attempt to parse
        // tool calls from the accumulated text. Local GGUF models (llama.cpp)
        // often emit tool calls as raw text like `<tool_call>{"name":...}</tool_call>`
        // rather than as structured stream events.
        if tool_calls.is_empty() {
            let parsed = Self::parse_tool_calls_from_text(accumulated_text);
            tracing::debug!(
                text_len = accumulated_text.len(),
                has_tool_tag = accumulated_text.contains("<tool_call>"),
                parsed_count = parsed.len(),
                "post-stream tool call text parsing"
            );
            if !parsed.is_empty() {
                // Strip the tool_call tags from the text shown to the user.
                let cleaned = Self::strip_tool_call_tags(accumulated_text);
                accumulated_text.clear();
                accumulated_text.push_str(&cleaned);
                return Ok(parsed);
            }
        }

        Ok(tool_calls)
    }

    /// Parses tool calls emitted as raw text by local models.
    ///
    /// Supports the common format used by Qwen3 and other GGUF models:
    /// `<tool_call>\n{"name": "...", "arguments": {...}}\n</tool_call>`
    ///
    /// The JSON may span multiple lines and contain nested braces.
    fn parse_tool_calls_from_text(text: &str) -> Vec<ToolCall> {
        let mut calls = Vec::new();
        let tag_open = "<tool_call>";
        let tag_close = "</tool_call>";

        let mut search_from = 0;
        while let Some(start) = text[search_from..].find(tag_open) {
            let json_start = search_from + start + tag_open.len();
            if let Some(end) = text[json_start..].find(tag_close) {
                let json_str = text[json_start..json_start + end].trim();
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(call) = Self::json_to_tool_call(&parsed, calls.len()) {
                        tracing::info!(
                            tool = %call.name,
                            "parsed tool call from text output"
                        );
                        calls.push(call);
                    }
                }
                search_from = json_start + end + tag_close.len();
            } else {
                break;
            }
        }

        calls
    }

    /// Converts a JSON value to a `ToolCall` if it has the expected shape.
    fn json_to_tool_call(value: &serde_json::Value, index: usize) -> Option<ToolCall> {
        let name = value.get("name")?.as_str()?;
        let arguments = value
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        Some(ToolCall {
            id: format!("call_{index}"),
            name: name.to_string(),
            arguments,
        })
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

    /// Strips `<tool_call>...</tool_call>` tags from text.
    fn strip_tool_call_tags(text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        let tag_open = "<tool_call>";
        let tag_close = "</tool_call>";
        let mut cursor = 0;

        while let Some(start) = text[cursor..].find(tag_open) {
            result.push_str(&text[cursor..cursor + start]);
            let after_open = cursor + start + tag_open.len();
            if let Some(end) = text[after_open..].find(tag_close) {
                cursor = after_open + end + tag_close.len();
            } else {
                // No closing tag, keep the rest as-is
                cursor += start;
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

        // Success path: only flag if heuristic fires (no schema validators
        // wired up yet, that comes with / per-tool registry).
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
    async fn process_tool_call(
        &self,
        ctx: ToolCallContext<'_>,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) {
        let ToolCallContext {
            session_id,
            message_id,
            call,
            pending_approvals,
        } = ctx;

        if acc.authorized.contains(&call.name) {
            let (record, tool_result) =
                self.execute_tool_call(session_id, message_id, call).await;
            llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
            acc.all_tool_calls.push(record);
            return;
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
        .await;
    }

    /// Apply the operator's HITL decision for an unauthorized tool call.
    async fn apply_tool_decision(
        &self,
        target: ToolExecTarget<'_>,
        decision: ToolDecision,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) {
        let ToolExecTarget {
            session_id,
            message_id,
            call,
        } = target;
        match decision {
            ToolDecision::Accept => {
                let (record, tool_result) =
                    self.execute_tool_call(session_id, message_id, call).await;
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                acc.all_tool_calls.push(record);
            }
            ToolDecision::AlwaysAccept { .. } => {
                acc.authorized.insert(call.name.clone());
                acc.newly_authorized.push(call.name.clone());
                let (record, tool_result) =
                    self.execute_tool_call(session_id, message_id, call).await;
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                acc.all_tool_calls.push(record);
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
            }
        }
    }

    async fn execute_tool_call(
        &self,
        session_id: &str,
        message_id: &str,
        call: &apollia_llm::types::ToolCall,
    ) -> (ToolCallRecord, String) {
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

        // ── Static analysis (always-on) ─────────────────────────────────────
        // Always-on : run the static error classifier (on failure) and the
        // hallucination heuristic (on every output). Opt-in: when the
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

        (record, llm_output)
    }
}

/// Build LLM messages from system prompt, chat history, and current user message.
///
/// Applies a sliding window over history: only the last `context_window_size`
/// messages are included. When a conversation summary is available, it is
/// injected as a system message between the system prompt and the windowed
/// history to preserve context from older messages.
///
/// Message order: system prompt → [summary] → windowed history → user message.
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
async fn build_tool_specs(
    available_tools: &[String],
    tool_registry: &ToolRegistryHandle,
) -> Vec<ToolSpec> {
    let mut specs = Vec::with_capacity(available_tools.len());
    for name in available_tools {
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
    specs
}

/// Truncate a string to a maximum length, appending "..." if truncated.
fn truncate_preview(s: &str) -> String {
    truncate_to(s, PREVIEW_MAX_LEN)
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
        "{truncated}\n\n[Output truncated — {total} chars total. \
         Refine the command to produce less output.]",
        total = s.len()
    )
}

/// Compact the `stdout` field of a JSON tool result, prioritizing user-space
/// lines. Returns `None` when `s` is not the expected JSON shape.
fn compact_json_stdout(s: &str) -> Option<String> {
    let mut val = serde_json::from_str::<serde_json::Value>(s).ok()?;
    let stdout = val.get("stdout").and_then(|v| v.as_str()).map(String::from)?;

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
            "{result}\n\n[Output filtered — showing {kept}/{total} lines, \
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
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute with a simple user message
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                "Salut",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
            )
            .await;

        // THEN response contains the text, no tool calls
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Bonjour !");
        assert!(resp.tool_calls.is_empty());
        assert!(resp.newly_authorized.is_empty());

        tool_registry.shutdown().await;
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
                "Execute echo",
                &[],
                "Tu es un assistant.",
                &["bash_executor".to_string()],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
                "Read file",
                &[],
                "assistant",
                &["file_read".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
                "Read",
                &[],
                "assistant",
                &["file_read".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
                "Read",
                &[],
                "assistant",
                &["file_read".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
                "Loop",
                &[],
                "assistant",
                &["bash_executor".to_string()],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
        let agent =
            BuiltInChatAgent::new(BuiltInChatAgentDeps {
                llm_router: router,
                tool_registry: tool_registry.clone(),
                tool_invoker: invoker,
                event_bus: event_tx,
                user_memory: None,
                a2a_invoker: None,
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
                "Go",
                &[],
                "prompt",
                &["bash".to_string()],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
        let agent =
            BuiltInChatAgent::new(BuiltInChatAgentDeps {
                llm_router: router,
                tool_registry: tool_registry.clone(),
                tool_invoker: invoker,
                event_bus: event_tx,
                user_memory: None,
                a2a_invoker: None,
            });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                "Salut",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
        });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                "test",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
        let agent =
            BuiltInChatAgent::new(BuiltInChatAgentDeps {
                llm_router: router,
                tool_registry: tool_registry.clone(),
                tool_invoker: invoker,
                event_bus: event_tx,
                user_memory: None,
                a2a_invoker: None,
            });

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                "test",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
        let agent =
            BuiltInChatAgent::new(BuiltInChatAgentDeps {
                llm_router: router,
                tool_registry: tool_registry.clone(),
                tool_invoker: invoker,
                event_bus: event_tx,
                user_memory: None,
                a2a_invoker: None,
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
                "lis le fichier",
                &[],
                "",
                &["file_read".to_string()],
                &authorized,
                &approvals,
                &budget,
                None,
                DEFAULT_CONTEXT_WINDOW_SIZE,
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
        // are ignored, storage is flat under ADR-087.
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
        let agent =
            BuiltInChatAgent::new(BuiltInChatAgentDeps {
                llm_router: router,
                tool_registry,
                tool_invoker: invoker,
                event_bus,
                user_memory: Some(repo),
                a2a_invoker: None,
            });

        // WHEN building the system prompt
        let prompt = agent.build_system_prompt("Base prompt.");

        // THEN the prompt opens with the authoritative environment block
        // (ADR-096 Step 0, temporal context now leads the prompt) and
        // still carries the base prompt + user persona section.
        assert!(prompt.starts_with("## CURRENT ENVIRONMENT"));
        assert!(prompt.contains("Base prompt."));
        assert!(prompt.contains("## User Persona"));
        assert!(prompt.contains("francais"));
        assert!(prompt.contains("markdown"));
        assert!(prompt.contains("apollia"));
    }

    #[tokio::test]
    async fn test_build_system_prompt_with_empty_user_memory() {
        // GIVEN a BuiltInChatAgent with an empty user memory repository
        let repo = make_user_memory_repo(&[]);
        let router = make_router(Arc::new(MockStopModel::with_content("ok")));
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent =
            BuiltInChatAgent::new(BuiltInChatAgentDeps {
                llm_router: router,
                tool_registry,
                tool_invoker: invoker,
                event_bus,
                user_memory: Some(repo),
                a2a_invoker: None,
            });

        // WHEN building the system prompt
        let prompt = agent.build_system_prompt("Base prompt.");

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
        });

        // WHEN building the system prompt
        let prompt = agent.build_system_prompt("Base prompt.");

        // THEN the prompt does NOT contain the user persona block
        assert!(!prompt.contains("User Persona"));
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
}
