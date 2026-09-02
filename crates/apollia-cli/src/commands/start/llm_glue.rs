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
// Filesystem roots for native tools.
// `FileIo` and friends confine every path to these roots.
// ─────────────────────────────────────────────────────────────

/// Return the filesystem roots used by file-oriented native tools.
///
/// `trusted` is `[filesystem] trusted_paths`, `~` already resolved. It defaults
/// to the user's home directory, which is what the root used to be, hardcoded:
/// an agent whose work lives on a mounted volume or under `/opt` had no way to
/// reach it and no setting to change that.
///
/// The home directory is the fallback when the list is empty, rather than
/// nothing at all: a file tool needs an anchor for relative paths, and an agent
/// with no reachable root is an agent that fails on its first call.
///
/// Centralised so every runner in this crate points at the same roots.
pub(super) fn sandbox_roots_for_agent(trusted: &[PathBuf]) -> Vec<PathBuf> {
    let roots: Vec<PathBuf> = trusted
        .iter()
        .filter(|p| !p.as_os_str().is_empty())
        .cloned()
        .collect();
    if roots.is_empty() {
        return vec![apollia_core::paths::home_dir_or_temp()];
    }
    roots
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

#[cfg(test)]
mod tests {
    use super::sandbox_roots_for_agent;
    use std::path::PathBuf;

    #[test]
    fn an_empty_trusted_list_still_yields_a_root() {
        // GIVEN an operator who emptied `[filesystem] trusted_paths`
        let trusted: Vec<PathBuf> = Vec::new();

        // WHEN the agent roots are derived
        let roots = sandbox_roots_for_agent(&trusted);

        // THEN one root remains. An empty list reaches `SandboxRoot::new` as a
        // construction failure, and the dispatcher logs and skips a tool it
        // cannot build: emptying a setting would silently remove every file
        // tool from the agent rather than narrow it.
        assert_eq!(roots.len(), 1);
        assert!(!roots[0].as_os_str().is_empty());
    }

    #[test]
    fn configured_roots_are_kept_in_order_and_empties_dropped() {
        // GIVEN a configured list carrying an entry that resolved to nothing
        let trusted = vec![
            PathBuf::from("/mnt/work"),
            PathBuf::new(),
            PathBuf::from("/opt/data"),
        ];

        // WHEN the agent roots are derived
        let roots = sandbox_roots_for_agent(&trusted);

        // THEN order is preserved, since the first entry is the anchor relative
        // paths land under, and the empty entry is gone: every path starts with
        // it, so keeping one would trust the whole disk.
        assert_eq!(
            roots,
            vec![PathBuf::from("/mnt/work"), PathBuf::from("/opt/data")]
        );
    }
}
