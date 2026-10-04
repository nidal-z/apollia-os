//! Documents attached to a user message of the chat.
//!
//! A client sends each document as its file name and its bytes in base64: a
//! PDF, a Word document, a PowerPoint presentation, a workbook, or a CSV file
//! that is not UTF-8. The runtime copies it under the data directory, one
//! folder per conversation, and extracts its text on reception with the
//! readers of the sources.
//!
//! Up to [`INLINE_MAX_CHARS`] characters, the text travels inside the message,
//! after the file name. Past that, the message carries the name, the size and
//! the path of the copy, and the agent reads it with `file_read`, a page or a
//! sheet at a time: the folder of the conversation is a reading root of that
//! conversation, and of no other.
//!
//! The text is the user's own gesture, in the user's own message. No byte and
//! no word of a document reaches a trace: events carry counts and sizes only.

use std::path::{Path, PathBuf};

use apollia_sources::{DocumentText, PartUnit, ReadError};
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::types::ChatError;

/// Most documents one message may carry.
pub const MAX_DOCUMENTS_PER_MESSAGE: usize = 4;

/// Largest document accepted, in bytes (20 MiB).
pub const MAX_DOCUMENT_BYTES: usize = 20 * 1024 * 1024;

/// Characters of text up to which a document travels inside the message.
pub const INLINE_MAX_CHARS: usize = 100_000;

/// Folder of the data directory holding the documents of the conversations.
pub const DOCUMENTS_DIR_NAME: &str = "chat_documents";

/// Key of the message metadata listing its documents.
pub const METADATA_KEY: &str = "documents";

/// Key of the message metadata giving where the text the user typed ends and
/// the documents begin, in UTF-16 code units of the content.
pub const TEXT_END_KEY: &str = "documents_start";

/// Largest body of a request sending a message: the images at their ceiling,
/// plus every document at its ceiling once encoded in base64.
#[must_use]
pub const fn max_message_body_bytes() -> usize {
    super::images::max_message_body_bytes()
        + MAX_DOCUMENTS_PER_MESSAGE * MAX_DOCUMENT_BYTES.div_ceil(3) * 4
}

/// One document as a client sends it.
#[derive(Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct DocumentAttachment {
    /// File name of the document, with its extension, such as `invoice.pdf`.
    pub name: String,
    /// Bytes of the document, in standard base64.
    pub data: String,
}

impl std::fmt::Debug for DocumentAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentAttachment")
            .field("name", &self.name)
            .field("data_len", &self.data.len())
            .finish()
    }
}

/// One document decoded and checked for count, size and format.
#[derive(Clone)]
pub struct MessageDocument {
    name: String,
    bytes: Vec<u8>,
}

impl MessageDocument {
    /// File name of the document, safe to write inside a folder.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Size in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the document holds no byte; a decoded one never does.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl std::fmt::Debug for MessageDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageDocument")
            .field("name", &self.name)
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// A document as the metadata of its message references it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StoredDocument {
    /// File name inside the folder of the conversation.
    pub file: String,
    /// File name as the user sent it.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// What its parts are: `page`, `slide`, `sheet` or `document`.
    pub unit: String,
    /// Number of pages, slides or sheets; 1 for a document without parts.
    pub parts: u32,
    /// Characters of text read from it.
    pub characters: u64,
    /// Whether the text travels inside the message; `false` when the agent
    /// reads the copy with `file_read`.
    pub inline: bool,
}

/// The documents of one message, once received: what its metadata lists, and
/// where the text the user typed ends in the content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDocuments {
    /// The documents, in the order they were sent.
    pub stored: Vec<StoredDocument>,
    /// Length of the text the user typed, in UTF-16 code units: the documents
    /// follow it in the content of the message.
    pub text_end: usize,
}

