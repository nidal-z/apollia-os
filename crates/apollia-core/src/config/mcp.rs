use serde::{Deserialize, Serialize};

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// McpConfig
// ─────────────────────────────────────────────

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
/// Controls the MCP-layer behaviors exposed by the runtime: the TTL of the HITL
/// approvals persisted in SQLite, the tool loading strategy, and the
/// `tool_search` result cap. Every field has a sane default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    /// Validity duration of MCP HITL approvals, in hours.
    ///
    /// When an operator runs `apollia-os mcp set-approval`, the `mcp_approvals`
    /// entry is created with `expires_at = now + approval_ttl_hours`. A value of
    /// `0` disables expiration (permanent approval).
    /// Default: 24. Bounds: [0, 8760] (0 h to 1 year).
    #[serde(default = "default_approval_ttl_hours")]
    pub approval_ttl_hours: u64,

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

    /// Maximum number of results returned by the `tool_search` synthetic tool.
    ///
    /// Default: 20. Bounds: [1, 500]. Passed to the `tool_search` executor at
    /// construction time.
    #[serde(default = "default_tool_search_limit")]
    pub tool_search_limit: usize,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            approval_ttl_hours: default_approval_ttl_hours(),
            tool_loading: McpToolLoading::default(),
            tool_search_limit: default_tool_search_limit(),
        }
    }
}

impl McpConfig {
    /// Validates the MCP configuration bounds at startup (fail-fast).
    ///
    /// - `approval_ttl_hours`: must be in [0, 8760].
    /// - `tool_search_limit`: must be in [1, 500].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "mcp.approval_ttl_hours",
            self.approval_ttl_hours,
            0_u64,
            8760_u64,
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

fn default_approval_ttl_hours() -> u64 {
    24
}

fn default_tool_search_limit() -> usize {
    20
}
