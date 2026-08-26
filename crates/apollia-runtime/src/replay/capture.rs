//! Replay capture extraction: read captured inputs back out of the audit
//! journal so a replay harness can re-inject them.
//!
//! This module never touches the network and never subscribes to the EventBus.
//! It operates on an immutable slice of [`JournalEntry`] handed in by the
//! caller, deserializing the typed payloads back into ordered cursors.
//!
//! Every captured input type ([`LlmCompletionSnapshot`], [`ToolOutputSnapshot`],
//! [`ClockSample`], [`RandomSample`]) owns a contiguous, 0-based `step_ordinal`
//! sequence within a run, so a missing entry surfaces as an
//! [`ReplayCaptureError::OrdinalGap`] instead of a silent divergence.

use apollia_core::events::RunId;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::audit_journal::entry::{JournalEntry, JournalEntryKind, PlanMutationSnapshot};

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

/// Captured tool output for deterministic replay.
///
/// Persisted in the payload of every `ToolOutput` entry. Carries the full
/// output (not the truncated preview), so the replay harness can return the
/// exact bytes the live tool produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolOutputSnapshot {
    /// Run this tool call belongs to.
    pub run_id: RunId,
    /// 0-based ordinal of this tool output within the run (contiguous).
    pub step_ordinal: u32,
    /// Identifier of the originating `ToolCall` (matches `ToolCall::id`).
    pub tool_call_id: String,
    /// Name of the invoked tool.
    pub tool_name: String,
    /// Full tool output, serialized as JSON (string body, object, or error text).
    pub output: serde_json::Value,
    /// Outcome of the call: `"success"`, `"error"`, or `"rejected"`.
    pub status: String,
}

/// A single clock reading consumed by run logic.
///
/// Captured so the replay harness can return the exact timestamp the original
/// run observed instead of the live wall clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSample {
    /// Run that read the clock.
    pub run_id: RunId,
    /// 0-based ordinal of this clock read within the run (contiguous).
    pub step_ordinal: u32,
    /// Unix timestamp in milliseconds that was observed.
    pub timestamp_ms: u64,
}

/// A single random draw consumed by run logic.
///
/// Captured so the replay harness can return the exact bytes the original run
/// drew. A draw that reaches the journal without having been recorded carries
/// `captured = false`, marking a capture bug explicitly rather than letting the
/// replay diverge silently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RandomSample {
    /// Run that drew the value.
    pub run_id: RunId,
    /// 0-based ordinal of this draw within the run (contiguous).
    pub step_ordinal: u32,
    /// Raw bytes of the random value (16 bytes is enough for a UUID).
    pub bytes: Vec<u8>,
    /// `false` when the draw was detected as un-captured (a capture bug); the
    /// entry is still journaled and a warning is emitted, never a silent gap.
    pub captured: bool,
    /// Call-site hint (function or module path) for diagnosing a capture bug.
    pub source_site: String,
}

/// Errors raised while extracting or advancing captured inputs for replay.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayCaptureError {
    /// The journal holds no captures of the requested kind for the run.
    #[error("no captures found in journal for run {run_id}")]
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

/// A journal-captured input that carries a per-run step ordinal and maps to a
/// single [`JournalEntryKind`].
///
/// Implemented by every capture snapshot so [`ReplayCursor`] can extract and
/// order them generically.
pub trait CapturedEntry: Clone + DeserializeOwned {
    /// The per-run, contiguous ordinal of this capture.
    fn step_ordinal(&self) -> u32;
    /// The journal kind that stores this capture in its payload.
    fn kind() -> JournalEntryKind;
}

impl CapturedEntry for LlmCompletionSnapshot {
    fn step_ordinal(&self) -> u32 {
        self.step_ordinal
    }
    fn kind() -> JournalEntryKind {
        JournalEntryKind::LlmCompletion
    }
}

impl CapturedEntry for ToolOutputSnapshot {
    fn step_ordinal(&self) -> u32 {
        self.step_ordinal
    }
    fn kind() -> JournalEntryKind {
        JournalEntryKind::ToolOutput
    }
}

