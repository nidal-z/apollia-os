//! The persisted rule's own vocabulary: action, scope, and the rule row.
//!
//! Split out of `prefix_rule_engine.rs`: the engine stays in the parent, the
//! types it stores and returns live here.

use serde::{Deserialize, Serialize};

use crate::error::PermissionError;

/// Action of a prefix rule: Allow or Deny.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuleAction {
    /// Auto-approve invocations matching this rule.
    Allow,
    /// Auto-deny invocations matching this rule.
    Deny,
}
impl RuleAction {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            RuleAction::Allow => "allow",
            RuleAction::Deny => "deny",
        }
    }

    pub(crate) fn from_str(s: &str) -> Result<Self, PermissionError> {
        match s {
            "allow" => Ok(RuleAction::Allow),
            "deny" => Ok(RuleAction::Deny),
            other => Err(PermissionError::InvalidRule(format!(
                "unknown action '{other}', expected 'allow' or 'deny'"
            ))),
        }
    }
}
/// Scope of a permission rule: one agent, or every agent.
///
/// A rule never targets a project or a single session: a fine rule binds a
/// tool (and an optional argument prefix) for one agent or for all of them.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionScope {
    /// Rule applying to the agent whose `agent_id` matches.
    Agent,
    /// Rule applying to every agent.
    #[default]
    Global,
}
impl PermissionScope {
    /// Textual representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionScope::Agent => "agent",
            PermissionScope::Global => "global",
        }
    }

    /// Parse the value stored in the database.
    pub(crate) fn from_db_str(s: &str) -> Result<Self, PermissionError> {
        match s {
            "agent" => Ok(PermissionScope::Agent),
            "global" => Ok(PermissionScope::Global),
            other => Err(PermissionError::InvalidRule(format!(
                "unknown scope '{other}', expected 'agent' | 'global'"
            ))),
        }
    }
}
/// Evaluation context for scope-aware rule matching.
///
/// Passed to the `PrefixRuleEngine` to select the `Agent` rules of the caller.
#[derive(Debug, Clone, Default)]
pub struct ScopeContext {
    /// Identifier of the current agent (`None` when outside an agent context).
    pub agent_id: Option<String>,
}
/// A prefix rule persisted in SQLite.
///
/// A rule binds a tool name and an optional argument prefix to an action.
/// `arg_prefix = None` means the rule applies to any argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixRule {
    /// Unique identifier (SQLite AUTOINCREMENT). 0 for a non-persisted rule.
    pub id: i64,
    /// Name of the targeted tool.
    pub tool_name: String,
    /// Argument prefix to match (None = any argument).
    pub arg_prefix: Option<String>,
    /// Action to apply when the rule matches.
    pub action: RuleAction,
    /// Creation timestamp (Unix epoch, seconds).
    pub created_at: i64,
    /// Name of the agent that created the rule (None = human operator).
    pub created_by_agent: Option<String>,
    /// Scope of the rule.
    pub scope: PermissionScope,
    /// Agent identifier (set when `scope == Agent`).
    pub agent_id: Option<String>,
    /// Unix expiration timestamp (None = permanent rule).
    pub expires_at: Option<i64>,
}
impl Default for PrefixRule {
    fn default() -> Self {
        Self {
            id: 0,
            tool_name: String::new(),
            arg_prefix: None,
            action: RuleAction::Allow,
            created_at: 0,
            created_by_agent: None,
            scope: PermissionScope::Global,
            agent_id: None,
            expires_at: None,
        }
    }
}
