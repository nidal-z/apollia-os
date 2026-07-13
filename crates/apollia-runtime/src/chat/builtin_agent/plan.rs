use super::*;

impl BuiltInChatAgent {
    /// Returns `true` when the `plan_*` tool surface should be active.
    ///
    /// Both conditions must hold: the session has plan mode enabled and a plan
    /// store is attached to drive the tools. This single predicate gates both the
    /// spec advertising and the inline tool dispatch.
    pub(in crate::chat::builtin_agent) fn plan_mode_active(&self) -> bool {
        self.session_plan_mode && self.plan.is_some()
    }

    /// Returns `true` when the plan-mode hard gate must refuse execution tools.
    ///
    /// Active only in plan mode and only while the plan is still being prepared:
    /// the discovery, drafting, and awaiting-approval phases. Once the user
    /// approves, the manager dispatches a continuation turn in
    /// [`PlanPhase::Executing`] and the gate opens; [`PlanPhase::Done`] (plan
    /// mode off or plan finished) never gates.
    pub(in crate::chat::builtin_agent) fn plan_gate_blocks(&self) -> bool {
        self.plan_mode_active()
            && matches!(
                self.session_plan_phase,
                PlanPhase::Discovery | PlanPhase::Drafting | PlanPhase::AwaitingApproval
            )
    }

    /// Surfaces a plan-mode phase change to the desktop.
    ///
    /// Emits [`RuntimeEvent::ChatPlanPhaseChanged`] on the event bus so the UI
    /// follows the discovery / drafting indicator live. Fire-and-forget: a
    /// saturated or closed bus is ignored, matching the plan and todo actors.
    /// A phase can move before any plan exists (discovery precedes drafting), so
    /// this is distinct from the plan-content `PlanUpdated` event.
    fn emit_plan_phase(&self, session_id: &str, phase: PlanPhase) {
        let _ = self.event_bus.send(RuntimeEvent::ChatPlanPhaseChanged {
            session_id: session_id.to_string(),
            phase: phase.as_sql().to_string(),
        });
        tracing::info!(
            session_id = %session_id,
            phase = %phase.as_sql(),
            "chat.plan_phase.changed"
        );
    }

    /// Opens the discovery phase for a substantive plan-mode turn.
    ///
    /// Sets the tracked phase to [`PlanPhase::Discovery`] and surfaces it so the
    /// agent can use the blocking `ask_user` tool to gather real inputs before
    /// drafting. The ReAct loop then advances the phase to drafting on the first
    /// plan-construction call, or back to a safe state on a cancelled question.
    pub(in crate::chat::builtin_agent) fn begin_discovery(
        &self,
        session_id: &str,
    ) -> PlanPhaseTracker {
        self.emit_plan_phase(session_id, PlanPhase::Discovery);
        PlanPhaseTracker {
            phase: PlanPhase::Discovery,
        }
    }

    /// Advances the plan phase after a tool-call turn, given the calls just run.
    ///
    /// While in discovery, the first plan-construction call moves the phase to
    /// drafting, and a fully cancelled `ask_user` answer moves it back to the
    /// safe done state. Other phases are left untouched. Returns `true` when the
    /// phase changed, so the caller can stop watching once discovery is over.
    pub(in crate::chat::builtin_agent) fn advance_plan_phase(
        &self,
        tracker: &mut PlanPhaseTracker,
        records: &[ToolCallRecord],
        session_id: &str,
    ) {
        if tracker.phase != PlanPhase::Discovery {
            return;
        }
        if records
            .iter()
            .any(|r| PlanPhaseTracker::is_construction_tool(&r.tool_name))
        {
            tracker.phase = PlanPhase::Drafting;
            self.emit_plan_phase(session_id, PlanPhase::Drafting);
            return;
        }
        if records.iter().any(PlanPhaseTracker::is_cancelled_ask_user) {
            tracker.phase = PlanPhase::Done;
            self.emit_plan_phase(session_id, PlanPhase::Done);
        }
    }

