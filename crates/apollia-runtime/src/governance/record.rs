//! Governance events and their journal form.

use serde::{Deserialize, Serialize};

use apollia_permissions::{ConsentFamily, RuleRecord, StoreEvent};
use apollia_tools::ActionFamily;

use crate::audit_journal::{JournalEntryDraft, JournalEntryKind};
use crate::governance::contract::{ConsentAnswer, GovernanceError};

/// Prefix of the journal run that holds the governance history of one agent.
const GOVERNANCE_RUN_PREFIX: &str = "governance:";

/// Prefix of the journal run that holds the history of one package.
const PACKAGE_RUN_PREFIX: &str = "governance:package:";

/// Journal run holding the organisation policy applied at each start, and
/// every entry into fail-closed.
pub const POLICY_RUN_ID: &str = "governance:policy:organisation";

/// Journal run holding the fine rules that bind every agent.
pub const GLOBAL_RULES_RUN_ID: &str = "governance:rules:global";

/// Journal run holding the exports of the journal.
pub const JOURNAL_EXPORT_RUN_ID: &str = "governance:journal:export";

/// Journal run holding what happens to the journal itself: a purge by
/// retention, an opening without its signing key.
pub const JOURNAL_STATE_RUN_ID: &str = "governance:journal:state";

/// Journal run holding the installations and updates of `package_id`.
pub fn package_run_id(package_id: &str) -> String {
    format!("{PACKAGE_RUN_PREFIX}{package_id}")
}

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
    /// The access was kept with another reach.
    Rescoped,
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
        /// Who made the change (a user, `policy`, `package`, `upgrade`).
        actor: String,
        /// Why the change was made, when the actor states it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
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
        /// `sources`, ...), or `model` for the model assigned to the agent.
        field: String,
        /// Whether the value was written or removed.
        change: OverrideChange,
        /// Who made the change.
        actor: String,
    },
    /// A package was installed: its agents, workflows, tools and the
    /// connections it brings.
    PackageInstalled {
        /// Installed package.
        package_id: String,
        /// Installed version.
        version: String,
        /// Publisher the package declares.
        publisher: String,
        /// Fingerprint of the key that signed it, when it is signed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key_fingerprint: Option<String>,
        /// Whether the package carried a verified signature.
        signed: bool,
        /// Who installed it.
        actor: String,
    },
    /// An installed package was replaced by another version.
    PackageUpdated {
        /// Updated package.
        package_id: String,
        /// Version before the update.
        from_version: String,
        /// Version after the update.
        to_version: String,
        /// Publisher the incoming package declares.
        publisher: String,
        /// Fingerprint of the key that signed the incoming package.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key_fingerprint: Option<String>,
        /// Whether the incoming package carried a verified signature.
        signed: bool,
        /// Who updated it.
        actor: String,
    },
    /// An agent was uninstalled.
    Uninstalled {
        /// Uninstalled agent.
        agent_id: String,
        /// Version that was removed.
        version: String,
        /// Whether its memory was erased rather than kept.
        #[serde(default)]
        memory_cleared: bool,
        /// Whether its history was erased rather than kept.
        #[serde(default)]
        history_cleared: bool,
        /// Who uninstalled it.
        actor: String,
    },
    /// A package was uninstalled: its agents, workflows, tools and the
    /// connections it brought.
    PackageUninstalled {
        /// Uninstalled package.
        package_id: String,
        /// Version that was removed.
        version: String,
        /// Whether the memory of its agents was erased rather than kept.
        memory_cleared: bool,
        /// Whether the history of its agents and workflows was erased rather
        /// than kept.
        history_cleared: bool,
        /// Who uninstalled it.
        actor: String,
    },
    /// The user answered a consent prompt. Neither the call's argument nor
    /// a reason typed with a refusal is recorded.
    ConsentAnswered {
        /// Agent the call was made by.
        agent_id: String,
        /// Access the call used (the tool).
        access_id: String,
        /// Family of the tool.
        family: ActionFamily,
        /// What the user answered, as it took effect.
        answer: ConsentAnswer,
    },
    /// A standing consent ("always") was withdrawn.
    AlwaysWithdrawn {
        /// Agent the consent was given to.
        agent_id: String,
        /// Family it covered.
        family: ConsentFamily,
    },
    /// The organisation policy refused a call.
    PolicyRefused {
        /// Agent the call was made by.
        agent_id: String,
        /// Access the call used (the tool).
        access_id: String,
        /// Family of the tool.
        family: ActionFamily,
        /// What the policy refuses, as shown to the user.
        detail: String,
    },
    /// A fine rule refused a call.
    RuleRefused {
        /// Agent the call was made by.
        agent_id: String,
        /// Access the call used (the tool).
        access_id: String,
        /// Family of the tool.
        family: ActionFamily,
        /// Identifier of the rule in `governance.db`.
        rule_id: i64,
    },
    /// A fine rule was added.
    RuleAdded(RuleRecord),
    /// A fine rule was removed.
    RuleRemoved(RuleRecord),
    /// A family was granted to an agent for one batch run.
    BatchAccessGranted {
        /// Agent the family was granted to.
        agent_id: String,
        /// The batch run the grant holds for.
        batch_id: String,
        /// Family granted.
        family: ConsentFamily,
        /// Who granted it.
        granted_by: String,
    },
    /// The organisation policy was applied at startup.
    PolicyApplied {
        /// Fingerprint of the key that signed the policy.
        key_fingerprint: String,
        /// SHA-256 of `policy.toml`.
        policy_digest: String,
        /// Publisher named by the signature.
        publisher: String,
        /// When the organisation signed the policy.
        signed_at: String,
    },
    /// The policy applied at startup is not the one applied before.
    PolicyChanged {
        /// Fingerprint of the key that signed the new policy.
        key_fingerprint: String,
        /// SHA-256 of the new `policy.toml`.
        policy_digest: String,
        /// SHA-256 of the policy applied before.
        previous_digest: String,
    },
    /// A deposited policy was refused at startup.
    PolicyUnreadable {
        /// Reason class (`schema`, `signature`, `key`, `owner`).
        reason: String,
        /// What exactly was refused.
        detail: String,
    },
    /// The policy fixed a setting the user's file sets otherwise: what the
    /// setting had opened was closed at startup. Neither value is recorded,
    /// the policy holds the one in force.
    PolicySettingEnforced {
        /// Dotted key of the setting, `server.serve_on_lan` for instance.
        key: String,
    },
    /// Governance entered fail-closed.
    FailClosedTriggered {
        /// What cannot be read (`store_unreadable`, `policy_invalid` or
        /// `journal_unavailable`).
        cause: String,
        /// Why, in a sentence.
        detail: String,
    },
    /// The journal was exported.
    JournalExported {
        /// Format of the export (`csv`, `json`, `pdf`).
        format: String,
        /// Number of entries exported.
        entries: u64,
        /// Who exported it.
        actor: String,
        /// What the export was narrowed to.
        #[serde(default)]
        filters: ExportFilters,
        /// Whether the export holds the detail of each entry, beyond its
        /// summary.
        #[serde(default)]
        detail: bool,
    },
    /// Entries older than the retention of the organisation policy were
    /// removed. The entry that says so is where verification starts from.
    JournalPurged {
        /// Days an entry is kept, as the policy sets them.
        retention_days: u32,
        /// Entries dated before this instant were removed (RFC 3339 UTC).
        cutoff: String,
        /// How many entries were removed.
        removed: u64,
        /// The last link of the global chain that was removed: the first
        /// entry kept follows it. `None` while the global chain still starts
        /// at its origin.
        #[serde(default)]
        anchor: Option<PurgeAnchor>,
        /// Per run that lost its first entries and still has some: the last
        /// entry removed, which the first entry kept follows.
        #[serde(default)]
        runs: std::collections::BTreeMap<String, RunCut>,
    },
    /// The journal opened without its signing key.
    JournalUnsigned {
        /// Why, in a sentence.
        detail: String,
    },
    /// The daily export the organisation policy asks for could not be
    /// written.
    JournalExportFailed {
        /// The folder the policy designates.
        directory: String,
        /// Why, in a sentence.
        detail: String,
    },
}

