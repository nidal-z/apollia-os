use super::*;

impl ChatSessionManagerHandle {
    /// Close a session.
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::CloseSession {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Delete a session and all its data.
    pub async fn delete_session(&self, session_id: SessionId) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::DeleteSession {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Rename a session (set a user-defined title).
    pub async fn rename_session(
        &self,
        session_id: SessionId,
        title: String,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::RenameSession {
                session_id,
                title,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Enable or disable plan mode for a session.
    ///
    /// Enabling initializes the phase to `Discovery`; disabling resets it to a
    /// neutral `Done` so the session never stays stuck awaiting approval.
    ///
    /// # Errors
    ///
    /// Returns [`ChatError::SessionNotFound`] when no session matches
    /// `session_id`, or [`ChatError::InternalError`] when the actor is gone.
    pub async fn set_plan_mode(
        &self,
        session_id: SessionId,
        enabled: bool,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::SetPlanMode {
                session_id,
                enabled,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Approve the plan awaiting approval for a session and resume execution.
    ///
    /// Moves the session to [`PlanPhase::Executing`], emits
    /// [`RuntimeEvent::ChatPlanApproved`], and dispatches a continuation turn so
    /// the agent executes the approved plan.
    ///
    /// # Errors
    ///
    /// - [`ChatError::SessionNotFound`] when no session matches `session_id`.
    /// - [`ChatError::NotAwaitingApproval`] when the session is not awaiting
    ///   approval.
    /// - [`ChatError::InternalError`] when the actor is gone.
    pub async fn approve_plan(&self, session_id: &str) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ApprovePlan {
                session_id: session_id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Reject the plan awaiting approval for a session and trigger a revision.
    ///
    /// Emits [`RuntimeEvent::ChatPlanRejected`] with the optional reason and
    /// dispatches a revision turn. The session stays in
    /// [`PlanPhase::AwaitingApproval`]: the gate is soft and execution never
    /// starts on a rejection.
    ///
    /// # Errors
    ///
    /// - [`ChatError::SessionNotFound`] when no session matches `session_id`.
    /// - [`ChatError::NotAwaitingApproval`] when the session is not awaiting
    ///   approval.
    /// - [`ChatError::InternalError`] when the actor is gone.
    pub async fn reject_plan(
        &self,
        session_id: &str,
        reason: Option<String>,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::RejectPlan {
                session_id: session_id.to_string(),
                reason,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Cooperatively pause the active ReAct turn for a session.
    ///
    /// The loop stops at its next checkpoint with partial step statuses already
    /// persisted. Pausing a session with no active turn is a no-op returning
    /// `Ok(())`.
    ///
    /// # Errors
    ///
    /// - [`PauseError::UnknownSession`] when no session matches `session_id`.
    pub async fn pause_session(&self, session_id: &str) -> Result<(), PauseError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::PauseSession {
                session_id: session_id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| PauseError::UnknownSession {
                session_id: session_id.to_string(),
            })?;
        reply_rx.await.map_err(|_| PauseError::UnknownSession {
            session_id: session_id.to_string(),
        })?
    }

    /// Resume a paused session, restarting the ReAct loop from the persisted plan
    /// state.
    ///
    /// A fresh cooperative token is attached and a continuation turn is dispatched.
    /// The step budget is rebuilt per turn from the runtime ceiling, so the
    /// safeguard is never disarmed.
    ///
    /// This is distinct from [`Self::resume_session`], which reloads a session
    /// from SQLite. Here the session is already in memory and only the loop is
    /// restarted.
    ///
    /// # Errors
    ///
    /// - [`PauseError::UnknownSession`] when no session matches `session_id`.
    pub async fn resume_paused_session(&self, session_id: &str) -> Result<(), PauseError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ResumePausedSession {
                session_id: session_id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| PauseError::UnknownSession {
                session_id: session_id.to_string(),
            })?;
        reply_rx.await.map_err(|_| PauseError::UnknownSession {
            session_id: session_id.to_string(),
        })?
    }

    /// Inject a natural-language instruction into a paused session and resume it.
    ///
    /// The instruction is queued as the next user message; on resume the agent
    /// adjusts the plan via the `plan_*` tools, with any created step stamped
    /// [`StepOrigin::UserInject`](apollia_core::plan::StepOrigin::UserInject).
    ///
    /// # Errors
    ///
    /// - [`InjectError::UnknownSession`] when no session matches `session_id`.
    /// - [`InjectError::EmptyInstruction`] when the text is empty or whitespace.
    /// - [`InjectError::NotPaused`] when the session is not paused.
    pub async fn inject_instruction(
        &self,
        session_id: &str,
        text: &str,
    ) -> Result<(), InjectError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::InjectInstruction {
                session_id: session_id.to_string(),
                text: text.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| InjectError::UnknownSession {
                session_id: session_id.to_string(),
            })?;
        reply_rx.await.map_err(|_| InjectError::UnknownSession {
            session_id: session_id.to_string(),
        })?
    }

    /// Read the cooperative pause state of a session.
    ///
    /// Returns `None` for an unknown session; a known session with no recorded
    /// state is [`PauseState::Running`].
    pub async fn pause_state(&self, session_id: &str) -> Option<PauseState> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::GetPauseState {
                session_id: session_id.to_string(),
                reply: reply_tx,
            })
            .await
            .ok()?;
        reply_rx.await.ok().flatten()
    }

    /// Signal the actor to shut down.
    /// Hot-reload the LLM router used by the chat subsystem.
    ///
    /// Called after the user configures a new LLM backend (e.g. during
    /// onboarding). The new router is used for all subsequent requests.
    pub async fn reload_llm(&self, router: Option<Arc<LlmRouter>>) {
        let _ = self.tx.send(ChatCommand::ReloadLlm { router }).await;
    }

    /// List the N most recent sessions with their first user message.
    ///
    /// Returns an empty vec if the actor is unreachable or the query fails.
    pub async fn list_recent_summaries(&self, limit: usize) -> Vec<RecentSessionSummary> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::GetRecentSummaries {
                limit,
                reply: reply_tx,
            })
            .await;

        if sent.is_err() {
            return Vec::new();
        }

        reply_rx.await.unwrap_or_default()
    }

    /// Load a session from SQLite (if not already in memory) and return its full detail.
    ///
    /// Resets `Processing` status to `Active` so the session can immediately accept
    /// new messages. Returns `Err(ChatError::SessionNotFound)` if the ID is unknown.
    pub async fn resume_session(&self, session_id: SessionId) -> Result<SessionDetail, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ResumeSession {
                session_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Fork an existing session, producing a new child session.
    ///
    /// `up_to_index` controls how many messages are copied: `None` copies
    /// the full history, `Some(n)` copies the first `n` messages.
    pub async fn fork_session(
        &self,
        session_id: SessionId,
        up_to_index: Option<usize>,
    ) -> Result<SessionInfo, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ForkSession {
                session_id,
                up_to_index,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Regenerate the assistant reply to the last user turn (truncate-in-place).
    ///
    /// `message_id` is the assistant message to regenerate: it and every later
    /// message are dropped from SQLite and memory, then the preceding user turn
    /// is replayed in the same session. Fails with [`ChatError::SessionBusy`]
    /// when a turn is already in flight.
    pub async fn regenerate_response(
        &self,
        session_id: SessionId,
        message_id: MessageId,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::RegenerateResponse {
                session_id,
                message_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Replace a user message and re-run from it (truncate-in-place).
    ///
    /// `message_id` is the user message to edit: it and every later message are
    /// dropped, then `content` is sent as a fresh user turn. Returns the new user
    /// message id. Fails with [`ChatError::SessionBusy`] when a turn is in flight.
    pub async fn edit_and_resend(
        &self,
        session_id: SessionId,
        message_id: MessageId,
        content: String,
    ) -> Result<MessageId, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::EditAndResend {
                session_id,
                message_id,
                content,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// List all direct child sessions (forks) of the given parent.
    ///
    /// Returns an empty vec if the actor is unreachable or the query fails.
    pub async fn list_children(&self, session_id: SessionId) -> Vec<SessionInfo> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::ListChildren {
                session_id,
                reply: reply_tx,
            })
            .await;

        if sent.is_err() {
            return Vec::new();
        }

        reply_rx.await.unwrap_or_default()
    }

    /// List all A2A skills available from active worker agents.
    ///
    /// Returns an empty vec when A2A is not wired or the actor is unreachable.
    /// Link or unlink a session to a project.
    pub async fn link_session_to_project(
        &self,
        session_id: SessionId,
        project_id: Option<String>,
    ) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::LinkSessionToProject {
                session_id,
                project_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;

        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// List sessions belonging to a specific project.
    pub async fn list_sessions_by_project(&self, project_id: String) -> Vec<SessionInfo> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::ListSessionsByProject {
                project_id,
                reply: reply_tx,
            })
            .await;

        if sent.is_err() {
            return Vec::new();
        }

        reply_rx.await.unwrap_or_default()
    }

    /// Orphan all sessions linked to a project (called on project deletion).
    ///
    /// Fire and forget: the caller has already deleted the project and must
    /// not fail on this. A dropped command is traced here, a failed database
    /// write is traced by the actor.
    pub async fn orphan_project_sessions(&self, project_id: String) {
        let sent = self
            .tx
            .send(ChatCommand::OrphanProjectSessions {
                project_id: project_id.clone(),
            })
            .await;

        if sent.is_err() {
            warn!(
                project_id = %project_id,
                cause = "chat actor channel closed",
                "chat.orphan_project_sessions.dropped"
            );
        }
    }

    pub async fn list_a2a_skills(&self) -> Vec<crate::a2a::SkillListing> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::ListA2ASkills { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// List aggregated A2A skill telemetry.
    pub async fn list_a2a_skill_telemetry(&self) -> Vec<crate::a2a::A2ASkillTelemetry> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::ListA2ASkillTelemetry { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// List A2A step provenance entries, optionally filtered by skill id.
    pub async fn list_a2a_step_provenance(
        &self,
        skill_id: Option<String>,
    ) -> Vec<crate::a2a::A2AStepProvenance> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::ListA2AStepProvenance {
                skill_id,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Check compatibility of a skill against a required semver version.
    pub async fn check_a2a_compatibility(
        &self,
        skill_id: String,
        required_version: String,
    ) -> Option<crate::a2a::A2ACompatibilityWarning> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(ChatCommand::CheckA2ACompatibility {
                skill_id,
                required_version,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return None;
        }
        reply_rx.await.unwrap_or(None)
    }

    /// List recently resolved chat tool approvals from the approval log.
    pub async fn list_approval_history(
        &self,
        limit: i64,
        days: i64,
    ) -> Result<Vec<super::super::repository::ChatApprovalLogRow>, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::ListApprovalHistory {
                limit,
                days,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor unavailable".into()))?;
        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }

    /// Fetch aggregated metrics for a session.
    ///
    /// Returns `None` when the session is unknown or no exchange has completed
    /// yet. Accumulated in-memory from each [`ChatAgentResponse`], cleared on
    /// actor restart.
    pub async fn get_session_metrics(&self, session_id: SessionId) -> Option<SessionMetrics> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(ChatCommand::GetSessionMetrics {
                session_id,
                reply: reply_tx,
            })
            .await;
        if sent.is_err() {
            return None;
        }
        reply_rx.await.ok().flatten()
    }

    pub async fn shutdown(&self) {
        let _ = self.tx.send(ChatCommand::Shutdown).await;
    }
}
