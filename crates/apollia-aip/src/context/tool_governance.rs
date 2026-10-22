//! The governance decision on a tool called from Python.
//!
//! A tool a package's code calls through `ctx.tools` is decided by the same
//! governance as a call of the Chat, of a plan or of a batch item: the policy,
//! a connection in pause, the fine rules, then the consents of the agent. A
//! workflow runs with nobody watching it, so it follows the rule of a batch
//! item: a call that would ask waits for an answer given later, which pauses
//! the run, and an access granted for the run counts as a consent.

use std::collections::HashSet;
use std::sync::Arc;

use apollia_runtime::governance::{
    AccessCall, AccessDecision, DecisionReason, GovernanceContract, GovernanceEvent,
};
use apollia_tools::ActionFamily;

use crate::context::tool_proxy::ToolProxyError;

/// The governance a [`ToolProxy`](crate::context::ToolProxy) decides its calls
/// by, and the accesses its run was granted.
pub struct ToolGovernance {
    contract: Arc<dyn GovernanceContract>,
    granted_for_run: HashSet<String>,
}

impl ToolGovernance {
    /// Decides through `contract`, the run holding no grant of its own.
    pub fn new(contract: Arc<dyn GovernanceContract>) -> Self {
        Self {
            contract,
            granted_for_run: HashSet::new(),
        }
    }

    // TEST-ONLY: no production run attaches `ctx.tools` yet, so none grants
    // an access for its run; a batch grants through the Chat engine instead.
    /// The accesses the caller of the run granted for the run alone, as a
    /// batch grants the accesses of its agent for its items.
    #[must_use]
    pub fn with_run_grants(mut self, grants: impl IntoIterator<Item = String>) -> Self {
        self.granted_for_run.extend(grants);
        self
    }

    /// Decides the call of `tool` by `agent_id`, a tool in `family`, with
    /// `input` as its arguments. `Ok` when the call may run; a refusal carries
    /// its reason, a call that would ask is an approval the run pauses on.
    pub(crate) fn gate(
        &self,
        agent_id: &str,
        tool: &str,
        family: ActionFamily,
        input: &serde_json::Value,
    ) -> Result<(), ToolProxyError> {
        let argument = apollia_permissions::extract_first_arg(input);
        let decision = self.contract.decide(&AccessCall {
            agent_id,
            access_id: tool,
            argument: argument.as_deref(),
            family,
            unattended: true,
            granted_for_run: self.granted_for_run.contains(tool),
        });
        match decision {
            AccessDecision::Allowed { .. } => Ok(()),
            AccessDecision::Denied { reason } => {
                tracing::event!(
                    tracing::Level::INFO,
                    tool = %tool,
                    agent_id = %agent_id,
                    reason = ?reason,
                    "tool.call.governance_denied"
                );
                self.journal_refusal(agent_id, tool, family, &reason);
                Err(match reason {
                    DecisionReason::FailClosed => ToolProxyError::FailClosed {
                        tool: tool.to_string(),
                        detail: "governance cannot read what it decides from".to_string(),
                    },
                    reason => ToolProxyError::Refused {
                        tool: tool.to_string(),
                        reason: refusal_reason(&reason),
                    },
                })
            }
            AccessDecision::AskUser { .. } | AccessDecision::Pending { .. } => {
                tracing::event!(
                    tracing::Level::INFO,
                    tool = %tool,
                    agent_id = %agent_id,
                    "tool.call.governance_waits"
                );
                Err(ToolProxyError::ApprovalRequired {
                    prompt: format!("Allow {agent_id} to use {tool}?"),
                    payload: serde_json::json!({
                        "genre": "approbation",
                        "geste": tool,
                        "risque": risk_of(family),
                    }),
                })
            }
        }
    }

    /// Records in the journal a call the policy, a rule or a pause refused.
    /// Neither the call's argument nor its result is recorded.
    fn journal_refusal(
        &self,
        agent_id: &str,
        tool: &str,
        family: ActionFamily,
        reason: &DecisionReason,
    ) {
        let event = match reason {
            DecisionReason::Policy { detail } => GovernanceEvent::PolicyRefused {
                agent_id: agent_id.to_string(),
                access_id: tool.to_string(),
                family,
                detail: detail.clone(),
            },
            DecisionReason::Rule { rule_id } => GovernanceEvent::RuleRefused {
                agent_id: agent_id.to_string(),
                access_id: tool.to_string(),
                family,
                rule_id: *rule_id,
            },
            DecisionReason::Paused { connection_id } => GovernanceEvent::PausedRefused {
                agent_id: agent_id.to_string(),
                access_id: tool.to_string(),
                family,
                connection_id: connection_id.clone(),
            },
            _ => return,
        };
        if let Err(error) = self.contract.record(&event) {
            tracing::event!(
                tracing::Level::DEBUG,
                tool = %tool,
                error = %error,
                "tool.call.governance_unjournaled"
            );
        }
    }
}

/// What the code that called a refused tool is told.
fn refusal_reason(reason: &DecisionReason) -> String {
    match reason {
        DecisionReason::Policy { detail } => detail.clone(),
        DecisionReason::Rule { rule_id } => format!("refused by permission rule {rule_id}"),
        DecisionReason::Paused { connection_id } => format!(
            "refused: the connection {connection_id} is paused; \
             the user can resume it from the connections"
        ),
        DecisionReason::FailClosed => {
            "refused: governance cannot read what it decides from".to_string()
        }
        DecisionReason::Consent | DecisionReason::NoConsentNeeded => {
            "refused by governance".to_string()
        }
    }
}

/// The risk an approval of a tool in `family` is shown with.
fn risk_of(family: ActionFamily) -> &'static str {
    match family {
        ActionFamily::ExecuteCode | ActionFamily::Unclassified => "critical",
        ActionFamily::Read | ActionFamily::Exempt => "low",
        _ => "medium",
    }
}

#[cfg(test)]
mod tests;
