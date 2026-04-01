//! Registre des backends LLM persisté dans `system.db`.
//!
//! [`LlmBackendRepository`] est un wrapper synchrone autour de `rusqlite` —
//! même pattern que `PlanRepository`. Toutes les méthodes sont synchrones ;
//! les acteurs Tokio les appellent via `spawn_blocking` si nécessaire.
//!
//! La migration est embarquée et appliquée idempotentiellement à l'ouverture.

use std::cell::RefCell;
use std::path::Path;

use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ────────────────────────────────────────────────────────────────────────────
// Migration
// ────────────────────────────────────────────────────────────────────────────

const MIGRATION_SQL: &str = "
CREATE TABLE IF NOT EXISTS llm_backends (
    name         TEXT PRIMARY KEY,
    provider     TEXT NOT NULL,
    model        TEXT NOT NULL,
    config_json  TEXT NOT NULL DEFAULT '{}',
    enabled      INTEGER NOT NULL DEFAULT 1,
    is_default   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (provider IN ('llama-cpp', 'openai', 'mistral', 'anthropic', 'ollama'))
);
";

// ────────────────────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────────────────────

/// Configuration d'un backend LLM enregistré dans `system.db`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendConfig {
    /// Nom unique (ex. `"local-code"`, `"mistral-small"`). Pattern : `[a-z0-9_-]+`.
    pub name: String,
    /// Fournisseur LLM.
    pub provider: LlmProvider,
    /// Nom du modèle ou chemin GGUF absolu.
    pub model: String,
    /// Paramètres provider-spécifiques (JSON). Peut contenir `"${VAR}"` pour les secrets.
    pub config_json: serde_json::Value,
    /// Si `false`, le backend n'est pas chargé au démarrage.
    pub enabled: bool,
    /// Si `true`, utilisé par les agents sans champ `llm_backend` explicite.
    pub is_default: bool,
}

/// Fournisseur LLM supporté.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LlmProvider {
    /// Backend llama.cpp embarqué (GGUF local).
    LlamaCpp,
    /// API OpenAI ou compatible OpenAI (LM Studio, vLLM).
    OpenAi,
    /// API Mistral AI.
    Mistral,
    /// API Anthropic.
    Anthropic,
    /// Ollama local.
    Ollama,
}

impl std::fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LlmProvider::LlamaCpp => "llama-cpp",
            LlmProvider::OpenAi => "openai",
            LlmProvider::Mistral => "mistral",
            LlmProvider::Anthropic => "anthropic",
            LlmProvider::Ollama => "ollama",
        };
        f.write_str(s)
    }
}

impl TryFrom<&str> for LlmProvider {
    type Error = LlmBackendError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "llama-cpp" => Ok(LlmProvider::LlamaCpp),
            "openai" => Ok(LlmProvider::OpenAi),
            "mistral" => Ok(LlmProvider::Mistral),
            "anthropic" => Ok(LlmProvider::Anthropic),
            "ollama" => Ok(LlmProvider::Ollama),
            other => Err(LlmBackendError::Serialization(format!(
                "unknown provider: {other}"
            ))),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Erreurs
// ────────────────────────────────────────────────────────────────────────────

/// Erreurs possibles du [`LlmBackendRepository`].
#[derive(Debug, Error)]
pub enum LlmBackendError {
    /// Aucun backend trouvé pour ce nom.
    #[error("backend '{0}' not found")]
    NotFound(String),

    /// Un backend par défaut existe déjà.
    #[error("a default backend already exists: '{0}'")]
    DefaultAlreadyExists(String),

    /// Suppression du backend par défaut refusée.
    #[error("cannot delete the default backend — set another default first")]
    CannotDeleteDefault,

    /// Nom de backend invalide (doit correspondre à `[a-z0-9_-]+`).
    #[error("invalid backend name '{0}': only [a-z0-9_-] allowed")]
    InvalidName(String),

