//! Stub LLM types the tool-call helper needs, and the per-agent sandbox rules.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use apollia_aip::context::ToolProxy;
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, LlmRouter,
    ObservabilityConfig, ToolInvoker,
};
use futures::stream;

// ─────────────────────────────────────────────────────────────
// Stub LLM types required by ToolCallHelper constructor.
// RouterModel delegates to the real LlmRouter; NoopToolInvoker returns errors.
// These stubs are only invoked when an agent uses the LLM ReAct loop.
// ─────────────────────────────────────────────────────────────

pub(super) struct RouterModel(pub(super) Arc<LlmRouter>);

#[async_trait::async_trait]
impl CompletionModel for RouterModel {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.0
            .complete_with_observability(None, req, None, &ObservabilityConfig::default())
            .await
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<
        Pin<Box<dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, LlmError>> + Send>>,
        LlmError,
    > {
        let s: Pin<
            Box<dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, LlmError>> + Send>,
        > = Box::pin(stream::empty());
        Ok(s)
    }

    fn is_available(&self) -> bool {
        !self.0.list().is_empty()
    }
    fn backend_name(&self) -> &str {
        "router"
    }
    fn model_id(&self) -> &str {
        "router"
    }
}

pub(super) struct NoopToolInvoker;

#[async_trait::async_trait]
impl ToolInvoker for NoopToolInvoker {
    async fn invoke(&self, name: &str, _args: &serde_json::Value) -> Result<String, String> {
        Err(format!(
            "tool '{name}' invocation via LLM loop not wired - use ctx.tools directly"
        ))
    }
}

/// Adapts an apollia-aip [`ToolProxy`] to ORIA's `ToolProxyTrait`.
///
/// Lets the orchestrated `ActorLoop` execute real, governed tools (permission
/// engine + audit trail + A2A routing + tool-call counting) instead of hitting
/// the engine's `NoopToolProxy` fallback. Tool output is normalised to a string
/// (JSON-serialised when not already a string) to match the trait contract.
pub(super) struct OriaToolProxy {
    pub(super) proxy: ToolProxy,
}

#[async_trait::async_trait]
impl apollia_oria::actor::ToolProxyTrait for OriaToolProxy {
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String> {
        match self.proxy.invoke_native(tool_name, input.clone()).await {
            Ok(serde_json::Value::String(s)) => Ok(s),
            Ok(other) => serde_json::to_string(&other).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    // `is_tool_read_only` keeps the trait default (false): orchestrated tool
    // steps run sequentially, never wrongly batched. Correct, if not maximally
    // parallel; ORIA-level read-only classification is a follow-up.

    async fn tool_schema(&self, tool_name: &str) -> Option<serde_json::Value> {
        self.proxy.tool_input_schema(tool_name).await
    }
}

// ─────────────────────────────────────────────────────────────
// Filesystem sandbox root for native tools (dev mode).
// `FileIo` and friends sandbox all paths under this root: we keep
// `$HOME` for parity with the previous embedded `NativeToolExecutor`
// so workspaces located anywhere under the user's home remain usable.
// ─────────────────────────────────────────────────────────────

/// Return the sandbox root used for file-oriented native tools.
///
/// Centralised so every runner in this crate points at the same root.
pub(super) fn sandbox_root_for_agent() -> PathBuf {
    apollia_core::paths::home_dir_or_temp()
}

/// Union of statically-disabled tools (from `apollia.toml`) with the runtime
/// disabled set (from `governance.db`). Either source disables a tool: the
/// dispatcher only registers tools absent from both lists.
pub(super) fn merge_disabled(
    static_disabled: &[String],
    mut runtime_disabled: Vec<String>,
) -> Vec<String> {
    for name in static_disabled {
        if !runtime_disabled.iter().any(|n| n == name) {
            runtime_disabled.push(name.clone());
        }
    }
    runtime_disabled
}
