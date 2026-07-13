//! Desktop-side bridge: produces [`ToolExecutor`]s that route agent tool
//! invocations through the connector clients (Gmail / Calendar / Drive).
//!
//! Counterpart of `apollia_runtime::connectors_bridge` (which produces the
//! static [`ToolDescriptor`]s registered at supervisor boot). The desktop
//! side cannot live in the runtime crate because it depends on the
//! `AuthManager` singleton owned by [`commands::integrations`], which itself
//! depends on `tauri`.
//!
//! ## Account resolution (v0.1.0)
//!
//! Each call resolves a single Google account: the first one stored in the
//! keyring under `ConnectorProvider::Google`. If none is connected, a clear
//! `ExecutionFailed` is returned so the agent can tell the user to wire up
//! Google in Settings → Integrations. Multi-account dispatch (`account_id`
//! input field, default account preference) is deferred; track it in a
//! follow-up.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_auth::{AccountId, AuthManager, ConnectorProvider, GoogleScope};
use apollia_connectors::google::{
    calendar::{Attendee, EventDraft, EventTime, EventUpdate, ListEventsFilter},
    drive_workspace::WorkspaceWrite,
    gmail::ComposeMail,
    GoogleConnector,
};
use apollia_tools::executor::{ToolExecutionError, ToolExecutor};
use serde_json::{json, Value};

/// Build the full set of Google [`ToolExecutor`]s the dispatcher should know
/// about. Each executor name matches the descriptor registered at supervisor
/// boot so `ToolDispatcher::dispatch` resolves them at call time.
pub async fn build_google_executors() -> Vec<Box<dyn ToolExecutor>> {
    let Ok(auth) = crate::commands::integrations::auth_manager().await else {
        // AuthManager couldn't initialise (keyring unavailable, etc.), so skip
        // the executors entirely. Agents will see the descriptors in their
        // catalogue but every call returns the clear "auth not ready" error
        // produced by the executor, strictly better than silent UnknownTool.
        return Vec::new();
    };
    let connector = match GoogleConnector::new(auth.clone()) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::warn!(error = %e, "Google connector init failed - tools unavailable");
            return Vec::new();
        }
    };

    let op_ids = [
        "gmail.send",
        "gmail.compose_draft",
        "gcal.list_events",
        "gcal.get_event",
        "gcal.create_event",
        "gcal.update_event",
        "gcal.delete_event",
        "gdrive.workspace_list",
        "gdrive.workspace_read",
        "gdrive.workspace_write",
        "gdrive.workspace_delete",
        "gdrive.workspace_share",
    ];
    op_ids
        .into_iter()
        .map(|id| {
            Box::new(GoogleToolExecutor {
                connector: connector.clone(),
                auth: auth.clone(),
                op_id: id,
            }) as Box<dyn ToolExecutor>
        })
        .collect()
}

/// Single executor parametrised by `op_id`. Routes the JSON input to the
/// appropriate `GoogleConnector` method, resolves a fresh bearer token for
/// every call (the `AuthManager` caches + refreshes with singleflight so
/// repeated calls don't burst the network), and serialises the response back
/// to JSON for the LLM.
struct GoogleToolExecutor {
    connector: Arc<GoogleConnector>,
    auth: Arc<AuthManager>,
    op_id: &'static str,
}