    /// Erreur SQLite sous-jacente.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// Erreur de sérialisation JSON.
    #[error("serialization error: {0}")]
    Serialization(String),
}

// ────────────────────────────────────────────────────────────────────────────
// Validation
// ────────────────────────────────────────────────────────────────────────────

fn validate_name(name: &str) -> Result<(), LlmBackendError> {
    // Pattern évalué une seule fois grâce au `Regex::new` — pas de `OnceLock` nécessaire
    // car la validation est appelée peu fréquemment (opérations d'écriture uniquement).
    let re = Regex::new(r"^[a-z0-9_-]+$").expect("static pattern is valid");
    if name.is_empty() || !re.is_match(name) {
        return Err(LlmBackendError::InvalidName(name.to_string()));
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Repository
// ────────────────────────────────────────────────────────────────────────────

/// Repository CRUD pour les backends LLM persistés dans `system.db`.
///
/// Encapsule une connexion `rusqlite` derrière un [`RefCell`] afin d'offrir
/// une API `&self` sur toutes les méthodes tout en autorisant l'emprunt
/// mutable nécessaire aux transactions atomiques.
///
/// **Thread safety :** `LlmBackendRepository` n'est pas `Send` (car `RefCell`).
/// Il doit être créé et utilisé dans le même thread, ou passé à `spawn_blocking`.
pub struct LlmBackendRepository {
    conn: RefCell<Connection>,
}

impl LlmBackendRepository {
    /// Ouvre (ou crée) `system.db` au chemin donné et applique la migration.
    ///
    /// La migration est idempotente (`CREATE TABLE IF NOT EXISTS`), donc sûre
    /// à réexécuter sur une base existante.
    ///
    /// # Errors
    /// Retourne [`LlmBackendError::Db`] si l'ouverture ou la migration échoue.
    pub fn open(path: &Path) -> Result<Self, LlmBackendError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    /// Crée ou met à jour un backend. Valide le nom avant insertion.
    ///
    /// Si `config.is_default` est `true`, tous les autres backends sont d'abord
    /// démarcés (`is_default = 0`) dans la même transaction.
    ///
    /// # Errors
    /// - [`LlmBackendError::InvalidName`] si le nom ne respecte pas `[a-z0-9_-]+`.
    /// - [`LlmBackendError::Db`] pour toute erreur SQLite.
    /// - [`LlmBackendError::Serialization`] si `config_json` ne peut pas être sérialisé.
    pub fn save(&self, config: &LlmBackendConfig) -> Result<(), LlmBackendError> {
        validate_name(&config.name)?;

        let provider_str = config.provider.to_string();
        let config_json_str = serde_json::to_string(&config.config_json)
            .map_err(|e| LlmBackendError::Serialization(e.to_string()))?;

        let upsert_sql = "
            INSERT INTO llm_backends (name, provider, model, config_json, enabled, is_default)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(name) DO UPDATE SET
                provider    = excluded.provider,
                model       = excluded.model,
                config_json = excluded.config_json,
                enabled     = excluded.enabled,
                is_default  = excluded.is_default,
                updated_at  = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        ";

        if config.is_default {
            let mut conn = self.conn.borrow_mut();
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE llm_backends SET is_default = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                 WHERE is_default = 1 AND name != ?1",
                params![config.name],
            )?;
            tx.execute(
                upsert_sql,
                params![
                    config.name,
                    provider_str,
                    config.model,
                    config_json_str,
                    config.enabled as i32,
                    config.is_default as i32,
                ],
            )?;
            tx.commit()?;
        } else {
            self.conn.borrow().execute(
                upsert_sql,
                params![
                    config.name,
                    provider_str,
                    config.model,
                    config_json_str,
                    config.enabled as i32,
                    config.is_default as i32,
                ],
            )?;
        }

        Ok(())
    }

    /// Retourne tous les backends (actifs et inactifs), triés par nom.
    ///
    /// # Errors
    /// Retourne [`LlmBackendError::Db`] ou [`LlmBackendError::Serialization`].
    pub fn list(&self) -> Result<Vec<LlmBackendConfig>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT name, provider, model, config_json, enabled, is_default \
             FROM llm_backends \
             ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })?;

