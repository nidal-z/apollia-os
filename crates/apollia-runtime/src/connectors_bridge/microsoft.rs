use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_auth::{AccountId, ConnectorProvider, MicrosoftScope};
use apollia_connectors::error::ConnectorError;
use apollia_connectors::microsoft::calendar::{
    EventAttendee as MsEventAttendee, EventBody as MsEventBody, EventDateTime as MsEventDateTime,
    EventDraft as MsEventDraft, EventLocation as MsEventLocation,
    ListEventsFilter as MsListEventsFilter,
};
use apollia_connectors::microsoft::mail::{
    BodyContentType as MsBodyContentType, ComposeMessage as MsComposeMessage,
    EmailAddress as MsEmailAddress, MessageBody as MsMessageBody, Recipient as MsRecipient,
};
use apollia_connectors::microsoft::MicrosoftConnector;
use apollia_tools::executor::{ToolExecutionError, ToolExecutor as DispatchableExecutor};
use serde_json::{json, Value};
use tokio::sync::OnceCell;

use crate::connectors_bridge::connection_tools::connection_tool_name;
use crate::connectors_bridge::{
    get_auth, get_str, get_str_array, get_str_opt, get_u32_or, parse_rfc3339,
};

// ─── Microsoft 365 dispatch ──────────────────────────────────────────────────
//
// Mirrors the Google path: a lazy `MicrosoftConnector` singleton sharing the
// same `AuthManager` (and OS keychain) as the desktop OAuth commands, a
// string-keyed dispatch table, and one `DispatchableExecutor` per op id so the
// chat dispatcher can route `outlook.*` / `onedrive.*` tool calls.

static MICROSOFT_CONNECTOR: OnceCell<Arc<MicrosoftConnector>> = OnceCell::const_new();

async fn get_microsoft_connector() -> Result<Arc<MicrosoftConnector>, String> {
    let auth = get_auth().await?;
    MICROSOFT_CONNECTOR
        .get_or_try_init(|| async {
            MicrosoftConnector::new(auth.clone())
                .map(Arc::new)
                .map_err(|e| format!("microsoft connector init failed: {e}"))
        })
        .await
        .cloned()
}

/// The token of `account` for `scopes`. The account is the one the tool was
/// published for: no other account is ever chosen in its place.
async fn ms_bearer_for(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    scopes: &[MicrosoftScope],
) -> Result<(AccountId, String), String> {
    let token = connector
        .bearer(account, scopes)
        .await
        .map_err(|e| format!("token: {e}"))?;
    Ok((account.clone(), token))
}