impl MessageDocuments {
    /// Add the documents to the `metadata` of their message.
    pub fn write_into(&self, metadata: &mut Option<serde_json::Value>) {
        let mut object = match metadata.take() {
            Some(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        };
        object.insert(METADATA_KEY.to_owned(), serde_json::json!(self.stored));
        object.insert(TEXT_END_KEY.to_owned(), serde_json::json!(self.text_end));
        *metadata = Some(serde_json::Value::Object(object));
    }
}

/// A file name safe to write inside a folder and inside an attribute: its
/// last path component, without a quote, an angle bracket, a control
/// character or a leading dot.
fn safe_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = last
        .chars()
        .map(|c| match c {
            '"' | '<' | '>' | ':' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    cleaned.trim().trim_start_matches('.').to_owned()
}

/// Decode the documents a client sent, refusing too many, a name without a
/// format a reader opens, undecodable base64, an empty document or one above
/// [`MAX_DOCUMENT_BYTES`].
///
/// # Errors
/// [`ChatError::DocumentRejected`] naming the first fault.
pub fn decode_attachments(
    attachments: &[DocumentAttachment],
) -> Result<Vec<MessageDocument>, ChatError> {
    if attachments.len() > MAX_DOCUMENTS_PER_MESSAGE {
        return Err(ChatError::DocumentRejected(format!(
            "{} documents, at most {MAX_DOCUMENTS_PER_MESSAGE} per message",
            attachments.len()
        )));
    }
    attachments
        .iter()
        .map(|attachment| {
            let name = safe_name(&attachment.name);
            if !apollia_sources::is_readable_document(Path::new(&name)) {
                return Err(ChatError::DocumentRejected(format!(
                    "{name}: not a format that is read (pdf, docx, pptx, xlsx, xlsm, xls, ods, csv)"
                )));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(attachment.data.as_bytes())
                .map_err(|_| ChatError::DocumentRejected(format!("{name}: invalid base64 data")))?;
            if bytes.is_empty() {
                return Err(ChatError::DocumentRejected(format!("{name}: empty")));
            }
            if bytes.len() > MAX_DOCUMENT_BYTES {
                return Err(ChatError::DocumentRejected(format!(
                    "{name}: {} bytes, at most {MAX_DOCUMENT_BYTES}",
                    bytes.len()
                )));
            }
            Ok(MessageDocument { name, bytes })
        })
        .collect()
}

/// The folder of the documents of `session_id` under `data_dir`.
#[must_use]
pub fn session_documents_dir(data_dir: &Path, session_id: &str) -> PathBuf {
    data_dir
        .join(DOCUMENTS_DIR_NAME)
        .join(super::images::session_folder(session_id))
}

/// Remove the folder of the documents of a conversation, if any.
pub fn remove_session_documents(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "chat.documents.remove_failed"
        ),
    }
}

/// Remove the copies `stored` from `dir`: the message that carried them was
/// not accepted.
pub fn discard(dir: &Path, stored: &[StoredDocument]) {
    for document in stored {
        let _ = std::fs::remove_file(dir.join(&document.file));
    }
}

/// Why a reader could not read a document, as the user reads it.
fn refusal(name: &str, error: &ReadError) -> ChatError {
    let cause = match error {
        ReadError::PasswordProtected { .. } => "it is protected by a password".to_owned(),
        ReadError::Corrupt { .. } => "it is damaged or is not the format its name says".to_owned(),
        ReadError::UnsupportedFormat { .. } => "it is not a format that is read".to_owned(),
        ReadError::ArchiveOverCap { cap, limit, .. } => {
            format!("it is over the cap of {limit} {cap}")
        }
        ReadError::Io { source, .. } => format!("it could not be read ({source})"),
        _ => "it could not be read".to_owned(),
    };
    ChatError::DocumentRejected(format!("{name}: {cause}"))
}

/// The attribute that counts the parts of a document: `pages="10"`.
fn parts_attribute(document: &DocumentText) -> String {
    match document.unit {
        PartUnit::Page => format!(" pages=\"{}\"", document.parts.len()),
        PartUnit::Slide => format!(" slides=\"{}\"", document.parts.len()),
        PartUnit::Sheet => format!(" sheets=\"{}\"", document.parts.len()),
        _ => String::new(),
    }
}

/// How `file_read` reads one part of a document of this unit.
fn reading_hint(unit: PartUnit) -> &'static str {
    match unit {
        PartUnit::Page => "one page at a time with its `page` argument",
        PartUnit::Slide => "one slide at a time with its `page` argument",
        PartUnit::Sheet => "one sheet at a time with its `sheet` argument",
        _ => "a range of lines at a time with its `offset` and `limit` arguments",
    }
}

