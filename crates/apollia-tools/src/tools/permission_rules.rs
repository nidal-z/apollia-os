//! Outils natifs `permission_rule_{add,remove,list}` — exposent l'API d'écriture
//! et de lecture de `governance.db` aux agents (ADR-086).
//!
//! Les écritures (`add` / `remove`) sont systématiquement HITL-gated par le
//! `PermissionEngine` (cf. ADR-082). La lecture (`list`) est read-only.
//!
//! Chaque tool ouvre une connexion fraîche au fichier `governance.db` à
//! l'invocation : SQLite WAL gère la concurrence avec la connexion détenue par
//! le `PermissionEngine` du dispatcher. Les écritures sont immédiatement
//! visibles pour les `decide()` ultérieurs.

use std::path::PathBuf;

use apollia_permissions::{
    PermissionError, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::executor::{ToolExecutionError, ToolExecutor};
use apollia_core::SandboxProfile;

// ─────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────

/// Erreurs domaine propres aux outils `permission_rule_*`.
#[derive(Debug, Error)]
pub enum PermissionRuleToolError {
    /// L'action fournie n'est ni `"allow"` ni `"deny"`.
    #[error("invalid action '{action}': expected 'allow' or 'deny'")]
    InvalidAction {
        /// Valeur reçue.
        action: String,
    },

    /// Le scope fourni n'est pas reconnu.
    #[error("invalid scope '{scope}': expected 'project' | 'agent' | 'global' (session non persisté)")]
    InvalidScope {
        /// Valeur reçue.
        scope: String,
    },

    /// Le scope `project` exige `project_path`.
    #[error("scope 'project' requires 'project_path'")]
    MissingProjectPath,

    /// Le scope `agent` exige `agent_id`.
    #[error("scope 'agent' requires 'agent_id'")]
    MissingAgentId,

    /// Erreur SQLite ou validation côté `PrefixRuleEngine`.
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

/// Représentation JSON d'une `PrefixRule` retournée à l'agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleDto {
    /// Identifiant SQLite.
    pub id: i64,
    /// Outil ciblé.
    pub tool_name: String,
    /// Préfixe d'argument (None = tout argument).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_prefix: Option<String>,
    /// `"allow"` ou `"deny"`.
    pub action: String,
    /// `"global"` | `"project"` | `"agent"`.
    pub scope: String,
    /// Chemin canonique du projet pour scope `project`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// Identifiant agent pour scope `agent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Auteur de la règle (ADR-086).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Timestamp Unix de création.
    pub created_at: i64,
    /// Expiration Unix optionnelle.
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

/// Input du tool `permission_rule_add`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleAddInput {
    /// Nom de l'outil ciblé.
    pub tool_name: String,
    /// `"allow"` ou `"deny"`.
    pub action: String,
    /// Préfixe d'argument optionnel (None = tout argument).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg_prefix: Option<String>,
    /// `"global"` (défaut), `"project"` ou `"agent"`. Session non supporté
    /// (toujours en RAM via le bouton HITL "session" du desktop).
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Requis lorsque `scope = "project"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// Requis lorsque `scope = "agent"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Timestamp Unix d'expiration (None = règle permanente).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

fn default_scope() -> String {
    "global".to_string()
}

/// Output du tool `permission_rule_add`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleAddOutput {
    /// Identifiant SQLite de la règle créée.
    pub rule_id: i64,
}

/// Tool natif `permission_rule_add` — propose l'ajout d'une règle de
/// permission persistée dans `governance.db`.
///
/// L'invocation est gatée par le `PermissionEngine` (HITL ADR-082). Le champ
/// `created_by` est automatiquement renseigné avec le nom de l'agent appelant
/// (ADR-086).
#[derive(Debug, Clone)]
pub struct PermissionRuleAdd {
    db_path: PathBuf,
    agent_id: String,
}

impl PermissionRuleAdd {
    /// Construit le tool en mémorisant le chemin de `governance.db` et l'`agent_id`
    /// de l'agent propriétaire du dispatcher.
    pub fn new(db_path: PathBuf, agent_id: String) -> Self {
        Self { db_path, agent_id }
    }

    /// Exécute l'ajout typé.
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
            "permission_rule_add (ADR-086)"
        );
        Ok(PermissionRuleAddOutput { rule_id: id })
    }

    /// Descriptor pour enregistrement dans le `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "permission_rule_add".to_string(),
            version: "1.0.0".to_string(),
            description: "Persist a new permission rule in governance.db. The rule is tagged \
                with the calling agent's identity in `created_by`. Subject to HITL approval \
                (ADR-082, ADR-086)."
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

