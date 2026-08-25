use super::*;

impl ChatSessionManager {
    /// Clear the `project_id` of cached sessions after their project is deleted.
    pub(in crate::chat::manager) fn handle_orphan_project_sessions(&mut self, project_id: &str) {
        match self.repository.orphan_project_sessions(project_id) {
            Ok(count) => {
                if count > 0 {
                    info!(project_id = %project_id, count, "Orphaned chat sessions after project deletion");
                    // Also update in-memory cache
                    for session in self.sessions.values_mut() {
                        if session.project_id.as_deref() == Some(project_id) {
                            session.project_id = None;
                        }
                    }
                }
            }
            Err(e) => {
                warn!(project_id = %project_id, error = %e, "Failed to orphan sessions");
            }
        }
    }
}

impl ChatSessionManager {
    /// Build a cross-session context block from past session summaries.
    ///
    /// Returns `None` if the first message is too short (trivial greeting)
    /// or no relevant past sessions are found.
    pub(in crate::chat::manager) fn build_cross_session_context(
        &self,
        first_message: &str,
    ) -> Option<String> {
        if first_message.len() < MIN_MESSAGE_LENGTH_FOR_RECALL {
            return None;
        }

        let sessions = self
            .repository
            .find_relevant_sessions(first_message, MAX_PAST_SESSIONS)
            .ok()?;

        if sessions.is_empty() {
            return None;
        }

        let mut block = String::from("## Previous conversations (for reference)\n");
        for session in &sessions {
            block.push_str(&format!("- [{}] {}\n", session.created_at, session.summary));
        }

        Some(block)
    }

    /// Validate a session-creation request against the registry and LLM config.
    ///
    /// Takes the borrowed dependencies explicitly rather than `&self`: holding a
    /// `&ChatSessionManager` (which owns a `RefCell` via rusqlite) across the
    /// `find_by_name` await would make `run`'s future non-`Send`. Borrowing only
    /// the `Send + Sync` registry handle keeps the future spawnable.
    pub(in crate::chat::manager) async fn validate_create_request(
        registry_handle: &AgentRegistryHandle,
        llm_configured: bool,
        mode: ChatMode,
        agent_name: Option<&str>,
    ) -> Result<(), ChatError> {
        // Agent mode requires an agent name
        if mode == ChatMode::Agent && agent_name.is_none() {
            return Err(ChatError::AgentNotFound(
                "agent_name is required for Agent mode".into(),
            ));
        }

        // Validate agent exists in the registry if agent mode
        if mode == ChatMode::Agent {
            if let Some(name) = agent_name {
                let found = registry_handle.find_by_name(name).await.map_err(|e| {
                    ChatError::InternalError(format!("registry lookup failed: {e}"))
                })?;
                if found.is_none() {
                    return Err(ChatError::AgentNotFound(name.to_string()));
                }
            }
        }

        // Libre and Companion modes both dispatch through the Libre exchange,
        // which needs a configured LLM; validate both up front rather than
        // letting a Companion session get stuck on its first message.
        if (mode == ChatMode::Libre || mode == ChatMode::Companion) && !llm_configured {
            return Err(ChatError::NoLlmConfigured);
        }

        Ok(())
    }