    /// Moves the tracked phase to [`PlanPhase::AwaitingApproval`] when the agent
    /// submitted the plan during the turn.
    ///
    /// A successful `plan_submit` call opens the soft conversational gate: the
    /// turn ends in awaiting-approval rather than blocking on an approval future.
    /// The resume is driven later by an approve command or by a revision message,
    /// both observable session transitions, never an `await` inside the loop.
    /// Unlike [`advance_plan_phase`](Self::advance_plan_phase) this fires from any
    /// phase, since the agent reaches submit from drafting (or directly when it
    /// proposes and submits within one turn).
    ///
    /// Returns `true` when a fresh `plan_submit` moved the tracker into
    /// awaiting-approval this turn, so the caller can end the turn immediately:
    /// the agent must not keep working (even read-only) once the plan is
    /// submitted; execution resumes only after the user approves.
    pub(in crate::chat::builtin_agent) fn advance_on_submit(
        &self,
        tracker: &mut PlanPhaseTracker,
        records: &[ToolCallRecord],
        session_id: &str,
    ) -> bool {
        if tracker.phase == PlanPhase::AwaitingApproval {
            return false;
        }
        let submitted = records.iter().any(|r| {
            r.tool_name == PLAN_SUBMIT_TOOL_NAME
                && r.output
                    .as_deref()
                    .and_then(|o| serde_json::from_str::<serde_json::Value>(o).ok())
                    .and_then(|v| v.get("ok").and_then(serde_json::Value::as_bool))
                    .unwrap_or(false)
        });
        if submitted {
            tracker.phase = PlanPhase::AwaitingApproval;
            self.emit_plan_phase(session_id, PlanPhase::AwaitingApproval);
        }
        submitted
    }

    /// Plan-mode safety net: submit a drafted-but-unsubmitted plan.
    ///
    /// Weak models commonly call `plan_propose`, then thrash on execution tools
    /// (denied by the gate) and never call `plan_submit`, leaving a draft the
    /// user can never approve. As soon as a non-empty plan exists in discovery
    /// or drafting, submit it so the approval gate engages and the caller can
    /// end the turn. Returns `true` when it submitted. Idempotent: a turn already
    /// in awaiting-approval skips this.
    pub(in crate::chat::builtin_agent) async fn auto_submit_if_drafted(
        &self,
        tracker: &mut PlanPhaseTracker,
        session_id: &str,
    ) -> bool {
        if !matches!(tracker.phase, PlanPhase::Discovery | PlanPhase::Drafting) {
            return false;
        }
        let Some(plan) = self.plan.as_ref() else {
            return false;
        };
        let has_steps = matches!(
            plan.get_plan(session_id).await,
            Ok(Some(p)) if !p.steps.is_empty()
        );
        if !has_steps {
            return false;
        }
        match plan.submit(session_id).await {
            Ok(_) => {
                tracker.phase = PlanPhase::AwaitingApproval;
                self.emit_plan_phase(session_id, PlanPhase::AwaitingApproval);
                tracing::info!(session_id = %session_id, "plan.auto_submit");
                true
            }
            Err(e) => {
                tracing::warn!(session_id = %session_id, error = %e, "plan.auto_submit_failed");
                false
            }
        }
    }

