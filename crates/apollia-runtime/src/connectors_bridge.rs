//! Bridge layer that turns connector operations (Gmail, Calendar, Drive, …)
//! into [`ToolDescriptor`]s the agent runtime can register at boot.
//!
//! Descriptors are STATIC metadata — they make the LLM aware that the tool
//! exists and what it accepts. The actual EXECUTION is plugged in by
//! `apollia-desktop` (which owns the `AuthManager` singleton and the live
//! `GoogleConnector` instance) at dispatcher-build time, via a parallel
//! `connectors_bridge` module that produces [`ToolExecutor`] implementations
//! sharing the same operation IDs.
//!
//! Splitting the descriptor side (runtime) from the executor side (desktop)
//! avoids a cyclic dependency on `apollia-desktop` from within the runtime
//! while still letting the supervisor register the tool names at startup so
//! agents see them in their tool catalogue regardless of when (or whether) a
//! Google account is connected.

use std::sync::Arc;

use apollia_auth::{AccountId, AuthManager, ConnectorProvider, GoogleScope};
use apollia_connectors::error::ConnectorError;
use apollia_connectors::google::{
    calendar::{Attendee, EventDraft, EventTime, ListEventsFilter},
    gmail::ComposeMail,
    operations as google_operations, GoogleConnector,
};
use apollia_connectors::operation::{ApprovalPolicy, OperationSpec};
use apollia_core::SandboxProfile;
use apollia_llm::ToolInvoker;
use apollia_tools::descriptor::{ApprovalRiskLevel, ToolDescriptor, ToolKind};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::OnceCell;

/// Build the full set of [`ToolDescriptor`]s for the Google Workspace
/// connector. Mirrors `apollia_connectors::google::OPERATIONS` 1:1 with the
/// per-op input schema attached (the `OperationSpec` constant stores Null for
/// `input_schema` because it's a build-time const — JSON schemas are filled
/// in here, where `serde_json::json!` is available).
pub fn google_tool_descriptors() -> Vec<ToolDescriptor> {
    google_operations()
        .iter()
        .map(|op| op_to_descriptor("google", op))
        .collect()
}

