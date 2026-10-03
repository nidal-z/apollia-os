//! Tauri IPC commands for the permissions given in the chat.
//!
//! Covers:
//! - list/delete of the `scope = 'agent'` rules created by the chat's
//!   "Always allow" button (`agent_id = "apollia:chat"`);
//! - list/revoke of the `scope = 'session'` authorizations active sessions
//!   hold in memory.
//!
//! The settings of the chat are those of the Apollia agent and are adjusted
//! on its sheet, through the commands of `agent_adjust`.

use std::path::PathBuf;

use apollia_permissions::PrefixRuleEngine;
use apollia_runtime::chat::SessionAuthorizationView;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::GovernanceDb;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::tool_governance::PermissionRuleDto;

/// Logical identifier of the Apollia Chat system agent.
///
/// Must stay aligned with `apollia_runtime::chat::APOLLIA_CHAT_AGENT_ID`.
pub const APOLLIA_CHAT_AGENT_ID: &str = "apollia:chat";

/// Opens (and migrates if needed) `governance.db`, then returns its path.
fn ensure_governance_db() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()?;
    let base = apollia_core::paths::data_dir_under(home);
    let db = GovernanceDb::open(&base)
        .map_err(|e| format!("failed to open governance database: {e}"))?;
    Ok(db.path().to_path_buf())
}

/// Lists the `scope = 'agent'` rules targeting the Apollia Chat system agent.
///
/// # Errors
///
/// Returns a Tauri-serializable error if `governance.db` cannot be opened or
/// queried.
#[tauri::command]
pub async fn list_chat_permission_rules(
    _state: State<'_, RuntimeHandle>,
) -> Result<Vec<PermissionRuleDto>, String> {
    let db_path = ensure_governance_db()?;
    let engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open prefix rule engine: {e}"))?;
    let rules = engine
        .list_rules_for_agent(APOLLIA_CHAT_AGENT_ID)
        .map_err(|e| format!("failed to list chat permission rules: {e}"))?;
    Ok(rules
        .iter()
        .map(super::tool_governance::rule_to_dto_pub)
        .collect())
}

/// Frontend DTO for an in-memory `scope = 'session'` authorization.
///
/// These authorizations live in `ChatSessionManager.sessions[].authorized_tools`,
/// never persisted in `governance.db`. They disappear when the session closes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAuthorizationDto {
    /// Unique session identifier.
    pub session_id: String,
    /// Session title (empty for untitled sessions).
    pub session_title: Option<String>,
    /// Session mode (`"libre"` | `"agent"` | `"companion"`).
    pub mode: String,
    /// Name of the auto-allowed tool.
    pub tool_name: String,
}

impl From<SessionAuthorizationView> for SessionAuthorizationDto {
    fn from(v: SessionAuthorizationView) -> Self {
        Self {
            session_id: v.session_id,
            session_title: v.session_title,
            mode: v.mode,
            tool_name: v.tool_name,
        }
    }
}

/// Lists the in-memory authorizations of all active sessions.
///
/// Lets `Settings > Permissions` display the `scope = 'session'`
/// authorizations that do not live in `governance.db`.
///
/// # Errors
///
/// Returns a serializable error if the chat subsystem is unavailable.
#[tauri::command]
pub async fn list_active_chat_session_authorizations(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<SessionAuthorizationDto>, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    let entries = manager.list_session_authorizations().await;
    Ok(entries
        .into_iter()
        .map(SessionAuthorizationDto::from)
        .collect())
}

/// Removes a `scope = 'session'` authorization from an active session.
///
/// # Errors
///
/// - Error if the session is unknown.
/// - Error if the chat subsystem is unavailable.
#[tauri::command]
pub async fn revoke_chat_session_authorization(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    tool_name: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    let removed = manager
        .revoke_session_authorization(session_id.clone(), tool_name.clone())
        .await
        .map_err(|e| e.to_string())?;
    if !removed {
        return Err(format!(
            "session authorization not found: session={session_id} tool={tool_name}"
        ));
    }
    tracing::info!(session_id, tool_name, "chat.session.authorization.revoked");
    Ok(())
}

/// Deletes a `scope = 'agent'` rule (reuses `PrefixRuleEngine`'s delete). The
/// identifier must belong to an agent rule; safety is enforced by the frontend
/// filter, but the deletion runs without extra checks on the Rust side, which
/// is consistent with `governance_revoke_permission_rule` (same guarantees).
///
/// # Errors
///
/// - `governance.db` missing / unreadable;
/// - unknown identifier (returns a descriptive error).
#[tauri::command]
pub async fn delete_chat_permission_rule(
    _state: State<'_, RuntimeHandle>,
    rule_id: i64,
) -> Result<(), String> {
    let db_path = ensure_governance_db()?;
    let mut engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open prefix rule engine: {e}"))?;
    let removed = engine
        .remove_rule_checked(rule_id)
        .map_err(|e| format!("failed to remove chat permission rule: {e}"))?;
    if !removed {
        return Err(format!("chat permission rule {rule_id} not found"));
    }
    tracing::info!(rule_id, "chat.permission.rule.revoked");
    Ok(())
}
