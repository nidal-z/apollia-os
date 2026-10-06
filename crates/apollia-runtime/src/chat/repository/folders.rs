//! The folders of the conversation list, in `chat_folders` and
//! `chat_sessions.folder_id`.
//!
//! A folder is a name and a place in the list. A conversation is in one
//! folder or in none. Removing a folder never removes a conversation: its
//! conversations go back out of any folder, the ones in the trash included,
//! so a conversation restored later does not point at a folder that is gone.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{ChatSessionRepository, OptionalExt};
use crate::chat::types::ChatError;

/// Longest name a folder takes, in characters.
pub const FOLDER_NAME_MAX_CHARS: usize = 100;

/// A folder of the conversation list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChatFolder {
    /// Folder identifier.
    pub id: String,
    /// Name the user gave it.
    pub name: String,
    /// Place in the list, the lowest first.
    pub position: i64,
    /// The conversations in the folder, the ones in the trash left out.
    pub session_ids: Vec<String>,
}

/// The name a folder is stored under: trimmed, never empty, at most
/// [`FOLDER_NAME_MAX_CHARS`] characters.
fn folder_name(raw: &str) -> Result<&str, ChatError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ChatError::InvalidFolderName(
            "a folder needs a name".to_owned(),
        ));
    }
    if name.chars().count() > FOLDER_NAME_MAX_CHARS {
        return Err(ChatError::InvalidFolderName(format!(
            "a folder name holds at most {FOLDER_NAME_MAX_CHARS} characters"
        )));
    }
    Ok(name)
}