/// All connector tool descriptors registered at supervisor boot. Today only
/// Google; Microsoft follows the same pattern once its executors are wired.
pub fn all_connector_descriptors() -> Vec<ToolDescriptor> {
    google_tool_descriptors()
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn op_to_descriptor(_connector_id: &str, op: &OperationSpec) -> ToolDescriptor {
    let read_only = op.is_read_only();
    let (risk_score, risk_level) = approval_to_risk(op.approval);

    ToolDescriptor {
        name: op.id.to_string(),
        version: "1.0.0".to_string(),
        description: op.description.to_string(),
        kind: ToolKind::Native,
        input_schema: input_schema_for(op.id),
        output_schema: None,
        // Connector tools make outbound HTTPS calls — they don't touch the
        // sandbox filesystem. `ReadOnly` is the closest existing profile;
        // network access is governed by the connector's own scope policy,
        // not the sandbox.
        sandbox_profile: SandboxProfile::ReadOnly,
        tags: connector_tags(op),
        dangerous: false,
        is_read_only: read_only,
        risk_score,
        approval_risk_level: Some(risk_level),
        impact_description: Some(short_impact(op)),
        reject_reason_required: matches!(op.approval, ApprovalPolicy::ConfirmPhrase),
    }
}

fn approval_to_risk(approval: ApprovalPolicy) -> (u8, ApprovalRiskLevel) {
    match approval {
        ApprovalPolicy::AutoApprove => (1, ApprovalRiskLevel::Low),
        ApprovalPolicy::AlwaysRequireApproval => (6, ApprovalRiskLevel::Medium),
        ApprovalPolicy::ConfirmPhrase => (9, ApprovalRiskLevel::Critical),
    }
}

fn connector_tags(op: &OperationSpec) -> Vec<String> {
    vec![
        "google".to_string(),
        op.service.to_string(),
        if op.is_read_only() {
            "read".to_string()
        } else {
            "write".to_string()
        },
    ]
}

fn short_impact(op: &OperationSpec) -> String {
    match op.id {
        "gmail.send" => "Sends a real email from the connected Gmail account.".into(),
        "gmail.compose_draft" => "Creates a draft in the Gmail Drafts folder.".into(),
        "gmail.list_drafts" => "Reads the list of drafts (read-only).".into(),
        "gmail.delete_draft" => "Permanently deletes a draft.".into(),
        "gcal.list_events" => "Reads calendar events (read-only).".into(),
        "gcal.get_event" => "Reads a single calendar event (read-only).".into(),
        "gcal.create_event" => "Creates a calendar event; may notify attendees.".into(),
        "gcal.update_event" => "Updates a calendar event; may re-notify attendees.".into(),
        "gcal.delete_event" => "Permanently removes a calendar event.".into(),
        "gdrive.list_my_files" => {
            "Lists files Apollia can see in Drive (its own + picker-granted).".into()
        }
        "gdrive.find_by_name" => {
            "Resolves a Drive file/folder by title — call this before asking the user for an ID."
                .into()
        }
        "gdrive.workspace_list" => "Lists files in the agent's Drive workspace folder.".into(),
        "gdrive.workspace_read" => "Reads the content of a workspace file.".into(),
        "gdrive.workspace_write" => "Writes a file under Drive/Apollia/<agent>/.".into(),
        "gdrive.workspace_delete" => "Moves a workspace file to Drive Trash.".into(),
        "gdrive.workspace_share" => "Grants reader access to an email address.".into(),
        "gsheets.list_sheets" => {
            "Lists the tabs of a spreadsheet — call before composing a range.".into()
        }
        _ => "Calls a Google Workspace API on behalf of the connected account.".into(),
    }
}

// ─── Per-operation input schemas ────────────────────────────────────────────
//
// Hand-authored JSON Schema fragments matching the parameters each connector
// method consumes. The shape mirrors the desktop-side executors that
// deserialise these payloads — keep both in sync when adding ops.

fn input_schema_for(op_id: &str) -> serde_json::Value {
    match op_id {
        "gmail.send" | "gmail.compose_draft" => json!({
            "type": "object",
            "properties": {
                "to": {"type": "string", "description": "Recipient email address."},
                "subject": {"type": "string"},
                "body": {"type": "string", "description": "Plain-text body."},
                "cc": {"type": "string", "description": "Optional CC address."},
                "bcc": {"type": "string", "description": "Optional BCC address."},
            },
            "required": ["to", "subject", "body"]
        }),
        "gmail.list_drafts" => json!({
            "type": "object",
            "properties": {
                "max_results": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20},
            },
            "required": []
        }),
        "gmail.delete_draft" => json!({
            "type": "object",
            "properties": {
                "draft_id": {"type": "string"},
            },
            "required": ["draft_id"]
        }),
        "gcal.list_events" => json!({
            "type": "object",
            "properties": {
                "calendar_id": {"type": "string", "default": "primary"},
                "time_min": {"type": "string", "description": "RFC 3339 lower bound."},
                "time_max": {"type": "string", "description": "RFC 3339 upper bound."},
                "max_results": {"type": "integer", "minimum": 1, "maximum": 250, "default": 25},
                "q": {"type": "string", "description": "Free-text filter."},
            },
            "required": []
        }),
        "gcal.get_event" => json!({
            "type": "object",
            "properties": {
                "calendar_id": {"type": "string", "default": "primary"},
                "event_id": {"type": "string"},
            },
            "required": ["event_id"]
        }),
        "gcal.create_event" | "gcal.update_event" => json!({
            "type": "object",
            "properties": {
                "calendar_id": {"type": "string", "default": "primary"},
                "event_id": {"type": "string", "description": "Required for update_event, ignored for create."},
                "summary": {"type": "string"},
                "description": {"type": "string"},
                "location": {"type": "string"},
                "start": {"type": "string", "description": "RFC 3339 start time."},
                "end": {"type": "string", "description": "RFC 3339 end time."},
                "attendees": {
                    "type": "array",
                    "items": {"type": "string", "description": "Attendee email."}
                },
            },
            "required": ["summary", "start", "end"]
        }),
        "gcal.delete_event" => json!({
            "type": "object",
            "properties": {
                "calendar_id": {"type": "string", "default": "primary"},
                "event_id": {"type": "string"},
            },
            "required": ["event_id"]
        }),
        "gdrive.find_by_name" => json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Title or substring to search for."},
                "mime_type_filter": {
                    "type": "string",
                    "description": "Restrict to one of: 'spreadsheet', 'document', 'presentation', 'folder', or a raw Drive MIME type. Omit to search all types."
                },
                "exact": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, matches the full title exactly. Default false → case-insensitive contains."
                },
            },
            "required": ["name"]
        }),
        "gdrive.list_my_files" => json!({
            "type": "object",
            "properties": {
                "folder_id": {
                    "type": "string",
                    "description": "Explicit Drive folder ID to list. Overrides the configured workspace."
                },
                "all": {
                    "type": "boolean",
                    "default": false,
                    "description": "When true, ignore the configured workspace and list every file Apollia has access to (drive.file scope, so still scoped to own + Picker grants)."
                },
                "page_size": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "default": 50
                },
            },
            "required": []
        }),
        "gdrive.workspace_list" => json!({
            "type": "object",
            "properties": {
                "agent_slug": {"type": "string", "description": "Agent name segment in the workspace path."},
            },
            "required": ["agent_slug"]
        }),
        "gdrive.workspace_read" => json!({
            "type": "object",
            "properties": {
                "file_id": {"type": "string"},
            },
            "required": ["file_id"]
        }),
        "gdrive.workspace_write" => json!({
            "type": "object",
            "properties": {
                "agent_slug": {"type": "string"},
                "name": {"type": "string", "description": "Target filename."},
                "content": {"type": "string", "description": "UTF-8 text content."},
                "mime_type": {
                    "type": "string",
                    "description": "Optional MIME type (defaults to text/plain). Use text/markdown, application/json, text/csv, etc. when the content isn't plain text."
                },
            },
            "required": ["agent_slug", "name", "content"]
        }),
        "gdrive.workspace_delete" => json!({
            "type": "object",
            "properties": {
                "file_id": {"type": "string"},
            },
            "required": ["file_id"]
        }),
        "gdrive.workspace_share" => json!({
            "type": "object",
            "properties": {
                "file_id": {"type": "string"},
                "email": {"type": "string", "description": "Email address to grant reader access."},
            },
            "required": ["file_id", "email"]
        }),
        "gdrive.list_picked_folders" => json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        "gdrive.list_files_in" => json!({
            "type": "object",
            "properties": {
                "folder_id": {"type": "string", "description": "Drive folder ID (from list_picked_folders)."},
            },
            "required": ["folder_id"]
        }),
        "gdrive.read_file" => json!({
            "type": "object",
            "properties": {
                "file_id": {"type": "string"},
            },
            "required": ["file_id"]
        }),
        "gdrive.write_to_folder" => json!({
            "type": "object",
            "properties": {
                "folder_id": {
                    "type": "string",
                    "description": "Explicit Drive folder ID. Overrides the configured workspace."
                },
                "at_drive_root": {
                    "type": "boolean",
                    "default": false,
                    "description": "When true, bypass the configured workspace and write directly at My Drive root."
                },
                "name": {"type": "string", "description": "Target filename."},
                "content": {"type": "string", "description": "UTF-8 text content."},
                "mime_type": {
                    "type": "string",
                    "description": "Optional MIME type (defaults to text/plain). Set explicitly for text/markdown, application/json, text/csv, etc."
                },
            },
            "required": ["name", "content"]
        }),
        // Sheets
        "gsheets.create" => json!({
            "type": "object",
            "properties": {"title": {"type": "string"}},
            "required": ["title"]
        }),
        "gsheets.list_sheets" => json!({
            "type": "object",
            "properties": {
                "spreadsheet_id": {"type": "string"},
            },
            "required": ["spreadsheet_id"]
        }),
        "gsheets.read_values" => json!({
            "type": "object",
            "properties": {
                "spreadsheet_id": {"type": "string"},
                "range": {"type": "string", "description": "A1 notation, e.g. Sheet1!A1:C10"},
            },
            "required": ["spreadsheet_id", "range"]
        }),
        "gsheets.append_values" | "gsheets.update_values" => json!({
            "type": "object",
            "properties": {
                "spreadsheet_id": {"type": "string"},
                "range": {"type": "string"},
                "values": {"type": "array", "items": {"type": "array"}},
            },
            "required": ["spreadsheet_id", "range", "values"]
        }),
        // Docs
        "gdocs.create" | "gslides.create" | "gforms.create" => json!({
            "type": "object",
            "properties": {"title": {"type": "string"}},
            "required": ["title"]
        }),
        "gdocs.read_text" => json!({
            "type": "object",
            "properties": {"document_id": {"type": "string"}},
            "required": ["document_id"]
        }),
        "gdocs.append_text" => json!({
            "type": "object",
            "properties": {
                "document_id": {"type": "string"},
                "text": {"type": "string"},
            },
            "required": ["document_id", "text"]
        }),
        // Slides
        "gslides.append_slide" => json!({
            "type": "object",
            "properties": {
                "presentation_id": {"type": "string"},
                "text": {"type": "string"},
            },
            "required": ["presentation_id", "text"]
        }),
        // Tasks
        "gtasks.list_lists" => json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        "gtasks.list_tasks" => json!({
            "type": "object",
            "properties": {"task_list_id": {"type": "string"}},
            "required": ["task_list_id"]
        }),
        "gtasks.create" => json!({
            "type": "object",
            "properties": {
                "task_list_id": {"type": "string"},
                "title": {"type": "string"},
                "notes": {"type": "string"},
                "due": {"type": "string", "description": "RFC 3339 timestamp"},
            },
            "required": ["task_list_id", "title"]
        }),
        "gtasks.complete" | "gtasks.delete" => json!({
            "type": "object",
            "properties": {
                "task_list_id": {"type": "string"},
                "task_id": {"type": "string"},
            },
            "required": ["task_list_id", "task_id"]
        }),
        // YouTube
        "youtube.search" => json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "max_results": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10},
            },
            "required": ["query"]
        }),
        "youtube.video_details" => json!({
            "type": "object",
            "properties": {"video_id": {"type": "string"}},
            "required": ["video_id"]
        }),
        _ => json!({"type": "object", "properties": {}, "required": []}),
    }
}

