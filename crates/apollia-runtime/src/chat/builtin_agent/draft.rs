//! The `propose_agent` call, answered inside the loop.

use super::*;
use crate::chat::agent_draft_tool::run_agent_draft;

impl BuiltInChatAgent {
    /// Run the `propose_agent` built-in tool inside the ReAct loop.
    ///
    /// Answers with the draft the offer of the conversation allows and
    /// injects it as the tool message; nothing is stored. Returns `true` when
    /// the call proposed nothing (malformed payload), so the loop counts it
    /// toward escalation. Outside the conversation that creates an agent the
    /// tool is not offered and the call is answered as a failure.
    pub(in crate::chat::builtin_agent) fn handle_agent_draft(
        &self,
        session_id: &str,
        call: &ToolCall,
        llm_messages: &mut Vec<LlmChatMessage>,
        acc: &mut ReactAccumulators,
    ) -> bool {
        let offer = self.agent_draft.clone().unwrap_or_default();
        let result = run_agent_draft(&offer, &call.arguments);
        tracing::event!(
            tracing::Level::INFO,
            session_id = %session_id,
            ok = result.ok,
            "chat.agent_draft.proposed"
        );
        let tool_result = serde_json::to_string(&result).unwrap_or_else(|_| {
            r#"{"ok":false,"error":"draft result serialization failed"}"#.to_string()
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
}
