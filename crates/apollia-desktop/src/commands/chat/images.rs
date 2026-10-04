//! The images of the chat: what the model of a conversation reads, and the
//! stored images a message shows as thumbnails.

use apollia_runtime::api::routes_chat::session_reads::SessionModelCapabilities;
use apollia_runtime::chat::images::ImageAttachment;
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// What the model of `session_id` can do: whether it reads images and in
/// which formats, which the composer reads before it accepts an image.
/// Attached, the daemon answers the same record.
#[tauri::command]
pub async fn get_chat_model_capabilities(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<SessionModelCapabilities, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            let path = format!(
                "/api/v1/sessions/{}/model-capabilities",
                segment(&session_id)
            );
            return Ok(remote.get(&path).await?);
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    manager
        .session_model_capabilities(session_id)
        .await
        .map(Into::into)
        .map_err(|e| e.to_string())
}

/// One stored image of `session_id`, by the file name its message metadata
/// lists: its media type and its bytes in base64, for a thumbnail.
#[tauri::command]
pub async fn read_chat_image(
    link: State<'_, RuntimeLink>,
    session_id: String,
    file: String,
) -> Result<ImageAttachment, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            let path = format!(
                "/api/v1/sessions/{}/images/{}",
                segment(&session_id),
                segment(&file)
            );
            return Ok(remote.get(&path).await?);
        }
    };
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    manager
        .session_image(&session_id, &file)
        .map_err(|e| e.to_string())
}
