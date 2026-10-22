//! The secrets a configuration sent to the MCP routes carries in clear, moved
//! into the secret store before it is written to `mcp.db`.

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::secret_filing::{file_literal_secrets, literal_secret_vars};
use axum::http::StatusCode;

use super::{json_err, JsonError};
use crate::inference_servers::SecretStoreOpener;

/// Files into the store every secret `config` keeps in clear, leaving the
/// placeholders in its place; `previous` is the configuration it replaces.
///
/// The store is opened only when there is something to file.
pub(super) fn file_secrets(
    opener: &SecretStoreOpener,
    config: &mut McpServerConfig,
    previous: Option<&McpServerConfig>,
) -> Result<(), JsonError> {
    if literal_secret_vars(config).is_empty() {
        return Ok(());
    }
    let store = opener().map_err(|e| {
        json_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("secret store unavailable: {e}"),
        )
    })?;
    let moved = file_literal_secrets(config, store.as_ref(), previous)
        .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    tracing::event!(
        tracing::Level::INFO,
        server = %config.name,
        moved = moved.len(),
        "mcp.server.secrets_filed"
    );
    Ok(())
}
