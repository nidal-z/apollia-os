use super::*;

impl ChatSessionManager {
    /// Enable or disable plan mode for a session and persist the change.
    ///
    /// Enabling sets the phase to [`PlanPhase::Discovery`]; disabling sets it to
    /// [`PlanPhase::Done`] so the session never stays stuck awaiting approval.
    /// Persistence happens first, so a missing session surfaces
    /// [`ChatError::SessionNotFound`] before any in-memory state is touched.
    pub(in crate::chat::manager) fn handle_set_plan_mode(
        &mut self,
        session_id: &str,
        enabled: bool,
    ) -> Result<(), ChatError> {
        let phase = if enabled {
            PlanPhase::Discovery
        } else {
            PlanPhase::Done
        };

        // Persist first: this fails fast with SessionNotFound when no row matches.
        self.repository.set_plan_mode(session_id, enabled, phase)?;

        // Mirror the change into the in-memory cache when the session is loaded.
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.plan_mode = enabled;
            session.plan_phase = phase;
        }

        // Surface the phase transition so the desktop tracks it without waiting
        // for the next agent turn (which is the only other emitter).
        let _ = self.event_bus.send(RuntimeEvent::ChatPlanPhaseChanged {
            session_id: session_id.to_string(),
            phase: phase.as_sql().to_string(),
        });

        tracing::info!(
            session_id = %session_id,
            plan_mode = enabled,
            plan_phase = %phase.as_sql(),
            "plan.mode.toggled"
        );
        Ok(())
    }

    /// Approve the plan awaiting approval and resume execution (soft gate).
    ///
    /// The session must be in [`PlanPhase::AwaitingApproval`], otherwise this
    /// fails fast with [`ChatError::NotAwaitingApproval`] (principle: fail fast),
    /// so execution never starts out of order. On success the phase moves to
    /// [`PlanPhase::Executing`] (persisted), [`RuntimeEvent::ChatPlanApproved`]
    /// is emitted, and a continuation turn is dispatched so the agent executes
    /// the steps. The resume is a normal exchange routed through this actor's own
    /// `ChatCommand` path, never a blocking await or a shared-state hand-off.
    ///
    /// # Errors
    ///
    /// - [`ChatError::SessionNotFound`] when no session matches `session_id`.
    /// - [`ChatError::NotAwaitingApproval`] when the session is not awaiting
    ///   approval.
    pub(in crate::chat::manager) fn handle_approve_plan(
        &mut self,
        session_id: &str,
    ) -> Result<(), ChatError> {
        self.guard_awaiting_approval(session_id)?;

        // Persist first so the approved transition survives a restart even if the
        // continuation dispatch below fails (e.g. no LLM configured).
        self.repository
            .set_plan_phase(session_id, PlanPhase::Executing)?;
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.plan_phase = PlanPhase::Executing;
        }

        let _ = self.event_bus.send(RuntimeEvent::ChatPlanApproved {
            session_id: session_id.to_string(),
        });
        // Also surface the phase move so a listener tracking phase transitions
        // sees Executing, not only the distinct approval event.
        let _ = self.event_bus.send(RuntimeEvent::ChatPlanPhaseChanged {
            session_id: session_id.to_string(),
            phase: PlanPhase::Executing.as_sql().to_string(),
        });
        tracing::info!(session_id = %session_id, decision = "approve", "plan.action");

        // Resume: a synthetic directive turn drives the agent to execute the
        // approved plan, keeping step statuses current as it goes. Best-effort:
        // a dispatch failure is logged but does not fail the approval, which has
        // already been recorded and emitted.
        self.dispatch_plan_continuation(session_id, PLAN_EXECUTE_DIRECTIVE);
        Ok(())
    }

    /// Reject the plan awaiting approval and let the agent revise it (soft gate).
    ///
    /// The session must be in [`PlanPhase::AwaitingApproval`], otherwise this
    /// fails fast with [`ChatError::NotAwaitingApproval`]. On success
    /// [`RuntimeEvent::ChatPlanRejected`] is emitted with the optional reason and
    /// a revision turn is dispatched so the agent adjusts the plan through the
    /// `plan_*` tools. The phase stays [`PlanPhase::AwaitingApproval`]: the gate
    /// is soft, the session never moves to execution on a rejection.
    ///
    /// # Errors
    ///
    /// - [`ChatError::SessionNotFound`] when no session matches `session_id`.
    /// - [`ChatError::NotAwaitingApproval`] when the session is not awaiting
    ///   approval.
    pub(in crate::chat::manager) fn handle_reject_plan(
        &mut self,
        session_id: &str,
        reason: Option<String>,
    ) -> Result<(), ChatError> {
        self.guard_awaiting_approval(session_id)?;

        let _ = self.event_bus.send(RuntimeEvent::ChatPlanRejected {
            session_id: session_id.to_string(),
            reason: reason.clone(),
        });
        tracing::info!(session_id = %session_id, decision = "reject", "plan.action");

        // Resume into a revision turn. The directive carries the operator reason
        // so the agent can take it into account; the phase stays AwaitingApproval
        // so the revised plan is re-submitted into the same soft gate.
        let directive = match reason {
            Some(r) if !r.trim().is_empty() => {
                format!("{PLAN_REVISE_DIRECTIVE}\n\nOperator feedback: {r}")
            }
            _ => PLAN_REVISE_DIRECTIVE.to_string(),
        };
        self.dispatch_plan_continuation(session_id, &directive);
        Ok(())
    }

    /// Return [`ChatError::NotAwaitingApproval`] unless the session is awaiting
    /// approval, or [`ChatError::SessionNotFound`] when it does not exist.
    fn guard_awaiting_approval(&self, session_id: &str) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
        if session.plan_phase != PlanPhase::AwaitingApproval {
            return Err(ChatError::NotAwaitingApproval {
                session_id: session_id.to_string(),
                current_phase: session.plan_phase.as_sql().to_string(),
            });
        }
        Ok(())
    }

    /// Dispatch a continuation turn carrying a synthetic directive.
    ///
    /// Reuses [`handle_send_message`](Self::handle_send_message) so the resume is
    /// an ordinary exchange through the same actor path: one user turn, one
    /// response cycle, the existing bounded `ChatCommand` channel. Best-effort:
    /// a busy or unreachable session is logged, not surfaced, because the gate
    /// decision it follows has already been recorded.
    pub(in crate::chat::manager) fn dispatch_plan_continuation(
        &mut self,
        session_id: &str,
        directive: &str,
    ) {
        if let Err(e) = self.handle_send_message(session_id, directive) {
            warn!(
                session_id = %session_id,
                error = %e,
                "plan.continuation.dispatch_failed"
            );
        }
    }
}
