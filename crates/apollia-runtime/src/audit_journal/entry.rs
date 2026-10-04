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
    /// A message was sent to another agent's mailbox. The payload is a
    /// [`MessageSnapshot`].
    MessageSent,
    /// A mailbox message was leased to its recipient. The payload is a
    /// [`MessageSnapshot`].
    MessageDelivered,
    /// A mailbox message was dropped (queue full). The payload is a
    /// [`MessageSnapshot`].
    MessageDropped,
    /// An access was granted to or revoked from an agent. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    AccessChanged,
    /// An agent was installed. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    AgentInstalled,
    /// An installed agent was updated. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    AgentUpdated,
    /// An agent was uninstalled. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    AgentUninstalled,
    /// A local override of an agent was set or removed. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    AgentOverrideChanged,
    /// A package was installed. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    PackageInstalled,
    /// An installed package was updated. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    PackageUpdated,
    /// A package was uninstalled. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    PackageUninstalled,
    /// A connection was added: an MCP server, a native account, or a folder. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    ConnectionAdded,
    /// The configuration of a connection was replaced. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    ConnectionModified,
    /// A connection was removed. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    ConnectionRemoved,
    /// The user accepted the warning shown before a connection is added. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    ConnectionWarningAccepted,
    /// A secret a connection names was set. The value is never journaled. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    ConnectionSecretSet,
    /// A secret a connection names was removed. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    ConnectionSecretRemoved,
    /// A native connector account was connected. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    AccountConnected,
    /// A native connector account was disconnected. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    AccountDisconnected,
    /// A folder of this machine was added as a connection. The payload is a
    /// [`crate::governance::ConnectionRecord`].
    FolderAdded,
    /// The user answered a consent prompt: once, always, or refuse. The payload is a
    /// [`crate::governance::GovernanceRecord`]; it never carries the call's argument.
    ConsentAnswered,
    /// A standing consent ("always") was withdrawn. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    AlwaysWithdrawn,
    /// The organisation policy refused a call. The payload is a
    /// [`crate::governance::GovernanceRecord`]; it never carries the call's argument.
    PolicyRefused,
    /// A fine rule refused a call. The payload is a
    /// [`crate::governance::GovernanceRecord`]; it never carries the call's argument.
    RuleRefused,
    /// A fine rule was added. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    RuleAdded,
    /// A fine rule was removed. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    RuleRemoved,
    /// A family was granted to an agent for one batch run. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    BatchAccessGranted,
    /// The organisation policy was applied at startup. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    PolicyApplied,
    /// The policy applied at startup differs from the one applied before. The
    /// payload is a [`crate::governance::GovernanceRecord`].
    PolicyChanged,
    /// A deposited policy was refused at startup. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    PolicyUnreadable,
    /// Governance entered fail-closed: it refuses what is sensitive. The
    /// payload is a [`crate::governance::GovernanceRecord`].
    FailClosedTriggered,
    /// The journal was exported. The payload is a
    /// [`crate::governance::GovernanceRecord`].
    JournalExported,
    /// `apollia.toml` changed. The payload is a
    /// [`crate::config_reload::ConfigChangeRecord`]: the paths of the keys,
    /// the origin, the fingerprint of the file before and after, never a value.
    ConfigurationChanged,
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
            JournalEntryKind::MessageSent => "message_sent",
            JournalEntryKind::MessageDelivered => "message_delivered",
            JournalEntryKind::MessageDropped => "message_dropped",
            JournalEntryKind::AccessChanged => "access_changed",
            JournalEntryKind::AgentInstalled => "agent_installed",
            JournalEntryKind::AgentUpdated => "agent_updated",
            JournalEntryKind::AgentUninstalled => "agent_uninstalled",
            JournalEntryKind::AgentOverrideChanged => "agent_override_changed",
            JournalEntryKind::PackageInstalled => "package_installed",
            JournalEntryKind::PackageUpdated => "package_updated",
            JournalEntryKind::PackageUninstalled => "package_uninstalled",
            JournalEntryKind::ConnectionAdded => "connection_added",
            JournalEntryKind::ConnectionModified => "connection_modified",
            JournalEntryKind::ConnectionRemoved => "connection_removed",
            JournalEntryKind::ConnectionWarningAccepted => "connection_warning_accepted",
            JournalEntryKind::ConnectionSecretSet => "connection_secret_set",
            JournalEntryKind::ConnectionSecretRemoved => "connection_secret_removed",
            JournalEntryKind::AccountConnected => "account_connected",
            JournalEntryKind::AccountDisconnected => "account_disconnected",
            JournalEntryKind::FolderAdded => "folder_added",
            JournalEntryKind::ConsentAnswered => "consent_answered",
            JournalEntryKind::AlwaysWithdrawn => "always_withdrawn",
            JournalEntryKind::PolicyRefused => "policy_refused",
            JournalEntryKind::RuleRefused => "rule_refused",
            JournalEntryKind::RuleAdded => "rule_added",
            JournalEntryKind::RuleRemoved => "rule_removed",
            JournalEntryKind::BatchAccessGranted => "batch_access_granted",
            JournalEntryKind::PolicyApplied => "policy_applied",
            JournalEntryKind::PolicyChanged => "policy_changed",
            JournalEntryKind::PolicyUnreadable => "policy_unreadable",
            JournalEntryKind::FailClosedTriggered => "fail_closed_triggered",
            JournalEntryKind::JournalExported => "journal_exported",
            JournalEntryKind::ConfigurationChanged => "configuration_changed",
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
            "message_sent" => JournalEntryKind::MessageSent,
            "message_delivered" => JournalEntryKind::MessageDelivered,
            "message_dropped" => JournalEntryKind::MessageDropped,
            "access_changed" => JournalEntryKind::AccessChanged,
            "agent_installed" => JournalEntryKind::AgentInstalled,
            "agent_updated" => JournalEntryKind::AgentUpdated,
            "agent_uninstalled" => JournalEntryKind::AgentUninstalled,
            "agent_override_changed" => JournalEntryKind::AgentOverrideChanged,
            "package_installed" => JournalEntryKind::PackageInstalled,
            "package_updated" => JournalEntryKind::PackageUpdated,
            "package_uninstalled" => JournalEntryKind::PackageUninstalled,
            "connection_added" => JournalEntryKind::ConnectionAdded,
            "connection_modified" => JournalEntryKind::ConnectionModified,
            "connection_removed" => JournalEntryKind::ConnectionRemoved,
            "connection_warning_accepted" => JournalEntryKind::ConnectionWarningAccepted,
            "connection_secret_set" => JournalEntryKind::ConnectionSecretSet,
            "connection_secret_removed" => JournalEntryKind::ConnectionSecretRemoved,
            "account_connected" => JournalEntryKind::AccountConnected,
            "account_disconnected" => JournalEntryKind::AccountDisconnected,
            "folder_added" => JournalEntryKind::FolderAdded,
            "consent_answered" => JournalEntryKind::ConsentAnswered,
            "always_withdrawn" => JournalEntryKind::AlwaysWithdrawn,
            "policy_refused" => JournalEntryKind::PolicyRefused,
            "rule_refused" => JournalEntryKind::RuleRefused,
            "rule_added" => JournalEntryKind::RuleAdded,
            "rule_removed" => JournalEntryKind::RuleRemoved,
            "batch_access_granted" => JournalEntryKind::BatchAccessGranted,
            "policy_applied" => JournalEntryKind::PolicyApplied,
            "policy_changed" => JournalEntryKind::PolicyChanged,
            "policy_unreadable" => JournalEntryKind::PolicyUnreadable,
            "fail_closed_triggered" => JournalEntryKind::FailClosedTriggered,
            "journal_exported" => JournalEntryKind::JournalExported,
            "configuration_changed" => JournalEntryKind::ConfigurationChanged,
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
///
/// `Eq` is not derived because the embedded `PlanStep`/`Plan` carry a non-`Eq`
/// `serde_json::Value` (the step arguments). `PartialEq` is retained, so the
/// replay harness's equality checks are unaffected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Captured mailbox message event for the audit chain.
///
/// Recorded for a `MessageSent`, `MessageDelivered`, or `MessageDropped` entry.
/// The payload content is represented by its `payload_hash` (SHA-256) by
/// default; the full payload is included only when the runtime is configured to
/// record it (regulated / high assurance), so the journal proves what was
/// exchanged without necessarily storing the content at rest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageSnapshot {
    /// Unique identifier of the message.
    pub message_id: String,
    /// Name of the sending agent (or `host:<id>`). `None` for delivery, where
    /// only the recipient side is known to the event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Name of the receiving agent.
    pub to: String,
    /// SHA-256 hash of the payload, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_hash: Option<String>,
    /// Drop cause for a `MessageDropped` entry (`"queue_full"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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

    // The governance kinds round-trip through their stable tags.
    #[test]
    fn test_governance_kinds_tags_round_trip() {
        // GIVEN the governance kinds and their expected tags
        let cases = [
            (JournalEntryKind::AccessChanged, "access_changed"),
            (JournalEntryKind::AgentInstalled, "agent_installed"),
            (JournalEntryKind::AgentUpdated, "agent_updated"),
            (JournalEntryKind::AgentUninstalled, "agent_uninstalled"),
            (
                JournalEntryKind::AgentOverrideChanged,
                "agent_override_changed",
            ),
            (JournalEntryKind::PackageInstalled, "package_installed"),
            (JournalEntryKind::PackageUpdated, "package_updated"),
            (JournalEntryKind::PackageUninstalled, "package_uninstalled"),
        ];
        for (kind, expected) in cases {
            // WHEN converting to its tag and back
            let tag = kind.tag().to_string();
            let back = JournalEntryKind::from_tag(&tag);
            // THEN the tag is stable and the round trip is identity
            assert_eq!(tag, expected);
            assert_eq!(back, kind);
        }
    }

    // The connection kinds round-trip through their stable tags.
    #[test]
    fn test_connection_kinds_tags_round_trip() {
        // GIVEN the connection kinds and their expected tags
        let cases = [
            (JournalEntryKind::ConnectionAdded, "connection_added"),
            (JournalEntryKind::ConnectionModified, "connection_modified"),
            (JournalEntryKind::ConnectionRemoved, "connection_removed"),
            (
                JournalEntryKind::ConnectionWarningAccepted,
                "connection_warning_accepted",
            ),
            (
                JournalEntryKind::ConnectionSecretSet,
                "connection_secret_set",
            ),
            (
                JournalEntryKind::ConnectionSecretRemoved,
                "connection_secret_removed",
            ),
            (JournalEntryKind::AccountConnected, "account_connected"),
            (
                JournalEntryKind::AccountDisconnected,
                "account_disconnected",
            ),
            (JournalEntryKind::FolderAdded, "folder_added"),
        ];
        for (kind, expected) in cases {
            // WHEN converting to its tag and back
            let tag = kind.tag().to_string();
            let back = JournalEntryKind::from_tag(&tag);
            // THEN the tag is stable and the round trip is identity
            assert_eq!(tag, expected);
            assert_eq!(back, kind);
        }
    }

    // The kinds of consents, refusals, rules, batches, policy and
    // configuration round-trip through their stable tags.
    #[test]
    fn test_decision_and_policy_kinds_tags_round_trip() {
        // GIVEN the kinds and their expected tags
        let cases = [
            (JournalEntryKind::ConsentAnswered, "consent_answered"),
            (JournalEntryKind::AlwaysWithdrawn, "always_withdrawn"),
            (JournalEntryKind::PolicyRefused, "policy_refused"),
            (JournalEntryKind::RuleRefused, "rule_refused"),
            (JournalEntryKind::RuleAdded, "rule_added"),
            (JournalEntryKind::RuleRemoved, "rule_removed"),
            (JournalEntryKind::BatchAccessGranted, "batch_access_granted"),
            (JournalEntryKind::PolicyApplied, "policy_applied"),
            (JournalEntryKind::PolicyChanged, "policy_changed"),
            (JournalEntryKind::PolicyUnreadable, "policy_unreadable"),
            (
                JournalEntryKind::FailClosedTriggered,
                "fail_closed_triggered",
            ),
            (JournalEntryKind::JournalExported, "journal_exported"),
            (
                JournalEntryKind::ConfigurationChanged,
                "configuration_changed",
            ),
        ];
        for (kind, expected) in cases {
            // WHEN converting to its tag and back
            let tag = kind.tag().to_string();
            let back = JournalEntryKind::from_tag(&tag);
            // THEN the tag is stable and the round trip is identity
            assert_eq!(tag, expected);
            assert_eq!(back, kind);
        }
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
