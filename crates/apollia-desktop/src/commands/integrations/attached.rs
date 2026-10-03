//! The account and Drive folder commands while the desktop is attached to the
//! daemon `apollia-os start`.
//!
//! The consent, the token store, the connection journal and the Drive
//! preferences belong to the daemon, which serves them under
//! `/api/v1/connectors/`. The window keeps the part that is its own: the
//! loopback listener the browser returns to, and the browser it opens. A
//! refusal of the daemon reaches the interface in the daemon's own words.

use apollia_runtime::drive_folders::DriveFolderStatus;
use serde_json::json;

use super::{IntegrationsError, OauthAccountInfo, OauthCompletedAccount};
use crate::attached::{segment, AttachedClient, AttachedError};

/// What the daemon answers when it starts a consent: the URL to send the user
/// to, and the value the browser returns with the code.
#[derive(Debug, serde::Deserialize)]
pub(super) struct ConsentStarted {
    pub(super) auth_url: String,
    pub(super) state: String,
}

fn refused(error: AttachedError) -> IntegrationsError {
    IntegrationsError::Service(String::from(error))
}

fn drive_route(account_id: &str) -> String {
    format!(
        "/api/v1/connectors/drive-folders/{}",
        segment(account_id.trim())
    )
}

/// Start a consent on the daemon for a browser that returns to
/// `redirect_uri`, the loopback listener of this window.
pub(super) async fn start_consent(
    remote: &AttachedClient,
    provider: &str,
    scopes: &[String],
    redirect_uri: &str,
) -> Result<ConsentStarted, IntegrationsError> {
    let body = json!({ "provider": provider, "scopes": scopes, "redirect_uri": redirect_uri });
    let started = remote.post("/api/v1/connectors/oauth/start", &body).await;
    started.map_err(refused)
}

/// Hand the code to the daemon, which exchanges it, stores the token and
/// journals the connection.
pub(super) async fn complete_consent(
    remote: &AttachedClient,
    state: &str,
    code: &str,
) -> Result<OauthCompletedAccount, IntegrationsError> {
    let body = json!({ "state": state, "code": code });
    let account = remote
        .post("/api/v1/connectors/oauth/complete", &body)
        .await;
    account.map_err(refused)
}

/// The accounts the daemon holds, every provider.
pub(super) async fn accounts(
    remote: &AttachedClient,
) -> Result<Vec<OauthAccountInfo>, IntegrationsError> {
    let accounts = remote.get("/api/v1/connectors/accounts").await;
    accounts.map_err(refused)
}

/// Forget an account on the daemon, which journals the disconnection.
pub(super) async fn disconnect(
    remote: &AttachedClient,
    provider: &str,
    account_id: &str,
) -> Result<(), IntegrationsError> {
    let path = format!(
        "/api/v1/connectors/accounts/{}/{}",
        segment(provider),
        segment(account_id)
    );
    remote.delete::<()>(&path).await.map_err(refused)
}

/// The Drive folder of every Google account the daemon holds.
pub(super) async fn drive_folders(
    remote: &AttachedClient,
) -> Result<Vec<DriveFolderStatus>, IntegrationsError> {
    let folders = remote.get("/api/v1/connectors/drive-folders").await;
    folders.map_err(refused)
}

/// Record the Drive folder of one Google account on the daemon.
pub(super) async fn set_drive_folder(
    remote: &AttachedClient,
    account_id: &str,
    folder_path: &str,
) -> Result<(), IntegrationsError> {
    let body = json!({ "folder_path": folder_path });
    let path = drive_route(account_id);
    let folder = remote.put::<DriveFolderStatus>(&path, &body).await;
    folder.map(drop).map_err(refused)
}

/// Forget the Drive folder of one Google account on the daemon.
pub(super) async fn reset_drive_folder(
    remote: &AttachedClient,
    account_id: &str,
) -> Result<(), IntegrationsError> {
    let path = drive_route(account_id);
    let folder = remote.delete::<DriveFolderStatus>(&path).await;
    folder.map(drop).map_err(refused)
}

#[cfg(test)]
mod tests;
