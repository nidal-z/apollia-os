use serde::{Deserialize, Serialize};

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// McpConfig
// ─────────────────────────────────────────────

/// Default address of the signed index of the connection catalogue.
///
/// A host the project serves, never the storage bucket behind it, so that an
/// organisation can point the same key at an internal mirror.
pub const DEFAULT_MCP_CATALOGUE_URL: &str = "https://registry.apollia.fr/mcp/stable/index.json";

/// Default fallback address, tried when the main one does not answer: the same
/// index attached to the latest release of the project.
pub const DEFAULT_MCP_CATALOGUE_MIRROR_URL: &str =
    "https://github.com/Apollia-OS/apollia-os/releases/latest/download/mcp-stable-index.json";

/// MCP tool loading strategy.
///
/// Controls whether tool schemas are loaded eagerly at session start or
/// deferred until the first use of each tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpToolLoading {
    /// Load every advertised tool schema up front, during the session handshake.
    ///
    /// Preserves the legacy behavior. Suitable for deployments with a small,
    /// fixed set of MCP servers where upfront loading is cheap.
    Eager,
    /// Load only a lightweight index at boot; fetch full schemas on demand.
    ///
    /// Default. Near-zero context cost for large MCP ecosystems. Relies on the
    /// synthetic `tool_search` tool, injected by the runtime, to let an agent
    /// discover tools by intent before any schema is fetched.
    #[default]
    Deferred,
}

/// MCP module configuration (`[mcp]` section in `apollia.toml`).
///
/// Controls the MCP-layer behaviors exposed by the runtime: the tool loading
/// strategy, the `tool_search` result cap, and whether Apollia may serve its
/// exposed agents and workflows to an MCP client. Every field has a sane default via
/// [`Default`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpConfig {
    /// Tool schema loading strategy for all MCP servers.
    ///
    /// `"deferred"` (default): only tool names and descriptions are loaded at
    /// boot; full schemas are fetched on demand. Recommended for large
    /// ecosystems and local models with narrow context windows.
    ///
    /// `"eager"`: all schemas are loaded at session start. Suitable for small,
    /// fixed server sets where upfront loading is cheap.
    #[serde(default)]
    pub tool_loading: McpToolLoading,

    /// Seconds an MCP server may go without a call before it is stopped. It
    /// starts again at its next call; its tools stay listed meanwhile.
    ///
    /// Default: 900. Bounds: [60, 86400]. Read at startup.
    #[serde(default = "default_idle_stop_secs")]
    pub idle_stop_secs: u64,

    /// Maximum number of results returned by the `tool_search` synthetic tool.
    ///
    /// Default: 20. Bounds: [1, 500]. Passed to the `tool_search` executor at
    /// construction time.
    #[serde(default = "default_tool_search_limit")]
    pub tool_search_limit: usize,

    /// Whether the daemon accepts the MCP bridge, `apollia-os mcp server`,
    /// which serves the agents and workflows marked exposed to an MCP client.
    ///
    /// Off by default: the daemon refuses the bridge until the user turns it
    /// on. Read again each time a bridge opens, so a change applies without a
    /// restart.
    #[serde(default)]
    pub server_enabled: bool,

    /// Address of the signed index of the connection catalogue, read when
    /// the catalogue is opened. Its entries replace the selection compiled
    /// into the application, entry by entry, and add to it.
    #[serde(default = "default_catalogue_url")]
    pub catalogue_url: String,

    /// Address tried when the index does not answer; empty disables it.
    #[serde(default = "default_catalogue_mirror_url")]
    pub catalogue_mirror_url: String,

    /// Read the online index of the connection catalogue. Off, the
    /// selection compiled into the application is the whole catalogue.
    #[serde(default = "default_catalogue_enabled")]
    pub catalogue_enabled: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            tool_loading: McpToolLoading::default(),
            idle_stop_secs: default_idle_stop_secs(),
            tool_search_limit: default_tool_search_limit(),
            server_enabled: false,
            catalogue_url: default_catalogue_url(),
            catalogue_mirror_url: default_catalogue_mirror_url(),
            catalogue_enabled: default_catalogue_enabled(),
        }
    }
}

impl McpConfig {
    /// Validates the MCP configuration bounds at startup (fail-fast).
    ///
    /// - `tool_search_limit`: must be in [1, 500].
    /// - `idle_stop_secs`: must be in [60, 86400].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "mcp.idle_stop_secs",
            self.idle_stop_secs,
            60_u64,
            86_400_u64,
        )?;
        validate_bounds(
            "mcp.tool_search_limit",
            self.tool_search_limit,
            1_usize,
            500_usize,
        )?;
        Ok(())
    }
}

fn default_idle_stop_secs() -> u64 {
    900
}

fn default_tool_search_limit() -> usize {
    20
}

fn default_catalogue_url() -> String {
    DEFAULT_MCP_CATALOGUE_URL.to_owned()
}

fn default_catalogue_mirror_url() -> String {
    DEFAULT_MCP_CATALOGUE_MIRROR_URL.to_owned()
}

fn default_catalogue_enabled() -> bool {
    true
}
