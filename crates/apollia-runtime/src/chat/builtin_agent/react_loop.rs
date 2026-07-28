use super::stream::StreamConsumeParams;
use super::*;

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
            "Chat ReAct loop: tool specs resolved"
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

    /// Run the ReAct loop to completion and return the terminal response.
    ///
    /// Drives the stream/tool-call cycle until the LLM produces a final text
    /// response, the stream errors, or the [`StepBudget`] is exhausted. The
    /// caller owns `llm_messages`, so it can append a correction turn and call
    /// this again for a bounded verification retry.
    ///
    /// # Errors
    ///
    /// - [`ChatError::BudgetExhausted`] if the step budget is exceeded.
    /// - [`ChatError::InternalError`] for LLM backend failures.
    #[allow(clippy::too_many_arguments)]
    async fn run_react_loop(
        &self,
        llm_messages: &mut Vec<LlmChatMessage>,
        tool_specs: &[ToolSpec],
        authorized_tools: &HashSet<String>,
        valid_tool_names: &HashSet<String>,
        budget: &StepBudget,
        ids: ToolCallContextIds<'_>,
        phase_tracker: &mut Option<PlanPhaseTracker>,
    ) -> Result<ChatAgentResponse, ChatError> {
        let session_id = ids.session_id;
        let message_id = ids.message_id;
        let run_id = ids.run_id;
        let mut total_usage = TokenUsage::default();
        // Real context window of the active model, in tokens, when it reports one.
        // `None` (e.g. cloud backend or before the local model loads) leaves the
        // context gauge in an "unknown" state rather than showing a wrong value.
        let context_window_tokens = self
            .llm_router
            .context_window_tokens()
            .and_then(|w| u32::try_from(w).ok());
        // Prompt-token count of the most recent LLM call. Overwritten each turn so
        // it reflects the current context occupancy (not a cumulative sum) for the
        // context gauge.
        let mut last_prompt_tokens: u32 = 0;
        let mut acc = ReactAccumulators {
            all_tool_calls: Vec::new(),
            newly_authorized: Vec::new(),
            authorized: authorized_tools.clone(),
        };
        let obs = ObservabilityConfig::default();
        // Each entry is (reasoning text, number of tool calls already made when
        // the fragment was captured), so the UI can interleave a step's
        // reasoning with that step's tool calls.
        let mut reasoning_fragments: Vec<(String, usize)> = Vec::new();
        // Conservative escalation heuristic: count consecutive failed tool calls
        // and, past the threshold, ask the router to escalate the next LLM call to
        // the frontier backend. Reset to 0 on the first successful tool call.
        let mut consecutive_tool_failures: u32 = 0;
        // Set when an escalation was requested but the cost ceiling kept the step
        // local. Surfaced on the terminal response so the caller can warn.
        let mut frontier_ceiling_reached = false;

        // Token cost of the tool schemas advertised every turn, computed once
        // since `tool_specs` is fixed for the loop. Reserved from the context
        // window on each compaction check below.
        // clippy::needless_borrow is a false positive here: `tool_specs` is an
        // owned Vec reused inside the loop (the request builder), so it cannot be
        // moved into the estimator.
        #[allow(clippy::needless_borrow)]
        let tool_reserve = apollia_llm::estimate_tool_specs_tokens(&tool_specs);

        loop {
            // Cooperative pause checkpoint: a pause request cancels the token; we
            // exit before spending the next step. Step statuses are already
            // persisted by the PlanActor at the end of the previous iteration, so
            // nothing is half-applied and no history is rewritten here. The turn
            // returns a paused response (not an error); the manager records the
            // session as Paused and the StepBudget carries over to the resume.
            if ids.cancel.is_cancelled() {
                tracing::info!(session_id = %session_id, "chat.react.paused");
                return Ok(self.paused_response(
                    &reasoning_fragments,
                    ResponseContext {
                        acc,
                        total_usage,
                        session_id,
                        message_id,
                        run_id,
                        frontier_ceiling_reached,
                        final_plan_phase: phase_tracker.as_ref().map(|t| t.phase),
                        context_window_tokens,
                        context_tokens_used: last_prompt_tokens,
                    },
                ));
            }

            // Step safeguard: budget check before every LLM call
            if budget.is_exhausted() {
                let reason = budget
                    .exhaustion_reason()
                    .unwrap_or_else(|| "unknown".into());
                tracing::warn!(
                    %reason,
                    session_id = %session_id,
                    "chat step budget exhausted"
                );
                return Err(ChatError::BudgetExhausted);
            }
            budget.increment_steps();

            // Compact context if messages approach the model's context limit.
            // The tool schemas travel in the same request, so `tool_reserve`
            // (their estimated token cost) is folded in: a large tool surface
            // must not silently push the combined prompt over the window (a hard
            // 400 from llama-server). When a compaction drops the history,
            // re-inject the todo list so the agent never loses track of pending
            // work (principle: memory at the agent's initiative; nothing is
            // injected when the list is empty).
            let was_compacted = self
                .maybe_compact_context(llm_messages, session_id, tool_reserve)
                .await;
            if was_compacted {
                if let Some(todo) = self.todo.as_ref() {
                    Self::inject_todo_after_compaction(todo, session_id, llm_messages).await;
                }
                // Re-inject an active plan so a compaction never erases the steps
                // the agent is executing. Only while plan mode is active and a plan
                // store is attached; a no-op otherwise.
                if self.plan_mode_active() {
                    if let Some(plan) = self.plan.as_ref() {
                        Self::inject_plan_after_compaction(plan, session_id, llm_messages).await;
                    }
                }
            }

            let request = CompletionRequest {
                messages: llm_messages.clone(),
                tools: tool_specs.to_vec(),
                temperature: self.turn_temperature(!tool_specs.is_empty()),
                ..Default::default()
            };

            // Emit ChatResponseStarted before the first token
            let _ = self.event_bus.send(RuntimeEvent::ChatResponseStarted {
                session_id: session_id.to_string(),
                message_id: message_id.to_string(),
                run_id: Some(run_id.clone()),
            });

            // Derive the escalation signal from the consecutive-failure counter.
            // Only when it escalates do we route through the frontier policy; the
            // non-escalated path keeps the default backend, so behavior without
            // hybrid routing or below threshold is byte-for-byte unchanged. Routing
            // and ceiling policy stay in `LlmRouter`; the loop only detects and
            // observes. The escalated backend is streamed directly (the router map
            // owns it), so this does not depend on a name round-trip.
            let signal = if consecutive_tool_failures >= ESCALATION_FAILURE_THRESHOLD {
                EscalationSignal::RepeatedStepFailure {
                    consecutive_failures: consecutive_tool_failures,
                }
            } else {
                EscalationSignal::None
            };
            let stream_result = if signal.is_escalation() {
                let backend = self
                    .llm_router
                    .route_with_escalation(signal, LlmRoutingLevel::Precise);
                if self.llm_router.is_ceiling_reached() {
                    frontier_ceiling_reached = true;
                    // Hard stop: when configured, stop the run cleanly instead of
                    // continuing on the degraded local backend. The router owns the
                    // threshold decision; the loop only applies the configured action.
                    if self.llm_router.ceiling_action() == CeilingAction::HardStop {
                        let cost_usd = self.llm_router.session_cost_usd();
                        let ceiling_usd = self.llm_router.cost_ceiling_usd().unwrap_or(0.0);
                        let _ = self.event_bus.send(RuntimeEvent::CostCeilingReached {
                            session_id: session_id.to_string(),
                            cost_usd,
                            ceiling_usd,
                        });
                        tracing::warn!(
                            session_id = %session_id,
                            cost_usd,
                            ceiling_usd,
                            ceiling_action = "hard_stop",
                            "chat.cost_ceiling.hard_stop"
                        );
                        return Err(ChatError::CostCeilingExceeded {
                            cost_usd,
                            ceiling_usd,
                        });
                    }
                }
                tracing::info!(
                    consecutive_failures = consecutive_tool_failures,
                    backend = %backend.backend_name(),
                    ceiling_reached = frontier_ceiling_reached,
                    session_id = %session_id,
                    "chat.escalation.requested"
                );
                // Race Stop against the setup await. This is the "thinking"
                // window: the backend blocks here until the first token (prompt
                // prefill / time-to-first-token), with nothing in `consume_stream`
                // yet, so without this a Stop is ignored during "réflexion". On
                // cancel, end the turn paused with no partial text.
                tokio::select! {
                    biased;
                    () = ids.cancel.cancelled() => {
                        tracing::info!(session_id = %session_id, "chat.react.paused_before_stream");
                        return Ok(self.paused_text_response(
                            "",
                            &mut reasoning_fragments,
                            ResponseContext {
                                acc,
                                total_usage,
                                session_id,
                                message_id,
                                run_id,
                                frontier_ceiling_reached,
                                final_plan_phase: phase_tracker.as_ref().map(|t| t.phase),
                                context_window_tokens,
                                context_tokens_used: last_prompt_tokens,
                            },
                        ));
                    }
                    r = backend.stream(request) => r,
                }
            } else {
                tokio::select! {
                    biased;
                    () = ids.cancel.cancelled() => {
                        tracing::info!(session_id = %session_id, "chat.react.paused_before_stream");
                        return Ok(self.paused_text_response(
                            "",
                            &mut reasoning_fragments,
                            ResponseContext {
                                acc,
                                total_usage,
                                session_id,
                                message_id,
                                run_id,
                                frontier_ceiling_reached,
                                final_plan_phase: phase_tracker.as_ref().map(|t| t.phase),
                                context_window_tokens,
                                context_tokens_used: last_prompt_tokens,
                            },
                        ));
                    }
                    r = self.llm_router.stream_with_observability(None, request, &obs) => r,
                }
            };
            let stream = stream_result.map_err(|e| ChatError::InternalError(e.to_string()))?;

            // Consume stream, emit ChatToken per token, accumulate text
            let mut accumulated_text = String::new();
            let mut chunk_usage = TokenUsage::default();
            let stream_result = self
                .consume_stream(
                    stream,
                    StreamConsumeParams {
                        session_id,
                        message_id,
                        cancel: &ids.cancel,
                    },
                    &mut accumulated_text,
                    &mut chunk_usage,
                )
                .await;
            // Fold this call's usage into the exchange total, and record its
            // prompt size as the current context occupancy.
            total_usage.merge(&chunk_usage);
            if chunk_usage.prompt_tokens > 0 {
                last_prompt_tokens = chunk_usage.prompt_tokens;
            }

            // Mid-stream cooperative stop. The user hit Stop while the LLM was
            // streaming: freeze the partial text as the assistant turn, mark the
            // turn paused (no ChatResponseCompleted), and let the manager persist
            // it. This is the checkpoint that makes Stop interrupt a single
            // streaming answer, not only the boundary between ReAct steps.
            if ids.cancel.is_cancelled() {
                tracing::info!(session_id = %session_id, "chat.react.paused_mid_stream");
                return Ok(self.paused_text_response(
                    &accumulated_text,
                    &mut reasoning_fragments,
                    ResponseContext {
                        acc,
                        total_usage,
                        session_id,
                        message_id,
                        run_id,
                        frontier_ceiling_reached,
                        final_plan_phase: phase_tracker.as_ref().map(|t| t.phase),
                        context_window_tokens,
                        context_tokens_used: last_prompt_tokens,
                    },
                ));
            }

            // Capture the full response for deterministic replay. A stream that
            // errored is captured as truncated with its partial text, never
            // dropped. The shared LLM router stays run-agnostic: capture happens
            // here, where run_id and the assembled response are both available.
            let captured_tool_calls: Vec<serde_json::Value> = match &stream_result {
                Ok(tool_calls) => tool_calls
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "name": tc.name,
                            "arguments": tc.arguments,
                        })
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            let _ = self.event_bus.send(RuntimeEvent::LlmResponseCaptured {
                run_id: run_id.clone(),
                backend: self.llm_router.default_name().to_string(),
                model: String::new(),
                content: accumulated_text.clone(),
                tool_calls: captured_tool_calls,
                prompt_tokens: chunk_usage.prompt_tokens,
                completion_tokens: chunk_usage.completion_tokens,
                cost_usd: chunk_usage.cost_usd,
                stream_truncated: stream_result.is_err(),
            });

            match stream_result {
                Ok(tool_calls) if tool_calls.is_empty() => {
                    // Plan-mode safety net: if the agent drafted a plan but ended
                    // the turn without submitting it, submit it now so the user
                    // gets an approval card instead of an un-actionable draft.
                    let submitted_now = if let Some(tracker) = phase_tracker.as_mut() {
                        self.auto_submit_if_drafted(tracker, session_id).await
                    } else {
                        false
                    };
                    // Fix: a plan submitted with no model prose would persist an
                    // empty assistant message (weak local models often emit the
                    // plan_submit call alone). Substitute a deterministic,
                    // runtime-generated summary so the UI never shows the
                    // empty-response fallback. Non-empty text is left untouched.
                    let content = if submitted_now {
                        self.plan_submit_text(session_id, &accumulated_text).await
                    } else {
                        accumulated_text.clone()
                    };
                    return Ok(self.finalize_text_response(
                        &content,
                        &mut reasoning_fragments,
                        ResponseContext {
                            acc,
                            total_usage,
                            session_id,
                            message_id,
                            run_id,
                            frontier_ceiling_reached,
                            final_plan_phase: phase_tracker.as_ref().map(|t| t.phase),
                            context_window_tokens,
                            context_tokens_used: last_prompt_tokens,
                        },
                    ));
                }
                Ok(tool_calls) => {
                    // Index of the first record produced by this turn, so the
                    // plan-phase advance inspects only the calls just run.
                    let turn_start = acc.all_tool_calls.len();
                    self.record_tool_turn(
                        RecordTurnInput {
                            accumulated_text: &accumulated_text,
                            tool_calls: &tool_calls,
                            budget,
                            ids: ids.clone(),
                            valid_tool_names,
                        },
                        &mut reasoning_fragments,
                        llm_messages,
                        &mut acc,
                        &mut consecutive_tool_failures,
                    )
                    .await;
                    // Discovery -> Drafting on the first plan-construction call;
                    // Discovery -> Done on a fully cancelled question. No-op
                    // outside the plan flow (tracker is `None`).
                    let mut submitted_now = if let Some(tracker) = phase_tracker.as_mut() {
                        let turn_records = &acc.all_tool_calls[turn_start..];
                        self.advance_plan_phase(tracker, turn_records, session_id);
                        // A successful plan_submit opens the soft gate.
                        self.advance_on_submit(tracker, turn_records, session_id)
                    } else {
                        false
                    };

                    // As soon as a complete plan has been drafted, submit it even
                    // if the model did not call plan_submit. A weak model tends to
                    // propose then thrash on denied execution tools; submitting the
                    // draft now and ending the turn cuts that loop short.
                    if !submitted_now {
                        if let Some(tracker) = phase_tracker.as_mut() {
                            submitted_now = self.auto_submit_if_drafted(tracker, session_id).await;
                        }
                    }

                    // Plan submitted: end the turn now. The agent must not keep
                    // working (even read-only) before approval, otherwise it does
                    // the whole job during "planning" and the approval +
                    // step-status execution become meaningless. The approval card
                    // is already up via the PlanSubmitted event; execution resumes
                    // on approval in a fresh turn.
                    if submitted_now {
                        // Fix: substitute a deterministic summary when the submit
                        // turn carried no model prose, so an empty assistant
                        // message is never persisted. Non-empty text is untouched.
                        let content = self.plan_submit_text(session_id, &accumulated_text).await;
                        return Ok(self.finalize_text_response(
                            &content,
                            &mut reasoning_fragments,
                            ResponseContext {
                                acc,
                                total_usage,
                                session_id,
                                message_id,
                                run_id,
                                frontier_ceiling_reached,
                                final_plan_phase: Some(PlanPhase::AwaitingApproval),
                                context_window_tokens,
                                context_tokens_used: last_prompt_tokens,
                            },
                        ));
                    }

                    // Cooperative pause checkpoint between tool turns. By this
                    // point every tool call of the turn has been awaited to
                    // completion inside `record_tool_turn`, so a pause requested
                    // during a long-running tool takes effect only here, at the
                    // next safe boundary, never mid-tool. No half-applied mutation
                    // can be observed on this path.
                    if ids.cancel.is_cancelled() {
                        tracing::info!(session_id = %session_id, "chat.react.paused");
                        return Ok(self.paused_response(
                            &reasoning_fragments,
                            ResponseContext {
                                acc,
                                total_usage,
                                session_id,
                                message_id,
                                run_id,
                                frontier_ceiling_reached,
                                final_plan_phase: phase_tracker.as_ref().map(|t| t.phase),
                                context_window_tokens,
                                context_tokens_used: last_prompt_tokens,
                            },
                        ));
                    }
                }
                Err(err) => {
                    return Ok(self.stream_error_response(
                        err,
                        accumulated_text,
                        ResponseContext {
                            acc,
                            total_usage,
                            session_id,
                            message_id,
                            run_id,
                            frontier_ceiling_reached,
                            final_plan_phase: phase_tracker.as_ref().map(|t| t.phase),
                            context_window_tokens,
                            context_tokens_used: last_prompt_tokens,
                        },
                    ));
                }
            }
        }
    }

    /// Build the [`ChatAgentResponse`] returned when the loop stops at a pause
    /// checkpoint.
    ///
    /// Carries the work already done this turn (tool-call records, reasoning,
    /// terminal plan phase) and sets `paused`. No `ChatResponseCompleted` event is
    /// emitted: the turn did not converge, it was suspended, and the manager
    /// records the session as paused so it can be resumed from the persisted plan
    /// state.
    fn paused_response(
        &self,
        reasoning_fragments: &[(String, usize)],
        ctx: ResponseContext<'_>,
    ) -> ChatAgentResponse {
        let ResponseContext {
            acc,
            total_usage,
            session_id,
            frontier_ceiling_reached,
            final_plan_phase,
            context_window_tokens,
            context_tokens_used,
            ..
        } = ctx;
        let (thinking_trace, reasoning_boundaries) =
            Self::build_thinking_trace(reasoning_fragments);
        tracing::info!(
            session_id = %session_id,
            tool_calls = acc.all_tool_calls.len(),
            "chat.react.pause_response"
        );
        ChatAgentResponse {
            content: String::new(),
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace,
            reasoning_boundaries,
            verification_report: None,
            frontier_ceiling_reached,
            final_plan_phase,
            paused: true,
            context_window_tokens,
            context_tokens_used,
        }
    }

    /// Join the per-step reasoning fragments into the `thinking_trace` blob and
    /// return the parallel tool-call boundaries. `(None, empty)` when there is
    /// no reasoning.
    fn build_thinking_trace(fragments: &[(String, usize)]) -> (Option<String>, Vec<usize>) {
        if fragments.is_empty() {
            return (None, Vec::new());
        }
        let trace = fragments
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        let boundaries = fragments.iter().map(|(_, before)| *before).collect();
        (Some(trace), boundaries)
    }

    /// Build a paused response that freezes the partial streamed text as the
    /// assistant turn.
    ///
    /// Used when the user stops generation mid-stream: the token stream was cut
    /// inside [`consume_stream`](Self::consume_stream), the accumulated partial
    /// is carried as `content` so the manager persists it, and `paused` is set so
    /// no [`RuntimeEvent::ChatResponseCompleted`] is emitted and the session is
    /// left resumable.
    fn paused_text_response(
        &self,
        accumulated_text: &str,
        reasoning_fragments: &mut Vec<(String, usize)>,
        ctx: ResponseContext<'_>,
    ) -> ChatAgentResponse {
        let ResponseContext {
            acc,
            total_usage,
            session_id,
            frontier_ceiling_reached,
            final_plan_phase,
            context_window_tokens,
            context_tokens_used,
            ..
        } = ctx;
        let final_thinking = Self::extract_think_blocks(accumulated_text);
        let clean = Self::strip_think_blocks(accumulated_text);
        if let Some(ft) = &final_thinking {
            reasoning_fragments.push((ft.clone(), acc.all_tool_calls.len()));
        }
        let (thinking_trace, reasoning_boundaries) =
            Self::build_thinking_trace(reasoning_fragments);
        tracing::info!(
            session_id = %session_id,
            partial_len = clean.len(),
            "chat.react.pause_text_response"
        );
        ChatAgentResponse {
            content: clean,
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace,
            reasoning_boundaries,
            verification_report: None,
            frontier_ceiling_reached,
            final_plan_phase,
            paused: true,
            context_window_tokens,
            context_tokens_used,
        }
    }

    /// Build the final [`ChatAgentResponse`] when the LLM produced no tool calls.
    ///
    /// Combines the accumulated reasoning fragments with the final thinking
    /// trace and emits [`RuntimeEvent::ChatResponseCompleted`].
    fn finalize_text_response(
        &self,
        accumulated_text: &str,
        reasoning_fragments: &mut Vec<(String, usize)>,
        ctx: ResponseContext<'_>,
    ) -> ChatAgentResponse {
        let ResponseContext {
            acc,
            total_usage,
            session_id,
            message_id,
            run_id,
            frontier_ceiling_reached,
            final_plan_phase,
            context_window_tokens,
            context_tokens_used,
        } = ctx;
        // Extract thinking trace before stripping.
        let final_thinking = Self::extract_think_blocks(accumulated_text);
        let clean = Self::strip_think_blocks(accumulated_text);
        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: clean.clone(),
            run_id: Some(run_id.clone()),
        });

        // Combine accumulated reasoning fragments with final thinking.
        if let Some(ft) = &final_thinking {
            reasoning_fragments.push((ft.clone(), acc.all_tool_calls.len()));
        }
        let (thinking_trace, reasoning_boundaries) =
            Self::build_thinking_trace(reasoning_fragments);

        tracing::info!(
            fragment_count = reasoning_fragments.len(),
            has_trace = thinking_trace.is_some(),
            trace_len = thinking_trace.as_ref().map(|t| t.len()).unwrap_or(0),
            session_id = %session_id,
            "ReAct complete: thinking_trace summary"
        );

        ChatAgentResponse {
            content: clean,
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace,
            reasoning_boundaries,
            verification_report: None,
            frontier_ceiling_reached,
            final_plan_phase,
            paused: false,
            context_window_tokens,
            context_tokens_used,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GIVEN two per-step fragments captured at tool-call counts 0 and 1
    /// WHEN build_thinking_trace runs
    /// THEN the trace joins them with the separator and the boundaries are kept
    #[test]
    fn test_build_thinking_trace_carries_boundaries() {
        let fragments = vec![("plan".to_string(), 0), ("act".to_string(), 1)];
        let (trace, boundaries) = BuiltInChatAgent::build_thinking_trace(&fragments);
        assert_eq!(trace.as_deref(), Some("plan\n\n---\n\nact"));
        assert_eq!(boundaries, vec![0, 1]);
    }

    /// GIVEN no fragments
    /// WHEN build_thinking_trace runs
    /// THEN it yields no trace and empty boundaries
    #[test]
    fn test_build_thinking_trace_empty() {
        let (trace, boundaries) = BuiltInChatAgent::build_thinking_trace(&[]);
        assert!(trace.is_none());
        assert!(boundaries.is_empty());
    }
}
