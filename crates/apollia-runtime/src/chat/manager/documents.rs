//! The documents of a user message, as the handle receives them and as the
//! file tools of their conversation reach them.

use super::*;
use crate::chat::documents;

/// `folders` with the folder of the documents of `session_id` added to what
/// its file tools read, when that conversation holds any.
///
/// The folder is added after the folders granted to the agent, so a relative
/// path still lands in the first of those. It is not added for writing.
pub(in crate::chat::manager) fn with_session_documents(
    mut folders: apollia_tools::FolderRoots,
    data_dir: Option<&std::path::Path>,
    session_id: &str,
) -> apollia_tools::FolderRoots {
    if let Some(data_dir) = data_dir {
        let dir = documents::session_documents_dir(data_dir, session_id);
        if dir.is_dir() {
            folders.read.push(dir);
        }
    }
    folders
}

impl ChatSessionManagerHandle {
    /// Send a user message carrying images and documents.
    ///
    /// Each document is copied under the folder of the conversation and its
    /// text extracted: up to [`documents::INLINE_MAX_CHARS`] characters it
    /// follows the text of the message, after the file name; past that the
    /// message carries the name, the size and the path of the copy, which the
    /// agent reads with `file_read`.
    ///
    /// # Errors
    /// [`ChatError::DocumentRejected`] when a reader cannot read a document,
    /// [`ChatError::SessionNotFound`] for an unknown session, and what
    /// [`Self::send_message_with_images`] refuses. A refused message keeps no
    /// copy of its documents.
    pub async fn send_message_with_attachments(
        &self,
        session_id: SessionId,
        content: String,
        images: Vec<apollia_llm::message_image::MessageImage>,
        message_documents: Vec<documents::MessageDocument>,
    ) -> Result<MessageId, ChatError> {
        if message_documents.is_empty() {
            return self
                .send_message_with_images(session_id, content, images)
                .await;
        }
        // An unknown session is refused before anything is written for it.
        self.session_model(session_id.clone()).await?;
        let dir = self
            .data_dir
            .as_deref()
            .map(|data_dir| documents::session_documents_dir(data_dir, &session_id))
            .ok_or_else(|| {
                ChatError::InternalError("no data directory to keep the documents in".into())
            })?;
        let folder = dir.clone();
        let (stored, blocks) =
            tokio::task::spawn_blocking(move || documents::receive(&folder, &message_documents))
                .await
                .map_err(|e| ChatError::InternalError(format!("document reading: {e}")))??;
        documents::trace_documents(&session_id, &stored);

        let received = documents::MessageDocuments {
            text_end: content.encode_utf16().count(),
            stored,
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::SendMessage {
                session_id,
                content: format!("{content}{blocks}"),
                images,
                documents: Some(received.clone()),
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()));
        let result = match sent {
            Ok(()) => reply_rx
                .await
                .map_err(|_| ChatError::InternalError("actor reply dropped".into()))
                .and_then(|reply| reply),
            Err(e) => Err(e),
        };
        if result.is_err() {
            let stored = received.stored;
            let _ = tokio::task::spawn_blocking(move || documents::discard(&dir, &stored)).await;
        }
        result
    }
}