// ─── Chat Libre invoker (Google) ────────────────────────────────────────────
//
// `NativeChatToolInvoker` uses a hardcoded match on tool names. Connector
// tools are not in that match — so we surface an injected [`ToolInvoker`]
// implementation it can delegate to when the LLM calls `gmail.send`,
// `gcal.list_events`, `gdrive.workspace_write`, etc.
//
// The invoker owns lazy-initialised [`AuthManager`] + [`GoogleConnector`]
// singletons. They share the OS keychain with the parallel singleton in
// `apollia-desktop::commands::integrations`, so OAuth tokens written by the
// desktop side are picked up here transparently. Singleflight refresh inside
// `AuthManager` keeps the two caches from racing.

static GOOGLE_AUTH: OnceCell<Arc<AuthManager>> = OnceCell::const_new();
static GOOGLE_CONNECTOR: OnceCell<Arc<GoogleConnector>> = OnceCell::const_new();

async fn get_google_connector() -> Result<Arc<GoogleConnector>, String> {
    let auth = GOOGLE_AUTH
        .get_or_try_init(|| async {
            AuthManager::new()
                .map(Arc::new)
                .map_err(|e| format!("auth init failed: {e}"))
        })
        .await?
        .clone();
    GOOGLE_CONNECTOR
        .get_or_try_init(|| async {
            GoogleConnector::new(auth.clone())
                .map(Arc::new)
                .map_err(|e| format!("google connector init failed: {e}"))
        })
        .await
        .cloned()
}

