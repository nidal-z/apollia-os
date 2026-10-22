//! Startup step: the connection secrets an earlier version kept outside the
//! secret store move into it, once.
//!
//! Three places held them: `oauth-clients.toml`, with the Google client
//! secret and API key in clear, `governance.db`, with the tool credentials
//! encrypted under a key file, and `mcp.db`, with the header values and secret
//! variables of an MCP server typed in clear. Each move is idempotent, so this
//! step runs at every start and does nothing once they are done.

use std::path::Path;

use apollia_core::paths::DataFile;
use apollia_mcp::secret_filing::{file_literal_secrets, literal_secret_vars};
use apollia_mcp::McpServerRepository;

use crate::inference_servers::SecretStoreOpener;

/// Runs the three moves under `data_dir`. A failure is logged and retried at the
/// next start; it never stops the runtime.
pub(super) fn move_legacy_secrets_into_store(data_dir: &Path) {
    let opener = crate::inference_servers::default_store_opener();
    let moved = crate::oauth_clients::migrate_file_secrets_at_startup(
        &crate::oauth_clients::file_in(data_dir),
        &opener,
    );
    if moved > 0 {
        tracing::event!(
            tracing::Level::INFO,
            moved,
            "supervisor.oauth_client_secrets.moved"
        );
    }

    move_mcp_secrets_into_store(&DataFile::Mcp.path(data_dir), &opener);

    let governance = DataFile::Governance.path(data_dir);
    if !governance.exists() {
        return;
    }
    let opened = opener().map_err(|e| e.to_string()).and_then(|store| {
        apollia_tools::ToolCredentialStore::new(&governance, store).map_err(|e| e.to_string())
    });
    if let Err(error) = opened {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            detail = "the tool credentials stay where they are until the next start",
            "supervisor.tool_credentials.upgrade.failed"
        );
    }
}

/// Moves into the store every secret the server configurations of `mcp_db`
/// keep in clear, leaving its placeholder in `mcp.db`, and logs each server
/// moved. Returns how many values moved.
///
/// The store is opened only when there is something to move. A server whose
/// move fails keeps its values and is tried again at the next start.
pub(super) fn move_mcp_secrets_into_store(mcp_db: &Path, opener: &SecretStoreOpener) -> usize {
    if !mcp_db.exists() {
        return 0;
    }
    let opened = McpServerRepository::open(mcp_db)
        .and_then(|repo| repo.list().map(|configs| (repo, configs)));
    let (repo, configs) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "supervisor.mcp_secrets.read_failed"
            );
            return 0;
        }
    };
    let pending: Vec<_> = configs
        .into_iter()
        .filter(|c| !literal_secret_vars(c).is_empty())
        .collect();
    if pending.is_empty() {
        return 0;
    }
    let store = match opener() {
        Ok(store) => store,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                detail = "the MCP secrets stay in mcp.db until the next start",
                "supervisor.mcp_secrets.store_unavailable"
            );
            return 0;
        }
    };
    let mut total = 0;
    for mut config in pending {
        let moved = file_literal_secrets(&mut config, store.as_ref(), None)
            .map_err(|e| e.to_string())
            .and_then(|moved| {
                repo.save(&config)
                    .map(|()| moved)
                    .map_err(|e| e.to_string())
            });
        match moved {
            Ok(moved) => {
                total += moved.len();
                tracing::event!(
                    tracing::Level::INFO,
                    server = %config.name,
                    moved = moved.len(),
                    "supervisor.mcp_secrets.moved"
                );
            }
            Err(error) => tracing::event!(
                tracing::Level::WARN,
                server = %config.name,
                error = %error,
                "supervisor.mcp_secrets.move_failed"
            ),
        }
    }
    total
}

#[cfg(test)]
#[path = "legacy_secrets/tests.rs"]
mod tests;
