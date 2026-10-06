//! The trash of the conversations, in `chat_sessions.deleted_at`.
//!
//! A conversation sent to the trash keeps its row, its messages, its tool
//! authorizations and its files: only the date it was removed is written.
//! Every listing of the repository leaves it out, and restoring it empties
//! the date. Erasing for good stays
//! [`ChatSessionRepository::delete_session`], which removes a conversation
//! whether it is in the trash or not.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::ChatSessionRepository;
use crate::chat::types::ChatError;

/// A conversation in the trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TrashedSession {
    /// Session identifier.
    pub id: String,
    /// Mode string (`"libre"`, `"agent"` or `"companion"`).
    pub mode: String,
    /// Agent of the conversation, when it has one.
    pub agent_name: Option<String>,
    /// Title the user or the model gave it, when it has one.
    pub title: Option<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 timestamp of the move to the trash.
    pub deleted_at: String,
}

impl ChatSessionRepository {
    /// Send a session to the trash, dated `deleted_at`. A session already
    /// there keeps its first date.
    ///
    /// # Errors
    ///
    /// [`ChatError::SessionNotFound`] when no session has this id.
    pub fn trash_session(&self, id: &str, deleted_at: &str) -> Result<(), ChatError> {
        let touched = self
            .conn
            .execute(
                "UPDATE chat_sessions SET deleted_at = COALESCE(deleted_at, ?1) WHERE id = ?2",
                params![deleted_at, id],
            )
            .map_err(|e| ChatError::InternalError(format!("trash_session: {e}")))?;
        if touched == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Take a session out of the trash, back into the list.
    ///
    /// # Errors
    ///
    /// [`ChatError::SessionNotFound`] when the trash holds no session with
    /// this id.
    pub fn restore_session(&self, id: &str) -> Result<(), ChatError> {
        let touched = self
            .conn
            .execute(
                "UPDATE chat_sessions SET deleted_at = NULL
                 WHERE id = ?1 AND deleted_at IS NOT NULL",
                params![id],
            )
            .map_err(|e| ChatError::InternalError(format!("restore_session: {e}")))?;
        if touched == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Whether the session `id` is in the trash. An unknown session is not.
    pub fn is_trashed(&self, id: &str) -> Result<bool, ChatError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM chat_sessions WHERE id = ?1 AND deleted_at IS NOT NULL",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .map_err(|e| ChatError::InternalError(format!("is_trashed: {e}")))
    }

    /// The sessions in the trash, the last removed first.
    pub fn list_trashed_sessions(&self) -> Result<Vec<TrashedSession>, ChatError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, mode, agent_name, title, created_at, deleted_at
                 FROM chat_sessions WHERE deleted_at IS NOT NULL
                 ORDER BY deleted_at DESC, id",
            )
            .map_err(|e| ChatError::InternalError(format!("list_trashed_sessions prepare: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TrashedSession {
                    id: row.get(0)?,
                    mode: row.get(1)?,
                    agent_name: row.get(2)?,
                    title: row.get(3)?,
                    created_at: row.get(4)?,
                    deleted_at: row.get(5)?,
                })
            })
            .map_err(|e| ChatError::InternalError(format!("list_trashed_sessions query: {e}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| ChatError::InternalError(format!("list_trashed_sessions row: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::repository::AppendMessageParams;
    use crate::chat::types::{ChatMode, ChatRole};

    fn repo_with_session(id: &str) -> ChatSessionRepository {
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        add_session(&repo, id, "2026-10-01T00:00:00Z");
        repo
    }

    fn add_session(repo: &ChatSessionRepository, id: &str, created_at: &str) {
        repo.create_session(id, &ChatMode::Libre, None, "", &[], created_at, None)
            .expect("session created");
        for (n, content) in ["hello", "hello, what can I do?"].iter().enumerate() {
            let message_id = format!("{id}-m{n}");
            let role = if n == 0 {
                ChatRole::User
            } else {
                ChatRole::Assistant
            };
            repo.append_message(&AppendMessageParams {
                id: &message_id,
                session_id: id,
                role: &role,
                content,
                tool_calls_json: None,
                tool_name: None,
                created_at,
                metadata: None,
            })
            .expect("message appended");
        }
    }

    fn listed(repo: &ChatSessionRepository) -> Vec<String> {
        repo.list_sessions(None)
            .expect("list")
            .into_iter()
            .map(|s| s.id)
            .collect()
    }

    fn trashed(repo: &ChatSessionRepository) -> Vec<String> {
        repo.list_trashed_sessions()
            .expect("trash")
            .into_iter()
            .map(|s| s.id)
            .collect()
    }

    #[test]
    fn a_trashed_session_leaves_the_list_and_keeps_its_messages() {
        // GIVEN a session holding two messages, and another one beside it
        let repo = repo_with_session("s1");
        add_session(&repo, "s2", "2026-10-02T00:00:00Z");

        // WHEN the first is sent to the trash
        repo.trash_session("s1", "2026-10-05T10:00:00Z")
            .expect("trashed");

        // THEN it is absent from the list, present in the trash with its
        // date, and its messages are intact
        assert_eq!(listed(&repo), ["s2"]);
        let in_trash = repo.list_trashed_sessions().expect("trash");
        assert_eq!(in_trash.len(), 1);
        assert_eq!(in_trash[0].id, "s1");
        assert_eq!(in_trash[0].deleted_at, "2026-10-05T10:00:00Z");
        assert!(repo.is_trashed("s1").expect("read"));
        let messages = repo.get_messages("s1", None).expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "hello");
    }

    #[test]
    fn a_trashed_session_is_left_out_of_the_recent_and_the_status_listings() {
        // GIVEN two active sessions, one of them in the trash
        let repo = repo_with_session("s1");
        add_session(&repo, "s2", "2026-10-02T00:00:00Z");
        repo.trash_session("s1", "2026-10-05T10:00:00Z")
            .expect("trashed");

        // WHEN the recent sessions and the active sessions are listed
        let recent: Vec<String> = repo
            .list_recent_summaries(10)
            .expect("recent")
            .into_iter()
            .map(|s| s.id)
            .collect();
        let active: Vec<String> = repo
            .list_sessions(Some("active"))
            .expect("active")
            .into_iter()
            .map(|s| s.id)
            .collect();

        // THEN neither names the trashed one
        assert_eq!(recent, ["s2"]);
        assert_eq!(active, ["s2"]);
    }

    #[test]
    fn a_trashed_session_is_left_out_of_the_summary_search() {
        // GIVEN two summarized sessions on the same subject, one in the trash
        let repo = repo_with_session("s1");
        add_session(&repo, "s2", "2026-10-02T00:00:00Z");
        for id in ["s1", "s2"] {
            repo.update_summary(id, "quarterly invoices of the Lyon office")
                .expect("summary");
        }
        repo.trash_session("s1", "2026-10-05T10:00:00Z")
            .expect("trashed");

        // WHEN the past sessions are searched on that subject
        let found: Vec<String> = repo
            .find_relevant_sessions("invoices Lyon", 5)
            .expect("search")
            .into_iter()
            .map(|s| s.session_id)
            .collect();

        // THEN only the session still in the list answers
        assert_eq!(found, ["s2"]);
    }

    #[test]
    fn restoring_puts_the_session_back_in_the_list_with_its_messages() {
        // GIVEN a session in the trash
        let repo = repo_with_session("s1");
        repo.trash_session("s1", "2026-10-05T10:00:00Z")
            .expect("trashed");

        // WHEN it is restored
        repo.restore_session("s1").expect("restored");

        // THEN the list holds it again, the trash is empty, the messages are
        // the ones it had
        assert_eq!(listed(&repo), ["s1"]);
        assert!(trashed(&repo).is_empty());
        assert!(!repo.is_trashed("s1").expect("read"));
        assert_eq!(repo.get_messages("s1", None).expect("messages").len(), 2);
    }

    #[test]
    fn trashing_twice_keeps_the_first_date() {
        // GIVEN a session already in the trash
        let repo = repo_with_session("s1");
        repo.trash_session("s1", "2026-10-05T10:00:00Z")
            .expect("trashed");

        // WHEN it is sent there again, later
        repo.trash_session("s1", "2026-10-06T09:00:00Z")
            .expect("trashed again");

        // THEN the date is the first one
        let in_trash = repo.list_trashed_sessions().expect("trash");
        assert_eq!(in_trash[0].deleted_at, "2026-10-05T10:00:00Z");
    }

    #[test]
    fn the_trash_lists_the_last_removed_first() {
        // GIVEN two sessions removed one day apart
        let repo = repo_with_session("old");
        add_session(&repo, "new", "2026-10-02T00:00:00Z");
        repo.trash_session("new", "2026-10-04T10:00:00Z")
            .expect("trashed");
        repo.trash_session("old", "2026-10-05T10:00:00Z")
            .expect("trashed");

        // WHEN the trash is listed
        // THEN the one removed last comes first
        assert_eq!(trashed(&repo), ["old", "new"]);
    }

    #[test]
    fn trashing_an_unknown_session_and_restoring_a_listed_one_are_refused() {
        // GIVEN one session, in the list
        let repo = repo_with_session("s1");

        // WHEN an unknown session is trashed, and the listed one is restored
        let unknown = repo.trash_session("ghost", "2026-10-05T10:00:00Z");
        let not_in_trash = repo.restore_session("s1");

        // THEN both answer that the session is not found
        assert!(matches!(unknown, Err(ChatError::SessionNotFound(_))));
        assert!(matches!(not_in_trash, Err(ChatError::SessionNotFound(_))));
    }

    #[test]
    fn erasing_a_trashed_session_removes_it_for_good() {
        // GIVEN a session in the trash
        let repo = repo_with_session("s1");
        repo.trash_session("s1", "2026-10-05T10:00:00Z")
            .expect("trashed");

        // WHEN it is erased
        repo.delete_session("s1").expect("erased");

        // THEN neither the trash nor the messages hold it
        assert!(trashed(&repo).is_empty());
        assert!(repo.get_messages("s1", None).expect("messages").is_empty());
        assert!(repo.get_session("s1").expect("read").is_none());
    }
}
