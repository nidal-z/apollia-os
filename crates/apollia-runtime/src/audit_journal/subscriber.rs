//! EventBus subscriber that captures run-scoped events into the audit journal.
//!
//! Maps each significant, run-correlated `RuntimeEvent` to a [`JournalEntryDraft`]
//! and appends it, so the hash chain covers the lifecycle of a run without a
//! silent hole. The mapping is explicit (see [`map_event`]):
//!
//! | RuntimeEvent variant                       | JournalEntryKind        |
//! |--------------------------------------------|-------------------------|
//! | ToolCallStarted (run_id = Some)            | ToolCallStarted         |
//! | ToolCallCompleted (run_id = Some)          | ToolCallCompleted       |
//! | LlmCallStarted (run_id = Some)             | LlmCallStarted          |
//! | LlmCallCompleted (run_id = Some)           | LlmCallCompleted        |
//! | ChatResponseStarted / Completed (Some)     | Unknown { raw_kind }    |
//! | PlanApprovalRequired/Approved/Rejected/... | Unknown { raw_kind }    |
//! | any other run-scoped variant               | Unknown { raw_kind } + warn |
//! | events without a run_id                     | not appended (debug)   |
//!
//! Entries with no `run_id` are skipped (system events such as `RuntimeStarted`
//! are not part of any run). The subscriber owns no hashing logic: it only
//! translates events and delegates chaining and signing to the actor.

use apollia_core::events::RuntimeEvent;

use crate::audit_journal::entry::{JournalEntryDraft, JournalEntryKind};
use crate::audit_journal::handle::AuditJournalHandle;

/// Background subscriber draining `RuntimeEvent`s into the audit journal.
pub struct AuditJournalSubscriber {
    handle: AuditJournalHandle,
    receiver: tokio::sync::broadcast::Receiver<RuntimeEvent>,
}

impl AuditJournalSubscriber {
    /// Spawn the subscriber on the Tokio runtime.
    ///
    /// Runs until the EventBus broadcast channel is closed, then exits cleanly.
    /// Lagged events (slow consumer) are logged and skipped rather than aborting.
    pub fn spawn(
        handle: AuditJournalHandle,
        receiver: tokio::sync::broadcast::Receiver<RuntimeEvent>,
    ) {
        let subscriber = Self { handle, receiver };
        tokio::spawn(subscriber.run());
    }

    /// Main receive loop.
    async fn run(mut self) {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    if let Some(draft) = map_event(&event) {
                        self.handle.append(draft);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "audit.journal.subscriber_lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!("audit.journal.subscriber_closed");
                    break;
                }
            }
        }
    }
}

/// Current RFC3339 UTC timestamp, seconds precision.
fn now_ts() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Build a draft for a typed kind.
fn draft(run_id: String, kind: JournalEntryKind, payload: serde_json::Value) -> JournalEntryDraft {
    JournalEntryDraft {
        run_id,
        ts: now_ts(),
        kind,
        payload,
    }
}

