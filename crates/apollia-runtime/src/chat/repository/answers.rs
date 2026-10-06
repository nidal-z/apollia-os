//! What the actions on one message write: the removal of a message with the
//! tool messages of its turn, and the rewriting of an answer in place.
//!
//! Both leave the summary of the session behind. It was written from the
//! messages as they were; once one of them is gone or reads differently, the
//! next exchange summarizes again from what is left.

use rusqlite::params;

use super::ChatSessionRepository;
use crate::chat::types::ChatError;

/// What a message reads after it was rewritten.
#[derive(Debug, Clone, Copy)]
pub struct MessageRewrite<'a> {
    /// Its new text.
    pub content: &'a str,
    /// Its tool calls, serialized; `None` keeps the ones it has.
    pub tool_calls_json: Option<&'a str>,
    /// Its metadata, serialized.
    pub metadata: Option<&'a str>,
    /// Whether the summary of the session is forgotten too: yes for a rewrite
    /// that changes what was said, no for one that extends it.
    pub drop_summary: bool,
}

impl ChatSessionRepository {
    /// Remove the messages of `session_id` whose `seq` is listed, their search
    /// entries and the summary of the session, together or not at all.
    /// Returns the number of messages removed.
    pub fn delete_messages(&self, session_id: &str, seqs: &[u32]) -> Result<u32, ChatError> {
        // SAFETY: single writer behind the actor that owns this repository, so
        // `unchecked_transaction` over `&self` is sound; the rows, their index
        // entries and the summary go together.
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| ChatError::InternalError(format!("delete_messages begin: {e}")))?;
        let mut deleted = 0;
        for seq in seqs {
            deleted += tx
                .execute(
                    "DELETE FROM chat_messages WHERE session_id = ?1 AND seq = ?2",
                    params![session_id, seq],
                )
                .map_err(|e| ChatError::InternalError(format!("delete_messages: {e}")))?;
            tx.execute(
                "DELETE FROM chat_messages_fts WHERE session_id = ?1 AND seq = ?2",
                params![session_id, seq],
            )
            .map_err(|e| ChatError::InternalError(format!("delete_messages index: {e}")))?;
        }
        Self::drop_summary(&tx, session_id)?;
        tx.commit()
            .map_err(|e| ChatError::InternalError(format!("delete_messages commit: {e}")))?;
        Ok(deleted as u32)
    }

    /// Rewrite the message `message_id` as `rewrite` says; its search entry
    /// follows.
    ///
    /// # Errors
    ///
    /// [`ChatError::MessageNotFound`] when the session holds no such message.
    pub fn rewrite_message(
        &self,
        session_id: &str,
        message_id: &str,
        rewrite: MessageRewrite<'_>,
    ) -> Result<(), ChatError> {
        let MessageRewrite {
            content,
            tool_calls_json,
            metadata,
            drop_summary,
        } = rewrite;
        // SAFETY: single writer, as in `delete_messages`.
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| ChatError::InternalError(format!("rewrite_message begin: {e}")))?;
        let found: Option<(u32, String)> = match tx.query_row(
            "SELECT seq, role FROM chat_messages WHERE session_id = ?1 AND id = ?2",
            params![session_id, message_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok(found) => Some(found),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => {
                return Err(ChatError::InternalError(format!(
                    "rewrite_message read: {e}"
                )))
            }
        };
        let Some((seq, role)) = found else {
            return Err(ChatError::MessageNotFound(message_id.to_string()));
        };
        tx.execute(
            "UPDATE chat_messages
                SET content = ?1,
                    tool_calls_json = COALESCE(?2, tool_calls_json),
                    metadata = ?3
              WHERE session_id = ?4 AND id = ?5",
            params![content, tool_calls_json, metadata, session_id, message_id],
        )
        .map_err(|e| ChatError::InternalError(format!("rewrite_message: {e}")))?;
        tx.execute(
            "DELETE FROM chat_messages_fts WHERE session_id = ?1 AND seq = ?2",
            params![session_id, seq],
        )
        .map_err(|e| ChatError::InternalError(format!("rewrite_message index: {e}")))?;
        Self::index_message(&tx, session_id, seq, &role, content)
            .map_err(|e| ChatError::InternalError(format!("rewrite_message index: {e}")))?;
        if drop_summary {
            Self::drop_summary(&tx, session_id)?;
        }
        tx.commit()
            .map_err(|e| ChatError::InternalError(format!("rewrite_message commit: {e}")))
    }

    /// Forget the summary of `session_id` and its entry in the index the
    /// recall of past sessions searches.
    fn drop_summary(conn: &rusqlite::Connection, session_id: &str) -> Result<(), ChatError> {
        conn.execute(
            "UPDATE chat_sessions SET summary = NULL WHERE id = ?1",
            params![session_id],
        )
        .map_err(|e| ChatError::InternalError(format!("drop_summary: {e}")))?;
        conn.execute(
            "DELETE FROM chat_sessions_fts WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| ChatError::InternalError(format!("drop_summary index: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::repository::AppendMessageParams;
    use crate::chat::types::{ChatMode, ChatRole};

    /// A session `s-1` holding a user message, an answer, a tool message of
    /// that answer's turn, then a second user message, with a summary.
    fn seeded() -> (tempfile::TempDir, ChatSessionRepository) {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = ChatSessionRepository::open(&dir.path().join("chat.db")).expect("open");
        repo.create_session(
            "s-1",
            &ChatMode::Libre,
            None,
            "prompt",
            &[],
            "2026-10-06T10:00:00Z",
            None,
        )
        .expect("session");
        for (id, role, content) in [
            ("u-1", ChatRole::User, "where do pears grow"),
            ("a-1", ChatRole::Assistant, "pears grow in orchards"),
            ("t-1", ChatRole::Tool, "orchard.txt"),
            ("u-2", ChatRole::User, "and cherries"),
        ] {
            repo.append_message(&AppendMessageParams {
                id,
                session_id: "s-1",
                role: &role,
                content,
                tool_calls_json: None,
                tool_name: None,
                created_at: "2026-10-06T10:00:01Z",
                metadata: None,
            })
            .expect("append");
        }
        repo.update_summary("s-1", "a talk about fruit trees")
            .expect("summary");
        (dir, repo)
    }

    fn ids(repo: &ChatSessionRepository) -> Vec<String> {
        repo.get_messages("s-1", None)
            .expect("messages")
            .into_iter()
            .map(|m| m.id)
            .collect()
    }

    #[test]
    fn deleting_messages_removes_them_their_search_entries_and_the_summary() {
        // GIVEN a session with four messages, searchable, and a summary
        let (_dir, repo) = seeded();
        assert_eq!(
            repo.search_messages("orchards", 10).expect("search").len(),
            1
        );

        // WHEN the answer and the tool message of its turn are deleted
        let deleted = repo.delete_messages("s-1", &[2, 3]).expect("delete");

        // THEN both are gone, the others stay in order, the answer is no
        // longer found by its words, and the summary is forgotten
        assert_eq!(deleted, 2);
        assert_eq!(ids(&repo), ["u-1", "u-2"]);
        assert!(repo
            .search_messages("orchards", 10)
            .expect("search")
            .is_empty());
        assert_eq!(repo.get_summary("s-1").expect("summary"), None);
        assert!(repo
            .find_relevant_sessions("fruit trees", 5)
            .expect("recall")
            .is_empty());
    }

    #[test]
    fn rewriting_an_answer_replaces_its_text_and_its_search_entry() {
        // GIVEN the same session
        let (_dir, repo) = seeded();

        // WHEN the answer is rewritten, marked edited, and the summary dropped
        repo.rewrite_message(
            "s-1",
            "a-1",
            MessageRewrite {
                content: "pears grow on espaliers",
                tool_calls_json: None,
                metadata: Some(r#"{"edited":true}"#),
                drop_summary: true,
            },
        )
        .expect("rewrite");

        // THEN the message reads the new text at the same place, is found by
        // its new words and no longer by the old ones, and the summary is gone
        let messages = repo.get_messages("s-1", None).expect("messages");
        assert_eq!(messages[1].id, "a-1");
        assert_eq!(messages[1].content, "pears grow on espaliers");
        assert_eq!(messages[1].metadata.as_deref(), Some(r#"{"edited":true}"#));
        assert_eq!(
            repo.search_messages("espaliers", 10).expect("search").len(),
            1
        );
        assert!(repo
            .search_messages("orchards", 10)
            .expect("search")
            .is_empty());
        assert_eq!(repo.get_summary("s-1").expect("summary"), None);
    }

    #[test]
    fn rewriting_an_unknown_message_is_refused_and_keeps_the_summary() {
        // GIVEN the same session
        let (_dir, repo) = seeded();

        // WHEN a message the session does not hold is rewritten
        let refused = repo.rewrite_message(
            "s-1",
            "ghost",
            MessageRewrite {
                content: "text",
                tool_calls_json: None,
                metadata: None,
                drop_summary: true,
            },
        );

        // THEN it is refused as not found and nothing was forgotten
        assert!(matches!(refused, Err(ChatError::MessageNotFound(id)) if id == "ghost"));
        assert_eq!(
            repo.get_summary("s-1").expect("summary").as_deref(),
            Some("a talk about fruit trees")
        );
    }
}