impl CapturedEntry for ClockSample {
    fn step_ordinal(&self) -> u32 {
        self.step_ordinal
    }
    fn kind() -> JournalEntryKind {
        JournalEntryKind::ClockSample
    }
}

impl CapturedEntry for RandomSample {
    fn step_ordinal(&self) -> u32 {
        self.step_ordinal
    }
    fn kind() -> JournalEntryKind {
        JournalEntryKind::RandomSample
    }
}

impl CapturedEntry for PlanMutationSnapshot {
    fn step_ordinal(&self) -> u32 {
        self.ordinal
    }
    fn kind() -> JournalEntryKind {
        JournalEntryKind::PlanMutation
    }
}

/// Ordered sequence of one captured input type for a single run.
///
/// Built from an immutable journal slice and advanced by the replay harness in
/// strict `step_ordinal` order. It performs no network or LLM calls and never
/// mutates agent state.
#[derive(Debug, Clone)]
pub struct ReplayCursor<T> {
    items: Vec<T>,
    position: usize,
}

impl<T: CapturedEntry> ReplayCursor<T> {
    /// Build a cursor from the audit journal entries of a given run.
    ///
    /// Filters entries of `T`'s kind for `run_id`, deserializes their payload,
    /// and orders them by `step_ordinal`. The ordinals must be contiguous from
    /// 0; a missing entry (an incomplete trace) yields
    /// [`ReplayCaptureError::OrdinalGap`].
    ///
    /// # Errors
    ///
    /// - [`ReplayCaptureError::NoCaptures`] when the run has no entries of this
    ///   kind (replay cannot proceed).
    /// - [`ReplayCaptureError::OrdinalGap`] when the captured ordinals are not
    ///   contiguous from 0.
    pub fn from_journal(
        entries: &[JournalEntry],
        run_id: &RunId,
    ) -> Result<Self, ReplayCaptureError> {
        let items = ordered_captures::<T>(entries, run_id)?;
        Ok(Self { items, position: 0 })
    }

    /// Build a cursor, treating an absent kind as an empty (not failed) cursor.
    ///
    /// Used for optional input categories in a [`ReplayBundle`] (a run may have
    /// no tool calls, no clock reads, or no random draws). An ordinal gap is
    /// still an error.
    ///
    /// # Errors
    ///
    /// - [`ReplayCaptureError::OrdinalGap`] when the present ordinals have a hole.
    pub fn from_journal_optional(
        entries: &[JournalEntry],
        run_id: &RunId,
    ) -> Result<Self, ReplayCaptureError> {
        match Self::from_journal(entries, run_id) {
            Ok(cursor) => Ok(cursor),
            Err(ReplayCaptureError::NoCaptures { .. }) => Ok(Self {
                items: Vec::new(),
                position: 0,
            }),
            Err(other) => Err(other),
        }
    }

    /// Advance and return the next captured value, in `step_ordinal` order.
    ///
    /// # Errors
    ///
    /// - [`ReplayCaptureError::StepExhausted`] when no more entries remain.
    // REASON: not an Iterator. Exhaustion is a terminal Err the replay harness
    // reports as a divergence/failure, not the Option::None of normal iteration.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<T, ReplayCaptureError> {
        match self.items.get(self.position) {
            Some(item) => {
                self.position += 1;
                Ok(item.clone())
            }
            None => Err(ReplayCaptureError::StepExhausted {
                requested: self.position as u32,
                available: self.items.len() as u32,
            }),
        }
    }

    /// Number of captured values in this cursor.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when no values were captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// True when every captured value has been consumed by [`ReplayCursor::next`].
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.position >= self.items.len()
    }

    /// The full ordered sequence of captured values, regardless of cursor
    /// position. Used by the harness to drive a comparison from the captured
    /// trace itself when no independent produced stream is available.
    #[must_use]
    pub fn snapshots(&self) -> &[T] {
        &self.items
    }
}

