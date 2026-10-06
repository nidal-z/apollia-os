//! What each conversation weighs in the list: how many messages it holds and
//! how many tokens the model generated for it.
//!
//! The tokens are the sum of the totals written under each answer, the
//! `generation.output_tokens` of its `metadata` (see
//! [`crate::chat::generation`]). An answer written before those totals
//! existed, or whose metadata does not parse, counts for none.

use std::collections::HashMap;

use super::ChatSessionRepository;
use crate::chat::types::ChatError;

/// The totals of one conversation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionTotals {
    /// Messages the conversation holds, whatever their role.
    pub message_count: u32,
    /// Tokens generated across its answers.
    pub output_tokens: u64,
}

impl ChatSessionRepository {
    /// The totals of every conversation that holds a message, by session id.
    /// A conversation without a message is absent: its totals are zero.
    ///
    /// # Errors
    ///
    /// [`ChatError::InternalError`] when the messages cannot be read.
    pub fn session_totals(&self) -> Result<HashMap<String, SessionTotals>, ChatError> {
        let failed = |e: rusqlite::Error| ChatError::InternalError(format!("session_totals: {e}"));
        let mut stmt = self
            .conn
            .prepare(
                "SELECT session_id, COUNT(*),
                        COALESCE(SUM(CASE WHEN json_valid(metadata)
                            THEN CAST(json_extract(metadata, '$.generation.output_tokens') AS INTEGER)
                        END), 0)
                 FROM chat_messages GROUP BY session_id",
            )
            .map_err(failed)?;
        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let messages: i64 = row.get(1)?;
                let tokens: i64 = row.get(2)?;
                Ok((id, messages, tokens))
            })
            .map_err(failed)?;
        let mut totals = HashMap::new();
        for row in rows {
            let (id, messages, tokens) = row.map_err(failed)?;
            totals.insert(
                id,
                SessionTotals {
                    message_count: u32::try_from(messages).unwrap_or(u32::MAX),
                    output_tokens: u64::try_from(tokens).unwrap_or(0),
                },
            );
        }
        Ok(totals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::repository::AppendMessageParams;
    use crate::chat::types::{ChatMode, ChatRole};

    fn say(
        repo: &ChatSessionRepository,
        session: &str,
        n: u32,
        role: ChatRole,
        metadata: Option<&str>,
    ) {
        let id = format!("{session}-m{n}");
        repo.append_message(&AppendMessageParams {
            id: &id,
            session_id: session,
            role: &role,
            content: "text",
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-10-01T00:00:00Z",
            metadata,
        })
        .expect("message appended");
    }

    #[test]
    fn test_the_totals_of_a_conversation_sum_the_tokens_of_its_answers() {
        // GIVEN a conversation of two turns whose answers carry 120 and 80
        //       generated tokens, a conversation whose answer carries no
        //       total and one whose metadata does not parse, and an empty one
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        for id in ["two-turns", "untotalled", "empty"] {
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
        }
        say(&repo, "two-turns", 0, ChatRole::User, None);
        say(
            &repo,
            "two-turns",
            1,
            ChatRole::Assistant,
            Some(r#"{"generation":{"calls":2,"output_tokens":120,"stop_reason":"completed"}}"#),
        );
        say(&repo, "two-turns", 2, ChatRole::User, None);
        say(
            &repo,
            "two-turns",
            3,
            ChatRole::Assistant,
            Some(r#"{"generation":{"calls":1,"output_tokens":80,"stop_reason":"completed"}}"#),
        );
        say(
            &repo,
            "untotalled",
            0,
            ChatRole::User,
            Some(r#"{"edited":true}"#),
        );
        say(
            &repo,
            "untotalled",
            1,
            ChatRole::Assistant,
            Some("not json"),
        );

        // WHEN the totals are read
        let totals = repo.session_totals().expect("totals");

        // THEN the first holds four messages and 200 tokens, the second two
        //      messages and no token, and the empty one is absent
        assert_eq!(
            totals.get("two-turns"),
            Some(&SessionTotals {
                message_count: 4,
                output_tokens: 200
            })
        );
        assert_eq!(
            totals.get("untotalled"),
            Some(&SessionTotals {
                message_count: 2,
                output_tokens: 0
            })
        );
        assert_eq!(totals.get("empty"), None);
    }
}
