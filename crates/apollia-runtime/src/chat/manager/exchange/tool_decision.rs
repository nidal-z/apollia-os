//! The human decision on a pending tool call.
//!
//! Resolves the approval the exchange is waiting on and, when the user asked
//! for it, persists the always-accept scope. A code executor is never granted
//! a blanket allow here: each of its invocations keeps its own checkpoint.

use super::super::*;

impl ChatSessionManager {
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
        use crate::chat::types::AlwaysAcceptScope;
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
