//! The OneDrive operations, bounded by the folders and files the user chose
//! for the account in `~/.apollia/onedrive-choices.toml`.
//!
//! The token of the account sees the whole drive. Every operation reads the
//! choices of the account first and goes through
//! [`ChosenOneDrive`], which answers only what sits inside them and refuses an
//! item outside them with the reason.

use std::sync::Arc;

use apollia_auth::{AccountId, MicrosoftScope};
use apollia_connectors::microsoft::onedrive_choices::{ChosenOneDrive, OneDriveChoice};
use apollia_connectors::microsoft::MicrosoftConnector;
use serde_json::{json, Value};

use super::{ms_bearer_for, ms_refresh_closure};
use crate::connectors_bridge::{get_str, get_u32_or};
use crate::onedrive_choices;

const SCOPES: [MicrosoftScope; 1] = [MicrosoftScope::FilesRead];

/// The choices of `account`, read from the file the user's choices are kept in.
fn choices(account: &AccountId) -> Result<Vec<OneDriveChoice>, String> {
    let path = onedrive_choices::prefs_path().map_err(|e| e.to_string())?;
    onedrive_choices::choices_of(&path, account.as_str()).map_err(|e| e.to_string())
}

pub(super) async fn search(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let query = get_str(input, "query")?;
    let top = get_u32_or(input, "top", 10);
    let choices = choices(account)?;
    let (account, token) = ms_bearer_for(connector, account, &SCOPES).await?;
    let mut refresh = ms_refresh_closure(connector.clone(), account, SCOPES.to_vec());
    let items = ChosenOneDrive::new(connector.onedrive(), &choices)
        .search(&query, top, &token, &mut refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&items).map_err(|e| e.to_string())
}

pub(super) async fn get_metadata(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let item_id = get_str(input, "item_id")?;
    let choices = choices(account)?;
    let (account, token) = ms_bearer_for(connector, account, &SCOPES).await?;
    let mut refresh = ms_refresh_closure(connector.clone(), account, SCOPES.to_vec());
    let item = ChosenOneDrive::new(connector.onedrive(), &choices)
        .get_metadata(&item_id, &token, &mut refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&item).map_err(|e| e.to_string())
}

pub(super) async fn download(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let item_id = get_str(input, "item_id")?;
    let choices = choices(account)?;
    let (account, token) = ms_bearer_for(connector, account, &SCOPES).await?;
    let mut refresh = ms_refresh_closure(connector.clone(), account, SCOPES.to_vec());
    let bytes = ChosenOneDrive::new(connector.onedrive(), &choices)
        .download(&item_id, &token, &mut refresh)
        .await
        .map_err(|e| e.to_string())?;
    let size = bytes.len();
    match String::from_utf8(bytes) {
        Ok(text) => Ok(json!({"item_id": item_id, "size": size, "text": text})),
        Err(e) => {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(e.into_bytes());
            Ok(json!({"item_id": item_id, "size": size, "base64": b64}))
        }
    }
}

pub(super) async fn list_recent(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let top = get_u32_or(input, "top", 10);
    let choices = choices(account)?;
    let (account, token) = ms_bearer_for(connector, account, &SCOPES).await?;
    let mut refresh = ms_refresh_closure(connector.clone(), account, SCOPES.to_vec());
    let items = ChosenOneDrive::new(connector.onedrive(), &choices)
        .list_recent(top, &token, &mut refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&items).map_err(|e| e.to_string())
}
