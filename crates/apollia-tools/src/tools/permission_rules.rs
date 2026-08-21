//! Native `permission_rule_{add,remove,list}` tools: they expose the read/write
//! API of `governance.db` to agents.
//!
//! The three are ordinary native tools (`tool_registry::NATIVE_TOOL_NAMES`), so
//! an agent invocation of any of them goes through the same gate as every other
//! tool call: the chat session's name-only authorization set, then the
//! per-invocation prefix-rule check, then human approval on a miss. Nothing in
//! this module adds a gate of its own.
//!
//! Each tool opens a fresh connection to the `governance.db` file at invocation
//! time: SQLite WAL handles concurrency with the connections held elsewhere.
//! Writes are immediately visible to later reads.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use apollia_permissions::{
    PermissionError, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::executor::{ToolExecutionError, ToolExecutor};
use apollia_core::SandboxProfile;

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Domain errors specific to the `permission_rule_*` tools.
#[derive(Debug, Error)]
pub enum PermissionRuleToolError {
    /// The provided action is neither `"allow"` nor `"deny"`.
    #[error("invalid action '{action}': expected 'allow' or 'deny'")]
    InvalidAction {
        /// Received value.
        action: String,
    },

    /// The provided scope is not recognized.
    #[error(
        "invalid scope '{scope}': expected 'project' | 'agent' | 'global' (session non persisté)"
    )]
    InvalidScope {
        /// Received value.
        scope: String,
    },

    /// The `project` scope requires `project_path`.
    #[error("scope 'project' requires 'project_path'")]
    MissingProjectPath,

    /// The `agent` scope requires `agent_id`.
    #[error("scope 'agent' requires 'agent_id'")]
    MissingAgentId,

    /// SQLite error or validation on the `PrefixRuleEngine` side.
    #[error("permission engine error: {0}")]
    Engine(#[from] PermissionError),
}

impl From<PermissionRuleToolError> for ToolExecutionError {
    fn from(err: PermissionRuleToolError) -> Self {
        let code = match &err {
            PermissionRuleToolError::InvalidAction { .. } => "invalid_action",
            PermissionRuleToolError::InvalidScope { .. } => "invalid_scope",
            PermissionRuleToolError::MissingProjectPath => "missing_project_path",
            PermissionRuleToolError::MissingAgentId => "missing_agent_id",
            PermissionRuleToolError::Engine(_) => "engine_error",
        };
        ToolExecutionError::ExecutionFailed {
            code: code.to_string(),
            message: err.to_string(),
        }
    }
}

// ─────────────────────────────────────────────
// DTO
// ─────────────────────────────────────────────

/// JSON representation of a `PrefixRule` returned to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleDto {
    /// SQLite identifier.
    pub id: i64,
    /// Targeted tool.
    pub tool_name: String,
    /// Argument prefix (None = any argument).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_prefix: Option<String>,
    /// `"allow"` or `"deny"`.
    pub action: String,
    /// `"global"` | `"project"` | `"agent"`.
    pub scope: String,
    /// Canonical project path for the `project` scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// Agent identifier for the `agent` scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Author of the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Unix creation timestamp.
    pub created_at: i64,
    /// Optional Unix expiration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl From<PrefixRule> for PermissionRuleDto {
    fn from(rule: PrefixRule) -> Self {
        Self {
            id: rule.id,
            tool_name: rule.tool_name,
            arg_prefix: rule.arg_prefix,
            action: match rule.action {
                RuleAction::Allow => "allow".to_string(),
                RuleAction::Deny => "deny".to_string(),
            },
            scope: rule.scope.as_str().to_string(),
            project_path: rule.project_path.map(|p| p.to_string_lossy().to_string()),
            agent_id: rule.agent_id,
            created_by: rule.created_by_agent,
            created_at: rule.created_at,
            expires_at: rule.expires_at,
        }
    }
}