async fn get_auth() -> Result<Arc<AuthManager>, String> {
    GOOGLE_AUTH
        .get_or_try_init(|| async {
            AuthManager::new()
                .map(Arc::new)
                .map_err(|e| format!("auth init failed: {e}"))
        })
        .await
        .cloned()
}

/// Tool invoker that routes connector operation names (`gmail.send`,
/// `gcal.list_events`, `gdrive.workspace_*`, …) to the in-process Google
/// Workspace connector. Returns the connector response as a stringified
/// JSON blob — same convention as the other Chat Libre tool invokers.
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
        "gmail.list_drafts" => gmail_list_drafts(&connector, &auth, input).await,
        "gmail.delete_draft" => gmail_delete_draft(&connector, &auth, input).await,
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

// ─── Per-op handlers — these mirror the desktop bridge but return `Value` ──

async fn resolve_account(auth: &Arc<AuthManager>) -> Result<AccountId, String> {
    let accounts = auth
        .list_accounts(ConnectorProvider::Google)
        .await
        .map_err(|e| format!("auth: {e}"))?;
    if accounts.is_empty() {
        return Err("no Google account connected — open Réglages → Intégrations to sign in".into());
    }
    if accounts.len() > 1 {
        tracing::warn!(
            count = accounts.len(),
            "multiple Google accounts connected — using the first"
        );
    }
    Ok(accounts.into_iter().next().expect("len>=1"))
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

fn get_str(input: &Value, key: &str) -> Result<String, String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing required field `{key}`"))
}

