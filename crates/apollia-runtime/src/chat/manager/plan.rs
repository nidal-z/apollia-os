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
    pub(in crate::chat::manager) async fn handle_approve_plan(
        &mut self,
        session_id: &str,
    ) -> Result<(), ChatError> {
        self.guard_awaiting_approval(session_id).await?;

        // Persist first so the approved transition survives a restart even if the
        // continuation dispatch below fails (e.g. no LLM configured).
        self.repository
            .set_plan_phase(session_id, PlanPhase::Executing)?;
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.plan_phase = PlanPhase::Executing;
        }

        // Move the persisted plan row out of `awaiting_approval` as well. Without
        // this, the plan status kept saying "awaiting approval" forever, and the
        // race fallback in `guard_awaiting_approval` could later reconcile the
        // session back into the gate, re-opening an already approved plan.
        if let Ok(Some(plan_handle)) = self.resolve_plan_handle(session_id) {
            if let Err(e) = plan_handle.mark_executing(session_id).await {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "plan.status.mark_executing_failed"
                );
            }
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
    pub(in crate::chat::manager) async fn handle_reject_plan(
        &mut self,
        session_id: &str,
        reason: Option<String>,
    ) -> Result<(), ChatError> {
        self.guard_awaiting_approval(session_id).await?;

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
    ///
    /// Closes a race: the fast broadcast events (`PlanSubmitted` /
    /// `ChatPlanPhaseChanged`) raise the approval card mid-turn when `plan_submit`
    /// runs, but the authoritative in-memory `session.plan_phase` only flips to
    /// [`PlanPhase::AwaitingApproval`] later, when this actor processes
    /// `ExchangeComplete`. `ApprovePlan` and `ExchangeComplete` are both
    /// `ChatCommand`s on the same serial loop, so a quick user click enqueues
    /// `ApprovePlan` before `ExchangeComplete` is processed and the phase read
    /// here is stale. When the in-memory phase is not yet awaiting approval, fall
    /// back to the persisted plan status through the [`PlanHandle`]: PlanActor's
    /// submit persists `PlanStatus::AwaitingApproval` synchronously before those
    /// events fire, so a status read is authoritative. On a match, reconcile the
    /// in-memory phase and proceed rather than failing the guard. This is a
    /// read-side reconciliation only: event ordering and the actor command model
    /// are untouched.
    async fn guard_awaiting_approval(&mut self, session_id: &str) -> Result<(), ChatError> {
        let phase = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?
            .plan_phase;
        if phase == PlanPhase::AwaitingApproval {
            return Ok(());
        }

        // Race fallback: consult the authoritative persisted plan status. Resolve
        // the handle synchronously (an owned clone) so no SQLite borrow is held
        // across the await, matching the GetPlan / ReadPlanMutations command path.
        if let Some(plan_handle) = self.resolve_plan_handle(session_id)? {
            if let Ok(Some(plan)) = plan_handle.get_plan(session_id).await {
                if plan.status == apollia_core::plan::PlanStatus::AwaitingApproval {
                    if let Some(session) = self.sessions.get_mut(session_id) {
                        session.plan_phase = PlanPhase::AwaitingApproval;
                    }
                    tracing::info!(
                        session_id = %session_id,
                        "plan.guard.reconciled_awaiting_approval"
                    );
                    return Ok(());
                }
            }
        }

        Err(ChatError::NotAwaitingApproval {
            session_id: session_id.to_string(),
            current_phase: phase.as_sql().to_string(),
        })
    }

    /// Dispatch a continuation turn carrying a synthetic directive.
    ///
    /// Reuses [`handle_send_message`](Self::handle_send_message) so the resume is
    /// an ordinary exchange through the same actor path: one user turn, one
    /// response cycle, the existing bounded `ChatCommand` channel.
    ///
    /// A [`ChatError::SessionBusy`] is not a failure: the decision was taken
    /// mid-turn (the approval card raises before this actor processes the turn's
    /// `ExchangeComplete`). The directive is parked in
    /// `pending_plan_continuations` and re-dispatched by
    /// [`dispatch_pending_plan_continuation`](Self::dispatch_pending_plan_continuation)
    /// when the in-flight turn completes. Other errors stay best-effort logs:
    /// the gate decision they follow has already been recorded and emitted.
    pub(in crate::chat::manager) fn dispatch_plan_continuation(
        &mut self,
        session_id: &str,
        directive: &str,
    ) {
        match self.handle_send_message(session_id, directive) {
            Ok(_) => {}
            Err(ChatError::SessionBusy(_)) => {
                self.pending_plan_continuations
                    .insert(session_id.to_string(), directive.to_string());
                tracing::info!(session_id = %session_id, "plan.continuation.queued");
            }
            Err(e) => {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "plan.continuation.dispatch_failed"
                );
            }
        }
    }

    /// Re-dispatch the parked plan continuation of a session, if any.
    ///
    /// Called from `handle_exchange_complete` after the session is reset to
    /// `Active`, so a plan decision taken while the previous turn was still
    /// running finally produces its execution (or revision) turn instead of
    /// being silently dropped.
    pub(in crate::chat::manager) fn dispatch_pending_plan_continuation(
        &mut self,
        session_id: &str,
    ) {
        if let Some(directive) = self.pending_plan_continuations.remove(session_id) {
            tracing::info!(session_id = %session_id, "plan.continuation.resumed");
            self.dispatch_plan_continuation(session_id, &directive);
        }
    }
}
