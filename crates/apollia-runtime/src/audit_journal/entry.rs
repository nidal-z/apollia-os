//! Journal entry types: the kind taxonomy and the chained entry record.

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