    /// Create a new chat session.
    pub(in crate::chat::manager) async fn handle_create_session(
        &mut self,
        params: CreateSessionParams,
    ) -> Result<SessionInfo, ChatError> {
        let CreateSessionParams {
            mode,
            agent_name,
            system_prompt,
            tools,
            project_id,
        } = params;
        Self::validate_create_request(
            &self.registry_handle,
            self.llm_router.is_some(),
            mode.clone(),
            agent_name.as_deref(),
        )
        .await?;

        // When no tools are explicitly specified, default to all tools in the registry.
        // Users can override by passing an explicit tools list when creating the session.
        let resolved_tools = if tools.is_empty() {
            let all =
                self.tool_registry.list().await.map_err(|e| {
                    ChatError::InternalError(format!("tool registry list failed: {e}"))
                })?;
            all.into_iter().map(|d| d.name).collect()
        } else {
            tools
        };

        let session_id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let mut prompt = system_prompt.unwrap_or_default();

        // ── Libre mode: pull persisted overrides from governance.db ──────────
        // Silent fallback when the DB is absent / empty / unreadable: legacy behavior.
        let LibreSessionDefaults {
            llm_backend: libre_llm_backend,
            pre_authorized,
        } = apply_libre_overrides(
            mode == ChatMode::Libre,
            &mut prompt,
            self.governance_db_path.as_deref(),
        );

        // Persist to SQLite
        self.repository.create_session(
            &session_id,
            &mode,
            agent_name.as_deref(),
            &prompt,
            &resolved_tools,
            &now,
            None,
            project_id.as_deref(),
        )?;

        // A new session inherits the runtime-level plan-mode default. When it is
        // on, the session starts in the Discovery phase; the per-session toggle
        // overrides it afterwards. The phase column defaults to Done for an
        // off default, so the flag is only persisted when it is on.
        let plan_mode = self.plan_mode_default;
        let plan_phase = if plan_mode {
            PlanPhase::Discovery
        } else {
            PlanPhase::Done
        };
        if plan_mode {
            self.repository
                .set_plan_mode(&session_id, true, plan_phase)?;
        }

        // Build in-memory session
        let session = ChatSession {
            id: session_id.clone(),
            mode: mode.clone(),
            agent_name: agent_name.clone(),
            system_prompt: prompt,
            status: SessionStatus::Active,
            history: Vec::new(),
            authorized_tools: pre_authorized,
            available_tools: resolved_tools,
            created_at: now.clone(),
            active_exchange: None,
            llm_backend: libre_llm_backend,
            title: None,
            parent_session_id: None,
            fork_depth: 0,
            project_id,
            force_project_context_inject: false,
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            // Plan mode is seeded from the runtime-level default; the per-session
            // toggle overrides it afterwards.
            plan_mode,
            plan_phase,
        };

        let info = session_to_info(&session);
        self.sessions.insert(session_id.clone(), session);

        // Emit event
        let _ = self.event_bus.send(RuntimeEvent::ChatSessionCreated {
            session_id,
            mode: mode.as_sql().to_string(),
            agent_name,
        });

        Ok(info)
    }

    /// Update session configuration.
    pub(in crate::chat::manager) fn handle_update_session(
        &mut self,
        session_id: &str,
        system_prompt: Option<&str>,
        available_tools: Option<&[String]>,
        llm_backend: Option<&Option<String>>,
    ) -> Result<(), ChatError> {
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

        // Persist to SQLite
        self.repository.update_session_config(
            session_id,
            system_prompt,
            available_tools,
            llm_backend.map(|b| b.as_deref()),
        )?;

        // Update in-memory session
        if let Some(prompt) = system_prompt {
            session.system_prompt = prompt.to_string();
        }
        if let Some(tools) = available_tools {
            session.available_tools = tools.to_vec();
        }
        if let Some(backend) = llm_backend {
            session.llm_backend = backend.clone();
        }

        info!(session_id = %session_id, "chat session config updated");
        Ok(())
    }
}

impl ChatSessionManager {
    /// Resolve a session's project id to its workspace path, if any.
    pub(in crate::chat::manager) fn resolve_project_workspace(
        &self,
        project_id: Option<&str>,
    ) -> Option<std::path::PathBuf> {
        let pid = project_id?;
        let repo = self.project_repo.as_ref()?;
        repo.get_project(pid)
            .ok()
            .and_then(|d| d.workspace_path.map(std::path::PathBuf::from))
    }

    /// Resolve the effective working directory for a session.
    ///
    /// Mirrors [`resolve_workspace_path`] applied per message: the project's
    /// `workspace_path` when the session is linked to a project, otherwise the
    /// operator-configured `[chat] default_workspace`, then `~/.apollia`. Kept
    /// synchronous (SQLite fallback lookup only, no await) so the actor loop
    /// never holds a connection borrow across a suspension point.
    ///
    /// Returns `None` for an unknown session, or a free chat with neither a
    /// configured default workspace nor an existing `~/.apollia`.
    pub(in crate::chat::manager) fn handle_resolve_session_workspace(
        &self,
        session_id: &str,
    ) -> Option<std::path::PathBuf> {
        // Resolve the session's project link in-memory first, then fall back to
        // SQLite. An unknown session yields `None` so the caller reveals the
        // raw path best-effort instead of a wrong location.
        let project_id: Option<String> = if let Some(session) = self.sessions.get(session_id) {
            session.project_id.clone()
        } else {
            match self.repository.get_session(session_id) {
                Ok(Some(row)) => row.project_id,
                _ => return None,
            }
        };

        match project_id {
            Some(pid) => self.resolve_project_workspace(Some(&pid)),
            None => {
                let default_workspace = self
                    .chat_tools_config
                    .as_ref()
                    .and_then(|c| c.default_workspace.clone());
                if let Some(p) = default_workspace.filter(|p| p.is_dir()) {
                    return Some(p);
                }
                apollia_core::paths::home_dir()
                    .map(|h| h.join(".apollia"))
                    .filter(|p| p.is_dir())
            }
        }
    }

