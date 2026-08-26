use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_auth::{AccountId, AuthManager, ConnectorProvider, GoogleScope};
use apollia_connectors::error::ConnectorError;
use apollia_connectors::google::calendar::{Attendee, EventDraft, EventTime};
use apollia_connectors::google::gmail::ComposeMail;
use apollia_connectors::google::GoogleConnector;
use apollia_llm::ToolInvoker;
use apollia_tools::executor::{ToolExecutionError, ToolExecutor as DispatchableExecutor};
use async_trait::async_trait;
use serde_json::Value;

use crate::connectors_bridge::{get_auth, get_google_connector, get_str, get_str_opt};

mod mail_calendar_drive;
mod sheets_docs_tasks;

use mail_calendar_drive::*;
use sheets_docs_tasks::*;

/// Tool invoker that routes connector operation names (`gmail.send`,
/// `gcal.list_events`, `gdrive.workspace_*`, …) to the in-process Google
/// Workspace connector. Returns the connector response as a stringified
/// JSON blob, same convention as the other Chat Libre tool invokers.
pub struct GoogleChatToolInvoker;

impl GoogleChatToolInvoker {
    /// True when `tool_name` is one of the connector operations this invoker
    /// can handle. Used by [`NativeChatToolInvoker`] to decide whether to
    /// route a call through here before falling through to the native match.
    pub fn handles(tool_name: &str) -> bool {
        tool_name.starts_with("gmail.")
            || tool_name.starts_with("gcal.")
            || tool_name.starts_with("gdrive.")
    }
}

#[async_trait]
impl ToolInvoker for GoogleChatToolInvoker {
    async fn invoke(&self, tool_name: &str, arguments: &Value) -> Result<String, String> {
        let value = dispatch_google_tool(tool_name, arguments)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&value).map_err(|e| format!("serialise result: {e}"))
    }
}

async fn dispatch_google_tool(tool_name: &str, input: &Value) -> Result<Value, String> {
    let connector = get_google_connector().await?;
    let auth = get_auth().await?;
    match tool_name {
        "gmail.send" => gmail_send(&connector, &auth, input).await,
        "gmail.compose_draft" => gmail_compose_draft(&connector, &auth, input).await,
        "gcal.list_events" => gcal_list_events(&connector, &auth, input).await,
        "gcal.get_event" => gcal_get_event(&connector, &auth, input).await,
        "gcal.create_event" => gcal_create_event(&connector, &auth, input).await,
        "gcal.update_event" => gcal_update_event(&connector, &auth, input).await,
        "gcal.delete_event" => gcal_delete_event(&connector, &auth, input).await,
        "gdrive.list_my_files" => gdrive_list_my_files(&connector, &auth, input).await,
        "gdrive.find_by_name" => gdrive_find_by_name(&connector, &auth, input).await,
        "gdrive.workspace_list" => gdrive_workspace_list(&connector, &auth, input).await,
        "gdrive.workspace_read" => gdrive_workspace_read(&connector, &auth, input).await,
        "gdrive.workspace_write" => gdrive_workspace_write(&connector, &auth, input).await,
        "gdrive.workspace_delete" => gdrive_workspace_delete(&connector, &auth, input).await,
        "gdrive.workspace_share" => gdrive_workspace_share(&connector, &auth, input).await,
        "gdrive.list_picked_folders" => gdrive_list_picked_folders(&auth).await,
        "gdrive.list_files_in" => gdrive_list_files_in(&connector, &auth, input).await,
        "gdrive.read_file" => gdrive_read_file(&connector, &auth, input).await,
        "gdrive.write_to_folder" => gdrive_write_to_folder(&connector, &auth, input).await,
        // Sheets
        "gsheets.create" => gsheets_create(&connector, &auth, input).await,
        "gsheets.list_sheets" => gsheets_list_sheets(&connector, &auth, input).await,
        "gsheets.read_values" => gsheets_read_values(&connector, &auth, input).await,
        "gsheets.append_values" => gsheets_append_values(&connector, &auth, input).await,
        "gsheets.update_values" => gsheets_update_values(&connector, &auth, input).await,
        // Docs
        "gdocs.create" => gdocs_create(&connector, &auth, input).await,
        "gdocs.read_text" => gdocs_read_text(&connector, &auth, input).await,
        "gdocs.append_text" => gdocs_append_text(&connector, &auth, input).await,
        // Slides
        "gslides.create" => gslides_create(&connector, &auth, input).await,
        "gslides.append_slide" => gslides_append_slide(&connector, &auth, input).await,
        // Tasks
        "gtasks.list_lists" => gtasks_list_lists(&connector, &auth).await,
        "gtasks.list_tasks" => gtasks_list_tasks(&connector, &auth, input).await,
        "gtasks.create" => gtasks_create(&connector, &auth, input).await,
        "gtasks.complete" => gtasks_complete(&connector, &auth, input).await,
        "gtasks.delete" => gtasks_delete(&connector, &auth, input).await,
        // Forms
        "gforms.create" => gforms_create(&connector, &auth, input).await,
        // YouTube
        "youtube.search" => youtube_search(&connector, &auth, input).await,
        "youtube.video_details" => youtube_video_details(&connector, &auth, input).await,
        other => Err(format!("unknown google tool: {other}")),
    }
}

// ─── Per-op handlers, these mirror the desktop bridge but return `Value` ──

