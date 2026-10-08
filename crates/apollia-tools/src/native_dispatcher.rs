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
use crate::source_access::SourceAccess;
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
    /// Folders the reading file and notebook tools may reach, anchor first:
    /// the folders granted to the agent ([`crate::folder_access`]).
    ///
    /// The anchor is where a relative path lands. The rest only widen what an
    /// absolute path reaches. Empty, the reading tools refuse every call with
    /// a message that says how to grant a folder.
    pub sandbox_roots: Vec<PathBuf>,
    /// Folders the writing file tools (`file_write`, `file_edit`,
    /// `notebook_edit`) may reach, anchor first: the folders granted to the
    /// agent for writing. Empty, those tools refuse every call.
    pub write_roots: Vec<PathBuf>,
    /// Agent identifier, namespaces the per-agent Python venv.
    pub agent_id: String,
    /// Root directory under which per-agent venvs are created for `python_executor`.
    pub venv_base_dir: PathBuf,
    /// Primary memory namespace (read-write). `None` omits `memory_search`,
    /// and so does a memory left unchecked in [`Self::sources`].
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
    /// `[tools] max_output_chars`: the largest tool output, in UTF-8 bytes,
    /// passed to the model; a longer one is trimmed in the middle. Use
    /// [`DEFAULT_MAX_OUTPUT_CHARS`](crate::executor::DEFAULT_MAX_OUTPUT_CHARS)
    /// when no `apollia.toml` is loaded.
    pub max_output_chars: usize,
    /// The sources the resolved agent has checked: `memory_search` is
    /// registered only when its memory is, `source_search` only when an
    /// indexable source is, and bounded to those.
    pub sources: SourceAccess,
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
    if is_active("memory_search") && cfg.sources.memory {
        if let Some(ns) = cfg.memory_namespace.as_ref() {
            executors.push(Box::new(MemorySearchTool::new(
                ns.clone(),
                cfg.memory_shared_namespaces.clone(),
                cfg.memory_base_dir.clone(),
            )));
        }
    }

    if is_active(crate::tools::source_search::SOURCE_SEARCH_TOOL) {
        if let Some(search) = cfg.sources.search.as_ref() {
            executors.push(Box::new(search.clone()));
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
    ToolDispatcher::new(executors).with_max_output_chars(cfg.max_output_chars)
}

/// Register the sandbox-bound file and notebook tools that are not explicitly
/// disabled. The reading ones are constructed against `cfg.sandbox_roots`, the
/// writing ones against `cfg.write_roots`; construction failures are logged and
/// skipped by [`push_sandbox_tool`]. A tool with no root answers every call
/// with [`NoFolderGranted`].
fn register_sandbox_tools(
    executors: &mut Vec<Box<dyn ToolExecutor>>,
    cfg: &NativeDispatcherConfig,
    is_active: &impl Fn(&str) -> bool,
) {
    let read = &cfg.sandbox_roots;
    let write = &cfg.write_roots;
    let mut push =
        |name: &'static str,
         roots: &Vec<PathBuf>,
         build: &dyn Fn(Vec<PathBuf>) -> Option<Box<dyn ToolExecutor>>| {
            if !is_active(name) {
                return;
            }
            if roots.is_empty() {
                executors.push(Box::new(NoFolderGranted { name }));
            } else if let Some(exec) = build(roots.clone()) {
                executors.push(exec);
            }
        };
    push("file_read", read, &|r| built("file_read", FileRead::new(r)));
    push("file_write", write, &|r| {
        built("file_write", FileWrite::new(r))
    });
    push("file_list", read, &|r| built("file_list", FileList::new(r)));
    push("file_edit", write, &|r| {
        built("file_edit", FileEdit::new(r))
    });
    push("file_glob", read, &|r| built("file_glob", FileGlob::new(r)));
    push("file_grep", read, &|r| built("file_grep", FileGrep::new(r)));
    push("notebook_read", read, &|r| {
        built("notebook_read", NotebookRead::new(r))
    });
    push("notebook_edit", write, &|r| {
        built("notebook_edit", NotebookEdit::new(r))
    });
}

/// A file tool of an agent granted no folder: every call is refused with a
/// message that says what to do.
pub struct NoFolderGranted {
    name: &'static str,
}

impl NoFolderGranted {
    /// The stand-in for the file tool `name`.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl ToolExecutor for NoFolderGranted {
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
                code: "no_folder_granted".to_string(),
                message: no_folder_message(self.name),
            })
        })
    }
}

