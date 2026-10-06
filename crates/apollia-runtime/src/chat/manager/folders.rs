//! The folders of the conversation list and the search in the messages, as
//! the actor serves them. Both read and write the chat database only: no
//! session in memory changes when a conversation changes folder.

use super::*;
use crate::chat::repository::{ChatFolder, MessageSearchHit};

/// One action on the folders of the conversation list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderAction {
    /// Create a folder with this name; it is answered alone.
    Create {
        /// Name of the folder.
        name: String,
    },
    /// Give a folder a new name.
    Rename {
        /// Target folder.
        folder_id: String,
        /// New name.
        name: String,
    },
    /// Remove a folder; its conversations go back out of any folder.
    Delete {
        /// Target folder.
        folder_id: String,
    },
    /// Put a session in a folder, or out of any folder with `None`.
    MoveSession {
        /// Target session.
        session_id: SessionId,
        /// Destination folder.
        folder_id: Option<String>,
    },
}

impl ChatSessionManager {
    /// Apply `action` and answer the folders as they are afterwards; a
    /// creation answers the new folder alone.
    pub(in crate::chat::manager) fn handle_folder_action(
        &mut self,
        action: FolderAction,
    ) -> Result<Vec<ChatFolder>, ChatError> {
        match action {
            FolderAction::Create { name } => {
                let folder = self.repository.create_folder(&name)?;
                info!(folder_id = %folder.id, "chat.folder.created");
                return Ok(vec![folder]);
            }
            FolderAction::Rename { folder_id, name } => {
                self.repository.rename_folder(&folder_id, &name)?;
                info!(folder_id = %folder_id, "chat.folder.renamed");
            }
            FolderAction::Delete { folder_id } => {
                let released = self.repository.delete_folder(&folder_id)?;
                info!(folder_id = %folder_id, released, "chat.folder.deleted");
            }
            FolderAction::MoveSession {
                session_id,
                folder_id,
            } => {
                self.repository
                    .move_session_to_folder(&session_id, folder_id.as_deref())?;
                info!(
                    session_id = %session_id,
                    in_folder = folder_id.is_some(),
                    "chat.session.moved"
                );
            }
        }
        self.repository.list_folders()
    }
}

impl ChatSessionManagerHandle {
    async fn folders(&self, action: Option<FolderAction>) -> Result<Vec<ChatFolder>, ChatError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(ChatCommand::Folders { action, reply })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;
        answer
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// The folders of the conversation list, in their order, each with the
    /// conversations it holds.
    ///
    /// # Errors
    ///
    /// [`ChatError::InternalError`] when the actor is gone or the database
    /// refuses the read.
    pub async fn list_folders(&self) -> Result<Vec<ChatFolder>, ChatError> {
        self.folders(None).await
    }

    /// Create a folder, placed after the existing ones.
    ///
    /// # Errors
    ///
    /// [`ChatError::InvalidFolderName`] when the name is blank or too long.
    pub async fn create_folder(&self, name: String) -> Result<ChatFolder, ChatError> {
        self.folders(Some(FolderAction::Create { name }))
            .await?
            .pop()
            .ok_or_else(|| ChatError::InternalError("no folder answered".into()))
    }

    /// Give a folder a new name.
    ///
    /// # Errors
    ///
    /// [`ChatError::InvalidFolderName`] or [`ChatError::FolderNotFound`].
    pub async fn rename_folder(&self, folder_id: String, name: String) -> Result<(), ChatError> {
        self.folders(Some(FolderAction::Rename { folder_id, name }))
            .await
            .map(|_| ())
    }

    /// Remove a folder; its conversations are kept, out of any folder.
    ///
    /// # Errors
    ///
    /// [`ChatError::FolderNotFound`] when no folder has this id.
    pub async fn delete_folder(&self, folder_id: String) -> Result<(), ChatError> {
        self.folders(Some(FolderAction::Delete { folder_id }))
            .await
            .map(|_| ())
    }

    /// Put a session in a folder, or out of any folder with `None`.
    ///
    /// # Errors
    ///
    /// [`ChatError::FolderNotFound`] or [`ChatError::SessionNotFound`].
    pub async fn move_session_to_folder(
        &self,
        session_id: SessionId,
        folder_id: Option<String>,
    ) -> Result<(), ChatError> {
        self.folders(Some(FolderAction::MoveSession {
            session_id,
            folder_id,
        }))
        .await
        .map(|_| ())
    }

    /// The conversations whose messages contain every word of `query`, each
    /// with an excerpt, `limit` at most; none from the trash.
    ///
    /// # Errors
    ///
    /// [`ChatError::InternalError`] when the actor is gone or the database
    /// refuses the read.
    pub async fn search_messages(
        &self,
        query: String,
        limit: usize,
    ) -> Result<Vec<MessageSearchHit>, ChatError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(ChatCommand::SearchMessages {
                query,
                limit,
                reply,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;
        answer
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }
}
