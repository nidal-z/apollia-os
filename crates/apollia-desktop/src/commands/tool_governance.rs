//! Tauri IPC commands for governing native tools and the desktop frontend's
//! permissions.
//!
//! These commands drive the consolidated `~/.apollia/governance.db` database
//! directly via the `ToolRegistry`, `ToolCredentialStore`, `PrefixRuleEngine`
//! and `PermissionAuditLog` components exposed by the `apollia-tools` and
//! `apollia-permissions` crates.
//!
//! *Session* permission rules are no longer supported on the desktop side:
//! only the `project` and `global` scopes (persisted in `governance.db`) are
//! exposed to the frontend. The historical in-memory store was removed because
//! no runtime path ever read it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use apollia_permissions::{
    PermissionAuditLog, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction,
};
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::{
    GovernanceDb, NativeToolRegistry, ToolCredentialStore, GOVERNANCE_DB_FILENAME,
};
use serde::{Deserialize, Serialize};
use tauri::State;

const KEYFILE_NAME: &str = ".keyfile";
const BRAVE_TEST_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const BRAVE_TEST_TIMEOUT: Duration = Duration::from_secs(10);

// ─────────────────────────────────────────────────────────────────────────────
// DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// State of a native tool as displayed by the `/settings/tools` page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStatusDto {
    /// Canonical tool name (e.g. `bash_executor`).
    pub name: String,
    /// `true` when the tool is active. See [`apollia_tools::NativeToolRegistry`].
    pub enabled: bool,
    /// Tool-specific JSON configuration, or `null`.
    pub config: Option<serde_json::Value>,
    /// Names of the credentials configured for this tool (values are never
    /// returned to the frontend).
    pub credential_keys: Vec<String>,
    /// Backend actually used by the tool when applicable
    /// (e.g. `"duckduckgo"` or `"brave"` for `web_search`).
    pub active_backend: Option<String>,
}

/// Metadata for a credential stored for a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialEntryDto {
    /// Name of the owning tool.
    pub tool_name: String,
    /// Logical key name (e.g. `brave.api_key`).
    pub key_name: String,
    /// Creation date in ISO 8601 / RFC 3339 format.
    pub created_at: String,
    /// Date of the last actual use, if any.
    pub last_used_at: Option<String>,
}

/// Result of a live credential validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialTestResultDto {
    /// `true` if the call succeeded.
    pub ok: bool,
    /// Measured latency in milliseconds (round-trip).
    pub latency_ms: Option<u64>,
    /// Diagnostic error message when `ok` is `false`.
    pub error: Option<String>,
}

/// Filter passed to [`list_permission_rules`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PermissionRuleFilter {
    /// Scope to filter on: `"session"`, `"project"` or `"global"`.
    pub scope: Option<String>,
    /// Tool name to filter on.
    pub tool_name: Option<String>,
}

/// Frontend representation of a permission rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleDto {
    /// Identifier. Positive for persisted rules (DB), negative for in-memory
    /// session rules.
    pub id: i64,
    /// Name of the targeted tool.
    pub tool_name: String,
    /// Optional argument prefix.
    pub arg_prefix: Option<String>,
    /// Applied action (`"allow"` or `"deny"`).
    pub action: String,
    /// Rule scope (`"session"`, `"project"`, `"global"`).
    pub scope: String,
    /// Canonical project path, for `project` rules.
    pub project_path: Option<String>,
    /// Agent identifier, for `agent` rules.
    pub agent_id: Option<String>,
    /// ISO 8601 expiration date, if any.
    pub expires_at: Option<String>,
    /// ISO 8601 creation date.
    pub created_at: String,
    /// Author of the rule (`None` means a human operator).
    pub created_by: Option<String>,
}

/// Entry in the immutable audit log of permission decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntryDto {
    /// Auto-incremented identifier.
    pub id: i64,
    /// Name of the invoked tool.
    pub tool_name: String,
    /// First argument extracted from the invocation, when available.
    pub first_arg: Option<String>,
    /// Serialized decision (e.g. `"AutoAllowedSafeList"`).
    pub decision: String,
    /// Scope of the rule that decided, when relevant.
    pub scope: Option<String>,
    /// Identifier of the rule that triggered the decision, when applicable.
    pub rule_id: Option<i64>,
    /// Name of the agent behind the invocation, if provided.
    pub agent: Option<String>,
    /// Decision date in ISO 8601 format.
    pub decided_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

