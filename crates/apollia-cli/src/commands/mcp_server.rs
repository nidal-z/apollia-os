//! `apollia-os mcp server`: a stdio bridge from an MCP client to the daemon.
//!
//! An MCP client (an editor, another assistant) spawns this command and talks
//! JSON-RPC 2.0 over stdin and stdout. The command holds no tool of its own:
//! it lists and runs, through the daemon, the agents and workflows the user
//! marked exposed, and nothing else. The daemon must be running, and refuses
//! the bridge until `[mcp] server_enabled` is turned on.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_mcp::{
    BridgeCallOutcome, BridgeError, BridgeFuture, McpBridge, McpServerError, McpStdioServer,
};

use crate::client::{ClientError, RuntimeClient};

/// Errors returned by [`run`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpServerCommandError {
    /// The daemon is not running, or refused the bridge.
    #[error("{0}")]
    Bridge(#[from] BridgeError),
    /// The MCP server I/O loop failed.
    #[error("MCP server error: {0}")]
    Server(#[from] McpServerError),
}

/// The bridge over the runtime HTTP API.
pub struct HttpBridge {
    client: RuntimeClient,
}

impl HttpBridge {
    /// A bridge to the daemon listening on `socket`.
    pub fn new(socket: PathBuf) -> Self {
        Self {
            client: RuntimeClient::new(socket),
        }
    }
}

/// What a failed request to the daemon means to the MCP client.
fn bridge_error(e: ClientError) -> BridgeError {
    match e {
        ClientError::ConnectionRefused => BridgeError::Unavailable(
            "the Apollia daemon is not running; start it with `apollia-os start`".to_string(),
        ),
        ClientError::ServerError { status: 403, body } => BridgeError::Refused(body),
        ClientError::ServerError { status: 404, body } => BridgeError::UnknownTool(body),
        ClientError::ServerError { status: 400, body } => BridgeError::InvalidArguments(body),
        other => BridgeError::Unavailable(other.to_string()),
    }
}

impl McpBridge for HttpBridge {
    fn list_tools(&self) -> BridgeFuture<'_, Result<Vec<serde_json::Value>, BridgeError>> {
        Box::pin(async move {
            let body = self.client.mcp_bridge_tools().await.map_err(bridge_error)?;
            Ok(body
                .get("tools")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default())
        })
    }

    fn call_tool(
        &self,
        name: String,
        arguments: serde_json::Value,
    ) -> BridgeFuture<'_, Result<BridgeCallOutcome, BridgeError>> {
        Box::pin(async move {
            let body = self
                .client
                .mcp_bridge_call(&name, &arguments)
                .await
                .map_err(bridge_error)?;
            Ok(BridgeCallOutcome {
                text: body
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                is_error: body
                    .get("isError")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            })
        })
    }
}

/// Run the MCP stdio bridge against the daemon on `socket`.
///
/// The daemon is asked once before the loop starts, so a stopped daemon or
/// a bridge left off is reported on stderr at once rather than as an empty
/// tool list in the client.
///
/// # Errors
///
/// [`McpServerCommandError::Bridge`] when the daemon is not running or
/// refuses the bridge; [`McpServerCommandError::Server`] on an I/O failure.
pub async fn run(socket: PathBuf) -> Result<(), McpServerCommandError> {
    let bridge = Arc::new(HttpBridge::new(socket));
    bridge.list_tools().await?;
    McpStdioServer::new(bridge).run().await?;
    Ok(())
}

#[cfg(test)]
mod tests;
