use super::*;

mod drive;
mod outcome;

impl BuiltInChatAgent {
    /// Execute a complete exchange: user message, LLM stream, tool calls, response.
    ///
    /// Uses `LlmRouter.stream()` to produce tokens one by one, emitting a
    /// [`RuntimeEvent::ChatToken`] for each token received. The ReAct loop
    /// continues until the LLM produces a final text response (no tool calls)
    /// or the [`StepBudget`] is exhausted.
    ///
    /// The autonomy tier governs the prompt variant, memory injection, and the
    /// post-run verification. The `budget` is built by the manager via
    /// `StepBudget::from_capped`, so it is already the capped ceiling; this method
    /// never raises it. `level_config` carries the resolved tier flags
    /// (`inject_memory`, `run_verification`); when `None` the call behaves as the
    /// assisted tier (no memory injection, no verification).
    ///
    /// # Errors
    ///
    /// - [`ChatError::BudgetExhausted`] if the step budget is exceeded
    /// - [`ChatError::InternalError`] for LLM backend failures
    // REASON: public entry of the ReAct loop: the arguments are the session's live handles, threaded not owned.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        session_id: &str,
        message_id: &str,
        run_id: &RunId,
        user_message: &str,
        history: &[ChatMessage],
        system_prompt: &str,
        available_tools: &[String],
        authorized_tools: &HashSet<String>,
        pending_approvals: &PendingChatApprovals,
        budget: &StepBudget,
        summary: Option<&str>,
        context_window_size: usize,
        autonomy: Option<&AutonomyLevel>,
        verification: Option<&VerificationLoop>,
        critic: Option<&CriticPass>,
        level_config: Option<&AutonomyLevelConfig>,
        cancel: CancellationToken,
    ) -> Result<ChatAgentResponse, ChatError> {
        let custom_prompt = if system_prompt.is_empty() {
            None
        } else {
            Some(system_prompt)
        };
        let level = autonomy.copied().unwrap_or(AutonomyLevel::Assisted);
        let inject_memory = level_config.is_some_and(|c| c.inject_memory);
        let run_verification = level_config.is_some_and(|c| c.run_verification);

        // Auditable trace of the applied tier. The budget is already capped by
        // the runtime ceiling at construction (principle #7); this only records
        // the effective values.
        tracing::info!(
            autonomy_level = %level.as_str(),
            inject_memory,
            run_verification,
            max_steps = budget.max_steps,
            max_tool_calls = budget.max_tool_calls,
            wall_clock_secs = budget.wall_clock_limit.as_secs(),
            "chat.autonomy_tier.applied"
        );

        let effective_prompt = self.build_system_prompt(custom_prompt, level, inject_memory);

        // Route the turn: in plan mode a substantive turn enters the plan flow
        // (discovery + plan), a trivial one stays conversational. Outside plan
        // mode the route is always conversational and the classic per-tool HITL
        // escalation path runs unchanged. The decision reuses the ORIA Observer
        // threshold; it never short-circuits the classic path.
        let route = classify_turn(user_message, self.session_plan_mode);
        tracing::info!(
            session_id = %session_id,
            plan_mode = self.session_plan_mode,
            route = ?route,
            "chat.turn.routed"
        );

        let mut tool_specs = build_tool_specs(
            available_tools,
            &self.tool_registry,
            self.mcp_index.as_deref(),
            self.tool_search_limit,
        )
        .await;
        if let Some(ref a2a) = self.a2a_invoker {
            tool_specs.extend(generate_a2a_tool_specs(a2a).await);
        }
        // Advertise the todo_write built-in whenever a todo store is attached.
        if self.todo.is_some() {
            tool_specs.push(todo_write_spec());
        }
        // Advertise the plan_* tool surface only while the session is in plan
        // mode, mirroring how todo_write is gated. The surface is phase-aware:
        // once a plan is approved (Executing) the proposal tools are withheld so
        // the agent executes the approved plan instead of re-proposing it.
        if self.plan_mode_active() {
            tool_specs.extend(plan_tool::plan_tool_specs_for_phase(
                self.session_plan_phase,
            ));
        }
        // Authoritative set of callable names for the turn: every advertised
        // spec, every session-declared tool, and every deferred MCP tool the
        // model can discover and invoke by its `mcp:server/tool` name. A call to
        // any other name is a hallucination, refused with a corrective tool
        // result rather than handed to the invoker. The union never false-refuses
        // a real tool and, since every member is a genuine tool, never lets a
        // hallucinated name slip through.
        let mut valid_tool_names: HashSet<String> =
            tool_specs.iter().map(|s| s.name.clone()).collect();
        valid_tool_names.extend(available_tools.iter().cloned());
        // A tool the operator pre-authorized for the session is, by definition,
        // a real callable tool: never refuse one as unknown.
        valid_tool_names.extend(authorized_tools.iter().cloned());
        if let Some(index) = self.mcp_index.as_deref() {
            valid_tool_names.extend(
                index
                    .iter()
                    .map(|e| format!("mcp:{}/{}", e.server_name, e.tool_name)),
            );
        }

        info!(
            session_id = %session_id,
            available = available_tools.len(),
            resolved = tool_specs.len(),
            tool_names = ?tool_specs.iter().map(|s| &s.name).collect::<Vec<_>>(),
            "chat.react.tools.resolved"
        );
        let mut llm_messages = build_llm_messages(
            &effective_prompt,
            history,
            user_message,
            summary,
            context_window_size,
        );

        // A resume turn carrying an operator instruction prepends it as a user
        // message so the agent reacts to it first, then adjusts the plan through
        // the `plan_*` tools. The loop stays agnostic: it forwards the message and
        // lets the agent pick the tool (add a step, set a dependency, or ask for
        // clarification). The runtime, not the model, stamps the `UserInject`
        // provenance on any step created during this turn.
        if let Some(injection) = self.pending_injection.as_ref() {
            llm_messages.push(LlmChatMessage::user(format!(
                "Operator instruction received while paused: {}",
                injection.text
            )));
            tracing::info!(
                session_id = %session_id,
                origin = "user_inject",
                "plan.inject.consumed"
            );
        }

        let ids = ToolCallContextIds {
            session_id,
            message_id,
            run_id,
            pending_approvals,
            cancel,
        };

        // Open discovery for a substantive plan-mode turn. The tracker is `None`
        // for conversational turns and outside plan mode, so the ReAct loop runs
        // exactly as before with no phase machinery and no extra events.
        //
        // A turn entered while the session is already in `AwaitingApproval` is a
        // revision turn: the soft gate is open and the user is iterating on the
        // submitted plan. It must not reopen discovery; the tracker starts in
        // `AwaitingApproval` and stays there (the agent revises via plan_* tools)
        // unless the agent re-submits, which keeps it there too. The loop never
        // blocks on an approval future, matching the soft-gate contract.
        // The phase tracker must run whenever plan mode is active, on the same
        // condition as the plan tools and the gate. Gating it on the route made
        // the phase machinery inconsistent: a turn the router scored as trivial
        // still let the agent submit a plan (the card showed) while the backend
        // session phase never moved to AwaitingApproval, so approval silently
        // failed its guard. The route is kept only for telemetry above.
        let mut phase_tracker = if self.plan_mode_active() {
            match self.session_plan_phase {
                // Revision turn: the soft gate is open, do not reopen discovery.
                PlanPhase::AwaitingApproval => Some(PlanPhaseTracker {
                    phase: PlanPhase::AwaitingApproval,
                }),
                // Execution turn (post-approval continuation): the plan is
                // approved, the tracker stays in Executing so the turn neither
                // reopens discovery nor re-submits the approved plan.
                PlanPhase::Executing => Some(PlanPhaseTracker {
                    phase: PlanPhase::Executing,
                }),
                _ => Some(self.begin_discovery(session_id)),
            }
        } else {
            None
        };

        let first = self
            .run_react_loop(
                &mut llm_messages,
                &tool_specs,
                authorized_tools,
                &valid_tool_names,
                budget,
                ids.clone(),
                &mut phase_tracker,
            )
            .await?;

        // A cooperative pause short-circuits verification: the turn stopped at a
        // checkpoint, so there is nothing to verify and the budget must be left
        // for the resume. Return the paused response untouched.
        if first.paused {
            return Ok(first);
        }

        // Post-run verification with bounded retry, gated by the autonomy tier.
        // The verification loop and critic are injected by the manager; when the
        // tier does not request verification, or neither is configured, this is a
        // no-op and the first response is returned unchanged.
        let Some(level) = autonomy.filter(|_| run_verification) else {
            return Ok(first);
        };
        let invoker = NoopCheckInvoker;
        let initial_output = first.content.clone();
        let tool_specs_ref: &[ToolSpec] = &tool_specs;
        let valid_tool_names_ref: &HashSet<String> = &valid_tool_names;
        // Seed the carried authorization set with the session's authorized tools
        // plus anything the first turn auto-authorized, so a correction turn does
        // not re-prompt for a tool the user already approved this turn.
        let mut carried_authorized = authorized_tools.clone();
        carried_authorized.extend(first.newly_authorized.iter().cloned());
        let carry = RetryCarry {
            messages: llm_messages,
            last_response: first,
            authorized: carried_authorized,
        };
        let (report, carry) = run_verification_with_retry(
            level,
            verification,
            critic,
            &invoker,
            user_message,
            &initial_output,
            budget,
            VERIFICATION_MAX_RETRIES,
            carry,
            move |mut state: RetryCarry, correction: String| {
                // Clone the per-turn ids (cheap `Arc` token plus borrows) before
                // the `async move` future takes ownership, so the FnMut closure can
                // be called once per retry without moving the captured value.
                let ids = ids.clone();
                async move {
                    state.messages.push(LlmChatMessage::user(correction));
                    // A verification retry is a correction turn, not a new
                    // discovery: it never opens or advances the plan phase, so pass
                    // no tracker.
                    let mut retry_phase: Option<PlanPhaseTracker> = None;
                    match self
                        .run_react_loop(
                            &mut state.messages,
                            tool_specs_ref,
                            &state.authorized,
                            valid_tool_names_ref,
                            budget,
                            ids,
                            &mut retry_phase,
                        )
                        .await
                    {
                        Ok(next) => {
                            let output = next.content.clone();
                            // Carry any tool authorized on this retry into the next.
                            state
                                .authorized
                                .extend(next.newly_authorized.iter().cloned());
                            state.last_response = next;
                            (Ok(output), state)
                        }
                        Err(error) => (Err(error), state),
                    }
                }
            },
        )
        .await;
        let mut response = carry.last_response;
        response.verification_report = report;
        Ok(response)
    }
}
