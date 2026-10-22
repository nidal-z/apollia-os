//! The OneDrive folders and files each connected Microsoft account lets
//! Apollia read.
//!
//! The `Files.Read` token sees the whole drive; the bound is Apollia's own and
//! lives in `~/.apollia/onedrive-choices.toml`. The OneDrive executors read it
//! before every call and refuse an item outside it. The desktop running the
//! runtime in its own process calls these functions directly; a client of the
//! daemon reaches the same functions through
//! `/api/v1/connectors/onedrive/{account_id}/...`.
//!
//! A choice is recorded from what Graph says about the item, never from what
//! the caller says: [`choose`] reads the item by id and records the name, kind
//! and path Graph answered.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use apollia_auth::{AccountId, MicrosoftScope};
use apollia_connectors::error::ConnectorError;
use apollia_connectors::microsoft::onedrive::DriveItem;
use apollia_connectors::microsoft::onedrive_choices::{ChoiceKind, OneDriveChoice};
use apollia_connectors::microsoft::MicrosoftConnector;
use serde::{Deserialize, Serialize};

/// On-disk filename of the choices, under `~/.apollia`.
pub const ONEDRIVE_CHOICES_FILENAME: &str = "onedrive-choices.toml";

/// Longest account identifier accepted. An email address is at most 320.
const MAX_ACCOUNT_LEN: usize = 320;

/// Longest Graph item id accepted.
const MAX_ITEM_LEN: usize = 512;

/// Most children answered for one folder.
const BROWSE_TOP: u32 = 200;

fn default_format_version() -> u32 {
    1
}

/// On-disk shape of `~/.apollia/onedrive-choices.toml`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OneDriveChoicesFile {
    /// Version of the persisted shape. A file without the key reads as 1.
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    /// The choices of each Microsoft account, by account identifier.
    pub accounts: BTreeMap<String, AccountChoices>,
}

/// The choices of one account.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountChoices {
    /// The folders and files chosen, in the order they were chosen.
    pub choices: Vec<OneDriveChoice>,
}

/// One chosen folder or file, as a client reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OneDriveChoiceView {
    /// Graph item id.
    pub id: String,
    /// Name at the time of the choice.
    pub name: String,
    /// `true` for a folder, which admits everything under it.
    pub is_folder: bool,
    /// Path of the item in the drive, `/drive/root:/Documents/Clients`.
    pub path: String,
}

/// One item of a folder being browsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OneDriveEntry {
    /// Graph item id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// `true` for a folder, which can be opened or chosen whole.
    pub is_folder: bool,
    /// `true` when the item itself is chosen.
    pub chosen: bool,
}

/// Why the choices could not be read, changed or browsed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OneDriveChoiceError {
    /// The home directory, which holds the choices file, is unknown.
    #[error("cannot determine $HOME directory")]
    NoHome,
    /// The account identifier is empty or too long.
    #[error("the account identifier is empty or too long")]
    InvalidAccount,
    /// The item id is empty or too long.
    #[error("the item id is empty or too long")]
    InvalidItem,
    /// The account has no usable OneDrive token.
    #[error("the Microsoft account is not connected: {0}")]
    NotConnected(String),
    /// The item cannot be chosen (the root of the drive).
    #[error("{0}")]
    NotChoosable(String),
    /// Graph refused or failed.
    #[error("OneDrive answered with an error: {0}")]
    Graph(String),
    /// The choices file exists and does not parse.
    #[error("onedrive-choices.toml is malformed: {0}")]
    Malformed(String),
    /// The choices file could not be read or written.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// `~/.apollia/onedrive-choices.toml`, the file the OneDrive executors read.
pub fn prefs_path() -> Result<PathBuf, OneDriveChoiceError> {
    let home = apollia_core::paths::home_dir().ok_or(OneDriveChoiceError::NoHome)?;
    Ok(prefs_path_in(home))
}