    /// List sessions with optional status filter.
    pub(in crate::chat::manager) fn handle_list_sessions(
        &self,
        status_filter: Option<&SessionStatus>,
    ) -> Vec<SessionInfo> {
        let filter_sql = status_filter.map(|s| s.as_sql());
        match self.repository.list_sessions(filter_sql) {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    let mode = ChatMode::from_sql(&row.mode)?;
                    let status = SessionStatus::from_sql(&row.status)?;
                    Some(SessionInfo {
                        id: row.id,
                        mode,
                        agent_name: row.agent_name,
                        status,
                        created_at: row.created_at,
                        title: row.title,
                        project_id: row.project_id,
                    })
                })
                .collect(),
            Err(e) => {
                error!(error = %e, "Failed to list sessions from SQLite");
                Vec::new()
            }
        }
    }

    /// Get session detail.
    /// Resolve the todo handle for a session, enforcing 404 semantics.
    ///
    /// Returns [`ChatError::SessionNotFound`] for an unknown session, `Ok(None)`
    /// for a known session when no todo store is attached, and `Ok(Some(handle))`
    /// otherwise. Kept synchronous so the actor loop never holds a borrow of the
    /// SQLite connection across an `await`; the caller reads through the cloned
    /// handle (principle: one actor, one responsibility).
    pub(in crate::chat::manager) fn resolve_todo_handle(
        &self,
        session_id: &str,
    ) -> Result<Option<TodoHandle>, ChatError> {
        let known = self.sessions.contains_key(session_id)
            || self.repository.get_session(session_id)?.is_some();
        if !known {
            return Err(ChatError::SessionNotFound(session_id.to_string()));
        }
        Ok(self.todo_handle.clone())
    }

    /// Resolve the plan handle for a known session without holding a SQLite
    /// borrow across an await.
    ///
    /// Returns [`ChatError::SessionNotFound`] for an unknown session and
    /// `Ok(None)` when the runtime has no plan actor wired (no plan history is
    /// available rather than an error).
    pub(in crate::chat::manager) fn resolve_plan_handle(
        &self,
        session_id: &str,
    ) -> Result<Option<PlanHandle>, ChatError> {
        let known = self.sessions.contains_key(session_id)
            || self.repository.get_session(session_id)?.is_some();
        if !known {
            return Err(ChatError::SessionNotFound(session_id.to_string()));
        }
        Ok(self.plan_handle.clone())
    }

    pub(in crate::chat::manager) fn handle_get_session(
        &self,
        session_id: &str,
    ) -> Option<SessionDetail> {
        // Try in-memory first
        if let Some(session) = self.sessions.get(session_id) {
            return Some(SessionDetail {
                session: session.clone(),
                message_count: session.history.len() as u32,
            });
        }

        // Fall back to SQLite
        let row = match self.repository.get_session(session_id) {
            Ok(Some(row)) => row,
            _ => return None,
        };

        let messages = self
            .repository
            .get_messages(session_id, None)
            .unwrap_or_default();
        let authorized_tools = self
            .repository
            .get_authorized_tools(session_id)
            .unwrap_or_default();

        let mode = ChatMode::from_sql(&row.mode)?;
        let status = SessionStatus::from_sql(&row.status)?;
        let plan_phase = PlanPhase::from_sql(&row.plan_phase).unwrap_or_default();
        let plan_mode = row.plan_mode;
        let available_tools: Vec<String> =
            serde_json::from_str(&row.available_tools).unwrap_or_default();

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

        let message_count = history.len() as u32;
        let session = ChatSession {
            id: row.id,
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

        Some(SessionDetail {
            session,
            message_count,
        })
    }

    /// Close a session.
    /// Drop every piece of in-memory runtime state tied to a session.
    ///
    /// Cancels the cooperative pause token so an in-flight ReAct loop stops at
    /// its next checkpoint, then clears the pause token, pause state, queued
    /// injection, and refuses any pending tool approval (so a loop blocked on
    /// approval unblocks). Called when a session closes or is deleted; without
    /// it these maps leak entries for the lifetime of the actor.
    fn purge_session_runtime_state(&mut self, session_id: &str) {
        if let Some(token) = self.pause_tokens.remove(session_id) {
            token.cancel();
        }
        self.pause_states.remove(session_id);
        self.pending_injections.remove(session_id);
        let refused = self.pending_chat_approvals.refuse_session(session_id);
        if refused > 0 {
            debug!(
                session_id = %session_id,
                refused,
                "chat.session.purge.approvals_refused"
            );
        }
    }

    pub(in crate::chat::manager) fn handle_close_session(
        &mut self,
        session_id: &str,
    ) -> Result<(), ChatError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatError::SessionNotFound(session_id.to_string()))?;

        if session.status == SessionStatus::Closed {
            return Err(ChatError::SessionClosed(session_id.to_string()));
        }

        // Capture history before mutating session state
        let history = session.history.clone();

        let now = now_rfc3339();

        // If an exchange is in progress, cancel it
        session.active_exchange = None;
        session.status = SessionStatus::Closed;

        self.repository.close_session(session_id, &now)?;

        // Drop pause tokens, pause state, queued injections and pending
        // approvals so a closed session cannot be resurrected by a late event.
        self.purge_session_runtime_state(session_id);

        let _ = self.event_bus.send(RuntimeEvent::ChatSessionClosed {
            session_id: session_id.to_string(),
        });

        // Fire-and-forget passive memory enrichment with rate limiting and deduplication
        if let Some(extractor) = &self.enrichment_extractor {
            UserMemoryExtractor::spawn_enrichment(Arc::clone(extractor), history);
        }

        Ok(())
    }

    /// Delete a session and all its data from SQLite and in-memory cache.
    pub(in crate::chat::manager) fn handle_delete_session(
        &mut self,
        session_id: &str,
    ) -> Result<(), ChatError> {
        // Remove from SQLite (messages, authorizations, FTS, session row).
        self.repository.delete_session(session_id)?;

        // Drop any in-memory runtime state before dropping the session itself.
        self.purge_session_runtime_state(session_id);

        // Remove from in-memory cache.
        self.sessions.remove(session_id);

        let _ = self.event_bus.send(RuntimeEvent::ChatSessionClosed {
            session_id: session_id.to_string(),
        });

        info!(session_id = %session_id, "chat session deleted");
        Ok(())
    }

    /// Rename a session by setting a user-defined title.
    pub(in crate::chat::manager) fn handle_rename_session(
        &mut self,
        session_id: &str,
        title: &str,
    ) -> Result<(), ChatError> {
        // Persist to SQLite.
        self.repository.rename_session(session_id, title)?;

        // Update in-memory cache if present.
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.title = Some(title.to_string());
        }

        info!(session_id = %session_id, title = %title, "chat session renamed");
        Ok(())
    }
}

