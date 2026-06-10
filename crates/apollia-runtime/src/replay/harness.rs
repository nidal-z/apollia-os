//! Deterministic replay harness.
//!
//! Loads a run's captured trace from the audit journal, then re-derives the run
//! from the captured inputs in `step_ordinal` order: each LLM response is pulled
//! from the [`ReplayBackend`], and for every tool call it requested the next
//! captured tool output is consumed. The harness compares the captured tool-call
//! sequence against the captured outputs and reports the run as identical,
//! diverged, or failed (incomplete trace). No network access, no live LLM.
//!
//! Scope note: this drives the replay from the captured input cursors (the same
//! source the injectors wrap). Re-running the live [`crate::chat::BuiltInChatAgent`]
//! end to end (to also catch agent-code drift) requires the original input
//! prompt, which the journal does not capture, and is deferred; the injectors in
//! [`crate::replay::injectors`] are ready for that integration.

use apollia_core::events::RunId;
use apollia_llm::types::{CompletionModel, CompletionRequest};
use serde::{Deserialize, Serialize};

use crate::audit_journal::handle::AuditJournalHandle;
use crate::replay::capture::{ReplayBundle, ReplayCaptureError, ToolReplayCursor};
use crate::replay::injectors::ReplayBackend;

/// Outcome of replaying a captured run against its trace.
#[derive(Debug)]
pub enum ReplayReport {
    /// Every replayed output matched the captured trace exactly.
    Identical,
    /// At least one output differed from the captured trace.
    Diverged {
        /// Every divergence found, exhaustively (no short-circuit).
        divergences: Vec<ReplayDivergence>,
    },
    /// Replay could not complete because the trace was missing an entry.
    Failed {
        /// Why the replay could not finish.
        reason: ReplayFailReason,
    },
}

/// A single point where the replayed run differs from its captured trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayDivergence {
    /// Step ordinal of the captured entry that mismatched.
    pub step_ordinal: u32,
    /// Capture kind that mismatched (e.g. `"ToolOutput"`).
    pub kind: String,
    /// What the trace expected, as JSON.
    pub expected: serde_json::Value,
    /// What the replay produced, as JSON.
    pub actual: serde_json::Value,
}

/// Why a replay could not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayFailReason {
    /// A required entry was missing for a step the replay reached.
    IncompleteTrace {
        /// Run being replayed.
        run_id: String,
        /// Step ordinal whose entry was missing.
        step: u32,
        /// Capture kind that was missing.
        kind: String,
    },
    /// Captured ordinals had a hole.
    OrdinalGap {
        /// Step where the gap was found.
        step: u32,
        /// Ordinal expected at that step.
        expected: u32,
        /// Ordinal actually found.
        found: u32,
    },
    /// The replayed agent returned an error.
    AgentError(String),
}

/// Errors raised while constructing or driving a replay.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// Loading the trace from the journal failed.
    #[error("journal access failed: {0}")]
    Journal(String),
    /// The trace lacks the minimum entries to replay (no captured LLM response).
    #[error("incomplete trace for run {run_id}: missing {kind} at step {step}")]
    IncompleteTrace {
        /// Run that cannot be replayed.
        run_id: String,
        /// Step ordinal that was missing.
        step: u32,
        /// Capture kind that was missing.
        kind: String,
    },
    /// The replayed agent raised an error.
    #[error("replay agent error: {0}")]
    Agent(String),
}

/// Entry point for deterministic replay of a single run.
pub struct ReplayHarness {
    run_id: String,
    backend: ReplayBackend,
    tools: ToolReplayCursor,
}

impl ReplayHarness {
    /// Build a harness from a pre-loaded [`ReplayBundle`].
    ///
    /// Used by the journal loader and by tests; consumes the LLM and tool
    /// cursors (clock and random cursors are reserved for the live-agent path).
    #[must_use]
    pub fn from_bundle(run_id: &RunId, bundle: ReplayBundle) -> Self {
        Self {
            run_id: run_id.as_str().to_string(),
            backend: ReplayBackend::new(bundle.llm),
            tools: bundle.tools,
        }
    }

