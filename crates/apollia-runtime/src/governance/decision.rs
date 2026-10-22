//! The decision on one tool call, read from what governance holds.
//!
//! [`GovernanceState`] is everything a decision reads: the refusals of the
//! organisation policy, the fine rules and the standing consents of
//! `governance.db`, and the developer mode. [`decide`] answers one call from
//! it, in a fixed order, the first line that applies deciding:
//!
//! 1. the policy refuses the tool, its connection or its family: refused;
//!    then the connection that brings the tool is in pause: refused;
//! 2. a fine rule `deny` matches: refused;
//! 3. the family reads only, or acts on the conversation alone: allowed;
//! 4. the family executes code: allowed by a fine rule `allow` on a single
//!    simple command, in developer mode only; asked otherwise, with "always"
//!    (which writes such a rule) only for a single simple command in
//!    developer mode;
//! 5. the call proposes a fact for the profile: asked, without "always",
//!    whatever consent stands; else a standing consent of the agent for the
//!    family, or a grant of the run: allowed;
//! 6. asked, with the three answers.
//!
//! A tool nobody classified skips line 5 and is asked without "always",
//! unless a fine rule `allow` names it in developer mode, which lets it run.
//! An unattended run waits where an attended one would ask. Governance that
//! cannot read what it decides from refuses every sensitive call.
//!
//! A run may also hold a grant for one call only ([`once_grant`]): the answer
//! "once" given to a batch item that stopped on executing code. The next call
//! of that access that would ask goes through ([`answered_once`]), the one
//! after asks again; a refusal of lines 1, 2 or of fail-closed
//! governance stays a refusal.
//!
//! `connections.deny` names connections in the vocabulary of the admission
//! (`native:google`, `selection:<entry>`, `package:npm:<name>`), not in the
//! one of a call (`conn:google:<account>/<operation>`, `mcp:<server>/<tool>`).
//! A call is therefore read in both: the kind of connection Apollia brings
//! itself is named from the call ([`policy_names`]), and an MCP server the
//! policy names by its entry or its package was found at boot and is held in
//! [`PolicyRefusals::forbidden_connections`].

use std::collections::HashSet;

use apollia_permissions::org_policy::PolicyFamily;
use apollia_permissions::{
    is_code_executor, is_single_simple_command, rule_matches, ConsentFamily, PrefixRule, RuleAction,
};
use apollia_tools::ActionFamily;

use crate::governance::contract::{
    AccessCall, AccessDecision, ConsentAnswer, DecisionReason, ALL_ANSWERS, NO_ALWAYS_ANSWERS,
};
use crate::governance::fail_closed::FailClosed;

/// Prefix of a run grant that lets one call of an access through.
const ONCE_PREFIX: &str = "once:";

/// The run grant that lets the next call of `access_id` through, and no
/// later one.
#[must_use]
pub fn once_grant(access_id: &str) -> String {
    format!("{ONCE_PREFIX}{access_id}")
}

/// Whether the run grant `grant` lets one call through only.
#[must_use]
pub fn is_once_grant(grant: &str) -> bool {
    grant.starts_with(ONCE_PREFIX)
}

/// `decision` for a call the user already answered "once": a call that would
/// ask or wait for a consent goes through; any other decision is kept.
#[must_use]
pub fn answered_once(decision: AccessDecision) -> AccessDecision {
    match decision {
        AccessDecision::AskUser {
            reason: DecisionReason::Consent,
            ..
        }
        | AccessDecision::Pending {
            reason: DecisionReason::Consent,
            ..
        } => AccessDecision::Allowed {
            reason: DecisionReason::Consent,
        },
        other => other,
    }
}

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

/// Prefix of the tools of a folder connection.
const FOLDER_TOOL_PREFIX: &str = "conn:folder:";

/// Prefix of the tools shared by every mailbox connected over IMAP.
const IMAP_TOOL_PREFIX: &str = "imap.";

/// The names `connections.deny` may give the connection of `access_id`, in
/// the admission vocabulary: `native:<provider>` for a tool of a connected
/// account, `native:folder` for a tool of a folder, `native:imap` for a
/// mailbox tool. An MCP server is named by its entry or its package, which
/// the call does not carry.
fn policy_names(access_id: &str) -> Vec<String> {
    if access_id.starts_with(IMAP_TOOL_PREFIX) {
        return vec!["native:imap".to_owned()];
    }
    if access_id.starts_with(FOLDER_TOOL_PREFIX) {
        return vec!["native:folder".to_owned()];
    }
    access_id
        .strip_prefix("conn:")
        .and_then(|rest| rest.split_once(':'))
        .map(|(provider, _)| vec![format!("native:{provider}")])
        .unwrap_or_default()
}

/// The connection that brings the tool `access_id`, as the connection list
/// names it: `mcp:<server>`, `native:<provider>:<account>`, `folder:<id>`;
/// `None` for a tool Apollia ships, and for a mailbox tool, shared by every
/// mailbox and refused by its executor.
#[must_use]
pub fn listed_connection_of(access_id: &str) -> Option<String> {
    let connection = connection_of(access_id)?;
    if let Some(folder) = connection.strip_prefix(FOLDER_TOOL_PREFIX) {
        return Some(format!("folder:{folder}"));
    }
    if let Some(account) = connection.strip_prefix("conn:") {
        return Some(format!("native:{account}"));
    }
    Some(connection.to_owned())
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
    /// The tools disabled in `governance.db`, left out of every dispatcher.
    pub disabled_tools: Vec<String>,
    /// The connections in pause in `governance.db`, as the connection list
    /// names them: every tool of one is refused.
    pub paused: HashSet<String>,
    /// Set while governance cannot read what it decides from: every sensitive
    /// call is refused.
    pub fail_closed: Option<FailClosed>,
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
    if let Some(connection_id) =
        listed_connection_of(call.access_id).filter(|id| state.paused.contains(id))
    {
        return AccessDecision::Denied {
            reason: DecisionReason::Paused { connection_id },
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
    if state.fail_closed.is_some() {
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
        if state.developer_mode && simple {
            return ask(call, &ALL_ANSWERS);
        }
        return ask(call, &NO_ALWAYS_ANSWERS);
    }
    let Some(consent_family) = family.consent_family() else {
        // No family, so no consent can stand for the tool: in developer mode,
        // a fine rule naming it is the one thing that lets it run unasked.
        let allow = matching.iter().find(|r| r.action == RuleAction::Allow);
        if let (true, Some(rule)) = (state.developer_mode, allow) {
            return AccessDecision::Allowed {
                reason: DecisionReason::Rule { rule_id: rule.id },
            };
        }
        return ask(call, &NO_ALWAYS_ANSWERS);
    };
    // A fact proposed for the profile is put to the person every time: the
    // profile follows them into every conversation, so no standing consent
    // and no grant of a run covers it.
    if call.access_id == apollia_tools::PROFILE_PROPOSE_TOOL {
        return ask(call, &NO_ALWAYS_ANSWERS);
    }
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
    if policy_names(access_id)
        .iter()
        .any(|name| policy.denied.contains(name))
    {
        return Some(
            "refused by the organisation policy: this connection is not allowed".to_owned(),
        );
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
