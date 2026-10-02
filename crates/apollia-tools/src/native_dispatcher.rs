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
    /// Chat session the dispatcher serves, stamped on every `ask_user` request
    /// so the inbox can open the conversation that asked. `None` for a
    /// dispatcher that serves no chat session (a task-mode agent).
    pub session_id: Option<String>,
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
    /// `[tools] python_interpreter`: an interpreter the operator chose over the
    /// bundled one, by absolute path.
    ///
    /// `None`, the default, means the interpreter Apollia ships with, which is
    /// the only configuration that depends on nothing installed on the machine.
    /// A value here is checked before it is used and dropped, with a warning,
    /// if it stops being valid.
    pub python_interpreter: Option<String>,
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

/// Whether `disabled` disables the tool `name`.
///
/// A tool is disabled when `disabled` names it. The tool of a connected account
/// (`conn:<connection>/<operation>`) is also disabled when `disabled` names its
/// operation: a bare operation name disables it for every account.
#[must_use]
pub fn is_disabled(name: &str, disabled: &[String]) -> bool {
    let operation = name
        .strip_prefix("conn:")
        .and_then(|rest| rest.rsplit_once('/'))
        .map(|(_, operation)| operation);
    disabled
        .iter()
        .any(|d| d == name || operation.is_some_and(|op| d == op))
}

