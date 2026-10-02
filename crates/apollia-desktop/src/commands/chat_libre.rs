//! Tauri IPC commands for governing the Apollia free chat.
//!
//! Covers:
//! - read/write of the settings of the free chat, which are those of the
//!   Apollia agent: its mission and access overrides and its model, reached
//!   through the runtime's routes so every write is journaled, in the shape
//!   the chat settings panel shows them;
//! - list/delete of the `scope = 'agent'` rules created by the chat's
//!   "Always allow" button (`agent_id = "apollia:chat"`).
//!
//! The Apollia Chat system agent is identified by [`APOLLIA_CHAT_AGENT_ID`].

use std::path::PathBuf;

use apollia_permissions::PrefixRuleEngine;
use apollia_runtime::agents::apollia::{self, ChatConfigView, ConfigWrite};
use apollia_runtime::chat::SessionAuthorizationView;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::GovernanceDb;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::tool_governance::PermissionRuleDto;
use crate::attached::RuntimeLink;

/// Logical identifier of the Apollia Chat system agent.
///
/// Must stay aligned with `apollia_runtime::chat::APOLLIA_CHAT_AGENT_ID`.
pub const APOLLIA_CHAT_AGENT_ID: &str = "apollia:chat";

/// Frontend DTO of the chat settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatLibreConfigDto {
    /// Mission of Apollia. Empty means no override.
    #[serde(default)]
    pub system_prompt: String,
    /// Accesses of Apollia. Empty means no override.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Model of Apollia. `None` means runtime default.
    #[serde(default)]
    pub llm_backend: Option<String>,
}

impl From<ChatConfigView> for ChatLibreConfigDto {
    fn from(c: ChatConfigView) -> Self {
        Self {
            system_prompt: c.system_prompt,
            allowed_tools: c.allowed_tools,
            llm_backend: c.llm_backend,
        }
    }
}

impl From<ChatLibreConfigDto> for ChatConfigView {
    fn from(c: ChatLibreConfigDto) -> Self {
        Self {
            system_prompt: c.system_prompt,
            allowed_tools: c.allowed_tools,
            llm_backend: c.llm_backend,
        }
    }
}

/// Opens (and migrates if needed) `governance.db`, then returns its path.
fn ensure_governance_db() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()?;
    let base = apollia_core::paths::data_dir_under(home);
    let db = GovernanceDb::open(&base)
        .map_err(|e| format!("failed to open governance database: {e}"))?;
    Ok(db.path().to_path_buf())
}

/// The settings of the chat as the runtime holds them now.
async fn current_config(link: &RuntimeLink) -> Result<ChatConfigView, String> {
    let resolved = link.get_json(&apollia::definition_path()).await?;
    Ok(ChatConfigView::from_resolved(&resolved))
}

/// Reads the settings of the free chat: the mission, the accesses and the
/// model of the Apollia agent.
///
/// # Errors
///
/// Returns a Tauri-serializable error if the runtime cannot be reached or
/// does not hold the Apollia agent.
#[tauri::command]
pub async fn get_chat_libre_config(
    link: State<'_, RuntimeLink>,
) -> Result<ChatLibreConfigDto, String> {
    Ok(current_config(link.inner()).await?.into())
}

/// Writes the settings of the free chat as overrides and the model of the
/// Apollia agent. Only the fields that changed are written.
///
/// # Errors
///
/// Returns a Tauri-serializable error if the runtime cannot be reached or
/// refuses a write.
#[tauri::command]
pub async fn update_chat_libre_config(
    link: State<'_, RuntimeLink>,
    config: ChatLibreConfigDto,
) -> Result<(), String> {
    let link = link.inner();
    let current = current_config(link).await?;
    let writes = apollia::changes(&current, &config.into());
    for write in &writes {
        apply_write(link, write).await?;
    }
    tracing::event!(
        tracing::Level::INFO,
        writes = writes.len(),
        "chat.config.updated"
    );
    Ok(())
}

/// Runs one write through the runtime's routes.
async fn apply_write(link: &RuntimeLink, write: &ConfigWrite) -> Result<(), String> {
    match write {
        ConfigWrite::SetMission(_) | ConfigWrite::SetAccess(_) => {
            let body = write.override_body().unwrap_or_default();
            link.put_json(&write.path(), &body).await?;
        }
        ConfigWrite::ClearMission | ConfigWrite::ClearAccess | ConfigWrite::ClearModel => {
            link.delete_json(&write.path()).await?;
        }
        ConfigWrite::SetModel(backend) => {
            let provider = link
                .get_json(&apollia::backend_path(backend))
                .await
                .ok()
                .and_then(|b| {
                    b.get("provider")
                        .and_then(|p| p.as_str())
                        .map(str::to_owned)
                });
            let body = apollia::model_body(backend, provider.as_deref());
            link.put_json(&write.path(), &body).await?;
        }
    }
    Ok(())
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
