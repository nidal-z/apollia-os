//! Commandes IPC Tauri pour la gouvernance du chat libre Apollia.
//!
//! Couvre :
//! - lecture/écriture de la config persistée du chat libre
//!   (`chat_libre_config` dans `governance.db`) ;
//! - liste/suppression des règles `scope = 'agent'` créées par le bouton
//!   "Toujours autoriser" du chat (`agent_id = "apollia:chat"`).
//!
//! L'agent système Apollia Chat est identifié par
//! [`APOLLIA_CHAT_AGENT_ID`].

use std::path::PathBuf;

use apollia_permissions::PrefixRuleEngine;
use apollia_runtime::chat::SessionAuthorizationView;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::chat_libre_config::{ChatLibreConfig, ChatLibreConfigRepository};
use apollia_tools::{GovernanceDb, GOVERNANCE_DB_FILENAME};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::tool_governance::PermissionRuleDto;

/// Identifiant logique de l'agent système Apollia Chat.
///
/// Doit rester aligné avec
/// `apollia_runtime::chat::APOLLIA_CHAT_AGENT_ID`.
pub const APOLLIA_CHAT_AGENT_ID: &str = "apollia:chat";

/// DTO frontend pour [`ChatLibreConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatLibreConfigDto {
    /// System prompt par défaut. Vide ⇒ pas d'override.
    #[serde(default)]
    pub system_prompt: String,
    /// Outils auto-autorisés par défaut. Vide ⇒ pas d'override.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Backend LLM préféré. `None` ⇒ défaut runtime.
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

/// Ouvre (et migre si besoin) `governance.db` puis retourne son chemin.
fn ensure_governance_db() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME variable not set: {e}"))?;
    let base = PathBuf::from(home).join(".apollia");
    let db = GovernanceDb::open(&base)
        .map_err(|e| format!("failed to open governance database: {e}"))?;
    Ok(db.path().to_path_buf())
}

/// Lit la configuration persistée du chat libre.
///
/// # Errors
///
/// Retourne une erreur sérialisable Tauri si `governance.db` ne peut pas
/// être ouvert ou lu.
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

/// Persiste la configuration du chat libre (UPSERT sur la ligne unique id=1).
///
/// # Errors
///
/// Retourne une erreur sérialisable Tauri si `governance.db` ne peut pas
/// être ouvert ou si l'écriture échoue.
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
        "chat_libre_config updated"
    );
    Ok(())
}

/// Liste les règles `scope = 'agent'` ciblant l'agent système Apollia Chat.
///
/// # Errors
///
/// Retourne une erreur sérialisable Tauri si `governance.db` ne peut pas
/// être ouvert ou requêté.
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

/// DTO frontend pour une autorisation in-memory `scope = 'session'`.
///
/// Ces autorisations vivent dans `ChatSessionManager.sessions[].authorized_tools`
/// — jamais persistées dans `governance.db`. Elles disparaissent à la fermeture
/// de la session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAuthorizationDto {
    /// Identifiant unique de la session.
    pub session_id: String,
    /// Titre de la session (vide pour les sessions sans titre).
    pub session_title: Option<String>,
    /// Mode de la session (`"libre"` | `"agent"` | `"companion"`).
    pub mode: String,
    /// Nom de l'outil auto-autorisé.
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

/// Liste les autorisations in-memory de toutes les sessions actives.
///
/// Permet à `Settings > Permissions` d'afficher les autorisations
/// `scope = 'session'` qui ne vivent pas dans `governance.db`.
///
/// # Errors
///
/// Renvoie une erreur sérialisable si le sous-système chat n'est pas
/// disponible.
#[tauri::command]
pub async fn list_active_chat_session_authorizations(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<SessionAuthorizationDto>, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    let entries = manager.list_session_authorizations().await;
    Ok(entries.into_iter().map(SessionAuthorizationDto::from).collect())
}

/// Retire une autorisation `scope = 'session'` d'une session active.
///
/// # Errors
///
/// - Erreur si la session est inconnue.
/// - Erreur si le sous-système chat n'est pas disponible.
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
    tracing::info!(
        session_id,
        tool_name,
        "in-memory chat session authorization revoked"
    );
    Ok(())
}

/// Supprime une règle `scope = 'agent'` (réutilise le delete de
/// `PrefixRuleEngine`). L'identifiant doit appartenir à une règle d'agent —
/// la sécurité est assurée par le filtre frontend, mais la suppression est
/// exécutée sans contrôle supplémentaire côté Rust : c'est cohérent avec
/// `governance_revoke_permission_rule` (mêmes garanties).
///
/// # Errors
///
/// - `governance.db` introuvable / illisible ;
/// - identifiant inconnu (retourne une erreur descriptive).
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
    tracing::info!(rule_id, "chat permission rule revoked");
    Ok(())
}