    /// Build a harness from the audit journal for the given `run_id`.
    ///
    /// # Errors
    ///
    /// - [`ReplayError::IncompleteTrace`] when the run has no captured LLM
    ///   response (the minimum for a replayable trace). Returned before any
    ///   replay runs (Principle #4: fail fast).
    pub async fn from_journal(
        handle: &AuditJournalHandle,
        run_id: &RunId,
    ) -> Result<Self, ReplayError> {
        let entries = handle.query_run(run_id.as_str()).await;
        let bundle = ReplayBundle::from_journal(&entries, run_id).map_err(|e| match e {
            ReplayCaptureError::NoCaptures { run_id } => ReplayError::IncompleteTrace {
                run_id,
                step: 0,
                kind: "LlmCompletion".to_string(),
            },
            other => ReplayError::Journal(other.to_string()),
        })?;
        Ok(Self::from_bundle(run_id, bundle))
    }

    /// Replay the captured run and compare it to its trace.
    ///
    /// Re-derives the run from the captured LLM responses: for each response,
    /// every requested tool call consumes the next captured tool output, which
    /// must match (same tool call id and name) or a [`ReplayDivergence`] is
    /// recorded. A tool output missing for a step the replay reaches yields
    /// [`ReplayReport::Failed`] with the step, without a panic. All divergences
    /// are collected before returning (no short-circuit).
    ///
    /// # Errors
    ///
    /// - [`ReplayError::Agent`] if the replay backend fails unexpectedly while a
    ///   step was still expected.
    pub async fn run(&mut self) -> Result<ReplayReport, ReplayError> {
        let req = CompletionRequest::default();
        let mut divergences = Vec::new();

        loop {
            // The LLM cursor exhausting means the run reached its captured end.
            let response = match self.backend.complete(req.clone()).await {
                Ok(response) => response,
                Err(_) => break,
            };

            // A response without tool calls is the final answer: the loop ends.
            if response.tool_calls.is_empty() {
                break;
            }

            for call in &response.tool_calls {
                match self.tools.next() {
                    Ok(output) => {
                        if output.tool_name != call.name || output.tool_call_id != call.id {
                            divergences.push(ReplayDivergence {
                                step_ordinal: output.step_ordinal,
                                kind: "ToolOutput".to_string(),
                                expected: serde_json::json!({
                                    "tool_call_id": call.id,
                                    "tool_name": call.name,
                                }),
                                actual: serde_json::json!({
                                    "tool_call_id": output.tool_call_id,
                                    "tool_name": output.tool_name,
                                }),
                            });
                        }
                    }
                    Err(ReplayCaptureError::StepExhausted { available, .. }) => {
                        tracing::error!(
                            run_id = %self.run_id,
                            step = available,
                            kind = "ToolOutput",
                            "replay.incomplete_trace"
                        );
                        return Ok(ReplayReport::Failed {
                            reason: ReplayFailReason::IncompleteTrace {
                                run_id: self.run_id.clone(),
                                step: available,
                                kind: "ToolOutput".to_string(),
                            },
                        });
                    }
                    Err(ReplayCaptureError::OrdinalGap {
                        expected, found, ..
                    }) => {
                        return Ok(ReplayReport::Failed {
                            reason: ReplayFailReason::OrdinalGap {
                                step: found,
                                expected,
                                found,
                            },
                        });
                    }
                    Err(ReplayCaptureError::NoCaptures { .. }) => break,
                }
            }
        }

        if divergences.is_empty() {
            Ok(ReplayReport::Identical)
        } else {
            Ok(ReplayReport::Diverged { divergences })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};
    use crate::replay::capture::{LlmCompletionSnapshot, ToolOutputSnapshot};

    fn llm_entry(run: &RunId, ordinal: u32, tool_calls: Vec<serde_json::Value>) -> JournalEntry {
        let snap = LlmCompletionSnapshot {
            run_id: run.clone(),
            step_ordinal: ordinal,
            backend_name: "local".into(),
            model_id: "m".into(),
            content: format!("turn {ordinal}"),
            tool_calls,
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
            stream_truncated: false,
        };
        entry(run, ordinal, JournalEntryKind::LlmCompletion, &snap)
    }

    fn tool_entry(run: &RunId, ordinal: u32, call_id: &str, tool_name: &str) -> JournalEntry {
        let snap = ToolOutputSnapshot {
            run_id: run.clone(),
            step_ordinal: ordinal,
            tool_call_id: call_id.into(),
            tool_name: tool_name.into(),
            output: serde_json::json!("ok"),
            status: "success".into(),
        };
        entry(run, ordinal, JournalEntryKind::ToolOutput, &snap)
    }

    fn entry<T: serde::Serialize>(
        run: &RunId,
        ordinal: u32,
        kind: JournalEntryKind,
        snap: &T,
    ) -> JournalEntry {
        JournalEntry {
            seq: u64::from(ordinal),
            run_id: run.as_str().to_string(),
            ts: "2026-06-10T00:00:00Z".into(),
            kind,
            payload: serde_json::to_value(snap).expect("serialize"),
            prev_hash: "x".into(),
            hash: "y".into(),
            signature: None,
            signing_key_id: None,
        }
    }

    fn call(id: &str, name: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "name": name, "arguments": {} })
    }

    fn harness(run: &RunId, entries: &[JournalEntry]) -> ReplayHarness {
        let bundle = ReplayBundle::from_journal(entries, run).expect("bundle");
        ReplayHarness::from_bundle(run, bundle)
    }

    // AC-1: a run whose tool calls all match their captured outputs is identical.
    #[tokio::test]
    async fn test_replay_identical_run_returns_identical_report() {
        // GIVEN one tool turn (calls c1/bash) then a final text turn
        let run = RunId::new();
        let entries = vec![
            llm_entry(&run, 0, vec![call("c1", "bash")]),
            llm_entry(&run, 1, vec![]),
            tool_entry(&run, 0, "c1", "bash"),
        ];
        let mut harness = harness(&run, &entries);

        // WHEN the run replays
        let report = harness.run().await.expect("run");

        // THEN it is identical
        assert!(matches!(report, ReplayReport::Identical), "got {report:?}");
    }

    // AC-2: a tool output missing for a reached step yields Failed with the step.
    #[tokio::test]
    async fn test_replay_incomplete_trace_returns_failed_with_step() {
        // GIVEN an LLM turn that calls a tool but no captured ToolOutput exists
        let run = RunId::new();
        let entries = vec![llm_entry(&run, 0, vec![call("c1", "bash")])];
        let mut harness = harness(&run, &entries);

        // WHEN the run replays and reaches the tool call
        let report = harness.run().await.expect("run");

        // THEN it fails with IncompleteTrace at step 0 for ToolOutput
        match report {
            ReplayReport::Failed {
                reason: ReplayFailReason::IncompleteTrace { step, kind, .. },
            } => {
                assert_eq!(step, 0);
                assert_eq!(kind, "ToolOutput");
            }
            other => panic!("expected IncompleteTrace, got {other:?}"),
        }
    }

    // AC-3: every divergence is reported, with no short-circuit.
    #[tokio::test]
    async fn test_replay_divergence_reports_all_differences() {
        // GIVEN two tool turns whose captured outputs name a different tool
        let run = RunId::new();
        let entries = vec![
            llm_entry(&run, 0, vec![call("c1", "bash")]),
            llm_entry(&run, 1, vec![call("c2", "web_search")]),
            llm_entry(&run, 2, vec![]),
            tool_entry(&run, 0, "c1", "python"), // diverges from "bash"
            tool_entry(&run, 1, "c2", "http"),   // diverges from "web_search"
        ];
        let mut harness = harness(&run, &entries);

        // WHEN the run replays
        let report = harness.run().await.expect("run");

        // THEN both divergences are reported
        match report {
            ReplayReport::Diverged { divergences } => {
                assert_eq!(divergences.len(), 2);
                assert_eq!(divergences[0].step_ordinal, 0);
                assert_eq!(divergences[1].step_ordinal, 1);
            }
            other => panic!("expected Diverged, got {other:?}"),
        }
    }

    // Building a harness for a run with no LLM captures fails fast.
    #[tokio::test]
    async fn test_from_bundle_requires_llm_captures() {
        // GIVEN a journal with only a tool output and no LLM response
        let run = RunId::new();
        let entries = vec![tool_entry(&run, 0, "c1", "bash")];

        // WHEN building the bundle
        let result = ReplayBundle::from_journal(&entries, &run);

        // THEN it reports NoCaptures (the harness loader maps this to IncompleteTrace)
        assert!(matches!(result, Err(ReplayCaptureError::NoCaptures { .. })));
    }
}