/// Ordered cursor over captured LLM responses.
pub type LlmReplayCursor = ReplayCursor<LlmCompletionSnapshot>;
/// Ordered cursor over captured tool outputs.
pub type ToolReplayCursor = ReplayCursor<ToolOutputSnapshot>;
/// Ordered cursor over captured clock samples.
pub type ClockReplayCursor = ReplayCursor<ClockSample>;
/// Ordered cursor over captured random draws.
pub type RandomReplayCursor = ReplayCursor<RandomSample>;
/// Ordered cursor over captured plan mutations.
///
/// Used by the replay harness to compare the plan-construction stream produced
/// by the replayed run against the captured trace, mutation by mutation. An
/// empty cursor is valid: a run may construct no plan.
pub type PlanMutationReplayCursor = ReplayCursor<PlanMutationSnapshot>;

/// All captured inputs of a single run, ready for replay injection.
///
/// The LLM cursor is mandatory: a run with no captured LLM response is not
/// replayable. Tool, clock and random cursors are optional (empty when the run
/// drew none).
#[derive(Debug, Clone)]
pub struct ReplayBundle {
    /// Captured LLM responses (mandatory; a replayable run has at least one).
    pub llm: LlmReplayCursor,
    /// Captured tool outputs (empty when the run invoked no tools).
    pub tools: ToolReplayCursor,
    /// Captured clock reads (empty when the run read no clock).
    pub clock: ClockReplayCursor,
    /// Captured random draws (empty when the run drew no randomness).
    pub random: RandomReplayCursor,
    /// Captured plan mutations (empty when the run constructed no plan).
    pub plan: PlanMutationReplayCursor,
}

impl ReplayBundle {
    /// Build all cursors from the journal entries of a given run.
    ///
    /// # Errors
    ///
    /// - [`ReplayCaptureError::NoCaptures`] when no LLM entries exist (the
    ///   minimum for a replayable trace).
    /// - [`ReplayCaptureError::OrdinalGap`] when any present category has a hole.
    pub fn from_journal(
        entries: &[JournalEntry],
        run_id: &RunId,
    ) -> Result<Self, ReplayCaptureError> {
        Ok(Self {
            llm: LlmReplayCursor::from_journal(entries, run_id)?,
            tools: ToolReplayCursor::from_journal_optional(entries, run_id)?,
            clock: ClockReplayCursor::from_journal_optional(entries, run_id)?,
            random: RandomReplayCursor::from_journal_optional(entries, run_id)?,
            plan: PlanMutationReplayCursor::from_journal_optional(entries, run_id)?,
        })
    }
}