/// Build a [`ToolDispatcher`] like [`build_native_dispatcher`] but with extra
/// executors appended after the native ones, typically MCP tools.
///
/// MCP executors are constructed by the runtime once it has resolved the
/// [`McpClientManagerHandle`](apollia_mcp::manager::McpClientManagerHandle)
/// and the active session list. They live outside `NativeDispatcherConfig`
/// because the `ToolExecutor` trait is not `Clone`, so the config struct must
/// stay `Clone`-friendly for the rest of the runtime.
///
/// `disabled_tools` applies to the extra executors as it does to the natives:
/// a disabled MCP or connector tool is left out, so a call to it answers
/// `UnknownTool`. An extra executor named after a native tool is the embedder's
/// replacement for that native (the chat path wraps `bash_executor` with its
/// approval guard), whose own disabled list it applies; it is kept here.
pub fn build_dispatcher_with(
    cfg: &NativeDispatcherConfig,
    extra: Vec<Box<dyn ToolExecutor>>,
) -> ToolDispatcher {
    let mut executors: Vec<Box<dyn ToolExecutor>> = Vec::with_capacity(16);
    let is_active = |name: &str| !is_disabled(name, &cfg.disabled_tools);

    if is_active("bash_executor") {
        executors.push(Box::new(BashExecutor::new()));
    }

    if is_active("python_executor") {
        match PythonExecutor::new_with_interpreter(
            &cfg.agent_id,
            &cfg.venv_base_dir,
            cfg.python_interpreter.as_deref(),
        ) {
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
            executors.push(Box::new(AskUserExecutor::new_with_session(
                pending,
                cfg.session_id.clone(),
            )));
        }
    }

    executors.extend(extra.into_iter().filter(|executor| {
        let name = executor.name();
        crate::tool_registry::NATIVE_TOOL_NAMES.contains(&name) || is_active(name)
    }));
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
    /// An executor standing for an MCP or connector tool, answering its name.
    struct NamedTool(&'static str);

    impl ToolExecutor for NamedTool {
        fn name(&self) -> &str {
            self.0
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
            Box::pin(async move { Ok(serde_json::json!({ "ran": self.0 })) })
        }
    }

    fn config_disabling(disabled: &[&str]) -> NativeDispatcherConfig {
        let root = std::env::temp_dir().join("apollia-native-dispatcher-disabled-test");
        NativeDispatcherConfig {
            sandbox_roots: vec![root.clone()],
            agent_id: "agent".to_string(),
            venv_base_dir: root.join("venvs"),
            memory_namespace: None,
            memory_shared_namespaces: Vec::new(),
            memory_base_dir: root.join("memory"),
            http_allowlist: None,
            pending_user_inputs: None,
            session_id: None,
            disabled_tools: disabled.iter().map(|s| (*s).to_string()).collect(),
            brave_api_key: None,
            web_search_config: Default::default(),
            web_read_config: Default::default(),
            python_interpreter: None,
        }
    }

    // A disabled MCP tool and a disabled connector tool are left out of the
    // dispatcher like a disabled native, and a call to them does not run.
    #[tokio::test]
    async fn a_disabled_mcp_or_connector_tool_is_not_dispatched() {
        // GIVEN an MCP tool, a connector operation and the tool of one
        // account disabled, and the tools of two accounts plus one MCP tool
        let cfg = config_disabling(&[
            "mcp:notion/create_page",
            "gmail.send",
            "conn:google:pro@example.com/gcal.create_event",
        ]);
        let extra: Vec<Box<dyn ToolExecutor>> = vec![
            Box::new(NamedTool("mcp:notion/create_page")),
            Box::new(NamedTool("conn:google:pro@example.com/gmail.send")),
            Box::new(NamedTool("conn:google:home@example.com/gmail.send")),
            Box::new(NamedTool("conn:google:pro@example.com/gcal.create_event")),
            Box::new(NamedTool("conn:google:home@example.com/gcal.create_event")),
            Box::new(NamedTool("mcp:notion/search")),
        ];

        // WHEN the dispatcher is built with them
        let dispatcher = build_dispatcher_with(&cfg, extra);

        // THEN the disabled ones are absent, the operation disabled for every
        // account, and the others are registered
        let names = dispatcher.tool_names();
        assert!(!names.contains(&"mcp:notion/create_page"), "{names:?}");
        assert!(
            !names.contains(&"conn:google:pro@example.com/gmail.send"),
            "{names:?}"
        );
        assert!(
            !names.contains(&"conn:google:home@example.com/gmail.send"),
            "{names:?}"
        );
        assert!(
            !names.contains(&"conn:google:pro@example.com/gcal.create_event"),
            "{names:?}"
        );
        assert!(
            names.contains(&"conn:google:home@example.com/gcal.create_event"),
            "{names:?}"
        );
        assert!(names.contains(&"mcp:notion/search"), "{names:?}");
        // AND a call to a disabled one is refused as unknown, never executed
        let refused = dispatcher
            .dispatch("mcp:notion/create_page", serde_json::json!({}))
            .await;
        assert!(
            matches!(refused, Err(ToolExecutionError::UnknownTool { .. })),
            "{refused:?}"
        );
        let ran = dispatcher
            .dispatch("mcp:notion/search", serde_json::json!({}))
            .await
            .expect("enabled tool runs");
        assert_eq!(ran["ran"], "mcp:notion/search");
    }

    // An extra executor replacing a native keeps the embedder's own filter.
    #[test]
    fn an_extra_executor_named_after_a_native_is_kept() {
        // GIVEN the plain bash_executor disabled so a wrapped one replaces it
        let cfg = config_disabling(&["bash_executor"]);
        let extra: Vec<Box<dyn ToolExecutor>> = vec![Box::new(NamedTool("bash_executor"))];

        // WHEN the dispatcher is built
        let dispatcher = build_dispatcher_with(&cfg, extra);

        // THEN exactly one bash_executor is registered: the replacement
        let count = dispatcher
            .tool_names()
            .iter()
            .filter(|n| **n == "bash_executor")
            .count();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn ask_user_request_carries_the_session_the_dispatcher_serves() {
        // GIVEN a dispatcher built for one chat session, with the ask_user
        // registry the inbox drains
        let pending = crate::tools::ask_user::PendingUserInputs::new();
        let root = std::env::temp_dir().join("apollia-native-dispatcher-session-test");
        let cfg = NativeDispatcherConfig {
            sandbox_roots: vec![root.clone()],
            agent_id: "apollia:chat:s1".to_string(),
            venv_base_dir: root.join("venvs"),
            memory_namespace: None,
            memory_shared_namespaces: Vec::new(),
            memory_base_dir: root.join("memory"),
            http_allowlist: None,
            pending_user_inputs: Some(pending.clone()),
            session_id: Some("s1".to_string()),
            disabled_tools: Vec::new(),
            brave_api_key: None,
            web_search_config: Default::default(),
            web_read_config: Default::default(),
            python_interpreter: None,
        };
        let dispatcher = std::sync::Arc::new(build_native_dispatcher(&cfg));
        assert!(
            dispatcher.tool_names().contains(&"ask_user"),
            "ask_user is registered when a pending registry is given"
        );

        // WHEN the agent asks the user a question through that dispatcher
        // (the call blocks until an answer, so it runs on its own task; a
        // call that returns before the registry saw the question is a defect
        // of its own, reported with the tool's answer)
        let asking = dispatcher.clone();
        let mut call = tokio::spawn(async move {
            asking
                .dispatch(
                    "ask_user",
                    serde_json::json!({
                        "questions": [{ "id": "q1", "question": "Your name?", "type": "open" }]
                    }),
                )
                .await
        });
        // No deadline is armed: either the registry receives the question, or
        // the call answers first, which is the failure this races against.
        let registered = tokio::select! {
            outcome = &mut call => panic!("ask_user answered before registering: {outcome:?}"),
            next = pending.next_pending() => next,
        };
        let (_request_id, request) = registered.expect("the registry hands the question back");

        // THEN the registered request names the session, which is what lets
        // the inbox open the conversation that asked
        assert_eq!(request.session_id.as_deref(), Some("s1"));
        call.abort();
    }
}
