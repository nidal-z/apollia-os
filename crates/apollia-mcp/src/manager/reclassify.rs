//! The family the operator gives a tool, taken in by the live manager.
//!
//! The family is stored in `mcp.db` by the caller; the manager only makes the
//! state it serves agree with it, without restarting the server: the
//! session's configuration, the registry descriptor in eager loading, and the
//! entries kept for a server that has no session, in deferred loading.

use apollia_tools::{ActionFamily, FamilySource};
use tokio::sync::oneshot;

use crate::manager::views::{tool_descriptor, ServerHints};
use crate::manager::{McpClientManager, McpClientManagerHandle, McpCommand};
use crate::session::{LoadingMode, McpSessionError};

impl McpClientManagerHandle {
    /// Classify the tool `tool` of the server `server_name` in `family`, the
    /// operator's choice, in the sessions and the index the manager serves.
    ///
    /// A server the manager does not know, or a tool it does not publish, is
    /// not an error: the stored family is read at its next start.
    pub async fn set_operator_family(
        &self,
        server_name: &str,
        tool: &str,
        family: ActionFamily,
    ) -> Result<(), McpSessionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let gone = || McpSessionError::ServerExited {
            server: server_name.to_string(),
        };
        self.tx
            .send(McpCommand::SetOperatorFamily {
                server_name: server_name.to_string(),
                tool: tool.to_string(),
                family,
                reply: reply_tx,
            })
            .await
            .map_err(|_| gone())?;
        reply_rx.await.map_err(|_| gone())
    }
}

impl McpClientManager {
    /// Take in the operator's `family` for `tool` of `server_name`. A server a
    /// request brought is left alone: nothing classifies its tools.
    pub(super) async fn handle_set_operator_family(
        &mut self,
        server_name: &str,
        tool: &str,
        family: ActionFamily,
    ) {
        if self.request_servers.contains(server_name) {
            return;
        }
        if let Some(session) = self.sessions.get_mut(server_name) {
            session.set_operator_family(tool, family);
            let definition = session.tools().iter().find(|t| t.name == tool);
            if let (LoadingMode::Eager, Some(definition)) = (self.loading_mode, definition) {
                let config = session.config();
                let descriptor = tool_descriptor(
                    server_name,
                    &config.tags,
                    config,
                    definition,
                    ServerHints::Read,
                );
                let name = descriptor.name.clone();
                let replaced = match self.tool_registry.unregister(&name).await {
                    Ok(_) => self.tool_registry.register(descriptor).await,
                    Err(e) => Err(e),
                };
                if let Err(e) = replaced {
                    tracing::event!(
                        tracing::Level::WARN,
                        server = %server_name,
                        error = %e,
                        "mcp.tool.reclassify.failed"
                    );
                }
            }
        }
        if let Some(kept) = self.kept_index.get_mut(server_name) {
            for entry in kept.iter_mut().filter(|e| e.tool_name == tool) {
                if entry.family_source != FamilySource::Selection {
                    entry.family = family;
                    entry.family_source = FamilySource::Operator;
                }
            }
        }
        tracing::event!(
            tracing::Level::INFO,
            server = %server_name,
            family = %family.as_str(),
            "mcp.tool.reclassified"
        );
    }
}
