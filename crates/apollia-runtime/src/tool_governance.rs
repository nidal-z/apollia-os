//! The governance of native tools as every surface reads and changes it: the
//! tools and their configuration, and the permission rules.
//!
//! Everything lives in `governance.db` under the data directory. The desktop
//! running the runtime in its own process calls these functions directly; a
//! client of the daemon reaches the same functions through
//! `/api/v1/governance/...`. Both therefore answer the same views and refuse
//! with the same words.

use std::path::{Path, PathBuf};

use apollia_permissions::{PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction};
use apollia_tools::{list_credential_index, GovernanceDb, NativeToolRegistry};
use serde::{Deserialize, Serialize};

/// State of a native tool as displayed by the tools page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ToolStatusDto {
    /// Canonical tool name (e.g. `bash_executor`).
    pub name: String,
    /// `true` when the tool is active.
    pub enabled: bool,
    /// Tool-specific JSON configuration, or `null`.
    #[schema(value_type = Option<Object>)]
    pub config: Option<serde_json::Value>,
    /// Names of the credentials configured for this tool (values are never
    /// returned).
    pub credential_keys: Vec<String>,
    /// Backend actually used by the tool when applicable
    /// (e.g. `"duckduckgo"` or `"brave"` for `web_search`).
    pub active_backend: Option<String>,
}

/// Filter of the permission rules listed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PermissionRuleFilter {
    /// Scope to filter on: `"agent"` or `"global"`.
    pub scope: Option<String>,
    /// Tool name to filter on.
    pub tool_name: Option<String>,
}

/// One permission rule, as the permissions page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PermissionRuleDto {
    /// Identifier of the persisted rule.
    pub id: i64,
    /// Name of the targeted tool.
    pub tool_name: String,
    /// Optional argument prefix.
    pub arg_prefix: Option<String>,
    /// Applied action (`"allow"` or `"deny"`).
    pub action: String,
    /// Rule scope (`"agent"`, `"global"`).
    pub scope: String,
    /// Agent identifier, for `agent` rules.
    pub agent_id: Option<String>,
    /// ISO 8601 expiration date, if any.
    pub expires_at: Option<String>,
    /// ISO 8601 creation date.
    pub created_at: String,
    /// Author of the rule (`None` means a human operator).
    pub created_by: Option<String>,
}

