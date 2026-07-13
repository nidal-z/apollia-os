use super::*;

impl ChatSessionManager {
    /// Collect the per-session tool authorizations for all open sessions.
    pub(in crate::chat::manager) fn handle_list_session_authorizations(
        &self,
    ) -> Vec<SessionAuthorizationView> {
        let mut out: Vec<SessionAuthorizationView> = Vec::new();
        for s in self.sessions.values() {
            if matches!(s.status, SessionStatus::Closed) {
                continue;
            }
            for tool in &s.authorized_tools {
                out.push(SessionAuthorizationView {
                    session_id: s.id.clone(),
                    session_title: s.title.clone(),
                    mode: s.mode.as_sql().to_string(),
                    tool_name: tool.clone(),
                });
            }
        }
        out.sort_by(|a, b| {
            a.session_id
                .cmp(&b.session_id)
                .then_with(|| a.tool_name.cmp(&b.tool_name))
        });
        out
    }

    /// Revoke a session-scoped tool authorization. Returns whether it existed.
    pub(in crate::chat::manager) fn handle_revoke_session_authorization(
        &mut self,
        session_id: &str,
        tool_name: &str,
    ) -> Result<bool, ChatError> {
        match self.sessions.get_mut(session_id) {
            Some(s) => Ok(s.authorized_tools.remove(tool_name)),
            None => Err(ChatError::SessionNotFound(session_id.to_string())),
        }
    }

    /// List A2A skills available from active worker agents (async, off-actor).
    pub(in crate::chat::manager) fn handle_list_a2a_skills(
        &self,
        reply: oneshot::Sender<Vec<crate::a2a::SkillListing>>,
    ) {
        if let Some(ref a2a) = self.a2a_invoker {
            let a2a = a2a.clone();
            tokio::spawn(async move {
                let skills = a2a.list_skills().await.unwrap_or_default();
                let _ = reply.send(skills);
            });
        } else {
            let _ = reply.send(Vec::new());
        }
    }

    /// Snapshot A2A skill telemetry (async, off-actor).
    pub(in crate::chat::manager) fn handle_list_a2a_skill_telemetry(
        &self,
        reply: oneshot::Sender<Vec<crate::a2a::A2ASkillTelemetry>>,
    ) {
        if let Some(ref a2a) = self.a2a_invoker {
            let a2a = a2a.clone();
            tokio::spawn(async move {
                let out = match a2a.telemetry() {
                    Some(t) => t.all_telemetry().await,
                    None => Vec::new(),
                };
                let _ = reply.send(out);
            });
        } else {
            let _ = reply.send(Vec::new());
        }
    }

    /// Retrieve A2A step provenance entries, optionally filtered by skill id.
    pub(in crate::chat::manager) fn handle_list_a2a_step_provenance(
        &self,
        skill_id: Option<String>,
        reply: oneshot::Sender<Vec<crate::a2a::A2AStepProvenance>>,
    ) {
        if let Some(ref a2a) = self.a2a_invoker {
            let a2a = a2a.clone();
            tokio::spawn(async move {
                let out = match a2a.telemetry() {
                    Some(t) => t.steps_for(skill_id.as_deref()).await,
                    None => Vec::new(),
                };
                let _ = reply.send(out);
            });
        } else {
            let _ = reply.send(Vec::new());
        }
    }

    /// Check compatibility of a skill against a required semver version.
    pub(in crate::chat::manager) fn handle_check_a2a_compatibility(
        &self,
        skill_id: String,
        required_version: String,
        reply: oneshot::Sender<Option<crate::a2a::A2ACompatibilityWarning>>,
    ) {
        if let Some(ref a2a) = self.a2a_invoker {
            let a2a = a2a.clone();
            tokio::spawn(async move {
                let out = a2a
                    .check_skill_compatibility(&skill_id, &required_version)
                    .await
                    .ok()
                    .flatten();
                let _ = reply.send(out);
            });
        } else {
            let _ = reply.send(None);
        }
    }

    /// Resolve a pending filesystem HITL request.
    pub(in crate::chat::manager) fn handle_resolve_fs_hitl(
        &self,
        request_id: &str,
        decision: super::super::types::FsHitlDecision,
    ) -> Result<(), ChatError> {
        if self.pending_fs_approvals.resolve(request_id, decision) {
            Ok(())
        } else {
            Err(ChatError::InternalError(format!(
                "no pending fs HITL request for id '{request_id}'"
            )))
        }
    }
}
