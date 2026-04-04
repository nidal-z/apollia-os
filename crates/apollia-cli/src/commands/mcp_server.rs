//! `apollia-os mcp-server` — launch Apollia as an MCP stdio server.
//!
//! Exposes 9 native tools to external MCP clients (Claude Desktop, VS Code
//! Copilot Chat, Cursor) via stdin/stdout JSON-RPC 2.0.
//!
//! With `--with-runtime`, starts the full Apollia runtime first and adds the
//! `submit_task` tool, allowing MCP clients to delegate tasks to local agents.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_mcp::{McpServerError, McpStdioServer, SubmitTaskHandler};
use apollia_tools::executor::ToolDispatcher;
use apollia_tools::tools::bash_executor::BashExecutor;
use apollia_tools::tools::file_edit::FileEdit;
use apollia_tools::tools::file_glob::FileGlob;
use apollia_tools::tools::file_grep::FileGrep;
use apollia_tools::tools::file_list::FileList;
use apollia_tools::tools::file_read::FileRead;
use apollia_tools::tools::file_write::FileWrite;

/// Arguments for the `apollia-os mcp-server` subcommand.
#[derive(Debug, clap::Args)]
pub struct McpServerArgs {
    /// Enable the `submit_task` tool by starting the full Apollia runtime.
    ///
    /// Without this flag, only the 9 stateless native tools are exposed.
    #[arg(long, default_value_t = false)]
    pub with_runtime: bool,

    /// Sandbox root for file tools (default: user home directory).
    ///
    /// File operations are restricted to this directory tree.
    #[arg(long, value_name = "PATH")]
    pub sandbox_root: Option<PathBuf>,
}

/// Errors returned by [`run`].
#[derive(Debug, thiserror::Error)]
pub enum McpServerCommandError {
    /// Could not determine the user home directory for the default sandbox root.
    #[error("cannot determine home directory — set --sandbox-root explicitly")]
    NoHomeDir,
    /// A file tool failed to initialise with the given sandbox root.
    #[error("file tool init error: {0}")]
    FileToolInit(String),
    /// The MCP server I/O loop failed.
    #[error("MCP server error: {0}")]
    Server(#[from] McpServerError),
}

/// Run the MCP stdio server.
///
/// Builds a [`ToolDispatcher`] with the 7 native file/bash executors,
/// constructs [`McpStdioServer`], and enters the read-dispatch-write loop.
/// When `args.with_runtime` is `true`, a [`SubmitTaskHandler`] backed by the
/// Apollia runtime is injected and `submit_task` becomes available.
pub async fn run(args: &McpServerArgs) -> Result<(), McpServerCommandError> {
    let sandbox_root = match &args.sandbox_root {
        Some(p) => p.clone(),
        None => dirs_or_home().ok_or(McpServerCommandError::NoHomeDir)?,
    };

    let dispatcher = build_dispatcher(sandbox_root)?;

    let server = if args.with_runtime {
        let handler = start_runtime_submit_handler().await;
        McpStdioServer::new(dispatcher).with_submit_task(handler)
    } else {
        McpStdioServer::new(dispatcher)
    };

    server.run().await?;
    Ok(())
}

/// Construct a [`ToolDispatcher`] pre-loaded with the 7 native executors.
fn build_dispatcher(sandbox_root: PathBuf) -> Result<ToolDispatcher, McpServerCommandError> {
    let bash = BashExecutor::new();

    let file_read = FileRead::new(sandbox_root.clone())
        .map_err(|e| McpServerCommandError::FileToolInit(e.to_string()))?;
    let file_write = FileWrite::new(sandbox_root.clone())
        .map_err(|e| McpServerCommandError::FileToolInit(e.to_string()))?;
    let file_edit = FileEdit::new(sandbox_root.clone())
        .map_err(|e| McpServerCommandError::FileToolInit(e.to_string()))?;
    let file_list = FileList::new(sandbox_root.clone())
        .map_err(|e| McpServerCommandError::FileToolInit(e.to_string()))?;
    let file_glob = FileGlob::new(sandbox_root.clone())
        .map_err(|e| McpServerCommandError::FileToolInit(e.to_string()))?;
    let file_grep = FileGrep::new(sandbox_root)
        .map_err(|e| McpServerCommandError::FileToolInit(e.to_string()))?;

    Ok(ToolDispatcher::new(vec![
        Box::new(bash),
        Box::new(file_read),
        Box::new(file_write),
        Box::new(file_edit),
        Box::new(file_list),
        Box::new(file_glob),
        Box::new(file_grep),
    ]))
}

/// Runtime-backed [`SubmitTaskHandler`] implementation.
///
/// Delegates `submit_task` calls to the Apollia runtime's task router.
struct RuntimeSubmitHandler {
    router: apollia_runtime::router::TaskRouterHandle<apollia_runtime::coordinator::DynBackend>,
}

#[async_trait::async_trait]
impl SubmitTaskHandler for RuntimeSubmitHandler {
    async fn submit(&self, task: String, agent_id: String) -> Result<String, String> {
        let input = apollia_core::AIPInput {
            parts: vec![apollia_core::AIPPart::Text(apollia_core::TextPart {
                text: task,
            })],
        };
        self.router
            .submit(&agent_id, input)
            .await
            .map(|id| id.to_string())
            .map_err(|e| e.to_string())
    }
}

/// Start the Apollia runtime and return an `Arc<dyn SubmitTaskHandler>`.
///
/// Uses `init_embedded` with default configuration so the runtime data directory
/// and TCP port mirror `apollia-os start` defaults.
async fn start_runtime_submit_handler() -> Arc<dyn SubmitTaskHandler> {
    use apollia_runtime::{init_embedded, EmbeddedConfig};

    // Default config mirrors `apollia-os start` defaults.
    let config = EmbeddedConfig::default();

    match init_embedded(config) {
        Ok(handle) => Arc::new(RuntimeSubmitHandler {
            router: handle.router_handle,
        }),
        Err(e) => {
            tracing::error!(error = %e, "runtime failed to start — submit_task will be unavailable");
            // Return a handler that always fails gracefully.
            Arc::new(NoopSubmitHandler)
        }
    }
}

/// Fallback handler used when the runtime failed to start.
struct NoopSubmitHandler;

#[async_trait::async_trait]
impl SubmitTaskHandler for NoopSubmitHandler {
    async fn submit(&self, _task: String, _agent_id: String) -> Result<String, String> {
        Err("Apollia runtime is not available".to_string())
    }
}

/// Resolve the sandbox root from the user home directory.
fn dirs_or_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
