use super::*;

impl ChatSessionManager {
    /// Register a pending `ask_user` reply channel and emit the input-required event.
    pub(in crate::chat::manager) fn handle_register_user_input_reply(
        &mut self,
        params: RegisterUserInputReplyParams,
    ) {
        let RegisterUserInputReplyParams {
            request_id,
            session_id,
            questions_json,
            context,
            reply_tx,
        } = params;
        let created_at = chrono::Utc::now().to_rfc3339();
        let meta = PendingUserInputMeta {
            session_id: session_id.clone(),
            questions_json: questions_json.clone(),
            context: context.clone(),
            created_at,
        };
        self.pending_user_replies
            .insert(request_id.clone(), (meta, reply_tx));
        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::ChatUserInputRequired {
                request_id,
                session_id,
                message_id: String::new(),
                questions_json,
                context,
            });
    }
}

impl ChatSessionManager {
    /// Restore active sessions from SQLite at boot.
    /// Resolve a pending `ask_user` request by delivering the user's answers.
    pub(in crate::chat::manager) fn resolve_user_input_internal(
        &mut self,
        request_id: &str,
        answers: Vec<apollia_tools::tools::ask_user::UserAnswer>,
    ) -> Result<(), ChatError> {
        let (meta, reply_tx) = self
            .pending_user_replies
            .remove(request_id)
            .ok_or_else(|| {
                ChatError::InternalError(format!(
                    "no pending ask_user request with id '{request_id}'"
                ))
            })?;

        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::ChatUserInputResolved {
                request_id: request_id.to_string(),
                session_id: meta.session_id,
            });

        let output = apollia_tools::tools::ask_user::AskUserOutput { answers };
        reply_tx.send(output).map_err(|_| {
            ChatError::InternalError(
                "ask_user reply channel closed (agent may have timed out)".into(),
            )
        })
    }

    /// Reject a pending `ask_user` request, sends skipped answers to unblock the agent.
    pub(in crate::chat::manager) fn reject_user_input_internal(
        &mut self,
        request_id: &str,
        _reason: String,
    ) -> Result<(), ChatError> {
        let (meta, reply_tx) = self
            .pending_user_replies
            .remove(request_id)
            .ok_or_else(|| {
                ChatError::InternalError(format!(
                    "no pending ask_user request with id '{request_id}'"
                ))
            })?;

        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::ChatUserInputResolved {
                request_id: request_id.to_string(),
                session_id: meta.session_id,
            });

        // Deliver all-skipped answers to unblock the agent loop.
        let output = apollia_tools::tools::ask_user::AskUserOutput { answers: vec![] };
        reply_tx.send(output).map_err(|_| {
            ChatError::InternalError(
                "ask_user reply channel closed (agent may have timed out)".into(),
            )
        })
    }

    pub(in crate::chat::manager) fn restore_sessions(&mut self) {
        let rows = match self.repository.list_sessions(Some("active")) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Failed to restore active sessions from SQLite");
                return;
            }
        };

        for row in rows {
            let mode = match ChatMode::from_sql(&row.mode) {
                Some(m) => m,
                None => continue,
            };
            let status = match SessionStatus::from_sql(&row.status) {
                Some(s) => s,
                None => continue,
            };
            let plan_phase = PlanPhase::from_sql(&row.plan_phase).unwrap_or_default();
            let plan_mode = row.plan_mode;
            let available_tools: Vec<String> =
                serde_json::from_str(&row.available_tools).unwrap_or_default();
            let mut authorized_tools = self
                .repository
                .get_authorized_tools(&row.id)
                .unwrap_or_default();
            // Libre sessions: also seed from governance.db agent-scoped allow
            // rules so cross-session "always allow" decisions survive.
            if mode == ChatMode::Libre {
                let overrides = load_chat_libre_overrides();
                for tool in overrides.pre_authorized_tools {
                    authorized_tools.insert(tool);
                }
            }
            let messages = self
                .repository
                .get_messages(&row.id, None)
                .unwrap_or_default();
            let history: Vec<ChatMessage> = messages
                .into_iter()
                .map(|m| {
                    let role = ChatRole::from_sql(&m.role).unwrap_or(ChatRole::User);
                    let tool_calls: Option<Vec<ToolCallRecord>> = m
                        .tool_calls_json
                        .as_deref()
                        .and_then(|j| serde_json::from_str(j).ok());
                    let metadata: Option<serde_json::Value> = m
                        .metadata
                        .as_deref()
                        .and_then(|j| serde_json::from_str(j).ok());
                    ChatMessage {
                        id: m.id,
                        role,
                        content: m.content,
                        tool_calls,
                        tool_name: m.tool_name,
                        created_at: m.created_at,
                        seq: m.seq,
                        metadata,
                    }
                })
                .collect();

            let session = ChatSession {
                id: row.id.clone(),
                mode,
                agent_name: row.agent_name,
                system_prompt: row.system_prompt,
                status,
                history,
                authorized_tools,
                available_tools,
                created_at: row.created_at,
                active_exchange: None,
                llm_backend: row.llm_backend,
                title: row.title,
                parent_session_id: row.parent_session_id,
                fork_depth: row.fork_depth,
                project_id: row.project_id,
                force_project_context_inject: false,
                fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                    std::collections::HashSet::new(),
                )),
                plan_mode,
                plan_phase,
            };
            self.sessions.insert(row.id, session);
        }

        if !self.sessions.is_empty() {
            info!(
                count = self.sessions.len(),
                "ChatSessionManager: restored active sessions from SQLite"
            );
        }
    }
}