fn ms_refresh_closure(
    connector: Arc<MicrosoftConnector>,
    account: AccountId,
    scopes: Vec<MicrosoftScope>,
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

fn ms_recipients(addrs: Vec<String>) -> Vec<MsRecipient> {
    addrs
        .into_iter()
        .map(|address| MsRecipient {
            email_address: MsEmailAddress {
                name: None,
                address,
            },
        })
        .collect()
}

fn ms_attendees(addrs: Vec<String>) -> Vec<MsEventAttendee> {
    addrs
        .into_iter()
        .map(|address| MsEventAttendee {
            email_address: MsEmailAddress {
                name: None,
                address,
            },
            attendee_type: None,
        })
        .collect()
}

fn build_ms_event_draft(input: &Value) -> Result<MsEventDraft, String> {
    let subject = get_str(input, "subject")?;
    let start = parse_rfc3339(&get_str(input, "start")?, "start")?;
    let end = parse_rfc3339(&get_str(input, "end")?, "end")?;
    Ok(MsEventDraft {
        subject,
        body: get_str_opt(input, "body").map(|content| MsEventBody {
            content_type: "text".to_string(),
            content,
        }),
        start: MsEventDateTime::from_utc(start),
        end: MsEventDateTime::from_utc(end),
        location: get_str_opt(input, "location")
            .map(|display_name| MsEventLocation { display_name }),
        attendees: ms_attendees(get_str_array(input, "attendees")),
        is_all_day: None,
    })
}

async fn dispatch_microsoft_tool(
    tool_name: &str,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let connector = get_microsoft_connector().await?;
    match tool_name {
        "outlook.search" => outlook_search(&connector, account, input).await,
        "outlook.get" => outlook_get(&connector, account, input).await,
        "outlook.send" => outlook_send(&connector, account, input).await,
        "outlook.reply" => outlook_reply(&connector, account, input).await,
        "outlook.list_folders" => outlook_list_folders(&connector, account).await,
        "outlook.move" => outlook_move(&connector, account, input).await,
        "outlook_cal.list_events" => outlook_cal_list_events(&connector, account, input).await,
        "outlook_cal.get_event" => outlook_cal_get_event(&connector, account, input).await,
        "outlook_cal.create_event" => outlook_cal_create_event(&connector, account, input).await,
        "outlook_cal.update_event" => outlook_cal_update_event(&connector, account, input).await,
        "outlook_cal.delete_event" => outlook_cal_delete_event(&connector, account, input).await,
        "onedrive.search" => onedrive_search(&connector, account, input).await,
        "onedrive.get_metadata" => onedrive_get_metadata(&connector, account, input).await,
        "onedrive.download" => onedrive_download(&connector, account, input).await,
        "onedrive.list_recent" => onedrive_list_recent(&connector, account, input).await,
        other => Err(format!("unknown microsoft tool: {other}")),
    }
}

async fn outlook_search(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let query = get_str(input, "query")?;
    let top = get_u32_or(input, "top", 10);
    let scopes = [MicrosoftScope::MailRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let msgs = connector
        .mail()
        .search(&query, top, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&msgs).map_err(|e| e.to_string())
}

async fn outlook_get(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let message_id = get_str(input, "message_id")?;
    let scopes = [MicrosoftScope::MailRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let msg = connector
        .mail()
        .get(&message_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&msg).map_err(|e| e.to_string())
}

async fn outlook_send(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let to = get_str_array(input, "to");
    if to.is_empty() {
        return Err("`to` must contain at least one recipient address".into());
    }
    let message = MsComposeMessage {
        subject: get_str(input, "subject")?,
        body: MsMessageBody {
            content_type: MsBodyContentType::Text,
            content: get_str(input, "body")?,
        },
        to_recipients: ms_recipients(to),
        cc_recipients: ms_recipients(get_str_array(input, "cc")),
        bcc_recipients: ms_recipients(get_str_array(input, "bcc")),
    };
    let scopes = [MicrosoftScope::MailSend];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .mail()
        .send(message, true, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"sent": true}))
}

async fn outlook_reply(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let message_id = get_str(input, "message_id")?;
    let comment = get_str(input, "comment")?;
    let scopes = [MicrosoftScope::MailSend];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .mail()
        .reply(&message_id, &comment, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"sent": true, "message_id": message_id}))
}