impl GoogleToolExecutor {
    async fn resolve_account(&self) -> Result<AccountId, ToolExecutionError> {
        let accounts = self
            .auth
            .list_accounts(ConnectorProvider::Google)
            .await
            .map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "auth".into(),
                message: e.to_string(),
            })?;
        if accounts.is_empty() {
            return Err(ToolExecutionError::ExecutionFailed {
                code: "no_account".into(),
                message: "no Google account connected - open Réglages → Intégrations to sign in"
                    .into(),
            });
        }
        if accounts.len() > 1 {
            tracing::warn!(
                count = accounts.len(),
                "multiple Google accounts connected - using the first one (multi-account dispatch is not yet implemented)"
            );
        }
        Ok(accounts.into_iter().next().expect("len>=1"))
    }

    async fn bearer_for(
        &self,
        scopes: &[GoogleScope],
    ) -> Result<(AccountId, String), ToolExecutionError> {
        let account = self.resolve_account().await?;
        let token = self.connector.bearer(&account, scopes).await.map_err(|e| {
            ToolExecutionError::ExecutionFailed {
                code: "token".into(),
                message: e.to_string(),
            }
        })?;
        Ok((account, token))
    }

    /// Build a closure suitable for the `refresh` parameter expected by the
    /// connector clients. Used in retry-on-401 paths inside the HTTP layer.
    fn refresh_closure(
        connector: Arc<GoogleConnector>,
        account: AccountId,
        scopes: Vec<GoogleScope>,
    ) -> impl FnMut() -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<String, apollia_connectors::error::ConnectorError>,
                > + Send,
        >,
    > + Send {
        move || {
            let connector = connector.clone();
            let account = account.clone();
            let scopes = scopes.clone();
            Box::pin(async move { connector.bearer(&account, &scopes).await })
        }
    }
}

impl ToolExecutor for GoogleToolExecutor {
    fn name(&self) -> &str {
        self.op_id
    }

    fn is_read_only(&self) -> bool {
        matches!(
            self.op_id,
            "gcal.list_events"
                | "gcal.get_event"
                | "gdrive.workspace_list"
                | "gdrive.workspace_read"
        )
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move { dispatch_google(self, input).await })
    }
}

// ─── Dispatch table ──────────────────────────────────────────────────────────
//
// One arm per op_id. Each arm:
// 1. deserialises the JSON input into a typed struct,
// 2. resolves the bearer token for the right scopes,
// 3. calls the matching `GoogleConnector` method,
// 4. serialises the response back to JSON.

async fn dispatch_google(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    match exec.op_id {
        "gmail.send" => gmail_send(exec, input).await,
        "gmail.compose_draft" => gmail_compose_draft(exec, input).await,
        "gcal.list_events" => gcal_list_events(exec, input).await,
        "gcal.get_event" => gcal_get_event(exec, input).await,
        "gcal.create_event" => gcal_create_event(exec, input).await,
        "gcal.update_event" => gcal_update_event(exec, input).await,
        "gcal.delete_event" => gcal_delete_event(exec, input).await,
        "gdrive.workspace_list" => gdrive_list(exec, input).await,
        "gdrive.workspace_read" => gdrive_read(exec, input).await,
        "gdrive.workspace_write" => gdrive_write(exec, input).await,
        "gdrive.workspace_delete" => gdrive_delete(exec, input).await,
        "gdrive.workspace_share" => gdrive_share(exec, input).await,
        other => Err(ToolExecutionError::ExecutionFailed {
            code: "unknown_operation".into(),
            message: format!("unknown google operation: {other}"),
        }),
    }
}

fn input_get_str(input: &Value, key: &str) -> Result<String, ToolExecutionError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolExecutionError::InvalidInput {
            message: format!("missing required field `{key}`"),
        })
}

fn input_get_str_opt(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn input_get_u32_or(input: &Value, key: &str, default: u32) -> u32 {
    input
        .get(key)
        .and_then(Value::as_u64)
        .map(|n| n.min(u32::MAX as u64) as u32)
        .unwrap_or(default)
}

fn calendar_id_or_primary(input: &Value) -> String {
    input_get_str_opt(input, "calendar_id").unwrap_or_else(|| "primary".to_string())
}

fn parse_rfc3339(
    s: &str,
    label: &str,
) -> Result<chrono::DateTime<chrono::Utc>, ToolExecutionError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| ToolExecutionError::InvalidInput {
            message: format!("`{label}` must be RFC 3339 ({e})"),
        })
}

fn map_connector_err(e: apollia_connectors::error::ConnectorError) -> ToolExecutionError {
    ToolExecutionError::ExecutionFailed {
        code: "connector".into(),
        message: e.to_string(),
    }
}

