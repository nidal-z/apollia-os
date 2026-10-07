//! The actions on one message of a conversation, beside playing a turn again:
//! continue the last answer, remove a message, rewrite an answer in place.
//!
//! Continuing an answer runs the last turn again with the answer as the
//! prefill of the assistant message. It is offered only where the engine was
//! measured to continue that message; everywhere else it is refused with its
//! reason, and never imitated by a message asking the model to go on.

use serde::{Deserialize, Serialize};

use super::*;

/// Why the last answer of a conversation cannot be continued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationRefusal {
    /// The engine serving the model of the conversation was not measured to
    /// continue an answer: a model reached on a server, or the MLX engine.
    Engine,
    /// The conversation runs an agent that brings its own code, which builds
    /// its requests itself.
    Agent,
    /// No model is configured.
    NoModel,
}

impl ContinuationRefusal {
    /// The reason as the API spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Agent => "agent",
            Self::NoModel => "no_model",
        }
    }
}

/// Whether the last answer of a conversation can be continued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Continuation {
    /// `true` when the model of the conversation continues an answer.
    pub available: bool,
    /// Why not, when it does not.
    pub reason: Option<ContinuationRefusal>,
}

impl From<Option<ContinuationRefusal>> for Continuation {
    fn from(reason: Option<ContinuationRefusal>) -> Self {
        Self {
            available: reason.is_none(),
            reason,
        }
    }
}

/// One action on a message, as the actor takes it.
#[derive(Debug)]
pub enum AnswerAction {
    /// Read whether the last answer can be continued.
    Continuation,
    /// Continue the answer `message_id`, the last message of the session.
    Continue {
        /// The answer to continue.
        message_id: MessageId,
    },
    /// Remove `message_id` and, for an answer, the tool messages of its turn.
    Delete {
        /// The message to remove.
        message_id: MessageId,
    },
    /// Replace the text of the answer `message_id`.
    Edit {
        /// The answer to rewrite.
        message_id: MessageId,
        /// Its new text.
        content: String,
    },
}

/// What an [`AnswerAction`] answers.
#[derive(Debug, PartialEq, Eq)]
pub enum AnswerOutcome {
    /// The answer of [`AnswerAction::Continuation`].
    Continuation(Continuation),
    /// The turn is running again.
    Continuing,
    /// The number of messages removed.
    Deleted(u32),
    /// The answer reads its new text.
    Edited,
}

/// The text of a continued answer: what the engine returned when it starts
/// with the answer continued, as `llama-server` returns it, and the two joined
/// otherwise, so the answer never loses what it already said.
pub(in crate::chat::manager) fn continued_text(before: &str, returned: &str) -> String {
    if returned.starts_with(before) {
        returned.to_string()
    } else {
        format!("{before}{returned}")
    }
}

impl ChatSessionManager {
    /// Serve one action on a message.
    pub(in crate::chat::manager) fn handle_answer_action(
        &mut self,
        session_id: &str,
        action: AnswerAction,
    ) -> Result<AnswerOutcome, ChatError> {
        match action {
            AnswerAction::Continuation => self
                .continuation_refusal(session_id)
                .map(|refusal| AnswerOutcome::Continuation(refusal.into())),
            AnswerAction::Continue { message_id } => self
                .handle_continue_answer(session_id, &message_id)
                .map(|()| AnswerOutcome::Continuing),
            AnswerAction::Delete { message_id } => self
                .handle_delete_message(session_id, &message_id)
                .map(AnswerOutcome::Deleted),
            AnswerAction::Edit {
                message_id,
                content,
            } => self
                .handle_edit_answer(session_id, &message_id, &content)
                .map(|()| AnswerOutcome::Edited),
        }
    }

    /// Why the model the next turn of `session_id` runs on cannot continue an
    /// answer, `None` when it can. The model is resolved as the dispatch of a
    /// message resolves it.
    fn continuation_refusal(
        &self,
        session_id: &str,
    ) -> Result<Option<ContinuationRefusal>, ChatError> {
        let model = self.handle_session_model(session_id)?;
        let runs_code = match self.sessions.get(session_id) {
            Some(session) => {
                session.mode == ChatMode::Agent && self.declarative_session_agent(session).is_none()
            }
            None => self.repository.get_session(session_id)?.is_some_and(|row| {
                row.mode == ChatMode::Agent.as_sql()
                    && row
                        .agent_name
                        .as_deref()
                        .and_then(|name| {
                            super::declarative::declarative_agent(
                                self.governance_db_path.as_deref(),
                                name,
                            )
                        })
                        .is_none()
            }),
        };
        if runs_code {
            return Ok(Some(ContinuationRefusal::Agent));
        }
        let Some(router) = self.llm_router.as_ref() else {
            return Ok(Some(ContinuationRefusal::NoModel));
        };
        let continues = router.route(model.as_deref()).continues_answer();
        Ok((!continues).then_some(ContinuationRefusal::Engine))
    }

