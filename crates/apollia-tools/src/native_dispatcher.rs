//! Native tool dispatcher factory, the single source of truth for "which native
//! tools exist" and how they're wired to per-agent context.
//!
//! The resulting [`ToolDispatcher`] is consumed by:
//! - `apollia-aip::DispatcherExecutor` (sync adapter used by Python agents)
//! - any future Rust caller that needs to invoke tools by name without going
//!   through the LLM's ReAct loop.
//!
//! Previously each embedding (CLI `AIPChatAgentRunner`, CLI `BridgeRunner`,
//! Desktop `BridgeRunner`) maintained its own `match tool_name { … }` dispatch,
//! which drifted: the CLI chat path only handled 4 tools, so `ask_user` /
//! `file_edit` / `http_fetch` / `python_executor` / `memory_search` /
//! `notebook_*` were silently unreachable for Python agents in Agent chat mode.
//! This module centralises the wiring so adding a native tool only touches one
//! place.

use std::path::PathBuf;

use apollia_core::{WebReadConfig, WebSearchConfig};

use crate::executor::{ToolDispatcher, ToolExecutor};
use crate::tools::ask_user::{AskUserExecutor, PendingUserInputs};
use crate::tools::bash_executor::BashExecutor;
use crate::tools::file_edit::FileEdit;
use crate::tools::file_glob::FileGlob;
use crate::tools::file_grep::FileGrep;
use crate::tools::file_list::FileList;
use crate::tools::file_read::FileRead;
use crate::tools::file_write::FileWrite;
use crate::tools::notebook_edit::NotebookEdit;
use crate::tools::notebook_read::NotebookRead;
use crate::tools::permission_rules::{PermissionRuleAdd, PermissionRuleList, PermissionRuleRemove};
use crate::tools::python_executor::PythonExecutor;

#[cfg(feature = "http")]
use crate::tools::http_fetch::HttpFetch;

#[cfg(feature = "memory-search")]
use crate::tools::memory_search::MemorySearchTool;

#[cfg(feature = "web-search")]
use crate::tools::web_search::WebSearch;

#[cfg(feature = "web-read")]
use crate::tools::web_read::WebRead;

/// Configuration for [`build_native_dispatcher`].
///
/// One instance per agent invocation. Executors are instantiated eagerly so
/// the dispatcher can be reused across tool calls within a single `run()`.
#[derive(Clone)]
pub struct NativeDispatcherConfig {
    /// Filesystem roots the file and notebook tools may reach, anchor first.
    ///
    /// The anchor is where a relative path lands and is created if missing. The
    /// rest come from `[filesystem] trusted_paths` and only widen what an
    /// absolute path reaches; naming a disk that is not mounted does not create
    /// it. A single-root caller passes a one-entry vector.
    pub sandbox_roots: Vec<PathBuf>,
    /// Agent identifier, namespaces the per-agent Python venv.
    pub agent_id: String,
    /// Root directory under which per-agent venvs are created for `python_executor`.
    pub venv_base_dir: PathBuf,
    /// Primary memory namespace (read-write). `None` omits `memory_search`.
    pub memory_namespace: Option<String>,
    /// Additional namespaces the agent may read.
    pub memory_shared_namespaces: Vec<String>,
    /// Root directory containing `<namespace>.db` memory files.
    pub memory_base_dir: PathBuf,
    /// Host allowlist for `http_fetch`. `None` denies all outbound HTTP.
    pub http_allowlist: Option<Vec<String>>,
    /// Pending user-input registry for `ask_user`. `None` omits the tool
    /// (task-mode agents rely on AIP `input_required` instead).
    pub pending_user_inputs: Option<PendingUserInputs>,
    /// Tool names disabled at runtime via [`crate::tool_registry::ToolRegistry`]
    /// or by static config. These are excluded from the dispatcher entirely
    /// so any agent invocation surfaces `UnknownTool`.
    pub disabled_tools: Vec<String>,
    /// Brave Search API key resolved from the credential store, used to
    /// initialise the Brave backend. When `None`, the runtime falls back to
    /// the `BRAVE_SEARCH_API_KEY` environment variable.
    pub brave_api_key: Option<String>,
    /// Operator-supplied configuration for the `web_search` tool. Drives
    /// backend selection, timeouts, and per-backend caps. Defaults to
    /// [`WebSearchConfig::default`] when no `apollia.toml` is loaded.
    pub web_search_config: WebSearchConfig,
    /// Operator-supplied configuration for the `web_read` tool. Drives the
    /// HTTP timeout, the maximum response size, and the SSRF guard toggle.
    pub web_read_config: WebReadConfig,
    /// Path to `governance.db` for the `permission_rule_*` tools. When `None`,
    /// those tools are not registered and the agent receives `UnknownTool` if it
    /// tries to invoke them.
    pub governance_db_path: Option<PathBuf>,
}

