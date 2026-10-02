//! Governance events and their journal form.

use serde::{Deserialize, Serialize};

use crate::audit_journal::{JournalEntryDraft, JournalEntryKind};
use crate::governance::contract::GovernanceError;

/// Prefix of the journal run that holds the governance history of one agent.
const GOVERNANCE_RUN_PREFIX: &str = "governance:";

/// Journal run holding the governance history of `agent_id`.
///
/// Each agent gets its own chain, so its history reads in order with
/// `query_run` and verifies with `verify_chain`; the global chain of the
/// journal still weaves it with every other run.
pub fn governance_run_id(agent_id: &str) -> String {
    format!("{GOVERNANCE_RUN_PREFIX}{agent_id}")
}

/// Direction of a change of access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessChange {
    /// The access was given to the agent.
    Granted,
    /// The access was taken from the agent.
    Revoked,
}

/// What happened to a local override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideChange {
    /// The user's value was written.
    Set,
    /// The user's value was removed; the package default applies again.
    Removed,
}

/// An event governance records: who did what to which agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GovernanceEvent {
    /// An access was granted to or revoked from an agent.
    AccessChanged {
        /// Agent whose accesses changed.
        agent_id: String,
        /// Access that changed.
        access_id: String,
        /// Whether it was granted or revoked.
        change: AccessChange,
        /// Who made the change (a user, `policy`, `package`).
        actor: String,
    },
    /// An agent was installed.
    Installed {
        /// Installed agent.
        agent_id: String,
        /// Installed version.
        version: String,
        /// Whether the package carried a verified signature.
        signed: bool,
        /// Who installed it.
        actor: String,
    },
    /// An installed agent was updated.
    Updated {
        /// Updated agent.
        agent_id: String,
        /// Version before the update.
        from_version: String,
        /// Version after the update.
        to_version: String,
        /// Whether the incoming package carried a verified signature.
        signed: bool,
        /// Who updated it.
        actor: String,
    },
    /// A local override of an agent was set or removed.
    OverrideChanged {
        /// Agent whose override changed.
        agent_id: String,
        /// Overridden field (`name`, `mission`, `examples`, `access`,
        /// `sources`).
        field: String,
        /// Whether the value was written or removed.
        change: OverrideChange,
        /// Who made the change.
        actor: String,
    },
    /// An agent was uninstalled.
    Uninstalled {
        /// Uninstalled agent.
        agent_id: String,
        /// Version that was removed.
        version: String,
        /// Who uninstalled it.
        actor: String,
    },
}

impl GovernanceEvent {
    /// Agent the event is about.
    pub fn agent_id(&self) -> &str {
        match self {
            GovernanceEvent::AccessChanged { agent_id, .. }
            | GovernanceEvent::Installed { agent_id, .. }
            | GovernanceEvent::Updated { agent_id, .. }
            | GovernanceEvent::Uninstalled { agent_id, .. }
            | GovernanceEvent::OverrideChanged { agent_id, .. } => agent_id,
        }
    }

    /// Journal kind the event is recorded under.
    pub fn journal_kind(&self) -> JournalEntryKind {
        match self {
            GovernanceEvent::AccessChanged { .. } => JournalEntryKind::AccessChanged,
            GovernanceEvent::Installed { .. } => JournalEntryKind::AgentInstalled,
            GovernanceEvent::Updated { .. } => JournalEntryKind::AgentUpdated,
            GovernanceEvent::Uninstalled { .. } => JournalEntryKind::AgentUninstalled,
            GovernanceEvent::OverrideChanged { .. } => JournalEntryKind::AgentOverrideChanged,
        }
    }
}

/// Payload of a governance journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceRecord {
    /// The recorded event.
    #[serde(flatten)]
    pub event: GovernanceEvent,
}

impl GovernanceRecord {
    /// Builds the journal draft for `event`, stamped at `ts` (RFC3339 UTC).
    pub(crate) fn draft(
        event: &GovernanceEvent,
        ts: String,
    ) -> Result<JournalEntryDraft, GovernanceError> {
        let record = GovernanceRecord {
            event: event.clone(),
        };
        let payload = serde_json::to_value(&record).map_err(GovernanceError::Serialize)?;
        Ok(JournalEntryDraft {
            run_id: governance_run_id(event.agent_id()),
            ts,
            kind: event.journal_kind(),
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each event maps to its own journal kind and to the run of its agent.
    #[test]
    fn test_draft_maps_kind_and_run() {
        // GIVEN one event of each nature for the same agent
        let events = [
            (
                GovernanceEvent::AccessChanged {
                    agent_id: "a1".into(),
                    access_id: "web_fetch".into(),
                    change: AccessChange::Granted,
                    actor: "user".into(),
                },
                JournalEntryKind::AccessChanged,
            ),
            (
                GovernanceEvent::Installed {
                    agent_id: "a1".into(),
                    version: "1.0.0".into(),
                    signed: false,
                    actor: "user".into(),
                },
                JournalEntryKind::AgentInstalled,
            ),
            (
                GovernanceEvent::Updated {
                    agent_id: "a1".into(),
                    from_version: "1.0.0".into(),
                    to_version: "1.1.0".into(),
                    signed: true,
                    actor: "user".into(),
                },
                JournalEntryKind::AgentUpdated,
            ),
            (
                GovernanceEvent::Uninstalled {
                    agent_id: "a1".into(),
                    version: "1.1.0".into(),
                    actor: "user".into(),
                },
                JournalEntryKind::AgentUninstalled,
            ),
            (
                GovernanceEvent::OverrideChanged {
                    agent_id: "a1".into(),
                    field: "mission".into(),
                    change: OverrideChange::Set,
                    actor: "user".into(),
                },
                JournalEntryKind::AgentOverrideChanged,
            ),
        ];
        for (event, kind) in events {
            // WHEN the draft is built
            let draft =
                GovernanceRecord::draft(&event, "2026-01-01T00:00:00Z".into()).expect("draft");
            // THEN it carries the kind, the agent run, and a payload that
            // round-trips to the same event
            assert_eq!(draft.kind, kind);
            assert_eq!(draft.run_id, "governance:a1");
            let back: GovernanceRecord =
                serde_json::from_value(draft.payload).expect("payload round trip");
            assert_eq!(back.event, event);
        }
    }

    // The payload names who did what, readable without the Rust types.
    #[test]
    fn test_access_changed_payload_shape() {
        // GIVEN a revocation made by the deposited policy
        let event = GovernanceEvent::AccessChanged {
            agent_id: "a2".into(),
            access_id: "mcp:github".into(),
            change: AccessChange::Revoked,
            actor: "policy".into(),
        };
        // WHEN the draft is built
        let draft = GovernanceRecord::draft(&event, "2026-01-01T00:00:00Z".into()).expect("draft");
        // THEN the payload is a flat object with the event tag and its fields
        assert_eq!(
            draft.payload,
            serde_json::json!({
                "event": "access_changed",
                "agent_id": "a2",
                "access_id": "mcp:github",
                "change": "revoked",
                "actor": "policy",
            })
        );
    }
}
