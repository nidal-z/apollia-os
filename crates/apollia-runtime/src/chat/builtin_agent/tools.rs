use super::*;

impl BuiltInChatAgent {
    /// Record one ReAct turn that produced tool calls: capture reasoning,
    /// append the assistant message, and dispatch each tool call.
    ///
    /// Updates `consecutive_tool_failures` per call: incremented on a failed
    /// call (execution error, non-zero exit code, or operator refusal), reset to
    /// 0 on the first success, so the loop can derive an escalation signal.
    // Records one tool turn from its independent signals; grouping them into a
    // struct would obscure the call site.
    // REASON: flattened fields of one tool turn, recorded into the same timeline row.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::chat::builtin_agent) async fn record_tool_turn(
        &self,
        input: RecordTurnInput<'_>,
        reasoning_fragments: &mut Vec<(String, usize)>,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
        consecutive_tool_failures: &mut u32,
    ) {
        let RecordTurnInput {
            accumulated_text,
            tool_calls,
            budget,
            ids,
            valid_tool_names,
        } = input;
        // Capture reasoning text emitted before tool calls.
        let clean_reasoning = Self::strip_think_blocks(accumulated_text);
        let reasoning_with_think = Self::extract_think_blocks(accumulated_text);
        let reasoning_text = reasoning_with_think.unwrap_or_else(|| clean_reasoning.clone());
        tracing::info!(
            accumulated_len = accumulated_text.len(),
            reasoning_len = reasoning_text.trim().len(),
            tool_count = tool_calls.len(),
            session_id = %ids.session_id,
            "chat.react.reasoning.captured"
        );
        if !reasoning_text.trim().is_empty() {
            // Captured before this step's tool calls are dispatched, so the
            // current tool-call count is the boundary that precedes it.
            reasoning_fragments.push((reasoning_text.trim().to_string(), acc.all_tool_calls.len()));
        }

        // Strip think blocks before re-injecting into the LLM context
        // so reasoning tokens don't pollute future turns.
        let clean_for_context = clean_reasoning;
        llm_messages.push(LlmChatMessage::assistant_with_calls(
            &clean_for_context,
            tool_calls,
        ));

        let session_id = ids.session_id;
        let message_id = ids.message_id;

        // PreToolUse hooks (blocking): resolve a decision per call before any
        // tool runs, so a `deny` truly prevents the invocation, including the
        // read-only calls that would otherwise execute in the parallel phase
        // below. `effective_calls` carries any rewritten arguments; `denied[i]`
        // holds the refusal reason when call `i` was blocked. With no hook
        // configured this is a borrow of the original calls with no denials, so
        // the loop behaves exactly as before.
        let pre = self
            .apply_pre_tool_use(tool_calls, session_id, ids.run_id)
            .await;
        let effective_calls: &[ToolCall] = pre.calls.as_ref();
        // A hook that replaced a call's arguments does not inherit the
        // operator's earlier "always allow" on the tool name: what was
        // authorised was the model's argument set, not a handler's
        // substitution. Such a call leaves the parallel fast path and is sent
        // through the approval flow below.
        let rewritten_by_hook = &pre.rewritten;

        // Determine read-only status for each call via the tool registry. A call
        // runs concurrently only when its tool is read-only AND already
        // authorized: execute_tool_call then touches neither llm_messages nor acc,
        // so the slow invocations overlap while results are applied in order.
        // Unknown tools (absent from the registry, e.g. hardcoded-false MCP specs)
        // are treated as write, the conservative default.
        let mut read_only: Vec<bool> = Vec::with_capacity(effective_calls.len());
        for call in effective_calls.iter() {
            let ro = self
                .tool_registry
                .describe(&call.name)
                .await
                .map(|d| d.is_read_only)
                .unwrap_or(false);
            read_only.push(ro);
        }

        // Plan-mode hard gate: before a plan is approved, refuse execution tools.
        // Only the plan_* surface, `ask_user`, and read-only tools may run, so the
        // agent must propose and submit a plan, then wait for approval, before
        // acting. The refusal reuses the PreToolUse deny path below: a synthetic
        // tool result is injected and the call never executes.
        let gate_blocks = self.plan_gate_blocks();
        let denied: Vec<Option<String>> = effective_calls
            .iter()
            .enumerate()
            .map(|(i, call)| {
                // Hook deny wins, then the executing-phase proposal refusal,
                // then an unknown (hallucinated) tool name, then the plan gate.
                // The proposal refusal must precede the unknown-name path:
                // post-approval, `plan_propose` / `plan_submit` are no longer
                // advertised, and the generic unknown-tool recovery text would
                // steer the model into rebuilding the approved plan.
                pre.denied[i]
                    .clone()
                    .or_else(|| {
                        if executing_denies_proposal(
                            self.plan_mode_active(),
                            self.session_plan_phase,
                            &call.name,
                        ) {
                            Some(PLAN_EXECUTING_PROPOSAL_DENY_REASON.to_string())
                        } else {
                            None
                        }
                    })
                    .or_else(|| unknown_tool_reason(&call.name, valid_tool_names))
                    .or_else(|| {
                        if plan_gate_denies(gate_blocks, &call.name, read_only[i]) {
                            Some(PLAN_GATE_DENY_REASON.to_string())
                        } else {
                            None
                        }
                    })
            })
            .collect();
        let denied = &denied;

        // The tool-call budget is a non-bypassable ceiling (principle #7). Bound
        // the calls this turn may execute to what the budget still allows,
        // computed before the parallel phase so read-only calls are bounded too.
        // Calls past the allowance are truncated in Phase B with a synthetic
        // result. Without this, a single turn that batches N calls could run all
        // N and overshoot max_tool_calls, since the step-boundary guard only
        // fires before the next LLM call.
        let allowed_calls = budget.tool_calls_left() as usize;

        // Phase A: invoke the parallel-safe calls concurrently, keyed by index.
        // Denied calls never run, even when read-only. Calls beyond the budget
        // allowance never run either. A persisted deny rule wins over the
        // name-only authorization: a call it matches is kept out of the fast
        // path and refused by the sequential path.
        let mut precomputed: std::collections::HashMap<usize, (ToolCallRecord, String, bool)> = {
            use futures::stream::{self, StreamExt};
            let parallel = (0..effective_calls.len())
                .filter(|&i| {
                    i < allowed_calls
                        && denied[i].is_none()
                        && !rewritten_by_hook[i]
                        && read_only[i]
                        && acc.authorized.contains(&effective_calls[i].name)
                        && !self.prefix_rule_denies(&effective_calls[i])
                })
                .map(|i| async move {
                    let outcome = self
                        .execute_tool_call(session_id, message_id, &effective_calls[i], ids.run_id)
                        .await;
                    (i, outcome)
                });
            stream::iter(parallel)
                .buffered(MAX_CONCURRENT_READONLY_TOOL_CALLS)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect()
        };

        // Phase B: apply every call in original order. Parallel-safe calls reuse
        // their precomputed result; everything else (write tools, read-only calls
        // awaiting HITL approval) goes through the sequential path.
        for (i, call) in effective_calls.iter().enumerate() {
            // Stop requested mid-turn: skip the remaining sequential calls rather
            // than run them, pairing each with a synthetic result so the frozen
            // history stays well-formed. Checked between calls, never mid-tool, so
            // no write tool is left half-applied; the react loop's next checkpoint
            // returns the paused response. Phase-A read-only calls already ran.
            if ids.cancel.is_cancelled() {
                let synthetic = "tool call skipped: generation stopped by user".to_string();
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &synthetic));
                acc.all_tool_calls.push(ToolCallRecord {
                    tool_name: call.name.clone(),
                    input: call.arguments.clone(),
                    output: Some(synthetic),
                    status: ToolCallStatus::Refused,
                    rationale: None,
                    retry_attempts: Vec::new(),
                });
                continue;
            }
            // Enforce the tool-call ceiling mid-turn: once the allowance is
            // spent, the remaining calls are not executed. A synthetic result
            // keeps each tool_call id paired with a tool_result so the model's
            // next turn sees a well-formed history.
            if i >= allowed_calls {
                let synthetic = "tool call budget exhausted".to_string();
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &synthetic));
                acc.all_tool_calls.push(ToolCallRecord {
                    tool_name: call.name.clone(),
                    input: call.arguments.clone(),
                    output: Some(synthetic),
                    status: ToolCallStatus::Refused,
                    rationale: None,
                    retry_attempts: Vec::new(),
                });
                tracing::warn!(
                    tool_name = %call.name,
                    session_id = %session_id,
                    "chat.budget.tool_calls.exhausted"
                );
                continue;
            }
            budget.increment_tool_calls();
            // PreToolUse deny: the tool is not invoked. Inject a synthetic tool
            // result so the model can react to the refusal on its next turn, and
            // do not count it as a tool failure (a deny is a policy decision, not
            // an execution failure).
            if let Some(reason) = &denied[i] {
                let synthetic = format!("tool denied: {reason}");
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &synthetic));
                acc.all_tool_calls.push(ToolCallRecord {
                    tool_name: call.name.clone(),
                    input: call.arguments.clone(),
                    output: Some(synthetic),
                    status: ToolCallStatus::Refused,
                    rationale: None,
                    retry_attempts: Vec::new(),
                });
                tracing::warn!(
                    tool_name = %call.name,
                    decision = "deny",
                    reason = %reason,
                    session_id = %session_id,
                    "hook.pretooluse.deny"
                );
                continue;
            }
            let (failed, executed) = match (call.name.as_str(), self.todo.as_ref()) {
                // todo_write is a safe built-in handled in-loop: it never goes
                // through the registry, the parallel partition, or HITL approval.
                // No PostToolUse hook fires for it.
                (TODO_WRITE_TOOL_NAME, Some(todo)) => (
                    Self::handle_todo_write(todo, session_id, call, llm_messages, acc).await,
                    None,
                ),
                // plan_* tools are safe built-ins handled in-loop, gated on plan
                // mode (a plan handle is present only in that case). They
                // delegate to the PlanHandle and inject the snapshot result;
                // they never reach the registry, parallel partition, or HITL
                // approval.
                (name, _)
                    if is_plan_tool(name) && self.plan_mode_active() && self.plan.is_some() =>
                {
                    // `plan_mode_active()` plus the `is_some()` guard guarantee a
                    // handle; `if let` binds it without an unwrap or panic.
                    if let Some(plan) = self.plan.as_ref() {
                        (
                            Self::handle_plan_tool(
                                plan,
                                session_id,
                                call,
                                llm_messages,
                                acc,
                                self.pending_injection.as_ref(),
                            )
                            .await,
                            None,
                        )
                    } else {
                        (false, None)
                    }
                }
                _ => match precomputed.remove(&i) {
                    Some((record, tool_result, success)) => {
                        llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                        acc.all_tool_calls.push(record);
                        (!success, Some((tool_result, success)))
                    }
                    None => {
                        let outcome = self
                            .process_tool_call(
                                ToolCallContext {
                                    session_id,
                                    message_id,
                                    call,
                                    run_id: ids.run_id,
                                    pending_approvals: ids.pending_approvals,
                                    rewritten_by_hook: rewritten_by_hook[i],
                                },
                                llm_messages,
                                acc,
                            )
                            .await;
                        (outcome.failed, outcome.executed)
                    }
                },
            };

            // PostToolUse (non-blocking, best-effort): fires only when the tool
            // actually ran. A returned injection is appended as a system message
            // so the model sees it on the next turn.
            if let Some((output, success)) = executed {
                if let Some(injection) = self
                    .fire_post_tool_use(&call.name, &output, success, session_id)
                    .await
                {
                    llm_messages.push(LlmChatMessage::system(injection));
                }
            }

            *consecutive_tool_failures = next_failure_count(*consecutive_tool_failures, failed);
        }
    }

    /// Run the `todo_write` built-in tool inside the ReAct loop.
    ///
    /// Persists the agent-provided list via the [`TodoHandle`] and injects the
    /// JSON result as the tool message. Returns `true` when the write failed
    /// (invariant violation or malformed payload) so the loop counts it toward
    /// escalation; the loop itself never stops on a todo error.
    async fn handle_todo_write(
        todo: &TodoHandle,
        session_id: &str,
        call: &ToolCall,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) -> bool {
        let result = run_todo_write(todo, session_id, &call.arguments).await;
        let item_count = result.count.unwrap_or(0);
        tracing::info!(
            session_id = %session_id,
            item_count,
            ok = result.ok,
            "chat.todo_write.applied"
        );
        let tool_result = serde_json::to_string(&result).unwrap_or_else(|_| {
            r#"{"ok":false,"error":"todo result serialization failed"}"#.to_string()
        });
        llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
        acc.all_tool_calls.push(ToolCallRecord {
            tool_name: call.name.clone(),
            input: call.arguments.clone(),
            output: Some(tool_result),
            status: ToolCallStatus::from_success(result.ok),
            rationale: None,
            retry_attempts: Vec::new(),
        });
        !result.ok
    }

    /// Execute a single tool call via the [`ToolInvoker`], emitting events.
    /// Process one tool call: run it directly when authorized, otherwise go
    /// through the HITL approval flow. Mutates `llm_messages` and `acc`.
    ///
    /// Returns a [`ToolCallOutcome`]: `failed` for the escalation counter, and
    /// the executed output when the tool actually ran (for the `PostToolUse`
    /// hook). A refusal yields `failed = true` with no executed output.
    async fn process_tool_call(
        &self,
        ctx: ToolCallContext<'_>,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) -> ToolCallOutcome {
        let ToolCallContext {
            session_id,
            message_id,
            call,
            run_id,
            pending_approvals,
            rewritten_by_hook,
        } = ctx;

        // The persisted prefix rules are evaluated first, against this call's
        // argument, so a deny rule wins even over a name-authorized tool: an
        // operator's standing refusal cannot be bypassed by a broader "always
        // allow". The grant side is per invocation, so an allow is
        // deliberately NOT inserted into `acc.authorized`: the next call
        // re-evaluates with its own argument. A code executor can only match
        // through the strict matcher (prefix plus single simple command),
        // never a blanket rule.
        let rule_hit = self.prefix_rule_decision(call);

        if let Some((rule_id, apollia_permissions::prefix_rule_engine::RuleAction::Deny)) = rule_hit
        {
            tracing::info!(
                tool = %call.name,
                rule_id,
                "chat.tool.prefix_rule_denied"
            );
            let refusal = "Tool refused by a permission rule".to_string();
            llm_messages.push(LlmChatMessage::tool_result(&call.id, &refusal));
            acc.all_tool_calls.push(ToolCallRecord {
                tool_name: call.name.clone(),
                input: call.arguments.clone(),
                output: Some(refusal),
                status: ToolCallStatus::Refused,
                rationale: None,
                retry_attempts: Vec::new(),
            });
            return ToolCallOutcome {
                failed: true,
                executed: None,
            };
        }

        let rule_allows = matches!(
            rule_hit,
            Some((
                _,
                apollia_permissions::prefix_rule_engine::RuleAction::Allow
            ))
        );
        if !rewritten_by_hook && (acc.authorized.contains(&call.name) || rule_allows) {
            if rule_allows && !acc.authorized.contains(&call.name) {
                if let Some((rule_id, _)) = rule_hit {
                    tracing::info!(
                        tool = %call.name,
                        rule_id,
                        "chat.tool.prefix_rule_allowed"
                    );
                }
            }
            let (record, tool_result, success) = self
                .execute_tool_call(session_id, message_id, call, run_id)
                .await;
            llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
            acc.all_tool_calls.push(record);
            return ToolCallOutcome {
                failed: !success,
                executed: Some((tool_result, success)),
            };
        }

        // HITL approval. The key is scoped by the unique tool-call id (not the
        // tool name) so the same tool invoked twice in one turn gets two
        // distinct pending slots: without it, the first call's timeout task
        // would evict the second call's live approval under a shared key.
        let key = format!("{session_id}::{message_id}::{}", call.id);
        let input_preview =
            truncate_preview(&serde_json::to_string(&call.arguments).unwrap_or_default());

        let _ = self.event_bus.send(RuntimeEvent::ChatApprovalRequired {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            prompt: format!("Tool '{}' asks to run with: {}", call.name, input_preview),
        });

        let rx = pending_approvals.register(key.clone());
        pending_approvals.start_timeout(ApprovalTimeoutParams {
            key,
            duration: CHAT_APPROVAL_TIMEOUT,
            event_bus: self.event_bus.clone(),
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
        });
        // The human wait starts here, not inside the invoker. A refusal, whether
        // typed or reached by timeout, runs no tool at all, so this is the only
        // point at which the wait can be attributed to anything.
        let approval_started = std::time::Instant::now();
        let decision = rx.await.unwrap_or(ToolDecision::refuse());
        crate::perf_trace::approval_resolved(
            &call.name,
            approval_started.elapsed().as_secs_f64() * 1000.0,
            !matches!(decision, ToolDecision::Refuse { .. }),
        );

        self.apply_tool_decision(
            ToolExecTarget {
                session_id,
                message_id,
                call,
                run_id,
            },
            decision,
            llm_messages,
            acc,
        )
        .await
    }

    /// Apply the operator's HITL decision for an unauthorized tool call.
    ///
    /// Returns a [`ToolCallOutcome`]: `failed` is set on an execution failure or
    /// a refusal; `executed` carries the output when the tool ran, enabling the
    /// `PostToolUse` hook.
    async fn apply_tool_decision(
        &self,
        target: ToolExecTarget<'_>,
        decision: ToolDecision,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) -> ToolCallOutcome {
        let ToolExecTarget {
            session_id,
            message_id,
            call,
            run_id,
        } = target;
        match decision {
            ToolDecision::Accept => {
                let (record, tool_result, success) = self
                    .execute_tool_call(session_id, message_id, call, run_id)
                    .await;
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                acc.all_tool_calls.push(record);
                ToolCallOutcome {
                    failed: !success,
                    executed: Some((tool_result, success)),
                }
            }
            ToolDecision::AlwaysAccept { .. } => {
                acc.authorized.insert(call.name.clone());
                acc.newly_authorized.push(call.name.clone());
                let (record, tool_result, success) = self
                    .execute_tool_call(session_id, message_id, call, run_id)
                    .await;
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                acc.all_tool_calls.push(record);
                ToolCallOutcome {
                    failed: !success,
                    executed: Some((tool_result, success)),
                }
            }
            ToolDecision::Refuse { reason } => {
                // The reason carries the operator's intent (e.g. "wrong
                // directory"), surface it to the LLM so it can correct course
                // on the next iteration instead of retrying blind.
                let refusal = match &reason {
                    Some(r) => format!("Tool refused by the operator. Reason: {r}"),
                    None => "Tool refused by the operator".to_string(),
                };
                llm_messages.push(LlmChatMessage::tool_result(&call.id, &refusal));
                acc.all_tool_calls.push(ToolCallRecord {
                    tool_name: call.name.clone(),
                    input: call.arguments.clone(),
                    output: Some(refusal),
                    status: ToolCallStatus::Refused,
                    rationale: None,
                    retry_attempts: Vec::new(),
                });
                ToolCallOutcome {
                    failed: true,
                    executed: None,
                }
            }
        }
    }

    /// Evaluate the persisted prefix rules for one call, if a checker is
    /// attached. Returns the matched rule id and action, `None` otherwise.
    fn prefix_rule_decision(
        &self,
        call: &apollia_llm::types::ToolCall,
    ) -> Option<(i64, apollia_permissions::prefix_rule_engine::RuleAction)> {
        let checker = self.prefix_checker.as_ref()?;
        let first_arg = apollia_permissions::extract_first_arg(&call.arguments);
        checker(&call.name, first_arg.as_deref())
    }

    /// Whether a persisted deny rule matches this call. Used by the parallel
    /// read-only fast path to keep rule-denied calls on the sequential path,
    /// where the refusal is synthesized.
    fn prefix_rule_denies(&self, call: &apollia_llm::types::ToolCall) -> bool {
        matches!(
            self.prefix_rule_decision(call),
            Some((_, apollia_permissions::prefix_rule_engine::RuleAction::Deny))
        )
    }

    async fn execute_tool_call(
        &self,
        session_id: &str,
        message_id: &str,
        call: &apollia_llm::types::ToolCall,
        run_id: &RunId,
    ) -> (ToolCallRecord, String, bool) {
        let input_preview =
            truncate_preview(&serde_json::to_string(&call.arguments).unwrap_or_default());

        // Generate the opt-in rationale *before* execution so the UI can
        // surface it immediately. Falls back to `None` when the meta handle
        // is absent, the routine is disabled, the budget is exhausted, or
        // the call fails / times out (see MetaOrchestratorHandle docs).
        let rationale = if let Some(handle) = self.meta_handle.as_ref() {
            handle
                .generate_tool_call_rationale(
                    &call.name,
                    &call.arguments,
                    "",
                    session_id.to_string(),
                )
                .await
        } else {
            None
        };

        let _ = self.event_bus.send(RuntimeEvent::ChatToolCallStarted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: call.name.clone(),
            input_preview,
            rationale: rationale.clone(),
        });

        let invoke_started = std::time::Instant::now();
        let result = self.tool_invoker.invoke(&call.name, &call.arguments).await;
        // Dispatch to result available, and nothing else. Any HITL approval was
        // awaited upstream of this call and is recorded there as its own sample,
        // so this span carries tool work only. An earlier comment here claimed
        // the approval wait was inside the invoker and therefore inside this
        // span, which was wrong in both halves.
        crate::perf_trace::tool_completed(
            &call.name,
            invoke_started.elapsed().as_secs_f64() * 1000.0,
            None,
        );
        let (output, success) = match result {
            Ok(s) => {
                // Detect tool-reported failures (e.g. bash_executor with exit_code != 0)
                let tool_failed = serde_json::from_str::<serde_json::Value>(&s)
                    .ok()
                    .and_then(|v| v.get("exit_code")?.as_i64())
                    .is_some_and(|code| code != 0);
                (s, !tool_failed)
            }
            Err(e) => {
                warn!(tool = %call.name, error = %e, "tool.call.failed");
                (format!("tool error: {e}"), false)
            }
        };

        let output_preview = truncate_preview(&output);

        // Static analysis (always-on): run the static error classifier (on
        // failure) and the hallucination heuristic (on every output).
        // Opt-in: when the
        // analysis falls back to `Unknown`, ask the meta-LLM to humanise
        // the message via `MetaRoutine::GenerateErrorExplanation`.
        let analysis = self
            .build_error_analysis(session_id, &call.name, &output, success)
            .await;
        let _ = self.event_bus.send(RuntimeEvent::ChatToolCallCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: call.name.clone(),
            success,
            output_preview: Some(output_preview),
            analysis,
        });

        // Capture the full tool output for deterministic replay. The output is
        // stored verbatim (JSON string when it is not itself JSON), distinct
        // from the truncated preview above.
        let captured_output = serde_json::from_str::<serde_json::Value>(&output)
            .unwrap_or_else(|_| serde_json::Value::String(output.clone()));
        let _ = self.event_bus.send(RuntimeEvent::ToolOutputCaptured {
            run_id: run_id.clone(),
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            output: captured_output,
            status: if success { "success" } else { "error" }.to_string(),
        });

        let record = ToolCallRecord {
            tool_name: call.name.clone(),
            input: call.arguments.clone(),
            output: Some(output.clone()),
            // The same verdict the event above carries. Persisting `Executed`
            // unconditionally is what made a failed call render as a success
            // once the turn finalized.
            status: ToolCallStatus::from_success(success),
            rationale,
            retry_attempts: Vec::new(),
        };

        // Truncate output for LLM context to avoid flooding the context window.
        // The full output is preserved in the ToolCallRecord for history/UI.
        let llm_output = truncate_tool_output(&output);

        (record, llm_output, success)
    }
}
