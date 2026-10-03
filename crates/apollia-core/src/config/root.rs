//! The single root of `apollia.toml`.

use serde::{Deserialize, Serialize};

use super::{
    ApiConfig, AuthorConfig, ChatConfig, DesktopConfig, EnginesConfig, FilesystemConfig,
    HitlConfig, HooksConfig, LlmConfig, McpConfig, ModelsConfig, RuntimeConfig, ServerConfig,
    ToolsConfig, WorkflowsConfig,
};
use crate::ObservabilityConfig;

/// Every section of `apollia.toml`, as the runtime, the CLI and the desktop
/// application read it.
///
/// Strict at every level: a key no structure declares is refused, with its
/// path, by [`validate`](super::validate). A key the product no longer reads
/// is not declared here; the key registry ([`keys`](super::keys)) lists it as
/// withdrawn, and [`validate`](super::validate) accepts it with a
/// `config.key.retired` warning before this type ever sees it.
///
/// An absent section takes its defaults, except `[llm]`, whose `default` and
/// `backends` keys have none: an absent `[llm]` is `None`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApolliaConfig {
    /// `[llm]`: the backends declared in the file, routing, the runner.
    #[serde(default)]
    pub llm: Option<LlmConfig>,
    /// `[api]`: the TCP listener, authentication, TLS, the Unix socket.
    #[serde(default)]
    pub api: ApiConfig,
    /// `[runtime]`: EventBus and mailbox capacities, startup timeout.
    #[serde(default)]
    pub runtime: RuntimeConfig,
    /// `[hitl]`: the Human-in-the-Loop watcher.
    #[serde(default)]
    pub hitl: HitlConfig,
    /// `[tools]`: native tools and their per-tool settings.
    #[serde(default)]
    pub tools: ToolsConfig,
    /// `[mcp]`: the MCP client.
    #[serde(default)]
    pub mcp: McpConfig,
    /// `[hooks]`: lifecycle hook handlers.
    #[serde(default)]
    pub hooks: HooksConfig,
    /// `[chat]`: chat session defaults.
    #[serde(default)]
    pub chat: ChatConfig,
    /// `[filesystem]`: the reversible journal and the trusted paths.
    #[serde(default)]
    pub filesystem: FilesystemConfig,
    /// `[models]`: the signed index of model cards, the models folder and the
    /// import rule.
    #[serde(default)]
    pub models: ModelsConfig,
    /// `[workflows]`: the budget a workflow run starts with.
    #[serde(default)]
    pub workflows: WorkflowsConfig,
    /// `[desktop]`: the settings of the desktop application that Rust reads.
    #[serde(default)]
    pub desktop: DesktopConfig,
    /// `[server]`: the compatible server, its switch and its port.
    #[serde(default)]
    pub server: ServerConfig,
    /// `[engines]`: the settings of the local inference engines.
    #[serde(default)]
    pub engines: EnginesConfig,
    /// `[author]`: the publisher name of the agents exported from this machine.
    #[serde(default)]
    pub author: AuthorConfig,
    /// `[observability]`: truncation and capture of persisted run data.
    #[serde(default)]
    pub observability: ObservabilityConfig,
    /// `[ui]`: the opt-in link preview of the desktop application.
    #[serde(default)]
    pub ui: UiConfig,
}

/// The `[ui]` section, read by the desktop link preview.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiConfig {
    /// Fetch the Open Graph preview of a link shown in the chat. Off by
    /// default: the fetch leaves the machine.
    #[serde(default)]
    pub link_preview_enabled: bool,
}