fn parse_action(s: &str) -> Result<RuleAction, PermissionRuleToolError> {
    match s {
        "allow" => Ok(RuleAction::Allow),
        "deny" => Ok(RuleAction::Deny),
        other => Err(PermissionRuleToolError::InvalidAction {
            action: other.to_string(),
        }),
    }
}

fn parse_scope(s: &str) -> Result<PermissionScope, PermissionRuleToolError> {
    match s {
        "global" => Ok(PermissionScope::Global),
        "project" => Ok(PermissionScope::Project),
        "agent" => Ok(PermissionScope::Agent),
        other => Err(PermissionRuleToolError::InvalidScope {
            scope: other.to_string(),
        }),
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ─────────────────────────────────────────────
// permission_rule_add
// ─────────────────────────────────────────────

/// Input of the `permission_rule_add` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleAddInput {
    /// Name of the targeted tool.
    pub tool_name: String,
    /// `"allow"` or `"deny"`.
    pub action: String,
    /// Optional argument prefix (None = any argument).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg_prefix: Option<String>,
    /// `"global"` (default), `"project"` or `"agent"`. Session not supported
    /// (always in RAM via the desktop "session" HITL button).
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Required when `scope = "project"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// Required when `scope = "agent"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Unix expiration timestamp (None = permanent rule).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

fn default_scope() -> String {
    "global".to_string()
}

/// Output of the `permission_rule_add` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleAddOutput {
    /// SQLite identifier of the created rule.
    pub rule_id: i64,
}

/// Native `permission_rule_add` tool: proposes adding a permission rule
/// persisted in `governance.db`.
///
/// The invocation is gated like any other tool call, by the chat approval flow.
/// The `created_by` field is automatically populated with the name of the
/// calling agent.
#[derive(Debug, Clone)]
pub struct PermissionRuleAdd {
    db_path: PathBuf,
    agent_id: String,
}

impl PermissionRuleAdd {
    /// Builds the tool, remembering the `governance.db` path and the `agent_id`
    /// of the agent that owns the dispatcher.
    pub fn new(db_path: PathBuf, agent_id: String) -> Self {
        Self { db_path, agent_id }
    }

    /// Executes the typed add.
    pub fn run(
        &self,
        input: PermissionRuleAddInput,
    ) -> Result<PermissionRuleAddOutput, PermissionRuleToolError> {
        let action = parse_action(&input.action)?;
        let scope = parse_scope(&input.scope)?;

        let project_path = match (scope, &input.project_path) {
            (PermissionScope::Project, Some(p)) => Some(PathBuf::from(p)),
            (PermissionScope::Project, None) => {
                return Err(PermissionRuleToolError::MissingProjectPath);
            }
            _ => None,
        };

        let agent_id = match (scope, &input.agent_id) {
            (PermissionScope::Agent, Some(a)) if !a.trim().is_empty() => Some(a.clone()),
            (PermissionScope::Agent, _) => return Err(PermissionRuleToolError::MissingAgentId),
            _ => None,
        };

        let rule = PrefixRule {
            tool_name: input.tool_name,
            arg_prefix: input.arg_prefix,
            action,
            created_at: now_unix_secs(),
            created_by_agent: Some(self.agent_id.clone()),
            scope,
            project_path,
            agent_id,
            expires_at: input.expires_at,
            ..PrefixRule::default()
        };

        let mut engine = PrefixRuleEngine::new(&self.db_path)?;
        let id = engine.add_rule(&rule)?;
        tracing::info!(
            rule_id = id,
            tool = %rule.tool_name,
            agent = %self.agent_id,
            "permission_rule_add"
        );
        Ok(PermissionRuleAddOutput { rule_id: id })
    }

    /// Descriptor pour enregistrement dans le `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "permission_rule_add".to_string(),
            version: "1.0.0".to_string(),
            description: "Persist a new permission rule in governance.db. The rule is tagged \
                with the calling agent's identity in `created_by`. Subject to HITL approval."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tool_name": {"type": "string", "description": "Target tool name."},
                    "action": {"type": "string", "enum": ["allow", "deny"]},
                    "arg_prefix": {"type": "string", "description": "Optional first-arg prefix."},
                    "scope": {
                        "type": "string",
                        "enum": ["global", "project", "agent"],
                        "default": "global"
                    },
                    "project_path": {"type": "string", "description": "Required when scope='project'."},
                    "agent_id": {"type": "string", "description": "Required when scope='agent'."},
                    "expires_at": {"type": "integer", "description": "Unix epoch seconds."}
                },
                "required": ["tool_name", "action"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {"rule_id": {"type": "integer"}},
                "required": ["rule_id"]
            })),
            sandbox_profile: SandboxProfile::ReadOnly,
            tags: vec!["governance".to_string(), "permissions".to_string()],
            dangerous: false,
            is_read_only: false,
            risk_score: 60,
            approval_risk_level: None,
            impact_description: Some(
                "Modifies persistent permission policy in governance.db.".to_string(),
            ),
            reject_reason_required: false,
        }
    }
}