#[async_trait]
impl ToolExecutor for PermissionRuleAdd {
    fn name(&self) -> &str {
        "permission_rule_add"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: PermissionRuleAddInput = serde_json::from_value(input).map_err(|e| {
            ToolExecutionError::InvalidInput {
                message: e.to_string(),
            }
        })?;
        let out = self.run(typed)?;
        serde_json::to_value(out).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ─────────────────────────────────────────────
// permission_rule_remove
// ─────────────────────────────────────────────

/// Input du tool `permission_rule_remove`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleRemoveInput {
    /// Identifiant de la règle à supprimer.
    pub rule_id: i64,
}

/// Output du tool `permission_rule_remove`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleRemoveOutput {
    /// `true` si une ligne a été supprimée, `false` si l'identifiant n'existait pas.
    pub removed: bool,
}

/// Tool natif `permission_rule_remove` — supprime une règle de `governance.db`
/// par identifiant. Gaté par HITL ADR-082.
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
                approval (ADR-082, ADR-086)."
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

#[async_trait]
impl ToolExecutor for PermissionRuleRemove {
    fn name(&self) -> &str {
        "permission_rule_remove"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: PermissionRuleRemoveInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;
        let out = self.run(typed)?;
        serde_json::to_value(out).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ─────────────────────────────────────────────
// permission_rule_list
// ─────────────────────────────────────────────

/// Input du tool `permission_rule_list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionRuleListInput {
    /// Filtre par nom d'outil exact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Filtre par auteur (`created_by`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Filtre par scope (`global` | `project` | `agent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Output du tool `permission_rule_list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleListOutput {
    /// Règles correspondant aux filtres, triées par identifiant croissant.
    pub rules: Vec<PermissionRuleDto>,
}

/// Tool natif `permission_rule_list` — read-only.
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

        // Stratégie : si created_by est fourni, on commence par ce filtre (le
        // plus sélectif), puis on affine en mémoire. Sinon on liste tout (hors
        // session) et on filtre.
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
                tool_name, created_by or scope. Read-only (ADR-086)."
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

#[async_trait]
impl ToolExecutor for PermissionRuleList {
    fn name(&self) -> &str {
        "permission_rule_list"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        // Input vide accepté (filtres optionnels).
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
        // GIVEN un PermissionRuleAdd configuré pour l'agent "onboarding-agent"
        let db = tmp_db();
        let tool = PermissionRuleAdd::new(db.path().to_path_buf(), "onboarding-agent".into());

        // WHEN on appelle run() avec une règle deny http_fetch https://
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

        // THEN la règle existe en DB avec created_by="onboarding-agent"
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
        // GIVEN une règle existante
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
        // GIVEN deux règles d'auteurs différents
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

        // THEN une seule règle, celle de onboarding-agent
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
        let out = list
            .run(PermissionRuleListInput::default())
            .expect("list");
        assert_eq!(out.rules.len(), 2);
    }

    #[test]
    fn descriptors_are_valid() {
        assert!(PermissionRuleAdd::descriptor().validate().is_ok());
        assert!(PermissionRuleRemove::descriptor().validate().is_ok());
        assert!(PermissionRuleList::descriptor().validate().is_ok());
    }
}