/// Build a [`ToolDispatcher`] populated with every native tool available for
/// *cfg*.
///
/// A code-execution tool whose construction fails (`python_executor` when no
/// system Python 3 exists) stays registered as an [`UnavailableTool`], so an
/// agent invoking it receives the constructor's actionable error instead of
/// `UnknownTool`. Other tools whose construction fails (file tools when the
/// sandbox root is invalid) are logged at `warn` and skipped; the dispatcher
/// returns `UnknownTool` if the agent later attempts to invoke them.
pub fn build_native_dispatcher(cfg: &NativeDispatcherConfig) -> ToolDispatcher {
    build_dispatcher_with(cfg, Vec::new())
}

/// Placeholder registered when a native tool cannot be constructed on this
/// host, so an agent invoking it receives the constructor's actionable error
/// (what is missing and how to obtain it) instead of `UnknownTool`.
pub struct UnavailableTool {
    name: &'static str,
    reason: String,
}

impl UnavailableTool {
    /// Registers `name` as unavailable, answering every call with `reason`.
    ///
    /// Public because the chat dispatcher builds its own copies of the
    /// HITL-wrapped natives and needs the same stub when one of them cannot be
    /// constructed on this host.
    pub fn new(name: &'static str, reason: String) -> Self {
        Self { name, reason }
    }
}

impl ToolExecutor for UnavailableTool {
    fn name(&self) -> &str {
        self.name
    }

    fn execute(
        &self,
        _input: serde_json::Value,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<serde_json::Value, crate::executor::ToolExecutionError>,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            Err(crate::executor::ToolExecutionError::ExecutionFailed {
                code: "tool_unavailable".to_string(),
                message: self.reason.clone(),
            })
        })
    }
}

/// Build a [`ToolDispatcher`] like [`build_native_dispatcher`] but with extra
/// executors appended after the native ones, typically MCP tools.
///
/// MCP executors are constructed by the runtime once it has resolved the
/// [`McpClientManagerHandle`](apollia_mcp::manager::McpClientManagerHandle)
/// and the active session list. They live outside `NativeDispatcherConfig`
/// because the `ToolExecutor` trait is not `Clone`, so the config struct must
/// stay `Clone`-friendly for the rest of the runtime.
pub fn build_dispatcher_with(
    cfg: &NativeDispatcherConfig,
    extra: Vec<Box<dyn ToolExecutor>>,
) -> ToolDispatcher {
    let mut executors: Vec<Box<dyn ToolExecutor>> = Vec::with_capacity(16);
    let is_active = |name: &str| !cfg.disabled_tools.iter().any(|n| n == name);

    if is_active("bash_executor") {
        executors.push(Box::new(BashExecutor::new()));
    }

    if is_active("python_executor") {
        match PythonExecutor::new(&cfg.agent_id, &cfg.venv_base_dir) {
            Ok(exec) => executors.push(Box::new(exec)),
            Err(e) => {
                // Keep the tool addressable: its descriptor is advertised to
                // the LLM independently of this dispatcher, so a silent drop
                // would surface as UnknownTool with no hint that Python is
                // the missing prerequisite. push_sandbox_tool failures are
                // candidates for the same treatment later.
                tracing::warn!(
                    error = %e,
                    detail = "registered as a stub",
                    "tool.python_executor.unavailable"
                );
                executors.push(Box::new(UnavailableTool {
                    name: "python_executor",
                    reason: e.to_string(),
                }));
            }
        }
    }

    register_sandbox_tools(&mut executors, cfg, &is_active);

    #[cfg(feature = "http")]
    {
        if is_active("http_fetch") {
            executors.push(Box::new(HttpFetch::new(cfg.http_allowlist.clone())));
        }
    }

    // Web tools are always registered in the dispatcher unless explicitly
    // disabled via the runtime registry. Per-session opt-in is handled by the
    // session tool filter (`allowed_tools`), which is the single source of
    // truth for "can this chat call web_search / web_read?". Compile out the
    // whole block with `--no-default-features` if network egress must be
    // impossible at the binary level.
    #[cfg(feature = "web-search")]
    {
        if is_active("web_search") {
            match WebSearch::from_config(&cfg.web_search_config, cfg.brave_api_key.clone()) {
                Ok(tool) => executors.push(Box::new(tool)),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        reason = "the configuration is invalid",
                        "tool.web_search.disabled"
                    );
                }
            }
        }
    }

    #[cfg(feature = "web-read")]
    {
        if is_active("web_read") {
            executors.push(Box::new(WebRead::from_config(&cfg.web_read_config)));
        }
    }

    #[cfg(feature = "memory-search")]
    if is_active("memory_search") {
        if let Some(ns) = cfg.memory_namespace.as_ref() {
            executors.push(Box::new(MemorySearchTool::new(
                ns.clone(),
                cfg.memory_shared_namespaces.clone(),
                cfg.memory_base_dir.clone(),
            )));
        }
    }

    if is_active("ask_user") {
        if let Some(pending) = cfg.pending_user_inputs.as_ref() {
            executors.push(Box::new(AskUserExecutor::new(pending)));
        }
    }

    register_governance_tools(&mut executors, cfg, &is_active);

    executors.extend(extra);
    ToolDispatcher::new(executors)
}

