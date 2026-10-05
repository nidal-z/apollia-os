//! The summary of a journal entry: who did what to what, with which result
//! and when.
//!
//! A summary is what a reader sees first, so it holds no content. Of a tool
//! call it keeps the tool, its family and the target the emitter wrote at call
//! time, which is the value of the one argument the tool designates and of no
//! other. The arguments and the output stay in the entries captured for
//! replay, and are read on demand through [`crate::audit_journal::detail`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};
use crate::governance::{
    AccessChange, ConnectionEvent, ConnectionRecord, ConsentAnswer, GovernanceEvent,
    GovernanceRecord, OverrideChange,
};

/// What kind of fact an entry records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JournalNature {
    /// An agent acted, or tried to: a tool call, a consent, a refusal.
    Action,
    /// What an agent can reach changed: an access, a connection, a secret, an
    /// installation, a rule.
    AccessChange,
    /// The organisation policy was applied, changed or refused.
    Policy,
    /// Everything the journal keeps for replay and for its own bookkeeping.
    System,
}

impl JournalNature {
    /// The name the nature carries on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            JournalNature::Action => "action",
            JournalNature::AccessChange => "access_change",
            JournalNature::Policy => "policy",
            JournalNature::System => "system",
        }
    }

    /// The nature named `name` on the wire, if there is one.
    pub fn parse(name: &str) -> Option<Self> {
        [
            JournalNature::Action,
            JournalNature::AccessChange,
            JournalNature::Policy,
            JournalNature::System,
        ]
        .into_iter()
        .find(|nature| nature.as_str() == name)
    }
}

/// How what an entry records ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JournalOutcome {
    /// It was done.
    Success,
    /// It was attempted and failed.
    Failure,
    /// It was refused: by the user, by a rule or by the policy.
    Refused,
}

impl JournalOutcome {
    /// The name the outcome carries on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            JournalOutcome::Success => "success",
            JournalOutcome::Failure => "failure",
            JournalOutcome::Refused => "refused",
        }
    }

    /// The outcome named `name` on the wire, if there is one.
    pub fn parse(name: &str) -> Option<Self> {
        [
            JournalOutcome::Success,
            JournalOutcome::Failure,
            JournalOutcome::Refused,
        ]
        .into_iter()
        .find(|outcome| outcome.as_str() == name)
    }
}

/// One journal entry, said in a line and without content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JournalSummary {
    /// Agent the entry is about, when it is about one.
    pub agent: Option<String>,
    /// What kind of fact the entry records.
    pub nature: JournalNature,
    /// Action family concerned (`read`, `write_or_delete`, ...), when there is one.
    pub family: Option<String>,
    /// What happened, in a few words.
    pub what: String,
    /// Tool concerned, when there is one.
    pub tool: Option<String>,
    /// What was acted on: a path, a recipient, the host of an address, a
    /// connection. Never an argument that carries content.
    pub target: Option<String>,
    /// How it ended. `None` for an entry that records no ending, such as the
    /// start of a call.
    pub outcome: Option<JournalOutcome>,
    /// When, RFC 3339 UTC.
    pub ts: String,
}

/// A journal entry together with its summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JournalLine {
    /// The entry as it is chained.
    #[serde(flatten)]
    pub entry: JournalEntry,
    /// Its summary.
    pub summary: JournalSummary,
}

impl JournalLine {
    /// `entry` with its summary.
    pub fn of(entry: JournalEntry) -> Self {
        let summary = summarize(&entry);
        Self { entry, summary }
    }
}

