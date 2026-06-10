//! Replay capture extraction: read captured inputs back out of the audit
//! journal so a replay harness can re-inject them.
//!
//! This module never touches the network and never subscribes to the EventBus.
//! It operates on an immutable slice of [`JournalEntry`] handed in by the
//! caller, deserializing the typed payloads back into ordered cursors.

use apollia_core::events::RunId;
use serde::{Deserialize, Serialize};

use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};

/// Captured LLM completion for deterministic replay.
///
/// Persisted in the audit journal payload of every `LlmCompletion` entry. When
/// the stream was interrupted before normal termination, `stream_truncated` is
/// `true` and `content` holds the partial text received: the entry stays usable
/// for replay instead of being dropped silently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmCompletionSnapshot {
    /// Stable run identifier this response belongs to.
    pub run_id: RunId,
    /// 0-based ordinal of this LLM call within the run (strictly increasing,
    /// contiguous).
    pub step_ordinal: u32,
    /// Backend that produced the response (best-effort label).
    pub backend_name: String,
    /// Model identifier (best-effort; may be empty for the streaming path).
    pub model_id: String,
    /// Full response text, or partial text when `stream_truncated` is true.
    pub content: String,
    /// Tool calls returned by the model (empty if none or if the stream was cut).
    pub tool_calls: Vec<serde_json::Value>,
    /// Prompt token count when known, `0` otherwise.
    pub prompt_tokens: u32,
    /// Completion token count when known, `0` otherwise.
    pub completion_tokens: u32,
    /// Cost in USD if the backend reported it, `None` for local backends.
    pub cost_usd: Option<f64>,
    /// True when the stream was cut before a normal finish reason.
    pub stream_truncated: bool,
}

/// Errors raised while extracting or advancing captured inputs for replay.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplayCaptureError {
    /// The journal holds no captures of the requested kind for the run.
    #[error("no LLM captures found in journal for run {run_id}")]
    NoCaptures {
        /// Run that has no captures of the requested kind.
        run_id: String,
    },
    /// A cursor was advanced past its last captured entry.
    #[error(
        "step exhausted: replay requested step {requested} but only {available} steps captured"
    )]
    StepExhausted {
        /// Ordinal the caller asked for.
        requested: u32,
        /// Number of captured entries available.
        available: u32,
    },
    /// The captured ordinals are not contiguous from 0 (a missing entry).
    #[error("step ordinal gap at position {position}: expected {expected}, found {found}")]
    OrdinalGap {
        /// Index in the ordered sequence where the gap was found.
        position: usize,
        /// Ordinal expected at that position.
        expected: u32,
        /// Ordinal actually found.
        found: u32,
    },
}

/// Ordered sequence of captured LLM responses for a single run.
///
/// Built from an immutable journal slice and advanced by the replay harness in
/// strict `step_ordinal` order. It performs no network or LLM calls and never
/// mutates agent state.
#[derive(Debug, Clone)]
pub struct LlmReplayCursor {
    snapshots: Vec<LlmCompletionSnapshot>,
    position: usize,
}

impl LlmReplayCursor {
    /// Build a cursor from the audit journal entries of a given run.
    ///
    /// Filters `LlmCompletion` entries for `run_id`, deserializes their payload,
    /// and orders them by `step_ordinal`. The ordinals must be contiguous from
    /// 0; a missing entry yields [`ReplayCaptureError::OrdinalGap`].
    ///
    /// # Errors
    ///
    /// - [`ReplayCaptureError::NoCaptures`] when the run has no `LlmCompletion`
    ///   entries (an incomplete trace; replay cannot proceed).
    /// - [`ReplayCaptureError::OrdinalGap`] when the captured ordinals are not
    ///   contiguous from 0.
    pub fn from_journal(
        entries: &[JournalEntry],
        run_id: &RunId,
    ) -> Result<Self, ReplayCaptureError> {
        let snapshots = ordered_snapshots(entries, run_id)?;
        Ok(Self {
            snapshots,
            position: 0,
        })
    }

    /// Advance and return the next captured response, in `step_ordinal` order.
    ///
    /// # Errors
    ///
    /// - [`ReplayCaptureError::StepExhausted`] when no more entries remain.
    // REASON: not an Iterator. Exhaustion is a terminal Err the replay harness
    // reports as a divergence/failure, not the Option::None of normal iteration.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<LlmCompletionSnapshot, ReplayCaptureError> {
        match self.snapshots.get(self.position) {
            Some(snapshot) => {
                self.position += 1;
                Ok(snapshot.clone())
            }
            None => Err(ReplayCaptureError::StepExhausted {
                requested: self.position as u32,
                available: self.snapshots.len() as u32,
            }),
        }
    }

    /// Number of captured responses in this cursor.
    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// True when no responses were captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

