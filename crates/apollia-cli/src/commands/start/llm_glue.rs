//! Stub LLM types the tool-call helper needs, and the per-agent sandbox rules.

use std::path::Path;
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

/// The access an agent's executor checks, and the folders its file tools
/// reach: the folders granted to it, read from the `system.db` of `data_dir`.
/// An agent granted no folder reaches none, and its file tools say so.
///
/// Centralised so every runner in this crate derives the same roots.
pub(super) fn file_tool_access(
    allowed_tools: Vec<String>,
    data_dir: &Path,
) -> (Vec<String>, apollia_tools::FolderRoots) {
    apollia_tools::granted_folders(allowed_tools, data_dir)
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
    use super::file_tool_access;
    use apollia_core::paths::DataFile;
    use apollia_core::FolderConnectionStore;

    #[test]
    fn a_cli_run_reaches_the_folders_granted_to_its_agent_for_writing() {
        // GIVEN one stored folder granted to read and to write
        let data = tempfile::tempdir().expect("tempdir");
        let folder = tempfile::tempdir().expect("tempdir");
        let store = FolderConnectionStore::open(&DataFile::System.path(data.path()))
            .expect("open system.db");
        let folder = store
            .add(&folder.path().to_string_lossy(), None)
            .expect("add");
        let access = vec![
            format!("conn:folder:{}/file_read", folder.id),
            format!("conn:folder:{}/file_edit", folder.id),
        ];

        // WHEN the CLI path derives the roots of its file tools
        let (tools, roots) = file_tool_access(access, data.path());

        // THEN the folder is both a read and a write root
        let path = std::path::PathBuf::from(&folder.path);
        assert_eq!(tools, vec!["file_read".to_owned(), "file_edit".to_owned()]);
        assert_eq!(roots.read, vec![path.clone()]);
        assert_eq!(roots.write, vec![path]);
    }

    #[test]
    fn a_cli_run_without_a_granted_folder_reaches_no_home_folder() {
        // GIVEN an agent granted the file tools but no folder
        let data = tempfile::tempdir().expect("tempdir");

        // WHEN the CLI path derives the roots of its file tools
        let (_, roots) = file_tool_access(vec!["file_write".to_owned()], data.path());

        // THEN no root is given, the home folder no longer being a default
        assert!(roots.read.is_empty() && roots.write.is_empty());
    }
}