/// The nature of an entry of `kind`. It depends on the kind alone, so a reader
/// can tell it without opening the payload.
pub fn nature_of(kind: &JournalEntryKind) -> JournalNature {
    use JournalEntryKind as K;
    match kind {
        K::ToolCallStarted
        | K::ToolCallCompleted
        | K::ConsentAnswered
        | K::PolicyRefused
        | K::RuleRefused => JournalNature::Action,
        K::AccessChanged
        | K::AgentInstalled
        | K::AgentUpdated
        | K::AgentUninstalled
        | K::AgentOverrideChanged
        | K::PackageInstalled
        | K::PackageUpdated
        | K::PackageUninstalled
        | K::ConnectionAdded
        | K::ConnectionModified
        | K::ConnectionRemoved
        | K::ConnectionWarningAccepted
        | K::ConnectionSecretSet
        | K::ConnectionSecretRemoved
        | K::AccountConnected
        | K::AccountDisconnected
        | K::FolderAdded
        | K::AlwaysWithdrawn
        | K::RuleAdded
        | K::RuleRemoved
        | K::BatchAccessGranted => JournalNature::AccessChange,
        K::PolicyApplied | K::PolicyChanged | K::PolicyUnreadable | K::FailClosedTriggered => {
            JournalNature::Policy
        }
        K::LlmCallStarted
        | K::LlmCallCompleted
        | K::LlmCompletion
        | K::ToolOutput
        | K::ClockSample
        | K::RandomSample
        | K::PlanMutation
        | K::AgentStarted
        | K::AgentStopped
        | K::EscalationTriggered
        | K::MessageSent
        | K::MessageDelivered
        | K::MessageDropped
        | K::JournalExported
        | K::ConfigurationChanged
        | K::Unknown { .. } => JournalNature::System,
    }
}

/// The summary of `entry`.
///
/// Reads the fields the payload names and copies none of the others, so
/// whatever a payload holds beyond them (a model response, a tool output)
/// never reaches a summary.
pub fn summarize(entry: &JournalEntry) -> JournalSummary {
    use JournalEntryKind as K;
    let payload = &entry.payload;
    let mut summary = JournalSummary {
        agent: None,
        nature: nature_of(&entry.kind),
        family: None,
        what: String::new(),
        tool: None,
        target: None,
        outcome: None,
        ts: entry.ts.clone(),
    };
    match &entry.kind {
        K::ToolCallStarted | K::ToolCallCompleted => {
            summary.agent = text(payload, "agent_id");
            summary.family = text(payload, "family");
            summary.tool = text(payload, "tool_name");
            summary.target = text(payload, "target");
            if entry.kind == K::ToolCallStarted {
                summary.what = "Tool call started".to_owned();
            } else {
                summary.what = "Tool call finished".to_owned();
                summary.outcome = payload
                    .get("success")
                    .and_then(Value::as_bool)
                    .map(success_outcome);
            }
        }
        K::LlmCallStarted => {
            summary.what = "Model call started".to_owned();
            summary.target = text(payload, "model");
        }
        K::LlmCallCompleted => {
            summary.what = "Model call finished".to_owned();
            summary.target = text(payload, "model");
            summary.outcome = Some(JournalOutcome::Success);
        }
        K::LlmCompletion => {
            summary.what = "Model response kept for replay".to_owned();
            summary.target = text(payload, "model_id");
        }
        K::ToolOutput => {
            summary.what = "Tool output kept for replay".to_owned();
            summary.tool = text(payload, "tool_name");
            summary.outcome = match payload.get("status").and_then(Value::as_str) {
                Some("success") => Some(JournalOutcome::Success),
                Some("error") => Some(JournalOutcome::Failure),
                Some("rejected") => Some(JournalOutcome::Refused),
                _ => None,
            };
        }
        K::ClockSample => summary.what = "Clock reading kept for replay".to_owned(),
        K::RandomSample => summary.what = "Random draw kept for replay".to_owned(),
        K::PlanMutation => summary.what = "Plan changed".to_owned(),
        K::ConfigurationChanged => {
            summary.what = "Configuration changed".to_owned();
            summary.outcome = Some(JournalOutcome::Success);
        }
        K::AgentStarted => {
            summary.what = "Agent started".to_owned();
            summary.agent = text(payload, "agent_id");
        }
        K::AgentStopped => {
            summary.what = "Agent stopped".to_owned();
            summary.agent = text(payload, "agent_id");
        }
        K::EscalationTriggered => {
            summary.what = "Escalation triggered".to_owned();
            summary.agent = text(payload, "agent_id");
        }
        K::MessageSent | K::MessageDelivered | K::MessageDropped => {
            summary.agent = text(payload, "from");
            summary.target = text(payload, "to");
            match entry.kind {
                K::MessageSent => summary.what = "Message sent to an agent".to_owned(),
                K::MessageDelivered => {
                    summary.what = "Message delivered to an agent".to_owned();
                    summary.outcome = Some(JournalOutcome::Success);
                }
                _ => {
                    summary.what = "Message to an agent dropped".to_owned();
                    summary.outcome = Some(JournalOutcome::Failure);
                }
            }
        }
        K::ConnectionAdded
        | K::ConnectionModified
        | K::ConnectionRemoved
        | K::ConnectionWarningAccepted
        | K::ConnectionSecretSet
        | K::ConnectionSecretRemoved
        | K::AccountConnected
        | K::AccountDisconnected
        | K::FolderAdded => match serde_json::from_value::<ConnectionRecord>(payload.clone()) {
            Ok(record) => connection_summary(&mut summary, &record.event),
            Err(_) => summary.what = entry.kind.tag().to_owned(),
        },
        K::AccessChanged
        | K::AgentInstalled
        | K::AgentUpdated
        | K::AgentUninstalled
        | K::AgentOverrideChanged
        | K::PackageInstalled
        | K::PackageUpdated
        | K::PackageUninstalled
        | K::ConsentAnswered
        | K::AlwaysWithdrawn
        | K::PolicyRefused
        | K::RuleRefused
        | K::RuleAdded
        | K::RuleRemoved
        | K::BatchAccessGranted
        | K::PolicyApplied
        | K::PolicyChanged
        | K::PolicyUnreadable
        | K::FailClosedTriggered
        | K::JournalExported => match serde_json::from_value::<GovernanceRecord>(payload.clone()) {
            Ok(record) => governance_summary(&mut summary, &record.event),
            Err(_) => summary.what = entry.kind.tag().to_owned(),
        },
        K::Unknown { raw_kind } => summary.what = raw_kind.clone(),
    }
    summary
}

