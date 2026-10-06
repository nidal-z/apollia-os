//! The search in what was said, over the `chat_messages_fts` index.
//!
//! The index holds one row per message a person or the model wrote, fed by
//! every write of the repository that adds or removes messages. A search
//! answers conversations, each with the excerpt of its best matching message;
//! a conversation in the trash is never answered.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::ChatSessionRepository;
use crate::chat::types::ChatError;

/// Most conversations one search answers.
pub const MESSAGE_SEARCH_MAX_RESULTS: usize = 50;

/// Words around the match an excerpt keeps.
const EXCERPT_TOKENS: u32 = 16;

/// Matching messages read per conversation answered, at most: the best
/// message of a conversation is found among them, and a conversation that
/// matches on every message cannot crowd the others out of the answer.
const ROWS_PER_RESULT: usize = 8;

/// A conversation that contains the words searched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MessageSearchHit {
    /// Session identifier.
    pub session_id: String,
    /// Title the user or the model gave the conversation, when it has one.
    pub title: Option<String>,
    /// Agent of the conversation, when it has one.
    pub agent_name: Option<String>,
    /// ISO-8601 creation timestamp of the conversation.
    pub created_at: String,
    /// The passage of the best matching message, on one line.
    pub excerpt: String,
}

/// Whether a message of this role is searched: what a person or the model
/// wrote, never a tool result or a system note.
pub(super) fn is_searched_role(role: &str) -> bool {
    matches!(role, "user" | "assistant")
}

