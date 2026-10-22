//! The executor a `ToolProxy` runs a tool through, and the typed failure of
//! one call.
//!
//! Split out of `tool_proxy.rs`: the proxy keeps its Python surface, the
//! dispatcher-backed executor behind it lives here.

use std::sync::Arc;

use apollia_tools::ToolDispatcher;

/// A tool call's failure, with the two approval outcomes kept typed.
#[derive(Debug, Clone)]
pub enum ToolCallError {
    /// Any other failure, as text.
    Failed(String),
    /// The call needs an approval first.
    ApprovalRequired {
        /// Sentence shown to the human.
        prompt: String,
        /// The `approbation` payload.
        payload: serde_json::Value,
    },
    /// The operator declined the call.
    ApprovalDenied {
        /// The declined gesture.
        gesture: String,
        /// The operator's reason.
        reason: Option<String>,
    },
}
/// Trait abstracting tool execution for testability.
///
/// Concrete implementations dispatch to native tools (FileIo, BashExecutor, etc.).
/// Tests use a mock executor.
pub trait ToolExecutor: Send + Sync {
    /// Executes the named tool with the given JSON input.
    fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// Executes the named tool, keeping an approval outcome typed.
    ///
    /// The default reads every failure as text, which is right for an executor
    /// that never gates. An executor that can answer an approval outcome
    /// overrides it, so that outcome reaches the agent as a pause or a typed
    /// refusal rather than as a message to recognise.
    fn execute_typed(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolCallError> {
        self.execute(tool_name, input)
            .map_err(ToolCallError::Failed)
    }
}
/// Sync adapter that exposes an `apollia_tools::ToolDispatcher` through the
/// [`ToolExecutor`] trait consumed by [`ToolProxy`].
///
/// Python agents use the sync [`ToolExecutor`] trait, while the dispatcher is
/// async. This adapter bridges the two by calling `block_in_place` +
/// `block_on`; we're always invoked from inside the Tokio runtime because
/// `ToolProxy::call` runs on a PyO3-async future.
///
/// Per-agent instance: the wrapped dispatcher holds per-agent state
/// (sandbox root, memory namespace, ask_user pending registry).
pub struct DispatcherExecutor {
    pub(crate) dispatcher: Arc<ToolDispatcher>,
}
impl DispatcherExecutor {
    /// Wrap *dispatcher* in a sync [`ToolExecutor`] facade.
    pub fn new(dispatcher: Arc<ToolDispatcher>) -> Self {
        Self { dispatcher }
    }
}
impl ToolExecutor for DispatcherExecutor {
    fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let dispatcher = Arc::clone(&self.dispatcher);
        let name = tool_name.to_string();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current()
                .block_on(async move { dispatcher.dispatch(&name, input).await })
                .map_err(|e| e.to_string())
        })
    }

    fn execute_typed(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolCallError> {
        let dispatcher = Arc::clone(&self.dispatcher);
        let name = tool_name.to_string();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current()
                .block_on(async move { dispatcher.dispatch(&name, input).await })
                .map_err(|e| match e {
                    apollia_tools::ToolExecutionError::ApprovalRequired {
                        prompt, payload, ..
                    } => ToolCallError::ApprovalRequired { prompt, payload },
                    apollia_tools::ToolExecutionError::ApprovalDenied { gesture, reason } => {
                        ToolCallError::ApprovalDenied { gesture, reason }
                    }
                    other => ToolCallError::Failed(other.to_string()),
                })
        })
    }
}
