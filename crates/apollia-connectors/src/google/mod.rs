//! Google Workspace connector, implements [`Connector`](crate::trait_def::Connector).
//!
//! Composes the Gmail / Calendar / Drive Workspace clients into a single
//! connector instance registered with the runtime [`ConnectorRegistry`](crate::registry::ConnectorRegistry).

pub mod calendar;
pub mod docs;
pub mod drive_workspace;
pub mod forms;
pub mod gmail;
pub mod sheets;
pub mod slides;
pub mod tasks;
pub mod youtube;

use std::sync::Arc;

use apollia_auth::{AccountId, AuthManager, ConnectorProvider, GoogleScope};
use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    error::ConnectorError,
    http::HttpClient,
    manifest::ConnectorManifest,
    operation::{ApprovalPolicy, OperationSpec},
    trait_def::{Connector, HealthReport},
};

pub use calendar::CalendarClient;
pub use docs::DocsClient;
pub use drive_workspace::DriveWorkspaceClient;
pub use forms::FormsClient;
pub use gmail::GmailClient;
pub use sheets::SheetsClient;
pub use slides::SlidesClient;
pub use tasks::TasksClient;
pub use youtube::YouTubeClient;

const GOOGLE_PROVIDER_ID: &str = "google";

const MANIFEST: ConnectorManifest = ConnectorManifest {
    id: GOOGLE_PROVIDER_ID,
    name: "Google Workspace",
    description: "Gmail (send + compose), Google Calendar, Google Drive Workspace",
    publisher: "Google LLC",
    services: &["gmail", "gcal", "gdrive"],
};

/// Google Workspace connector. Construct once at startup, register with the
/// [`ConnectorRegistry`](crate::registry::ConnectorRegistry).
///
/// Holds typed sub-clients (Gmail, Calendar, Drive Workspace) and an
/// [`AuthManager`] handle used to resolve tokens at call time.
#[derive(Clone)]
pub struct GoogleConnector {
    auth: Arc<AuthManager>,
    gmail: GmailClient,
    calendar: CalendarClient,
    drive: DriveWorkspaceClient,
    sheets: SheetsClient,
    docs: DocsClient,
    slides: SlidesClient,
    forms: FormsClient,
    tasks: TasksClient,
    youtube: YouTubeClient,
}

impl GoogleConnector {
    /// Build a Google connector.
    pub fn new(auth: Arc<AuthManager>) -> Result<Self, ConnectorError> {
        let http = HttpClient::new(GOOGLE_PROVIDER_ID)?;
        Ok(Self {
            auth,
            gmail: GmailClient::new(http.clone()),
            calendar: CalendarClient::new(http.clone()),
            drive: DriveWorkspaceClient::new(http.clone()),
            sheets: SheetsClient::new(http.clone()),
            docs: DocsClient::new(http.clone()),
            slides: SlidesClient::new(http.clone()),
            forms: FormsClient::new(http.clone()),
            tasks: TasksClient::new(http.clone()),
            youtube: YouTubeClient::new(http),
        })
    }

    /// Access the Gmail sub-client.
    pub fn gmail(&self) -> &GmailClient {
        &self.gmail
    }

    /// Access the Calendar sub-client.
    pub fn calendar(&self) -> &CalendarClient {
        &self.calendar
    }

    /// Access the Drive Workspace sub-client.
    pub fn drive(&self) -> &DriveWorkspaceClient {
        &self.drive
    }

    /// Access the Sheets sub-client.
    pub fn sheets(&self) -> &SheetsClient {
        &self.sheets
    }

    /// Access the Docs sub-client.
    pub fn docs(&self) -> &DocsClient {
        &self.docs
    }

    /// Access the Slides sub-client.
    pub fn slides(&self) -> &SlidesClient {
        &self.slides
    }

    /// Access the Forms sub-client.
    pub fn forms(&self) -> &FormsClient {
        &self.forms
    }

    /// Access the Tasks sub-client.
    pub fn tasks(&self) -> &TasksClient {
        &self.tasks
    }

    /// Access the YouTube sub-client.
    pub fn youtube(&self) -> &YouTubeClient {
        &self.youtube
    }