    /// The session `session_id`, in memory and free to be changed: neither
    /// closed nor in the middle of a turn.
    fn idle_session(&self, session_id: &str) -> Result<&ChatSession, ChatError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
        match session.status {
            SessionStatus::Closed => Err(ChatError::SessionClosed(session_id.to_string())),
            SessionStatus::Processing => Err(ChatError::SessionBusy(session_id.to_string())),
            _ => Ok(session),
        }
    }

    /// Continue the last answer: the turn it answered runs again, on the
    /// history before it, with the answer as the prefill. The answer stays in
    /// place until the exchange lands, so a failed continuation loses nothing.
    fn handle_continue_answer(
        &mut self,
        session_id: &str,
        message_id: &str,
    ) -> Result<(), ChatError> {
        let (user_id, user_content, answer) = {
            let session = self.idle_session(session_id)?;
            let (last, earlier) = session
                .history
                .split_last()
                .ok_or_else(|| ChatError::MessageNotFound(message_id.to_string()))?;
            if !session.history.iter().any(|m| m.id == message_id) {
                return Err(ChatError::MessageNotFound(message_id.to_string()));
            }
            if last.id != message_id || last.role != ChatRole::Assistant {
                return Err(ChatError::MessageRefused(
                    "only the last answer of a conversation is continued".into(),
                ));
            }
            if last.content.trim().is_empty() {
                return Err(ChatError::MessageRefused(
                    "an empty answer has nothing to continue".into(),
                ));
            }
            let user = earlier
                .last()
                .filter(|m| m.role == ChatRole::User)
                .ok_or_else(|| {
                    ChatError::MessageRefused("the answer does not follow a user message".into())
                })?;
            (user.id.clone(), user.content.clone(), last.content.clone())
        };
        if let Some(refusal) = self.continuation_refusal(session_id)? {
            return Err(ChatError::ContinuationRefused(refusal.as_str().into()));
        }

        self.answer_prefills.insert(session_id.to_string(), answer);
        let started = self.rerun_turn(session_id, &user_id, &user_content, message_id.to_string());
        if started.is_err() {
            self.answer_prefills.remove(session_id);
        }
        started
    }

    /// Remove a message. An answer takes the tool messages of its turn with
    /// it; the summary of the session, written from the messages as they
    /// were, is forgotten.
    fn handle_delete_message(
        &mut self,
        session_id: &str,
        message_id: &str,
    ) -> Result<u32, ChatError> {
        let seqs: Vec<u32> = {
            let session = self.idle_session(session_id)?;
            let at = session
                .history
                .iter()
                .position(|m| m.id == message_id)
                .ok_or_else(|| ChatError::MessageNotFound(message_id.to_string()))?;
            let target = &session.history[at];
            let mut seqs = vec![target.seq];
            if target.role == ChatRole::Assistant {
                // The turn runs from the user message before the answer to the
                // one after it; its tool messages go with the answer.
                let start = session.history[..at]
                    .iter()
                    .rposition(|m| m.role == ChatRole::User)
                    .map_or(0, |user| user + 1);
                let end = session.history[at..]
                    .iter()
                    .position(|m| m.role == ChatRole::User)
                    .map_or(session.history.len(), |next| at + next);
                seqs.extend(
                    session.history[start..end]
                        .iter()
                        .filter(|m| m.role == ChatRole::Tool)
                        .map(|m| m.seq),
                );
            }
            seqs
        };

        let deleted = self.repository.delete_messages(session_id, &seqs)?;
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.history.retain(|m| !seqs.contains(&m.seq));
        }
        info!(
            session_id = %session_id,
            message_id = %message_id,
            deleted,
            "chat.message.deleted"
        );
        Ok(deleted)
    }

    /// Replace the text of an answer in the history and mark it edited. The
    /// next turn reads the new text; no turn is played again.
    fn handle_edit_answer(
        &mut self,
        session_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<(), ChatError> {
        if content.trim().is_empty() {
            return Err(ChatError::MessageRefused(
                "an answer cannot be left empty".into(),
            ));
        }
        let metadata = {
            let session = self.idle_session(session_id)?;
            let target = session
                .history
                .iter()
                .find(|m| m.id == message_id)
                .ok_or_else(|| ChatError::MessageNotFound(message_id.to_string()))?;
            if target.role != ChatRole::Assistant {
                return Err(ChatError::MessageRefused(
                    "only an answer is rewritten in place".into(),
                ));
            }
            let mut metadata = match target.metadata.clone() {
                Some(serde_json::Value::Object(fields)) => fields,
                _ => serde_json::Map::new(),
            };
            metadata.insert("edited".into(), serde_json::Value::Bool(true));
            serde_json::Value::Object(metadata)
        };

        self.repository.rewrite_message(
            session_id,
            message_id,
            crate::chat::repository::MessageRewrite {
                content,
                tool_calls_json: None,
                metadata: Some(&metadata.to_string()),
                drop_summary: true,
            },
        )?;
        if let Some(message) = self
            .sessions
            .get_mut(session_id)
            .and_then(|s| s.history.iter_mut().find(|m| m.id == message_id))
        {
            message.content = content.to_string();
            message.metadata = Some(metadata);
        }
        info!(session_id = %session_id, message_id = %message_id, "chat.answer.edited");
        Ok(())
    }
}

