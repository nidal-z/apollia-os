//! One arm per connector operation, calling the real client method.
//!
//! The arms mirror the runtime bridge in `apollia-runtime`'s
//! `connectors_bridge`: the same operation id reaches the same client method.
//! What differs is everything the bridge does around that call, which this
//! crate cannot reach: resolving an account, fetching a token, marshalling the
//! agent's JSON arguments, and reshaping the returned value into the payload
//! the agent sees. See the module documentation in `mod.rs`.
//!
//! Arguments are placeholders. The mock server answers every path, so the
//! argument values change the request but never the answer, and the harness
//! measures the reading of the answer.

use serde::Serialize;
use serde_json::Value;

use crate::error::ConnectorError;
use crate::google::{
    calendar as gcal, docs::DocsClient, drive_workspace as gdrive, forms::FormsClient,
    gmail as gmail_mod, sheets as gsheets, slides::SlidesClient, tasks as gtasks,
    youtube::YouTubeClient, CalendarClient, DriveWorkspaceClient, GmailClient, SheetsClient,
    TasksClient,
};
use crate::http::HttpClient;
use crate::microsoft::{
    calendar as mscal, mail as msmail, onedrive::OneDriveClient, OutlookCalendarClient,
    OutlookMailClient,
};

/// Bearer handed to every replayed call. The mock server never checks it.
const TOKEN: &str = "replay-token";

/// The refresh closure every client method takes. A replayed 401 refreshes to
/// the same token, so a second 401 surfaces as `Unauthorized` without sleeping.
macro_rules! refresh {
    () => {
        || async { Ok::<_, ConnectorError>(TOKEN.to_owned()) }
    };
}

/// Serialise what the client read into the value a fixture asserts against.
fn ok<T: Serialize>(value: T) -> Result<Value, ConnectorError> {
    serde_json::to_value(value).map_err(|e| ConnectorError::Decoding(e.to_string()))
}

/// Bytes come back as text when they are valid UTF-8, so a fixture can assert
/// against a readable string rather than a list of byte values.
fn ok_bytes(bytes: Vec<u8>) -> Result<Value, ConnectorError> {
    match String::from_utf8(bytes) {
        Ok(text) => Ok(Value::String(text)),
        Err(e) => ok(e.into_bytes()),
    }
}

fn compose_mail() -> gmail_mod::ComposeMail {
    gmail_mod::ComposeMail {
        to: "alice@example.com".into(),
        cc: None,
        bcc: None,
        subject: "Replay".into(),
        body: "Replay body.".into(),
    }
}

fn gcal_draft() -> gcal::EventDraft {
    gcal::EventDraft {
        summary: "Replay".into(),
        description: None,
        location: None,
        start: gcal::EventTime::default(),
        end: gcal::EventTime::default(),
        attendees: Vec::new(),
    }
}

fn ms_event_draft() -> mscal::EventDraft {
    mscal::EventDraft {
        subject: "Replay".into(),
        body: None,
        start: mscal::EventDateTime {
            date_time: "2026-09-04T09:00:00".into(),
            time_zone: "UTC".into(),
        },
        end: mscal::EventDateTime {
            date_time: "2026-09-04T10:00:00".into(),
            time_zone: "UTC".into(),
        },
        location: None,
        attendees: Vec::new(),
        is_all_day: None,
    }
}

fn ms_compose() -> msmail::ComposeMessage {
    msmail::ComposeMessage {
        subject: "Replay".into(),
        body: msmail::MessageBody {
            content_type: msmail::BodyContentType::Text,
            content: "Replay body.".into(),
        },
        to_recipients: vec![msmail::Recipient {
            email_address: msmail::EmailAddress {
                name: None,
                address: "alice@example.com".into(),
            },
        }],
        cc_recipients: Vec::new(),
        bcc_recipients: Vec::new(),
    }
}

/// Operations that reach no HTTP endpoint, so an HTTP replay cannot cover them.
///
/// `gdrive.list_picked_folders` answers from `apollia_auth::drive_prefs`, the
/// local record of folders the user granted through the Drive picker. It never
/// calls Google.
pub(crate) const NO_UPSTREAM_CALL: &[&str] = &["gdrive.list_picked_folders"];