    /// Resolve the current bearer token for `account_id`.
    ///
    /// Connector implementations call this on each request; the underlying
    /// [`AuthManager`] caches and refreshes (with singleflight) so successive
    /// calls are cheap.
    pub async fn bearer(
        &self,
        account_id: &AccountId,
        scopes: &[GoogleScope],
    ) -> Result<String, ConnectorError> {
        let cfg = apollia_auth::build_google_provider(scopes);
        let token = self
            .auth
            .get_valid_token(ConnectorProvider::Google, account_id, &cfg)
            .await?;
        Ok(token.access_token)
    }
}

#[async_trait]
impl Connector for GoogleConnector {
    fn id(&self) -> &'static str {
        GOOGLE_PROVIDER_ID
    }

    fn manifest(&self) -> &ConnectorManifest {
        &MANIFEST
    }

    fn operations(&self) -> &[OperationSpec] {
        OPERATIONS
    }

    async fn check(&self, account_id: &AccountId) -> Result<HealthReport, ConnectorError> {
        // The cheapest authenticated probe Google offers is the OIDC userinfo
        // endpoint, which only requires the profile scope (always implicitly
        // granted).
        let cfg = apollia_auth::build_google_provider(&[]);
        let token = self
            .auth
            .get_valid_token(ConnectorProvider::Google, account_id, &cfg)
            .await?;
        let access = token.access_token.clone();
        let access_for_refresh = access.clone();
        let http = HttpClient::new(GOOGLE_PROVIDER_ID)?;
        let info: UserInfo = http
            .get_json(
                "https://www.googleapis.com/oauth2/v3/userinfo",
                &access,
                || async move { Ok::<_, ConnectorError>(access_for_refresh) },
            )
            .await?;
        Ok(HealthReport {
            reachable: true,
            granted_scopes: token.scopes,
            detail: format!(
                "connected as {}",
                info.email.unwrap_or_else(|| "<unknown>".into())
            ),
        })
    }
}

/// Minimal subset of the Google OIDC userinfo response.
#[derive(Debug, Deserialize)]
struct UserInfo {
    #[serde(default)]
    email: Option<String>,
}

// ─── Operations catalog ──────────────────────────────────────────────────────

/// Public accessor for the static [`OPERATIONS`] catalog, used by
/// `apollia-runtime::connectors_bridge` to derive [`ToolDescriptor`]s
/// without holding a live `GoogleConnector` instance.
pub fn operations() -> &'static [OperationSpec] {
    OPERATIONS
}