fn text(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn success_outcome(success: bool) -> JournalOutcome {
    if success {
        JournalOutcome::Success
    } else {
        JournalOutcome::Failure
    }
}

/// Fills `summary` from a connection event. The target is the connection, the
/// account or the folder; a secret is named by its connection, never shown.
fn connection_summary(summary: &mut JournalSummary, event: &ConnectionEvent) {
    summary.outcome = Some(JournalOutcome::Success);
    summary.target = Some(event.connection_id().to_owned());
    summary.what = match event {
        ConnectionEvent::Added { .. } => "Connection added",
        ConnectionEvent::Modified { .. } => "Connection changed",
        ConnectionEvent::Removed { .. } => "Connection removed",
        ConnectionEvent::WarningAccepted { .. } => "Connection warning accepted",
        ConnectionEvent::SecretSet { .. } => "Secret set on a connection",
        ConnectionEvent::SecretRemoved { .. } => "Secret removed from a connection",
        ConnectionEvent::AccountConnected { .. } => "Account connected",
        ConnectionEvent::AccountDisconnected { .. } => "Account disconnected",
        ConnectionEvent::FolderAdded { path, .. } => {
            summary.target = Some(path.clone());
            "Folder added"
        }
    }
    .to_owned();
}

/// Fills `summary` from a governance event.
fn governance_summary(summary: &mut JournalSummary, event: &GovernanceEvent) {
    summary.outcome = Some(JournalOutcome::Success);
    let what = match event {
        GovernanceEvent::AccessChanged {
            agent_id,
            access_id,
            change,
            ..
        } => {
            summary.agent = Some(agent_id.clone());
            summary.target = Some(access_id.clone());
            match change {
                AccessChange::Granted => "Access granted",
                AccessChange::Revoked => "Access revoked",
                AccessChange::Rescoped => "Access reach changed",
            }
        }
        GovernanceEvent::Installed { agent_id, .. } => {
            summary.agent = Some(agent_id.clone());
            "Agent installed"
        }
        GovernanceEvent::Updated { agent_id, .. } => {
            summary.agent = Some(agent_id.clone());
            "Agent updated"
        }
        GovernanceEvent::Uninstalled { agent_id, .. } => {
            summary.agent = Some(agent_id.clone());
            "Agent uninstalled"
        }
        GovernanceEvent::OverrideChanged {
            agent_id,
            field,
            change,
            ..
        } => {
            summary.agent = Some(agent_id.clone());
            summary.target = Some(field.clone());
            match change {
                OverrideChange::Set => "Agent setting changed",
                OverrideChange::Removed => "Agent setting reset",
            }
        }
        GovernanceEvent::PackageInstalled { package_id, .. } => {
            summary.target = Some(package_id.clone());
            "Package installed"
        }
        GovernanceEvent::PackageUpdated { package_id, .. } => {
            summary.target = Some(package_id.clone());
            "Package updated"
        }
        GovernanceEvent::PackageUninstalled { package_id, .. } => {
            summary.target = Some(package_id.clone());
            "Package uninstalled"
        }
        GovernanceEvent::ConsentAnswered {
            agent_id,
            access_id,
            family,
            answer,
        } => {
            summary.agent = Some(agent_id.clone());
            summary.tool = Some(access_id.clone());
            summary.family = Some(family.as_str().to_owned());
            match answer {
                ConsentAnswer::Once => "Consent given for this call",
                ConsentAnswer::Always => "Consent given for good",
                ConsentAnswer::Refuse => {
                    summary.outcome = Some(JournalOutcome::Refused);
                    "Consent refused"
                }
            }
        }
        GovernanceEvent::AlwaysWithdrawn { agent_id, family } => {
            summary.agent = Some(agent_id.clone());
            summary.family = Some(family.as_str().to_owned());
            "Standing consent withdrawn"
        }
        GovernanceEvent::PolicyRefused {
            agent_id,
            access_id,
            family,
            ..
        } => {
            summary.agent = Some(agent_id.clone());
            summary.tool = Some(access_id.clone());
            summary.family = Some(family.as_str().to_owned());
            summary.outcome = Some(JournalOutcome::Refused);
            "Call refused by the organisation policy"
        }
        GovernanceEvent::RuleRefused {
            agent_id,
            access_id,
            family,
            ..
        } => {
            summary.agent = Some(agent_id.clone());
            summary.tool = Some(access_id.clone());
            summary.family = Some(family.as_str().to_owned());
            summary.outcome = Some(JournalOutcome::Refused);
            "Call refused by a rule"
        }
        GovernanceEvent::RuleAdded(rule) => {
            summary.agent = rule.agent_id.clone();
            summary.tool = Some(rule.tool_name.clone());
            "Rule added"
        }
        GovernanceEvent::RuleRemoved(rule) => {
            summary.agent = rule.agent_id.clone();
            summary.tool = Some(rule.tool_name.clone());
            "Rule removed"
        }
        GovernanceEvent::BatchAccessGranted {
            agent_id,
            batch_id,
            family,
            ..
        } => {
            summary.agent = Some(agent_id.clone());
            summary.family = Some(family.as_str().to_owned());
            summary.target = Some(batch_id.clone());
            "Access granted for one batch"
        }
        GovernanceEvent::PolicyApplied { publisher, .. } => {
            summary.target = Some(publisher.clone());
            "Organisation policy applied"
        }
        GovernanceEvent::PolicyChanged { .. } => "Organisation policy changed",
        GovernanceEvent::PolicyUnreadable { reason, .. } => {
            summary.target = Some(reason.clone());
            summary.outcome = Some(JournalOutcome::Failure);
            "Organisation policy refused"
        }
        GovernanceEvent::FailClosedTriggered { cause, .. } => {
            summary.target = Some(cause.clone());
            summary.outcome = Some(JournalOutcome::Failure);
            "Sensitive calls now refused"
        }
        GovernanceEvent::JournalExported { format, .. } => {
            summary.target = Some(format.clone());
            "Journal exported"
        }
    };
    summary.what = what.to_owned();
}

#[cfg(test)]
mod tests;
