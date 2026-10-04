//! Tauri IPC commands for governing native tools and the desktop frontend's
//! permissions.
//!
//! The tools, their configuration, the permission rules and the audit are
//! served by [`apollia_runtime::tool_governance`] in both modes: called
//! directly when the runtime runs in this process, reached over
//! `/api/v1/governance/...` when the desktop is attached to a daemon. The
//! credentials are served the same way by [`apollia_runtime::tool_credentials`],
//! over `/api/v1/tool-credentials` when attached.
//!
//! *Session* permission rules are no longer supported on the desktop side:
//! only the `agent` and `global` scopes (persisted in `governance.db`) are
//! exposed to the frontend. The historical in-memory store was removed because
//! no runtime path ever read it.

use std::path::PathBuf;

use apollia_permissions::PrefixRule;
use apollia_runtime::api::routes_tool_governance::{
    RevokeAllBody, RevokedRule, RevokedRules, ToolConfigBody, ToolEnabledBody,
};
pub use apollia_runtime::tool_credentials::{
    CredentialEntryView as CredentialEntryDto, CredentialTestView as CredentialTestResultDto,
};
use apollia_runtime::tool_governance as governance;
pub use apollia_runtime::tool_governance::{
    AuditEntryDto, PermissionRuleDto, PermissionRuleFilter, ToolStatusDto,
};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

mod credentials;

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

fn home_base_dir() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()?;
    Ok(apollia_core::paths::data_dir_under(home))
}

pub(crate) fn rule_to_dto_pub(rule: &PrefixRule) -> PermissionRuleDto {
    governance::rule_view(rule)
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
/// Cleartext values are never returned. Attached to a daemon,
/// `GET /api/v1/tool-credentials` answers the same list.
///
/// # Errors
///
/// Returns a serializable error if database access fails.
#[tauri::command]
pub async fn governance_list_credentials(
    tool_name: Option<String>,
    link: State<'_, RuntimeLink>,
) -> Result<Vec<CredentialEntryDto>, String> {
    credentials::list(link.inner(), tool_name).await
}

/// Stores (or updates) a credential for a tool.
///
/// The value goes to the secret store on the Rust side and is never returned
/// to the frontend.
///
/// # Errors
///
/// Returns a serializable error if encryption or writing fails.
#[tauri::command]
pub async fn governance_set_credential(
    tool_name: String,
    key_name: String,
    value: String,
    link: State<'_, RuntimeLink>,
) -> Result<(), String> {
    credentials::set(link.inner(), tool_name, key_name, value).await
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
    link: State<'_, RuntimeLink>,
) -> Result<(), String> {
    credentials::delete(link.inner(), tool_name, key_name).await
}

/// Makes a live call to validate a credential.
///
/// Today only `web_search` (Brave key) is tested; any other `tool_name`
/// returns an explicit error. Attached to a daemon, the daemon makes the call.
///
/// # Errors
///
/// Returns a serializable error if the database cannot be read or if the
/// targeted tool has no validation routine.
#[tauri::command]
pub async fn governance_test_credential(
    tool_name: String,
    link: State<'_, RuntimeLink>,
) -> Result<CredentialTestResultDto, String> {
    credentials::test(link.inner(), tool_name).await
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
