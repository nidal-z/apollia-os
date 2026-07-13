use apollia_connectors::google::operations as google_operations;
use apollia_connectors::microsoft::operations as microsoft_operations;
use apollia_connectors::operation::{ApprovalPolicy, OperationSpec};
use apollia_core::SandboxProfile;
use apollia_tools::descriptor::{ApprovalRiskLevel, ToolDescriptor, ToolKind};
use serde_json::json;

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
