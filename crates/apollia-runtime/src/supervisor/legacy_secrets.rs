//! Startup step: the connection secrets an earlier version kept outside the
//! secret store move into it, once.
//!
//! Two places held them: `oauth-clients.toml`, with the Google client secret
//! and API key in clear, and `governance.db`, with the tool credentials
//! encrypted under a key file. Both moves are idempotent, so this step runs at
//! every start and does nothing once they are done.

use std::path::Path;

use apollia_core::paths::DataFile;

/// Runs both moves under `data_dir`. A failure is logged and retried at the
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