impl ChatSessionManagerHandle {
    async fn answer_action(
        &self,
        session_id: SessionId,
        action: AnswerAction,
    ) -> Result<AnswerOutcome, ChatError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(ChatCommand::Answer {
                session_id,
                action,
                reply,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;
        answer
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Whether the last answer of `session_id` can be continued on the model
    /// its next turn runs on, and why not.
    ///
    /// # Errors
    ///
    /// [`ChatError::SessionNotFound`] for an unknown session.
    pub async fn continuation(&self, session_id: SessionId) -> Result<Continuation, ChatError> {
        match self
            .answer_action(session_id, AnswerAction::Continuation)
            .await?
        {
            AnswerOutcome::Continuation(found) => Ok(found),
            other => Err(unexpected(&other)),
        }
    }

    /// Continue `message_id`, the last answer of `session_id`: the turn runs
    /// again with that answer as the prefill, and the answer that lands
    /// replaces it under the same id.
    ///
    /// # Errors
    ///
    /// [`ChatError::ContinuationRefused`] when the model does not continue an
    /// answer, [`ChatError::MessageRefused`] when `message_id` is not the last
    /// answer, [`ChatError::SessionBusy`] while a turn is in flight.
    pub async fn continue_answer(
        &self,
        session_id: SessionId,
        message_id: MessageId,
    ) -> Result<(), ChatError> {
        self.answer_action(session_id, AnswerAction::Continue { message_id })
            .await
            .map(|_| ())
    }

    /// Remove `message_id` and, for an answer, the tool messages of its turn.
    /// Returns the number of messages removed.
    ///
    /// # Errors
    ///
    /// [`ChatError::MessageNotFound`] when the session holds no such message,
    /// [`ChatError::SessionBusy`] while a turn is in flight.
    pub async fn delete_message(
        &self,
        session_id: SessionId,
        message_id: MessageId,
    ) -> Result<u32, ChatError> {
        match self
            .answer_action(session_id, AnswerAction::Delete { message_id })
            .await?
        {
            AnswerOutcome::Deleted(count) => Ok(count),
            other => Err(unexpected(&other)),
        }
    }

    /// Replace the text of the answer `message_id` with `content`, marked
    /// edited. No turn is played again.
    ///
    /// # Errors
    ///
    /// [`ChatError::MessageRefused`] for a user message or an empty text,
    /// [`ChatError::MessageNotFound`], [`ChatError::SessionBusy`].
    pub async fn edit_answer(
        &self,
        session_id: SessionId,
        message_id: MessageId,
        content: String,
    ) -> Result<(), ChatError> {
        self.answer_action(
            session_id,
            AnswerAction::Edit {
                message_id,
                content,
            },
        )
        .await
        .map(|_| ())
    }
}

fn unexpected(outcome: &AnswerOutcome) -> ChatError {
    ChatError::InternalError(format!(
        "unexpected answer to a message action: {outcome:?}"
    ))
}