/// Every operation this dispatch table can replay, in registry order.
///
/// Crossed against the connectors' own operation catalogues by
/// `test_dispatch_table_matches_the_operation_catalogue`, so an operation added
/// to a connector without an arm here fails the suite.
pub(crate) const DISPATCHABLE: &[&str] = &[
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
    "gdrive.list_my_files",
    "gdrive.find_by_name",
    "gdrive.list_files_in",
    "gdrive.read_file",
    "gdrive.write_to_folder",
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

/// Run one operation against `base`, and return what the client read.
///
/// The returned value is the client method's own return value, serialised. A
/// method that returns nothing reads as `null`.
pub(crate) async fn dispatch(operation: &str, base: &str) -> Result<Value, ConnectorError> {
    let http = HttpClient::new("replay")?;
    match operation {
        // ─── Gmail ──────────────────────────────────────────────────────────
        "gmail.send" => ok(GmailClient::with_base_url(http, base)
            .send(&compose_mail(), TOKEN, refresh!())
            .await?),
        "gmail.compose_draft" => ok(GmailClient::with_base_url(http, base)
            .compose_draft(&compose_mail(), TOKEN, refresh!())
            .await?),

        // ─── Google Calendar ────────────────────────────────────────────────
        "gcal.list_events" => ok(CalendarClient::with_base_url(http, base)
            .list_events(
                "primary",
                &gcal::ListEventsFilter::default(),
                TOKEN,
                refresh!(),
            )
            .await?),
        "gcal.get_event" => ok(CalendarClient::with_base_url(http, base)
            .get_event("primary", "evt-1", TOKEN, refresh!())
            .await?),
        "gcal.create_event" => ok(CalendarClient::with_base_url(http, base)
            .create_event("primary", &gcal_draft(), TOKEN, refresh!())
            .await?),
        "gcal.update_event" => {
            let draft = gcal_draft();
            ok(CalendarClient::with_base_url(http, base)
                .update_event(
                    gcal::EventUpdate {
                        calendar_id: "primary",
                        event_id: "evt-1",
                        draft: &draft,
                    },
                    TOKEN,
                    refresh!(),
                )
                .await?)
        }
        "gcal.delete_event" => ok(CalendarClient::with_base_url(http, base)
            .delete_event("primary", "evt-1", TOKEN, refresh!())
            .await?),

        // ─── Google Drive ───────────────────────────────────────────────────
        "gdrive.workspace_list" => ok(DriveWorkspaceClient::with_base_url(http, base)
            .workspace_list("Documents/Apollia", "replay-agent", TOKEN, refresh!())
            .await?),
        "gdrive.workspace_read" | "gdrive.read_file" => ok_bytes(
            DriveWorkspaceClient::with_base_url(http, base)
                .workspace_read("file-1", TOKEN, refresh!())
                .await?,
        ),
        "gdrive.workspace_write" => ok(DriveWorkspaceClient::with_base_url(http, base)
            .workspace_write(
                gdrive::WorkspaceWrite {
                    root_path: "Documents/Apollia",
                    agent_slug: "replay-agent",
                    name: "note.txt",
                    content: b"replay",
                    mime_type: Some("text/plain"),
                },
                TOKEN,
                refresh!(),
            )
            .await?),
        "gdrive.workspace_delete" => ok(DriveWorkspaceClient::with_base_url(http, base)
            .workspace_delete("file-1", TOKEN, refresh!())
            .await?),
        "gdrive.workspace_share" => ok(DriveWorkspaceClient::with_base_url(http, base)
            .workspace_share("file-1", "alice@example.com", TOKEN, refresh!())
            .await?),
        "gdrive.list_my_files" => ok(DriveWorkspaceClient::with_base_url(http, base)
            .list_visible_files(None, 50, TOKEN, refresh!())
            .await?),
        "gdrive.find_by_name" => ok(DriveWorkspaceClient::with_base_url(http, base)
            .find_by_name(
                gdrive::NameSearch {
                    name: "note",
                    mime_type_filter: None,
                    exact: false,
                },
                TOKEN,
                refresh!(),
            )
            .await?),
        "gdrive.list_files_in" => ok(DriveWorkspaceClient::with_base_url(http, base)
            .list_files_in_folder("folder-1", TOKEN, refresh!())
            .await?),
        "gdrive.write_to_folder" => ok(DriveWorkspaceClient::with_base_url(http, base)
            .write_file_in_folder(
                gdrive::FolderWrite {
                    folder_id: Some("folder-1"),
                    name: "note.txt",
                    content: b"replay",
                    mime_type: Some("text/plain"),
                },
                TOKEN,
                refresh!(),
            )
            .await?),

        // ─── Google Sheets ──────────────────────────────────────────────────
        "gsheets.create" => ok(SheetsClient::with_base_url(http, base)
            .create("Replay", TOKEN, refresh!())
            .await?),
        "gsheets.list_sheets" => ok(SheetsClient::with_base_url(http, base)
            .list_sheets("sheet-1", TOKEN, refresh!())
            .await?),
        "gsheets.read_values" => ok(SheetsClient::with_base_url(http, base)
            .read_values("sheet-1", "A1:C3", TOKEN, refresh!())
            .await?),
        "gsheets.append_values" => {
            let values = vec![vec![Value::String("a".into())]];
            ok(SheetsClient::with_base_url(http, base)
                .append_values(
                    gsheets::ValueWrite {
                        spreadsheet_id: "sheet-1",
                        range: "A1",
                        values: &values,
                    },
                    TOKEN,
                    refresh!(),
                )
                .await?)
        }
        "gsheets.update_values" => {
            let values = vec![vec![Value::String("a".into())]];
            ok(SheetsClient::with_base_url(http, base)
                .update_values(
                    gsheets::ValueWrite {
                        spreadsheet_id: "sheet-1",
                        range: "A1",
                        values: &values,
                    },
                    TOKEN,
                    refresh!(),
                )
                .await?)
        }

        // ─── Google Docs, Slides, Forms ─────────────────────────────────────
        "gdocs.create" => ok(DocsClient::with_base_url(http, base)
            .create("Replay", TOKEN, refresh!())
            .await?),
        "gdocs.read_text" => ok(DocsClient::with_base_url(http, base)
            .read_plain_text("doc-1", TOKEN, refresh!())
            .await?),
        "gdocs.append_text" => ok(DocsClient::with_base_url(http, base)
            .append_text("doc-1", "Replay", TOKEN, refresh!())
            .await?),
        "gslides.create" => ok(SlidesClient::with_base_url(http, base)
            .create("Replay", TOKEN, refresh!())
            .await?),
        "gslides.append_slide" => ok(SlidesClient::with_base_url(http, base)
            .append_slide_with_text("pres-1", "Replay", TOKEN, refresh!())
            .await?),
        "gforms.create" => ok(FormsClient::with_base_url(http, base)
            .create("Replay", TOKEN, refresh!())
            .await?),

        // ─── Google Tasks ───────────────────────────────────────────────────
        "gtasks.list_lists" => ok(TasksClient::with_base_url(http, base)
            .list_lists(TOKEN, refresh!())
            .await?),
        "gtasks.list_tasks" => ok(TasksClient::with_base_url(http, base)
            .list_tasks("@default", TOKEN, refresh!())
            .await?),
        "gtasks.create" => ok(TasksClient::with_base_url(http, base)
            .create_task(
                gtasks::NewTask {
                    task_list_id: "@default",
                    title: "Replay",
                    notes: None,
                    due_rfc3339: None,
                },
                TOKEN,
                refresh!(),
            )
            .await?),
        "gtasks.complete" => ok(TasksClient::with_base_url(http, base)
            .complete_task("@default", "task-1", TOKEN, refresh!())
            .await?),
        "gtasks.delete" => ok(TasksClient::with_base_url(http, base)
            .delete_task("@default", "task-1", TOKEN, refresh!())
            .await?),

        // ─── YouTube ────────────────────────────────────────────────────────
        "youtube.search" => ok(YouTubeClient::with_base_url(http, base)
            .search_videos("replay", 5, TOKEN, refresh!())
            .await?),
        "youtube.video_details" => ok(YouTubeClient::with_base_url(http, base)
            .video_details("vid-1", TOKEN, refresh!())
            .await?),

        // ─── Outlook Mail ───────────────────────────────────────────────────
        "outlook.search" => ok(OutlookMailClient::with_base_url(http, base)
            .search("replay", 10, TOKEN, refresh!())
            .await?),
        "outlook.get" => ok(OutlookMailClient::with_base_url(http, base)
            .get("msg-1", TOKEN, refresh!())
            .await?),
        "outlook.send" => ok(OutlookMailClient::with_base_url(http, base)
            .send(ms_compose(), true, TOKEN, refresh!())
            .await?),
        "outlook.reply" => ok(OutlookMailClient::with_base_url(http, base)
            .reply("msg-1", "Replay", TOKEN, refresh!())
            .await?),
        "outlook.list_folders" => ok(OutlookMailClient::with_base_url(http, base)
            .list_folders(TOKEN, refresh!())
            .await?),
        "outlook.move" => ok(OutlookMailClient::with_base_url(http, base)
            .move_to("msg-1", "folder-1", TOKEN, refresh!())
            .await?),

        // ─── Outlook Calendar ───────────────────────────────────────────────
        "outlook_cal.list_events" => ok(OutlookCalendarClient::with_base_url(http, base)
            .list_events(&mscal::ListEventsFilter::default(), TOKEN, refresh!())
            .await?),
        "outlook_cal.get_event" => ok(OutlookCalendarClient::with_base_url(http, base)
            .get_event("evt-1", TOKEN, refresh!())
            .await?),
        "outlook_cal.create_event" => ok(OutlookCalendarClient::with_base_url(http, base)
            .create_event(&ms_event_draft(), TOKEN, refresh!())
            .await?),
        "outlook_cal.update_event" => ok(OutlookCalendarClient::with_base_url(http, base)
            .update_event("evt-1", &ms_event_draft(), TOKEN, refresh!())
            .await?),
        "outlook_cal.delete_event" => ok(OutlookCalendarClient::with_base_url(http, base)
            .delete_event("evt-1", TOKEN, refresh!())
            .await?),

        // ─── OneDrive ───────────────────────────────────────────────────────
        "onedrive.search" => ok(OneDriveClient::with_base_url(http, base)
            .search("replay", 10, TOKEN, refresh!())
            .await?),
        "onedrive.get_metadata" => ok(OneDriveClient::with_base_url(http, base)
            .get_metadata("item-1", TOKEN, refresh!())
            .await?),
        "onedrive.download" => ok_bytes(
            OneDriveClient::with_base_url(http, base)
                .download("item-1", TOKEN, refresh!())
                .await?,
        ),
        "onedrive.list_recent" => ok(OneDriveClient::with_base_url(http, base)
            .list_recent(10, TOKEN, refresh!())
            .await?),

        other => Err(ConnectorError::InvalidArgument(format!(
            "no replay arm for operation {other:?}"
        ))),
    }
}
