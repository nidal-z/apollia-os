//! Journal entry types: the kind taxonomy and the chained entry record.

use apollia_core::plan::{Plan, PlanMutationKind, PlanStep};
use serde::{Deserialize, Serialize};

/// Kind of a single audit journal entry.
///
/// Maps the significant lifecycle events of a run (tool calls, LLM calls, agent
/// transitions, escalations). Any `RuntimeEvent` that is run-scoped but not
/// explicitly mapped falls back to [`JournalEntryKind::Unknown`] so the hash
/// chain never has a silent hole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEntryKind {
    /// A tool invocation started.
    ToolCallStarted,
    /// A tool invocation completed.
    ToolCallCompleted,
    /// An LLM call was dispatched.
    LlmCallStarted,
    /// An LLM call returned.
    LlmCallCompleted,
    /// A full LLM response captured for deterministic replay. The payload is an
    /// [`crate::replay::LlmCompletionSnapshot`].
    LlmCompletion,
    /// A tool output captured for deterministic replay. The payload is a
    /// [`crate::replay::ToolOutputSnapshot`].
    ToolOutput,
    /// A clock reading captured for deterministic replay. The payload is a
    /// [`crate::replay::ClockSample`].
    ClockSample,
    /// A random draw captured for deterministic replay. The payload is a
    /// [`crate::replay::RandomSample`].
    RandomSample,
    /// A single mutation of the conversational plan was applied. The payload is
    /// a [`PlanMutationSnapshot`] carrying both the single-step delta and the
    /// full resulting plan, so plan construction is replayable.
    PlanMutation,
    /// An agent became active.
    AgentStarted,
    /// An agent stopped.
    AgentStopped,
    /// An escalation was triggered during the run.
    EscalationTriggered,
    /// A run-scoped event with no explicit mapping; `raw_kind` keeps the
    /// original variant name so coverage stays auditable.
    Unknown {
        /// Original `RuntimeEvent` variant name.
        raw_kind: String,
    },
}

impl JournalEntryKind {
    /// Stable string tag used both for the SQLite `kind` column and for the
    /// canonical hash input. For [`JournalEntryKind::Unknown`] the tag is the
    /// raw variant name, so two different unknown events never collide.
    pub fn tag(&self) -> &str {
        match self {
            JournalEntryKind::ToolCallStarted => "tool_call_started",
            JournalEntryKind::ToolCallCompleted => "tool_call_completed",
            JournalEntryKind::LlmCallStarted => "llm_call_started",
            JournalEntryKind::LlmCallCompleted => "llm_call_completed",
            JournalEntryKind::LlmCompletion => "llm_completion",
            JournalEntryKind::ToolOutput => "tool_output",
            JournalEntryKind::ClockSample => "clock_sample",
            JournalEntryKind::RandomSample => "random_sample",
            JournalEntryKind::PlanMutation => "plan_mutation",
            JournalEntryKind::AgentStarted => "agent_started",
            JournalEntryKind::AgentStopped => "agent_stopped",
            JournalEntryKind::EscalationTriggered => "escalation_triggered",
            JournalEntryKind::Unknown { raw_kind } => raw_kind,
        }
    }

    /// Reconstructs a kind from its stored [`JournalEntryKind::tag`]. Unknown
    /// tags round-trip through [`JournalEntryKind::Unknown`].
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "tool_call_started" => JournalEntryKind::ToolCallStarted,
            "tool_call_completed" => JournalEntryKind::ToolCallCompleted,
            "llm_call_started" => JournalEntryKind::LlmCallStarted,
            "llm_call_completed" => JournalEntryKind::LlmCallCompleted,
            "llm_completion" => JournalEntryKind::LlmCompletion,
            "tool_output" => JournalEntryKind::ToolOutput,
            "clock_sample" => JournalEntryKind::ClockSample,
            "random_sample" => JournalEntryKind::RandomSample,
            "plan_mutation" => JournalEntryKind::PlanMutation,
            "agent_started" => JournalEntryKind::AgentStarted,
            "agent_stopped" => JournalEntryKind::AgentStopped,
            "escalation_triggered" => JournalEntryKind::EscalationTriggered,
            other => JournalEntryKind::Unknown {
                raw_kind: other.to_string(),
            },
        }
    }
}

/// Content of an entry before it is chained.
///
/// The caller supplies only the event content; the journal actor assigns `seq`,
/// links `prev_hash`, and computes `hash`. Chain integrity therefore cannot be
/// forged by a caller, which can never choose a sequence number or a hash.
#[derive(Debug, Clone)]
pub struct JournalEntryDraft {
    /// Identifier of the run this entry belongs to.
    pub run_id: String,
    /// RFC3339 UTC timestamp of the entry.
    pub ts: String,
    /// Kind of the captured event.
    pub kind: JournalEntryKind,
    /// JSON payload extracted from the source event.
    pub payload: serde_json::Value,
}

/// A single append-only audit journal entry.
///
/// Entries are scoped per `run_id` and chained: `prev_hash` of an entry is the
/// `hash` of the previous entry in the same run, the first entry pointing at
/// [`crate::audit_journal::hash::SENTINEL_PREV_HASH`]. The `hash` field commits
/// to all the content fields plus the chain position, making any reorder,
/// deletion, or mutation detectable on recomputation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Monotonic sequence number within the run (starts at 0).
    pub seq: u64,
    /// Identifier of the run this entry belongs to.
    pub run_id: String,
    /// RFC3339 UTC timestamp of the entry.
    pub ts: String,
    /// Kind of the captured event.
    pub kind: JournalEntryKind,
    /// JSON payload extracted from the source event.
    pub payload: serde_json::Value,
    /// Hash of the previous entry in the same run (sentinel for the first one).
    pub prev_hash: String,
    /// SHA256 commitment over the content fields and `prev_hash`.
    pub hash: String,
    /// Base64url (no padding) signature of `hash`. `None` when no signer is
    /// configured or under the warn-and-continue degraded mode.
    #[serde(default)]
    pub signature: Option<String>,
    /// Opaque identifier of the key that produced `signature`. `None` when
    /// `signature` is `None`.
    #[serde(default)]
    pub signing_key_id: Option<String>,
}