impl ChatSessionManager {
    /// Load a session from SQLite into memory (if not already there) and return its detail.
    ///
    /// If the session is already in memory, its current state is returned immediately.
    /// If the loaded session has status `Processing`, it is reset to `Active` both in
    /// memory and in SQLite before being returned.
    pub(in crate::chat::manager) fn handle_resume_session(
        &mut self,
        session_id: &str,
    ) -> Result<SessionDetail, ChatError> {
        // If already in memory, return current state directly.
        if let Some(session) = self.sessions.get(session_id) {
            let message_count = session.history.len() as u32;
            return Ok(SessionDetail {
                session: session.clone(),
                message_count,
            });
        }

        // Load from SQLite.
        let mut session = self.repository.load_session_with_history(session_id)?;

        // If the session was left in Processing state, reset it to Active.
        if session.status == SessionStatus::Processing {
            if let Err(e) = self
                .repository
                .update_status(session_id, &SessionStatus::Active)
            {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "Failed to reset Processing session to Active in SQLite during resume"
                );
            }
            session.status = SessionStatus::Active;
            session.active_exchange = None;
        }

        let message_count = session.history.len() as u32;
        let detail = SessionDetail {
            session: session.clone(),
            message_count,
        };

        self.sessions.insert(session_id.to_string(), session);

