//! Landing of an exchange, whether it converged or failed.
//!
//! Persists the assistant turn and its tool records, updates the metrics, and
//! releases the session back to an idle state.

use super::super::*;

impl ChatSessionManager {
    /// Handle successful completion of a ReAct exchange.
    pub(in crate::chat::manager) fn handle_exchange_complete(
        &mut self,
        session_id: &str,
        _message_id: &str,
        response: ChatAgentResponse,
    ) {
        let session = match self.sessions.get_mut(session_id) {
            Some(s) => s,
            None => {
                warn!(session_id = %session_id, "chat.exchange.session.unknown");
                return;
            }
        };

        // A closed session must never be resurrected by a late in-flight event.
        // The exchange spawned before close can still deliver here; drop it.
        if session.status == SessionStatus::Closed {
            warn!(session_id = %session_id, "chat.exchange.session.closed");
            return;
        }

        let now = now_rfc3339();
        let assistant_msg_id = uuid::Uuid::new_v4().to_string();
        let tokens_used = response.tokens_used.clone();
        let context_window_tokens = response.context_window_tokens;
        let context_tokens_used = response.context_tokens_used;
        let max_steps = self.runtime_budget.max_steps;
        // Terminal plan phase of a plan-flow turn (discovery, then drafting once
        // the agent proposes steps, or back to done on a cancelled discovery).
        // `None` for conversational turns: the phase is left untouched.
        let final_plan_phase = response.final_plan_phase;

        // Serialize tool calls for SQLite
        let tool_calls_json = if response.tool_calls.is_empty() {
            None
        } else {
            serde_json::to_string(&response.tool_calls).ok()
        };

        // Serialize thinking trace (and its per-fragment tool-call boundaries,
        // when present) as JSON metadata so the UI can interleave each reasoning
        // fragment with the tool calls of its step.
        let metadata_json = response.thinking_trace.as_ref().map(|t| {
            if response.reasoning_boundaries.is_empty() {
                serde_json::json!({ "thinking_trace": t }).to_string()
            } else {
                serde_json::json!({
                    "thinking_trace": t,
                    "reasoning_boundaries": response.reasoning_boundaries,
                })
                .to_string()
            }
        });

        // Persist assistant response message
        match self.repository.append_message(&AppendMessageParams {
            id: &assistant_msg_id,
            session_id,
            role: &ChatRole::Assistant,
            content: &response.content,
            tool_calls_json: tool_calls_json.as_deref(),
            tool_name: None,
            created_at: &now,
            metadata: metadata_json.as_deref(),
        }) {
            Ok(seq) => {
                let metadata_value = metadata_json
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok());
                session.history.push(ChatMessage {
                    id: assistant_msg_id,
                    role: ChatRole::Assistant,
                    content: response.content,
                    tool_calls: if response.tool_calls.is_empty() {
                        None
                    } else {
                        Some(response.tool_calls)
                    },
                    tool_name: None,
                    created_at: now,
                    seq,
                    metadata: metadata_value,
                });
            }
            Err(e) => {
                error!(error = %e, "chat.message.persist.failed");
            }
        }

        // Persist newly authorized tools. A code executor (bash/python) is never
        // blanket-authorized by name: skip it so the next invocation still asks.
        for tool_name in &response.newly_authorized {
            if apollia_permissions::is_code_executor(tool_name) {
                warn!(
                    tool = %tool_name,
                    detail = "code executor,
                    each invocation requires approval",
                    "chat.approval.always_accept.refused"
                );
                continue;
            }
            let auth_now = now_rfc3339();
            if let Err(e) = self
                .repository
                .authorize_tool(session_id, tool_name, &auth_now)
            {
                warn!(error = %e, tool = %tool_name, "chat.authorization.persist.failed");
            }
            session.authorized_tools.insert(tool_name.clone());
        }

