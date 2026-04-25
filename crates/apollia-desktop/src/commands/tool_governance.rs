//! Commandes IPC Tauri pour la gouvernance des outils natifs et des
//! permissions du frontend desktop.
//!
//! Ces commandes pilotent directement la base consolidée
//! `~/.apollia/governance.db` via les composants `ToolRegistry`,
//! `ToolCredentialStore`, `PrefixRuleEngine` et `PermissionAuditLog` exposés
//! par les crates `apollia-tools` et `apollia-permissions`.
//!
//! Les règles de permission *session* vivent uniquement en mémoire dans le
//! processus desktop : elles sont persistées dans un store statique partagé
//! entre [`crate::commands::hitl::add_permission_prefix_rule`] et
//! [`list_permission_rules`] / [`revoke_permission_rule`] /
//! [`revoke_all_rules`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};
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

/// État d'un outil natif tel qu'affiché par la page `/settings/tools`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStatusDto {
    /// Nom canonique de l'outil (ex. `bash_executor`).
    pub name: String,
    /// `true` lorsque l'outil est actif. Voir [`apollia_tools::NativeToolRegistry`].
    pub enabled: bool,
    /// Configuration JSON spécifique à l'outil, ou `null`.
    pub config: Option<serde_json::Value>,
    /// Noms des credentials configurées pour cet outil (les valeurs ne sont
    /// jamais retournées au frontend).
    pub credential_keys: Vec<String>,
    /// Backend effectivement utilisé par l'outil quand applicable
    /// (ex. `"duckduckgo"` ou `"brave"` pour `web_search`).
    pub active_backend: Option<String>,
}

/// Métadonnées d'une credential stockée pour un outil.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialEntryDto {
    /// Nom de l'outil propriétaire.
    pub tool_name: String,
    /// Nom logique de la clé (ex. `brave.api_key`).
    pub key_name: String,
    /// Date de création au format ISO 8601 / RFC 3339.
    pub created_at: String,
    /// Date de la dernière utilisation effective, le cas échéant.
    pub last_used_at: Option<String>,
}

/// Résultat d'une validation live de credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialTestResultDto {
    /// `true` si l'appel a réussi.
    pub ok: bool,
    /// Latence mesurée en millisecondes (round-trip).
    pub latency_ms: Option<u64>,
    /// Message d'erreur diagnostique quand `ok` est `false`.
    pub error: Option<String>,
}

/// Filtre passé à [`list_permission_rules`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PermissionRuleFilter {
    /// Portée à filtrer : `"session"`, `"project"` ou `"global"`.
    pub scope: Option<String>,
    /// Nom d'outil à filtrer.
    pub tool_name: Option<String>,
}

/// Représentation frontend d'une règle de permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleDto {
    /// Identifiant. Positif pour les règles persistées (DB), négatif pour
    /// les règles de session (mémoire).
    pub id: i64,
    /// Nom de l'outil ciblé.
    pub tool_name: String,
    /// Préfixe d'argument optionnel.
    pub arg_prefix: Option<String>,
    /// Action appliquée (`"allow"` ou `"deny"`).
    pub action: String,
    /// Portée de la règle (`"session"`, `"project"`, `"global"`).
    pub scope: String,
    /// Chemin canonique du projet, pour les règles `project`.
    pub project_path: Option<String>,
    /// Date d'expiration ISO 8601, le cas échéant.
    pub expires_at: Option<String>,
    /// Date de création ISO 8601.
    pub created_at: String,
    /// Auteur de la règle (`None` ⇒ opérateur humain).
    pub created_by: Option<String>,
}

/// Entrée du log d'audit immuable des décisions de permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntryDto {
    /// Identifiant auto-incrémenté.
    pub id: i64,
    /// Nom de l'outil invoqué.
    pub tool_name: String,
    /// Premier argument extrait de l'invocation, lorsque disponible.
    pub first_arg: Option<String>,
    /// Décision sérialisée (ex. `"AutoAllowedSafeList"`).
    pub decision: String,
    /// Portée de la règle ayant tranché, lorsque pertinent.
    pub scope: Option<String>,
    /// Identifiant de la règle ayant déclenché la décision, lorsque applicable.
    pub rule_id: Option<i64>,
    /// Nom de l'agent à l'origine de l'invocation, si renseigné.
    pub agent: Option<String>,
    /// Date de la décision au format ISO 8601.
    pub decided_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Session rules store (process-local, in-memory)
// ─────────────────────────────────────────────────────────────────────────────

/// Conteneur en mémoire d'une règle session, associé à un identifiant négatif
/// stable pour la durée du processus.
#[derive(Debug, Clone)]
pub(crate) struct SessionRule {
    pub id: i64,
    pub rule: PrefixRule,
}

fn session_store() -> &'static Mutex<Vec<SessionRule>> {
    static STORE: OnceLock<Mutex<Vec<SessionRule>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