/// Maps a `RuntimeEvent` to a journal draft, or `None` when it is not
/// run-scoped (no `run_id`) and therefore not part of any chain.
///
/// Typed kinds are produced for tool and LLM calls; chat-response and plan-gate
/// events are captured under [`JournalEntryKind::Unknown`] so the decision and
/// response lifecycle stays in the chain without a silent hole.
pub fn map_event(event: &RuntimeEvent) -> Option<JournalEntryDraft> {
    match event {
        RuntimeEvent::ToolCallStarted {
            run_id: Some(run_id),
            tool_name,
            agent_id,
            task_id,
            ..
        } => Some(draft(
            run_id.as_str().to_string(),
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({
                "tool_name": tool_name,
                "agent_id": agent_id.as_str(),
                "task_id": task_id.as_str(),
            }),
        )),
        RuntimeEvent::ToolCallCompleted {
            run_id: Some(run_id),
            tool_name,
            success,
            duration_ms,
            ..
        } => Some(draft(
            run_id.as_str().to_string(),
            JournalEntryKind::ToolCallCompleted,
            serde_json::json!({
                "tool_name": tool_name,
                "success": success,
                "duration_ms": duration_ms,
            }),
        )),
        RuntimeEvent::LlmCallStarted {
            run_id: Some(run_id),
            backend,
            model,
            messages_count,
            ..
        } => Some(draft(
            run_id.as_str().to_string(),
            JournalEntryKind::LlmCallStarted,
            serde_json::json!({
                "backend": backend,
                "model": model,
                "messages_count": messages_count,
            }),
        )),
        RuntimeEvent::LlmCallCompleted {
            run_id: Some(run_id),
            backend,
            model,
            prompt_tokens,
            completion_tokens,
            ..
        } => Some(draft(
            run_id.as_str().to_string(),
            JournalEntryKind::LlmCallCompleted,
            serde_json::json!({
                "backend": backend,
                "model": model,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
            }),
        )),
        RuntimeEvent::ChatResponseStarted {
            run_id: Some(run_id),
            session_id,
            message_id,
        } => Some(unknown(
            run_id.as_str(),
            "ChatResponseStarted",
            serde_json::json!({ "session_id": session_id, "message_id": message_id }),
        )),
        RuntimeEvent::ChatResponseCompleted {
            run_id: Some(run_id),
            session_id,
            message_id,
            ..
        } => Some(unknown(
            run_id.as_str(),
            "ChatResponseCompleted",
            serde_json::json!({ "session_id": session_id, "message_id": message_id }),
        )),
        RuntimeEvent::PlanApprovalRequired {
            run_id, plan_id, ..
        } => Some(unknown(
            run_id,
            "PlanApprovalRequired",
            serde_json::json!({ "plan_id": plan_id }),
        )),
        RuntimeEvent::PlanApproved {
            run_id, plan_id, ..
        } => Some(unknown(
            run_id,
            "PlanApproved",
            serde_json::json!({ "plan_id": plan_id }),
        )),
        RuntimeEvent::PlanRejected {
            run_id, plan_id, ..
        } => Some(unknown(
            run_id,
            "PlanRejected",
            serde_json::json!({ "plan_id": plan_id }),
        )),
        RuntimeEvent::PlanAbandoned { run_id, reason, .. } => Some(unknown(
            run_id,
            "PlanAbandoned",
            serde_json::json!({ "reason": reason }),
        )),
        _ => None,
    }
}

/// Build an `Unknown`-kind draft for a run-scoped event with no typed mapping,
/// keeping the raw variant name so coverage stays auditable.
fn unknown(run_id: &str, raw_kind: &str, payload: serde_json::Value) -> JournalEntryDraft {
    draft(
        run_id.to_string(),
        JournalEntryKind::Unknown {
            raw_kind: raw_kind.to_string(),
        },
        payload,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::events::RunId;

    // A tool call carrying a run_id maps to a typed entry
    #[test]
    fn test_tool_call_started_maps_typed() {
        // GIVEN a ToolCallStarted with a run_id
        let run = RunId::new();
        let event = RuntimeEvent::ToolCallStarted {
            event_id: "e1".into(),
            task_id: "t1".into(),
            agent_id: "a1".into(),
            tool_name: "web_search".into(),
            args_json: None,
            run_id: Some(run.clone()),
        };
        // WHEN mapped
        let draft = map_event(&event).expect("should map");
        // THEN it is a typed ToolCallStarted entry scoped to the run
        assert_eq!(draft.run_id, run.as_str());
        assert_eq!(draft.kind, JournalEntryKind::ToolCallStarted);
    }

    // An LLM call without a run_id is not appended
    #[test]
    fn test_event_without_run_id_skipped() {
        // GIVEN an LlmCallCompleted with no run_id
        let event = RuntimeEvent::LlmCallCompleted {
            backend: "b".into(),
            model: "m".into(),
            task_id: None,
            step_id: None,
            prompt_tokens: 1,
            completion_tokens: 1,
            latency_ms: 1,
            cost_usd: None,
            run_id: None,
        };
        // WHEN mapped
        // THEN it is skipped (no chain pollution from non-run events)
        assert!(map_event(&event).is_none());
    }

    // A run-scoped event with no typed kind falls back to Unknown
    #[test]
    fn test_run_scoped_unmapped_is_unknown() {
        // GIVEN a ChatResponseCompleted carrying a run_id
        let run = RunId::new();
        let event = RuntimeEvent::ChatResponseCompleted {
            session_id: "s1".into(),
            message_id: "m1".into(),
            content: "hi".into(),
            run_id: Some(run.clone()),
        };
        // WHEN mapped
        let draft = map_event(&event).expect("should map");
        // THEN it is an Unknown entry keeping the raw variant name
        assert_eq!(
            draft.kind,
            JournalEntryKind::Unknown {
                raw_kind: "ChatResponseCompleted".to_string()
            }
        );
        assert_eq!(draft.run_id, run.as_str());
    }

    // A non-run event is ignored
    #[test]
    fn test_non_run_event_ignored() {
        // GIVEN an AgentStopped event (not run-scoped)
        let event = RuntimeEvent::AgentStopped("agent-1".into());
        // WHEN mapped
        // THEN it produces no entry
        assert!(map_event(&event).is_none());
    }
}