/// Register the sandbox-bound file and notebook tools that are not explicitly
/// disabled. Each is constructed against `cfg.sandbox_roots`; construction
/// failures are logged and skipped by [`push_sandbox_tool`].
fn register_sandbox_tools(
    executors: &mut Vec<Box<dyn ToolExecutor>>,
    cfg: &NativeDispatcherConfig,
    is_active: &impl Fn(&str) -> bool,
) {
    if is_active("file_read") {
        push_sandbox_tool(
            executors,
            "file_read",
            FileRead::new(cfg.sandbox_roots.clone()),
        );
    }
    if is_active("file_write") {
        push_sandbox_tool(
            executors,
            "file_write",
            FileWrite::new(cfg.sandbox_roots.clone()),
        );
    }
    if is_active("file_list") {
        push_sandbox_tool(
            executors,
            "file_list",
            FileList::new(cfg.sandbox_roots.clone()),
        );
    }
    if is_active("file_edit") {
        push_sandbox_tool(
            executors,
            "file_edit",
            FileEdit::new(cfg.sandbox_roots.clone()),
        );
    }
    if is_active("file_glob") {
        push_sandbox_tool(
            executors,
            "file_glob",
            FileGlob::new(cfg.sandbox_roots.clone()),
        );
    }
    if is_active("file_grep") {
        push_sandbox_tool(
            executors,
            "file_grep",
            FileGrep::new(cfg.sandbox_roots.clone()),
        );
    }
    if is_active("notebook_read") {
        push_sandbox_tool(
            executors,
            "notebook_read",
            NotebookRead::new(cfg.sandbox_roots.clone()),
        );
    }
    if is_active("notebook_edit") {
        push_sandbox_tool(
            executors,
            "notebook_edit",
            NotebookEdit::new(cfg.sandbox_roots.clone()),
        );
    }
}

/// Register the permission-governance tools when a governance database
/// path is configured and the individual tools are not disabled.
fn register_governance_tools(
    executors: &mut Vec<Box<dyn ToolExecutor>>,
    cfg: &NativeDispatcherConfig,
    is_active: &impl Fn(&str) -> bool,
) {
    // Permission governance: agent-driven tools gated by human-in-the-loop.
    let Some(db_path) = cfg.governance_db_path.as_ref() else {
        return;
    };
    if is_active("permission_rule_add") {
        executors.push(Box::new(PermissionRuleAdd::new(
            db_path.clone(),
            cfg.agent_id.clone(),
        )));
    }
    if is_active("permission_rule_remove") {
        executors.push(Box::new(PermissionRuleRemove::new(db_path.clone())));
    }
    if is_active("permission_rule_list") {
        executors.push(Box::new(PermissionRuleList::new(db_path.clone())));
    }
}

/// Push a sandbox-bound tool onto *executors*, logging a warning if
/// construction failed (invalid sandbox root, for instance).
fn push_sandbox_tool<T, E>(
    executors: &mut Vec<Box<dyn ToolExecutor>>,
    name: &'static str,
    built: Result<T, E>,
) where
    T: ToolExecutor + 'static,
    E: std::fmt::Display,
{
    match built {
        Ok(exec) => executors.push(Box::new(exec)),
        Err(e) => {
            tracing::warn!(
                tool = name,
                error = %e,
                reason = "the tool is unavailable",
                "tool.native.skipped"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::ToolExecutionError;

    #[tokio::test]
    async fn unavailable_tool_returns_actionable_error_not_unknown_tool() {
        // GIVEN a dispatcher containing an UnavailableTool carrying the
        // PythonUnavailable constructor message
        let reason = "no working Python 3 interpreter found on PATH (tried: \
                      python, py -3, python3). Install Python 3 from \
                      python.org and ensure it is on PATH."
            .to_string();
        let dispatcher = ToolDispatcher::new(vec![Box::new(UnavailableTool {
            name: "python_executor",
            reason: reason.clone(),
        })]);

        // WHEN the agent invokes the tool
        let result = dispatcher
            .dispatch("python_executor", serde_json::json!({}))
            .await;

        // THEN it receives the actionable reason, not UnknownTool
        match result {
            Err(ToolExecutionError::ExecutionFailed { code, message }) => {
                assert_eq!(code, "tool_unavailable");
                assert!(
                    message.contains("Install Python 3"),
                    "message must carry the actionable guidance, got: {message}"
                );
            }
            other => panic!("expected ExecutionFailed with tool_unavailable, got {other:?}"),
        }
    }
}