/// Filter, deserialize and order the captures of one kind for a run.
///
/// Validates non-emptiness and the contiguous-ordinal invariant. Shared by
/// every [`ReplayCursor`].
fn ordered_captures<T: CapturedEntry>(
    entries: &[JournalEntry],
    run_id: &RunId,
) -> Result<Vec<T>, ReplayCaptureError> {
    let mut items: Vec<T> = entries
        .iter()
        .filter(|e| e.run_id == run_id.as_str() && e.kind == T::kind())
        .filter_map(|e| serde_json::from_value(e.payload.clone()).ok())
        .collect();

    if items.is_empty() {
        return Err(ReplayCaptureError::NoCaptures {
            run_id: run_id.as_str().to_string(),
        });
    }

    items.sort_by_key(CapturedEntry::step_ordinal);

    for (position, item) in items.iter().enumerate() {
        let expected = position as u32;
        if item.step_ordinal() != expected {
            return Err(ReplayCaptureError::OrdinalGap {
                position,
                expected,
                found: item.step_ordinal(),
            });
        }
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::entry::JournalEntry;

    fn llm_snapshot(run_id: &RunId, ordinal: u32) -> LlmCompletionSnapshot {
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

    fn tool_snapshot(run_id: &RunId, ordinal: u32) -> ToolOutputSnapshot {
        ToolOutputSnapshot {
            run_id: run_id.clone(),
            step_ordinal: ordinal,
            tool_call_id: format!("call-{ordinal}"),
            tool_name: "bash_executor".into(),
            output: serde_json::json!({ "stdout": ordinal }),
            status: "success".into(),
        }
    }

    fn entry<T: CapturedEntry + Serialize>(run_id: &RunId, ordinal: u32, snap: &T) -> JournalEntry {
        JournalEntry {
            seq: u64::from(ordinal),
            run_id: run_id.as_str().to_string(),
            ts: "2026-06-10T00:00:00Z".into(),
            kind: T::kind(),
            payload: serde_json::to_value(snap).expect("serialize"),
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
        let entries = vec![
            entry(&run, 2, &llm_snapshot(&run, 2)),
            entry(&run, 0, &llm_snapshot(&run, 0)),
            entry(&run, 1, &llm_snapshot(&run, 1)),
        ];
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
        let entries = vec![
            entry(&run, 0, &llm_snapshot(&run, 0)),
            entry(&run, 2, &llm_snapshot(&run, 2)),
        ];

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
            entry(&run_a, 0, &llm_snapshot(&run_a, 0)),
            entry(&run_b, 0, &llm_snapshot(&run_b, 0)),
            entry(&run_a, 1, &llm_snapshot(&run_a, 1)),
            entry(&run_b, 1, &llm_snapshot(&run_b, 1)),
        ];

        // WHEN building a cursor for run A
        let cursor = LlmReplayCursor::from_journal(&entries, &run_a).expect("cursor");

        // THEN it sees only run A's two captures
        assert_eq!(cursor.len(), 2);
    }

    #[test]
    fn test_tool_cursor_returns_in_ordinal_order() {
        // GIVEN three ToolOutput entries at ordinals 0,1,2
        let run = RunId::new();
        let entries = vec![
            entry(&run, 0, &tool_snapshot(&run, 0)),
            entry(&run, 1, &tool_snapshot(&run, 1)),
            entry(&run, 2, &tool_snapshot(&run, 2)),
        ];
        let mut cursor = ToolReplayCursor::from_journal(&entries, &run).expect("cursor");

        // WHEN next() is called
        // THEN outputs come back in order and exhaust cleanly
        assert_eq!(cursor.next().expect("0").tool_call_id, "call-0");
        assert_eq!(cursor.next().expect("1").tool_call_id, "call-1");
        assert_eq!(cursor.next().expect("2").tool_call_id, "call-2");
        assert!(matches!(
            cursor.next(),
            Err(ReplayCaptureError::StepExhausted { .. })
        ));
    }

    #[test]
    fn test_tool_cursor_detects_ordinal_gap() {
        // GIVEN ToolOutput entries at ordinals 0 and 2 (step 1 missing)
        let run = RunId::new();
        let entries = vec![
            entry(&run, 0, &tool_snapshot(&run, 0)),
            entry(&run, 2, &tool_snapshot(&run, 2)),
        ];

        // WHEN building the tool cursor
        let result = ToolReplayCursor::from_journal(&entries, &run);

        // THEN the gap is reported explicitly
        assert!(matches!(
            result,
            Err(ReplayCaptureError::OrdinalGap {
                position: 1,
                expected: 1,
                found: 2,
            })
        ));
    }

    fn plan_snapshot(run_id: &RunId, ordinal: u32) -> PlanMutationSnapshot {
        use apollia_core::plan::{Plan, PlanMutationKind, PlanScope, PlanStatus, PlanStep};
        PlanMutationSnapshot {
            run_id: run_id.as_str().to_string(),
            session_id: "sess-1".into(),
            ordinal,
            kind: PlanMutationKind::AddStep,
            step_id: Some(format!("s{ordinal}")),
            reason: None,
            before: None,
            after: Some(PlanStep::new(format!("s{ordinal}"), "step")),
            revision: ordinal + 1,
            plan: Plan {
                plan_id: "p1".into(),
                scope: PlanScope::Session("sess-1".into()),
                revision: ordinal + 1,
                status: PlanStatus::Draft,
                steps: vec![],
            },
        }
    }

    // The plan cursor returns snapshots in ordinal order and exhausts cleanly
    #[test]
    fn test_plan_cursor_next_in_ordinal_order() {
        // GIVEN three PlanMutation entries at ordinals 0,1,2 (out of slice order)
        let run = RunId::new();
        let entries = vec![
            entry(&run, 2, &plan_snapshot(&run, 2)),
            entry(&run, 0, &plan_snapshot(&run, 0)),
            entry(&run, 1, &plan_snapshot(&run, 1)),
        ];
        let mut cursor = PlanMutationReplayCursor::from_journal(&entries, &run).expect("cursor");

        // WHEN next() is called three times
        // THEN snapshots come back in ordinal order 0,1,2
        assert_eq!(cursor.next().expect("0").ordinal, 0);
        assert_eq!(cursor.next().expect("1").ordinal, 1);
        assert_eq!(cursor.next().expect("2").ordinal, 2);

        // AND a fourth call is exhausted
        assert!(matches!(
            cursor.next(),
            Err(ReplayCaptureError::StepExhausted { .. })
        ));
        assert!(cursor.is_exhausted());
    }

    // A hole in the plan ordinals is reported as an OrdinalGap (no panic)
    #[test]
    fn test_plan_cursor_ordinal_gap_errors() {
        // GIVEN plan entries at ordinals 0,1,3 (the step 2 entry is missing)
        let run = RunId::new();
        let entries = vec![
            entry(&run, 0, &plan_snapshot(&run, 0)),
            entry(&run, 1, &plan_snapshot(&run, 1)),
            entry(&run, 3, &plan_snapshot(&run, 3)),
        ];

        // WHEN building the cursor
        let result = PlanMutationReplayCursor::from_journal(&entries, &run);

        // THEN the gap is reported explicitly at position 2 (expected 2, found 3)
        assert!(matches!(
            result,
            Err(ReplayCaptureError::OrdinalGap {
                position: 2,
                expected: 2,
                found: 3,
            })
        ));
    }

    // An empty plan sequence is valid (a run may construct no plan)
    #[test]
    fn test_plan_cursor_empty_is_valid() {
        // GIVEN a journal with no PlanMutation entry for the run
        let run = RunId::new();
        let entries = vec![entry(&run, 0, &llm_snapshot(&run, 0))];

        // WHEN building the optional plan cursor
        let cursor = PlanMutationReplayCursor::from_journal_optional(&entries, &run).expect("ok");

        // THEN it is an empty, already-exhausted cursor (not an error)
        assert!(cursor.is_empty());
        assert!(cursor.is_exhausted());
    }

    #[test]
    fn test_clock_and_random_cursors_extract_their_kind() {
        // GIVEN a run with two clock samples and one random draw
        let run = RunId::new();
        let clock = |o: u32| ClockSample {
            run_id: run.clone(),
            step_ordinal: o,
            timestamp_ms: 1_700_000_000_000 + u64::from(o),
        };
        let random = RandomSample {
            run_id: run.clone(),
            step_ordinal: 0,
            bytes: vec![1, 2, 3, 4],
            captured: true,
            source_site: "hitl.request_id".into(),
        };
        let entries = vec![
            entry(&run, 0, &clock(0)),
            entry(&run, 1, &clock(1)),
            entry(&run, 0, &random),
        ];

        // WHEN building each typed cursor
        let mut clock_cursor = ClockReplayCursor::from_journal(&entries, &run).expect("clock");
        let mut random_cursor = RandomReplayCursor::from_journal(&entries, &run).expect("random");

        // THEN each sees only its own kind, in order
        assert_eq!(clock_cursor.len(), 2);
        assert_eq!(
            clock_cursor.next().expect("0").timestamp_ms,
            1_700_000_000_000
        );
        assert_eq!(random_cursor.len(), 1);
        assert_eq!(random_cursor.next().expect("0").bytes, vec![1, 2, 3, 4]);
    }
}