/// Like [`map_connector_err`] but turns a 403 / post-refresh 401 on Gmail send
/// into an actionable "reconnect with send permission" message, since that is
/// almost always a missing send scope rather than a transient upstream error.
fn map_gmail_send_err(e: apollia_connectors::error::ConnectorError) -> ToolExecutionError {
    use apollia_connectors::error::ConnectorError;
    match e {
        ConnectorError::Upstream { status: 403, .. } | ConnectorError::Unauthorized { .. } => {
            ToolExecutionError::ExecutionFailed {
                code: "gmail_send_permission".into(),
                message: "Gmail refused the send: the connected account likely did not grant send \
                          permission. Reconnect Google with \"Envoyer des emails (Gmail)\" enabled, \
                          then retry. Do not create a draft as a substitute."
                    .into(),
            }
        }
        other => map_connector_err(other),
    }
}

// ─── Gmail ───────────────────────────────────────────────────────────────────

fn extract_compose(input: &Value) -> Result<ComposeMail, ToolExecutionError> {
    Ok(ComposeMail {
        to: input_get_str(input, "to")?,
        subject: input_get_str(input, "subject")?,
        body: input_get_str(input, "body")?,
        cc: input_get_str_opt(input, "cc"),
        bcc: input_get_str_opt(input, "bcc"),
    })
}

async fn gmail_send(exec: &GoogleToolExecutor, input: Value) -> Result<Value, ToolExecutionError> {
    let mail = extract_compose(&input)?;
    let scopes = [GoogleScope::MailSend];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let result = exec
        .connector
        .gmail()
        .send(&mail, &token, refresh)
        .await
        .map_err(map_gmail_send_err)?;
    Ok(json!({"sent": true, "message_id": result.id, "thread_id": result.thread_id}))
}

async fn gmail_compose_draft(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let mail = extract_compose(&input)?;
    let scopes = [GoogleScope::MailDraftsCreate];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let result = exec
        .connector
        .gmail()
        .compose_draft(&mail, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    Ok(json!({"sent": false, "draft_id": result.id, "thread_id": result.thread_id}))
}

// ─── Calendar ────────────────────────────────────────────────────────────────

fn build_event_time(value: &str) -> EventTime {
    // Accept both timed (RFC 3339 with `T` and timezone) and all-day (`YYYY-MM-DD`).
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

fn build_event_draft(input: &Value) -> Result<EventDraft, ToolExecutionError> {
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
        summary: input_get_str(input, "summary")?,
        description: input_get_str_opt(input, "description"),
        location: input_get_str_opt(input, "location"),
        start: build_event_time(&input_get_str(input, "start")?),
        end: build_event_time(&input_get_str(input, "end")?),
        attendees,
    })
}

async fn gcal_list_events(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let cal = calendar_id_or_primary(&input);
    let time_min = match input_get_str_opt(&input, "time_min") {
        Some(s) => Some(parse_rfc3339(&s, "time_min")?),
        None => None,
    };
    let time_max = match input_get_str_opt(&input, "time_max") {
        Some(s) => Some(parse_rfc3339(&s, "time_max")?),
        None => None,
    };
    let filter = ListEventsFilter {
        time_min,
        time_max,
        max_results: Some(input_get_u32_or(&input, "max_results", 25)),
        query: input_get_str_opt(&input, "q"),
    };
    let scopes = [GoogleScope::CalendarRead];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let events = exec
        .connector
        .calendar()
        .list_events(&cal, &filter, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    Ok(
        serde_json::to_value(&events).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialise".into(),
            message: e.to_string(),
        })?,
    )
}

async fn gcal_get_event(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let cal = calendar_id_or_primary(&input);
    let event_id = input_get_str(&input, "event_id")?;
    let scopes = [GoogleScope::CalendarRead];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let event = exec
        .connector
        .calendar()
        .get_event(&cal, &event_id, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    Ok(
        serde_json::to_value(&event).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialise".into(),
            message: e.to_string(),
        })?,
    )
}