        // Reset session to Active
        session.status = SessionStatus::Active;
        session.active_exchange = None;
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Active)
        {
            warn!(error = %e, "chat.session.status.reset.failed");
        }

        // Record the cooperative pause disposition of the turn. A paused turn
        // stopped at a checkpoint with partial step statuses already persisted by
        // the PlanActor; the session is left ready for a resume. A converged turn
        // clears any pause state.
        let was_paused = response.paused;
        if was_paused {
            self.pause_states
                .insert(session_id.to_string(), PauseState::Paused);
            tracing::info!(session_id = %session_id, "chat.session.paused");
        } else {
            self.pause_states.remove(session_id);
            self.pause_tokens.remove(session_id);
        }

        // Persist the terminal plan phase when the turn ran in the plan flow, so
        // discovery / drafting survives a restart. Conversational turns leave it
        // untouched (`None`). One transition is refused: a turn that ended in
        // AwaitingApproval carries a snapshot taken before the operator approved
        // mid-turn; writing it back would regress Executing to AwaitingApproval
        // and re-open an already approved gate.
        if let Some(phase) = final_plan_phase {
            if session.plan_phase == PlanPhase::Executing && phase == PlanPhase::AwaitingApproval {
                tracing::info!(
                    session_id = %session_id,
                    "plan.phase.writeback_skipped_stale"
                );
            } else {
                session.plan_phase = phase;
                if let Err(e) = self.repository.set_plan_phase(session_id, phase) {
                    warn!(error = %e, "chat.plan.phase.persist.failed");
                }
            }
        }

        // `usage_reported = false` marks a turn whose backend path carries no
        // token accounting (Agent mode, or a fully aborted generation); the
        // zeros are then an absence of measurement, not a measurement.
        info!(
            session_id = %session_id,
            prompt_tokens = tokens_used.prompt_tokens,
            completion_tokens = tokens_used.completion_tokens,
            usage_reported = tokens_used.prompt_tokens + tokens_used.completion_tokens > 0,
            "chat.exchange.completed"
        );

        // ── accumulate session metrics ─────────────────────
        let entry = self
            .metrics
            .entry(session_id.to_string())
            .or_insert_with(|| SessionMetrics::new(session_id.to_string()));
        accumulate_exchange_metrics(
            entry,
            &tokens_used,
            session,
            max_steps,
            context_window_tokens,
            context_tokens_used,
        );

        // Emit a lightweight runtime event so the UI can trigger a refetch.
        // The event bridge forwards it as a generic `runtime-event` with
        // `event_type = "ChatResponseCompleted"`, and the frontend's metrics
        // store throttles subsequent `chat_session_metrics` calls to max 2/s.

        // A plan decision taken while this turn was still running parked its
        // continuation; the session is Active again, dispatch it now.
        self.dispatch_pending_plan_continuation(session_id);
    }

    /// Handle a failed ReAct exchange.
    pub(in crate::chat::manager) fn handle_exchange_error(
        &mut self,
        session_id: &str,
        message_id: &str,
        error: &str,
    ) {
        // A closed session must never be flipped back to Active by a late error
        // from an exchange that was in flight when the session closed. Drop it.
        if self
            .sessions
            .get(session_id)
            .is_some_and(|s| s.status == SessionStatus::Closed)
        {
            warn!(session_id = %session_id, "chat.exchange.session.closed");
            return;
        }

        error!(
            session_id = %session_id,
            message_id = %message_id,
            error = %error,
            "chat.exchange.failed"
        );

        let _ = self.event_bus.send(RuntimeEvent::ChatError {
            session_id: session_id.to_string(),
            message_id: Some(message_id.to_string()),
            error: error.to_string(),
        });

        // Always emit ChatResponseCompleted so the frontend exits the "generating"
        // state even when the exchange fails.  Without this the UI stays blocked
        // indefinitely because it waits for ChatResponseCompleted to clear the
        // typing indicator.
        let run_id = self
            .sessions
            .get(session_id)
            .and_then(|s| s.active_exchange.as_ref())
            .map(|e| e.run_id.clone());
        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: format!("[Error: {error}]"),
            run_id,
        });

        // Reset session to Active so it can accept new messages
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.status = SessionStatus::Active;
            session.active_exchange = None;
        }
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Active)
        {
            warn!(error = %e, "chat.session.status.reset.failed");
        }
    }
}
