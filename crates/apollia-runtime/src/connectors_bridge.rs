//! Bridge layer that turns connector operations (Gmail, Calendar, Drive, …)
//! into [`ToolDescriptor`]s the agent runtime can register at boot.
//!
//! Descriptors are STATIC metadata, they make the LLM aware that the tool
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

use apollia_auth::{AccountId, AuthManager, ConnectorProvider, GoogleScope, MicrosoftScope};
use apollia_connectors::error::ConnectorError;
use apollia_connectors::google::{
    calendar::{Attendee, EventDraft, EventTime, EventUpdate, ListEventsFilter},
    drive_workspace::{FolderWrite, NameSearch, WorkspaceWrite},
    gmail::ComposeMail,
    operations as google_operations,
    sheets::ValueWrite,
    tasks::NewTask,
    GoogleConnector,
};
use apollia_connectors::microsoft::{
    calendar::{
        EventAttendee as MsEventAttendee, EventBody as MsEventBody,
        EventDateTime as MsEventDateTime, EventDraft as MsEventDraft,
        EventLocation as MsEventLocation, ListEventsFilter as MsListEventsFilter,
    },
    mail::{
        BodyContentType as MsBodyContentType, ComposeMessage as MsComposeMessage,
        EmailAddress as MsEmailAddress, MessageBody as MsMessageBody, Recipient as MsRecipient,
    },
    operations as microsoft_operations, MicrosoftConnector,
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
/// `input_schema` because it's a build-time const, JSON schemas are filled
/// in here, where `serde_json::json!` is available).
pub fn google_tool_descriptors() -> Vec<ToolDescriptor> {
    google_operations()
        .iter()
        .map(|op| op_to_descriptor("google", op))
        .collect()
}

/// Tool descriptors for every Microsoft 365 operation.
pub fn microsoft_tool_descriptors() -> Vec<ToolDescriptor> {
    microsoft_operations()
        .iter()
        .map(|op| op_to_descriptor("microsoft", op))
        .collect()
}

/// All connector tool descriptors registered at supervisor boot (Google + Microsoft).
pub fn all_connector_descriptors() -> Vec<ToolDescriptor> {
    let mut descs = google_tool_descriptors();
    descs.extend(microsoft_tool_descriptors());
    descs
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn op_to_descriptor(connector_id: &str, op: &OperationSpec) -> ToolDescriptor {
    let read_only = op.is_read_only();
    let (risk_score, risk_level) = approval_to_risk(op.approval);

    ToolDescriptor {
        name: op.id.to_string(),
        version: "1.0.0".to_string(),
        description: op.description.to_string(),
        kind: ToolKind::Native,
        input_schema: input_schema_for(op.id),
        output_schema: None,
        // Connector tools make outbound HTTPS calls, they don't touch the
        // sandbox filesystem. `ReadOnly` is the closest existing profile;
        // network access is governed by the connector's own scope policy,
        // not the sandbox.
        sandbox_profile: SandboxProfile::ReadOnly,
        tags: connector_tags(connector_id, op),
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

fn connector_tags(connector_id: &str, op: &OperationSpec) -> Vec<String> {
    vec![
        connector_id.to_string(),
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
        "gcal.list_events" => "Reads calendar events (read-only).".into(),
        "gcal.get_event" => "Reads a single calendar event (read-only).".into(),
        "gcal.create_event" => "Creates a calendar event; may notify attendees.".into(),
        "gcal.update_event" => "Updates a calendar event; may re-notify attendees.".into(),
        "gcal.delete_event" => "Permanently removes a calendar event.".into(),
        "gdrive.list_my_files" => {
            "Lists files Apollia can see in Drive (its own + picker-granted).".into()
        }
        "gdrive.find_by_name" => {
            "Resolves a Drive file/folder by title - call this before asking the user for an ID."
                .into()
        }
        "gdrive.workspace_list" => "Lists files in the agent's Drive workspace folder.".into(),
        "gdrive.workspace_read" => "Reads the content of a workspace file.".into(),
        "gdrive.workspace_write" => "Writes a file under Drive/Apollia/<agent>/.".into(),
        "gdrive.workspace_delete" => "Moves a workspace file to Drive Trash.".into(),
        "gdrive.workspace_share" => "Grants reader access to an email address.".into(),
        "gsheets.list_sheets" => {
            "Lists the tabs of a spreadsheet - call before composing a range.".into()
        }
        "outlook.search" => "Searches the connected Outlook mailbox (read-only).".into(),
        "outlook.get" => "Reads a single Outlook message (read-only).".into(),
        "outlook.send" => "Sends a real email from the connected Outlook account.".into(),
        "outlook.reply" => "Sends a reply to an Outlook message.".into(),
        "outlook.list_folders" => "Lists Outlook mail folders (read-only).".into(),
        "outlook.move" => "Moves an Outlook message to another folder.".into(),
        "outlook_cal.list_events" => "Reads Outlook calendar events (read-only).".into(),
        "outlook_cal.get_event" => "Reads a single Outlook event (read-only).".into(),
        "outlook_cal.create_event" => "Creates an Outlook event; may notify attendees.".into(),
        "outlook_cal.update_event" => "Updates an Outlook event; may re-notify attendees.".into(),
        "outlook_cal.delete_event" => "Permanently removes an Outlook event.".into(),
        "onedrive.search" => "Searches OneDrive (read-only).".into(),
        "onedrive.get_metadata" => "Reads OneDrive item metadata (read-only).".into(),
        "onedrive.download" => "Downloads a OneDrive item's content (read-only).".into(),
        "onedrive.list_recent" => "Lists recent OneDrive items (read-only).".into(),
        _ => "Calls a connected cloud account API on behalf of the user.".into(),
    }
}

// ─── Per-operation input schemas ────────────────────────────────────────────
//
// Hand-authored JSON Schema fragments matching the parameters each connector
// method consumes. The shape mirrors the desktop-side executors that
// deserialise these payloads, keep both in sync when adding ops.

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
        "outlook.search" => json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Free-text search (sender, subject, body)."},
                "top": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10}
            },
            "required": ["query"]
        }),
        "outlook.get" => json!({
            "type": "object",
            "properties": {"message_id": {"type": "string"}},
            "required": ["message_id"]
        }),
        "outlook.send" => json!({
            "type": "object",
            "properties": {
                "to": {"type": "array", "items": {"type": "string"}, "description": "Recipient email addresses."},
                "subject": {"type": "string"},
                "body": {"type": "string", "description": "Plain-text body."},
                "cc": {"type": "array", "items": {"type": "string"}},
                "bcc": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["to", "subject", "body"]
        }),
        "outlook.reply" => json!({
            "type": "object",
            "properties": {
                "message_id": {"type": "string"},
                "comment": {"type": "string", "description": "Reply body."}
            },
            "required": ["message_id", "comment"]
        }),
        "outlook.list_folders" => json!({"type": "object", "properties": {}, "required": []}),
        "outlook.move" => json!({
            "type": "object",
            "properties": {
                "message_id": {"type": "string"},
                "destination_folder_id": {"type": "string"}
            },
            "required": ["message_id", "destination_folder_id"]
        }),
        "outlook_cal.list_events" => json!({
            "type": "object",
            "properties": {
                "start_after": {"type": "string", "description": "RFC 3339 lower bound (UTC)."},
                "end_before": {"type": "string", "description": "RFC 3339 upper bound (UTC)."},
                "top": {"type": "integer", "minimum": 1, "maximum": 100, "default": 25}
            },
            "required": []
        }),
        "outlook_cal.get_event" => json!({
            "type": "object",
            "properties": {"event_id": {"type": "string"}},
            "required": ["event_id"]
        }),
        "outlook_cal.create_event" => json!({
            "type": "object",
            "properties": {
                "subject": {"type": "string"},
                "start": {"type": "string", "description": "RFC 3339 start (UTC)."},
                "end": {"type": "string", "description": "RFC 3339 end (UTC)."},
                "body": {"type": "string", "description": "Optional plain-text description."},
                "location": {"type": "string"},
                "attendees": {"type": "array", "items": {"type": "string"}, "description": "Attendee email addresses."}
            },
            "required": ["subject", "start", "end"]
        }),
        "outlook_cal.update_event" => json!({
            "type": "object",
            "properties": {
                "event_id": {"type": "string"},
                "subject": {"type": "string"},
                "start": {"type": "string", "description": "RFC 3339 start (UTC)."},
                "end": {"type": "string", "description": "RFC 3339 end (UTC)."},
                "body": {"type": "string"},
                "location": {"type": "string"},
                "attendees": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["event_id", "subject", "start", "end"]
        }),
        "outlook_cal.delete_event" => json!({
            "type": "object",
            "properties": {"event_id": {"type": "string"}},
            "required": ["event_id"]
        }),
        "onedrive.search" => json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "top": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10}
            },
            "required": ["query"]
        }),
        "onedrive.get_metadata" => json!({
            "type": "object",
            "properties": {"item_id": {"type": "string"}},
            "required": ["item_id"]
        }),
        "onedrive.download" => json!({
            "type": "object",
            "properties": {"item_id": {"type": "string"}},
            "required": ["item_id"]
        }),
        "onedrive.list_recent" => json!({
            "type": "object",
            "properties": {"top": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10}},
            "required": []
        }),
        _ => json!({"type": "object", "properties": {}, "required": []}),
    }
}

