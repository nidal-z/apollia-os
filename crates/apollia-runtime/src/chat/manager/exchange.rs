use super::*;

mod dispatch;
mod outcome;
mod tool_decision;

impl ChatSessionManager {
    /// Send a user message in a session.
    pub(in crate::chat::manager) fn handle_send_message(
        &mut self,
        session_id: &str,
        content: &str,
    ) -> Result<MessageId, ChatError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        if session.status == SessionStatus::Closed {
            return Err(ChatError::SessionClosed(session_id.to_string()));
        }

        if session.status == SessionStatus::Processing {
            return Err(ChatError::SessionBusy(session_id.to_string()));
        }

        let message_id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();

        // Persist message to SQLite
        let seq = self.repository.append_message(&AppendMessageParams {
            id: &message_id,
            session_id,
            role: &ChatRole::User,
            content,
            tool_calls_json: None,
            tool_name: None,
            created_at: &now,
            metadata: None,
        })?;

        // Add to in-memory history
        let msg = ChatMessage {
            id: message_id.clone(),
            role: ChatRole::User,
            content: content.to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: now.clone(),
            seq,
            metadata: None,
        };
        session.history.push(msg);

        // Set session to Processing. A fresh run_id correlates every event
        // emitted during this exchange (one user turn, one response cycle).
        session.status = SessionStatus::Processing;
        let run_id = RunId::new();
        session.active_exchange = Some(ExchangeState {
            message_id: message_id.clone(),
            started_at: now,
            run_id: run_id.clone(),
        });
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Processing)
        {
            warn!(error = %e, "chat.session.status.persist.failed");
        }

        // Emit event
        let _ = self.event_bus.send(RuntimeEvent::ChatMessageSent {
            session_id: session_id.to_string(),
            message_id: message_id.clone(),
        });

        // Launch BuiltInChatAgent in a background task for Libre mode.
        // For Agent mode, a different path will be used.
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;

        // The agent path clones the full session, which already carries `run_id`
        // in `active_exchange`; only the Libre path needs it threaded explicitly.
        let dispatch = if session.mode == ChatMode::Libre || session.mode == ChatMode::Companion {
            self.dispatch_libre_exchange(session_id, &message_id, content, &run_id)
        } else {
            self.dispatch_agent_exchange(session_id, &message_id, content)
        };

        // A dispatch failure (e.g. no LLM configured) must not leave the session
        // stuck in Processing forever: reset it to Active, clear the exchange and
        // persist before surfacing the error, so the next send is accepted.
        if let Err(e) = dispatch {
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.status = SessionStatus::Active;
                session.active_exchange = None;
            }
            if let Err(persist_err) = self
                .repository
                .update_status(session_id, &SessionStatus::Active)
            {
                warn!(error = %persist_err, "chat.session.status.reset.failed");
            }
            return Err(e);
        }

        Ok(message_id)
    }

    /// Regenerate the assistant reply to the last user turn (truncate-in-place).
    ///
    /// Locates the user turn that `message_id` (an assistant message) answered,
    /// drops that user turn's reply and everything after it from both SQLite and
    /// the in-memory history, then replays the turn on the shortened history. The
    /// session id is unchanged (ChatGPT/Claude style), not forked.
    pub(in crate::chat::manager) fn handle_regenerate_response(
        &mut self,
        session_id: &str,
        message_id: &str,
    ) -> Result<(), ChatError> {
        let (user_id, user_seq, user_content) = {
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
            if session.status == SessionStatus::Closed {
                return Err(ChatError::SessionClosed(session_id.to_string()));
            }
            if session.status == SessionStatus::Processing {
                return Err(ChatError::SessionBusy(session_id.to_string()));
            }
            let target_idx = session
                .history
                .iter()
                .position(|m| m.id == message_id)
                .ok_or_else(|| {
                    ChatError::InternalError(format!("message not found: {message_id}"))
                })?;
            let user_msg = session.history[..target_idx]
                .iter()
                .rev()
                .find(|m| m.role == ChatRole::User)
                .ok_or_else(|| {
                    ChatError::InternalError("no user turn to regenerate".to_string())
                })?;
            (user_msg.id.clone(), user_msg.seq, user_msg.content.clone())
        };

        // Delete everything after the user turn (its reply + any later messages),
        // keeping the user message itself as the trailing prompt.
        self.repository
            .truncate_messages_from_seq(session_id, user_seq, false)?;
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.history.retain(|m| m.seq <= user_seq);
        }

        self.rerun_turn(session_id, &user_id, &user_content)
    }

    /// Replace a user message and re-run from it (truncate-in-place).
    ///
    /// Truncates the edited user message and everything after it, then sends
    /// `content` as a fresh user turn through the normal send path.
    pub(in crate::chat::manager) fn handle_edit_and_resend(
        &mut self,
        session_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<MessageId, ChatError> {
        let seq = {
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
            if session.status == SessionStatus::Closed {
                return Err(ChatError::SessionClosed(session_id.to_string()));
            }
            if session.status == SessionStatus::Processing {
                return Err(ChatError::SessionBusy(session_id.to_string()));
            }
            let msg = session
                .history
                .iter()
                .find(|m| m.id == message_id)
                .ok_or_else(|| {
                    ChatError::InternalError(format!("message not found: {message_id}"))
                })?;
            if msg.role != ChatRole::User {
                return Err(ChatError::InternalError(
                    "can only edit a user message".to_string(),
                ));
            }
            msg.seq
        };

        // Delete the edited message and everything after it, then re-send.
        self.repository
            .truncate_messages_from_seq(session_id, seq, true)?;
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.history.retain(|m| m.seq < seq);
        }

        self.handle_send_message(session_id, content)
    }

    /// Re-run a turn on the existing history without appending a new user
    /// message.
    ///
    /// Shared tail of [`handle_regenerate_response`](Self::handle_regenerate_response):
    /// the caller has already truncated the history so it ends with
    /// `user_message_id`. Mirrors the dispatch half of
    /// [`handle_send_message`](Self::handle_send_message) (fresh run_id,
    /// Processing status, fresh pause token created inside the dispatch) but
    /// reuses the already-persisted user turn as the prompt.
    fn rerun_turn(
        &mut self,
        session_id: &str,
        user_message_id: &str,
        user_content: &str,
    ) -> Result<(), ChatError> {
        let now = now_rfc3339();
        let run_id = RunId::new();
        {
            let session = self
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
            session.status = SessionStatus::Processing;
            session.active_exchange = Some(ExchangeState {
                message_id: user_message_id.to_string(),
                started_at: now,
                run_id: run_id.clone(),
            });
        }
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Processing)
        {
            warn!(error = %e, "chat.session.status.persist.failed");
        }

        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;
        let dispatch = if session.mode == ChatMode::Libre || session.mode == ChatMode::Companion {
            self.dispatch_libre_exchange(session_id, user_message_id, user_content, &run_id)
        } else {
            self.dispatch_agent_exchange(session_id, user_message_id, user_content)
        };

        if let Err(e) = dispatch {
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.status = SessionStatus::Active;
                session.active_exchange = None;
            }
            if let Err(persist_err) = self
                .repository
                .update_status(session_id, &SessionStatus::Active)
            {
                warn!(error = %persist_err, "chat.session.status.reset.failed");
            }
            return Err(e);
        }

        Ok(())
    }
}
