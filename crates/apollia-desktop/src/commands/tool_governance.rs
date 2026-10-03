//! Tauri IPC commands for governing native tools and the desktop frontend's
//! permissions.
//!
//! The tools, their configuration, the permission rules and the audit are
//! served by [`apollia_runtime::tool_governance`] in both modes: called
//! directly when the runtime runs in this process, reached over
//! `/api/v1/governance/...` when the desktop is attached to a daemon. The
//! credentials still drive the consolidated `~/.apollia/governance.db`
//! database directly via the `ToolCredentialStore` of `apollia-tools`.
//!
//! *Session* permission rules are no longer supported on the desktop side:
//! only the `agent` and `global` scopes (persisted in `governance.db`) are
//! exposed to the frontend. The historical in-memory store was removed because
//! no runtime path ever read it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use apollia_permissions::{PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction};
use apollia_runtime::api::routes_tool_governance::{
    RevokeAllBody, RevokedRule, RevokedRules, ToolConfigBody, ToolEnabledBody,
};
use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::tool_governance as governance;
pub use apollia_runtime::tool_governance::{
    AuditEntryDto, PermissionRuleDto, PermissionRuleFilter, ToolStatusDto,
};
use apollia_tools::{GovernanceDb, ToolCredentialStore, GOVERNANCE_DB_FILENAME};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

const KEYFILE_NAME: &str = ".keyfile";
const BRAVE_TEST_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const BRAVE_TEST_TIMEOUT: Duration = Duration::from_secs(10);

// ─────────────────────────────────────────────────────────────────────────────
// DTOs
// ─────────────────────────────────────────────────────────────────────────────

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

pub(crate) fn rule_to_dto_pub(rule: &PrefixRule) -> PermissionRuleDto {
    governance::rule_view(rule)
}

/// Persists a scope-aware rule in `governance.db`.
// REASON: internal helper persisting one scoped rule; the arguments are that rule's columns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn persist_scoped_rule(
    base_dir: &Path,
    tool_name: String,
    arg_prefix: Option<String>,
    action: RuleAction,
    scope: PermissionScope,
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
        agent_id,
        ..PrefixRule::default()
    };

    engine
        .add_rule(&rule)
        .map_err(|e| format!("failed to persist prefix rule: {e}"))
}

/// A query string of the present pairs, each value encoded, `""` when none is.
fn query_string(pairs: &[(&str, Option<String>)]) -> String {
    let present: Vec<String> = pairs
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_ref()
                .map(|v| format!("{key}={}", urlencoding::encode(v)))
        })
        .collect();
    if present.is_empty() {
        String::new()
    } else {
        format!("?{}", present.join("&"))
    }
}

fn tool_path(tool_name: &str, leaf: &str) -> String {
    format!("/api/v1/governance/tools/{}/{leaf}", segment(tool_name))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tools: list / enable / config, served in both modes
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) async fn list_tools_on(link: &RuntimeLink) -> Result<Vec<ToolStatusDto>, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(governance::list_tools(home_base_dir()?)
            .await
            .map_err(|e| e.to_string())?),
        RuntimeLink::Attached(remote) => Ok(remote.get("/api/v1/governance/tools").await?),
    }
}

pub(crate) async fn set_tool_enabled_on(
    link: &RuntimeLink,
    tool_name: String,
    enabled: bool,
) -> Result<(), String> {
    match link {
        RuntimeLink::Embedded(_) => {
            Ok(
                governance::set_tool_enabled(home_base_dir()?, tool_name, enabled)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        RuntimeLink::Attached(remote) => {
            let _: ToolEnabledBody = remote
                .put(
                    &tool_path(&tool_name, "enabled"),
                    &ToolEnabledBody { enabled },
                )
                .await?;
            Ok(())
        }
    }
}

pub(crate) async fn get_tool_config_on(
    link: &RuntimeLink,
    tool_name: String,
) -> Result<Option<serde_json::Value>, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(governance::get_tool_config(home_base_dir()?, tool_name)
            .await
            .map_err(|e| e.to_string())?),
        RuntimeLink::Attached(remote) => {
            let body: ToolConfigBody = remote.get(&tool_path(&tool_name, "config")).await?;
            Ok(body.config)
        }
    }
}

