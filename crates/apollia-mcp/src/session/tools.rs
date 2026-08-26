//! Tool discovery and schema caching for an MCP session.
//!
//! Split out of `session.rs`: the transport and the handshake stay in the
//! parent, the `tools/list` reads and the deferred schema fetch live here.

use crate::protocol::{McpToolDefinition, ToolsListResult};
use crate::session::{LoadingMode, McpSession, McpSessionError, ToolIndexEntry};

impl McpSession {
    /// Discover tools available on this MCP server via `tools/list`.
    ///
    /// Called automatically at the end of `start()` after the `initialize` handshake.
    /// Populates the `tools` field; logs a warning when the server exposes no tools.
    pub(super) async fn discover_tools(&mut self) -> Result<(), McpSessionError> {
        let timeout_secs = self.config.init_timeout_secs;
        let response = self.send_request("tools/list", None, timeout_secs).await?;

        let result: ToolsListResult =
            serde_json::from_value(response).map_err(|e| McpSessionError::InitializeFailed {
                server: self.config.name.clone(),
                cause: e.to_string(),
            })?;

        tracing::info!(
            server = %self.config.name,
            tools_count = result.tools.len(),
            "mcp.tools.discovered"
        );

        if result.tools.is_empty() {
            tracing::warn!(server = %self.config.name, "mcp.tools.empty");
        }

        self.tools = crate::sanitize::sanitize_tool_definitions(
            result.tools,
            &self.config.name,
            self.config.max_tools as usize,
        );
        Ok(())
    }
    /// Discover only the lightweight tool index (names and descriptions) via
    /// `tools/list`.
    ///
    /// Called at the end of `start_with_mode` in [`LoadingMode::Deferred`]. The
    /// full `tools/list` response is parsed, but only `{name, description}` is
    /// retained in `tool_index`; the `tools` field stays empty and schemas are
    /// fetched on demand by [`McpSession::fetch_tool_schema`].
    pub(super) async fn discover_tools_index(&mut self) -> Result<(), McpSessionError> {
        let timeout_secs = self.config.init_timeout_secs;
        let response = self.send_request("tools/list", None, timeout_secs).await?;

        let result: ToolsListResult =
            serde_json::from_value(response).map_err(|e| McpSessionError::InitializeFailed {
                server: self.config.name.clone(),
                cause: e.to_string(),
            })?;

        let sanitized = crate::sanitize::sanitize_tool_definitions(
            result.tools,
            &self.config.name,
            self.config.max_tools as usize,
        );
        // One `tools/list` carries names, descriptions and schemas together. The
        // index keeps the first two, and the schema cache keeps the third rather
        // than dropping it: deferring is about what enters the prompt, not about
        // what the process holds. Seeding the cache here also removes the extra
        // `tools/list` round-trip `fetch_tool_schema` used to pay on its first
        // call, and gives `collect_tool_index` a schema to hand to the model
        // when the index is small enough to advertise in full.
        self.tool_index = sanitized
            .into_iter()
            .map(|tool| {
                self.schema_cache
                    .insert(tool.name.clone(), tool.input_schema);
                ToolIndexEntry {
                    name: tool.name,
                    description: tool.description,
                }
            })
            .collect();

        tracing::info!(
            server = %self.config.name,
            tools_count = self.tool_index.len(),
            "mcp.tools.index.discovered"
        );

        if self.tool_index.is_empty() {
            tracing::warn!(server = %self.config.name, "mcp.tools.empty");
        }

        Ok(())
    }
    /// Returns the tools discovered via `tools/list`, or an empty slice before discovery.
    ///
    /// Populated in [`LoadingMode::Eager`]. In [`LoadingMode::Deferred`] this is
    /// empty; use [`McpSession::tool_index`] instead.
    pub fn tools(&self) -> &[McpToolDefinition] {
        &self.tools
    }
    /// Returns the lightweight tool index, populated in [`LoadingMode::Deferred`].
    ///
    /// Returns an empty slice in [`LoadingMode::Eager`] (use [`McpSession::tools`]
    /// instead).
    pub fn tool_index(&self) -> &[ToolIndexEntry] {
        &self.tool_index
    }
    /// The cached input schema for `tool_name`, without any I/O.
    ///
    /// In [`LoadingMode::Deferred`] the cache is seeded at discovery, so this
    /// answers for every indexed tool. In [`LoadingMode::Eager`] the schemas
    /// live in `tools` instead and this returns `None`.
    #[must_use]
    pub fn cached_tool_schema(&self, tool_name: &str) -> Option<&serde_json::Value> {
        self.schema_cache.get(tool_name)
    }
    /// Fetch and cache the full JSON schema for a named tool.
    ///
    /// In [`LoadingMode::Eager`] the schema is read from the already-loaded
    /// `tools` slice with no network round-trip. In [`LoadingMode::Deferred`] the
    /// cache is seeded at discovery, so an indexed tool never reaches the wire;
    /// a name that appeared after discovery triggers a single `tools/list` whose
    /// every schema is cached.
    ///
    /// # Errors
    ///
    /// Returns [`McpSessionError::SchemaFetchFailed`] when the tool name is not
    /// known to the server, or when the on-demand `tools/list` round-trip fails
    /// at the transport level.
    pub async fn fetch_tool_schema(
        &mut self,
        tool_name: &str,
    ) -> Result<serde_json::Value, McpSessionError> {
        match self.loading_mode {
            LoadingMode::Eager => self
                .tools
                .iter()
                .find(|tool| tool.name == tool_name)
                .map(|tool| tool.input_schema.clone())
                .ok_or_else(|| McpSessionError::SchemaFetchFailed {
                    server: self.config.name.clone(),
                    tool: tool_name.to_string(),
                    cause: "tool not found".to_string(),
                }),
            LoadingMode::Deferred => {
                if let Some(schema) = self.schema_cache.get(tool_name) {
                    return Ok(schema.clone());
                }

                let timeout_secs = self.config.init_timeout_secs;
                let response = self
                    .send_request("tools/list", None, timeout_secs)
                    .await
                    .map_err(|e| McpSessionError::SchemaFetchFailed {
                        server: self.config.name.clone(),
                        tool: tool_name.to_string(),
                        cause: e.to_string(),
                    })?;

                let result: ToolsListResult = serde_json::from_value(response).map_err(|e| {
                    McpSessionError::SchemaFetchFailed {
                        server: self.config.name.clone(),
                        tool: tool_name.to_string(),
                        cause: e.to_string(),
                    }
                })?;

                for tool in result.tools {
                    self.schema_cache.insert(tool.name, tool.input_schema);
                }

                self.schema_cache.get(tool_name).cloned().ok_or_else(|| {
                    McpSessionError::SchemaFetchFailed {
                        server: self.config.name.clone(),
                        tool: tool_name.to_string(),
                        cause: "tool not found".to_string(),
                    }
                })
            }
        }
    }
}