/// The block one document adds to the message: its name, then its text; or
/// its name, its size and the path of its copy when the text is too long.
fn message_block(name: &str, size: usize, path: &Path, document: &DocumentText) -> (String, bool) {
    let characters = document.characters();
    let parts = parts_attribute(document);
    if characters == 0 {
        return (
            format!(
                "\n\n<attachment name=\"{name}\" size=\"{size}\"{parts}>\n\
                 No text could be read from this document: it holds images only, \
                 such as scanned pages.\n</attachment>"
            ),
            true,
        );
    }
    if characters <= INLINE_MAX_CHARS {
        return (
            format!(
                "\n\n<attachment name=\"{name}\" size=\"{size}\"{parts}>\n{}\n</attachment>",
                document.full_text()
            ),
            true,
        );
    }
    (
        format!(
            "\n\n<attachment name=\"{name}\" size=\"{size}\"{parts} characters=\"{characters}\" \
             path=\"{}\">\nThis document is too long to be included here. Read it with the \
             `file_read` tool at that path, {}.\n</attachment>",
            path.display(),
            reading_hint(document.unit)
        ),
        false,
    )
}

/// Copy the documents of a message into `dir`, extract their text, and build
/// what the message carries for them.
///
/// Returns what the metadata lists and the blocks to append to the text the
/// user typed. Synchronous: an async caller runs it under `spawn_blocking`.
///
/// # Errors
/// [`ChatError::InternalError`] when the folder or a copy cannot be written,
/// [`ChatError::DocumentRejected`] when a reader cannot read a document
/// (protected by a password, damaged, over a cap). Nothing is kept on error.
pub fn receive(
    dir: &Path,
    documents: &[MessageDocument],
) -> Result<(Vec<StoredDocument>, String), ChatError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| ChatError::InternalError(format!("documents folder: {e}")))?;
    let mut stored: Vec<StoredDocument> = Vec::with_capacity(documents.len());
    let mut blocks = String::new();
    for document in documents {
        let prefix = uuid::Uuid::new_v4().simple().to_string();
        let file = format!("{}-{}", &prefix[..8], document.name);
        let path = dir.join(&file);
        let read = std::fs::write(&path, &document.bytes)
            .map_err(|e| ChatError::InternalError(format!("document file: {e}")))
            .and_then(|()| {
                apollia_sources::read_document(&path).map_err(|e| refusal(&document.name, &e))
            });
        let text = match read {
            Ok(text) => text,
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                discard(dir, &stored);
                return Err(e);
            }
        };
        let (block, inline) = message_block(&document.name, document.len(), &path, &text);
        blocks.push_str(&block);
        stored.push(StoredDocument {
            file,
            name: document.name.clone(),
            size: document.len() as u64,
            unit: text.unit.as_str().to_owned(),
            parts: u32::try_from(text.parts.len()).unwrap_or(u32::MAX),
            characters: text.characters() as u64,
            inline,
        });
    }
    Ok((stored, blocks))
}

/// Trace the documents a message carries: count, total size, total
/// characters read, and how many travel inside the message.
pub fn trace_documents(session_id: &str, stored: &[StoredDocument]) {
    tracing::event!(
        tracing::Level::INFO,
        session_id = %session_id,
        count = stored.len(),
        bytes = stored.iter().map(|d| d.size).sum::<u64>(),
        characters = stored.iter().map(|d| d.characters).sum::<u64>(),
        inline = stored.iter().filter(|d| d.inline).count(),
        "chat.documents.received"
    );
}

#[cfg(test)]
mod tests;