pub(crate) async fn set_tool_config_on(
    link: &RuntimeLink,
    tool_name: String,
    config: serde_json::Value,
) -> Result<(), String> {
    match link {
        RuntimeLink::Embedded(_) => {
            Ok(
                governance::set_tool_config(home_base_dir()?, tool_name, config)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        RuntimeLink::Attached(remote) => {
            let body = ToolConfigBody {
                config: Some(config),
            };
            let _: ToolConfigBody = remote.put(&tool_path(&tool_name, "config"), &body).await?;
            Ok(())
        }
    }
}

/// Returns the full state of each native tool (enabled flag, config,
/// configured credentials, active backend).
///
/// # Errors
///
/// Returns a Tauri-serializable error if the governance database cannot be
/// opened or read, or the daemon refuses.
#[tauri::command]
pub async fn governance_list_tools(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<ToolStatusDto>, String> {
    list_tools_on(link.inner()).await
}

/// Enables or disables a native tool.
///
/// # Errors
///
/// Returns a serializable error if the name is empty or the write fails.
#[tauri::command]
pub async fn governance_set_tool_enabled(
    tool_name: String,
    enabled: bool,
    link: State<'_, RuntimeLink>,
) -> Result<(), String> {
    set_tool_enabled_on(link.inner(), tool_name, enabled).await
}

/// Reads the JSON configuration associated with a tool, or `null`.
///
/// # Errors
///
/// Returns a serializable error if the name is empty or the read fails.
#[tauri::command]
pub async fn governance_get_tool_config(
    tool_name: String,
    link: State<'_, RuntimeLink>,
) -> Result<Option<serde_json::Value>, String> {
    get_tool_config_on(link.inner(), tool_name).await
}

/// Stores a tool's JSON configuration.
///
/// # Errors
///
/// Returns a serializable error if the name is empty or the write fails.
#[tauri::command]
pub async fn governance_set_tool_config(
    tool_name: String,
    config: serde_json::Value,
    link: State<'_, RuntimeLink>,
) -> Result<(), String> {
    set_tool_config_on(link.inner(), tool_name, config).await
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
// Permission rules and audit, served in both modes
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) async fn list_permission_rules_on(
    link: &RuntimeLink,
    filter: PermissionRuleFilter,
) -> Result<Vec<PermissionRuleDto>, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(governance::list_permission_rules(home_base_dir()?, filter)
            .await
            .map_err(|e| e.to_string())?),
        RuntimeLink::Attached(remote) => {
            let query = query_string(&[("scope", filter.scope), ("tool_name", filter.tool_name)]);
            Ok(remote
                .get(&format!("/api/v1/governance/rules{query}"))
                .await?)
        }
    }
}

pub(crate) async fn revoke_permission_rule_on(
    link: &RuntimeLink,
    rule_id: i64,
) -> Result<(), String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(governance::revoke_permission_rule(
            home_base_dir()?,
            rule_id,
        )
        .await
        .map_err(|e| e.to_string())?),
        RuntimeLink::Attached(remote) => {
            let _: RevokedRule = remote
                .delete(&format!("/api/v1/governance/rules/{rule_id}"))
                .await?;
            Ok(())
        }
    }
}

pub(crate) async fn revoke_all_rules_on(
    link: &RuntimeLink,
    scope: Option<String>,
) -> Result<u32, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(governance::revoke_all_rules(home_base_dir()?, scope)
            .await
            .map_err(|e| e.to_string())?),
        RuntimeLink::Attached(remote) => {
            let answer: RevokedRules = remote
                .post(
                    "/api/v1/governance/rules/revoke-all",
                    &RevokeAllBody { scope },
                )
                .await?;
            Ok(answer.removed)
        }
    }
}

pub(crate) async fn list_audit_on(
    link: &RuntimeLink,
    tool_name: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<AuditEntryDto>, String> {
    match link {
        RuntimeLink::Embedded(_) => {
            Ok(
                governance::list_audit(home_base_dir()?, tool_name, limit, offset)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        RuntimeLink::Attached(remote) => {
            let query = query_string(&[
                ("tool_name", tool_name),
                ("limit", limit.map(|l| l.to_string())),
                ("offset", offset.map(|o| o.to_string())),
            ]);
            Ok(remote
                .get(&format!("/api/v1/governance/audit{query}"))
                .await?)
        }
    }
}

/// Lists the `agent` / `global` permission rules from `governance.db`.
///
/// # Errors
///
/// Returns a serializable error if the scope is unknown or the rules cannot
/// be read.
#[tauri::command]
pub async fn governance_list_permission_rules(
    filter: PermissionRuleFilter,
    link: State<'_, RuntimeLink>,
) -> Result<Vec<PermissionRuleDto>, String> {
    list_permission_rules_on(link.inner(), filter).await
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
    link: State<'_, RuntimeLink>,
) -> Result<(), String> {
    revoke_permission_rule_on(link.inner(), rule_id).await
}

/// Deletes all rules of a given scope; `None` targets the `global` rules.
///
/// # Errors
///
/// Returns a serializable error if the scope is unknown or the rules cannot
/// be removed.
#[tauri::command]
pub async fn governance_revoke_all_rules(
    scope: Option<String>,
    link: State<'_, RuntimeLink>,
) -> Result<u32, String> {
    revoke_all_rules_on(link.inner(), scope).await
}

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
    link: State<'_, RuntimeLink>,
) -> Result<Vec<AuditEntryDto>, String> {
    list_audit_on(link.inner(), tool_name, limit, offset).await
}

#[cfg(test)]
mod tests;
