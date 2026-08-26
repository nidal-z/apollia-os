//! Dispatch of one exchange to the backend that will run it.
//!
//! Two shapes share the entry: a free chat driven by the built-in agent, and
//! an agent-backed session routed through the coordinator.

use super::super::*;

impl ChatSessionManager {
    /// Build the system prompt for a Libre/Companion exchange, optionally
    /// enriched with cross-session context on the first message.
    pub(super) fn build_libre_system_prompt(
        &self,
        base_prompt: &str,
        content: &str,
        enrich: bool,
    ) -> String {
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
    pub(super) fn dispatch_libre_exchange(
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