/// All operations exposed by the Google Workspace connector.
///
/// Build-time array consumed by `apollia-tools::registry` to register each
/// operation as a native tool with the canonical naming `<service>.<action>`.
const OPERATIONS: &[OperationSpec] = &[
    OperationSpec {
        id: "gmail.send",
        service: "gmail",
        action: "send",
        scopes_required: &["mail.send"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Send an email via Gmail.\n\nWhen to use: when the user explicitly asks to email or send something to someone. This is the only tool that actually sends; do NOT fall back to gmail.compose_draft when the user asked to send. If sending fails (e.g. missing send permission), report the error instead of silently creating a draft.\nInputs: to (email), subject (string), body (plain text), optional cc, bcc.\nApproval: required.\nReturns: {\"sent\": true, \"message_id\", \"thread_id\"} once delivered.\nSide effects: sends an email from the connected account.",
    },
    OperationSpec {
        id: "gmail.compose_draft",
        service: "gmail",
        action: "compose_draft",
        scopes_required: &["mail.drafts"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a draft email in the user's Drafts folder. This does NOT send. When the user asks to send, use gmail.send instead.\n\nWhen to use: only when the user explicitly wants to prepare an email without sending it.\nInputs: to (email), subject (string), body (plain text), optional cc, bcc.\nApproval: required.\nReturns: {\"sent\": false, \"draft_id\", \"thread_id\"}.\nSide effects: creates a draft visible in the user's Gmail Drafts folder.",
    },
    OperationSpec {
        id: "gcal.list_events",
        service: "gcal",
        action: "list_events",
        scopes_required: &["calendar.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List calendar events in a time window.\n\nWhen to use: to find what is on the user's calendar.\nInputs: calendar_id (string, default \"primary\"), time_min, time_max (RFC 3339), max_results, q (text filter).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gcal.get_event",
        service: "gcal",
        action: "get_event",
        scopes_required: &["calendar.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Retrieve a single calendar event by id.\n\nWhen to use: to inspect a specific event.\nInputs: calendar_id (string), event_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gcal.create_event",
        service: "gcal",
        action: "create_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a new calendar event.\n\nWhen to use: when the user wants to schedule something on their calendar.\nInputs: calendar_id (string), summary, start, end, optional description, location, attendees.\nApproval: required.\nSide effects: creates a calendar event that may notify attendees.",
    },
    OperationSpec {
        id: "gcal.update_event",
        service: "gcal",
        action: "update_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Update an existing calendar event (full replace).\n\nWhen to use: when the user wants to modify an existing event.\nInputs: calendar_id (string), event_id (string), full event draft.\nApproval: required.\nSide effects: updates the event and may re-notify attendees.",
    },
    OperationSpec {
        id: "gcal.delete_event",
        service: "gcal",
        action: "delete_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::ConfirmPhrase,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Delete a calendar event.\n\nWhen to use: when the user wants to cancel an event.\nInputs: calendar_id (string), event_id (string).\nApproval: required + confirmation phrase.\nSide effects: permanently removes the event from the calendar.",
    },
    OperationSpec {
        id: "gdrive.workspace_list",
        service: "gdrive",
        action: "workspace_list",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List files in the agent's Drive workspace folder.\n\nWhen to use: to see what files the agent has created or has access to.\nInputs: agent_slug (string).\nApproval: not required.\nSide effects: none. Scope: only files under Drive/Apollia/<agent_slug>/.",
    },
    OperationSpec {
        id: "gdrive.workspace_read",
        service: "gdrive",
        action: "workspace_read",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Read a workspace file by id.\n\nWhen to use: to fetch the content of a previously-listed file.\nInputs: file_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gdrive.workspace_write",
        service: "gdrive",
        action: "workspace_write",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create or replace a file in the agent's Drive workspace.\n\nWhen to use: to persist text content to the user's Drive scoped to this agent.\nInputs: agent_slug (string), name (string), content (string).\nApproval: required.\nSide effects: writes a file under Drive/Apollia/<agent_slug>/.",
    },
    OperationSpec {
        id: "gdrive.workspace_delete",
        service: "gdrive",
        action: "workspace_delete",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Move a workspace file to Trash.\n\nWhen to use: when the user wants to remove a file the agent created.\nInputs: file_id (string).\nApproval: required.\nSide effects: file is moved to Drive Trash (recoverable for 30 days).",
    },
    OperationSpec {
        id: "gdrive.workspace_share",
        service: "gdrive",
        action: "workspace_share",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Share a workspace file with a specific email address (reader role).\n\nWhen to use: when the user explicitly asks to share a file with someone.\nInputs: file_id (string), email (string).\nApproval: required.\nSide effects: grants reader access to the specified email.",
    },
    // ─── Generic visibility list (drive.file scope: own + granted) ────
    OperationSpec {
        id: "gdrive.list_my_files",
        service: "gdrive",
        action: "list_my_files",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List the contents of the Apollia workspace inside the user's Drive - the workspace anchor is set in Settings → Intégrations → \"Dossier racine\".\n\n**Default behaviour (no args)**: lists files in the configured workspace folder. If the folder doesn't exist yet on Drive, returns `[]` with a hint. When the user set the root to \"\" (literal Drive root), lists everything at My Drive root.\n\n**Overrides**: `folder_id` lists a specific Drive folder by id; `all: true` ignores the workspace and returns every file Apollia has access to (own + Picker grants). Use `all: true` when the user explicitly asks to see everything across Drive.\n\nUnder `drive.file` OAuth scope, Apollia never sees files it didn't create or wasn't granted via Picker.\n\nInputs: folder_id (optional string), all (optional bool, default false), page_size (optional integer, default 50, max 100).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gdrive.find_by_name",
        service: "gdrive",
        action: "find_by_name",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Resolve a Drive file, folder, document, spreadsheet, or presentation by its **title** (exact or fuzzy substring match). Returns matching items with their ids.\n\n**ALWAYS call this BEFORE asking the user for an ID.** When the user references a Google asset by name (\"the Apollia Test sheet\", \"my Demo presentation\"), use this tool to look up the id, then call the operation they actually want. Do NOT ask the user for the alphanumeric id - that's a bad user experience.\n\nInputs: name (string, required), mime_type_filter (optional string: 'spreadsheet', 'document', 'presentation', 'folder', or a raw Drive mime type), exact (optional bool, default false → case-insensitive `contains`).\nApproval: not required.\nSide effects: none.",
    },
    // ─── Picker-aware ops (post Google Drive Picker integration) ───────
    //
    // These operate on folders the user designated via Google Picker in
    // the desktop UI. Apollia accumulates `drive.file` access to each
    // picked folder (and its descendants) without needing a CASA audit.
    OperationSpec {
        id: "gdrive.list_picked_folders",
        service: "gdrive",
        action: "list_picked_folders",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List the Drive folders the user picked via the desktop Picker, with their ids and names.\n\nWhen to use: to see which folders the agent has access to before listing or writing files.\nInputs: none.\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gdrive.list_files_in",
        service: "gdrive",
        action: "list_files_in",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List files inside a Drive folder the user previously picked.\n\nWhen to use: to enumerate files in a folder identified by its Drive id (obtained from `gdrive.list_picked_folders`).\nInputs: folder_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gdrive.read_file",
        service: "gdrive",
        action: "read_file",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Read a Drive file by id. The file must belong to a folder previously picked (drive.file scope rule).\n\nInputs: file_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gdrive.write_to_folder",
        service: "gdrive",
        action: "write_to_folder",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create or replace a text file in Drive.\n\n**Default behaviour (no folder_id, no at_drive_root)**: writes inside the Apollia workspace folder configured in Settings → Intégrations. Missing path segments are created on the fly.\n\n**Overrides**: pass `folder_id` to write inside a specific Drive folder; pass `at_drive_root: true` to bypass the workspace and write directly at My Drive root.\n\nInputs: name (string), content (string), folder_id (optional string), at_drive_root (optional bool, default false), mime_type (optional string, default text/plain).\nApproval: required.\nSide effects: writes a new file (and possibly creates folders to materialise the workspace path).",
    },
    // ─── Sheets ────────────────────────────────────────────────────────
    OperationSpec {
        id: "gsheets.create",
        service: "gsheets",
        action: "create",
        scopes_required: &["sheets"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a new Google Sheet with the given title. The sheet is owned by the connected Google account.\n\nThe response includes `default_sheet_title` (the title of the first tab, e.g. `Sheet1` or `Feuille 1` depending on the user's locale) - use this when constructing an A1 `range` afterwards; do NOT use the spreadsheet title.\n\nInputs: title (string).\nApproval: required.\nSide effects: creates a new spreadsheet visible in the user's Drive.",
    },
    OperationSpec {
        id: "gsheets.list_sheets",
        service: "gsheets",
        action: "list_sheets",
        scopes_required: &["sheets"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List the tabs (sheets) inside a spreadsheet. Use this to discover the tab title before constructing an A1 range - the spreadsheet title (visible in the browser tab) is NOT the same as a sheet title.\n\n**Prerequisite**: if you only know the spreadsheet by name (e.g. \"Apollia Test\"), call `gdrive.find_by_name(name=..., mime_type_filter=\"spreadsheet\")` first to resolve the id. NEVER ask the user for the alphanumeric id.\n\nInputs: spreadsheet_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gsheets.read_values",
        service: "gsheets",
        action: "read_values",
        scopes_required: &["sheets"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Read values from a Google Sheet range in A1 notation.\n\n**Prerequisite**: if you only know the spreadsheet by name, call `gdrive.find_by_name(name=..., mime_type_filter=\"spreadsheet\")` first to resolve the id. NEVER ask the user for the alphanumeric id.\n\n**Range format**: `<tab_title>!A1:C10`. `tab_title` is the name of an individual tab (call `gsheets.list_sheets` if you don't know it), NOT the spreadsheet title. Tab titles containing spaces must be single-quoted: `'Feuille 1'!A1:C10`.\n\nInputs: spreadsheet_id (string), range (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gsheets.append_values",
        service: "gsheets",
        action: "append_values",
        scopes_required: &["sheets"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Append rows at the bottom of a Google Sheet range.\n\n**Prerequisite**: if you only know the spreadsheet by name, call `gdrive.find_by_name(name=..., mime_type_filter=\"spreadsheet\")` first to resolve the id. NEVER ask the user for the alphanumeric id.\n\n**Range format**: same rule as `gsheets.read_values` - use the tab title, single-quote if it contains spaces. Call `gsheets.list_sheets` if you only know the spreadsheet id.\n\nInputs: spreadsheet_id (string), range (string), values (array of arrays).\nApproval: required.\nSide effects: appends rows; sheet auto-extends.",
    },
    OperationSpec {
        id: "gsheets.update_values",
        service: "gsheets",
        action: "update_values",
        scopes_required: &["sheets"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Overwrite values inside a Google Sheet range.\n\n**Prerequisite**: if you only know the spreadsheet by name, call `gdrive.find_by_name(name=..., mime_type_filter=\"spreadsheet\")` first to resolve the id. NEVER ask the user for the alphanumeric id.\n\n**Range format**: same rule as `gsheets.read_values` - `<tab_title>!A1:C1`, single-quote tab titles containing spaces (e.g. `'Feuille 1'!A1:C1`). Do NOT use the spreadsheet title here. Call `gsheets.list_sheets` if unsure.\n\nInputs: spreadsheet_id (string), range (string), values (array of arrays).\nApproval: required.\nSide effects: overwrites cells in the range.",
    },
    // ─── Docs ──────────────────────────────────────────────────────────
    OperationSpec {
        id: "gdocs.create",
        service: "gdocs",
        action: "create",
        scopes_required: &["docs"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a new empty Google Doc with the given title.\n\nInputs: title (string).\nApproval: required.\nSide effects: creates a new document in the user's Drive.",
    },
    OperationSpec {
        id: "gdocs.read_text",
        service: "gdocs",
        action: "read_text",
        scopes_required: &["docs"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Read the plain-text content of a Google Doc.\n\n**Prerequisite**: if you only know the document by name, call `gdrive.find_by_name(name=..., mime_type_filter=\"document\")` first to resolve the id. NEVER ask the user for the alphanumeric id.\n\nInputs: document_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gdocs.append_text",
        service: "gdocs",
        action: "append_text",
        scopes_required: &["docs"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Append text to the end of a Google Doc body.\n\n**Prerequisite**: if you only know the document by name, call `gdrive.find_by_name(name=..., mime_type_filter=\"document\")` first to resolve the id. NEVER ask the user for the alphanumeric id.\n\nInputs: document_id (string), text (string).\nApproval: required.\nSide effects: modifies the document.",
    },
    // ─── Slides ────────────────────────────────────────────────────────
    OperationSpec {
        id: "gslides.create",
        service: "gslides",
        action: "create",
        scopes_required: &["slides"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a new Google Slides presentation with the given title.\n\nInputs: title (string).\nApproval: required.\nSide effects: creates a new presentation in the user's Drive.",
    },
    OperationSpec {
        id: "gslides.append_slide",
        service: "gslides",
        action: "append_slide",
        scopes_required: &["slides"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Append a slide with a single text body to an existing Slides deck.\n\n**Prerequisite**: if you only know the presentation by name, call `gdrive.find_by_name(name=..., mime_type_filter=\"presentation\")` first to resolve the id. NEVER ask the user for the alphanumeric id.\n\nInputs: presentation_id (string), text (string).\nApproval: required.\nSide effects: adds a slide to the deck.",
    },
    // ─── Tasks ─────────────────────────────────────────────────────────
    OperationSpec {
        id: "gtasks.list_lists",
        service: "gtasks",
        action: "list_lists",
        scopes_required: &["tasks"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List the user's Google Tasks lists (default `@default` always exists).\n\nInputs: none.\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gtasks.list_tasks",
        service: "gtasks",
        action: "list_tasks",
        scopes_required: &["tasks"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List tasks inside a Google Tasks list (includes completed).\n\nInputs: task_list_id (string, e.g. `@default`).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gtasks.create",
        service: "gtasks",
        action: "create",
        scopes_required: &["tasks"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a new Google Task in a list.\n\nInputs: task_list_id (string), title (string), notes (optional string), due (optional RFC 3339).\nApproval: required.\nSide effects: adds a task to the list.",
    },
    OperationSpec {
        id: "gtasks.complete",
        service: "gtasks",
        action: "complete",
        scopes_required: &["tasks"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Mark a Google Task as completed.\n\nInputs: task_list_id (string), task_id (string).\nApproval: required.\nSide effects: changes the task status.",
    },
    OperationSpec {
        id: "gtasks.delete",
        service: "gtasks",
        action: "delete",
        scopes_required: &["tasks"],
        approval: ApprovalPolicy::ConfirmPhrase,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Delete a Google Task permanently.\n\nInputs: task_list_id (string), task_id (string).\nApproval: required + confirmation phrase.\nSide effects: removes the task irreversibly.",
    },
    // ─── Forms ─────────────────────────────────────────────────────────
    OperationSpec {
        id: "gforms.create",
        service: "gforms",
        action: "create",
        scopes_required: &["forms"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create an empty Google Form with the given title.\n\nInputs: title (string).\nApproval: required.\nSide effects: creates a new form in the user's Drive. Returns the responder URL the user can share.",
    },
    // ─── YouTube (read-only) ───────────────────────────────────────────
    OperationSpec {
        id: "youtube.search",
        service: "youtube",
        action: "search",
        scopes_required: &["youtube.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Free-text search across YouTube videos.\n\nInputs: query (string), max_results (optional integer 1..=50, default 10).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "youtube.video_details",
        service: "youtube",
        action: "video_details",
        scopes_required: &["youtube.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Fetch full metadata for a YouTube video (snippet + statistics + content details).\n\nInputs: video_id (string).\nApproval: not required.\nSide effects: none.",
    },
];

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_id_matches_provider_id() {
        assert_eq!(MANIFEST.id, GOOGLE_PROVIDER_ID);
    }

    #[test]
    fn test_manifest_lists_three_services() {
        assert_eq!(MANIFEST.services, &["gmail", "gcal", "gdrive"]);
    }

    #[test]
    fn test_operations_cover_expected_services() {
        let services: std::collections::HashSet<&'static str> =
            OPERATIONS.iter().map(|op| op.service).collect();
        assert!(services.contains("gmail"));
        assert!(services.contains("gcal"));
        assert!(services.contains("gdrive"));
    }

    #[test]
    fn test_write_operations_require_approval() {
        let writes = ["gmail.send", "gcal.create_event", "gdrive.workspace_write"];
        for id in writes {
            let op = OPERATIONS.iter().find(|o| o.id == id).expect(id);
            assert!(op.requires_approval(), "expected {id} to require approval");
        }
    }

    #[test]
    fn test_read_operations_auto_approve() {
        let reads = ["gcal.list_events", "gdrive.workspace_read"];
        for id in reads {
            let op = OPERATIONS.iter().find(|o| o.id == id).expect(id);
            assert!(op.is_read_only(), "expected {id} to auto-approve");
        }
    }

    #[test]
    fn test_delete_event_uses_confirm_phrase() {
        let op = OPERATIONS
            .iter()
            .find(|o| o.id == "gcal.delete_event")
            .expect("op");
        assert_eq!(op.approval, ApprovalPolicy::ConfirmPhrase);
    }

    #[test]
    fn test_no_restricted_scopes_in_v0_1_0_catalog() {
        // GIVEN the Google operations catalog
        // WHEN we collect every scope required
        // THEN none of the restricted scopes appear (free-tier policy).
        let restricted = [
            "mail.read_inbox",
            "mail.full",
            "drive.read_all",
            "drive.full",
        ];
        for op in OPERATIONS {
            for scope in op.scopes_required {
                assert!(
                    !restricted.contains(scope),
                    "operation {} requires restricted scope {} - must be deferred to Expert Mode",
                    op.id,
                    scope
                );
            }
        }
    }
}
