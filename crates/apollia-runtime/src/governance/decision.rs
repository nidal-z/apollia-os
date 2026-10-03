//! The decision on one tool call, read from what governance holds.
//!
//! [`GovernanceState`] is everything a decision reads: the refusals of the
//! organisation policy, the fine rules and the standing consents of
//! `governance.db`, and the developer mode. [`decide`] answers one call from
//! it, in a fixed order, the first line that applies deciding:
//!
//! 1. the policy refuses the tool, its connection or its family: refused;
//! 2. a fine rule `deny` matches: refused;
//! 3. the family reads only, or acts on the conversation alone: allowed;
//! 4. the family executes code: allowed by a fine rule `allow` on a single
//!    simple command, in developer mode only; asked otherwise, without
//!    "always";
//! 5. a standing consent of the agent for the family, or a grant of the run:
//!    allowed;
//! 6. asked, with the three answers.
//!
//! A tool nobody classified skips line 5 and is asked without "always". An
//! unattended run waits where an attended one would ask. Governance that
//! cannot read what it decides from refuses every sensitive call.

use std::collections::HashSet;

use apollia_permissions::org_policy::PolicyFamily;
use apollia_permissions::{
    is_code_executor, is_single_simple_command, rule_matches, ConsentFamily, PrefixRule, RuleAction,
};
use apollia_tools::ActionFamily;

use crate::governance::contract::{
    AccessCall, AccessDecision, ConsentAnswer, DecisionReason, ALL_ANSWERS, NO_ALWAYS_ANSWERS,
};

/// What the organisation policy refuses at the moment of a call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyRefusals {
    /// Families refused whatever the agent or the consent.
    pub families: HashSet<PolicyFamily>,
    /// Tools refused by identifier.
    pub denied: HashSet<String>,
    /// Connections the policy forbade at boot, as tool identifiers name them
    /// (`mcp:<server>`, `conn:<provider>:<account>`): every tool of one is
    /// refused.
    pub forbidden_connections: HashSet<String>,
}

/// Everything a decision reads.
#[derive(Debug, Clone, Default)]
pub struct GovernanceState {
    /// What the organisation policy refuses.
    pub policy: PolicyRefusals,
    /// Whether an installation requires a signed package.
    pub require_signature: bool,
    /// The fine rules of `governance.db`.
    pub rules: Vec<PrefixRule>,
    /// The permanent standing consents, as (agent, family) pairs.
    pub consents: HashSet<(String, ConsentFamily)>,
    /// The developer mode in force (`[desktop] developer_mode`).
    pub developer_mode: bool,
    /// Governance could not read what it decides from: every sensitive call
    /// is refused.
    pub fail_closed: bool,
}

/// Decides `call` from `state` at Unix time `now`.
#[must_use]
pub fn decide(state: &GovernanceState, call: &AccessCall<'_>, now: i64) -> AccessDecision {
    let code = call.family == ActionFamily::ExecuteCode || is_code_executor(call.access_id);
    let family = if code {
        ActionFamily::ExecuteCode
    } else {
        call.family
    };
    if let Some(detail) = policy_refusal(&state.policy, call.access_id, family) {
        return AccessDecision::Denied {
            reason: DecisionReason::Policy { detail },
        };
    }
    let matching: Vec<&PrefixRule> = state
        .rules
        .iter()
        .filter(|r| rule_matches(r, call.access_id, call.argument, call.agent_id, now))
        .collect();
    if let Some(rule) = matching.iter().find(|r| r.action == RuleAction::Deny) {
        return AccessDecision::Denied {
            reason: DecisionReason::Rule { rule_id: rule.id },
        };
    }
    if matches!(family, ActionFamily::Read | ActionFamily::Exempt) {
        return AccessDecision::Allowed {
            reason: DecisionReason::NoConsentNeeded,
        };
    }
    if state.fail_closed {
        return AccessDecision::Denied {
            reason: DecisionReason::FailClosed,
        };
    }
    if code {
        let simple = call.argument.is_some_and(is_single_simple_command);
        let allow = matching
            .iter()
            .find(|r| r.action == RuleAction::Allow && r.arg_prefix.is_some());
        if let (true, true, Some(rule)) = (state.developer_mode, simple, allow) {
            return AccessDecision::Allowed {
                reason: DecisionReason::Rule { rule_id: rule.id },
            };
        }
        return ask(call, &NO_ALWAYS_ANSWERS);
    }
    let Some(consent_family) = family.consent_family() else {
        return ask(call, &NO_ALWAYS_ANSWERS);
    };
    let standing = state
        .consents
        .contains(&(call.agent_id.to_owned(), consent_family));
    if standing || call.granted_for_run {
        return AccessDecision::Allowed {
            reason: DecisionReason::Consent,
        };
    }
    ask(call, &ALL_ANSWERS)
}

/// The decision of a call that needs the user's answer.
fn ask(call: &AccessCall<'_>, answers: &[ConsentAnswer]) -> AccessDecision {
    let reason = DecisionReason::Consent;
    let answers = answers.to_vec();
    if call.unattended {
        AccessDecision::Pending { reason, answers }
    } else {
        AccessDecision::AskUser { reason, answers }
    }
}

/// The connection that brings the tool `access_id`: `mcp:<server>` for a
/// tool of an MCP server, `conn:<provider>:<account>` for a tool of a
/// connected account, `None` for a tool Apollia ships.
fn connection_of(access_id: &str) -> Option<&str> {
    if !(access_id.starts_with("mcp:") || access_id.starts_with("conn:")) {
        return None;
    }
    Some(access_id.split_once('/').map_or(access_id, |(c, _)| c))
}

/// The family of the policy vocabulary, `None` for a tool outside the five
/// families.
fn policy_family(family: ActionFamily) -> Option<PolicyFamily> {
    match family {
        ActionFamily::Read => Some(PolicyFamily::Read),
        ActionFamily::ReadPersonal => Some(PolicyFamily::ReadPersonal),
        ActionFamily::WriteOrDelete => Some(PolicyFamily::WriteOrDelete),
        ActionFamily::SendExternal => Some(PolicyFamily::SendExternal),
        ActionFamily::ExecuteCode => Some(PolicyFamily::ExecuteCode),
        ActionFamily::Unclassified | ActionFamily::Exempt => None,
    }
}

/// Why the policy refuses a call of `access_id` in `family`, `None` when it
/// does not.
fn policy_refusal(
    policy: &PolicyRefusals,
    access_id: &str,
    family: ActionFamily,
) -> Option<String> {
    if policy_family(family).is_some_and(|f| policy.families.contains(&f)) {
        return Some(
            "refused by the organisation policy: this kind of action is not allowed".to_owned(),
        );
    }
    if policy.denied.contains(access_id) {
        return Some("refused by the organisation policy: this tool is not allowed".to_owned());
    }
    let connection = connection_of(access_id)?;
    if policy.denied.contains(connection) || policy.forbidden_connections.contains(connection) {
        return Some(
            "refused by the organisation policy: this connection is not allowed".to_owned(),
        );
    }
    None
}

#[cfg(test)]
mod tests;
