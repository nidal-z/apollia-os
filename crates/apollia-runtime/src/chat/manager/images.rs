//! The images of a user message, as the manager checks, stores and reads them.

use apollia_llm::message_image::MessageImage;

use super::*;
use crate::chat::images;

impl ChatSessionManager {
    /// Refuse images the conversation cannot carry: an agent that runs code
    /// (its SDK reads text only), or a model that does not read images, or
    /// whose capabilities are not known.
    pub(in crate::chat::manager) fn check_images_readable(
        &self,
        session_id: &str,
        images: &[MessageImage],
    ) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
        let model = self.handle_session_model(session_id)?;
        if session.mode == ChatMode::Agent && self.declarative_session_agent(session).is_none() {
            return Err(ChatError::ImagesNotSupported { model });
        }
        let capabilities = self
            .repository
            .data_dir()
            .and_then(|data_dir| images::model_capabilities_in(data_dir, model.as_deref()))
            .and_then(|found| found.capabilities);
        let checked = images::check_model_reads(model.as_deref(), capabilities.as_ref(), images);
        if let Err(e) = &checked {
            tracing::event!(
                tracing::Level::INFO,
                session_id = %session_id,
                count = images.len(),
                reason = %e,
                "chat.images.refused"
            );
        }
        checked
    }

    /// Write the images of a new user message and return the metadata that
    /// references them, `None` when it carries none.
    pub(in crate::chat::manager) fn store_message_images(
        &self,
        session_id: &str,
        message_id: &str,
        message_images: &[MessageImage],
    ) -> Result<Option<serde_json::Value>, ChatError> {
        if message_images.is_empty() {
            return Ok(None);
        }
        let dir = self
            .repository
            .session_images_dir(session_id)
            .ok_or_else(|| {
                ChatError::InternalError("no data directory to keep the images in".into())
            })?;
        images::trace_images(session_id, message_images);
        let stored = images::store_images(&dir, message_id, message_images)?;
        Ok(Some(images::images_metadata(&stored)))
    }

    /// The images of a turn on `history`, which ends with the message the turn
    /// answers: that message's, and those of the earlier user messages a
    /// window of `window` messages keeps.
    pub(in crate::chat::manager) fn turn_images(
        &self,
        session_id: &str,
        history: &[ChatMessage],
        window: usize,
    ) -> images::TurnImages {
        let mut turn = images::TurnImages::default();
        let start = history.len().saturating_sub(window.saturating_add(1));
        let Some((last, earlier)) = history[start..].split_last() else {
            return turn;
        };
        if last.role == ChatRole::User {
            turn.current = self.message_images(session_id, last);
        }
        for message in earlier.iter().filter(|m| m.role == ChatRole::User) {
            let found = self.message_images(session_id, message);
            if !found.is_empty() {
                turn.history.insert(message.id.clone(), found);
            }
        }
        turn
    }

    /// The images a message of `session_id` references, read back from disk.
    pub(in crate::chat::manager) fn message_images(
        &self,
        session_id: &str,
        message: &ChatMessage,
    ) -> Vec<MessageImage> {
        let stored = images::stored_images(message.metadata.as_ref());
        if stored.is_empty() {
            return Vec::new();
        }
        match self.repository.session_images_dir(session_id) {
            Some(dir) => images::load_images(&dir, &stored),
            None => Vec::new(),
        }
    }
}

impl ChatSessionManagerHandle {
    /// What the model the next message of `session_id` would run on can do,
    /// read from the data directory of the chat.
    ///
    /// # Errors
    /// [`ChatError::SessionNotFound`] for an unknown session.
    pub async fn session_model_capabilities(
        &self,
        session_id: SessionId,
    ) -> Result<crate::local_engine::ModelCapabilities, ChatError> {
        let model = self.session_model(session_id).await?;
        let found = self
            .data_dir
            .as_deref()
            .and_then(|dir| images::model_capabilities_in(dir, model.as_deref()));
        Ok(found.unwrap_or(crate::local_engine::ModelCapabilities {
            model,
            local: false,
            capabilities: None,
        }))
    }

    /// One stored image of `session_id`, by the file name its message
    /// metadata lists.
    ///
    /// # Errors
    /// [`ChatError::ImageRejected`] for a name that is not a plain image file
    /// name, [`ChatError::InternalError`] when it cannot be read.
    pub fn session_image(
        &self,
        session_id: &str,
        file: &str,
    ) -> Result<images::ImageAttachment, ChatError> {
        let dir = self
            .data_dir
            .as_deref()
            .ok_or_else(|| ChatError::InternalError("no data directory".into()))?;
        images::read_session_image(dir, session_id, file)
    }
}