async fn gcal_create_event(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let cal = calendar_id_or_primary(&input);
    let draft = build_event_draft(&input)?;
    let scopes = [GoogleScope::CalendarWrite];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let event = exec
        .connector
        .calendar()
        .create_event(&cal, &draft, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    Ok(
        serde_json::to_value(&event).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialise".into(),
            message: e.to_string(),
        })?,
    )
}

async fn gcal_update_event(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let cal = calendar_id_or_primary(&input);
    let event_id = input_get_str(&input, "event_id")?;
    let draft = build_event_draft(&input)?;
    let scopes = [GoogleScope::CalendarWrite];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let event = exec
        .connector
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
        .map_err(map_connector_err)?;
    Ok(
        serde_json::to_value(&event).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialise".into(),
            message: e.to_string(),
        })?,
    )
}

async fn gcal_delete_event(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let cal = calendar_id_or_primary(&input);
    let event_id = input_get_str(&input, "event_id")?;
    let scopes = [GoogleScope::CalendarWrite];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    exec.connector
        .calendar()
        .delete_event(&cal, &event_id, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    Ok(json!({"deleted": true, "event_id": event_id}))
}

// ─── Drive workspace ─────────────────────────────────────────────────────────

fn resolve_drive_root(account_id: &AccountId) -> String {
    apollia_auth::drive_prefs::effective_folder_path("google", account_id.as_str())
}

async fn gdrive_list(exec: &GoogleToolExecutor, input: Value) -> Result<Value, ToolExecutionError> {
    let agent_slug = input_get_str(&input, "agent_slug")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let root_path = resolve_drive_root(&account);
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let files = exec
        .connector
        .drive()
        .workspace_list(&root_path, &agent_slug, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    Ok(
        serde_json::to_value(&files).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialise".into(),
            message: e.to_string(),
        })?,
    )
}

async fn gdrive_read(exec: &GoogleToolExecutor, input: Value) -> Result<Value, ToolExecutionError> {
    let file_id = input_get_str(&input, "file_id")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let bytes = exec
        .connector
        .drive()
        .workspace_read(&file_id, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    // Best-effort: return as UTF-8 text. Binary blobs are dumped as base64.
    match String::from_utf8(bytes.clone()) {
        Ok(text) => Ok(json!({"file_id": file_id, "content": text, "encoding": "utf-8"})),
        Err(_) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(json!({"file_id": file_id, "content_base64": b64, "encoding": "base64"}))
        }
    }
}

async fn gdrive_write(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let agent_slug = input_get_str(&input, "agent_slug")?;
    let name = input_get_str(&input, "name")?;
    let content = input_get_str(&input, "content")?;
    let mime_type = input_get_str_opt(&input, "mime_type");
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let root_path = resolve_drive_root(&account);
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let file = exec
        .connector
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
        .map_err(map_connector_err)?;
    Ok(
        serde_json::to_value(&file).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialise".into(),
            message: e.to_string(),
        })?,
    )
}

async fn gdrive_delete(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let file_id = input_get_str(&input, "file_id")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    exec.connector
        .drive()
        .workspace_delete(&file_id, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    Ok(json!({"deleted": true, "file_id": file_id}))
}

async fn gdrive_share(
    exec: &GoogleToolExecutor,
    input: Value,
) -> Result<Value, ToolExecutionError> {
    let file_id = input_get_str(&input, "file_id")?;
    let email = input_get_str(&input, "email")?;
    let scopes = [GoogleScope::DriveWorkspace];
    let (account, token) = exec.bearer_for(&scopes).await?;
    let refresh =
        GoogleToolExecutor::refresh_closure(exec.connector.clone(), account, scopes.to_vec());
    let permission_id = exec
        .connector
        .drive()
        .workspace_share(&file_id, &email, &token, refresh)
        .await
        .map_err(map_connector_err)?;
    Ok(json!({"shared": true, "file_id": file_id, "email": email, "permission_id": permission_id}))
}
