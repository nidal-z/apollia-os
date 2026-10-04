//! The daemon routes behind the commands that manage the conversations of the
//! chat, one function per command: what the attached arm of each command sends.
//! A refusal of the daemon comes back as its own message.

use apollia_runtime::api::routes_chat::conversations::GeneratedTitle;
use serde_json::{json, Value};

use super::UpdateSessionRequest;
use crate::attached::{segment, AttachedClient};

/// `PATCH /api/v1/sessions/{id}`, for `update_chat_session`.
pub(crate) async fn update(
    remote: &AttachedClient,
    session_id: &str,
    update: &UpdateSessionRequest,
) -> Result<(), String> {
    let path = format!("/api/v1/sessions/{}", segment(session_id));
    let _: Value = remote.patch(&path, update).await?;
    Ok(())
}

/// `POST /api/v1/sessions/{id}/erase`, for `delete_chat_session`.
pub(crate) async fn erase(remote: &AttachedClient, session_id: &str) -> Result<(), String> {
    let path = format!("/api/v1/sessions/{}/erase", segment(session_id));
    let _: Value = remote.post(&path, &json!({})).await?;
    Ok(())
}

/// `PUT /api/v1/sessions/{id}/title`, for `rename_chat_session`.
pub(crate) async fn rename(
    remote: &AttachedClient,
    session_id: &str,
    title: &str,
) -> Result<(), String> {
    let path = format!("/api/v1/sessions/{}/title", segment(session_id));
    let _: Value = remote.put(&path, &json!({ "title": title })).await?;
    Ok(())
}

/// `POST /api/v1/sessions/{id}/title/generate`, for
/// `generate_chat_session_name`.
pub(crate) async fn name(
    remote: &AttachedClient,
    session_id: &str,
    first_message: &str,
) -> Result<String, String> {
    let path = format!("/api/v1/sessions/{}/title/generate", segment(session_id));
    let body = json!({ "first_message": first_message });
    let named: GeneratedTitle = remote.post(&path, &body).await?;
    Ok(named.title)
}
