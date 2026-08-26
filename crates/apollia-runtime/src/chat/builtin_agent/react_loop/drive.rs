//! The ReAct iteration itself: stream a turn, run the tool calls it asks for,
//! and start again until the model answers without one.

use super::super::stream::StreamConsumeParams;
use super::super::*;
use tracing::Instrument;

impl BuiltInChatAgent {
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
    // REASON: threads the loop's borrowed state through one iteration; a struct would borrow the same fields.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_react_loop(
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
                    "chat.budget.steps.exhausted"
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

            // Opens the iteration this request belongs to. Everything the
            // stream consumer and the tool dispatcher record from here on is
            // attributed to it.
            crate::perf_trace::iteration_begin(request.temperature, context_window_tokens);
            // Attached to the two awaits that make up an iteration rather than
            // entered as a guard: a guard held across an await would make the
            // turn future non-Send.
            let iteration_span = tracing::info_span!(
                "chat.react.iteration",
                session_id = %session_id,
                steps_left = budget.steps_left(),
            );

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
            // Time-to-first-token origin. The request body is complete here and
            // the dispatch is the next statement. The backend blocks until the
            // first token arrives, so prefill elapses inside the await below,
            // before `consume_stream` ever runs. A clock started any later
            // measures everything except the wait the user experiences.
            let dispatched_at = std::time::Instant::now();
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
                        dispatched_at,
                    },
                    &mut accumulated_text,
                    &mut chunk_usage,
                )
                .instrument(iteration_span.clone())
                .await;
            // Fold this call's usage into the exchange total, and record its
            // prompt size as the current context occupancy.
            total_usage.merge(&chunk_usage);
            if chunk_usage.prompt_tokens > 0 {
                last_prompt_tokens = chunk_usage.prompt_tokens;
            }
            // Closes the iteration. The character counts are exact; the token
            // split the record derives from them is an approximation and is
            // labelled as one.
            {
                let reasoning_chars =
                    Self::extract_think_blocks(&accumulated_text).map_or(0, |r| r.chars().count());
                let content_chars = Self::strip_think_blocks(&accumulated_text).chars().count();
                crate::perf_trace::iteration_end(
                    chunk_usage.prompt_tokens,
                    reasoning_chars,
                    content_chars,
                );
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
                    .instrument(iteration_span.clone())
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
}