impl ToolExecutor for PermissionRuleAdd {
    fn name(&self) -> &str {
        "permission_rule_add"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: PermissionRuleAddInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;
            let out = self.run(typed)?;
            serde_json::to_value(out).map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "serialization_error".to_string(),
                message: e.to_string(),
            })
        })
    }
}

// ─────────────────────────────────────────────
// permission_rule_remove
// ─────────────────────────────────────────────

/// Input of the `permission_rule_remove` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleRemoveInput {
    /// Identifier of the rule to remove.
    pub rule_id: i64,
}

/// Output of the `permission_rule_remove` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleRemoveOutput {
    /// `true` if a row was removed, `false` if the identifier did not exist.
    pub removed: bool,
}

/// Native `permission_rule_remove` tool: removes a rule from `governance.db` by
/// identifier. HITL-gated.
#[derive(Debug, Clone)]
pub struct PermissionRuleRemove {
    db_path: PathBuf,
}

impl PermissionRuleRemove {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub fn run(
        &self,
        input: PermissionRuleRemoveInput,
    ) -> Result<PermissionRuleRemoveOutput, PermissionRuleToolError> {
        let mut engine = PrefixRuleEngine::new(&self.db_path)?;
        let removed = engine.remove_rule_checked(input.rule_id)?;
        tracing::info!(rule_id = input.rule_id, removed, "permission_rule_remove");
        Ok(PermissionRuleRemoveOutput { removed })
    }

    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "permission_rule_remove".to_string(),
            version: "1.0.0".to_string(),
            description: "Remove a permission rule from governance.db by id. Subject to HITL \
                approval."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {"rule_id": {"type": "integer"}},
                "required": ["rule_id"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {"removed": {"type": "boolean"}},
                "required": ["removed"]
            })),
            sandbox_profile: SandboxProfile::ReadOnly,
            tags: vec!["governance".to_string(), "permissions".to_string()],
            dangerous: false,
            is_read_only: false,
            risk_score: 70,
            approval_risk_level: None,
            impact_description: Some(
                "Deletes a persistent permission rule from governance.db.".to_string(),
            ),
            reject_reason_required: false,
        }
    }
}

impl ToolExecutor for PermissionRuleRemove {
    fn name(&self) -> &str {
        "permission_rule_remove"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: PermissionRuleRemoveInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;
            let out = self.run(typed)?;
            serde_json::to_value(out).map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "serialization_error".to_string(),
                message: e.to_string(),
            })
        })
    }
}

// ─────────────────────────────────────────────
// permission_rule_list
// ─────────────────────────────────────────────

/// Input of the `permission_rule_list` tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionRuleListInput {
    /// Filter by exact tool name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Filter by author (`created_by`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Filter by scope (`global` | `project` | `agent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Output of the `permission_rule_list` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleListOutput {
    /// Rules matching the filters, sorted by ascending identifier.
    pub rules: Vec<PermissionRuleDto>,
}

/// Native `permission_rule_list` tool, read-only.
#[derive(Debug, Clone)]
pub struct PermissionRuleList {
    db_path: PathBuf,
}

