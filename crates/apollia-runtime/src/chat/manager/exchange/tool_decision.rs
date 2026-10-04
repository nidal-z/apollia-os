//! The human decision on a pending tool call.
//!
//! Resolves the approval the exchange is waiting on. An "always" is not
//! written here: the run that asked turns it into a standing consent of its
//! agent for the family of the tool, through governance, never into a grant
//! by tool name.

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
        // A run outside any conversation names its task where a session would
        // be; its approvals wait in the same registry.
        if !self.sessions.contains_key(session_id) {
            return self.resolve_task_approval(session_id, message_id, tool_call_id, decision);
        }
        let key = format!("{session_id}::{message_id}::{tool_call_id}");
        let resolved = self.pending_chat_approvals.resolve(&key, decision.clone());

        if !resolved {
            return Err(ChatError::SessionNotFound(format!(
                "no pending approval for {key}"
            )));
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
}