fn get_str_opt(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn get_u32_or(input: &Value, key: &str, default: u32) -> u32 {
    input
        .get(key)
        .and_then(Value::as_u64)
        .map(|n| n.min(u32::MAX as u64) as u32)
        .unwrap_or(default)
}

fn parse_rfc3339(s: &str, label: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| format!("`{label}` must be RFC 3339 ({e})"))
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

async fn gmail_send(
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
        .map_err(|e| e.to_string())?;
    Ok(json!({"id": result.id, "thread_id": result.thread_id}))
}

async fn gmail_compose_draft(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let mail = extract_compose(input)?;
    let scopes = [GoogleScope::MailCompose];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let result = connector
        .gmail()
        .compose_draft(&mail, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"id": result.id, "thread_id": result.thread_id}))
}

async fn gmail_list_drafts(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let max_results = get_u32_or(input, "max_results", 20);
    let scopes = [GoogleScope::MailCompose];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    let drafts = connector
        .gmail()
        .list_drafts(max_results, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&drafts).map_err(|e| e.to_string())
}

async fn gmail_delete_draft(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let draft_id = get_str(input, "draft_id")?;
    let scopes = [GoogleScope::MailCompose];
    let (account, token) = bearer_for(connector, auth, &scopes).await?;
    let refresh = refresh_closure(connector.clone(), account, scopes.to_vec());
    connector
        .gmail()
        .delete_draft(&draft_id, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"deleted": true, "draft_id": draft_id}))
}