/// The last link of the global chain a purge removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeAnchor {
    /// Its position in the global chain.
    pub global_seq: u64,
    /// Its global hash.
    pub global_hash: String,
}

/// The last entry of a run a purge removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCut {
    /// Its sequence number in the run.
    pub seq: u64,
    /// Its hash.
    pub hash: String,
}

/// The filters an export of the journal was made with, as the journal keeps
/// them. An empty list, or an absent bound, narrowed nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportFilters {
    /// Agents kept.
    #[serde(default)]
    pub agents: Vec<String>,
    /// Action families kept.
    #[serde(default)]
    pub families: Vec<String>,
    /// Natures kept.
    #[serde(default)]
    pub natures: Vec<String>,
    /// Results kept.
    #[serde(default)]
    pub results: Vec<String>,
    /// Prefixes of the tools kept. Left out of the record when it narrowed
    /// nothing, so an export made without it is recorded as before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_prefixes: Vec<String>,
    /// Start of the period, included, RFC 3339 UTC.
    #[serde(default)]
    pub since: Option<String>,
    /// End of the period, excluded, RFC 3339 UTC.
    #[serde(default)]
    pub until: Option<String>,
}

impl From<StoreEvent> for GovernanceEvent {
    fn from(event: StoreEvent) -> Self {
        match event {
            StoreEvent::AlwaysWithdrawn { agent_id, family } => {
                Self::AlwaysWithdrawn { agent_id, family }
            }
            StoreEvent::BatchAccessGranted {
                agent_id,
                batch_id,
                family,
                granted_by,
            } => Self::BatchAccessGranted {
                agent_id,
                batch_id,
                family,
                granted_by,
            },
            StoreEvent::RuleAdded(rule) => Self::RuleAdded(rule),
            StoreEvent::RuleRemoved(rule) => Self::RuleRemoved(rule),
        }
    }
}

