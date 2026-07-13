use std::sync::Arc;

use apollia_auth::{AccountId, AuthManager, GoogleScope};
use apollia_connectors::google::calendar::{EventUpdate, ListEventsFilter};
use apollia_connectors::google::drive_workspace::{FolderWrite, NameSearch, WorkspaceWrite};
use apollia_connectors::google::GoogleConnector;
use serde_json::{json, Value};

use super::{
    bearer_for, build_event_draft, extract_compose, gmail_send_error, refresh_closure,
    resolve_account,
};
use crate::connectors_bridge::{get_str, get_str_opt, get_u32_or, parse_rfc3339};

pub(crate) async fn gmail_send(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let mail = extract_compose(input)?;
    let scopes = [GoogleScope::MailSend];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let result = connector
        .gmail()
        .send(&mail, &token, refresh)
        .await
        .map_err(gmail_send_error)?;
    Ok(json!({"sent": true, "message_id": result.id, "thread_id": result.thread_id}))
}

pub(crate) async fn gmail_compose_draft(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let mail = extract_compose(input)?;
    let scopes = [GoogleScope::MailDraftsCreate];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let result = connector
        .gmail()
        .compose_draft(&mail, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"sent": false, "draft_id": result.id, "thread_id": result.thread_id}))
}

pub(crate) async fn gcal_list_events(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let cal = get_str_opt(input, "calendar_id").unwrap_or_else(|| "primary".to_string());
    let time_min = match get_str_opt(input, "time_min") {
        Some(s) => Some(parse_rfc3339(&s, "time_min")?),
        None => None,
    };
    let time_max = match get_str_opt(input, "time_max") {
        Some(s) => Some(parse_rfc3339(&s, "time_max")?),
        None => None,
    };
    let filter = ListEventsFilter {
        time_min,
        time_max,
        max_results: Some(get_u32_or(input, "max_results", 25)),
        query: get_str_opt(input, "q"),
    };
    let scopes = [GoogleScope::CalendarRead];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let events = connector
        .calendar()
        .list_events(&cal, &filter, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&events).map_err(|e| e.to_string())
}

pub(crate) async fn gcal_get_event(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let cal = get_str_opt(input, "calendar_id").unwrap_or_else(|| "primary".to_string());
    let event_id = get_str(input, "event_id")?;
    let scopes = [GoogleScope::CalendarRead];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let event = connector
        .calendar()
        .get_event(&cal, &event_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&event).map_err(|e| e.to_string())
}

pub(crate) async fn gcal_create_event(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let cal = get_str_opt(input, "calendar_id").unwrap_or_else(|| "primary".to_string());
    let draft = build_event_draft(input)?;
    let scopes = [GoogleScope::CalendarWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let event = connector
        .calendar()
        .create_event(&cal, &draft, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&event).map_err(|e| e.to_string())
}

pub(crate) async fn gcal_update_event(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let cal = get_str_opt(input, "calendar_id").unwrap_or_else(|| "primary".to_string());
    let event_id = get_str(input, "event_id")?;
    let draft = build_event_draft(input)?;
    let scopes = [GoogleScope::CalendarWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let event = connector
        .calendar()
        .update_event(
            EventUpdate {
                calendar_id: &cal,
                event_id: &event_id,
                draft: &draft,
            },
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&event).map_err(|e| e.to_string())
}

pub(crate) async fn gcal_delete_event(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let cal = get_str_opt(input, "calendar_id").unwrap_or_else(|| "primary".to_string());
    let event_id = get_str(input, "event_id")?;
    let scopes = [GoogleScope::CalendarWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .calendar()
        .delete_event(&cal, &event_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"deleted": true, "event_id": event_id}))
}

/// Read the user-configured Drive root path for `account_id`, falling back
/// to the legacy `Apollia` default when no override is set.
fn resolve_drive_root(account_id: &AccountId) -> String {
    apollia_auth::drive_prefs::effective_folder_path("google", account_id.as_str())
}

pub(crate) async fn gdrive_list_my_files(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let explicit_folder_id = get_str_opt(input, "folder_id");
    let all = input.get("all").and_then(Value::as_bool).unwrap_or(false);
    let page_size = input
        .get("page_size")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(50);
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let root_path = resolve_drive_root(&account);
    let mut refresh = refresh_closure(connector.clone(), account, scopes.to_vec());

    // Resolution order:
    //   1. explicit `folder_id` from the caller → use it as-is.
    //   2. `all=true` → unscoped global listing (everything drive.file lets us see).
    //   3. Default → scope the listing to the user's configured root folder.
    //      Empty configured path = literal My Drive root (`'root' in parents`).
    //      Non-empty path that doesn't exist yet on Drive → return [] with a hint.
    let mut scope_note: Option<String> = None;
    let scope: Option<String> = if let Some(id) = explicit_folder_id {
        scope_note = Some(format!("Scoped to folder_id={id}."));
        Some(id)
    } else if all {
        scope_note = Some(
            "all=true: listing every Drive file visible to Apollia (own + Picker grants), ignoring the configured root.".into(),
        );
        None
    } else if root_path.is_empty() {
        scope_note = Some(
            "Scoped to My Drive root (the user explicitly set the configured root to Drive root)."
                .into(),
        );
        Some("root".to_string())
    } else {
        match connector
            .drive()
            .find_path_folder_id(&root_path, &token, || refresh())
            .await
        {
            Ok(Some(id)) => {
                scope_note = Some(format!(
                    "Scoped to the configured root `{root_path}`. Pass `all=true` to list every Drive file Apollia can see."
                ));
                Some(id)
            }
            Ok(None) => {
                return Ok(json!({
                    "files": [],
                    "note": format!(
                        "Configured root `{root_path}` doesn't exist on Drive yet - Apollia hasn't created or written anything in it. Either ask the user to create the folder manually, or call `gdrive.write_to_folder` to materialise it. Pass `all=true` to bypass the workspace scope and see every Drive file Apollia has access to."
                    ),
                }));
            }
            Err(e) => return Err(e.to_string()),
        }
    };

    let files = connector
        .drive()
        .list_visible_files(scope.as_deref(), page_size, &token, || refresh())
        .await
        .map_err(|e| e.to_string())?;
    let note = format!(
        "Drive `drive.file` scope: only files Apollia created or that the user explicitly granted via Picker are visible. {}",
        scope_note.unwrap_or_default()
    );
    Ok(json!({"files": files, "note": note}))
}

pub(crate) async fn gdrive_find_by_name(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let name = get_str(input, "name")?;
    let mime_filter = get_str_opt(input, "mime_type_filter");
    let exact = input.get("exact").and_then(Value::as_bool).unwrap_or(false);
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let files = connector
        .drive()
        .find_by_name(
            NameSearch {
                name: &name,
                mime_type_filter: mime_filter.as_deref(),
                exact,
            },
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "matches": files,
        "query": {"name": name, "mime_type_filter": mime_filter, "exact": exact},
    }))
}