/// What the agent hears when `tool` has no folder to work in.
fn no_folder_message(tool: &str) -> String {
    let what = if crate::folder_access::FOLDER_WRITE_TOOLS.contains(&tool) {
        "for writing"
    } else {
        "to read"
    };
    // The model repeats this to the person: it has to say that a folder
    // already added is one grant away, or it sends them to add it again.
    format!(
        "no folder is granted to this agent {what}: a folder already added in Connections \
         still has to be granted to this agent, on its sheet under Agents; a folder that \
         is not a connection yet is added in Connections first"
    )
}

/// The executor `result` built, boxed, or `None` after a warning.
fn built<T, E>(name: &'static str, result: Result<T, E>) -> Option<Box<dyn ToolExecutor>>
where
    T: ToolExecutor + 'static,
    E: std::fmt::Display,
{
    let mut out: Vec<Box<dyn ToolExecutor>> = Vec::new();
    push_sandbox_tool(&mut out, name, result);
    out.pop()
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
            write_roots: vec![root.clone()],
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
            max_output_chars: crate::executor::DEFAULT_MAX_OUTPUT_CHARS,
            sources: SourceAccess::default(),
        }
    }

    // GIVEN a memory namespace, with the memory checked then unchecked
    // WHEN the dispatcher is built
    // THEN memory_search is registered in the first case only
    #[cfg(feature = "memory-search")]
    #[test]
    fn memory_search_is_registered_only_when_the_memory_is_checked() {
        let mut checked = config_disabling(&[]);
        checked.memory_namespace = Some("apollia".to_string());
        checked.sources = SourceAccess::memory_only(true);
        let mut unchecked = checked.clone();
        unchecked.sources = SourceAccess::memory_only(false);

        let with = build_native_dispatcher(&checked);
        let without = build_native_dispatcher(&unchecked);

        assert!(with.tool_names().contains(&"memory_search"));
        assert!(!without.tool_names().contains(&"memory_search"));
    }

    // GIVEN an agent with a checked folder, then one with none
    // WHEN the dispatcher is built
    // THEN source_search is registered in the first case only, and a
    //      disabled source_search is not registered at all
    #[tokio::test]
    async fn source_search_is_registered_only_with_an_indexable_source_checked() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            apollia_sources::SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();
        let folder = apollia_sources::CheckedSource::Registered(apollia_sources::Source {
            id: "src-folder".to_string(),
            kind: apollia_sources::SourceKind::Folder,
            label: "Contracts".to_string(),
            group: None,
            reference: Some("folder-contracts".to_string()),
            index_state: apollia_sources::IndexState::Absent,
            added_at: "2026-10-04T00:00:00.000Z".to_string(),
        });
        let mut checked = config_disabling(&[]);
        checked.sources = SourceAccess::from_checked(&[folder], Some(&registry));
        let none = config_disabling(&[]);
        let mut disabled = config_disabling(&["source_search"]);
        disabled.sources = checked.sources.clone();

        let with = build_native_dispatcher(&checked);
        let without = build_native_dispatcher(&none);
        let off = build_native_dispatcher(&disabled);

        assert!(with.tool_names().contains(&"source_search"));
        assert!(!without.tool_names().contains(&"source_search"));
        assert!(!off.tool_names().contains(&"source_search"));
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

    /// An executor answering a text of a fixed length.
    struct LongTool(usize);

    impl ToolExecutor for LongTool {
        fn name(&self) -> &str {
            "long_tool"
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
            Box::pin(async move { Ok(serde_json::Value::String("x".repeat(self.0))) })
        }
    }

    // `[tools] max_output_chars` reaches the dispatcher the factory builds.
    #[tokio::test]
    async fn the_configured_output_limit_trims_a_longer_tool_output() {
        // GIVEN a limit of 100 in the configuration, and a tool answering 500
        // characters
        let mut cfg = config_disabling(&[]);
        cfg.max_output_chars = 100;
        let dispatcher = build_dispatcher_with(&cfg, vec![Box::new(LongTool(500))]);

        // WHEN the tool is called through the dispatcher
        let output = dispatcher
            .dispatch("long_tool", serde_json::json!({}))
            .await
            .expect("the tool runs");

        // THEN the output keeps 100 characters of the original, head and tail,
        // around the truncation marker
        let text = output.as_str().expect("a trimmed output is a string");
        let (head, rest) = text.split_once("\n\n... [").expect("the truncation marker");
        let tail = rest.split_once("] ...\n\n").expect("the marker closes").1;
        assert_eq!(head.len() + tail.len(), 100, "{text}");
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
            write_roots: vec![root.clone()],
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
            max_output_chars: crate::executor::DEFAULT_MAX_OUTPUT_CHARS,
            sources: SourceAccess::default(),
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

    #[tokio::test]
    async fn a_file_tool_without_a_granted_folder_refuses_with_a_readable_message() {
        // GIVEN an agent granted no folder at all
        let mut cfg = config_disabling(&[]);
        cfg.sandbox_roots = Vec::new();
        cfg.write_roots = Vec::new();
        let dispatcher = build_native_dispatcher(&cfg);

        // WHEN it reads and writes a file
        let read = dispatcher
            .dispatch("file_read", serde_json::json!({ "path": "notes.txt" }))
            .await;
        let write = dispatcher
            .dispatch(
                "file_write",
                serde_json::json!({ "path": "notes.txt", "content": "x" }),
            )
            .await;

        // THEN both are refused, saying which folder is missing and how to grant one
        for (result, what) in [(read, "to read"), (write, "for writing")] {
            match result {
                Err(ToolExecutionError::ExecutionFailed { code, message }) => {
                    assert_eq!(code, "no_folder_granted");
                    assert!(message.contains(what), "{message}");
                    assert!(
                        message.contains("granted to this agent, on its sheet"),
                        "{message}"
                    );
                    assert!(
                        message.contains("already added in Connections"),
                        "{message}"
                    );
                }
                other => panic!("expected a refusal, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn a_folder_granted_to_read_only_refuses_a_write_there() {
        // GIVEN an agent granted one folder, to read only
        let folder = tempfile::tempdir().expect("tempdir");
        std::fs::write(folder.path().join("notes.txt"), "kept").expect("seed file");
        let mut cfg = config_disabling(&[]);
        cfg.sandbox_roots = vec![folder.path().to_path_buf()];
        cfg.write_roots = Vec::new();
        let dispatcher = build_native_dispatcher(&cfg);

        // WHEN it reads, then writes, in that folder
        let read = dispatcher
            .dispatch("file_read", serde_json::json!({ "path": "notes.txt" }))
            .await;
        let write = dispatcher
            .dispatch(
                "file_write",
                serde_json::json!({ "path": "notes.txt", "content": "changed" }),
            )
            .await;

        // THEN the read succeeds and the write is refused, the file untouched
        assert!(read.is_ok(), "{read:?}");
        assert!(
            matches!(write, Err(ToolExecutionError::ExecutionFailed { ref code, .. }) if code == "no_folder_granted"),
            "{write:?}"
        );
        assert_eq!(
            std::fs::read_to_string(folder.path().join("notes.txt")).expect("read back"),
            "kept"
        );
    }
}
