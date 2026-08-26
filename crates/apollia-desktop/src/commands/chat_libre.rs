//! Tauri IPC commands for governing the Apollia free chat.
//!
//! Covers:
//! - read/write of the free chat's persisted config
//!   (`chat_libre_config` in `governance.db`);
//! - list/delete of the `scope = 'agent'` rules created by the chat's
//!   "Always allow" button (`agent_id = "apollia:chat"`).
//!
//! The Apollia Chat system agent is identified by [`APOLLIA_CHAT_AGENT_ID`].

use std::path::PathBuf;

use apollia_permissions::PrefixRuleEngine;
use apollia_runtime::chat::SessionAuthorizationView;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::chat_libre_config::{ChatLibreConfig, ChatLibreConfigRepository};
use apollia_tools::GovernanceDb;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::tool_governance::PermissionRuleDto;

/// Logical identifier of the Apollia Chat system agent.
///
/// Must stay aligned with `apollia_runtime::chat::APOLLIA_CHAT_AGENT_ID`.
pub const APOLLIA_CHAT_AGENT_ID: &str = "apollia:chat";

/// Frontend DTO for [`ChatLibreConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatLibreConfigDto {
    /// Default system prompt. Empty means no override.
    #[serde(default)]
    pub system_prompt: String,
    /// Tools auto-allowed by default. Empty means no override.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Preferred LLM backend. `None` means runtime default.
    #[serde(default)]
    pub llm_backend: Option<String>,
}

impl From<ChatLibreConfig> for ChatLibreConfigDto {
    fn from(c: ChatLibreConfig) -> Self {
        Self {
            system_prompt: c.system_prompt,
            allowed_tools: c.allowed_tools,
            llm_backend: c.llm_backend,
        }
    }
}

impl From<ChatLibreConfigDto> for ChatLibreConfig {
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

/// Reads the free chat's persisted configuration.
///
/// # Errors
///
/// Returns a Tauri-serializable error if `governance.db` cannot be opened or
/// read.
#[tauri::command]
pub async fn get_chat_libre_config(
    _state: State<'_, RuntimeHandle>,
) -> Result<ChatLibreConfigDto, String> {
    let db_path = ensure_governance_db()?;
    let repo = ChatLibreConfigRepository::open(&db_path)
        .map_err(|e| format!("failed to open chat_libre_config: {e}"))?;
    let cfg = repo
        .load()
        .map_err(|e| format!("failed to load chat_libre_config: {e}"))?;
    Ok(cfg.into())
}

/// Persists the free chat's configuration (UPSERT on the single id=1 row).
///
/// # Errors
///
/// Returns a Tauri-serializable error if `governance.db` cannot be opened or
/// if the write fails.
#[tauri::command]
pub async fn update_chat_libre_config(
    _state: State<'_, RuntimeHandle>,
    config: ChatLibreConfigDto,
) -> Result<(), String> {
    let db_path = ensure_governance_db()?;
    let repo = ChatLibreConfigRepository::open(&db_path)
        .map_err(|e| format!("failed to open chat_libre_config: {e}"))?;
    let cfg: ChatLibreConfig = config.into();
    repo.save(&cfg)
        .map_err(|e| format!("failed to save chat_libre_config: {e}"))?;
    tracing::info!(
        prompt_len = cfg.system_prompt.len(),
        allowed_tools = cfg.allowed_tools.len(),
        llm_backend = ?cfg.llm_backend,
        "chat.config.updated"
    );
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