async fn gcal_list_events(
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

async fn gcal_get_event(
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

async fn gcal_create_event(
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

async fn gcal_update_event(
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
        .update_event(&cal, &event_id, &draft, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&event).map_err(|e| e.to_string())
}

async fn gcal_delete_event(
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

async fn gdrive_list_my_files(
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
                        "Configured root `{root_path}` doesn't exist on Drive yet — Apollia hasn't created or written anything in it. Either ask the user to create the folder manually, or call `gdrive.write_to_folder` to materialise it. Pass `all=true` to bypass the workspace scope and see every Drive file Apollia has access to."
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

async fn gdrive_find_by_name(
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
        .find_by_name(&name, mime_filter.as_deref(), exact, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "matches": files,
        "query": {"name": name, "mime_type_filter": mime_filter, "exact": exact},
    }))
}

async fn gdrive_workspace_list(
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

async fn gdrive_workspace_read(
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

async fn gdrive_workspace_write(
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
            &root_path,
            &agent_slug,
            &name,
            content.as_bytes(),
            mime_type.as_deref(),
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

async fn gdrive_workspace_delete(
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

async fn gdrive_list_picked_folders(auth: &Arc<AuthManager>) -> Result<Value, String> {
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

async fn gdrive_list_files_in(
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

async fn gdrive_read_file(
    connector: &Arc<GoogleConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    // Same as gdrive.workspace_read but accessible outside the Apollia
    // root — Apollia has `drive.file` access to the file because it lives
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

async fn gdrive_write_to_folder(
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
            target_folder.as_deref(),
            &name,
            content.as_bytes(),
            mime_type.as_deref(),
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

async fn gdrive_workspace_share(
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

// ─── Tier 1 service handlers (Sheets / Docs / Slides / Forms / Tasks / YT) ─

async fn gsheets_create(
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

async fn gsheets_list_sheets(
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

async fn gsheets_read_values(
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

async fn gsheets_append_values(
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
        .append_values(&id, &range, &values, &token, refresh)
        .await
        .map_err(|e| e.to_string())
}

async fn gsheets_update_values(
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
        .update_values(&id, &range, &values, &token, refresh)
        .await
        .map_err(|e| e.to_string())
}

async fn gdocs_create(
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

async fn gdocs_read_text(
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

async fn gdocs_append_text(
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

async fn gslides_create(
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

async fn gslides_append_slide(
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

async fn gtasks_list_lists(
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

async fn gtasks_list_tasks(
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

async fn gtasks_create(
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
            &list_id,
            &title,
            notes.as_deref(),
            due.as_deref(),
            &token,
            refresh,
        )
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&task).map_err(|e| e.to_string())
}

async fn gtasks_complete(
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

async fn gtasks_delete(
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

async fn gforms_create(
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

async fn youtube_search(
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

async fn youtube_video_details(
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

// ─── ToolExecutor wrappers (Phase 2 — ADR-096) ──────────────────────────────
//
// `GoogleChatToolInvoker` implements `ToolInvoker` (string-typed). To plug
// the connector ops into a unified `ToolDispatcher` alongside MCP + native
// executors, we wrap each op in a tiny `ToolExecutor` that forwards through
// the shared invoker. Reuses the same `OnceCell` singletons → no second
// AuthManager, no second connector instance.

use apollia_tools::executor::{ToolExecutionError, ToolExecutor as DispatchableExecutor};

struct GoogleOpExecutor {
    op_id: &'static str,
}

#[async_trait]
impl DispatchableExecutor for GoogleOpExecutor {
    fn name(&self) -> &str {
        self.op_id
    }

    fn is_read_only(&self) -> bool {
        matches!(
            self.op_id,
            "gmail.list_drafts"
                | "gcal.list_events"
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

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        dispatch_google_tool(self.op_id, &input)
            .await
            .map_err(|msg| ToolExecutionError::ExecutionFailed {
                code: "google".into(),
                message: msg,
            })
    }
}

/// Build a [`ToolExecutor`] per Google op so they can all be plugged into a
/// single shared [`ToolDispatcher`] alongside MCP + future-provider tools.
pub fn build_google_executors() -> Vec<Box<dyn DispatchableExecutor>> {
    [
        "gmail.send",
        "gmail.compose_draft",
        "gmail.list_drafts",
        "gmail.delete_draft",
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_descriptors_cover_every_operation() {
        let ops_count = google_operations().len();
        let descriptors = google_tool_descriptors();
        assert_eq!(
            descriptors.len(),
            ops_count,
            "one descriptor per Google operation"
        );
    }

    #[test]
    fn every_descriptor_validates() {
        for d in google_tool_descriptors() {
            d.validate().unwrap_or_else(|e| {
                panic!("descriptor `{}` failed validation: {e}", d.name);
            });
        }
    }

    #[test]
    fn read_ops_are_marked_read_only() {
        let descs = google_tool_descriptors();
        for name in [
            "gmail.list_drafts",
            "gcal.list_events",
            "gcal.get_event",
            "gdrive.workspace_list",
            "gdrive.workspace_read",
        ] {
            let d = descs.iter().find(|d| d.name == name).expect(name);
            assert!(d.is_read_only, "{name} should be is_read_only=true");
        }
    }

    #[test]
    fn write_ops_have_medium_or_higher_risk() {
        let descs = google_tool_descriptors();
        for name in ["gmail.send", "gcal.create_event", "gdrive.workspace_write"] {
            let d = descs.iter().find(|d| d.name == name).expect(name);
            assert!(
                matches!(
                    d.approval_risk_level,
                    Some(
                        ApprovalRiskLevel::Medium
                            | ApprovalRiskLevel::High
                            | ApprovalRiskLevel::Critical
                    ),
                ),
                "{name} should require approval"
            );
        }
    }

    #[test]
    fn delete_event_requires_confirm_phrase() {
        let descs = google_tool_descriptors();
        let d = descs
            .iter()
            .find(|d| d.name == "gcal.delete_event")
            .expect("delete_event");
        assert!(
            d.reject_reason_required,
            "ConfirmPhrase approval policy must set reject_reason_required"
        );
        assert_eq!(d.approval_risk_level, Some(ApprovalRiskLevel::Critical));
    }
}