/// The FTS5 expression of a typed query: every word must be there, each read
/// as the start of a word so a search answers while it is typed. Anything
/// that is not a letter or a digit separates words, so no character of the
/// query reaches the FTS5 syntax. Empty when the query holds no word.
pub(super) fn match_expression(query: &str) -> String {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{word}\"*"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl ChatSessionRepository {
    /// Index one message. A message of a role that is not searched, or with
    /// nothing written, is left out.
    pub(super) fn index_message(
        conn: &rusqlite::Connection,
        session_id: &str,
        seq: u32,
        role: &str,
        content: &str,
    ) -> Result<(), rusqlite::Error> {
        if !is_searched_role(role) || content.trim().is_empty() {
            return Ok(());
        }
        conn.execute(
            "INSERT INTO chat_messages_fts (session_id, seq, content) VALUES (?1, ?2, ?3)",
            params![session_id, seq, content],
        )
        .map(|_| ())
    }

    /// The conversations whose messages contain every word of `query`, the
    /// best match first, each once, `limit` at most. A conversation in the
    /// trash is left out; a query with no word answers nothing.
    pub fn search_messages(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MessageSearchHit>, ChatError> {
        let expression = match_expression(query);
        let limit = limit.min(MESSAGE_SEARCH_MAX_RESULTS);
        if expression.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT chat_messages_fts.session_id, chat_sessions.title,
                        chat_sessions.agent_name, chat_sessions.created_at,
                        snippet(chat_messages_fts, 2, '', '', '...', ?2)
                 FROM chat_messages_fts
                 JOIN chat_sessions ON chat_sessions.id = chat_messages_fts.session_id
                 WHERE chat_messages_fts MATCH ?1 AND chat_sessions.deleted_at IS NULL
                 ORDER BY rank
                 LIMIT ?3",
            )
            .map_err(|e| ChatError::InternalError(format!("search_messages prepare: {e}")))?;
        let rows = stmt
            .query_map(
                params![expression, EXCERPT_TOKENS, limit * ROWS_PER_RESULT],
                |row| {
                    Ok(MessageSearchHit {
                        session_id: row.get(0)?,
                        title: row.get(1)?,
                        agent_name: row.get(2)?,
                        created_at: row.get(3)?,
                        excerpt: row.get(4)?,
                    })
                },
            )
            .map_err(|e| ChatError::InternalError(format!("search_messages query: {e}")))?;

        let mut hits: Vec<MessageSearchHit> = Vec::new();
        for row in rows {
            let mut hit =
                row.map_err(|e| ChatError::InternalError(format!("search_messages row: {e}")))?;
            if hits.iter().any(|known| known.session_id == hit.session_id) {
                continue;
            }
            hit.excerpt = hit.excerpt.split_whitespace().collect::<Vec<_>>().join(" ");
            hits.push(hit);
            if hits.len() == limit {
                break;
            }
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::repository::AppendMessageParams;
    use crate::chat::types::{ChatMode, ChatRole};

    fn repo() -> ChatSessionRepository {
        ChatSessionRepository::open_in_memory().expect("open")
    }

    fn session(repo: &ChatSessionRepository, id: &str, title: Option<&str>) {
        repo.create_session(
            id,
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-10-01T00:00:00Z",
            None,
        )
        .expect("session created");
        if let Some(title) = title {
            repo.rename_session(id, title).expect("renamed");
        }
    }

    fn say(repo: &ChatSessionRepository, session_id: &str, role: ChatRole, content: &str) -> u32 {
        let id = uuid::Uuid::new_v4().to_string();
        repo.append_message(&AppendMessageParams {
            id: &id,
            session_id,
            role: &role,
            content,
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-10-01T00:00:00Z",
            metadata: None,
        })
        .expect("message appended")
    }

    fn found(repo: &ChatSessionRepository, query: &str) -> Vec<String> {
        repo.search_messages(query, 10)
            .expect("search")
            .into_iter()
            .map(|hit| hit.session_id)
            .collect()
    }

    #[test]
    fn test_a_word_of_a_message_finds_its_conversation_with_an_excerpt() {
        // GIVEN two conversations, one of which speaks of a quarterly invoice
        let repo = repo();
        session(&repo, "s-invoice", Some("Supplier follow-up"));
        session(&repo, "s-other", None);
        say(&repo, "s-invoice", ChatRole::User, "Hello there");
        say(
            &repo,
            "s-invoice",
            ChatRole::Assistant,
            "The quarterly invoice\nof the supplier is still unpaid.",
        );
        say(&repo, "s-other", ChatRole::User, "What is the weather?");

        // WHEN a word of that answer is searched
        let hits = repo.search_messages("invoice", 10).expect("search");

        // THEN that conversation alone is answered, with its title and the
        // passage on one line
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "s-invoice");
        assert_eq!(hits[0].title.as_deref(), Some("Supplier follow-up"));
        assert_eq!(
            hits[0].excerpt,
            "The quarterly invoice of the supplier is still unpaid."
        );
    }

    #[test]
    fn test_every_word_is_required_and_a_word_being_typed_already_matches() {
        // GIVEN a conversation about an invoice and one about an inventory
        let repo = repo();
        session(&repo, "s-invoice", None);
        session(&repo, "s-inventory", None);
        say(
            &repo,
            "s-invoice",
            ChatRole::User,
            "Send the invoice to Paris",
        );
        say(
            &repo,
            "s-inventory",
            ChatRole::User,
            "Count the inventory in Lyon",
        );

        // WHEN the start of a word, two words, and two words of two
        // different conversations are searched
        let prefix = found(&repo, "inv");
        let both_words = found(&repo, "paris invo");
        let split = found(&repo, "invoice lyon");

        // THEN the prefix finds both, the two words find the one that holds
        // both, and words no single message holds find nothing
        assert_eq!(prefix.len(), 2);
        assert_eq!(both_words, ["s-invoice"]);
        assert!(split.is_empty());
    }

    #[test]
    fn test_a_conversation_in_the_trash_is_not_found_until_it_is_restored() {
        // GIVEN two conversations that both speak of a budget
        let repo = repo();
        session(&repo, "s-kept", None);
        session(&repo, "s-trashed", None);
        say(&repo, "s-kept", ChatRole::User, "The budget of March");
        say(&repo, "s-trashed", ChatRole::User, "The budget of April");

        // WHEN one goes to the trash
        repo.trash_session("s-trashed", "2026-10-05T00:00:00Z")
            .expect("trash");

        // THEN the search answers the other one only
        assert_eq!(found(&repo, "budget"), ["s-kept"]);

        // WHEN it is restored
        repo.restore_session("s-trashed").expect("restore");

        // THEN it is found again
        let mut both = found(&repo, "budget");
        both.sort();
        assert_eq!(both, ["s-kept", "s-trashed"]);
    }

    #[test]
    fn test_a_conversation_is_answered_once_however_many_messages_match() {
        // GIVEN a conversation where twelve messages hold the word, and
        // another where one does
        let repo = repo();
        session(&repo, "s-many", None);
        session(&repo, "s-one", None);
        for n in 0..12 {
            say(
                &repo,
                "s-many",
                ChatRole::User,
                &format!("contract number {n}"),
            );
        }
        say(&repo, "s-one", ChatRole::User, "the contract is signed");

        // WHEN the word is searched
        let hits = found(&repo, "contract");

        // THEN each conversation is answered once
        assert_eq!(hits.len(), 2);
        assert!(hits.contains(&"s-many".to_owned()));
        assert!(hits.contains(&"s-one".to_owned()));
    }

    #[test]
    fn test_tool_results_and_system_notes_are_not_searched() {
        // GIVEN a conversation whose tool result and system note hold a word
        // no person and no answer wrote
        let repo = repo();
        session(&repo, "s-1", None);
        say(&repo, "s-1", ChatRole::User, "List my files");
        say(&repo, "s-1", ChatRole::Tool, "zanzibar.txt");
        say(&repo, "s-1", ChatRole::System, "zanzibar policy applied");

        // WHEN that word is searched, then a word the person wrote
        // THEN only the second finds the conversation
        assert!(found(&repo, "zanzibar").is_empty());
        assert_eq!(found(&repo, "files"), ["s-1"]);
    }

    #[test]
    fn test_a_query_of_punctuation_or_fts_syntax_answers_without_failing() {
        // GIVEN a conversation
        let repo = repo();
        session(&repo, "s-1", None);
        say(
            &repo,
            "s-1",
            ChatRole::User,
            "the well-known \"report\" (draft)",
        );

        // WHEN queries made of FTS5 syntax are searched
        // THEN none fails; the ones that hold a word find it
        assert!(found(&repo, "  ").is_empty());
        assert!(found(&repo, "\"*()-:^").is_empty());
        assert_eq!(found(&repo, "\"report\" AND (draft"), Vec::<String>::new());
        assert_eq!(found(&repo, "well-known"), ["s-1"]);
        assert_eq!(found(&repo, "(draft) \"report\""), ["s-1"]);
        assert_eq!(
            match_expression("o'neil  2026"),
            "\"o\"* \"neil\"* \"2026\"*"
        );
    }

    #[test]
    fn test_messages_removed_by_a_truncation_or_a_deletion_are_no_longer_found() {
        // GIVEN a conversation of three messages and another of one
        let repo = repo();
        session(&repo, "s-cut", None);
        session(&repo, "s-gone", None);
        say(&repo, "s-cut", ChatRole::User, "first about apples");
        let second = say(&repo, "s-cut", ChatRole::Assistant, "second about pears");
        say(&repo, "s-cut", ChatRole::User, "third about plums");
        say(&repo, "s-gone", ChatRole::User, "alone about apples");

        // WHEN the first is cut from its second message on, and the other is
        // deleted
        repo.truncate_messages_from_seq("s-cut", second, true)
            .expect("truncate");
        repo.delete_session("s-gone").expect("delete");

        // THEN only what remains is found
        assert_eq!(found(&repo, "apples"), ["s-cut"]);
        assert!(found(&repo, "pears").is_empty());
        assert!(found(&repo, "plums").is_empty());

        // AND a message written after the cut is found under its new place
        say(&repo, "s-cut", ChatRole::Assistant, "now about cherries");
        assert_eq!(found(&repo, "cherries"), ["s-cut"]);
    }

    #[test]
    fn test_a_fork_is_found_by_the_messages_it_copied() {
        // GIVEN a conversation of two messages
        let repo = repo();
        session(&repo, "s-parent", None);
        say(&repo, "s-parent", ChatRole::User, "plan the harvest");
        say(&repo, "s-parent", ChatRole::Assistant, "in september");

        // WHEN it is forked after its first message
        repo.create_fork_session("s-child", "s-parent", Some(1), "2026-10-02T00:00:00Z")
            .expect("fork");

        // THEN the fork is found by the message it copied and not by the one
        // it left behind
        let mut harvest = found(&repo, "harvest");
        harvest.sort();
        assert_eq!(harvest, ["s-child", "s-parent"]);
        assert_eq!(found(&repo, "september"), ["s-parent"]);
    }

    /// chat.db as the first shipped binary wrote it, with rows.
    const CHAT_V1_FIXTURE: &str = include_str!("../../../tests/fixtures/schemas/chat_v1.sql");

    #[test]
    fn test_messages_written_before_the_index_are_found_once_after_the_upgrade() {
        // GIVEN a chat.db written by the first shipped binary, holding the
        // messages "hello" and "hi there", and no index of the messages
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("chat.db");
        let seed = rusqlite::Connection::open(&path).expect("open raw");
        seed.execute_batch(CHAT_V1_FIXTURE).expect("seed v1");
        drop(seed);

        // WHEN the repository opens it, twice, the version stamp removed in
        // between so every migration step replays
        drop(ChatSessionRepository::open(&path).expect("first open"));
        let raw = rusqlite::Connection::open(&path).expect("open raw");
        raw.pragma_update(None, "user_version", 0).expect("unstamp");
        drop(raw);
        let repo = ChatSessionRepository::open(&path).expect("second open");

        // THEN the old messages are found, and each is indexed once
        assert_eq!(found(&repo, "hello"), ["legacy-session"]);
        assert_eq!(found(&repo, "there"), ["legacy-session"]);
        let indexed: i64 = repo
            .conn
            .query_row("SELECT COUNT(*) FROM chat_messages_fts", [], |r| r.get(0))
            .expect("count");
        assert_eq!(indexed, 2);
    }
}