/// Filter, deserialize and order the `LlmCompletion` snapshots of a run.
///
/// Shared by [`LlmReplayCursor::from_journal`]; validates non-emptiness and the
/// contiguous-ordinal invariant.
fn ordered_snapshots(
    entries: &[JournalEntry],
    run_id: &RunId,
) -> Result<Vec<LlmCompletionSnapshot>, ReplayCaptureError> {
    let mut snapshots: Vec<LlmCompletionSnapshot> = entries
        .iter()
        .filter(|e| e.run_id == run_id.as_str() && e.kind == JournalEntryKind::LlmCompletion)
        .filter_map(|e| serde_json::from_value(e.payload.clone()).ok())
        .collect();

    if snapshots.is_empty() {
        return Err(ReplayCaptureError::NoCaptures {
            run_id: run_id.as_str().to_string(),
        });
    }

    snapshots.sort_by_key(|s| s.step_ordinal);

    for (position, snapshot) in snapshots.iter().enumerate() {
        let expected = position as u32;
        if snapshot.step_ordinal != expected {
            return Err(ReplayCaptureError::OrdinalGap {
                position,
                expected,
                found: snapshot.step_ordinal,
            });
        }
    }

    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::entry::JournalEntry;

    fn snapshot(run_id: &RunId, ordinal: u32) -> LlmCompletionSnapshot {
        LlmCompletionSnapshot {
            run_id: run_id.clone(),
            step_ordinal: ordinal,
            backend_name: "local".into(),
            model_id: "test-model".into(),
            content: format!("response {ordinal}"),
            tool_calls: vec![],
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
            stream_truncated: false,
        }
    }

    fn entry(run_id: &RunId, ordinal: u32) -> JournalEntry {
        JournalEntry {
            seq: u64::from(ordinal),
            run_id: run_id.as_str().to_string(),
            ts: "2026-06-10T00:00:00Z".into(),
            kind: JournalEntryKind::LlmCompletion,
            payload: serde_json::to_value(snapshot(run_id, ordinal)).expect("serialize"),
            prev_hash: "x".into(),
            hash: "y".into(),
            signature: None,
            signing_key_id: None,
        }
    }

    #[test]
    fn test_cursor_errors_on_empty_journal() {
        // GIVEN a journal with no LlmCompletion entries for the run
        let run = RunId::new();
        let entries: Vec<JournalEntry> = vec![];

        // WHEN building a cursor
        let result = LlmReplayCursor::from_journal(&entries, &run);

        // THEN it reports NoCaptures
        assert!(matches!(result, Err(ReplayCaptureError::NoCaptures { .. })));
    }

    #[test]
    fn test_cursor_next_returns_in_ordinal_order() {
        // GIVEN a cursor over three snapshots out of order in the slice
        let run = RunId::new();
        let entries = vec![entry(&run, 2), entry(&run, 0), entry(&run, 1)];
        let mut cursor = LlmReplayCursor::from_journal(&entries, &run).expect("cursor");

        // WHEN next() is called three times
        // THEN snapshots come back in step_ordinal order 0,1,2
        assert_eq!(cursor.next().expect("0").step_ordinal, 0);
        assert_eq!(cursor.next().expect("1").step_ordinal, 1);
        assert_eq!(cursor.next().expect("2").step_ordinal, 2);

        // AND a fourth call is exhausted
        assert!(matches!(
            cursor.next(),
            Err(ReplayCaptureError::StepExhausted { .. })
        ));
    }

    #[test]
    fn test_cursor_detects_ordinal_gap() {
        // GIVEN snapshots at ordinals 0 and 2 (the step 1 entry is missing)
        let run = RunId::new();
        let entries = vec![entry(&run, 0), entry(&run, 2)];

        // WHEN building a cursor
        let result = LlmReplayCursor::from_journal(&entries, &run);

        // THEN the missing step is reported as an OrdinalGap
        assert!(matches!(
            result,
            Err(ReplayCaptureError::OrdinalGap {
                position: 1,
                expected: 1,
                found: 2,
            })
        ));
    }

    #[test]
    fn test_cursor_isolates_by_run() {
        // GIVEN entries from two distinct runs interleaved
        let run_a = RunId::new();
        let run_b = RunId::new();
        let entries = vec![
            entry(&run_a, 0),
            entry(&run_b, 0),
            entry(&run_a, 1),
            entry(&run_b, 1),
        ];

        // WHEN building a cursor for run A
        let cursor = LlmReplayCursor::from_journal(&entries, &run_a).expect("cursor");

        // THEN it sees only run A's two captures
        assert_eq!(cursor.len(), 2);
    }
}
