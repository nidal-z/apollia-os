//! Images attached to a user message of the chat.
//!
//! A client sends each image as its media type and its bytes in base64. The
//! runtime checks them (count, size, format), refuses them when the model of
//! the conversation does not read images, writes them as files under the data
//! directory, one folder per conversation, and references them in the
//! `metadata` column of the user message. Replaying the history reads them
//! back from there.
//!
//! No byte of an image ever reaches a trace: events carry the count, the
//! media types and the sizes only.

use std::path::{Path, PathBuf};

use apollia_llm::message_image::{describe_images, MessageImage};
use apollia_llm::Capabilities;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::types::ChatError;

/// Most images one message may carry.
pub const MAX_IMAGES_PER_MESSAGE: usize = 8;

/// Largest image accepted, in bytes (10 MiB).
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Folder of the data directory holding the images of the conversations.
pub const IMAGES_DIR_NAME: &str = "chat_images";

/// Key of the message metadata listing its images.
pub const METADATA_KEY: &str = "images";

/// Largest body of a request sending a message with images: every image at
/// its ceiling once encoded in base64, plus room for the text and the JSON
/// around it.
#[must_use]
pub const fn max_message_body_bytes() -> usize {
    const TEXT_ROOM: usize = 2 * 1024 * 1024;
    MAX_IMAGES_PER_MESSAGE * MAX_IMAGE_BYTES.div_ceil(3) * 4 + TEXT_ROOM
}

/// One image as a client sends it.
#[derive(Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ImageAttachment {
    /// Media type of the image, such as `image/png`.
    pub mime_type: String,
    /// Bytes of the image, in standard base64.
    pub data: String,
}

impl std::fmt::Debug for ImageAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageAttachment")
            .field("mime_type", &self.mime_type)
            .field("data_len", &self.data.len())
            .finish()
    }
}

/// An image as the metadata of its message references it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StoredImage {
    /// File name inside the folder of the conversation.
    pub file: String,
    /// Media type of the image.
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
}

/// The format name a capability lists for a media type (`image/png` reads
/// `png`), `None` for a type that is not an image format the engines know.
#[must_use]
pub fn format_of(mime: &str) -> Option<&'static str> {
    match mime.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" | "image/pjpeg" => Some("jpeg"),
        "image/gif" => Some("gif"),
        "image/bmp" | "image/x-bmp" | "image/x-ms-bmp" => Some("bmp"),
        "image/webp" => Some("webp"),
        "image/tiff" => Some("tiff"),
        "image/x-tga" | "image/tga" | "image/x-targa" => Some("tga"),
        _ => None,
    }
}

fn extension_of(format: &str) -> &str {
    match format {
        "jpeg" => "jpg",
        other => other,
    }
}

/// Decode the images a client sent, refusing too many, an unknown media
/// type, undecodable base64, an empty image or one above [`MAX_IMAGE_BYTES`].
///
/// # Errors
/// [`ChatError::ImageRejected`] naming the first fault.
pub fn decode_attachments(attachments: &[ImageAttachment]) -> Result<Vec<MessageImage>, ChatError> {
    if attachments.len() > MAX_IMAGES_PER_MESSAGE {
        return Err(ChatError::ImageRejected(format!(
            "{} images, at most {MAX_IMAGES_PER_MESSAGE} per message",
            attachments.len()
        )));
    }
    attachments
        .iter()
        .enumerate()
        .map(|(index, attachment)| {
            let position = index + 1;
            let format = format_of(&attachment.mime_type).ok_or_else(|| {
                ChatError::ImageRejected(format!(
                    "image {position}: unknown image type {}",
                    attachment.mime_type
                ))
            })?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(attachment.data.as_bytes())
                .map_err(|_| {
                    ChatError::ImageRejected(format!("image {position}: invalid base64 data"))
                })?;
            if bytes.is_empty() {
                return Err(ChatError::ImageRejected(format!("image {position}: empty")));
            }
            if bytes.len() > MAX_IMAGE_BYTES {
                return Err(ChatError::ImageRejected(format!(
                    "image {position}: {} bytes, at most {MAX_IMAGE_BYTES}",
                    bytes.len()
                )));
            }
            Ok(MessageImage::new(format!("image/{format}"), bytes))
        })
        .collect()
}

/// Refuse images the model cannot read: no capability known, no vision, or a
/// format its engine does not decode.
///
/// # Errors
/// [`ChatError::ImagesNotSupported`] when the model reads no image,
/// [`ChatError::ImageRejected`] when it does not read one of their formats.
pub fn check_model_reads(
    model: Option<&str>,
    capabilities: Option<&Capabilities>,
    images: &[MessageImage],
) -> Result<(), ChatError> {
    if images.is_empty() {
        return Ok(());
    }
    let Some(caps) = capabilities.filter(|caps| caps.vision) else {
        return Err(ChatError::ImagesNotSupported {
            model: model.map(str::to_owned),
        });
    };
    // A model whose formats are not listed reads images in some format; the
    // engine answers for the rest.
    if caps.image_formats.is_empty() {
        return Ok(());
    }
    for image in images {
        let format = format_of(image.mime()).unwrap_or("unknown");
        if !caps.image_formats.iter().any(|f| f == format) {
            return Err(ChatError::ImageRejected(format!(
                "the model of this conversation does not read {format} images"
            )));
        }
    }
    Ok(())
}