impl PermissionRuleList {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub fn run(
        &self,
        input: PermissionRuleListInput,
    ) -> Result<PermissionRuleListOutput, PermissionRuleToolError> {
        let engine = PrefixRuleEngine::new(&self.db_path)?;

        // Strategy: if created_by is provided, start with that filter (the most
        // selective), then refine in memory. Otherwise list everything (except
        // session) and filter.
        let candidates = if let Some(creator) = input.created_by.as_deref() {
            engine.list_rules_by_creator(creator)?
        } else {
            engine.list_rules()?
        };

        let scope_filter = match input.scope.as_deref() {
            Some(s) => Some(parse_scope(s)?),
            None => None,
        };

        let rules: Vec<PermissionRuleDto> = candidates
            .into_iter()
            .filter(|r| {
                input
                    .tool_name
                    .as_deref()
                    .map(|t| r.tool_name == t)
                    .unwrap_or(true)
                    && scope_filter.map(|s| r.scope == s).unwrap_or(true)
            })
            .map(PermissionRuleDto::from)
            .collect();

        Ok(PermissionRuleListOutput { rules })
    }

    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "permission_rule_list".to_string(),
            version: "1.0.0".to_string(),
            description: "List permission rules from governance.db, optionally filtered by \
                tool_name, created_by or scope. Read-only."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tool_name": {"type": "string"},
                    "created_by": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "project", "agent"]}
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "rules": {"type": "array"}
                },
                "required": ["rules"]
            })),
            sandbox_profile: SandboxProfile::ReadOnly,
            tags: vec!["governance".to_string(), "permissions".to_string()],
            dangerous: false,
            is_read_only: true,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