        info!(session_id = %session_id, "chat session resumed from SQLite");

        Ok(detail)
    }

    /// Fork a session, create a child with a copy of the parent's history.
    ///
    /// The parent may be in any non-Closed state; a Closed parent can also be
    /// forked (useful for branching from an archived conversation). The child
    /// inherits mode, system prompt, available tools, and LLM backend from the
    /// parent. Messages up to `up_to_index` are copied (all if `None`).
    pub(in crate::chat::manager) fn handle_fork_session(
        &mut self,
        parent_id: &str,
        up_to_index: Option<usize>,
    ) -> Result<SessionInfo, ChatError> {
        // Resolve the message count to copy from in-memory history when available,
        // so the caller can pass an index relative to the current in-memory state.
        let resolved_count = if let Some(n) = up_to_index {
            let parent_len = if let Some(s) = self.sessions.get(parent_id) {
                s.history.len()
            } else {
                // Fall back to repository count, session may not be in memory.
                self.repository
                    .get_messages(parent_id, None)
                    .unwrap_or_default()
                    .len()
            };
            Some(n.min(parent_len))
        } else {
            None
        };

        let child_id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();

        let child =
            self.repository
                .create_fork_session(&child_id, parent_id, resolved_count, &now)?;

        let messages_copied = child.history.len();

        tracing::info!(
            parent_id = %parent_id,
            child_id = %child_id,
            messages_copied = messages_copied,
            "session forked"
        );

        let _ = self.event_bus.send(RuntimeEvent::ChatSessionCreated {
            session_id: child_id.clone(),
            mode: child.mode.as_sql().to_string(),
            agent_name: child.agent_name.clone(),
        });

        let info = session_to_info(&child);
        self.sessions.insert(child_id, child);

        Ok(info)
    }

    /// List direct child sessions (forks) of the given parent.
    pub(in crate::chat::manager) fn handle_list_children(
        &self,
        parent_id: &str,
    ) -> Vec<SessionInfo> {
        match self.repository.list_children(parent_id) {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    let mode = ChatMode::from_sql(&row.mode)?;
                    let status = SessionStatus::from_sql(&row.status)?;
                    Some(SessionInfo {
                        id: row.id,
                        mode,
                        agent_name: row.agent_name,
                        status,
                        created_at: row.created_at,
                        title: row.title,
                        project_id: row.project_id,
                    })
                })
                .collect(),
            Err(e) => {
                error!(parent_id = %parent_id, error = %e, "Failed to list session children");
                Vec::new()
            }
        }
    }

    /// Link or unlink a session to a project.
    pub(in crate::chat::manager) fn handle_link_session_to_project(
        &mut self,
        session_id: &str,
        project_id: Option<&str>,
    ) -> Result<(), ChatError> {
        self.repository
            .set_session_project(session_id, project_id)?;

        // Update in-memory cache and flag the next user message so the
        // project-context provider re-runs (the initial-injection path is
        // gated on `is_first_message`, which is false for already-active
        // sessions).
        if let Some(session) = self.sessions.get_mut(session_id) {
            let new_pid = project_id.map(|s| s.to_string());
            if session.project_id != new_pid {
                session.force_project_context_inject = new_pid.is_some();
            }
            session.project_id = new_pid;
        }
        Ok(())
    }

    /// List sessions belonging to a specific project.
    pub(in crate::chat::manager) fn handle_list_sessions_by_project(
        &self,
        project_id: &str,
    ) -> Vec<SessionInfo> {
        match self.repository.list_sessions_by_project(project_id) {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    let mode = ChatMode::from_sql(&row.mode)?;
                    let status = SessionStatus::from_sql(&row.status)?;
                    Some(SessionInfo {
                        id: row.id,
                        mode,
                        agent_name: row.agent_name,
                        status,
                        created_at: row.created_at,
                        title: row.title,
                        project_id: row.project_id,
                    })
                })
                .collect(),
            Err(e) => {
                error!(project_id = %project_id, error = %e, "Failed to list sessions by project");
                Vec::new()
            }
        }
    }
}
