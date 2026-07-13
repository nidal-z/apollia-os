use std::sync::Arc;

use apollia_auth::{AuthManager, GoogleScope};
use apollia_connectors::google::sheets::ValueWrite;
use apollia_connectors::google::tasks::NewTask;
use apollia_connectors::google::GoogleConnector;
use serde_json::{json, Value};

use super::{bearer_for, refresh_closure};
use crate::connectors_bridge::{get_str, get_str_opt, get_u32_or};

// ─── Tier 1 service handlers (Sheets / Docs / Slides / Forms / Tasks / YT) ─

pub(crate) async fn gsheets_create(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let title = get_str(input, "title")?;
    let scopes = [GoogleScope::SheetsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let res = connector
        .sheets()
        .create(&title, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    // Surface the default tab title at top level so the LLM doesn't confuse
    // it with the spreadsheet title when building its first range.
    let default_sheet_title = res
        .sheets
        .first()
        .map(|s| s.properties.title.clone())
        .unwrap_or_default();
    let mut value = serde_json::to_value(&res).map_err(|e| e.to_string())?;
    if let Value::Object(ref mut map) = value {
        map.insert(
            "default_sheet_title".into(),
            Value::String(default_sheet_title.clone()),
        );
        map.insert(
            "usage_hint".into(),
            Value::String(format!(
                "Subsequent `gsheets.*` calls must use the tab title `{default_sheet_title}` in their `range` (e.g. `'{default_sheet_title}'!A1:C1`), NOT the spreadsheet title `{}`.",
                res.properties.title
            )),
        );
    }
    Ok(value)
}

pub(crate) async fn gsheets_list_sheets(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let id = get_str(input, "spreadsheet_id")?;
    let scopes = [GoogleScope::SheetsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let sheets = connector
        .sheets()
        .list_sheets(&id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "spreadsheet_id": id,
        "sheets": sheets,
        "usage_hint": "Range expressions use a sheet `title` from this list (single-quote it if it contains spaces). The spreadsheet title is NOT a valid range prefix.",
    }))
}

pub(crate) async fn gsheets_read_values(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let id = get_str(input, "spreadsheet_id")?;
    let range = get_str(input, "range")?;
    let scopes = [GoogleScope::SheetsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let res = connector
        .sheets()
        .read_values(&id, &range, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&res).map_err(|e| e.to_string())
}

fn json_values_2d(input: &Value) -> Result<Vec<Vec<Value>>, String> {
    input
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing `values` array".to_string())?
        .iter()
        .map(|row| {
            row.as_array()
                .cloned()
                .ok_or_else(|| "each row of `values` must be an array".to_string())
        })
        .collect()
}

pub(crate) async fn gsheets_append_values(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let id = get_str(input, "spreadsheet_id")?;
    let range = get_str(input, "range")?;
    let values = json_values_2d(input)?;
    let scopes = [GoogleScope::SheetsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .sheets()
        .append_values(
            ValueWrite {
                spreadsheet_id: &id,
                range: &range,
                values: &values,
            },
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn gsheets_update_values(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let id = get_str(input, "spreadsheet_id")?;
    let range = get_str(input, "range")?;
    let values = json_values_2d(input)?;
    let scopes = [GoogleScope::SheetsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .sheets()
        .update_values(
            ValueWrite {
                spreadsheet_id: &id,
                range: &range,
                values: &values,
            },
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn gdocs_create(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let title = get_str(input, "title")?;
    let scopes = [GoogleScope::DocsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let res = connector
        .docs()
        .create(&title, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&res).map_err(|e| e.to_string())
}

pub(crate) async fn gdocs_read_text(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let id = get_str(input, "document_id")?;
    let scopes = [GoogleScope::DocsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let text = connector
        .docs()
        .read_plain_text(&id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"document_id": id, "text": text}))
}

pub(crate) async fn gdocs_append_text(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let id = get_str(input, "document_id")?;
    let text = get_str(input, "text")?;
    let scopes = [GoogleScope::DocsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .docs()
        .append_text(&id, &text, &token, refresh)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn gslides_create(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let title = get_str(input, "title")?;
    let scopes = [GoogleScope::SlidesReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let res = connector
        .slides()
        .create(&title, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&res).map_err(|e| e.to_string())
}

pub(crate) async fn gslides_append_slide(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let id = get_str(input, "presentation_id")?;
    let text = get_str(input, "text")?;
    let scopes = [GoogleScope::SlidesReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .slides()
        .append_slide_with_text(&id, &text, &token, refresh)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn gtasks_list_lists(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
) -> Result<Value, String> {
    let scopes = [GoogleScope::Tasks];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let lists = connector
        .tasks()
        .list_lists(&token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&lists).map_err(|e| e.to_string())
}

pub(crate) async fn gtasks_list_tasks(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let list_id = get_str(input, "task_list_id")?;
    let scopes = [GoogleScope::Tasks];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let tasks = connector
        .tasks()
        .list_tasks(&list_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&tasks).map_err(|e| e.to_string())
}

pub(crate) async fn gtasks_create(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let list_id = get_str(input, "task_list_id")?;
    let title = get_str(input, "title")?;
    let notes = get_str_opt(input, "notes");
    let due = get_str_opt(input, "due");
    let scopes = [GoogleScope::Tasks];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let task = connector
        .tasks()
        .create_task(
            NewTask {
                task_list_id: &list_id,
                title: &title,
                notes: notes.as_deref(),
                due_rfc3339: due.as_deref(),
            },
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&task).map_err(|e| e.to_string())
}

pub(crate) async fn gtasks_complete(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let list_id = get_str(input, "task_list_id")?;
    let task_id = get_str(input, "task_id")?;
    let scopes = [GoogleScope::Tasks];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let task = connector
        .tasks()
        .complete_task(&list_id, &task_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&task).map_err(|e| e.to_string())
}

pub(crate) async fn gtasks_delete(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let list_id = get_str(input, "task_list_id")?;
    let task_id = get_str(input, "task_id")?;
    let scopes = [GoogleScope::Tasks];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .tasks()
        .delete_task(&list_id, &task_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"deleted": true, "task_id": task_id}))
}

pub(crate) async fn gforms_create(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let title = get_str(input, "title")?;
    let scopes = [GoogleScope::FormsReadWrite];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let res = connector
        .forms()
        .create(&title, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&res).map_err(|e| e.to_string())
}

pub(crate) async fn youtube_search(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let query = get_str(input, "query")?;
    let max_results = get_u32_or(input, "max_results", 10);
    let scopes = [GoogleScope::YouTubeReadOnly];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let results = connector
        .youtube()
        .search_videos(&query, max_results, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&results).map_err(|e| e.to_string())
}

pub(crate) async fn youtube_video_details(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let id = get_str(input, "video_id")?;
    let scopes = [GoogleScope::YouTubeReadOnly];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let details = connector
        .youtube()
        .video_details(&id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&details).map_err(|e| e.to_string())
}

// ToolExecutor wrappers (Phase 2)
//
// `GoogleChatToolInvoker` implements `ToolInvoker` (string-typed). To plug
// the connector ops into a unified `ToolDispatcher` alongside MCP + native
// executors, we wrap each op in a tiny `ToolExecutor` that forwards through
// the shared invoker. Reuses the same `OnceCell` singletons → no second
// AuthManager, no second connector instance.