/// The choices file rooted at `home`.
pub fn prefs_path_in<P: Into<PathBuf>>(home: P) -> PathBuf {
    apollia_core::paths::data_dir_under(home).join(ONEDRIVE_CHOICES_FILENAME)
}

fn load(path: &Path) -> Result<OneDriveChoicesFile, OneDriveChoiceError> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|e| OneDriveChoiceError::Malformed(e.to_string()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(OneDriveChoicesFile::default()),
        Err(e) => Err(e.into()),
    }
}

fn save(path: &Path, file: &OneDriveChoicesFile) -> Result<(), OneDriveChoiceError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text =
        toml::to_string_pretty(file).map_err(|e| OneDriveChoiceError::Malformed(e.to_string()))?;
    std::fs::write(path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn checked_account(account_id: &str) -> Result<&str, OneDriveChoiceError> {
    let account_id = account_id.trim();
    if account_id.is_empty() || account_id.len() > MAX_ACCOUNT_LEN {
        return Err(OneDriveChoiceError::InvalidAccount);
    }
    Ok(account_id)
}

fn checked_item(item_id: &str) -> Result<&str, OneDriveChoiceError> {
    let item_id = item_id.trim();
    if item_id.is_empty() || item_id.len() > MAX_ITEM_LEN {
        return Err(OneDriveChoiceError::InvalidItem);
    }
    Ok(item_id)
}

/// Refuse an empty or oversized account or item before anything is read,
/// written or asked of Graph.
pub fn check_input(account_id: &str, item_id: Option<&str>) -> Result<(), OneDriveChoiceError> {
    checked_account(account_id)?;
    item_id.map(checked_item).transpose()?;
    Ok(())
}

/// The choices of `account_id` in the file at `path`, empty when nothing is
/// chosen. A malformed file is refused rather than read as empty.
pub fn choices_of(
    path: &Path,
    account_id: &str,
) -> Result<Vec<OneDriveChoice>, OneDriveChoiceError> {
    let account_id = checked_account(account_id)?;
    let mut file = load(path)?;
    Ok(file
        .accounts
        .remove(account_id)
        .map(|a| a.choices)
        .unwrap_or_default())
}

fn view(choice: &OneDriveChoice) -> OneDriveChoiceView {
    OneDriveChoiceView {
        id: choice.id.clone(),
        name: choice.name.clone(),
        is_folder: choice.kind == ChoiceKind::Folder,
        path: choice.path.clone(),
    }
}

/// The choices of `account_id`, as a client reads them.
pub fn list(path: &Path, account_id: &str) -> Result<Vec<OneDriveChoiceView>, OneDriveChoiceError> {
    Ok(choices_of(path, account_id)?.iter().map(view).collect())
}

/// Record `choice` for `account_id`, replacing an earlier choice of the same
/// item, and answer the choices as recorded.
pub fn record(
    path: &Path,
    account_id: &str,
    choice: OneDriveChoice,
) -> Result<Vec<OneDriveChoiceView>, OneDriveChoiceError> {
    let account_id = checked_account(account_id)?;
    let mut file = load(path)?;
    let entry = file.accounts.entry(account_id.to_owned()).or_default();
    match entry.choices.iter_mut().find(|c| c.id == choice.id) {
        Some(existing) => *existing = choice,
        None => entry.choices.push(choice),
    }
    save(path, &file)?;
    list(path, account_id)
}

/// Forget the choice of `item_id` for `account_id`, and answer the choices as
/// recorded. Forgetting an item never chosen changes nothing.
pub fn forget(
    path: &Path,
    account_id: &str,
    item_id: &str,
) -> Result<Vec<OneDriveChoiceView>, OneDriveChoiceError> {
    let account_id = checked_account(account_id)?;
    let item_id = checked_item(item_id)?;
    let mut file = load(path)?;
    if let Some(entry) = file.accounts.get_mut(account_id) {
        entry.choices.retain(|c| c.id != item_id);
        if entry.choices.is_empty() {
            file.accounts.remove(account_id);
        }
        save(path, &file)?;
    }
    list(path, account_id)
}

/// The entries of `children`, each marked chosen or not against `choices`.
pub fn entries(children: Vec<DriveItem>, choices: &[OneDriveChoice]) -> Vec<OneDriveEntry> {
    children
        .into_iter()
        .map(|item| OneDriveEntry {
            chosen: choices.iter().any(|c| c.id == item.id),
            is_folder: item.folder.is_some(),
            id: item.id,
            name: item.name,
        })
        .collect()
}

async fn token(
    connector: &MicrosoftConnector,
    account: &AccountId,
) -> Result<String, OneDriveChoiceError> {
    connector
        .bearer(account, &[MicrosoftScope::FilesRead])
        .await
        .map_err(|e| OneDriveChoiceError::NotConnected(e.to_string()))
}

fn graph(error: ConnectorError) -> OneDriveChoiceError {
    match error {
        ConnectorError::InvalidArgument(reason) => OneDriveChoiceError::NotChoosable(reason),
        other => OneDriveChoiceError::Graph(other.to_string()),
    }
}

/// The children of `folder_id`, or of the root of the drive, for the user to
/// choose from; each entry says whether it is already chosen.
pub async fn browse(
    connector: &MicrosoftConnector,
    path: &Path,
    account_id: &str,
    folder_id: Option<&str>,
) -> Result<Vec<OneDriveEntry>, OneDriveChoiceError> {
    let account_id = checked_account(account_id)?;
    let folder_id = folder_id.map(checked_item).transpose()?;
    let choices = choices_of(path, account_id)?;
    let account = AccountId::new(account_id);
    let bearer = token(connector, &account).await?;
    let refresh = || connector.bearer(&account, &[MicrosoftScope::FilesRead]);
    let children = connector
        .onedrive()
        .list_children(folder_id, BROWSE_TOP, &bearer, refresh)
        .await
        .map_err(graph)?;
    Ok(entries(children, &choices))
}

/// Choose the item `item_id` for `account_id`: read it from Graph, record its
/// name, kind and place as Graph answered, and answer the choices as recorded.
pub async fn choose(
    connector: &MicrosoftConnector,
    path: &Path,
    account_id: &str,
    item_id: &str,
) -> Result<Vec<OneDriveChoiceView>, OneDriveChoiceError> {
    let account_id = checked_account(account_id)?;
    let item_id = checked_item(item_id)?;
    let account = AccountId::new(account_id);
    let bearer = token(connector, &account).await?;
    let refresh = || connector.bearer(&account, &[MicrosoftScope::FilesRead]);
    let item = connector
        .onedrive()
        .get_metadata(item_id, &bearer, refresh)
        .await
        .map_err(graph)?;
    let choice = OneDriveChoice::from_item(&item).map_err(graph)?;
    record(path, account_id, choice)
}

/// [`browse`] with the runtime's own Microsoft connector and choices file,
/// the entry point of the HTTP route and of the desktop running the runtime in
/// its process. The input is checked before the connector is opened.
pub async fn browse_drive(
    account_id: &str,
    folder_id: Option<&str>,
) -> Result<Vec<OneDriveEntry>, OneDriveChoiceError> {
    check_input(account_id, folder_id)?;
    let prefs = prefs_path()?;
    let connector = crate::connectors_bridge::get_microsoft_connector()
        .await
        .map_err(OneDriveChoiceError::NotConnected)?;
    browse(&connector, &prefs, account_id, folder_id).await
}

/// [`choose`] with the runtime's own Microsoft connector and choices file.
/// The input is checked before the connector is opened.
pub async fn choose_item(
    account_id: &str,
    item_id: &str,
) -> Result<Vec<OneDriveChoiceView>, OneDriveChoiceError> {
    check_input(account_id, Some(item_id))?;
    let prefs = prefs_path()?;
    let connector = crate::connectors_bridge::get_microsoft_connector()
        .await
        .map_err(OneDriveChoiceError::NotConnected)?;
    choose(&connector, &prefs, account_id, item_id).await
}

#[cfg(test)]
mod tests;
