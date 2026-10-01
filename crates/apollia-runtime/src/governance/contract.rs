//! The four calls of the governance contract and the types they exchange.

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
}

/// Decision governance returns for one use of an access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum AccessDecision {
    /// The call proceeds.
    Allowed,
    /// The call waits for the user's answer.
    AskUser,
    /// The call is refused by the deposited policy.
    DeniedByPolicy {
        /// Why the policy refuses, shown to the user as is.
        reason: String,
    },
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