// ─── Chat Libre invoker (Google) ────────────────────────────────────────────
//
// `NativeChatToolInvoker` uses a hardcoded match on tool names. Connector
// tools are not in that match, so we surface an injected [`ToolInvoker`]
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
    if accounts.is_empty() {
        return Err("no Google account connected - open Réglages → Intégrations to sign in".into());
    }
    if accounts.len() > 1 {
        tracing::warn!(
            count = accounts.len(),
            "multiple Google accounts connected - using the first"
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
        .map_err(gmail_send_error)?;
    Ok(json!({"sent": true, "message_id": result.id, "thread_id": result.thread_id}))
}

async fn gmail_compose_draft(
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

// ToolExecutor wrappers (Phase 2)
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

async fn ms_resolve_account(auth: &Arc<AuthManager>) -> Result<AccountId, String> {
    let accounts = auth
        .list_accounts(ConnectorProvider::Microsoft)
        .await
        .map_err(|e| format!("auth: {e}"))?;
    if accounts.is_empty() {
        return Err(
            "no Microsoft account connected - open Réglages → Intégrations to sign in".into(),
        );
    }
    if accounts.len() > 1 {
        tracing::warn!(
            count = accounts.len(),
            "multiple Microsoft accounts connected - using the first"
        );
    }
    Ok(accounts.into_iter().next().expect("len>=1"))
}

async fn ms_bearer_for(
    connector: &Arc<MicrosoftConnector>,
    auth: &Arc<AuthManager>,
    scopes: &[MicrosoftScope],
) -> Result<(AccountId, String), String> {
    let account = ms_resolve_account(auth).await?;
    let token = connector
        .bearer(&account, scopes)
        .await
        .map_err(|e| format!("token: {e}"))?;
    Ok((account, token))
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

fn get_str_array(input: &Value, key: &str) -> Vec<String> {
    input
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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

async fn dispatch_microsoft_tool(tool_name: &str, input: &Value) -> Result<Value, String> {
    let connector = get_microsoft_connector().await?;
    let auth = get_auth().await?;
    match tool_name {
        "outlook.search" => outlook_search(&connector, &auth, input).await,
        "outlook.get" => outlook_get(&connector, &auth, input).await,
        "outlook.send" => outlook_send(&connector, &auth, input).await,
        "outlook.reply" => outlook_reply(&connector, &auth, input).await,
        "outlook.list_folders" => outlook_list_folders(&connector, &auth).await,
        "outlook.move" => outlook_move(&connector, &auth, input).await,
        "outlook_cal.list_events" => outlook_cal_list_events(&connector, &auth, input).await,
        "outlook_cal.get_event" => outlook_cal_get_event(&connector, &auth, input).await,
        "outlook_cal.create_event" => outlook_cal_create_event(&connector, &auth, input).await,
        "outlook_cal.update_event" => outlook_cal_update_event(&connector, &auth, input).await,
        "outlook_cal.delete_event" => outlook_cal_delete_event(&connector, &auth, input).await,
        "onedrive.search" => onedrive_search(&connector, &auth, input).await,
        "onedrive.get_metadata" => onedrive_get_metadata(&connector, &auth, input).await,
        "onedrive.download" => onedrive_download(&connector, &auth, input).await,
        "onedrive.list_recent" => onedrive_list_recent(&connector, &auth, input).await,
        other => Err(format!("unknown microsoft tool: {other}")),
    }
}

async fn outlook_search(
    connector: &Arc<MicrosoftConnector>,
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let query = get_str(input, "query")?;
    let top = get_u32_or(input, "top", 10);
    let scopes = [MicrosoftScope::MailRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let message_id = get_str(input, "message_id")?;
    let scopes = [MicrosoftScope::MailRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
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
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let message_id = get_str(input, "message_id")?;
    let comment = get_str(input, "comment")?;
    let scopes = [MicrosoftScope::MailSend];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
) -> Result<Value, String> {
    let scopes = [MicrosoftScope::MailRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let message_id = get_str(input, "message_id")?;
    let destination = get_str(input, "destination_folder_id")?;
    let scopes = [MicrosoftScope::MailRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
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
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let event_id = get_str(input, "event_id")?;
    let scopes = [MicrosoftScope::CalendarRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let draft = build_ms_event_draft(input)?;
    let scopes = [MicrosoftScope::CalendarWrite];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let event_id = get_str(input, "event_id")?;
    let draft = build_ms_event_draft(input)?;
    let scopes = [MicrosoftScope::CalendarWrite];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let event_id = get_str(input, "event_id")?;
    let scopes = [MicrosoftScope::CalendarWrite];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let query = get_str(input, "query")?;
    let top = get_u32_or(input, "top", 10);
    let scopes = [MicrosoftScope::FilesRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let item_id = get_str(input, "item_id")?;
    let scopes = [MicrosoftScope::FilesRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let item_id = get_str(input, "item_id")?;
    let scopes = [MicrosoftScope::FilesRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
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
    auth: &Arc<AuthManager>,
    input: &Value,
) -> Result<Value, String> {
    let top = get_u32_or(input, "top", 10);
    let scopes = [MicrosoftScope::FilesRead];
    let (account, token) = ms_bearer_for(connector, auth, &scopes).await?;
    let refresh = ms_refresh_closure(connector.clone(), account, scopes.to_vec());
    let items = connector
        .onedrive()
        .list_recent(top, &token, refresh)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&items).map_err(|e| e.to_string())
}

struct MicrosoftOpExecutor {
    op_id: &'static str,
}

#[async_trait]
impl DispatchableExecutor for MicrosoftOpExecutor {
    fn name(&self) -> &str {
        self.op_id
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

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        dispatch_microsoft_tool(self.op_id, &input)
            .await
            .map_err(|msg| ToolExecutionError::ExecutionFailed {
                code: "microsoft".into(),
                message: msg,
            })
    }
}

/// Build one [`ToolExecutor`] per Microsoft 365 op for the shared dispatcher.
pub fn build_microsoft_executors() -> Vec<Box<dyn DispatchableExecutor>> {
    [
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
    ]
    .into_iter()
    .map(|id| Box::new(MicrosoftOpExecutor { op_id: id }) as Box<dyn DispatchableExecutor>)
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