impl GovernanceEvent {
    /// Agent, or package, the event is about.
    pub fn subject_id(&self) -> &str {
        match self {
            GovernanceEvent::AccessChanged { agent_id, .. }
            | GovernanceEvent::Installed { agent_id, .. }
            | GovernanceEvent::Updated { agent_id, .. }
            | GovernanceEvent::Uninstalled { agent_id, .. }
            | GovernanceEvent::OverrideChanged { agent_id, .. } => agent_id,
            GovernanceEvent::PackageInstalled { package_id, .. }
            | GovernanceEvent::PackageUpdated { package_id, .. }
            | GovernanceEvent::PackageUninstalled { package_id, .. } => package_id,
            GovernanceEvent::ConsentAnswered { agent_id, .. }
            | GovernanceEvent::AlwaysWithdrawn { agent_id, .. }
            | GovernanceEvent::PolicyRefused { agent_id, .. }
            | GovernanceEvent::RuleRefused { agent_id, .. }
            | GovernanceEvent::BatchAccessGranted { agent_id, .. } => agent_id,
            GovernanceEvent::RuleAdded(rule) | GovernanceEvent::RuleRemoved(rule) => {
                rule.agent_id.as_deref().unwrap_or(&rule.tool_name)
            }
            GovernanceEvent::PolicyApplied { .. }
            | GovernanceEvent::PolicyChanged { .. }
            | GovernanceEvent::PolicyUnreadable { .. }
            | GovernanceEvent::PolicySettingEnforced { .. }
            | GovernanceEvent::FailClosedTriggered { .. } => POLICY_RUN_ID,
            GovernanceEvent::JournalExported { .. }
            | GovernanceEvent::JournalExportFailed { .. } => JOURNAL_EXPORT_RUN_ID,
            GovernanceEvent::JournalPurged { .. } | GovernanceEvent::JournalUnsigned { .. } => {
                JOURNAL_STATE_RUN_ID
            }
        }
    }

