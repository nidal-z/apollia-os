//! The governance decision on one tool call of the loop, and what the loop
//! makes of it.

use super::*;

use apollia_tools::ActionFamily;

use crate::governance::{
    answered_once, once_grant, AccessCall, AccessDecision, DecisionReason, GovernanceContract,
    ALL_ANSWERS,
};

/// What governance asked about a call, carried to the answer.
pub(super) struct AskedConsent {
    /// Family of the tool.
    pub(super) family: ActionFamily,
    /// Whether "always" was among the answers offered.
    pub(super) always_offered: bool,
}

/// The family of the tool `name`, as its descriptor declares it. The search
/// over the deferred tools reads an in-memory index and acts on nothing else;
/// a tool nothing describes is unclassified.
pub(super) fn family_of(
    name: &str,
    descriptor: Option<&apollia_tools::ToolDescriptor>,
) -> ActionFamily {
    match descriptor {
        Some(descriptor) => descriptor.family,
        None if name == apollia_mcp::tool_search::TOOL_SEARCH_NAME => ActionFamily::Exempt,
        None => ActionFamily::Unclassified,
    }
}

/// Whether `call`, of a tool in `family`, runs code.
pub(super) fn is_code_call(call: &ToolCall, family: ActionFamily) -> bool {
    family == ActionFamily::ExecuteCode || apollia_permissions::is_code_executor(&call.name)
}

/// What the model is told when governance refuses a call.
pub(super) fn refusal_text(reason: &DecisionReason) -> String {
    match reason {
        DecisionReason::Policy { detail } => format!("Tool {detail}"),
        DecisionReason::Rule { .. } => "Tool refused by a permission rule".to_string(),
        DecisionReason::FailClosed => {
            "Tool refused: governance cannot read what it decides from".to_string()
        }
        DecisionReason::Consent | DecisionReason::NoConsentNeeded => {
            "Tool refused by governance".to_string()
        }
    }
}

impl BuiltInChatAgent {
    /// The governance decision on `call`, whose tool is in `family`.
    ///
    /// The run's own grants (`acc.authorized`) count as a consent for the
    /// run, and a grant of one call ([`once_grant`]) lets the next call of
    /// its tool through. A call whose arguments a `PreToolUse` hook replaced
    /// does not inherit a consent: what was consented to is the model's call,
    /// so it is asked about again.
    pub(super) fn decide_call(
        &self,
        call: &ToolCall,
        family: ActionFamily,
        acc: &ReactAccumulators,
        rewritten_by_hook: bool,
    ) -> AccessDecision {
        let argument = apollia_permissions::extract_first_arg(&call.arguments);
        let decision = self.governance.decide(&AccessCall {
            agent_id: &self.agent_id,
            access_id: &call.name,
            argument: argument.as_deref(),
            family,
            unattended: self.unattended,
            granted_for_run: acc.authorized.contains(&call.name),
        });
        match decision {
            AccessDecision::Allowed {
                reason: DecisionReason::Consent,
            } if rewritten_by_hook => {
                let reason = DecisionReason::Consent;
                let answers = ALL_ANSWERS.to_vec();
                if self.unattended {
                    AccessDecision::Pending { reason, answers }
                } else {
                    AccessDecision::AskUser { reason, answers }
                }
            }
            other if !rewritten_by_hook && acc.authorized.contains(&once_grant(&call.name)) => {
                answered_once(other)
            }
            other => other,
        }
    }

    /// Writes the fine rule an "always" on a command stands for, scoped to
    /// this agent and to the whole command. A write governance refuses, or
    /// one that fails, leaves the answer worth this call only.
    pub(super) async fn allow_command_for_good(&self, call: &ToolCall) {
        let Some(command) = apollia_permissions::extract_first_arg(&call.arguments) else {
            tracing::warn!(tool = %call.name, "chat.tool.always_accept.downgraded");
            return;
        };
        match self
            .governance
            .allow_command(&self.agent_id, &call.name, &command)
            .await
        {
            Ok(rule_id) => tracing::info!(
                tool = %call.name,
                rule_id,
                "chat.tool.always_accept.command_rule"
            ),
            Err(error) => tracing::warn!(
                tool = %call.name,
                error = %error,
                "chat.tool.always_accept.not_recorded"
            ),
        }
    }
}
