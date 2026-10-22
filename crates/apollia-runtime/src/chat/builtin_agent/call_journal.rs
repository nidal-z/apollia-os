//! The record of a chat tool call on the event bus, in the form the audit
//! journal chains: a start and an end that carry the run, the identifier the
//! model gave the call, the family of the tool and the target of the call.
//!
//! The chat has its own pair of events for the conversation view, which carry
//! neither a run nor a call identifier. These two are for the journal: they
//! are what lets an entry be joined to the arguments and the output the run
//! captured.

use super::*;

/// What identifies and situates one call, read once before it runs and written
/// on both of its events.
pub(super) struct CallFacts {
    event_id: String,
    run_id: RunId,
    tool_call_id: String,
    tool_name: String,
    family: Option<String>,
    target: Option<String>,
}

impl BuiltInChatAgent {
    /// Announces that `call` is about to run in `run_id`.
    pub(super) async fn journal_call_started(
        &self,
        call: &apollia_llm::types::ToolCall,
        run_id: &RunId,
    ) -> CallFacts {
        let descriptor = self.tool_registry.describe(&call.name).await;
        let facts = CallFacts {
            event_id: uuid::Uuid::now_v7().to_string(),
            run_id: run_id.clone(),
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            family: Some(
                super::governed::family_of(
                    &call.name,
                    descriptor.as_ref(),
                    self.mcp_index.as_deref(),
                )
                .as_str()
                .to_owned(),
            ),
            target: descriptor
                .as_ref()
                .and_then(|d| d.call_target(&call.arguments)),
        };
        let _ = self.event_bus.send(RuntimeEvent::ToolCallStarted {
            event_id: facts.event_id.clone(),
            // A chat turn runs under no task: its run stands for it.
            task_id: run_id.as_str().to_owned().into(),
            agent_id: self.agent_id.clone().into(),
            tool_name: facts.tool_name.clone(),
            args_json: serde_json::to_string(&call.arguments).ok(),
            run_id: Some(facts.run_id.clone()),
            tool_call_id: Some(facts.tool_call_id.clone()),
            family: facts.family.clone(),
            target: facts.target.clone(),
        });
        facts
    }

    /// Announces that the call `facts` describes ended, with `output` and
    /// `success`.
    pub(super) fn journal_call_completed(
        &self,
        facts: CallFacts,
        output: &str,
        success: bool,
        duration_ms: u64,
    ) {
        let _ = self.event_bus.send(RuntimeEvent::ToolCallCompleted {
            parent_event_id: facts.event_id,
            task_id: facts.run_id.as_str().to_owned().into(),
            agent_id: self.agent_id.clone().into(),
            tool_name: facts.tool_name,
            output_json: Some(output.to_owned()),
            exit_code: None,
            duration_ms,
            success,
            run_id: Some(facts.run_id),
            tool_call_id: Some(facts.tool_call_id),
            family: facts.family,
            target: facts.target,
        });
    }
}