    /// Journal run the event is recorded in: the run of its agent, or the run
    /// of its package.
    pub fn run_id(&self) -> String {
        match self {
            GovernanceEvent::PackageInstalled { package_id, .. }
            | GovernanceEvent::PackageUpdated { package_id, .. }
            | GovernanceEvent::PackageUninstalled { package_id, .. } => package_run_id(package_id),
            GovernanceEvent::RuleAdded(rule) | GovernanceEvent::RuleRemoved(rule) => {
                match &rule.agent_id {
                    Some(agent_id) => governance_run_id(agent_id),
                    None => GLOBAL_RULES_RUN_ID.to_owned(),
                }
            }
            GovernanceEvent::PolicyApplied { .. }
            | GovernanceEvent::PolicyChanged { .. }
            | GovernanceEvent::PolicyUnreadable { .. }
            | GovernanceEvent::PolicySettingEnforced { .. }
            | GovernanceEvent::FailClosedTriggered { .. }
            | GovernanceEvent::JournalExported { .. }
            | GovernanceEvent::JournalPurged { .. }
            | GovernanceEvent::JournalUnsigned { .. }
            | GovernanceEvent::JournalExportFailed { .. } => other_run(self),
            other => governance_run_id(other.subject_id()),
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
            GovernanceEvent::PackageInstalled { .. } => JournalEntryKind::PackageInstalled,
            GovernanceEvent::PackageUpdated { .. } => JournalEntryKind::PackageUpdated,
            GovernanceEvent::PackageUninstalled { .. } => JournalEntryKind::PackageUninstalled,
            GovernanceEvent::ConsentAnswered { .. } => JournalEntryKind::ConsentAnswered,
            GovernanceEvent::AlwaysWithdrawn { .. } => JournalEntryKind::AlwaysWithdrawn,
            GovernanceEvent::PolicyRefused { .. } => JournalEntryKind::PolicyRefused,
            GovernanceEvent::RuleRefused { .. } => JournalEntryKind::RuleRefused,
            GovernanceEvent::RuleAdded(_) => JournalEntryKind::RuleAdded,
            GovernanceEvent::RuleRemoved(_) => JournalEntryKind::RuleRemoved,
            GovernanceEvent::BatchAccessGranted { .. } => JournalEntryKind::BatchAccessGranted,
            GovernanceEvent::PolicyApplied { .. } => JournalEntryKind::PolicyApplied,
            GovernanceEvent::PolicyChanged { .. } => JournalEntryKind::PolicyChanged,
            GovernanceEvent::PolicyUnreadable { .. } => JournalEntryKind::PolicyUnreadable,
            GovernanceEvent::PolicySettingEnforced { .. } => {
                JournalEntryKind::PolicySettingEnforced
            }
            GovernanceEvent::FailClosedTriggered { .. } => JournalEntryKind::FailClosedTriggered,
            GovernanceEvent::JournalExported { .. } => JournalEntryKind::JournalExported,
            GovernanceEvent::JournalPurged { .. } => JournalEntryKind::JournalPurged,
            GovernanceEvent::JournalUnsigned { .. } => JournalEntryKind::JournalUnsigned,
            GovernanceEvent::JournalExportFailed { .. } => JournalEntryKind::JournalExportFailed,
        }
    }
}

/// The run of an event that concerns no agent and no package: its subject
/// names the run.
fn other_run(event: &GovernanceEvent) -> String {
    event.subject_id().to_owned()
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
            run_id: event.run_id(),
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
                    reason: None,
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
                    memory_cleared: false,
                    history_cleared: true,
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

    // A package event lands in the run of its package, under its own kind.
    #[test]
    fn test_package_events_map_to_the_package_run() {
        // GIVEN an installation and an update of the same package
        let installed = GovernanceEvent::PackageInstalled {
            package_id: "crm".into(),
            version: "1.0.0".into(),
            publisher: "Acme".into(),
            key_fingerprint: Some("ab".repeat(32)),
            signed: true,
            actor: "api".into(),
        };
        let updated = GovernanceEvent::PackageUpdated {
            package_id: "crm".into(),
            from_version: "1.0.0".into(),
            to_version: "1.1.0".into(),
            publisher: "Acme".into(),
            key_fingerprint: None,
            signed: false,
            actor: "api".into(),
        };

        // WHEN their drafts are built
        let a = GovernanceRecord::draft(&installed, "2026-01-01T00:00:00Z".into()).expect("draft");
        let b = GovernanceRecord::draft(&updated, "2026-01-01T00:00:00Z".into()).expect("draft");

        // THEN both land in the package run, each under its kind, and round-trip
        assert_eq!(a.run_id, "governance:package:crm");
        assert_eq!(b.run_id, "governance:package:crm");
        assert_eq!(a.kind, JournalEntryKind::PackageInstalled);
        assert_eq!(b.kind, JournalEntryKind::PackageUpdated);
        assert_eq!(a.payload["event"], "package_installed");
        assert!(b.payload.get("key_fingerprint").is_none());
        let back: GovernanceRecord = serde_json::from_value(a.payload).expect("round trip");
        assert_eq!(back.event, installed);
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
            reason: None,
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