impl ToolExecutor for PermissionRuleList {
    fn name(&self) -> &str {
        "permission_rule_list"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            // Empty input accepted (filters are optional).
            let typed: PermissionRuleListInput = if input.is_null() {
                PermissionRuleListInput::default()
            } else {
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?
            };
            let out = self.run(typed)?;
            serde_json::to_value(out).map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "serialization_error".to_string(),
                message: e.to_string(),
            })
        })
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn tmp_db() -> NamedTempFile {
        NamedTempFile::new().expect("tempfile")
    }

    #[test]
    fn add_persists_rule_with_creator() {
        // GIVEN a PermissionRuleAdd configured for the "onboarding-agent" agent
        let db = tmp_db();
        let tool = PermissionRuleAdd::new(db.path().to_path_buf(), "onboarding-agent".into());

        // WHEN run() is called with a deny http_fetch https:// rule
        let out = tool
            .run(PermissionRuleAddInput {
                tool_name: "http_fetch".into(),
                action: "deny".into(),
                arg_prefix: Some("https://".into()),
                scope: "global".into(),
                project_path: None,
                agent_id: None,
                expires_at: None,
            })
            .expect("run");

        // THEN the rule exists in the DB with created_by="onboarding-agent"
        let engine = PrefixRuleEngine::new(db.path()).expect("engine");
        let rules = engine
            .list_rules_by_creator("onboarding-agent")
            .expect("list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, out.rule_id);
        assert_eq!(rules[0].tool_name, "http_fetch");
        assert_eq!(rules[0].action, RuleAction::Deny);
        assert_eq!(rules[0].arg_prefix.as_deref(), Some("https://"));
    }

    #[test]
    fn add_invalid_action_fails() {
        let db = tmp_db();
        let tool = PermissionRuleAdd::new(db.path().to_path_buf(), "agent".into());
        let err = tool
            .run(PermissionRuleAddInput {
                tool_name: "x".into(),
                action: "approve".into(),
                arg_prefix: None,
                scope: "global".into(),
                project_path: None,
                agent_id: None,
                expires_at: None,
            })
            .expect_err("must reject");
        assert!(matches!(err, PermissionRuleToolError::InvalidAction { .. }));
    }

    #[test]
    fn add_project_scope_requires_path() {
        let db = tmp_db();
        let tool = PermissionRuleAdd::new(db.path().to_path_buf(), "agent".into());
        let err = tool
            .run(PermissionRuleAddInput {
                tool_name: "x".into(),
                action: "allow".into(),
                arg_prefix: None,
                scope: "project".into(),
                project_path: None,
                agent_id: None,
                expires_at: None,
            })
            .expect_err("must reject");
        assert!(matches!(err, PermissionRuleToolError::MissingProjectPath));
    }

    #[test]
    fn remove_deletes_rule() {
        // GIVEN an existing rule
        let db = tmp_db();
        let add = PermissionRuleAdd::new(db.path().to_path_buf(), "agent".into());
        let added = add
            .run(PermissionRuleAddInput {
                tool_name: "x".into(),
                action: "allow".into(),
                arg_prefix: None,
                scope: "global".into(),
                project_path: None,
                agent_id: None,
                expires_at: None,
            })
            .expect("add");

        // WHEN remove
        let remove = PermissionRuleRemove::new(db.path().to_path_buf());
        let out = remove
            .run(PermissionRuleRemoveInput {
                rule_id: added.rule_id,
            })
            .expect("remove");
        assert!(out.removed);

        // AND second remove → false
        let out2 = remove
            .run(PermissionRuleRemoveInput {
                rule_id: added.rule_id,
            })
            .expect("remove twice");
        assert!(!out2.removed);
    }

    #[test]
    fn list_filters_by_creator() {
        // GIVEN two rules from different authors
        let db = tmp_db();
        PermissionRuleAdd::new(db.path().to_path_buf(), "onboarding-agent".into())
            .run(PermissionRuleAddInput {
                tool_name: "tool_a".into(),
                action: "allow".into(),
                arg_prefix: None,
                scope: "global".into(),
                project_path: None,
                agent_id: None,
                expires_at: None,
            })
            .expect("add 1");
        PermissionRuleAdd::new(db.path().to_path_buf(), "user-hitl".into())
            .run(PermissionRuleAddInput {
                tool_name: "tool_b".into(),
                action: "deny".into(),
                arg_prefix: None,
                scope: "global".into(),
                project_path: None,
                agent_id: None,
                expires_at: None,
            })
            .expect("add 2");

        // WHEN list filtered by creator
        let list = PermissionRuleList::new(db.path().to_path_buf());
        let out = list
            .run(PermissionRuleListInput {
                created_by: Some("onboarding-agent".into()),
                ..Default::default()
            })
            .expect("list");

        // THEN a single rule, the one from onboarding-agent
        assert_eq!(out.rules.len(), 1);
        assert_eq!(out.rules[0].tool_name, "tool_a");
        assert_eq!(out.rules[0].created_by.as_deref(), Some("onboarding-agent"));
    }

    #[test]
    fn list_no_filter_returns_all_persisted() {
        let db = tmp_db();
        PermissionRuleAdd::new(db.path().to_path_buf(), "a".into())
            .run(PermissionRuleAddInput {
                tool_name: "tool_a".into(),
                action: "allow".into(),
                arg_prefix: None,
                scope: "global".into(),
                project_path: None,
                agent_id: None,
                expires_at: None,
            })
            .expect("add");
        PermissionRuleAdd::new(db.path().to_path_buf(), "b".into())
            .run(PermissionRuleAddInput {
                tool_name: "tool_b".into(),
                action: "deny".into(),
                arg_prefix: None,
                scope: "global".into(),
                project_path: None,
                agent_id: None,
                expires_at: None,
            })
            .expect("add");
        let list = PermissionRuleList::new(db.path().to_path_buf());
        let out = list.run(PermissionRuleListInput::default()).expect("list");
        assert_eq!(out.rules.len(), 2);
    }

    #[test]
    fn descriptors_are_valid() {
        assert!(PermissionRuleAdd::descriptor().validate().is_ok());
        assert!(PermissionRuleRemove::descriptor().validate().is_ok());
        assert!(PermissionRuleList::descriptor().validate().is_ok());
    }
}
