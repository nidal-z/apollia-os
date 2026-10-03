//! The Drive folder each connected Google account writes to.
//!
//! The choice lives in `~/.apollia/drive-prefs.toml`, read by the Drive
//! executors through `apollia_auth::drive_prefs`. The desktop running the
//! runtime in its own process calls these functions directly; a client of the
//! daemon reaches the same functions through `/api/v1/connectors/drive-folders`.
//! Both therefore list, set and reset a folder the same way and refuse with the
//! same words.

use std::path::{Path, PathBuf};

use apollia_auth::drive_prefs;
use apollia_auth::{AuthManager, ConnectorProvider};
use serde::{Deserialize, Serialize};

/// The provider whose accounts carry a Drive folder.
const GOOGLE: &str = "google";

/// Longest account identifier accepted. An email address is at most 320.
const MAX_ACCOUNT_LEN: usize = 320;

/// Longest folder path accepted.
const MAX_FOLDER_LEN: usize = 1024;

/// The Drive folder of one connected Google account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DriveFolderStatus {
    /// Account identifier, the Google email.
    pub account_id: String,
    /// The path the user chose, absent when the default applies. An empty
    /// path means the root of My Drive.
    pub folder_path: Option<String>,
    /// The path the executors walk: the user's choice, or the default.
    pub effective_folder_path: String,
}

/// Why a Drive folder could not be listed or changed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DriveFolderError {
    /// The home directory, which holds the preferences file, is unknown.
    #[error("cannot determine $HOME directory")]
    NoHome,
    /// The account identifier is empty or too long.
    #[error("the account identifier is empty or too long")]
    InvalidAccount,
    /// The folder path is too long.
    #[error("the folder path is too long")]
    InvalidFolder,
    /// The connected accounts could not be read.
    #[error("{0}")]
    Accounts(String),
    /// The preferences file could not be read or written.
    #[error(transparent)]
    Prefs(#[from] std::io::Error),
}

/// `~/.apollia/drive-prefs.toml`, the file the Drive executors read.
pub fn prefs_path() -> Result<PathBuf, DriveFolderError> {
    drive_prefs::drive_prefs_path().ok_or(DriveFolderError::NoHome)
}

/// The Drive folder of every Google account `manager` holds, read from the
/// preferences file at `prefs`. An unreadable account index is refused.
pub async fn list(
    manager: &AuthManager,
    prefs: &Path,
) -> Result<Vec<DriveFolderStatus>, DriveFolderError> {
    let accounts = manager
        .list_accounts(ConnectorProvider::Google)
        .await
        .map_err(|e| DriveFolderError::Accounts(e.to_string()))?;
    Ok(accounts
        .into_iter()
        .map(|account| status(prefs, account.0))
        .collect())
}

/// The Drive folder recorded for `account_id` in the file at `prefs`.
fn status(prefs: &Path, account_id: String) -> DriveFolderStatus {
    let folder_path = drive_prefs::lookup_folder_path_at(prefs, GOOGLE, &account_id);
    let effective_folder_path = folder_path
        .clone()
        .unwrap_or_else(|| drive_prefs::DEFAULT_ROOT_FOLDER_PATH.to_owned());
    DriveFolderStatus {
        account_id,
        folder_path,
        effective_folder_path,
    }
}

/// Record `folder_path` as the Drive folder of `account_id` in the file at
/// `prefs`, and answer the folder as recorded. An empty path means the root of
/// My Drive; [`reset`] restores the default instead. An oversized account or
/// folder is refused before anything is written.
pub fn set(
    prefs: &Path,
    account_id: &str,
    folder_path: &str,
) -> Result<DriveFolderStatus, DriveFolderError> {
    let account_id = checked_account(account_id)?;
    let folder_path = folder_path.trim();
    if folder_path.len() > MAX_FOLDER_LEN {
        return Err(DriveFolderError::InvalidFolder);
    }
    drive_prefs::set_folder_path_at(prefs, GOOGLE, account_id, folder_path)?;
    Ok(status(prefs, account_id.to_owned()))
}

/// Forget the Drive folder of `account_id` in the file at `prefs`, so the
/// default applies again, and answer the folder as recorded. Folders picked
/// with Google Picker stay.
pub fn reset(prefs: &Path, account_id: &str) -> Result<DriveFolderStatus, DriveFolderError> {
    let account_id = checked_account(account_id)?;
    drive_prefs::reset_folder_path_at(prefs, GOOGLE, account_id)?;
    Ok(status(prefs, account_id.to_owned()))
}

fn checked_account(account_id: &str) -> Result<&str, DriveFolderError> {
    let account_id = account_id.trim();
    if account_id.is_empty() || account_id.len() > MAX_ACCOUNT_LEN {
        return Err(DriveFolderError::InvalidAccount);
    }
    Ok(account_id)
}
