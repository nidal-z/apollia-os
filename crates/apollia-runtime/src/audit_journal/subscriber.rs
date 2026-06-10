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

use std::collections::HashMap;

use apollia_core::events::RuntimeEvent;

use crate::audit_journal::entry::{JournalEntryDraft, JournalEntryKind};
use crate::audit_journal::handle::AuditJournalHandle;
use crate::replay::{ClockSample, LlmCompletionSnapshot, RandomSample, ToolOutputSnapshot};

/// Per-run step-ordinal counters for captured replay inputs.
///
/// Each captured input type owns a contiguous 0-based sequence within a run, so
/// the replay cursors can validate the no-gap invariant. Counters are dropped
/// when the run ends.
#[derive(Debug, Default)]
struct RunOrdinals {
    /// Next ordinal for `LlmCompletion` captures.
    llm: u32,
    /// Next ordinal for `ToolOutput` captures.
    tool: u32,
    /// Next ordinal for `ClockSample` captures.
    clock: u32,
    /// Next ordinal for `RandomSample` captures.
    random: u32,
}

/// Background subscriber draining `RuntimeEvent`s into the audit journal.
pub struct AuditJournalSubscriber {
    handle: AuditJournalHandle,
    receiver: tokio::sync::broadcast::Receiver<RuntimeEvent>,
    /// Per-run capture ordinals, owned by this actor (no shared lock).
    ordinals: HashMap<String, RunOrdinals>,
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
        let subscriber = Self {
            handle,
            receiver,
            ordinals: HashMap::new(),
        };
        tokio::spawn(subscriber.run());
    }

    /// Main receive loop.
    async fn run(mut self) {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    // Capture events carry a per-run step ordinal (stateful);
                    // everything else maps statelessly.
                    if let Some(draft) = map_capture(&mut self.ordinals, &event) {
                        self.handle.append(draft);
                    } else if let Some(draft) = map_event(&event) {
                        self.handle.append(draft);
                    }
                    // Free the per-run counters once the run finishes.
                    if let Some(run_id) = run_end_run_id(&event) {
                        self.ordinals.remove(run_id);
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

/// Returns the run id when an event marks the end of a run, so its capture
/// counters can be freed.
fn run_end_run_id(event: &RuntimeEvent) -> Option<&str> {
    match event {
        RuntimeEvent::ChatResponseCompleted {
            run_id: Some(run_id),
            ..
        } => Some(run_id.as_str()),
        _ => None,
    }
}

/// Maps a capture `RuntimeEvent` to a journal draft, assigning the per-run step
/// ordinal from `ordinals`. Returns `None` for non-capture events, which fall
/// through to the stateless [`map_event`].
///
/// The step ordinal is contiguous and 0-based per run and per capture type, so
/// the replay cursors can detect a missing entry as a gap.
fn map_capture(
    ordinals: &mut HashMap<String, RunOrdinals>,
    event: &RuntimeEvent,
) -> Option<JournalEntryDraft> {
    match event {
        RuntimeEvent::LlmResponseCaptured {
            run_id,
            backend,
            model,
            content,
            tool_calls,
            prompt_tokens,
            completion_tokens,
            cost_usd,
            stream_truncated,
        } => {
            let counters = ordinals.entry(run_id.as_str().to_string()).or_default();
            let step_ordinal = counters.llm;
            counters.llm += 1;

            let snapshot = LlmCompletionSnapshot {
                run_id: run_id.clone(),
                step_ordinal,
                backend_name: backend.clone(),
                model_id: model.clone(),
                content: content.clone(),
                tool_calls: tool_calls.clone(),
                prompt_tokens: *prompt_tokens,
                completion_tokens: *completion_tokens,
                cost_usd: *cost_usd,
                stream_truncated: *stream_truncated,
            };
            let payload = serde_json::to_value(&snapshot).unwrap_or(serde_json::Value::Null);
            Some(draft(
                run_id.as_str().to_string(),
                JournalEntryKind::LlmCompletion,
                payload,
            ))
        }
        RuntimeEvent::ToolOutputCaptured {
            run_id,
            tool_call_id,
            tool_name,
            output,
            status,
        } => {
            let counters = ordinals.entry(run_id.as_str().to_string()).or_default();
            let step_ordinal = counters.tool;
            counters.tool += 1;

            let snapshot = ToolOutputSnapshot {
                run_id: run_id.clone(),
                step_ordinal,
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                output: output.clone(),
                status: status.clone(),
            };
            let payload = serde_json::to_value(&snapshot).unwrap_or(serde_json::Value::Null);
            Some(draft(
                run_id.as_str().to_string(),
                JournalEntryKind::ToolOutput,
                payload,
            ))
        }
        RuntimeEvent::ClockSampled {
            run_id,
            timestamp_ms,
            ..
        } => {
            let counters = ordinals.entry(run_id.as_str().to_string()).or_default();
            let step_ordinal = counters.clock;
            counters.clock += 1;

            let snapshot = ClockSample {
                run_id: run_id.clone(),
                step_ordinal,
                timestamp_ms: *timestamp_ms,
            };
            let payload = serde_json::to_value(&snapshot).unwrap_or(serde_json::Value::Null);
            Some(draft(
                run_id.as_str().to_string(),
                JournalEntryKind::ClockSample,
                payload,
            ))
        }
        RuntimeEvent::RandomSampled {
            run_id,
            bytes,
            captured,
            source_site,
        } => {
            let counters = ordinals.entry(run_id.as_str().to_string()).or_default();
            let step_ordinal = counters.random;
            counters.random += 1;

            // An un-captured draw is journaled with the flag and warned, never
            // dropped: the replay would otherwise diverge silently (Principle #7).
            if !*captured {
                tracing::warn!(
                    run_id = %run_id.as_str(),
                    step_ordinal,
                    source_site = %source_site,
                    "replay.random.uncaptured"
                );
            }

            let snapshot = RandomSample {
                run_id: run_id.clone(),
                step_ordinal,
                bytes: bytes.clone(),
                captured: *captured,
                source_site: source_site.clone(),
            };
            let payload = serde_json::to_value(&snapshot).unwrap_or(serde_json::Value::Null);
            Some(draft(
                run_id.as_str().to_string(),
                JournalEntryKind::RandomSample,
                payload,
            ))
        }
        _ => None,
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

    fn captured(run: &RunId, content: &str, truncated: bool) -> RuntimeEvent {
        RuntimeEvent::LlmResponseCaptured {
            run_id: run.clone(),
            backend: "local".into(),
            model: "m".into(),
            content: content.into(),
            tool_calls: vec![],
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
            stream_truncated: truncated,
        }
    }

    fn snapshot_of(draft: &JournalEntryDraft) -> LlmCompletionSnapshot {
        serde_json::from_value(draft.payload.clone()).expect("snapshot payload")
    }

    // AC-1 / AC-3: ordinal increases within a run, contiguous from 0
    #[test]
    fn test_llm_capture_assigns_increasing_ordinal() {
        // GIVEN a fresh ordinal map and a run with two captured responses
        let mut ordinals = HashMap::new();
        let run = RunId::new();

        // WHEN both events are mapped in order
        let first = map_capture(&mut ordinals, &captured(&run, "a", false)).expect("first");
        let second = map_capture(&mut ordinals, &captured(&run, "b", false)).expect("second");

        // THEN the kind is LlmCompletion and the ordinals are 0 then 1
        assert_eq!(first.kind, JournalEntryKind::LlmCompletion);
        assert_eq!(snapshot_of(&first).step_ordinal, 0);
        assert_eq!(snapshot_of(&second).step_ordinal, 1);
        assert!(!snapshot_of(&first).stream_truncated);
    }

    // AC-3: two runs keep independent ordinal sequences
    #[test]
    fn test_step_ordinal_independent_per_run() {
        // GIVEN two distinct runs whose captures are interleaved
        let mut ordinals = HashMap::new();
        let a = RunId::new();
        let b = RunId::new();

        // WHEN events arrive a0, b0, a1, b1
        let a0 = map_capture(&mut ordinals, &captured(&a, "a0", false)).expect("a0");
        let b0 = map_capture(&mut ordinals, &captured(&b, "b0", false)).expect("b0");
        let a1 = map_capture(&mut ordinals, &captured(&a, "a1", false)).expect("a1");
        let b1 = map_capture(&mut ordinals, &captured(&b, "b1", false)).expect("b1");

        // THEN each run owns its own 0,1 sequence without cross-contamination
        assert_eq!(snapshot_of(&a0).step_ordinal, 0);
        assert_eq!(snapshot_of(&a1).step_ordinal, 1);
        assert_eq!(snapshot_of(&b0).step_ordinal, 0);
        assert_eq!(snapshot_of(&b1).step_ordinal, 1);
    }

    // AC-2: an interrupted stream is captured with the flag, not dropped
    #[test]
    fn test_truncated_stream_captured_with_flag() {
        // GIVEN a captured response flagged as truncated with partial text
        let mut ordinals = HashMap::new();
        let run = RunId::new();

        // WHEN it is mapped
        let draft = map_capture(&mut ordinals, &captured(&run, "partial", true)).expect("draft");

        // THEN the entry keeps the partial text and the truncation flag
        let snap = snapshot_of(&draft);
        assert!(snap.stream_truncated);
        assert_eq!(snap.content, "partial");
    }

    // A non-capture event falls through (handled by map_event instead)
    #[test]
    fn test_map_capture_ignores_non_capture_event() {
        // GIVEN an ordinal map and a non-capture event
        let mut ordinals = HashMap::new();
        let event = RuntimeEvent::AgentStopped("a".into());

        // WHEN mapped through the capture path
        // THEN nothing is produced (no ordinal consumed)
        assert!(map_capture(&mut ordinals, &event).is_none());
        assert!(ordinals.is_empty());
    }

    // AC-1 (595): a completed tool call maps to a ToolOutput entry
    #[test]
    fn test_tool_output_capture_maps_with_ordinal() {
        // GIVEN a captured tool output for a run
        let mut ordinals = HashMap::new();
        let run = RunId::new();
        let event = RuntimeEvent::ToolOutputCaptured {
            run_id: run.clone(),
            tool_call_id: "c1".into(),
            tool_name: "bash_executor".into(),
            output: serde_json::json!({ "stdout": "ok" }),
            status: "success".into(),
        };

        // WHEN mapped
        let draft = map_capture(&mut ordinals, &event).expect("draft");

        // THEN it is a ToolOutput entry at ordinal 0 keeping the status
        assert_eq!(draft.kind, JournalEntryKind::ToolOutput);
        let snap: ToolOutputSnapshot =
            serde_json::from_value(draft.payload.clone()).expect("snapshot");
        assert_eq!(snap.step_ordinal, 0);
        assert_eq!(snap.tool_call_id, "c1");
        assert_eq!(snap.status, "success");
    }

    // AC-2 (595): a clock read maps to a ClockSample entry
    #[test]
    fn test_clock_sample_capture_maps_with_ordinal() {
        // GIVEN a captured clock reading
        let mut ordinals = HashMap::new();
        let run = RunId::new();
        let event = RuntimeEvent::ClockSampled {
            run_id: run.clone(),
            timestamp_ms: 1_700_000_000_123,
            source_site: "agent.turn".into(),
        };

        // WHEN mapped
        let draft = map_capture(&mut ordinals, &event).expect("draft");

        // THEN it is a ClockSample entry carrying the timestamp
        assert_eq!(draft.kind, JournalEntryKind::ClockSample);
        let snap: ClockSample = serde_json::from_value(draft.payload.clone()).expect("snapshot");
        assert_eq!(snap.timestamp_ms, 1_700_000_000_123);
    }

    // AC-3 (595): an un-captured random draw is journaled with captured=false
    #[test]
    fn test_uncaptured_random_journaled_with_flag() {
        // GIVEN a random draw flagged as un-captured (a capture bug)
        let mut ordinals = HashMap::new();
        let run = RunId::new();
        let event = RuntimeEvent::RandomSampled {
            run_id: run.clone(),
            bytes: vec![],
            captured: false,
            source_site: "hitl.request_id".into(),
        };

        // WHEN mapped
        let draft = map_capture(&mut ordinals, &event).expect("draft");

        // THEN the entry is still produced with captured=false (no silent loss)
        assert_eq!(draft.kind, JournalEntryKind::RandomSample);
        let snap: RandomSample = serde_json::from_value(draft.payload.clone()).expect("snapshot");
        assert!(!snap.captured);
        assert_eq!(snap.source_site, "hitl.request_id");
    }

    // The shared per-run ordinal sequences are independent per capture type
    #[test]
    fn test_ordinals_independent_per_capture_type() {
        // GIVEN one run emitting an LLM, then a tool, then another LLM
        let mut ordinals = HashMap::new();
        let run = RunId::new();
        let llm0 = map_capture(&mut ordinals, &captured(&run, "a", false)).expect("llm0");
        let tool0 = map_capture(
            &mut ordinals,
            &RuntimeEvent::ToolOutputCaptured {
                run_id: run.clone(),
                tool_call_id: "c1".into(),
                tool_name: "bash".into(),
                output: serde_json::json!("out"),
                status: "success".into(),
            },
        )
        .expect("tool0");
        let llm1 = map_capture(&mut ordinals, &captured(&run, "b", false)).expect("llm1");

        // THEN LLM and tool each own a 0-based sequence (llm: 0,1 ; tool: 0)
        assert_eq!(snapshot_of(&llm0).step_ordinal, 0);
        assert_eq!(snapshot_of(&llm1).step_ordinal, 1);
        let tool_snap: ToolOutputSnapshot =
            serde_json::from_value(tool0.payload.clone()).expect("tool snapshot");
        assert_eq!(tool_snap.step_ordinal, 0);
    }
}