/// What the model `model` names can do, as the data directory `data_dir`
/// records it (its backend table and its model index); `None` when the
/// backend table cannot be read.
#[must_use]
pub fn model_capabilities_in(
    data_dir: &Path,
    model: Option<&str>,
) -> Option<crate::local_engine::ModelCapabilities> {
    let repo = apollia_core::LlmBackendRepository::open(
        &apollia_core::paths::DataFile::System.path(data_dir),
    )
    .ok()?;
    let models_dir = crate::installed_models::models_dir(data_dir);
    crate::local_engine::model_capabilities(&repo, &models_dir, model).ok()
}

/// A session identifier as a folder name: anything but ASCII letters,
/// digits, `-` and `_` is replaced, so no identifier names a path outside
/// the images folder.
#[must_use]
pub fn session_folder(session_id: &str) -> String {
    let name: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        "_".to_owned()
    } else {
        name
    }
}

/// The folder of the images of `session_id` under `data_dir`.
#[must_use]
pub fn session_images_dir(data_dir: &Path, session_id: &str) -> PathBuf {
    data_dir
        .join(IMAGES_DIR_NAME)
        .join(session_folder(session_id))
}

/// Write the images of message `message_id` into `dir`, one file each.
///
/// # Errors
/// [`ChatError::InternalError`] when the folder or a file cannot be written.
pub fn store_images(
    dir: &Path,
    message_id: &str,
    images: &[MessageImage],
) -> Result<Vec<StoredImage>, ChatError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| ChatError::InternalError(format!("images folder: {e}")))?;
    let stem = session_folder(message_id);
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let format = format_of(image.mime()).unwrap_or("bin");
            let file = format!("{stem}-{}.{}", index + 1, extension_of(format));
            std::fs::write(dir.join(&file), image.bytes())
                .map_err(|e| ChatError::InternalError(format!("image file: {e}")))?;
            Ok(StoredImage {
                file,
                mime_type: image.mime().to_owned(),
                size: image.len() as u64,
            })
        })
        .collect()
}

/// The metadata of a user message referencing `stored`.
#[must_use]
pub fn images_metadata(stored: &[StoredImage]) -> serde_json::Value {
    serde_json::json!({ METADATA_KEY: stored })
}

/// The images a message metadata references, empty when it names none.
#[must_use]
pub fn stored_images(metadata: Option<&serde_json::Value>) -> Vec<StoredImage> {
    metadata
        .and_then(|m| m.get(METADATA_KEY))
        .and_then(|v| serde_json::from_value::<Vec<StoredImage>>(v.clone()).ok())
        .unwrap_or_default()
}

/// Read back the images `stored` from `dir`. A missing or unreadable file,
/// or a name that leaves the folder, is skipped and traced.
#[must_use]
pub fn load_images(dir: &Path, stored: &[StoredImage]) -> Vec<MessageImage> {
    stored
        .iter()
        .filter_map(|image| {
            if !plain_file_name(&image.file) {
                tracing::event!(tracing::Level::WARN, "chat.images.name_refused");
                return None;
            }
            match std::fs::read(dir.join(&image.file)) {
                Ok(bytes) => Some(MessageImage::new(image.mime_type.clone(), bytes)),
                Err(e) => {
                    tracing::event!(
                        tracing::Level::WARN,
                        error = %e,
                        "chat.images.missing"
                    );
                    None
                }
            }
        })
        .collect()
}

/// Whether `file` names a file directly inside a folder: no separator, no
/// hidden or parent entry.
fn plain_file_name(file: &str) -> bool {
    !file.is_empty() && !file.contains(['/', '\\']) && !file.starts_with('.')
}

/// The media type of a stored image, read from its extension.
fn mime_of_file(file: &str) -> Option<&'static str> {
    let extension = file.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "tiff" => "image/tiff",
        "tga" => "image/x-tga",
        _ => return None,
    })
}

/// One stored image of a conversation, as a client shows it.
///
/// # Errors
/// [`ChatError::ImageRejected`] for a name that is not a plain image file
/// name, [`ChatError::InternalError`] when the file cannot be read.
pub fn read_session_image(
    data_dir: &Path,
    session_id: &str,
    file: &str,
) -> Result<ImageAttachment, ChatError> {
    let mime = mime_of_file(file)
        .filter(|_| plain_file_name(file))
        .ok_or_else(|| ChatError::ImageRejected(format!("not an image name: {file}")))?;
    let bytes = std::fs::read(session_images_dir(data_dir, session_id).join(file))
        .map_err(|e| ChatError::InternalError(format!("image file: {e}")))?;
    Ok(ImageAttachment {
        mime_type: mime.to_owned(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

/// Remove the folder of the images of a conversation, if any.
pub fn remove_session_images(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "chat.images.remove_failed"
        ),
    }
}

/// The images of one turn of the chat: those of the message being answered,
/// and those of the earlier user messages still in the window, by message id.
#[derive(Debug, Clone, Default)]
pub struct TurnImages {
    /// Images of the message the turn answers.
    pub current: Vec<MessageImage>,
    /// Images of earlier user messages, keyed by message id.
    pub history: std::collections::HashMap<String, Vec<MessageImage>>,
}

/// Trace the images a message carries: count, media types and total size.
pub fn trace_images(session_id: &str, images: &[MessageImage]) {
    let (types, bytes) = describe_images(images);
    tracing::event!(
        tracing::Level::INFO,
        session_id = %session_id,
        count = images.len(),
        types = %types,
        bytes = bytes,
        "chat.images.received"
    );
}

#[cfg(test)]
mod tests;