/// Captured plan mutation for deterministic replay of plan construction.
///
/// Appended for every `RuntimeEvent::PlanUpdated` that resolves to an open run.
/// The snapshot freezes both the single mutation that produced the new revision
/// (`kind`, `step_id`, `reason`, `before`, `after`) and the full resulting
/// `plan`. The single-step `before`/`after` delta is lossy for a `Propose`
/// mutation, which builds a whole multi-step plan in one move; the full `plan`
/// snapshot closes that gap, so plan construction is replayable from the journal
/// without re-deriving it from a stream of single-step deltas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanMutationSnapshot {
    /// Stable run identifier this mutation belongs to.
    pub run_id: String,
    /// Chat session that owns the plan.
    pub session_id: String,
    /// 0-based ordinal of this mutation within the run (strictly increasing,
    /// contiguous).
    pub ordinal: u32,
    /// Kind of the mutation (Propose, AddStep, ModifyStep, RemoveStep, Reorder,
    /// StatusChange, Submit, Approve, Reject).
    pub kind: PlanMutationKind,
    /// Identifier of the affected step, `None` for whole-plan mutations
    /// (Propose, Reorder, Submit, Approve, Reject).
    pub step_id: Option<String>,
    /// Human-readable reason recorded by the agent or the user, if any.
    pub reason: Option<String>,
    /// State of the affected step before the mutation. `None` for AddStep and
    /// Propose (no prior single-step state). This is the single-step delta and is
    /// lossy for Propose; rely on `plan` for the full state.
    pub before: Option<PlanStep>,
    /// State of the affected step after the mutation. `None` for RemoveStep. This
    /// is the single-step delta and is lossy for Propose; rely on `plan` for the
    /// full state.
    pub after: Option<PlanStep>,
    /// Monotonic plan revision number after the mutation was applied.
    pub revision: u32,
    /// Full resulting plan after the mutation. Captured in addition to the
    /// single-step delta so a multi-step `Propose` is fully replayable: the
    /// delta alone cannot reconstruct a plan built in one move.
    pub plan: Plan,
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::plan::{Plan, PlanScope, PlanStatus, PlanStep};

    fn sample_plan() -> Plan {
        Plan {
            plan_id: "p1".into(),
            scope: PlanScope::Session("sess-1".into()),
            revision: 2,
            status: PlanStatus::Draft,
            steps: vec![PlanStep::new("s1", "first"), PlanStep::new("s2", "second")],
        }
    }

    // The plan_mutation kind round-trips through its stable tag.
    #[test]
    fn test_plan_mutation_tag_round_trips() {
        // GIVEN the PlanMutation kind
        let kind = JournalEntryKind::PlanMutation;
        // WHEN converting to its tag and back
        let tag = kind.tag();
        let back = JournalEntryKind::from_tag(tag);
        // THEN the tag is stable and the round trip is identity
        assert_eq!(tag, "plan_mutation");
        assert_eq!(back, kind);
    }

    // A snapshot with Some deltas round-trips through JSON unchanged.
    #[test]
    fn test_plan_mutation_snapshot_round_trip_with_deltas() {
        // GIVEN a ModifyStep snapshot carrying before/after, reason and a full plan
        let before = PlanStep::new("s1", "first");
        let mut after = PlanStep::new("s1", "first");
        after.title = "first revised".into();
        let snapshot = PlanMutationSnapshot {
            run_id: "run-1".into(),
            session_id: "sess-1".into(),
            ordinal: 3,
            kind: PlanMutationKind::ModifyStep,
            step_id: Some("s1".into()),
            reason: Some("dependency added".into()),
            before: Some(before),
            after: Some(after),
            revision: 2,
            plan: sample_plan(),
        };
        // WHEN serialized to a value and back
        let value = serde_json::to_value(&snapshot).expect("serialize");
        let back: PlanMutationSnapshot = serde_json::from_value(value).expect("deserialize");
        // THEN the reconstructed snapshot equals the original
        assert_eq!(back, snapshot);
    }

    // A snapshot with None deltas (Propose) round-trips, full plan preserved.
    #[test]
    fn test_plan_mutation_snapshot_round_trip_propose_none_deltas() {
        // GIVEN a Propose snapshot with no single-step deltas
        let snapshot = PlanMutationSnapshot {
            run_id: "run-1".into(),
            session_id: "sess-1".into(),
            ordinal: 0,
            kind: PlanMutationKind::Propose,
            step_id: None,
            reason: None,
            before: None,
            after: None,
            revision: 1,
            plan: sample_plan(),
        };
        // WHEN round-tripped through JSON
        let value = serde_json::to_value(&snapshot).expect("serialize");
        let back: PlanMutationSnapshot = serde_json::from_value(value).expect("deserialize");
        // THEN the None deltas and the full plan are preserved
        assert_eq!(back, snapshot);
        assert_eq!(back.before, None);
        assert_eq!(back.after, None);
        assert_eq!(back.plan.steps.len(), 2);
    }
}
