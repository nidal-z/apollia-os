//! The inter-agent mailbox as the observability view reads it: the rows of
//! `mailbox.db`, newest first, read by the runtime and served by
//! `GET /api/v1/mailbox/messages`.

use tauri::State;

pub use apollia_runtime::observability::mailbox_rows::MailboxMessageRow;

use crate::attached::RuntimeLink;

/// Lists the most recent inter-agent mailbox messages across every recipient,
/// newest first. An empty store yields an empty list.
#[tauri::command]
pub async fn list_mailbox_messages(
    state: State<'_, RuntimeLink>,
    limit: Option<u32>,
) -> Result<Vec<MailboxMessageRow>, String> {
    mailbox_on(&state, limit.unwrap_or(100)).await
}

/// The mailbox read on the runtime `link` drives.
pub(super) async fn mailbox_on(
    link: &RuntimeLink,
    limit: u32,
) -> Result<Vec<MailboxMessageRow>, String> {
    let json = link
        .get_json(&format!("/api/v1/mailbox/messages?limit={limit}"))
        .await?;
    serde_json::from_value(json).map_err(|e| format!("unreadable mailbox: {e}"))
}