fn next_session_id() -> i64 {
    static COUNTER: AtomicI64 = AtomicI64::new(0);
    -1 - COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Ajoute une règle dans le store de session du processus desktop.
///
/// L'id retourné est négatif et sert de référence stable pour
/// [`revoke_permission_rule`].
pub(crate) fn push_session_rule(mut rule: PrefixRule) -> Result<i64, String> {
    rule.scope = PermissionScope::Session;
    rule.project_path = None;
    let id = next_session_id();
    rule.id = id;
    session_store()
        .lock()
        .map_err(|_| "session rules mutex poisoned".to_string())?
        .push(SessionRule { id, rule });
    Ok(id)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers privés
// ─────────────────────────────────────────────────────────────────────────────

fn home_base_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME variable not set: {e}"))?;
    Ok(PathBuf::from(home).join(".apollia"))
}

/// Ouvre `governance.db` dans `~/.apollia/`, en effectuant la migration
/// initiale si nécessaire. Retourne le chemin du fichier.
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
        "global" => Ok(PermissionScope::Global),
        other => Err(format!(
            "unknown scope '{other}', expected 'session' | 'project' | 'global'"
        )),
    }
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
        expires_at: iso8601_opt(rule.expires_at),
        created_at: iso8601(rule.created_at),
        created_by: rule.created_by_agent.clone(),
    }
}

/// Normalise un `project_path` brut reçu du frontend : trim + canonicalisation
/// quand le chemin existe. Les chemins inexistants sont retournés tels quels
/// (utiles pour décrire un projet supprimé qu'on souhaite encore filtrer).
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

/// Persiste une règle scope-aware dans `governance.db`.
pub(crate) fn persist_scoped_rule(
    base_dir: &Path,
    tool_name: String,
    arg_prefix: Option<String>,
    action: RuleAction,
    scope: PermissionScope,
    project_path: Option<PathBuf>,
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
        ..PrefixRule::default()
    };

    engine
        .add_rule(&rule)
        .map_err(|e| format!("failed to persist prefix rule: {e}"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tools — list / enable / config
// ─────────────────────────────────────────────────────────────────────────────

/// Retourne l'état complet de chaque outil natif (activation, config,
/// credentials configurées, backend actif).
///
/// # Errors
///
/// Retourne une erreur sérialisable pour Tauri si la base de gouvernance ne
/// peut pas être ouverte ou lue.
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

/// Active ou désactive un outil natif via `ToolRegistry::set_enabled`.
///
/// # Errors
///
/// Retourne une erreur sérialisable si l'écriture en base échoue.
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
    tracing::info!(tool = %tool_name, enabled, "operator updated tool enabled flag");
    Ok(())
}

/// Lit la configuration JSON associée à un outil, ou `null`.
///
/// # Errors
///
/// Retourne une erreur sérialisable si la lecture échoue.
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

/// Stocke la configuration JSON d'un outil.
///
/// # Errors
///
/// Retourne une erreur sérialisable si l'écriture échoue.
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
    tracing::info!(tool = %tool_name, "operator updated tool config");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Credentials — list / set / delete / test
// ─────────────────────────────────────────────────────────────────────────────

/// Liste les credentials configurées, optionnellement filtrées par outil.
/// Les valeurs claires ne sont jamais retournées.
///
/// # Errors
///
/// Retourne une erreur sérialisable si l'accès à la base échoue.
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

/// Stocke (ou met à jour) une credential pour un outil.
///
/// La valeur claire est chiffrée AES-256-GCM côté Rust et n'est jamais
/// renvoyée au frontend.
///
/// # Errors
///
/// Retourne une erreur sérialisable si le chiffrement ou l'écriture échoue.
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
    tracing::info!(tool = %tool_name, key = %key_name, "credential stored");
    Ok(())
}

/// Supprime une credential identifiée par `(tool_name, key_name)`.
///
/// # Errors
///
/// Retourne une erreur sérialisable si la suppression échoue.
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
    tracing::info!(tool = %tool_name, key = %key_name, "credential deleted");
    Ok(())
}

/// Effectue un appel live pour valider une credential.
///
/// Aujourd'hui seul `web_search` (clé Brave) est testé : un GET
/// `https://api.search.brave.com/res/v1/web/search` est émis avec la clé en
/// header `X-Subscription-Token`. Toute autre combinaison `tool_name` retourne
/// une erreur explicite.
///
/// # Errors
///
/// Retourne une erreur sérialisable si la base ne peut être lue ou si le tool
/// ciblé ne dispose pas de routine de validation.
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
            tracing::warn!(error = %err, "failed to update last_used_at");
        }
    }

    Ok(outcome)
}