async fn outlook_list_folders(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
) -> Result<Value, String> {
    let scopes = [MicrosoftScope::MailRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let folders = connector
        .mail()
        .list_folders(&token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&folders).map_err(|e| e.to_string())
}

async fn outlook_move(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let message_id = get_str(input, "message_id")?;
    let destination = get_str(input, "destination_folder_id")?;
    let scopes = [MicrosoftScope::MailReadWrite];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let moved = connector
        .mail()
        .move_to(&message_id, &destination, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&moved).map_err(|e| e.to_string())
}

async fn outlook_cal_list_events(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let start_after = match get_str_opt(input, "start_after") {
        Some(s) => Some(parse_rfc3339(&s, "start_after")?),
        None => None,
    };
    let end_before = match get_str_opt(input, "end_before") {
        Some(s) => Some(parse_rfc3339(&s, "end_before")?),
        None => None,
    };
    let filter = MsListEventsFilter {
        start_after,
        end_before,
        top: Some(get_u32_or(input, "top", 25)),
    };
    let scopes = [MicrosoftScope::CalendarRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let events = connector
        .calendar()
        .list_events(&filter, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&events).map_err(|e| e.to_string())
}

async fn outlook_cal_get_event(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let event_id = get_str(input, "event_id")?;
    let scopes = [MicrosoftScope::CalendarRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let event = connector
        .calendar()
        .get_event(&event_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&event).map_err(|e| e.to_string())
}

async fn outlook_cal_create_event(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let draft = build_ms_event_draft(input)?;
    let scopes = [MicrosoftScope::CalendarWrite];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let event = connector
        .calendar()
        .create_event(&draft, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&event).map_err(|e| e.to_string())
}

async fn outlook_cal_update_event(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let event_id = get_str(input, "event_id")?;
    let draft = build_ms_event_draft(input)?;
    let scopes = [MicrosoftScope::CalendarWrite];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let event = connector
        .calendar()
        .update_event(&event_id, &draft, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&event).map_err(|e| e.to_string())
}

async fn outlook_cal_delete_event(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let event_id = get_str(input, "event_id")?;
    let scopes = [MicrosoftScope::CalendarWrite];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .calendar()
        .delete_event(&event_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"deleted": true, "event_id": event_id}))
}

async fn onedrive_search(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let query = get_str(input, "query")?;
    let top = get_u32_or(input, "top", 10);
    let scopes = [MicrosoftScope::FilesRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let items = connector
        .onedrive()
        .search(&query, top, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&items).map_err(|e| e.to_string())
}

async fn onedrive_get_metadata(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let item_id = get_str(input, "item_id")?;
    let scopes = [MicrosoftScope::FilesRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let item = connector
        .onedrive()
        .get_metadata(&item_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&item).map_err(|e| e.to_string())
}

async fn onedrive_download(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let item_id = get_str(input, "item_id")?;
    let scopes = [MicrosoftScope::FilesRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let bytes = connector
        .onedrive()
        .download(&item_id, &token, refresh)
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

async fn onedrive_list_recent(
    connector: &Arc<MicrosoftConnector>,
    account: &AccountId,
    input: &Value,
) -> Result<Value, String> {
    let top = get_u32_or(input, "top", 10);
    let scopes = [MicrosoftScope::FilesRead];
    let (account, token) = ms_bearer_for(connector, account, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let items = connector
        .onedrive()
        .list_recent(top, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&items).map_err(|e| e.to_string())
}

/// One Microsoft 365 operation of one connected account, named
/// `conn:microsoft:<account>/<operation>`.
struct MicrosoftOpExecutor {
    op_id: &'static str,
    account: AccountId,
    name: String,
}

impl DispatchableExecutor for MicrosoftOpExecutor {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_read_only(&self) -> bool {
        matches!(
            self.op_id,
            "outlook.search"
                | "outlook.get"
                | "outlook.list_folders"
                | "outlook_cal.list_events"
                | "outlook_cal.get_event"
                | "onedrive.search"
                | "onedrive.get_metadata"
                | "onedrive.download"
                | "onedrive.list_recent"
        )
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            dispatch_microsoft_tool(self.op_id, &self.account, &input)
                .await
                .map_err(|msg| ToolExecutionError::ExecutionFailed {
                    code: "microsoft".into(),
                    message: msg,
                })
        })
    }
}

/// Every Microsoft 365 operation, in the order the executors are built.
const MICROSOFT_OPERATIONS: [&str; 15] = [
    "outlook.search",
    "outlook.get",
    "outlook.send",
    "outlook.reply",
    "outlook.list_folders",
    "outlook.move",
    "outlook_cal.list_events",
    "outlook_cal.get_event",
    "outlook_cal.create_event",
    "outlook_cal.update_event",
    "outlook_cal.delete_event",
    "onedrive.search",
    "onedrive.get_metadata",
    "onedrive.download",
    "onedrive.list_recent",
];

/// Build one [`ToolExecutor`] per Microsoft 365 operation and per connected
/// account in `accounts`, each named `conn:microsoft:<account>/<operation>`
/// and bound to that account's token.
pub fn build_microsoft_executors(accounts: &[AccountId]) -> Vec<Box<dyn DispatchableExecutor>> {
    microsoft_op_executors(accounts)
        .into_iter()
        .map(|executor| Box::new(executor) as Box<dyn DispatchableExecutor>)
        .collect()
}

fn microsoft_op_executors(accounts: &[AccountId]) -> Vec<MicrosoftOpExecutor> {
    accounts
        .iter()
        .flat_map(|account| {
            MICROSOFT_OPERATIONS
                .into_iter()
                .map(move |op_id| MicrosoftOpExecutor {
                    op_id,
                    account: account.clone(),
                    name: connection_tool_name(
                        ConnectorProvider::Microsoft,
                        account.as_str(),
                        op_id,
                    ),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests;
