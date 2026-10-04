//! The four calls of the governance contract and the types they exchange.

use apollia_tools::ActionFamily;
use serde::{Deserialize, Serialize};

use crate::governance::record::GovernanceEvent;

/// An access an agent declares, before governance has resolved it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredAccess {
    /// Stable identifier of the access (a tool name, `mcp:<server>`,
    /// `agent:<id>`, `workflow:<id>`).
    pub access_id: String,
    /// Plain-language statement of what the access lets the agent do.
    pub permits: String,
    /// Whether the tool behind the access declares itself sensitive.
    pub declared_sensitive: bool,
}

/// An access an agent effectively holds, as resolved by governance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveAccess {
    /// Stable identifier of the access.
    pub access_id: String,
    /// Plain-language statement of what the access lets the agent do.
    pub permits: String,
    /// Whether using the access is sensitive.
    pub sensitive: bool,
}

/// One use of an access, submitted for a decision at the moment of the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessCall<'a> {
    /// Agent issuing the call.
    pub agent_id: &'a str,
    /// Access being used.
    pub access_id: &'a str,
    /// Argument of the call when one is meaningful for a rule (a command, a
    /// path), `None` otherwise.
    pub argument: Option<&'a str>,
    /// Action family of the tool behind the access, as its descriptor
    /// declares it; [`ActionFamily::Unclassified`] when nothing describes it.
    pub family: ActionFamily,
    /// The run is not watched by anyone (an item of a batch): a call that
    /// would ask waits instead.
    pub unattended: bool,
    /// The caller of the run granted this access for the run alone, as a batch
    /// grants the accesses of its agent for its items.
    pub granted_for_run: bool,
}

/// Why governance decided what it decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionReason {
    /// The deposited policy refuses the tool, its connection or its family.
    Policy {
        /// Why the policy refuses, shown to the user as is.
        detail: String,
    },
    /// A fine rule decides.
    Rule {
        /// Identifier of the rule in `governance.db`.
        rule_id: i64,
    },
    /// Governance cannot read what it decides from and refuses what is
    /// sensitive.
    FailClosed,
    /// The user's consent decides: one given for good or for the run, or the
    /// one still to ask for.
    Consent,
    /// The family needs no consent: reading, or acting on the conversation
    /// alone.
    NoConsentNeeded,
}

/// An answer the user may give when a call asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentAnswer {
    /// This call only.
    Once,
    /// This call and every later one of the same agent in the same family.
    Always,
    /// Not this call.
    Refuse,
}

/// The answers of a sensitive call that can be consented to for good.
pub const ALL_ANSWERS: [ConsentAnswer; 3] = [
    ConsentAnswer::Once,
    ConsentAnswer::Always,
    ConsentAnswer::Refuse,
];

/// The answers of a call no standing consent can cover: executing code, or a
/// tool nobody classified.
pub const NO_ALWAYS_ANSWERS: [ConsentAnswer; 2] = [ConsentAnswer::Once, ConsentAnswer::Refuse];

/// Decision governance returns for one use of an access, with its reason and,
/// when the user is to answer, the answers offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum AccessDecision {
    /// The call proceeds.
    Allowed {
        /// Why it proceeds.
        reason: DecisionReason,
    },
    /// The call waits for the user's answer.
    AskUser {
        /// Why the user is asked.
        reason: DecisionReason,
        /// The answers offered.
        answers: Vec<ConsentAnswer>,
    },
    /// The run is unattended: the call waits for an answer given later.
    Pending {
        /// Why the call waits.
        reason: DecisionReason,
        /// The answers that will be offered.
        answers: Vec<ConsentAnswer>,
    },
    /// The call is refused.
    Denied {
        /// Why it is refused.
        reason: DecisionReason,
    },
}

impl AccessDecision {
    /// Whether the call proceeds without asking.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    /// The answers offered to the user, empty when nobody is asked.
    #[must_use]
    pub fn answers(&self) -> &[ConsentAnswer] {
        match self {
            Self::AskUser { answers, .. } | Self::Pending { answers, .. } => answers,
            Self::Allowed { .. } | Self::Denied { .. } => &[],
        }
    }
}

/// A package about to be installed, submitted for the signature question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallSubject<'a> {
    /// Identifier of the package.
    pub package_id: &'a str,
    /// Publisher named by the package, when it names one.
    pub publisher: Option<&'a str>,
}

/// Errors raised by a governance call.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GovernanceError {
    /// The record could not be turned into a journal payload.
    #[error("failed to serialize governance record: {0}")]
    Serialize(#[source] serde_json::Error),
    /// No audit journal is open: a record is never dropped silently.
    #[error("no audit journal is open, the record cannot be written")]
    JournalUnavailable,
}

/// What the agent side expects from governance.
///
/// Implementations are shared across the runtime behind an `Arc<dyn
/// GovernanceContract>`, hence the `Send + Sync` bound; every call is
/// synchronous and cheap, so it can sit on a tool-dispatch path.
pub trait GovernanceContract: Send + Sync {
    /// Resolves the accesses `agent_id` effectively holds from the accesses it
    /// declares, each with what it permits and whether it is sensitive.
    fn effective_accesses(
        &self,
        agent_id: &str,
        declared: &[DeclaredAccess],
    ) -> Vec<EffectiveAccess>;

    /// Decides one use of an access at the moment of the call.
    fn decide(&self, call: &AccessCall<'_>) -> AccessDecision;

    /// Whether installing `subject` requires a signed package.
    fn signature_required(&self, subject: &InstallSubject<'_>) -> bool;

    /// Records a change of access, an installation, an update, or an
    /// uninstallation in the audit journal.
    fn record(&self, event: &GovernanceEvent) -> Result<(), GovernanceError>;
}