/// Why a governance operation did not complete. The display text is what the
/// interface shows, the same whichever process ran the operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolGovernanceError {
    /// The runtime was started without a data directory.
    #[error("the governance database has no data directory")]
    Unavailable,
    /// The tool name is empty.
    #[error("tool_name must not be empty")]
    EmptyToolName,
    /// The scope is neither `agent` nor `global`.
    #[error("unknown scope '{0}', expected 'agent' | 'global'")]
    UnknownScope(String),
    /// No rule has this identifier.
    #[error("permission rule {0} not found")]
    RuleNotFound(i64),
    /// A store of `governance.db` refused the operation.
    #[error("{context}: {source}")]
    Storage {
        /// What was being done, in the words the interface already shows.
        context: &'static str,
        /// The store's own error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The blocking task did not complete.
    #[error("spawn_blocking failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}

fn storage<E>(context: &'static str) -> impl FnOnce(E) -> ToolGovernanceError
where
    E: std::error::Error + Send + Sync + 'static,
{
    move |source| ToolGovernanceError::Storage {
        context,
        source: Box::new(source),
    }
}

/// Opens `governance.db` under `base_dir`, migrating it if needed, and returns
/// its path.
fn governance_db(base_dir: &Path) -> Result<PathBuf, ToolGovernanceError> {
    if base_dir.as_os_str().is_empty() {
        return Err(ToolGovernanceError::Unavailable);
    }
    let db = GovernanceDb::open(base_dir).map_err(storage("failed to open governance database"))?;
    Ok(db.path().to_path_buf())
}

fn require_tool_name(tool_name: &str) -> Result<(), ToolGovernanceError> {
    if tool_name.trim().is_empty() {
        return Err(ToolGovernanceError::EmptyToolName);
    }
    Ok(())
}

fn iso8601(secs: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

fn parse_scope(value: &str) -> Result<PermissionScope, ToolGovernanceError> {
    match value {
        "agent" => Ok(PermissionScope::Agent),
        "global" => Ok(PermissionScope::Global),
        other => Err(ToolGovernanceError::UnknownScope(other.to_owned())),
    }
}

fn active_backend_for(tool_name: &str, credential_keys: &[String]) -> Option<String> {
    if tool_name != "web_search" {
        return None;
    }
    let has_brave = credential_keys.iter().any(|k| k == "brave.api_key");
    Some(if has_brave { "brave" } else { "duckduckgo" }.to_owned())
}

/// The view of one persisted rule.
pub fn rule_view(rule: &PrefixRule) -> PermissionRuleDto {
    PermissionRuleDto {
        id: rule.id,
        tool_name: rule.tool_name.clone(),
        arg_prefix: rule.arg_prefix.clone(),
        action: match rule.action {
            RuleAction::Allow => "allow".to_owned(),
            RuleAction::Deny => "deny".to_owned(),
        },
        scope: rule.scope.as_str().to_owned(),
        agent_id: rule.agent_id.clone(),
        expires_at: rule.expires_at.map(iso8601),
        created_at: iso8601(rule.created_at),
        created_by: rule.created_by_agent.clone(),
    }
}

async fn blocking<T, F>(work: F) -> Result<T, ToolGovernanceError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ToolGovernanceError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(ToolGovernanceError::Join)?
}

/// Every native tool with its flag, configuration, credential names and
/// active backend.
///
/// # Errors
///
/// [`ToolGovernanceError::Storage`] when `governance.db` cannot be read.
pub async fn list_tools(base_dir: PathBuf) -> Result<Vec<ToolStatusDto>, ToolGovernanceError> {
    blocking(move || {
        let db_path = governance_db(&base_dir)?;
        let registry =
            NativeToolRegistry::new(&db_path).map_err(storage("failed to open tool registry"))?;
        let statuses = registry.list().map_err(storage("failed to list tools"))?;
        let creds =
            list_credential_index(&db_path, None).map_err(storage("failed to list credentials"))?;
        Ok(statuses
            .into_iter()
            .map(|s| {
                let credential_keys: Vec<String> = creds
                    .iter()
                    .filter(|c| c.tool_name == s.name)
                    .map(|c| c.key_name.clone())
                    .collect();
                let active_backend = active_backend_for(&s.name, &credential_keys);
                ToolStatusDto {
                    name: s.name,
                    enabled: s.enabled,
                    config: s.config,
                    credential_keys,
                    active_backend,
                }
            })
            .collect())
    })
    .await
}

/// Enables or disables one native tool.
///
/// # Errors
///
/// [`ToolGovernanceError::EmptyToolName`], or
/// [`ToolGovernanceError::Storage`] when the write fails.
pub async fn set_tool_enabled(
    base_dir: PathBuf,
    tool_name: String,
    enabled: bool,
) -> Result<(), ToolGovernanceError> {
    require_tool_name(&tool_name)?;
    blocking(move || {
        let db_path = governance_db(&base_dir)?;
        let mut registry =
            NativeToolRegistry::new(&db_path).map_err(storage("failed to open tool registry"))?;
        registry
            .set_enabled(&tool_name, enabled)
            .map_err(storage("failed to update tool"))?;
        tracing::event!(tracing::Level::INFO, tool = %tool_name, enabled, "tool.enabled.updated");
        Ok(())
    })
    .await
}

/// The JSON configuration of one tool, or `None`.
///
/// # Errors
///
/// [`ToolGovernanceError::EmptyToolName`], or
/// [`ToolGovernanceError::Storage`] when the read fails.
pub async fn get_tool_config(
    base_dir: PathBuf,
    tool_name: String,
) -> Result<Option<serde_json::Value>, ToolGovernanceError> {
    require_tool_name(&tool_name)?;
    blocking(move || {
        let db_path = governance_db(&base_dir)?;
        let registry =
            NativeToolRegistry::new(&db_path).map_err(storage("failed to open tool registry"))?;
        registry
            .get_config(&tool_name)
            .map_err(storage("failed to read tool config"))
    })
    .await
}

/// Stores the JSON configuration of one tool.
///
/// # Errors
///
/// [`ToolGovernanceError::EmptyToolName`], or
/// [`ToolGovernanceError::Storage`] when the write fails.
pub async fn set_tool_config(
    base_dir: PathBuf,
    tool_name: String,
    config: serde_json::Value,
) -> Result<(), ToolGovernanceError> {
    require_tool_name(&tool_name)?;
    blocking(move || {
        let db_path = governance_db(&base_dir)?;
        let mut registry =
            NativeToolRegistry::new(&db_path).map_err(storage("failed to open tool registry"))?;
        registry
            .set_config(&tool_name, &config)
            .map_err(storage("failed to write tool config"))?;
        tracing::event!(tracing::Level::INFO, tool = %tool_name, "tool.config.updated");
        Ok(())
    })
    .await
}

/// The `agent` and `global` permission rules matching `filter`.
///
/// # Errors
///
/// [`ToolGovernanceError::UnknownScope`], or
/// [`ToolGovernanceError::Storage`] when the rules cannot be read.
pub async fn list_permission_rules(
    base_dir: PathBuf,
    filter: PermissionRuleFilter,
) -> Result<Vec<PermissionRuleDto>, ToolGovernanceError> {
    let scope = filter.scope.as_deref().map(parse_scope).transpose()?;
    blocking(move || {
        let db_path = governance_db(&base_dir)?;
        let engine = PrefixRuleEngine::new(&db_path)
            .map_err(storage("failed to open prefix rule engine"))?;
        let rules = engine
            .list_rules_filtered(scope)
            .map_err(storage("failed to list permission rules"))?;
        Ok(rules
            .iter()
            .filter(|r| filter.tool_name.as_deref().is_none_or(|t| r.tool_name == t))
            .map(rule_view)
            .collect())
    })
    .await
}

/// Deletes the rule identified by `rule_id`.
///
/// # Errors
///
/// [`ToolGovernanceError::RuleNotFound`], or
/// [`ToolGovernanceError::Storage`] when the deletion fails.
pub async fn revoke_permission_rule(
    base_dir: PathBuf,
    rule_id: i64,
) -> Result<(), ToolGovernanceError> {
    blocking(move || {
        let db_path = governance_db(&base_dir)?;
        let mut engine = PrefixRuleEngine::new(&db_path)
            .map_err(storage("failed to open prefix rule engine"))?;
        let removed = engine
            .remove_rule_checked(rule_id)
            .map_err(storage("failed to remove permission rule"))?;
        if !removed {
            return Err(ToolGovernanceError::RuleNotFound(rule_id));
        }
        tracing::event!(tracing::Level::INFO, rule_id, "permission.rule.revoked");
        Ok(())
    })
    .await
}

/// Deletes every rule of one scope, `global` when `scope` is `None`, and
/// answers how many were removed.
///
/// # Errors
///
/// [`ToolGovernanceError::UnknownScope`], or
/// [`ToolGovernanceError::Storage`] when the deletion fails.
pub async fn revoke_all_rules(
    base_dir: PathBuf,
    scope: Option<String>,
) -> Result<u32, ToolGovernanceError> {
    let target = scope
        .as_deref()
        .map(parse_scope)
        .transpose()?
        .unwrap_or(PermissionScope::Global);
    blocking(move || {
        let db_path = governance_db(&base_dir)?;
        let mut engine = PrefixRuleEngine::new(&db_path)
            .map_err(storage("failed to open prefix rule engine"))?;
        let removed = engine
            .remove_rules_by_scope(target)
            .map_err(storage("failed to revoke rules"))?;
        tracing::event!(
            tracing::Level::INFO,
            scope = %target.as_str(),
            count = removed,
            "permission.rules.revoked"
        );
        Ok(removed)
    })
    .await
}

#[cfg(test)]
mod tests;