    /// Run a `plan_*` built-in tool inside the ReAct loop.
    ///
    /// Routes the call to the matching `run_plan_*` function, which delegates to
    /// the [`PlanHandle`] and shapes a serializable result. The JSON snapshot is
    /// injected as the tool message. Returns `true` when the operation failed (a
    /// rejected mutation or a malformed payload) so the loop counts it toward
    /// escalation; the loop itself never stops on a plan error.
    // REASON: six borrowed parameters (handle, ids, call, two sinks, optional
    // injection) are each independent; bundling them into a struct would not
    // improve clarity, so the threshold is relaxed for this in-loop dispatcher.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::chat::builtin_agent) async fn handle_plan_tool(
        plan: &PlanHandle,
        session_id: &str,
        call: &ToolCall,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
        injection: Option<&InjectedInstruction>,
    ) -> bool {
        // On an inject-driven resume turn, the runtime (not the model) is the
        // source of truth for provenance: stamp UserInject + the operator reason
        // onto any step the agent adds or modifies, so AC-1 holds regardless of
        // what the LLM emitted in the `step` payload.
        let stamped;
        let args = match (injection, call.name.as_str()) {
            (Some(inj), PLAN_ADD_STEP_TOOL_NAME | PLAN_MODIFY_STEP_TOOL_NAME) => {
                stamped = stamp_inject_provenance(&call.arguments, &inj.provenance());
                &stamped
            }
            _ => &call.arguments,
        };
        let result = match call.name.as_str() {
            PLAN_PROPOSE_TOOL_NAME => plan_tool::run_plan_propose(plan, session_id, args).await,
            PLAN_ADD_STEP_TOOL_NAME => plan_tool::run_plan_add_step(plan, session_id, args).await,
            PLAN_MODIFY_STEP_TOOL_NAME => {
                plan_tool::run_plan_modify_step(plan, session_id, args).await
            }
            PLAN_REMOVE_STEP_TOOL_NAME => {
                plan_tool::run_plan_remove_step(plan, session_id, args).await
            }
            PLAN_REORDER_TOOL_NAME => plan_tool::run_plan_reorder(plan, session_id, args).await,
            PLAN_SET_STEP_STATUS_TOOL_NAME => {
                plan_tool::run_plan_set_step_status(plan, session_id, args).await
            }
            PLAN_SUBMIT_TOOL_NAME => plan_tool::run_plan_submit(plan, session_id).await,
            // Only plan tool names reach this method (the dispatch guards on
            // `is_plan_tool`); an unknown name yields a structured error rather
            // than a panic.
            other => plan_tool::PlanToolResult {
                ok: false,
                error: Some(format!("unknown plan tool: {other}")),
                plan: None,
            },
        };
        tracing::info!(
            session_id = %session_id,
            tool_name = %call.name,
            ok = result.ok,
            "chat.plan_tool.applied"
        );
        let tool_result = serde_json::to_string(&result).unwrap_or_else(|_| {
            r#"{"ok":false,"error":"plan tool result serialization failed"}"#.to_string()
        });
        llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
        acc.all_tool_calls.push(ToolCallRecord {
            tool_name: call.name.clone(),
            input: call.arguments.clone(),
            output: Some(tool_result),
            status: ToolCallStatus::Executed,
            rationale: None,
            retry_attempts: Vec::new(),
        });
        !result.ok
    }

    /// Re-inject the active plan as a reminder after a context compaction dropped
    /// the conversation history.
    ///
    /// Mirrors [`inject_todo_after_compaction`](Self::inject_todo_after_compaction):
    /// reads the plan through the [`PlanHandle`] and, when one exists with steps,
    /// appends a single user-role reminder enumerating each step with its status,
    /// so the agent keeps executing the approved plan instead of re-proposing it.
    /// Errors are logged and swallowed: the loop continues without the reminder
    /// rather than failing.
    pub(in crate::chat::builtin_agent) async fn inject_plan_after_compaction(
        plan: &PlanHandle,
        session_id: &str,
        messages: &mut Vec<LlmChatMessage>,
    ) {
        let plan = match plan.get_plan(session_id).await {
            Ok(Some(plan)) => plan,
            Ok(None) => return,
            Err(e) => {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "plan.reinject.failed"
                );
                return;
            }
        };
        if plan.steps.is_empty() {
            return;
        }
        let mut reminder = String::from(PLAN_REMINDER_PREFIX);
        for step in &plan.steps {
            reminder.push_str(&format!(
                "\n- [{}] {}: {}",
                step_status_token(&step.status),
                step.step_id,
                if step.title.is_empty() {
                    step.description.as_str()
                } else {
                    step.title.as_str()
                }
            ));
        }
        messages.push(LlmChatMessage::user(reminder));
        tracing::info!(
            session_id = %session_id,
            step_count = plan.steps.len(),
            compaction_triggered = true,
            "chat.plan.reinjected_after_compaction"
        );
    }
}