        let mut configs = Vec::new();
        for row in rows {
            let (name, provider_str, model, config_json_str, enabled, is_default) = row?;
            configs.push(row_to_config(
                name,
                &provider_str,
                model,
                &config_json_str,
                enabled != 0,
                is_default != 0,
            )?);
        }
        Ok(configs)
    }

    /// Trouve un backend par nom exact. Retourne `None` si introuvable.
    ///
    /// # Errors
    /// Retourne [`LlmBackendError::Db`] ou [`LlmBackendError::Serialization`].
    pub fn find_by_name(&self, name: &str) -> Result<Option<LlmBackendConfig>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT name, provider, model, config_json, enabled, is_default \
             FROM llm_backends \
             WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })?;

        match rows.next() {
            None => Ok(None),
            Some(row) => {
                let (name, provider_str, model, config_json_str, enabled, is_default) = row?;
                Ok(Some(row_to_config(
                    name,
                    &provider_str,
                    model,
                    &config_json_str,
                    enabled != 0,
                    is_default != 0,
                )?))
            }
        }
    }

    /// Retourne le backend marqué `is_default = true`, ou `None` si aucun.
    ///
    /// # Errors
    /// Retourne [`LlmBackendError::Db`] ou [`LlmBackendError::Serialization`].
    pub fn find_default(&self) -> Result<Option<LlmBackendConfig>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT name, provider, model, config_json, enabled, is_default \
             FROM llm_backends \
             WHERE is_default = 1 \
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })?;

        match rows.next() {
            None => Ok(None),
            Some(row) => {
                let (name, provider_str, model, config_json_str, enabled, is_default) = row?;
                Ok(Some(row_to_config(
                    name,
                    &provider_str,
                    model,
                    &config_json_str,
                    enabled != 0,
                    is_default != 0,
                )?))
            }
        }
    }

    /// Marque `name` comme backend par défaut.
    ///
    /// L'ancien défaut (s'il existe) est démarcé atomiquement dans la même transaction.
    ///
    /// # Errors
    /// - [`LlmBackendError::NotFound`] si `name` n'existe pas dans la DB.
    /// - [`LlmBackendError::Db`] pour toute erreur SQLite.
    pub fn set_default(&self, name: &str) -> Result<(), LlmBackendError> {
        // find_by_name emprunte conn puis relâche avant borrow_mut ci-dessous.
        if self.find_by_name(name)?.is_none() {
            return Err(LlmBackendError::NotFound(name.to_string()));
        }

        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE llm_backends \
             SET is_default = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE is_default = 1",
            [],
        )?;
        tx.execute(
            "UPDATE llm_backends \
             SET is_default = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE name = ?1",
            params![name],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Supprime un backend.
    ///
    /// # Errors
    /// - [`LlmBackendError::NotFound`] si `name` est absent.
    /// - [`LlmBackendError::CannotDeleteDefault`] si `name` est le backend par défaut.
    /// - [`LlmBackendError::Db`] pour toute erreur SQLite.
    pub fn delete(&self, name: &str) -> Result<(), LlmBackendError> {
        // find_by_name emprunte conn puis relâche avant le borrow ci-dessous.
        match self.find_by_name(name)? {
            None => return Err(LlmBackendError::NotFound(name.to_string())),
            Some(b) if b.is_default => return Err(LlmBackendError::CannotDeleteDefault),
            _ => {}
        }
        self.conn
            .borrow()
            .execute("DELETE FROM llm_backends WHERE name = ?1", params![name])?;
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

/// Construit un [`LlmBackendConfig`] depuis les valeurs brutes d'une ligne SQLite.
fn row_to_config(
    name: String,
    provider_str: &str,
    model: String,
    config_json_str: &str,
    enabled: bool,
    is_default: bool,
) -> Result<LlmBackendConfig, LlmBackendError> {
    let provider = LlmProvider::try_from(provider_str)?;
    let config_json: serde_json::Value = serde_json::from_str(config_json_str)
        .map_err(|e| LlmBackendError::Serialization(e.to_string()))?;
    Ok(LlmBackendConfig {
        name,
        provider,
        model,
        config_json,
        enabled,
        is_default,
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_repo() -> (LlmBackendRepository, TempDir) {
        let dir = TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
        (repo, dir)
    }

    fn make_config(name: &str, is_default: bool) -> LlmBackendConfig {
        LlmBackendConfig {
            name: name.to_string(),
            provider: LlmProvider::OpenAi,
            model: "gpt-4o-mini".to_string(),
            config_json: serde_json::json!({ "api_key": "${OPENAI_KEY}" }),
            enabled: true,
            is_default,
        }
    }

    // GIVEN un repository vide
    // WHEN  save() + list()
    // THEN  la liste contient le backend sauvegardé
    #[test]
    fn test_ac1_save_and_list() {
        let (repo, _dir) = make_repo();
        let config = make_config("openai", false);

        repo.save(&config).unwrap();
        let list = repo.list().unwrap();

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "openai");
        assert_eq!(list[0].provider, LlmProvider::OpenAi);
        assert_eq!(list[0].model, "gpt-4o-mini");
        assert!(!list[0].is_default);
    }

    // GIVEN deux backends, "a" est le défaut
    // WHEN  set_default("b")
    // THEN  exactement 1 backend avec is_default=true, c'est "b"
    #[test]
    fn test_ac2_set_default_replaces_previous() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", true)).unwrap();
        repo.save(&make_config("b", false)).unwrap();

        repo.set_default("b").unwrap();

        let defaults: Vec<_> = repo
            .list()
            .unwrap()
            .into_iter()
            .filter(|c| c.is_default)
            .collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "b");
    }

    // GIVEN un backend "a" marqué is_default=true
    // WHEN  delete("a")
    // THEN  LlmBackendError::CannotDeleteDefault retourné, "a" toujours présent
    #[test]
    fn test_ac3_cannot_delete_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", true)).unwrap();

        assert!(matches!(
            repo.delete("a"),
            Err(LlmBackendError::CannotDeleteDefault)
        ));
        assert!(repo.find_by_name("a").unwrap().is_some());
    }

    // GIVEN un repository vide
    // WHEN  find_by_name("inexistant")
    // THEN  Ok(None) retourné
    #[test]
    fn test_ac4_find_by_name_missing_returns_none() {
        let (repo, _dir) = make_repo();
        assert!(repo.find_by_name("ghost").unwrap().is_none());
    }

    // GIVEN backends présents mais aucun is_default=true
    // WHEN  find_default()
    // THEN  Ok(None) retourné
    #[test]
    fn test_ac5_find_default_none_when_no_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", false)).unwrap();
        assert!(repo.find_default().unwrap().is_none());
    }

    // GIVEN un backend non-défaut
    // WHEN  delete()
    // THEN  backend supprimé, list() retourne vide
    #[test]
    fn test_delete_non_default_succeeds() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", false)).unwrap();

        repo.delete("a").unwrap();

        assert!(repo.list().unwrap().is_empty());
    }

    // GIVEN un backend inexistant
    // WHEN  delete()
    // THEN  LlmBackendError::NotFound retourné
    #[test]
    fn test_delete_not_found() {
        let (repo, _dir) = make_repo();
        assert!(matches!(
            repo.delete("ghost"),
            Err(LlmBackendError::NotFound(_))
        ));
    }

    // GIVEN un backend inexistant
    // WHEN  set_default()
    // THEN  LlmBackendError::NotFound retourné
    #[test]
    fn test_set_default_not_found() {
        let (repo, _dir) = make_repo();
        assert!(matches!(
            repo.set_default("ghost"),
            Err(LlmBackendError::NotFound(_))
        ));
    }

    // GIVEN un backend existant
    // WHEN  save() avec un nom invalide (majuscules)
    // THEN  LlmBackendError::InvalidName retourné
    #[test]
    fn test_invalid_name_rejected() {
        let (repo, _dir) = make_repo();
        let config = make_config("MyBackend", false);
        assert!(matches!(
            repo.save(&config),
            Err(LlmBackendError::InvalidName(_))
        ));
    }

    // GIVEN un backend sauvegardé
    // WHEN  find_default() après save(is_default=true)
    // THEN  le défaut est retourné
    #[test]
    fn test_find_default_returns_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("main", true)).unwrap();
        let default = repo.find_default().unwrap();
        assert!(default.is_some());
        assert_eq!(default.unwrap().name, "main");
    }

    // GIVEN deux backends avec is_default=true successifs
    // WHEN  save() du second (is_default=true)
    // THEN  un seul défaut dans la liste
    #[test]
    fn test_save_second_default_replaces_first() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("first", true)).unwrap();
        repo.save(&make_config("second", true)).unwrap();

        let defaults: Vec<_> = repo
            .list()
            .unwrap()
            .into_iter()
            .filter(|c| c.is_default)
            .collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "second");
    }
}
