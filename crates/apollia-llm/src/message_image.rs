//! An image attached to a user message.
//!
//! The bytes are shared behind an `Arc` because a chat history is cloned at
//! every turn and an image weighs megabytes. `Debug` prints the media type and
//! the size only: a message is formatted into errors and traces, and the bytes
//! of a user's picture must never reach a log.

use std::fmt;
use std::sync::Arc;

/// One image of a multimodal user message: its media type and its bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct MessageImage {
    mime: String,
    bytes: Arc<[u8]>,
}

impl MessageImage {
    /// Build an image from its media type (`image/png`, ...) and its bytes.
    pub fn new(mime: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            mime: mime.into(),
            bytes: bytes.into(),
        }
    }

    /// The media type, such as `image/png`.
    #[must_use]
    pub fn mime(&self) -> &str {
        &self.mime
    }

    /// The raw bytes of the image.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Size of the image in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the image carries no byte.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl fmt::Debug for MessageImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageImage")
            .field("mime", &self.mime)
            .field("len", &self.bytes.len())
            .finish()
    }
}

impl crate::types::ChatMessage {
    /// Build a user message carrying images after its text. With no image,
    /// this is a plain text message.
    pub fn user_with_images(text: impl Into<String>, images: Vec<MessageImage>) -> Self {
        let text = text.into();
        let content = if images.is_empty() {
            crate::types::MessageContent::Text(text)
        } else {
            crate::types::MessageContent::TextWithImages { text, images }
        };
        Self {
            role: crate::types::Role::User,
            content,
            cache_control: None,
            reasoning: None,
        }
    }
}

/// Media types and total size of a set of images, the only facts about an
/// image a trace may carry.
#[must_use]
pub fn describe_images(images: &[MessageImage]) -> (String, usize) {
    let types = images
        .iter()
        .map(MessageImage::mime)
        .collect::<Vec<_>>()
        .join(",");
    let bytes = images.iter().map(MessageImage::len).sum();
    (types, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_bytes() {
        // GIVEN an image whose bytes spell a recognisable marker
        let image = MessageImage::new("image/png", b"SECRET-PIXELS".to_vec());

        // WHEN it is formatted with Debug
        let printed = format!("{image:?}");

        // THEN the media type and size appear, the bytes do not
        assert!(printed.contains("image/png"));
        assert!(printed.contains("13"));
        assert!(!printed.contains("SECRET"));
        assert!(!printed.contains("83, 69"));
    }

    #[test]
    fn describe_images_lists_types_and_sums_sizes() {
        // GIVEN two images
        let images = vec![
            MessageImage::new("image/png", vec![0u8; 3]),
            MessageImage::new("image/jpeg", vec![0u8; 5]),
        ];

        // WHEN they are described
        let (types, bytes) = describe_images(&images);

        // THEN the types are listed in order and the sizes summed
        assert_eq!(types, "image/png,image/jpeg");
        assert_eq!(bytes, 8);
    }
}
