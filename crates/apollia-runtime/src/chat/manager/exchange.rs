use super::*;

impl ChatSessionManager {
    /// Send a user message in a session.
    pub(in crate::chat::manager) fn handle_send_message(
        &mut self,
        session_id: &str,
        content: &str,
    ) -> Result<MessageId, ChatError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        if session.status == SessionStatus::Closed {
            return Err(ChatError::SessionClosed(session_id.to_string()));
        }

        if session.status == SessionStatus::Processing {
            return Err(ChatError::SessionBusy(session_id.to_string()));
        }

        let message_id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();

        // Persist message to SQLite
        let seq = self.repository.append_message(&AppendMessageParams {
            id: &message_id,
            session_id,
            role: &ChatRole::User,
            content,
            tool_calls_json: None,
            tool_name: None,
            created_at: &now,
            metadata: None,
        })?;

        // Add to in-memory history
        let msg = ChatMessage {
            id: message_id.clone(),
            role: ChatRole::User,
            content: content.to_string(),
            tool_calls: None,
            tool_name: None,
            created_at: now.clone(),
            seq,
            metadata: None,
        };
        session.history.push(msg);

        // Set session to Processing. A fresh run_id correlates every event
        // emitted during this exchange (one user turn, one response cycle).
        session.status = SessionStatus::Processing;
        let run_id = RunId::new();
        session.active_exchange = Some(ExchangeState {
            message_id: message_id.clone(),
            started_at: now,
            run_id: run_id.clone(),
        });
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Processing)
        {
            warn!(error = %e, "chat.session.status.persist.failed");
        }

        // Emit event
        let _ = self.event_bus.send(RuntimeEvent::ChatMessageSent {
            session_id: session_id.to_string(),
            message_id: message_id.clone(),
        });

        // Launch BuiltInChatAgent in a background task for Libre mode.
        // For Agent mode, a different path will be used.
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;

        // The agent path clones the full session, which already carries `run_id`
        // in `active_exchange`; only the Libre path needs it threaded explicitly.
        let dispatch = if session.mode == ChatMode::Libre || session.mode == ChatMode::Companion {
            self.dispatch_libre_exchange(session_id, &message_id, content, &run_id)
        } else {
            self.dispatch_agent_exchange(session_id, &message_id, content)
        };

        // A dispatch failure (e.g. no LLM configured) must not leave the session
        // stuck in Processing forever: reset it to Active, clear the exchange and
        // persist before surfacing the error, so the next send is accepted.
        if let Err(e) = dispatch {
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.status = SessionStatus::Active;
                session.active_exchange = None;
            }
            if let Err(persist_err) = self
                .repository
                .update_status(session_id, &SessionStatus::Active)
            {
                warn!(error = %persist_err, "chat.session.status.reset.failed");
            }
            return Err(e);
        }

        Ok(message_id)
    }

    /// Regenerate the assistant reply to the last user turn (truncate-in-place).
    ///
    /// Locates the user turn that `message_id` (an assistant message) answered,
    /// drops that user turn's reply and everything after it from both SQLite and
    /// the in-memory history, then replays the turn on the shortened history. The
    /// session id is unchanged (ChatGPT/Claude style), not forked.
    pub(in crate::chat::manager) fn handle_regenerate_response(
        &mut self,
        session_id: &str,
        message_id: &str,
    ) -> Result<(), ChatError> {
        let (user_id, user_seq, user_content) = {
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
            if session.status == SessionStatus::Closed {
                return Err(ChatError::SessionClosed(session_id.to_string()));
            }
            if session.status == SessionStatus::Processing {
                return Err(ChatError::SessionBusy(session_id.to_string()));
            }
            let target_idx = session
                .history
                .iter()
                .position(|m| m.id == message_id)
                .ok_or_else(|| {
                    ChatError::InternalError(format!("message not found: {message_id}"))
                })?;
            let user_msg = session.history[..target_idx]
                .iter()
                .rev()
                .find(|m| m.role == ChatRole::User)
                .ok_or_else(|| {
                    ChatError::InternalError("no user turn to regenerate".to_string())
                })?;
            (user_msg.id.clone(), user_msg.seq, user_msg.content.clone())
        };

        // Delete everything after the user turn (its reply + any later messages),
        // keeping the user message itself as the trailing prompt.
        self.repository
            .truncate_messages_from_seq(session_id, user_seq, false)?;
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.history.retain(|m| m.seq <= user_seq);
        }

        self.rerun_turn(session_id, &user_id, &user_content)
    }

    /// Replace a user message and re-run from it (truncate-in-place).
    ///
    /// Truncates the edited user message and everything after it, then sends
    /// `content` as a fresh user turn through the normal send path.
    pub(in crate::chat::manager) fn handle_edit_and_resend(
        &mut self,
        session_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<MessageId, ChatError> {
        let seq = {
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
            if session.status == SessionStatus::Closed {
                return Err(ChatError::SessionClosed(session_id.to_string()));
            }
            if session.status == SessionStatus::Processing {
                return Err(ChatError::SessionBusy(session_id.to_string()));
            }
            let msg = session
                .history
                .iter()
                .find(|m| m.id == message_id)
                .ok_or_else(|| {
                    ChatError::InternalError(format!("message not found: {message_id}"))
                })?;
            if msg.role != ChatRole::User {
                return Err(ChatError::InternalError(
                    "can only edit a user message".to_string(),
                ));
            }
            msg.seq
        };

        // Delete the edited message and everything after it, then re-send.
        self.repository
            .truncate_messages_from_seq(session_id, seq, true)?;
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.history.retain(|m| m.seq < seq);
        }

        self.handle_send_message(session_id, content)
    }

    /// Re-run a turn on the existing history without appending a new user
    /// message.
    ///
    /// Shared tail of [`handle_regenerate_response`](Self::handle_regenerate_response):
    /// the caller has already truncated the history so it ends with
    /// `user_message_id`. Mirrors the dispatch half of
    /// [`handle_send_message`](Self::handle_send_message) (fresh run_id,
    /// Processing status, fresh pause token created inside the dispatch) but
    /// reuses the already-persisted user turn as the prompt.
    fn rerun_turn(
        &mut self,
        session_id: &str,
        user_message_id: &str,
        user_content: &str,
    ) -> Result<(), ChatError> {
        let now = now_rfc3339();
        let run_id = RunId::new();
        {
            let session = self
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;
            session.status = SessionStatus::Processing;
            session.active_exchange = Some(ExchangeState {
                message_id: user_message_id.to_string(),
                started_at: now,
                run_id: run_id.clone(),
            });
        }
        if let Err(e) = self
            .repository
            .update_status(session_id, &SessionStatus::Processing)
        {
            warn!(error = %e, "chat.session.status.persist.failed");
        }

        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;
        let dispatch = if session.mode == ChatMode::Libre || session.mode == ChatMode::Companion {
            self.dispatch_libre_exchange(session_id, user_message_id, user_content, &run_id)
        } else {
            self.dispatch_agent_exchange(session_id, user_message_id, user_content)
        };

        if let Err(e) = dispatch {
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.status = SessionStatus::Active;
                session.active_exchange = None;
            }
            if let Err(persist_err) = self
                .repository
                .update_status(session_id, &SessionStatus::Active)
            {
                warn!(error = %persist_err, "chat.session.status.reset.failed");
            }
            return Err(e);
        }

        Ok(())
    }

    /// Build the system prompt for a Libre/Companion exchange, optionally
    /// enriched with cross-session context on the first message.
    fn build_libre_system_prompt(&self, base_prompt: &str, content: &str, enrich: bool) -> String {
        let mut prompt = base_prompt.to_string();
        if enrich {
            if let Some(block) = self.build_cross_session_context(content) {
                prompt.push_str("\n\n");
                prompt.push_str(&block);
            }
        }
        prompt
    }

    /// Spawn the BuiltInChatAgent background task for a Libre/Companion exchange.
    fn dispatch_libre_exchange(
        &mut self,
        session_id: &str,
        message_id: &str,
        content: &str,
        run_id: &RunId,
    ) -> Result<(), ChatError> {
        let message_id = message_id.to_string();
        {
            let llm_router = self.llm_router.clone().ok_or(ChatError::NoLlmConfigured)?;
            // Read and consume the late-link injection flag, it triggers a
            // single re-injection of the project context on the next message
            // after a session was linked to a project mid-conversation.
            let force_inject_project_context = {
                let s_mut = self
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;
                let v = s_mut.force_project_context_inject;
                s_mut.force_project_context_inject = false;
                v
            };

            // Fresh cooperative pause token for this turn. A clone is threaded into
            // the ReAct loop; `pause_session` cancels the copy stored here so the
            // loop stops at its next checkpoint. The session starts Running.
            let cancel = CancellationToken::new();
            self.pause_tokens
                .insert(session_id.to_string(), cancel.clone());
            self.pause_states
                .insert(session_id.to_string(), PauseState::Running);

            // Take any operator instruction queued while the session was paused.
            // It is consumed exactly once, on this turn.
            let pending_injection = self.pending_injections.remove(session_id);

            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;

            // Companion sessions are intentionally isolated from user memory and
            // cross-session history (memory stays at the agent's initiative).
            // The companion helps with the platform, not the user's personal context.
            let session_user_memory = if session.mode == ChatMode::Companion {
                None
            } else {
                self.user_memory.clone()
            };

            // `session.history` already ends with the in-flight user message
            // (pushed before dispatch). The agent re-adds it from `user_msg`,
            // so the history handed to the agent drops that trailing entry to
            // keep a single user turn in the LLM prompt.
            let is_first_message = session.history.len() == 1;
            let history: Vec<ChatMessage> =
                session.history[..session.history.len().saturating_sub(1)].to_vec();
            let session_plan_mode = session.plan_mode;
            let session_plan_phase = session.plan_phase;
            let is_companion = session.mode == ChatMode::Companion;
            let inject_project_context = is_first_message || force_inject_project_context;
            // On the first message, enrich the system prompt with cross-session context.
            // Companion sessions are excluded, they must not inherit personal history.
            let system_prompt = self.build_libre_system_prompt(
                &session.system_prompt,
                content,
                is_first_message && !is_companion,
            );
            let available_tools = session.available_tools.clone();
            // Live merge: governance.db is re-read on every message so Apollia
            // Chat config changes (auto-authorized tools, agent-scoped rules)
            // apply to already-open Libre sessions without closing/reopening the
            // conversation. The merge is purely additive: an authorization
            // granted during the session is never removed, only added to if the
            // config has been enriched since.
            let authorized_tools = merge_live_authorized_tools(
                &session.authorized_tools,
                &session.mode,
                self.governance_db_path.as_deref(),
            );
            let pending_approvals = self.pending_chat_approvals.clone();

            // Resolve the autonomy tier and build the effective budget from it.
            // `runtime_budget` is the ceiling: `from_capped` guarantees no tier
            // raises the budget above it (principle #7). The verification loop and
            // critic are constructed only when the tier requests verification;
            // Chat Libre declares no manifest check commands, so the loop is
            // critic-driven (empty command list).
            let autonomy_config = AutonomyConfig::default();
            let autonomy_level = autonomy_config.default_level;
            let level_config = autonomy_config.level_config(autonomy_level);
            let budget = StepBudget::from_capped(&level_config.budget, &self.runtime_budget);
            let (verification, critic) = if level_config.run_verification {
                (
                    Some(VerificationLoop::new(Vec::new(), Vec::new())),
                    Some(CriticPass::new(llm_router.clone())),
                )
            } else {
                (None, None)
            };
            let sid = session_id.to_string();
            let mid = message_id.clone();
            let user_msg = content.to_string();
            let tx = self.tx.clone();
            let context_window_size = DEFAULT_CONTEXT_WINDOW_SIZE;

            let stored_summary = self.repository.get_summary(session_id).unwrap_or(None);
            let llm_for_summarize = self.llm_router.clone();

            // Capture project context and repo for async injection in spawned task.
            // The invoker is created per-session inside the task.
            let project_ctx = self.project_context.clone();
            let session_project_id = session.project_id.clone();
            let project_repo_for_session = self.project_repo.clone();
            let a2a_for_agent = self.a2a_invoker.clone();
            let tool_registry = self.tool_registry.clone();
            let event_bus = self.event_bus.clone();

            let pending_user_inputs_for_session = self.pending_user_inputs.clone();
            let mcp_handle_for_session = self.mcp_handle.clone();
            let chat_tools_config_for_session = self.chat_tools_config.clone();
            let mcp_loading = self.mcp_loading;
            let tool_search_limit = self.tool_search_limit;
            let session_id_str = session_id.to_string();

            // Capture HITL filesystem params for the invoker.
            let hitl_params = HitlInvokerParams {
                session_id: session_id.to_string(),
                event_bus: self.event_bus.clone(),
                pending_fs: self.pending_fs_approvals.clone(),
                fs_allow_rules: std::sync::Arc::clone(&session.fs_allow_rules),
                risk_config: apollia_core::FilesystemRiskConfig::default(),
            };

            tokio::spawn(run_libre_exchange(LibreExchangeParams {
                llm_router,
                tool_registry,
                event_bus,
                a2a_for_agent,
                session_user_memory,
                pending_approvals,
                budget,
                autonomy_level,
                level_config,
                verification,
                critic,
                history,
                available_tools,
                authorized_tools,
                system_prompt,
                inject_project_context,
                is_companion,
                context_window_size,
                stored_summary,
                llm_for_summarize,
                project_ctx,
                session_project_id,
                project_repo_for_session,
                pending_user_inputs_for_session,
                mcp_handle_for_session,
                chat_tools_config_for_session,
                mcp_loading,
                tool_search_limit,
                session_id_str,
                hitl_params,
                sid,
                mid,
                run_id: run_id.clone(),
                user_msg,
                tx,
                todo: self.todo_handle.clone(),
                plan: self.plan_handle.clone(),
                session_plan_mode,
                session_plan_phase,
                hook_executor: self.hook_executor.clone(),
                cancel,
                pending_injection,
            }));
        }

        Ok(())
    }

    /// Dispatch an Agent-mode exchange to the [`AgentChatExecutor`].
    fn dispatch_agent_exchange(
        &mut self,
        session_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ChatError::InternalError("session vanished".into()))?;

        // Agent mode: dispatch to AgentChatExecutor.
        let agent_runner = match self.agent_runner.clone() {
            Some(r) => r,
            None => {
                warn!(session_id = %session_id, "chat.agent.runner.missing");
                if let Some(s) = self.sessions.get_mut(session_id) {
                    s.status = SessionStatus::Active;
                    s.active_exchange = None;
                }
                if let Err(e) = self
                    .repository
                    .update_status(session_id, &SessionStatus::Active)
                {
                    warn!(error = %e, "chat.session.status.reset.failed");
                }
                return Err(ChatError::AgentLoadFailed(
                    "no ChatAgentRunner configured - Agent mode unavailable".into(),
                ));
            }
        };

        let executor = AgentChatExecutor::new(agent_runner, self.event_bus.clone());
        let session_clone = session.clone();
        // A code executor is never pre-authorized (HITL always fires); drop any
        // (e.g. legacy chat.db) entry before it can skip approval in the loop.
        let mut authorized = session.authorized_tools.clone();
        authorized.retain(|tool| !apollia_permissions::is_code_executor(tool));
        let pending = self.pending_chat_approvals.clone();
        let sid = session_id.to_string();
        let mid = message_id.to_string();
        let user_msg = content.to_string();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            let result = executor
                .execute(AgentChatRequest {
                    session: &session_clone,
                    user_message: &user_msg,
                    message_id: &mid,
                    authorized_tools: &authorized,
                    pending_approvals: &pending,
                })
                .await;

            let cmd = match result {
                Ok(response) => ChatCommand::ExchangeComplete {
                    session_id: sid,
                    message_id: mid,
                    response,
                },
                Err(err) => ChatCommand::ExchangeError {
                    session_id: sid,
                    message_id: mid,
                    error: err.to_string(),
                },
            };
            let _ = tx.send(cmd).await;
        });

        Ok(())
    }

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

    /// Resolve a pending tool approval.
    // The session/message/tool-call/tool identifiers plus the decision map
    // one-to-one onto the resolve request; grouping them into a struct would
    // only add indirection.
    // REASON: flattened fields of one resolve-tool message, handled in one place.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::chat::manager) fn handle_resolve_tool(
        &mut self,
        session_id: &str,
        message_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        decision: ToolDecision,
    ) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        let key = format!("{session_id}::{message_id}::{tool_call_id}");
        let resolved = self.pending_chat_approvals.resolve(&key, decision.clone());

        if !resolved {
            return Err(ChatError::SessionNotFound(format!(
                "no pending approval for {key}"
            )));
        }

        // AlwaysAccept: dispatch persistence by scope.
        // - ThisTool / ThisSession: in-memory only (session.authorized_tools).
        // - ThisAgent  : scope='agent' rule in governance.db (agent_id derived from the mode).
        // - ThisProject: scope='project' rule in governance.db (workspace_path of the current project).
        // - Global     : scope='global' rule in governance.db.
        if let ToolDecision::AlwaysAccept { scope } = &decision {
            // A code executor (bash/python) is never blanket-authorized: the
            // current call is still approved once (the pending request was
            // resolved above), but "always" is downgraded to a one-time approval
            // so the next invocation asks again. Closes the in-session branch of
            // the "always allow bash = blank check" finding.
            if apollia_permissions::is_code_executor(tool_name) {
                warn!(
                    tool = %tool_name,
                    detail = "code executor,
                    treated as a one-time approval",
                    "chat.approval.always_accept.downgraded"
                );
            } else {
                // Always update the current session (immediate authorization).
                session.authorized_tools.insert(tool_name.to_string());

                // Capture the scope-resolution inputs before releasing the session
                // borrow, so governance.db persistence can use a disjoint &self.
                let session_mode = session.mode.clone();
                let session_agent_name = session.agent_name.clone();
                let session_project_id = session.project_id.clone();

                // chat.db.authorized_tools: written to preserve the authorization if
                // the runtime crashes mid-session. Kept for the ThisTool/ThisSession
                // scopes (otherwise they would be lost on restart). For the
                // persistent scopes it is redundant with governance.db but has no
                // side effect, to be cleaned up later.
                let now = now_rfc3339();
                if let Err(e) = self.repository.authorize_tool(session_id, tool_name, &now) {
                    warn!(error = %e, "chat.authorization.persist.failed");
                }

                self.persist_always_accept_scope(AlwaysAcceptScopeCtx {
                    scope,
                    session_mode,
                    session_agent_name,
                    session_project_id: session_project_id.as_deref(),
                    session_id,
                    tool_name,
                });
            }
        }

        // Trace-log the enriched metadata (reason / scope) without breaking
        // the existing `log_tool_approval` SQL schema.
        log_resolution_metadata(&decision, session_id, message_id, tool_name);

        let decision_str = decision.as_str();
        let reason: Option<&str> = match &decision {
            ToolDecision::Refuse { reason: Some(r) } => Some(r.as_str()),
            _ => None,
        };

        let resolved_at = now_rfc3339();

        // Persist decision in approval log for history view.
        if let Err(e) = self.repository.log_tool_approval(ToolApprovalLogEntry {
            session_id,
            message_id,
            tool_name,
            decision: decision_str,
            resolved_at: &resolved_at,
            reason,
        }) {
            warn!(error = %e, "chat.approval.log.persist.failed");
        }

        let _ = self.event_bus.send(RuntimeEvent::ChatApprovalResolved {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_call_id: tool_call_id.to_string(),
            tool_name: tool_name.to_string(),
            decision: decision_str.to_string(),
        });

        Ok(())
    }

    /// Persist an `AlwaysAccept` rule in governance.db according to its scope.
    ///
    /// `ThisTool` / `ThisSession` are in-session only (no governance write).
    /// `ThisAgent` derives the agent id from the session mode; `ThisProject`
    /// resolves the workspace path from the project repository.
    fn persist_always_accept_scope(&self, ctx: AlwaysAcceptScopeCtx<'_>) {
        let AlwaysAcceptScopeCtx {
            scope,
            session_mode,
            session_agent_name,
            session_project_id,
            session_id,
            tool_name,
        } = ctx;
        use super::super::types::AlwaysAcceptScope;
        match scope {
            AlwaysAcceptScope::ThisTool | AlwaysAcceptScope::ThisSession => {
                // No governance.db persistence, purely in-session.
            }
            AlwaysAcceptScope::ThisAgent => {
                let agent_id = match session_mode {
                    ChatMode::Libre | ChatMode::Companion => {
                        Some(APOLLIA_CHAT_AGENT_ID.to_string())
                    }
                    ChatMode::Agent => session_agent_name,
                };
                if let Some(aid) = agent_id {
                    persist_chat_allow_rule(
                        apollia_permissions::PermissionScope::Agent,
                        None,
                        Some(aid),
                        tool_name,
                        self.governance_db_path.as_deref(),
                    );
                }
            }
            AlwaysAcceptScope::ThisProject => {
                match self.resolve_project_workspace(session_project_id) {
                    Some(ws) => {
                        persist_chat_allow_rule(
                            apollia_permissions::PermissionScope::Project,
                            Some(ws),
                            None,
                            tool_name,
                            self.governance_db_path.as_deref(),
                        );
                    }
                    None => {
                        warn!(
                            session_id,
                            tool_name,
                            detail = "no resolvable workspace_path, session-only authorization",
                            "chat.approval.scope.downgraded"
                        );
                    }
                }
            }
            AlwaysAcceptScope::Global => {
                persist_chat_allow_rule(
                    apollia_permissions::PermissionScope::Global,
                    None,
                    None,
                    tool_name,
                    self.governance_db_path.as_deref(),
                );
            }
        }
    }
}