pub(crate) async fn gdrive_workspace_list(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let agent_slug = get_str(input, "agent_slug")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let root_path = resolve_drive_root(&account);
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let files = connector
        .drive()
        .workspace_list(&root_path, &agent_slug, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&files).map_err(|e| e.to_string())
}

pub(crate) async fn gdrive_workspace_read(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let file_id = get_str(input, "file_id")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let bytes = connector
        .drive()
        .workspace_read(&file_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    match String::from_utf8(bytes.clone()) {
        Ok(text) => Ok(json!({"file_id": file_id, "content": text, "encoding": "utf-8"})),
        Err(_) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(json!({"file_id": file_id, "content_base64": b64, "encoding": "base64"}))
        }
    }
}

pub(crate) async fn gdrive_workspace_write(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let agent_slug = get_str(input, "agent_slug")?;
    let name = get_str(input, "name")?;
    let content = get_str(input, "content")?;
    let mime_type = get_str_opt(input, "mime_type");
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let root_path = resolve_drive_root(&account);
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let file = connector
        .drive()
        .workspace_write(
            WorkspaceWrite {
                root_path: &root_path,
                agent_slug: &agent_slug,
                name: &name,
                content: content.as_bytes(),
                mime_type: mime_type.as_deref(),
            },
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

pub(crate) async fn gdrive_workspace_delete(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let file_id = get_str(input, "file_id")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .drive()
        .workspace_delete(&file_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"deleted": true, "file_id": file_id}))
}

// ─── Picker-aware handlers ──────────────────────────────────────────────────

pub(crate) async fn gdrive_list_picked_folders(auth: &Arc<AuthManager>) -> Result<Value, String> {
    let account = resolve_account(auth).await?;
    let folders = apollia_auth::drive_prefs::list_picked_folders("google", account.as_str());
    Ok(json!({
        "folders": folders.iter().map(|f| json!({
            "id": f.id,
            "name": f.name,
            "mime_type": f.mime_type,
        })).collect::<Vec<_>>(),
    }))
}

pub(crate) async fn gdrive_list_files_in(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let folder_id = get_str(input, "folder_id")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let files = connector
        .drive()
        .list_files_in_folder(&folder_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&files).map_err(|e| e.to_string())
}

pub(crate) async fn gdrive_read_file(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    // Same as gdrive.workspace_read but accessible outside the Apollia
    // root, Apollia has `drive.file` access to the file because it lives
    // in a picked folder.
    let file_id = get_str(input, "file_id")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let bytes = connector
        .drive()
        .workspace_read(&file_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    match String::from_utf8(bytes.clone()) {
        Ok(text) => Ok(json!({"file_id": file_id, "content": text, "encoding": "utf-8"})),
        Err(_) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(json!({"file_id": file_id, "content_base64": b64, "encoding": "base64"}))
        }
    }
}

pub(crate) async fn gdrive_write_to_folder(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let explicit_folder_id = get_str_opt(input, "folder_id");
    let at_drive_root = input
        .get("at_drive_root")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let name = get_str(input, "name")?;
    let content = get_str(input, "content")?;
    let mime_type = get_str_opt(input, "mime_type");
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let root_path = resolve_drive_root(&account);
    let mut refresh = refresh_closure(connector.clone(), account, scopes.to_vec());

    // Resolution order:
    //   1. explicit `folder_id` from the caller → drop there.
    //   2. `at_drive_root=true` → bypass the configured workspace, write at root.
    //   3. Default → place the file inside the configured root, creating missing
    //      segments along the way (consistent with workspace_write's semantics).
    let target_folder: Option<String> = if let Some(id) = explicit_folder_id {
        Some(id)
    } else if at_drive_root || root_path.is_empty() {
        None
    } else {
        connector
            .drive()
            .ensure_path_folder_id(&root_path, &token, &mut refresh)
            .await
            .map_err(|e| e.to_string())?
    };

    let file = connector
        .drive()
        .write_file_in_folder(
            FolderWrite {
                folder_id: target_folder.as_deref(),
                name: &name,
                content: content.as_bytes(),
                mime_type: mime_type.as_deref(),
            },
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

pub(crate) async fn gdrive_workspace_share(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let file_id = get_str(input, "file_id")?;
    let email = get_str(input, "email")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let permission_id = connector
        .drive()
        .workspace_share(&file_id, &email, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"shared": true, "file_id": file_id, "email": email, "permission_id": permission_id}))
}