fn home_base_dir() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()?;
    Ok(apollia_core::paths::data_dir_under(home))
}

/// Opens `governance.db` in `~/.apollia/`, performing the initial migration if
/// needed. Returns the file path.
fn ensure_governance_db() -> Result<PathBuf, String> {
    let base = home_base_dir()?;
    let db = GovernanceDb::open(&base)
        .map_err(|e| format!("failed to open governance database: {e}"))?;
    Ok(db.path().to_path_buf())
}

fn keyfile_path_for(base_dir: &Path) -> PathBuf {
    base_dir.join(KEYFILE_NAME)
}

fn current_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn iso8601(secs: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

fn iso8601_opt(secs: Option<i64>) -> Option<String> {
    secs.map(iso8601)
}

fn parse_scope(value: &str) -> Result<PermissionScope, String> {
    match value {
        "session" => Ok(PermissionScope::Session),
        "project" => Ok(PermissionScope::Project),
        "agent" => Ok(PermissionScope::Agent),
        "global" => Ok(PermissionScope::Global),
        other => Err(format!(
            "unknown scope '{other}', expected 'session' | 'project' | 'agent' | 'global'"
        )),
    }
}

pub(crate) fn rule_to_dto_pub(rule: &PrefixRule) -> PermissionRuleDto {
    rule_to_dto(rule)
}

fn rule_to_dto(rule: &PrefixRule) -> PermissionRuleDto {
    PermissionRuleDto {
        id: rule.id,
        tool_name: rule.tool_name.clone(),
        arg_prefix: rule.arg_prefix.clone(),
        action: match rule.action {
            RuleAction::Allow => "allow".to_string(),
            RuleAction::Deny => "deny".to_string(),
        },
        scope: rule.scope.as_str().to_string(),
        project_path: rule
            .project_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        agent_id: rule.agent_id.clone(),
        expires_at: iso8601_opt(rule.expires_at),
        created_at: iso8601(rule.created_at),
        created_by: rule.created_by_agent.clone(),
    }
}

/// Normalizes a raw `project_path` received from the frontend: trim and
/// canonicalize when the path exists. Non-existent paths are returned as-is
/// (useful to describe a deleted project we still want to filter on).
pub(crate) fn canonical_project_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("project_path must not be empty".to_string());
    }
    let path = PathBuf::from(trimmed);
    match path.canonicalize() {
        Ok(c) => Ok(c),
        Err(_) => Ok(path),
    }
}

