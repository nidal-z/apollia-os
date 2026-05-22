//! Adapter bridging [`ToolDispatcher`] (executor-based, JSON-typed) and
//! [`apollia_llm::ToolInvoker`] (streaming-friendly, string-typed).
//!
//! Lets the same `ToolDispatcher` instance serve both:
//! - **Python agents** (Agent mode + Triggers) → already via
//!   `DispatcherExecutor` in `apollia-aip`.
//! - **Chat Libre** Rust ReAct loop → via this adapter, plugged into
//!   `NativeChatToolInvoker.fallback_dispatcher` so any tool not in the
//!   hardcoded fast path is automatically resolved through the registry.
//!
//! Goal: every connector tool (MCP, Google, Microsoft, future) needs only
//! ONE registration (in the dispatcher) to become available across all
//! three runtime modes. No more hand-patching `NativeChatToolInvoker` with
//! a new field per provider.

use std::sync::Arc;

use apollia_llm::ToolInvoker;
use async_trait::async_trait;
use serde_json::Value;

use crate::executor::{ToolDispatcher, ToolExecutionError};

/// Wraps a [`ToolDispatcher`] and exposes it as an [`apollia_llm::ToolInvoker`].
///
/// Returns the tool's JSON output as a string for the streaming LLM loop.
/// Errors are surfaced verbatim — the LLM sees the same diagnostic the
/// dispatcher would have logged for an Agent-mode call.
pub struct DispatcherToolInvoker {
    dispatcher: Arc<ToolDispatcher>,
}

impl DispatcherToolInvoker {
    /// Wrap an existing dispatcher.
    pub fn new(dispatcher: Arc<ToolDispatcher>) -> Self {
        Self { dispatcher }
    }
}

#[async_trait]
impl ToolInvoker for DispatcherToolInvoker {
    async fn invoke(&self, tool_name: &str, arguments: &Value) -> Result<String, String> {
        match self.dispatcher.dispatch(tool_name, arguments.clone()).await {
            Ok(value) => match value {
                Value::String(s) => Ok(s),
                other => serde_json::to_string(&other).map_err(|e| e.to_string()),
            },
            Err(err) => Err(format_dispatch_error(&err)),
        }
    }
}

/// Map [`ToolExecutionError`] variants to operator-friendly strings —
/// keeps the wording close to what the existing native fast path returns
/// so log scraping + UI rendering don't need to special-case the source.
fn format_dispatch_error(err: &ToolExecutionError) -> String {
    match err {
        ToolExecutionError::UnknownTool { name } => format!("unknown tool: {name}"),
        ToolExecutionError::ToolNotAllowed { tool_name } => {
            format!("tool not allowed for this session: {tool_name}")
        }
        ToolExecutionError::InvalidInput { message } => {
            format!("invalid input: {message}")
        }
        ToolExecutionError::ExecutionFailed { code, message } => {
            format!("{code}: {message}")
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{ToolDispatcher, ToolExecutor};
    use serde_json::json;

    struct EchoExecutor;
    #[async_trait]
    impl ToolExecutor for EchoExecutor {
        fn name(&self) -> &str {
            "echo"
        }
        async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
            Ok(input)
        }
    }

    #[tokio::test]
    async fn invokes_through_dispatcher_and_returns_json_string() {
        let dispatcher = Arc::new(ToolDispatcher::new(vec![Box::new(EchoExecutor)]));
        let invoker = DispatcherToolInvoker::new(dispatcher);

        let result = invoker.invoke("echo", &json!({"hello": "world"})).await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn unknown_tool_propagates_clear_error() {
        let dispatcher = Arc::new(ToolDispatcher::new(vec![]));
        let invoker = DispatcherToolInvoker::new(dispatcher);

        let err = invoker.invoke("nope", &json!({})).await.unwrap_err();

        assert!(err.contains("unknown tool"));
        assert!(err.contains("nope"));
    }

    #[tokio::test]
    async fn pure_string_output_is_passed_through_unquoted() {
        struct PlainExecutor;
        #[async_trait]
        impl ToolExecutor for PlainExecutor {
            fn name(&self) -> &str {
                "plain"
            }
            async fn execute(&self, _input: Value) -> Result<Value, ToolExecutionError> {
                Ok(Value::String("hello".into()))
            }
        }

        let dispatcher = Arc::new(ToolDispatcher::new(vec![Box::new(PlainExecutor)]));
        let invoker = DispatcherToolInvoker::new(dispatcher);

        let result = invoker.invoke("plain", &json!({})).await.unwrap();

        // Streaming clients expect the raw string, not `"hello"`.
        assert_eq!(result, "hello");
    }
}
