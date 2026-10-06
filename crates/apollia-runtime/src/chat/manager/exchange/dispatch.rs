//! Dispatch of one exchange to the backend that will run it.
//!
//! Two shapes share the entry: a free chat driven by the built-in agent, and
//! an agent-backed session routed through the coordinator.

use super::super::*;

impl ChatSessionManager {
    /// Spawn the agent executor background task for a Libre/Companion
    /// exchange, or for an agent session on `declarative`, a declarative
    /// agent as resolved for this message.
    // REASON: the resolved agent joins the four identifiers of the exchange; a
    // struct would carry the same five values one call further.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn dispatch_libre_exchange(
        &mut self,
        session_id: &str,
        message_id: &str,
        content: &str,
        run_id: &RunId,
        declarative: Option<apollia_tools::ResolvedAgent>,
    ) -> Result<(), ChatError> {
        let message_id = message_id.to_string();
        if let Some(agent) = &declarative {
            super::super::declarative::require_enabled(agent)?;
        }
        {
            let llm_router = self.llm_router.clone().ok_or(ChatError::NoLlmConfigured)?;
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

            // `session.history` already ends with the in-flight user message
            // (pushed before dispatch). The agent re-adds it from `user_msg`,
            // so the history handed to the agent drops that trailing entry to
            // keep a single user turn in the LLM prompt.
            //
            // A continued answer is still in the history, after the user
            // message it answers: both are dropped, the user message comes
            // back from `user_msg` and the answer as the prefill.
            let answer_prefill = self.answer_prefills.remove(session_id);
            let answered = session
                .history
                .len()
                .saturating_sub(usize::from(answer_prefill.is_some()));
            let is_first_message = session.history.len() == 1;
            let history: Vec<ChatMessage> = session.history[..answered.saturating_sub(1)].to_vec();
            // The conversation that creates an agent: a free chat created with
            // the draft tool in its list. It drafts and does nothing else, so
            // it never plans.
            let agent_draft = declarative.is_none()
                && session.mode == ChatMode::Libre
                && crate::chat::agent_draft_tool::is_draft_session(&session.available_tools);
            let session_plan_mode = session.plan_mode && !agent_draft;
            let turn_id = session
                .active_exchange
                .as_ref()
                .and_then(|exchange| exchange.answer_id.clone());
            let session_plan_phase = session.plan_phase;
            // A model chosen for the session wins over the agent's own: the
            // agent stays the same, the next turns run on the chosen model.
            let preferred_backend = match &declarative {
                Some(agent) => session
                    .llm_backend
                    .clone()
                    .or_else(|| agent.model.as_ref().map(|m| m.model_id.clone())),
                None => session.llm_backend.clone(),
            };
            // A declarative agent is isolated as the companion is: no
            // principle 6 injection, no prefix rule of Apollia.
            // So is the conversation that creates an agent: it describes an
            // agent, not the person.
            let is_companion =
                session.mode == ChatMode::Companion || declarative.is_some() || agent_draft;
            // The principle 6 injections gathered here (the profile of the
            // conversation while its switch is on, and the past sessions block
            // on the first message) are made by the executor for the Apollia
            // agent only. Companion sessions are isolated from both: the
            // companion helps with the platform, not with the user's personal
            // context.
            let apollia_context = if is_companion {
                ApolliaContext::default()
            } else {
                ApolliaContext {
                    user_memory: crate::chat::profile_injection::switched_store(
                        session,
                        self.user_memory.as_ref(),
                    ),
                    profile_id: session.profile_id.clone(),
                    past_sessions: if is_first_message {
                        self.build_cross_session_context(content)
                    } else {
                        None
                    },
                }
            };
            let system_prompt = match &declarative {
                Some(agent) => agent.mission.clone(),
                None => session.system_prompt.clone(),
            };
            // The agent is offered its granted accesses, read on every message
            // so a change on its card applies to open conversations. An agent
            // created in the interface whose access nobody chose reads by
            // default.
            let available_tools = match &declarative {
                Some(agent) => apollia_tools::AgentAccess::of(agent),
                // The draft tool belongs to the loop; no access is offered
                // beside it.
                None if agent_draft => apollia_tools::AgentAccess::Listed(Vec::new()),
                None => {
                    super::super::apollia_agent::apollia_access(self.governance_db_path.as_deref())
                        .unwrap_or_else(|| {
                            apollia_tools::AgentAccess::Listed(session.available_tools.clone())
                        })
                }
            };
            // Live merge: governance.db is re-read on every message so Apollia
            // Chat config changes (auto-authorized tools, agent-scoped rules)
            // apply to already-open Libre sessions without closing/reopening the
            // conversation. The merge is purely additive: an authorization
            // granted during the session is never removed, only added to if the
            // config has been enriched since.
            let authorized_tools = session_run_grants(&session.authorized_tools);
            let pending_approvals = self.pending_chat_approvals.clone();

            // The tier of the agent: a declarative agent's as resolved; the
            // Apollia agent's is the global default, as any agent that names
            // none. The executor builds the budget from it, capped by
            // `runtime_budget` (principle #7), and the verification loop and
            // critic when the tier asks for them.
            let autonomy_level = declarative
                .as_ref()
                .map_or_else(crate::agents::global_default_autonomy, |agent| {
                    agent.autonomy
                });
            let agent_id = declarative.map_or_else(
                || apollia_tools::APOLLIA_AGENT_ID.to_string(),
                |agent| agent.id,
            );
            let sid = session_id.to_string();
            let mid = message_id.clone();
            let user_msg = content.to_string();
            let tx = self.tx.clone();
            let context_window_size = DEFAULT_CONTEXT_WINDOW_SIZE;
            let turn_images = self.turn_images(
                session_id,
                &session.history[..answered],
                context_window_size,
            );

            let stored_summary = self.repository.get_summary(session_id).unwrap_or(None);
            let llm_for_summarize = self.llm_router.clone();

            // The invoker is created per-session inside the task.
            let a2a_for_agent = self.a2a_invoker.clone();
            let tool_registry = self.tool_registry.clone();
            let event_bus = self.event_bus.clone();

            let pending_user_inputs_for_session = self.pending_user_inputs.clone();
            let mcp_handle_for_session = self.mcp_handle.clone();
            let package_tools = package_executors(&self.package_tools, &agent_id, &cancel);
            let chat_tools_config_for_session = self.chat_tools_config.clone();
            let mcp_loading = self.mcp_loading;
            let tool_search_limit = self.tool_search_limit;
            let session_id_str = session_id.to_string();

            tokio::spawn(run_libre_exchange(LibreExchangeParams {
                agent_id,
                llm_router,
                tool_registry,
                event_bus,
                a2a_for_agent,
                apollia_context,
                pending_approvals,
                autonomy_level,
                runtime_budget: self.runtime_budget.clone(),
                history,
                available_tools,
                authorized_tools,
                system_prompt,
                context_window_size,
                stored_summary,
                llm_for_summarize,
                pending_user_inputs_for_session,
                mcp_handle_for_session,
                package_tools,
                composer: self.composer.clone(),
                governance: self.governance.clone(),
                chat_tools_config_for_session,
                mcp_loading,
                tool_search_limit,
                session_id_str,
                sid,
                mid,
                run_id: run_id.clone(),
                user_msg,
                tx,
                // A draft keeps no task list.
                todo: self.todo_handle.clone().filter(|_| !agent_draft),
                plan: self.plan_handle.clone(),
                session_plan_mode,
                session_plan_phase,
                hook_executor: self.hook_executor.clone(),
                cancel,
                pending_injection,
                answer_prefill,
                preferred_backend,
                turn_id,
                turn_images,
                agent_draft,
            }));
        }

        Ok(())
    }

    /// Dispatch an Agent-mode exchange to the [`AgentChatExecutor`].
    pub(super) fn dispatch_agent_exchange(
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
}
