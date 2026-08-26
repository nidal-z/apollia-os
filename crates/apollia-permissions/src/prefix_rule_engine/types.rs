//! The persisted rule's own vocabulary: action, scope, and the rule row.
//!
//! Split out of `prefix_rule_engine.rs`: the engine stays in the parent, the
//! types it stores and returns live here.

use std::path::PathBuf;

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
/// Scope of a permission rule.
///
/// Determines where the rule is stored and how it is filtered during evaluation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionScope {
    /// Rule living only in memory, for the duration of one chat session.
    ///
    /// Disappears when the process stops. Never persisted to SQLite.
    Session,
    /// Rule persisted and filtered by the canonical path of the current project.
    ///
    /// Applies only to invocations issued from the matching project.
    Project,
    /// Rule persisted and filtered by the identity of the current agent.
    ///
    /// Applies to any invocation issued by the agent whose `agent_id` matches.
    /// Independent of the project: an agent can run outside a project.
    Agent,
    /// Rule persisted and applying to any project.
    #[default]
    Global,
}
impl PermissionScope {
    /// Textual representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionScope::Session => "session",
            PermissionScope::Project => "project",
            PermissionScope::Agent => "agent",
            PermissionScope::Global => "global",
        }
    }

    /// Parse the value stored in the database, defaulting to `Global` for null columns.
    pub(crate) fn from_db_str(s: &str) -> Result<Self, PermissionError> {
        match s {
            "session" => Ok(PermissionScope::Session),
            "project" => Ok(PermissionScope::Project),
            "agent" => Ok(PermissionScope::Agent),
            "global" => Ok(PermissionScope::Global),
            other => Err(PermissionError::InvalidRule(format!(
                "unknown scope '{other}', expected 'session' | 'project' | 'agent' | 'global'"
            ))),
        }
    }
}
/// Evaluation context for scope-aware rule matching.
///
/// Passed to the `PrefixRuleEngine` to filter `Project`/`Agent` rules.
#[derive(Debug, Clone, Default)]
pub struct ScopeContext {
    /// Scope of the current invocation (informational, not used for filtering).
    pub scope: PermissionScope,
    /// Canonical path of the current project (`None` when outside a project).
    pub project_path: Option<PathBuf>,
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
    /// Canonical project path (set when `scope == Project`).
    pub project_path: Option<PathBuf>,
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
            project_path: None,
            agent_id: None,
            expires_at: None,
        }
    }
}
