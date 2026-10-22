//! The OneDrive folders and files a Microsoft account lets Apollia read: list
//! them, browse the drive to choose more, choose one, forget one.
//!
//! The choices live in the runtime, which reads them before every OneDrive
//! call of an agent. Embedded, the commands call
//! `apollia_runtime::onedrive_choices` in this process; attached to the daemon
//! `apollia-os start`, they reach the same functions under
//! `/api/v1/connectors/onedrive/{account_id}/`.

use apollia_runtime::onedrive_choices::{self, OneDriveChoiceError};
use tauri::State;

pub use apollia_runtime::onedrive_choices::{OneDriveChoiceView, OneDriveEntry};

use super::IntegrationsError;
use crate::attached::{segment, AttachedClient, AttachedError, RuntimeLink};

fn choice_error(error: OneDriveChoiceError) -> IntegrationsError {
    match error {
        OneDriveChoiceError::NotConnected(reason) => IntegrationsError::Auth(reason),
        other => IntegrationsError::Service(other.to_string()),
    }
}

fn refused(error: AttachedError) -> IntegrationsError {
    IntegrationsError::Service(String::from(error))
}

fn account_route(account_id: &str) -> String {
    format!("/api/v1/connectors/onedrive/{}", segment(account_id.trim()))
}

fn item_route(account_id: &str, item_id: &str) -> String {
    format!(
        "{}/choices/{}",
        account_route(account_id),
        segment(item_id.trim())
    )
}

/// The folders and files `account_id` lets Apollia read.
#[tauri::command]
pub async fn onedrive_list_choices(
    link: State<'_, RuntimeLink>,
    account_id: String,
) -> Result<Vec<OneDriveChoiceView>, IntegrationsError> {
    if let RuntimeLink::Attached(remote) = link.inner() {
        return attached_list(remote, &account_id).await;
    }
    let prefs = onedrive_choices::prefs_path().map_err(choice_error)?;
    onedrive_choices::list(&prefs, &account_id).map_err(choice_error)
}

/// The children of `folder_id` in the drive of `account_id`, or of the root of
/// the drive, each marked chosen or not.
#[tauri::command]
pub async fn onedrive_browse(
    link: State<'_, RuntimeLink>,
    account_id: String,
    folder_id: Option<String>,
) -> Result<Vec<OneDriveEntry>, IntegrationsError> {
    if let RuntimeLink::Attached(remote) = link.inner() {
        return attached_browse(remote, &account_id, folder_id.as_deref()).await;
    }
    onedrive_choices::browse_drive(&account_id, folder_id.as_deref())
        .await
        .map_err(choice_error)
}

/// Let Apollia read the folder or file `item_id` of `account_id`.
#[tauri::command]
pub async fn onedrive_choose(
    link: State<'_, RuntimeLink>,
    account_id: String,
    item_id: String,
) -> Result<Vec<OneDriveChoiceView>, IntegrationsError> {
    if let RuntimeLink::Attached(remote) = link.inner() {
        return attached_choose(remote, &account_id, &item_id).await;
    }
    onedrive_choices::choose_item(&account_id, &item_id)
        .await
        .map_err(choice_error)
}

/// Stop letting Apollia read the folder or file `item_id` of `account_id`.
#[tauri::command]
pub async fn onedrive_forget(
    link: State<'_, RuntimeLink>,
    account_id: String,
    item_id: String,
) -> Result<Vec<OneDriveChoiceView>, IntegrationsError> {
    if let RuntimeLink::Attached(remote) = link.inner() {
        return attached_forget(remote, &account_id, &item_id).await;
    }
    let prefs = onedrive_choices::prefs_path().map_err(choice_error)?;
    onedrive_choices::forget(&prefs, &account_id, &item_id).map_err(choice_error)
}

async fn attached_list(
    remote: &AttachedClient,
    account_id: &str,
) -> Result<Vec<OneDriveChoiceView>, IntegrationsError> {
    let path = format!("{}/choices", account_route(account_id));
    remote.get(&path).await.map_err(refused)
}

async fn attached_browse(
    remote: &AttachedClient,
    account_id: &str,
    folder_id: Option<&str>,
) -> Result<Vec<OneDriveEntry>, IntegrationsError> {
    let mut path = format!("{}/browse", account_route(account_id));
    if let Some(folder) = folder_id {
        path.push_str(&format!("?folder_id={}", segment(folder.trim())));
    }
    remote.get(&path).await.map_err(refused)
}

async fn attached_choose(
    remote: &AttachedClient,
    account_id: &str,
    item_id: &str,
) -> Result<Vec<OneDriveChoiceView>, IntegrationsError> {
    let path = item_route(account_id, item_id);
    remote
        .put(&path, &serde_json::json!({}))
        .await
        .map_err(refused)
}

async fn attached_forget(
    remote: &AttachedClient,
    account_id: &str,
    item_id: &str,
) -> Result<Vec<OneDriveChoiceView>, IntegrationsError> {
    let path = item_route(account_id, item_id);
    remote.delete(&path).await.map_err(refused)
}

#[cfg(test)]
mod tests;
