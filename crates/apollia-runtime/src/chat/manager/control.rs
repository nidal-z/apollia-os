use super::*;

impl ChatSessionManager {
    /// Returns the cooperative pause state of a session.
    ///
    /// `None` for an unknown session. A known session with no recorded state is
    /// [`PauseState::Running`] (the steady state).
    pub(in crate::chat::manager) fn pause_state(&self, session_id: &str) -> Option<PauseState> {
        if !self.sessions.contains_key(session_id) {
            return None;
        }
        Some(
            self.pause_states
                .get(session_id)
                .copied()
                .unwrap_or(PauseState::Running),
        )
    }

    /// Requests a cooperative pause of the active ReAct turn for `session_id`.
    ///
    /// Cancels the session's [`CancellationToken`]: the loop stops at its next
    /// checkpoint and the manager records [`PauseState::Paused`] once the turn
    /// reports back. When no turn is active (no token registered) this is a no-op
    /// returning `Ok(())`: no token is cancelled and the state stays Running.
    pub(in crate::chat::manager) fn handle_pause_session(
        &mut self,
        session_id: &str,
    ) -> Result<(), PauseError> {
        if !self.sessions.contains_key(session_id) {
            return Err(PauseError::UnknownSession {
                session_id: session_id.to_string(),
            });
        }
        match self.pause_tokens.get(session_id) {
            Some(token) => {
                token.cancel();
                self.pause_states
                    .insert(session_id.to_string(), PauseState::Pausing);
                tracing::info!(session_id = %session_id, "chat.session.pause_requested");
                Ok(())
            }
            None => {
                tracing::debug!(session_id = %session_id, "chat.session.pause_noop");
                Ok(())
            }
        }
    }

    /// Resumes a paused session by restarting a ReAct turn from the persisted
    /// plan state.
    ///
    /// A fresh token is attached by the dispatch path, the state returns to
    /// [`PauseState::Running`], and a continuation turn is dispatched. The
    /// [`StepBudget`] is rebuilt from the runtime ceiling per turn, exactly as for
    /// every chat exchange, so the safeguard is never disarmed (principle #7).
    ///
    /// This is distinct from
    /// [`ChatSessionManagerHandle::resume_session`], which reloads a session from
    /// SQLite; here the session is already in memory and only the loop is
    /// restarted.
    pub(in crate::chat::manager) fn handle_resume_paused_session(
        &mut self,
        session_id: &str,
    ) -> Result<(), PauseError> {
        if !self.sessions.contains_key(session_id) {
            return Err(PauseError::UnknownSession {
                session_id: session_id.to_string(),
            });
        }
        self.pause_states
            .insert(session_id.to_string(), PauseState::Running);
        tracing::info!(session_id = %session_id, "chat.session.resumed");

        // Resume is an ordinary continuation turn through the same actor path: the
        // dispatch installs a fresh token and consumes any queued injection. A
        // best-effort directive nudges the agent to keep executing the plan.
        self.dispatch_plan_continuation(session_id, PLAN_RESUME_DIRECTIVE);
        Ok(())
    }

    /// Injects a natural-language instruction into a paused session and resumes it.
    ///
    /// Validates the inputs (known session, non-empty text, session paused), then
    /// queues the instruction and triggers a resume turn. The agent reacts to the
    /// instruction first and adjusts the plan via the `plan_*` tools; the runtime
    /// stamps [`StepOrigin::UserInject`](apollia_core::plan::StepOrigin::UserInject)
    /// provenance on any step it creates. An instruction referencing an unknown
    /// step does not error here: the agent asks for clarification via `ask_user`.
    pub(in crate::chat::manager) fn handle_inject_instruction(
        &mut self,
        session_id: &str,
        text: &str,
    ) -> Result<(), InjectError> {
        if !self.sessions.contains_key(session_id) {
            return Err(InjectError::UnknownSession {
                session_id: session_id.to_string(),
            });
        }
        if text.trim().is_empty() {
            return Err(InjectError::EmptyInstruction);
        }
        if self.pause_state(session_id) != Some(PauseState::Paused) {
            return Err(InjectError::NotPaused {
                session_id: session_id.to_string(),
            });
        }

        self.pending_injections.insert(
            session_id.to_string(),
            InjectedInstruction {
                session_id: session_id.to_string(),
                text: text.to_string(),
            },
        );
        tracing::info!(
            session_id = %session_id,
            origin = "user_inject",
            "chat.session.instruction_injected"
        );

        // Resume consumes the queued instruction as the first user message of the
        // new turn (the resume mechanics own budget carry-over and a fresh token).
        self.handle_resume_paused_session(session_id)
            .map_err(|e| match e {
                PauseError::UnknownSession { session_id } => {
                    InjectError::UnknownSession { session_id }
                }
            })
    }

    /// Return a lightweight summary of the N most recent sessions.
    ///
    /// Calls the repository and logs on error, returning an empty vec on failure.
    pub(in crate::chat::manager) fn handle_get_recent_summaries(
        &self,
        limit: usize,
    ) -> Vec<RecentSessionSummary> {
        match self.repository.list_recent_summaries(limit) {
            Ok(summaries) => summaries,
            Err(e) => {
                error!(error = %e, "Failed to list recent session summaries from SQLite");
                Vec::new()
            }
        }
    }
}