async fn resolve_account(auth: &Arc<AuthManager>) -> Result<AccountId, String> {
    let accounts = auth
        .list_accounts(ConnectorProvider::Google)
        .await
        .map_err(|e| format!("auth: {e}"))?;
    if accounts.len() > 1 {
        tracing::warn!(
            count = accounts.len(),
            "multiple Google accounts connected - using the first"
        );
    }
    accounts.into_iter().next().ok_or_else(|| {
        "no Google account connected - open Réglages → Intégrations to sign in".to_string()
    })
}

async fn bearer_for(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    scopes: &[GoogleScope],
) -> Result<(AccountId, String), String> {
    let account = resolve_account(auth).await?;
    let token = connector
        .bearer(&account, scopes)
        .await
        .map_err(|e| format!("token: {e}"))?;
    Ok((account, token))
}

fn refresh_closure(
    connector: Arc<GoogleConnector>,
    account: AccountId,
    scopes: Vec<GoogleScope>,
) -> impl FnMut() -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<String, ConnectorError>> + Send>,
> + Send {
    move || {
        let connector = connector.clone();
        let account = account.clone();
        let scopes = scopes.clone();
        Box::pin(async move { connector.bearer(&account, &scopes).await })
    }
}

fn build_event_time(value: &str) -> EventTime {
    if value.contains('T') {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
            return EventTime {
                date_time: Some(dt.with_timezone(&chrono::Utc)),
                date: None,
                time_zone: None,
            };
        }
    }
    EventTime {
        date_time: None,
        date: Some(value.to_string()),
        time_zone: None,
    }
}

fn build_event_draft(input: &Value) -> Result<EventDraft, String> {
    let attendees: Vec<Attendee> = input
        .get("attendees")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|email| Attendee {
                    email: email.to_string(),
                    display_name: None,
                    response_status: None,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(EventDraft {
        summary: get_str(input, "summary")?,
        description: get_str_opt(input, "description"),
        location: get_str_opt(input, "location"),
        start: build_event_time(&get_str(input, "start")?),
        end: build_event_time(&get_str(input, "end")?),
        attendees,
    })
}

fn extract_compose(input: &Value) -> Result<ComposeMail, String> {
    Ok(ComposeMail {
        to: get_str(input, "to")?,
        subject: get_str(input, "subject")?,
        body: get_str(input, "body")?,
        cc: get_str_opt(input, "cc"),
        bcc: get_str_opt(input, "bcc"),
    })
}

/// Map a Gmail send failure to an actionable message. A 403 or post-refresh
/// 401 on send almost always means the connected token lacks the send scope
/// (the account was linked without "send mail" granted), so steer the agent
/// toward reconnecting rather than silently creating a draft.
fn gmail_send_error(e: ConnectorError) -> String {
    match e {
        ConnectorError::Upstream { status: 403, .. } | ConnectorError::Unauthorized { .. } => {
            "Gmail refused the send: the connected account likely did not grant send permission. \
             Reconnect Google with \"Envoyer des emails (Gmail)\" enabled, then retry. \
             Do not create a draft as a substitute."
                .to_string()
        }
        other => other.to_string(),
    }
}

struct GoogleOpExecutor {
    op_id: &'static str,
}

impl DispatchableExecutor for GoogleOpExecutor {
    fn name(&self) -> &str {
        self.op_id
    }

    fn is_read_only(&self) -> bool {
        matches!(
            self.op_id,
            "gcal.list_events"
                | "gcal.get_event"
                | "gdrive.list_my_files"
                | "gdrive.find_by_name"
                | "gdrive.workspace_list"
                | "gdrive.workspace_read"
                | "gdrive.list_picked_folders"
                | "gdrive.list_files_in"
                | "gdrive.read_file"
                | "gsheets.list_sheets"
                | "gsheets.read_values"
                | "gdocs.read_text"
                | "gtasks.list_lists"
                | "gtasks.list_tasks"
                | "youtube.search"
                | "youtube.video_details"
        )
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            dispatch_google_tool(self.op_id, &input)
                .await
                .map_err(|msg| ToolExecutionError::ExecutionFailed {
                    code: "google".into(),
                    message: msg,
                })
        })
    }
}

/// Build a [`ToolExecutor`] per Google op so they can all be plugged into a
/// single shared [`ToolDispatcher`] alongside MCP + future-provider tools.
pub fn build_google_executors() -> Vec<Box<dyn DispatchableExecutor>> {
    [
        "gmail.send",
        "gmail.compose_draft",
        "gcal.list_events",
        "gcal.get_event",
        "gcal.create_event",
        "gcal.update_event",
        "gcal.delete_event",
        "gdrive.list_my_files",
        "gdrive.find_by_name",
        "gdrive.workspace_list",
        "gdrive.workspace_read",
        "gdrive.workspace_write",
        "gdrive.workspace_delete",
        "gdrive.workspace_share",
        "gdrive.list_picked_folders",
        "gdrive.list_files_in",
        "gdrive.read_file",
        "gdrive.write_to_folder",
        // Tier 1 additions
        "gsheets.create",
        "gsheets.list_sheets",
        "gsheets.read_values",
        "gsheets.append_values",
        "gsheets.update_values",
        "gdocs.create",
        "gdocs.read_text",
        "gdocs.append_text",
        "gslides.create",
        "gslides.append_slide",
        "gtasks.list_lists",
        "gtasks.list_tasks",
        "gtasks.create",
        "gtasks.complete",
        "gtasks.delete",
        "gforms.create",
        "youtube.search",
        "youtube.video_details",
    ]
    .into_iter()
    .map(|id| Box::new(GoogleOpExecutor { op_id: id }) as Box<dyn DispatchableExecutor>)
    .collect()
}
