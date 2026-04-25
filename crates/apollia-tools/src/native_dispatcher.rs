//! Native tool dispatcher factory — single source of truth for "which native
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
/// One instance per agent invocation — executors are instantiated eagerly so
/// the dispatcher can be reused across tool calls within a single `run()`.
#[derive(Clone)]
pub struct NativeDispatcherConfig {
    /// Filesystem sandbox root for file and notebook tools.
    pub sandbox_root: PathBuf,
    /// Agent identifier — namespaces the per-agent Python venv.
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
}

/// Build a [`ToolDispatcher`] populated with every native tool available for
/// *cfg*.
///
/// Tools whose construction fails (e.g. `python_executor` when `python3` is
/// missing, file tools when the sandbox root is invalid) are logged at `warn`
/// and skipped. The dispatcher returns `UnknownTool` if the agent later
/// attempts to invoke them, which surfaces as a clear error to the caller.
pub fn build_native_dispatcher(cfg: &NativeDispatcherConfig) -> ToolDispatcher {
    let mut executors: Vec<Box<dyn ToolExecutor>> = Vec::with_capacity(16);

    executors.push(Box::new(BashExecutor::new()));

    match PythonExecutor::new(&cfg.agent_id, &cfg.venv_base_dir) {
        Ok(exec) => executors.push(Box::new(exec)),
        Err(e) => tracing::warn!(error = %e, "python_executor unavailable — skipped"),
    }

    push_sandbox_tool(
        &mut executors,
        "file_read",
        FileRead::new(cfg.sandbox_root.clone()),
    );
    push_sandbox_tool(
        &mut executors,
        "file_write",
        FileWrite::new(cfg.sandbox_root.clone()),
    );
    push_sandbox_tool(
        &mut executors,
        "file_list",
        FileList::new(cfg.sandbox_root.clone()),
    );
    push_sandbox_tool(
        &mut executors,
        "file_edit",
        FileEdit::new(cfg.sandbox_root.clone()),
    );
    push_sandbox_tool(
        &mut executors,
        "file_glob",
        FileGlob::new(cfg.sandbox_root.clone()),
    );
    push_sandbox_tool(
        &mut executors,
        "file_grep",
        FileGrep::new(cfg.sandbox_root.clone()),
    );
    push_sandbox_tool(
        &mut executors,
        "notebook_read",
        NotebookRead::new(cfg.sandbox_root.clone()),
    );
    push_sandbox_tool(
        &mut executors,
        "notebook_edit",
        NotebookEdit::new(cfg.sandbox_root.clone()),
    );

    #[cfg(feature = "http")]
    {
        executors.push(Box::new(HttpFetch::new(cfg.http_allowlist.clone())));
    }

    // Web tools are always registered in the dispatcher. Per-session opt-in is
    // handled by the session tool filter (`allowed_tools`), which is the single
    // source of truth for "can this chat call web_search / web_read?". Compile
    // out the whole block with `--no-default-features` if network egress must
    // be impossible at the binary level.
    #[cfg(feature = "web-search")]
    {
        executors.push(Box::new(WebSearch::with_default_backends()));
    }

    #[cfg(feature = "web-read")]
    {
        executors.push(Box::new(WebRead::new()));
    }

    #[cfg(feature = "memory-search")]
    if let Some(ns) = cfg.memory_namespace.as_ref() {
        executors.push(Box::new(MemorySearchTool::new(
            ns.clone(),
            cfg.memory_shared_namespaces.clone(),
            cfg.memory_base_dir.clone(),
        )));
    }

    if let Some(pending) = cfg.pending_user_inputs.as_ref() {
        executors.push(Box::new(AskUserExecutor::new(pending)));
    }

    ToolDispatcher::new(executors)
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
        Err(e) => tracing::warn!(tool = name, error = %e, "native tool unavailable — skipped"),
    }
}
