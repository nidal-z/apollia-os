//! Drive folder management: the sync root and the picked folders.

use apollia_auth::ConnectorProvider;

use crate::exit_codes;
use crate::note;

use super::{emit_error, open_auth_manager, DriveCommand, DriveFolderCommand, PickedFolderCommand};

// ─── drive folder management ──────────────────────────────────────────────────

pub(super) async fn run_drive(cmd: &DriveCommand, json: bool) -> i32 {
    match cmd {
        DriveCommand::Folder { command } => run_drive_folder(command, json).await,
    }
}

pub(super) async fn run_drive_folder(cmd: &DriveFolderCommand, json: bool) -> i32 {
    match cmd {
        DriveFolderCommand::List => run_drive_folder_list(json).await,
        DriveFolderCommand::Set { account, path } => run_drive_folder_set(account, path, json),
        DriveFolderCommand::Reset { account, confirm } => {
            run_drive_folder_reset(account, *confirm, json)
        }
        DriveFolderCommand::Picked { command } => run_drive_picked(command, json),
    }
}

pub(super) async fn run_drive_folder_list(json: bool) -> i32 {
    let Some(auth) = open_auth_manager(json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let accounts = match auth.list_accounts(ConnectorProvider::Google).await {
        Ok(a) => a,
        Err(e) => {
            emit_error(format!("failed to list google accounts: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    let rows: Vec<(String, Option<String>, String)> = accounts
        .iter()
        .map(|a| {
            let override_path = apollia_auth::drive_prefs::lookup_folder_path("google", a.as_str());
            let effective = apollia_auth::drive_prefs::effective_folder_path("google", a.as_str());
            (a.0.clone(), override_path, effective)
        })
        .collect();

    if json {
        let array: Vec<serde_json::Value> = rows
            .iter()
            .map(|(account_id, override_path, effective)| {
                serde_json::json!({
                    "account_id": account_id,
                    "folder_path": override_path,
                    "effective_folder_path": effective,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else if rows.is_empty() {
        println!("No connected Google accounts. Connect one from the desktop app, Settings > Integrations.");
    } else {
        note!("  Drive folder configuration (google):");
        for (account_id, override_path, effective) in &rows {
            println!("  * {account_id}");
            match override_path {
                Some(p) => println!("      override : {p}"),
                None => println!("      override : <default>"),
            }
            println!("      effective: {effective}");
        }
    }
    exit_codes::SUCCESS
}

pub(super) fn run_drive_folder_set(account: &str, path: &str, json: bool) -> i32 {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        emit_error(
            "folder path must not be empty (use `reset` to clear an override)".into(),
            json,
        );
        return exit_codes::GENERAL_ERROR;
    }
    match apollia_auth::drive_prefs::set_folder_path("google", account, trimmed) {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "provider": "google",
                    "account_id": account,
                    "folder_path": trimmed,
                    "updated": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                println!("  * google / {account} folder set to: {trimmed}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("failed to write drive-prefs.toml: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

pub(super) fn run_drive_folder_reset(account: &str, confirm: bool, json: bool) -> i32 {
    if let Some(code) = crate::output::require_confirmation(
        confirm,
        json,
        &format!("reset the Drive folder override of '{account}'"),
    ) {
        return code;
    }
    match apollia_auth::drive_prefs::reset_folder_path("google", account) {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "provider": "google",
                    "account_id": account,
                    "reset": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                println!("  * google / {account} folder override reset to default");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("failed to write drive-prefs.toml: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

pub(super) fn run_drive_picked(cmd: &PickedFolderCommand, json: bool) -> i32 {
    match cmd {
        PickedFolderCommand::List { account } => run_drive_picked_list(account, json),
        PickedFolderCommand::Remove {
            account,
            folder_id,
            confirm,
        } => run_drive_picked_remove(account, folder_id, *confirm, json),
    }
}

pub(super) fn run_drive_picked_list(account: &str, json: bool) -> i32 {
    let folders = apollia_auth::drive_prefs::list_picked_folders("google", account);
    if json {
        let array: Vec<serde_json::Value> = folders
            .iter()
            .map(|f| {
                serde_json::json!({
                    "id": f.id,
                    "name": f.name,
                    "mime_type": f.mime_type,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else if folders.is_empty() {
        println!("No picked Drive folders for google / {account}.");
        println!("  -> Use the Desktop app to pick folders (the CLI has no Picker UI).");
    } else {
        note!("  Picked Drive folders (google / {account}):");
        for f in &folders {
            println!("  * {} ({})", f.name, f.id);
            println!("      mime: {}", f.mime_type);
        }
    }
    exit_codes::SUCCESS
}

pub(super) fn run_drive_picked_remove(
    account: &str,
    folder_id: &str,
    confirm: bool,
    json: bool,
) -> i32 {
    if let Some(code) = crate::output::require_confirmation(
        confirm,
        json,
        &format!("remove picked Drive folder '{folder_id}' of '{account}'"),
    ) {
        return code;
    }
    match apollia_auth::drive_prefs::remove_picked_folder("google", account, folder_id) {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "provider": "google",
                    "account_id": account,
                    "folder_id": folder_id,
                    "removed": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                println!("  * google / {account} picked folder {folder_id} removed");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("failed to write drive-prefs.toml: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}