impl ChatSessionRepository {
    /// Create a folder named `name`, placed after the existing ones.
    ///
    /// # Errors
    ///
    /// [`ChatError::InvalidFolderName`] when the name is blank or too long.
    pub fn create_folder(&self, name: &str) -> Result<ChatFolder, ChatError> {
        let name = folder_name(name)?;
        let id = uuid::Uuid::new_v4().to_string();
        let position: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM chat_folders",
                [],
                |row| row.get(0),
            )
            .map_err(|e| ChatError::InternalError(format!("create_folder position: {e}")))?;
        self.conn
            .execute(
                "INSERT INTO chat_folders (id, name, position) VALUES (?1, ?2, ?3)",
                params![id, name, position],
            )
            .map_err(|e| ChatError::InternalError(format!("create_folder insert: {e}")))?;
        Ok(ChatFolder {
            id,
            name: name.to_owned(),
            position,
            session_ids: Vec::new(),
        })
    }

    /// Give the folder `id` a new name.
    ///
    /// # Errors
    ///
    /// [`ChatError::InvalidFolderName`] when the name is blank or too long,
    /// [`ChatError::FolderNotFound`] when no folder has this id.
    pub fn rename_folder(&self, id: &str, name: &str) -> Result<(), ChatError> {
        let name = folder_name(name)?;
        let touched = self
            .conn
            .execute(
                "UPDATE chat_folders SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .map_err(|e| ChatError::InternalError(format!("rename_folder: {e}")))?;
        if touched == 0 {
            return Err(ChatError::FolderNotFound(id.to_owned()));
        }
        Ok(())
    }

    /// Remove the folder `id`. Its conversations are kept and go back out of
    /// any folder; the count of them is returned.
    ///
    /// # Errors
    ///
    /// [`ChatError::FolderNotFound`] when no folder has this id.
    pub fn delete_folder(&self, id: &str) -> Result<usize, ChatError> {
        // SAFETY: same single-writer invariant as `delete_session`; the
        // transaction makes the two writes one, and an early `?` rolls back.
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| ChatError::InternalError(format!("delete_folder begin: {e}")))?;
        let released = tx
            .execute(
                "UPDATE chat_sessions SET folder_id = NULL WHERE folder_id = ?1",
                params![id],
            )
            .map_err(|e| ChatError::InternalError(format!("delete_folder release: {e}")))?;
        let deleted = tx
            .execute("DELETE FROM chat_folders WHERE id = ?1", params![id])
            .map_err(|e| ChatError::InternalError(format!("delete_folder: {e}")))?;
        if deleted == 0 {
            return Err(ChatError::FolderNotFound(id.to_owned()));
        }
        tx.commit()
            .map_err(|e| ChatError::InternalError(format!("delete_folder commit: {e}")))?;
        Ok(released)
    }

    /// Put the session in the folder `folder_id`, or out of any folder when
    /// it is `None`.
    ///
    /// # Errors
    ///
    /// [`ChatError::FolderNotFound`] when a folder is named and does not
    /// exist, [`ChatError::SessionNotFound`] when no session has this id.
    pub fn move_session_to_folder(
        &self,
        session_id: &str,
        folder_id: Option<&str>,
    ) -> Result<(), ChatError> {
        if let Some(folder) = folder_id {
            let known = self
                .conn
                .query_row(
                    "SELECT 1 FROM chat_folders WHERE id = ?1",
                    params![folder],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| ChatError::InternalError(format!("move_session folder: {e}")))?;
            if known.is_none() {
                return Err(ChatError::FolderNotFound(folder.to_owned()));
            }
        }
        let touched = self
            .conn
            .execute(
                "UPDATE chat_sessions SET folder_id = ?1 WHERE id = ?2",
                params![folder_id, session_id],
            )
            .map_err(|e| ChatError::InternalError(format!("move_session: {e}")))?;
        if touched == 0 {
            return Err(ChatError::SessionNotFound(session_id.to_owned()));
        }
        Ok(())
    }

    /// The folders in the order of the list, each with the conversations it
    /// holds, the most recent first.
    pub fn list_folders(&self) -> Result<Vec<ChatFolder>, ChatError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, position FROM chat_folders ORDER BY position, id")
            .map_err(|e| ChatError::InternalError(format!("list_folders prepare: {e}")))?;
        let mut folders = stmt
            .query_map([], |row| {
                Ok(ChatFolder {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    position: row.get(2)?,
                    session_ids: Vec::new(),
                })
            })
            .map_err(|e| ChatError::InternalError(format!("list_folders query: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ChatError::InternalError(format!("list_folders row: {e}")))?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT folder_id, id FROM chat_sessions
                 WHERE folder_id IS NOT NULL AND deleted_at IS NULL
                 ORDER BY created_at DESC, id",
            )
            .map_err(|e| ChatError::InternalError(format!("list_folders members prepare: {e}")))?;
        let members = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| ChatError::InternalError(format!("list_folders members query: {e}")))?;
        for member in members {
            let (folder_id, session_id) = member
                .map_err(|e| ChatError::InternalError(format!("list_folders members row: {e}")))?;
            if let Some(folder) = folders.iter_mut().find(|f| f.id == folder_id) {
                folder.session_ids.push(session_id);
            }
        }
        Ok(folders)
    }

    /// The name of the folder each listed session is in, by session id. A
    /// session out of any folder, or in the trash, is absent.
    pub fn session_folder_names(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, ChatError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.id, f.name FROM chat_sessions s
                 JOIN chat_folders f ON f.id = s.folder_id
                 WHERE s.deleted_at IS NULL",
            )
            .map_err(|e| ChatError::InternalError(format!("session_folder_names prepare: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| ChatError::InternalError(format!("session_folder_names query: {e}")))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| ChatError::InternalError(format!("session_folder_names row: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::types::ChatMode;

    fn repo_with_sessions(ids: &[&str]) -> ChatSessionRepository {
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        for (n, id) in ids.iter().enumerate() {
            let created_at = format!("2026-10-0{}T00:00:00Z", n + 1);
            repo.create_session(id, &ChatMode::Libre, None, "", &[], &created_at, None)
                .expect("session created");
        }
        repo
    }

    fn listed(repo: &ChatSessionRepository) -> Vec<String> {
        repo.list_sessions(None)
            .expect("list")
            .into_iter()
            .map(|row| row.id)
            .collect()
    }

    #[test]
    fn test_a_new_folder_is_listed_after_the_existing_ones_and_empty() {
        // GIVEN a database holding one folder
        let repo = repo_with_sessions(&[]);
        let first = repo.create_folder("Clients").expect("first");

        // WHEN a second one is created, its name typed with spaces around
        let second = repo.create_folder("  Invoices ").expect("second");

        // THEN both are listed in the order they were made, the name trimmed,
        // with no conversation in them
        let folders = repo.list_folders().expect("folders");
        assert_eq!(
            folders,
            vec![
                ChatFolder {
                    id: first.id,
                    name: "Clients".into(),
                    position: 0,
                    session_ids: vec![],
                },
                ChatFolder {
                    id: second.id,
                    name: "Invoices".into(),
                    position: 1,
                    session_ids: vec![],
                },
            ]
        );
    }

    #[test]
    fn test_a_blank_or_oversized_folder_name_is_refused() {
        // GIVEN an empty database
        let repo = repo_with_sessions(&[]);

        // WHEN a folder is created with a blank name, then with 101 characters
        let blank = repo.create_folder("   ");
        let long = repo.create_folder(&"n".repeat(FOLDER_NAME_MAX_CHARS + 1));

        // THEN both are refused and no folder exists; 100 characters pass
        assert!(matches!(blank, Err(ChatError::InvalidFolderName(_))));
        assert!(matches!(long, Err(ChatError::InvalidFolderName(_))));
        assert_eq!(repo.list_folders().expect("folders"), vec![]);
        repo.create_folder(&"n".repeat(FOLDER_NAME_MAX_CHARS))
            .expect("the longest name is accepted");
    }

    #[test]
    fn test_a_session_moves_into_a_folder_then_to_another_then_out() {
        // GIVEN two folders and two sessions out of any folder
        let repo = repo_with_sessions(&["s-old", "s-new"]);
        let clients = repo.create_folder("Clients").expect("clients");
        let drafts = repo.create_folder("Drafts").expect("drafts");
        let members = |repo: &ChatSessionRepository| -> Vec<Vec<String>> {
            repo.list_folders()
                .expect("folders")
                .into_iter()
                .map(|f| f.session_ids)
                .collect()
        };

        // WHEN both go into the first folder
        repo.move_session_to_folder("s-old", Some(&clients.id))
            .expect("move old");
        repo.move_session_to_folder("s-new", Some(&clients.id))
            .expect("move new");

        // THEN the folder holds them, the most recent first
        assert_eq!(members(&repo), [vec!["s-new", "s-old"], vec![]]);

        // WHEN one goes to the second folder and the other out of any folder
        repo.move_session_to_folder("s-old", Some(&drafts.id))
            .expect("move to drafts");
        repo.move_session_to_folder("s-new", None)
            .expect("move out");

        // THEN a session is in one folder at most, and both stay in the list
        assert_eq!(members(&repo), [vec![], vec!["s-old"]]);
        assert_eq!(listed(&repo), ["s-new", "s-old"]);
    }

    #[test]
    fn test_moving_to_an_unknown_folder_or_an_unknown_session_is_refused() {
        // GIVEN one folder and one session
        let repo = repo_with_sessions(&["s-1"]);
        let folder = repo.create_folder("Clients").expect("folder");

        // WHEN the session goes to a folder that does not exist, and a
        // session that does not exist goes to the real one
        let no_folder = repo.move_session_to_folder("s-1", Some("ghost"));
        let no_session = repo.move_session_to_folder("ghost", Some(&folder.id));

        // THEN each is refused for what is missing, and nothing moved
        assert!(matches!(no_folder, Err(ChatError::FolderNotFound(id)) if id == "ghost"));
        assert!(matches!(no_session, Err(ChatError::SessionNotFound(id)) if id == "ghost"));
        assert_eq!(
            repo.list_folders().expect("folders")[0].session_ids.len(),
            0
        );
    }

    #[test]
    fn test_deleting_a_folder_puts_its_sessions_back_out_of_any_folder() {
        // GIVEN a folder holding two sessions, one of them in the trash, and
        // another folder holding a third
        let repo = repo_with_sessions(&["s-1", "s-2", "s-3"]);
        let doomed = repo.create_folder("Doomed").expect("doomed");
        let kept = repo.create_folder("Kept").expect("kept");
        for id in ["s-1", "s-2"] {
            repo.move_session_to_folder(id, Some(&doomed.id))
                .expect("move");
        }
        repo.move_session_to_folder("s-3", Some(&kept.id))
            .expect("move");
        repo.trash_session("s-2", "2026-10-05T00:00:00Z")
            .expect("trash");

        // WHEN the first folder is deleted
        let released = repo.delete_folder(&doomed.id).expect("delete");

        // THEN its two sessions were released, none was removed, the other
        // folder is untouched, and the trashed one is restored out of any
        // folder
        assert_eq!(released, 2);
        let folders = repo.list_folders().expect("folders");
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "Kept");
        assert_eq!(folders[0].session_ids, ["s-3"]);
        assert_eq!(listed(&repo), ["s-3", "s-1"]);
        repo.restore_session("s-2").expect("restore");
        assert_eq!(listed(&repo), ["s-3", "s-2", "s-1"]);
        assert_eq!(
            repo.list_folders().expect("folders")[0].session_ids,
            ["s-3"]
        );
    }

    #[test]
    fn test_renaming_and_deleting_an_unknown_folder_are_refused() {
        // GIVEN one folder
        let repo = repo_with_sessions(&[]);
        let folder = repo.create_folder("Clients").expect("folder");

        // WHEN it is renamed, and an unknown one is renamed and deleted
        repo.rename_folder(&folder.id, " Customers ")
            .expect("rename");
        let renamed = repo.rename_folder("ghost", "Anything");
        let deleted = repo.delete_folder("ghost");

        // THEN the real one carries its new name and the two others are
        // refused as unknown
        assert_eq!(repo.list_folders().expect("folders")[0].name, "Customers");
        assert!(matches!(renamed, Err(ChatError::FolderNotFound(_))));
        assert!(matches!(deleted, Err(ChatError::FolderNotFound(_))));
    }

    #[test]
    fn test_a_trashed_session_leaves_its_folder_listing_and_comes_back_in_it() {
        // GIVEN a folder holding one session
        let repo = repo_with_sessions(&["s-1"]);
        let folder = repo.create_folder("Clients").expect("folder");
        repo.move_session_to_folder("s-1", Some(&folder.id))
            .expect("move");

        // WHEN the session goes to the trash
        repo.trash_session("s-1", "2026-10-05T00:00:00Z")
            .expect("trash");

        // THEN the folder lists nothing and names no session
        assert_eq!(
            repo.list_folders().expect("folders")[0].session_ids.len(),
            0
        );
        assert!(repo.session_folder_names().expect("names").is_empty());

        // WHEN it is restored
        repo.restore_session("s-1").expect("restore");

        // THEN it is back in its folder
        assert_eq!(
            repo.list_folders().expect("folders")[0].session_ids,
            ["s-1"]
        );
        assert_eq!(
            repo.session_folder_names().expect("names").get("s-1"),
            Some(&"Clients".to_owned())
        );
    }
}