async fn test_brave_key(key: &str) -> CredentialTestResultDto {
    let client = match reqwest::Client::builder()
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
// Permission rules — list / revoke
// ─────────────────────────────────────────────────────────────────────────────

/// Liste les règles de permission selon `filter`.
///
/// Les règles `session` sont lues depuis le store mémoire du processus, les
/// règles `project` / `global` depuis `governance.db`.
///
/// # Errors
///
/// Retourne une erreur sérialisable si le scope est inconnu ou si la base ne
/// peut pas être lue.
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

    if scope_filter.is_none() || scope_filter == Some(PermissionScope::Session) {
        let session = session_store()
            .lock()
            .map_err(|_| "session rules mutex poisoned".to_string())?;
        for entry in session.iter() {
            if matches_tool(&entry.rule.tool_name, tool_filter) {
                out.push(rule_to_dto(&entry.rule));
            }
        }
    }

    if scope_filter != Some(PermissionScope::Session) {
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
    }

    Ok(out)
}

fn matches_tool(rule_tool: &str, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(t) => rule_tool == t,
    }
}

/// Supprime la règle identifiée par `rule_id`.
///
/// Les identifiants négatifs ciblent une règle session (mémoire), les
/// identifiants positifs une règle persistée.
///
/// # Errors
///
/// Retourne une erreur sérialisable si l'identifiant n'existe pas ou si la
/// suppression échoue.
#[tauri::command]
pub async fn governance_revoke_permission_rule(
    rule_id: i64,
    _state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    if rule_id < 0 {
        let mut session = session_store()
            .lock()
            .map_err(|_| "session rules mutex poisoned".to_string())?;
        let before = session.len();
        session.retain(|entry| entry.id != rule_id);
        if session.len() == before {
            return Err(format!("session rule {rule_id} not found"));
        }
        tracing::info!(rule_id, "session permission rule revoked");
        return Ok(());
    }

    let db_path = ensure_governance_db()?;
    let mut engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open prefix rule engine: {e}"))?;
    let removed = engine
        .remove_rule_checked(rule_id)
        .map_err(|e| format!("failed to remove permission rule: {e}"))?;
    if !removed {
        return Err(format!("permission rule {rule_id} not found"));
    }
    tracing::info!(rule_id, "persisted permission rule revoked");
    Ok(())
}

/// Supprime toutes les règles d'une portée donnée — `None` cible les trois.
///
/// Retourne le nombre total de règles supprimées (session + DB).
///
/// # Errors
///
/// Retourne une erreur sérialisable si le scope est inconnu ou si la base ne
/// peut pas être modifiée.
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

    if target_scope.is_none() || target_scope == Some(PermissionScope::Session) {
        let mut session = session_store()
            .lock()
            .map_err(|_| "session rules mutex poisoned".to_string())?;
        total = total.saturating_add(u32::try_from(session.len()).unwrap_or(u32::MAX));
        session.clear();
    }

    if target_scope != Some(PermissionScope::Session) {
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
    }

    tracing::info!(scope = ?scope, count = total, "permission rules revoked in bulk");
    Ok(total)
}

// ─────────────────────────────────────────────────────────────────────────────
// Audit log
// ─────────────────────────────────────────────────────────────────────────────

/// Liste les entrées du log d'audit, optionnellement filtrées par outil et
/// paginées. Les entrées sont triées par date décroissante.
///
/// # Errors
///
/// Retourne une erreur sérialisable si la lecture échoue.
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
    fn test_add_session_rule_not_persisted() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let _ = GovernanceDb::open(dir.path()).expect("init");
        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);

        // WHEN — ajout d'une règle session via le store mémoire
        let id = push_session_rule(PrefixRule {
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Allow,
            ..PrefixRule::default()
        })
        .expect("push session");

        // THEN — l'id est négatif et la DB ne contient aucune règle
        assert!(id < 0);
        let engine = PrefixRuleEngine::new(&db_path).expect("engine");
        let persisted = engine.list_rules().expect("list");
        assert!(persisted.is_empty(), "no rule must be persisted in DB");

        // Cleanup pour ne pas polluer les autres tests qui partagent le store statique
        session_store().lock().expect("lock").retain(|e| e.id != id);
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
        // GIVEN — une règle global et une règle project
        let dir = TempDir::new().expect("tempdir");
        let project = PathBuf::from("/home/user/projet-bar");
        persist_scoped_rule(
            dir.path(),
            "bash_executor".into(),
            None,
            RuleAction::Allow,
            PermissionScope::Global,
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
        )
        .expect("project");
        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
        assert_eq!(count_project_rules(&db_path), 1);

        // WHEN — on ne supprime que les règles globales
        let mut engine = PrefixRuleEngine::new(&db_path).expect("engine");
        let removed = engine
            .remove_rules_by_scope(PermissionScope::Global, None)
            .expect("remove globals");

        // THEN — il ne reste que la règle project
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
        assert_eq!(parse_scope("session"), Ok(PermissionScope::Session));
        assert_eq!(parse_scope("project"), Ok(PermissionScope::Project));
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
        // Inexistant → renvoyé tel quel.
        let res = canonical_project_path("/definitely/not/here").expect("ok");
        assert_eq!(res, PathBuf::from("/definitely/not/here"));
    }
}