/// Persists a scope-aware rule in `governance.db`.
// Rule persistence: the tool name, arg prefix, action, scope, project path and
// agent id exceed 5 by design; they are the full scope-key of a governance rule.
// REASON: internal helper persisting one scoped rule; the arguments are that rule's columns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn persist_scoped_rule(
    base_dir: &Path,
    tool_name: String,
    arg_prefix: Option<String>,
    action: RuleAction,
    scope: PermissionScope,
    project_path: Option<PathBuf>,
    agent_id: Option<String>,
) -> Result<i64, String> {
    let _migrate = GovernanceDb::open(base_dir)
        .map_err(|e| format!("failed to open governance database: {e}"))?;
    let db_path = base_dir.join(GOVERNANCE_DB_FILENAME);

    let mut engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open prefix rule engine: {e}"))?;

    let rule = PrefixRule {
        tool_name,
        arg_prefix,
        action,
        created_at: current_unix_secs(),
        scope,
        project_path,
        agent_id,
        ..PrefixRule::default()
    };

    engine
        .add_rule(&rule)
        .map_err(|e| format!("failed to persist prefix rule: {e}"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tools: list / enable / config
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the full state of each native tool (enabled flag, config,
/// configured credentials, active backend).
///
/// # Errors
///
/// Returns a Tauri-serializable error if the governance database cannot be
/// opened or read.
#[tauri::command]
pub async fn governance_list_tools(
    _state: State<'_, RuntimeHandle>,
) -> Result<Vec<ToolStatusDto>, String> {
    let base = home_base_dir()?;
    let db_path = ensure_governance_db()?;
    let registry = NativeToolRegistry::new(&db_path)
        .map_err(|e| format!("failed to open tool registry: {e}"))?;
    let store = ToolCredentialStore::new(&db_path, &keyfile_path_for(&base))
        .map_err(|e| format!("failed to open credential store: {e}"))?;

    let statuses = registry
        .list()
        .map_err(|e| format!("failed to list tools: {e}"))?;
    let creds = store
        .list(None)
        .map_err(|e| format!("failed to list credentials: {e}"))?;

    let dtos = statuses
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
        .collect();
    Ok(dtos)
}

fn active_backend_for(tool_name: &str, credential_keys: &[String]) -> Option<String> {
    if tool_name != "web_search" {
        return None;
    }
    let has_brave = credential_keys.iter().any(|k| k == "brave.api_key");
    Some(if has_brave {
        "brave".into()
    } else {
        "duckduckgo".into()
    })
}

/// Enables or disables a native tool via `ToolRegistry::set_enabled`.
///
/// # Errors
///
/// Returns a serializable error if the database write fails.
#[tauri::command]
pub async fn governance_set_tool_enabled(
    tool_name: String,
    enabled: bool,
    _state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    if tool_name.trim().is_empty() {
        return Err("tool_name must not be empty".to_string());
    }
    let db_path = ensure_governance_db()?;
    let mut registry = NativeToolRegistry::new(&db_path)
        .map_err(|e| format!("failed to open tool registry: {e}"))?;
    registry
        .set_enabled(&tool_name, enabled)
        .map_err(|e| format!("failed to update tool: {e}"))?;
    tracing::info!(tool = %tool_name, enabled, "tool.enabled.updated");
    Ok(())
}

/// Reads the JSON configuration associated with a tool, or `null`.
///
/// # Errors
///
/// Returns a serializable error if the read fails.
#[tauri::command]
pub async fn governance_get_tool_config(
    tool_name: String,
    _state: State<'_, RuntimeHandle>,
) -> Result<Option<serde_json::Value>, String> {
    if tool_name.trim().is_empty() {
        return Err("tool_name must not be empty".to_string());
    }
    let db_path = ensure_governance_db()?;
    let registry = NativeToolRegistry::new(&db_path)
        .map_err(|e| format!("failed to open tool registry: {e}"))?;
    registry
        .get_config(&tool_name)
        .map_err(|e| format!("failed to read tool config: {e}"))
}

/// Stores a tool's JSON configuration.
///
/// # Errors
///
/// Returns a serializable error if the write fails.
#[tauri::command]
pub async fn governance_set_tool_config(
    tool_name: String,
    config: serde_json::Value,
    _state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    if tool_name.trim().is_empty() {
        return Err("tool_name must not be empty".to_string());
    }
    let db_path = ensure_governance_db()?;
    let mut registry = NativeToolRegistry::new(&db_path)
        .map_err(|e| format!("failed to open tool registry: {e}"))?;
    registry
        .set_config(&tool_name, &config)
        .map_err(|e| format!("failed to write tool config: {e}"))?;
    tracing::info!(tool = %tool_name, "tool.config.updated");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Credentials: list / set / delete / test
// ─────────────────────────────────────────────────────────────────────────────

/// Lists the configured credentials, optionally filtered by tool.
/// Cleartext values are never returned.
///
/// # Errors
///
/// Returns a serializable error if database access fails.
#[tauri::command]
pub async fn governance_list_credentials(
    tool_name: Option<String>,
    _state: State<'_, RuntimeHandle>,
) -> Result<Vec<CredentialEntryDto>, String> {
    let base = home_base_dir()?;
    let db_path = ensure_governance_db()?;
    let store = ToolCredentialStore::new(&db_path, &keyfile_path_for(&base))
        .map_err(|e| format!("failed to open credential store: {e}"))?;
    let entries = store
        .list(tool_name.as_deref())
        .map_err(|e| format!("failed to list credentials: {e}"))?;
    Ok(entries
        .into_iter()
        .map(|e| CredentialEntryDto {
            tool_name: e.tool_name,
            key_name: e.key_name,
            created_at: iso8601(e.created_at),
            last_used_at: iso8601_opt(e.last_used_at),
        })
        .collect())
}

/// Stores (or updates) a credential for a tool.
///
/// The cleartext value is encrypted with AES-256-GCM on the Rust side and is
/// never returned to the frontend.
///
/// # Errors
///
/// Returns a serializable error if encryption or writing fails.
#[tauri::command]
pub async fn governance_set_credential(
    tool_name: String,
    key_name: String,
    value: String,
    _state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    if tool_name.trim().is_empty() || key_name.trim().is_empty() {
        return Err("tool_name and key_name must not be empty".to_string());
    }
    if value.is_empty() {
        return Err("credential value must not be empty".to_string());
    }
    let base = home_base_dir()?;
    let db_path = ensure_governance_db()?;
    let mut store = ToolCredentialStore::new(&db_path, &keyfile_path_for(&base))
        .map_err(|e| format!("failed to open credential store: {e}"))?;
    store
        .set(&tool_name, &key_name, &value)
        .map_err(|e| format!("failed to store credential: {e}"))?;
    tracing::info!(tool = %tool_name, key = %key_name, "tool.credential.stored");
    Ok(())
}

/// Deletes a credential identified by `(tool_name, key_name)`.
///
/// # Errors
///
/// Returns a serializable error if the deletion fails.
#[tauri::command]
pub async fn governance_delete_credential(
    tool_name: String,
    key_name: String,
    _state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    let base = home_base_dir()?;
    let db_path = ensure_governance_db()?;
    let mut store = ToolCredentialStore::new(&db_path, &keyfile_path_for(&base))
        .map_err(|e| format!("failed to open credential store: {e}"))?;
    store
        .delete(&tool_name, &key_name)
        .map_err(|e| format!("failed to delete credential: {e}"))?;
    tracing::info!(tool = %tool_name, key = %key_name, "tool.credential.deleted");
    Ok(())
}

/// Makes a live call to validate a credential.
///
/// Today only `web_search` (Brave key) is tested: a GET to
/// `https://api.search.brave.com/res/v1/web/search` is issued with the key in
/// the `X-Subscription-Token` header. Any other `tool_name` combination returns
/// an explicit error.
///
/// # Errors
///
/// Returns a serializable error if the database cannot be read or if the
/// targeted tool has no validation routine.
#[tauri::command]
pub async fn governance_test_credential(
    tool_name: String,
    _state: State<'_, RuntimeHandle>,
) -> Result<CredentialTestResultDto, String> {
    if tool_name != "web_search" {
        return Err(format!(
            "no credential test implemented for tool '{tool_name}'"
        ));
    }

    let base = home_base_dir()?;
    let db_path = ensure_governance_db()?;
    let store = ToolCredentialStore::new(&db_path, &keyfile_path_for(&base))
        .map_err(|e| format!("failed to open credential store: {e}"))?;
    let key = match store
        .get("web_search", "brave.api_key")
        .map_err(|e| format!("failed to read credential: {e}"))?
    {
        Some(k) => k,
        None => {
            return Ok(CredentialTestResultDto {
                ok: false,
                latency_ms: None,
                error: Some("brave.api_key is not configured".to_string()),
            });
        }
    };

    let outcome = test_brave_key(&key).await;

    if outcome.ok {
        let mut store_mut = ToolCredentialStore::new(&db_path, &keyfile_path_for(&base))
            .map_err(|e| format!("failed to reopen credential store: {e}"))?;
        if let Err(err) = store_mut.touch_last_used("web_search", "brave.api_key") {
            tracing::warn!(error = %err, "tool.credential.last_used.update.failed");
        }
    }

    Ok(outcome)
}

async fn test_brave_key(key: &str) -> CredentialTestResultDto {
    let client = match apollia_core::net::safe_client_builder()
        .timeout(BRAVE_TEST_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return CredentialTestResultDto {
                ok: false,
                latency_ms: None,
                error: Some(format!("failed to build HTTP client: {e}")),
            };
        }
    };
    let started = Instant::now();
    let response = client
        .get(BRAVE_TEST_URL)
        .header("X-Subscription-Token", key)
        .header("Accept", "application/json")
        .query(&[("q", "apollia"), ("count", "1")])
        .send()
        .await;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    match response {
        Ok(r) if r.status().is_success() => CredentialTestResultDto {
            ok: true,
            latency_ms: Some(latency_ms),
            error: None,
        },
        Ok(r) => CredentialTestResultDto {
            ok: false,
            latency_ms: Some(latency_ms),
            error: Some(format!("brave returned HTTP {}", r.status().as_u16())),
        },
        Err(e) => CredentialTestResultDto {
            ok: false,
            latency_ms: Some(latency_ms),
            error: Some(e.to_string()),
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Permission rules: list / revoke
// ─────────────────────────────────────────────────────────────────────────────

/// Lists the `project` / `global` permission rules from `governance.db`.
///
/// # Errors
///
/// Returns a serializable error if the scope is unknown or the database cannot
/// be read.
#[tauri::command]
pub async fn governance_list_permission_rules(
    filter: PermissionRuleFilter,
    _state: State<'_, RuntimeHandle>,
) -> Result<Vec<PermissionRuleDto>, String> {
    let scope_filter = match &filter.scope {
        Some(s) => Some(parse_scope(s)?),
        None => None,
    };
    let tool_filter = filter.tool_name.as_deref();

    let mut out = Vec::new();

    let db_path = ensure_governance_db()?;
    let engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open prefix rule engine: {e}"))?;
    let rules = engine
        .list_rules_filtered(scope_filter, None)
        .map_err(|e| format!("failed to list permission rules: {e}"))?;
    for r in rules {
        if matches_tool(&r.tool_name, tool_filter) {
            out.push(rule_to_dto(&r));
        }
    }

    Ok(out)
}

fn matches_tool(rule_tool: &str, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(t) => rule_tool == t,
    }
}

/// Deletes the rule identified by `rule_id`.
///
/// # Errors
///
/// Returns a serializable error if the identifier does not exist or the
/// deletion fails.
#[tauri::command]
pub async fn governance_revoke_permission_rule(
    rule_id: i64,
    _state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    let db_path = ensure_governance_db()?;
    let mut engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open prefix rule engine: {e}"))?;
    let removed = engine
        .remove_rule_checked(rule_id)
        .map_err(|e| format!("failed to remove permission rule: {e}"))?;
    if !removed {
        return Err(format!("permission rule {rule_id} not found"));
    }
    tracing::info!(rule_id, "permission.rule.revoked");
    Ok(())
}

/// Deletes all rules of a given scope; `None` targets both `project` and
/// `global`.
///
/// # Errors
///
/// Returns a serializable error if the scope is unknown or the database cannot
/// be modified.
#[tauri::command]
pub async fn governance_revoke_all_rules(
    scope: Option<String>,
    _state: State<'_, RuntimeHandle>,
) -> Result<u32, String> {
    let target_scope = match &scope {
        Some(s) => Some(parse_scope(s)?),
        None => None,
    };

    let mut total: u32 = 0;

    let db_path = ensure_governance_db()?;
    let mut engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open prefix rule engine: {e}"))?;
    let scopes_to_clear: Vec<PermissionScope> = match target_scope {
        None => vec![PermissionScope::Project, PermissionScope::Global],
        Some(s) => vec![s],
    };
    for s in scopes_to_clear {
        let removed = engine
            .remove_rules_by_scope(s, None)
            .map_err(|e| format!("failed to revoke rules: {e}"))?;
        total = total.saturating_add(removed);
    }

    tracing::info!(scope = ?scope, count = total, "permission.rules.revoked");
    Ok(total)
}

// ─────────────────────────────────────────────────────────────────────────────
// Audit log
// ─────────────────────────────────────────────────────────────────────────────

/// Lists the audit log entries, optionally filtered by tool and paginated.
/// Entries are sorted by date descending.
///
/// # Errors
///
/// Returns a serializable error if the read fails.
#[tauri::command]
pub async fn governance_list_audit(
    tool_name: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    _state: State<'_, RuntimeHandle>,
) -> Result<Vec<AuditEntryDto>, String> {
    let db_path = ensure_governance_db()?;
    let log =
        PermissionAuditLog::new(&db_path).map_err(|e| format!("failed to open audit log: {e}"))?;
    let entries = log
        .query(
            tool_name.as_deref(),
            limit.unwrap_or(100),
            offset.unwrap_or(0),
        )
        .map_err(|e| format!("failed to query audit log: {e}"))?;
    Ok(entries
        .into_iter()
        .map(|e| AuditEntryDto {
            id: e.id,
            tool_name: e.tool_name,
            first_arg: e.first_arg,
            decision: e.decision,
            scope: e.scope,
            rule_id: e.rule_id,
            agent: e.agent,
            decided_at: iso8601(e.decided_at),
        })
        .collect())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection, OpenFlags};
    use tempfile::TempDir;

    fn count_project_rules(db_path: &Path) -> i64 {
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open ro");
        conn.query_row(
            "SELECT COUNT(*) FROM permission_rules WHERE scope = 'project'",
            params![],
            |r| r.get::<_, i64>(0),
        )
        .expect("count")
    }

    #[test]
    fn test_add_project_rule_persisted_with_path() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let project = PathBuf::from("/home/user/projet-foo");

        // WHEN
        let id = persist_scoped_rule(
            dir.path(),
            "bash_executor".into(),
            Some("git".into()),
            RuleAction::Allow,
            PermissionScope::Project,
            Some(project.clone()),
            None,
        )
        .expect("persist project");

        // THEN
        assert!(id > 0);
        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
        let engine = PrefixRuleEngine::new(&db_path).expect("engine");
        let rules = engine
            .list_rules_filtered(Some(PermissionScope::Project), Some(&project))
            .expect("list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].project_path.as_deref(), Some(project.as_path()));
        assert_eq!(rules[0].action, RuleAction::Allow);
    }

    #[test]
    fn test_revoke_rule_removes_from_db() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let id = persist_scoped_rule(
            dir.path(),
            "bash_executor".into(),
            None,
            RuleAction::Allow,
            PermissionScope::Global,
            None,
            None,
        )
        .expect("persist global");
        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);

        // WHEN
        let mut engine = PrefixRuleEngine::new(&db_path).expect("engine");
        let removed = engine.remove_rule_checked(id).expect("remove");

        // THEN
        assert!(removed);
        assert!(engine.list_rules().expect("list").is_empty());
    }

    #[test]
    fn test_remove_rules_by_scope_only_clears_target() {
        // GIVEN: one global rule and one project rule
        let dir = TempDir::new().expect("tempdir");
        let project = PathBuf::from("/home/user/projet-bar");
        persist_scoped_rule(
            dir.path(),
            "bash_executor".into(),
            None,
            RuleAction::Allow,
            PermissionScope::Global,
            None,
            None,
        )
        .expect("global");
        persist_scoped_rule(
            dir.path(),
            "bash_executor".into(),
            None,
            RuleAction::Allow,
            PermissionScope::Project,
            Some(project),
            None,
        )
        .expect("project");
        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
        assert_eq!(count_project_rules(&db_path), 1);

        // WHEN: only the global rules are removed
        let mut engine = PrefixRuleEngine::new(&db_path).expect("engine");
        let removed = engine
            .remove_rules_by_scope(PermissionScope::Global, None)
            .expect("remove globals");

        // THEN: only the project rule remains
        assert_eq!(removed, 1);
        let remaining = engine.list_rules().expect("list");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].scope, PermissionScope::Project);
    }

    #[test]
    fn test_active_backend_for_web_search() {
        assert_eq!(
            active_backend_for("web_search", &["brave.api_key".to_string()]),
            Some("brave".to_string())
        );
        assert_eq!(
            active_backend_for("web_search", &[]),
            Some("duckduckgo".to_string())
        );
        assert!(active_backend_for("bash_executor", &[]).is_none());
    }

    #[test]
    fn test_parse_scope_unknown_value() {
        assert!(parse_scope("user").is_err());
        // "session" is now accepted for backward compatibility with the
        // frontend filters; list_rules_filtered simply returns an empty list
        // since no rule has scope='session' in the database.
        assert_eq!(parse_scope("session"), Ok(PermissionScope::Session));
        assert_eq!(parse_scope("project"), Ok(PermissionScope::Project));
        assert_eq!(parse_scope("agent"), Ok(PermissionScope::Agent));
        assert_eq!(parse_scope("global"), Ok(PermissionScope::Global));
    }

    #[test]
    fn test_rule_to_dto_serializes_action_and_scope() {
        let rule = PrefixRule {
            id: 7,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Deny,
            created_at: 1_700_000_000,
            scope: PermissionScope::Project,
            project_path: Some(PathBuf::from("/tmp/p")),
            agent_id: None,
            expires_at: None,
            created_by_agent: Some("operator".into()),
        };
        let dto = rule_to_dto(&rule);
        assert_eq!(dto.id, 7);
        assert_eq!(dto.action, "deny");
        assert_eq!(dto.scope, "project");
        assert_eq!(dto.project_path.as_deref(), Some("/tmp/p"));
        assert_eq!(dto.created_by.as_deref(), Some("operator"));
    }

    #[test]
    fn test_canonical_project_path_rejects_empty() {
        assert!(canonical_project_path("").is_err());
        assert!(canonical_project_path("   ").is_err());
        // Non-existent → returned as-is.
        let res = canonical_project_path("/definitely/not/here").expect("ok");
        assert_eq!(res, PathBuf::from("/definitely/not/here"));
    }
}
