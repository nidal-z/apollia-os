//! Persistance de la configuration du chat libre dans `governance.db`.
//!
//! Singleton (id=1) regroupant le system_prompt par défaut, la liste des outils
//! autorisés sans HITL, et le backend LLM préféré pour les sessions Libre.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::governance_db::GovernanceError;

/// Configuration du chat libre persistée dans `governance.db`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatLibreConfig {
    /// System prompt par défaut injecté en tête de chaque session Libre.
    pub system_prompt: String,
    /// Liste des outils auto-autorisés (équivaut à des règles agent-scoped).
    pub allowed_tools: Vec<String>,
    /// Backend LLM préféré (`None` ⇒ défaut runtime).
    pub llm_backend: Option<String>,
}

/// Repository SQLite pour [`ChatLibreConfig`].
///
/// La table `chat_libre_config` est créée par la migration de [`crate::governance_db`].
/// Ce repository ne fait qu'ouvrir la connexion et lire/écrire la ligne unique (id=1).
pub struct ChatLibreConfigRepository {
    conn: Connection,
}

impl ChatLibreConfigRepository {
    /// Ouvre la base `governance.db` en lecture/écriture.
    ///
    /// # Errors
    ///
    /// Retourne [`GovernanceError::Database`] si l'ouverture échoue.
    pub fn open(db_path: &Path) -> Result<Self, GovernanceError> {
        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        Ok(Self { conn })
    }

    /// Lit la ligne id=1 de la table `chat_libre_config`.
    ///
    /// Si la table est vide (cas inattendu après migration), retourne la
    /// configuration par défaut pour préserver le fallback silencieux.
    ///
    /// # Errors
    ///
    /// Retourne [`GovernanceError::Database`] en cas d'erreur SQLite.
    pub fn load(&self) -> Result<ChatLibreConfig, GovernanceError> {
        let row = self.conn.query_row(
            "SELECT system_prompt, allowed_tools, llm_backend FROM chat_libre_config WHERE id = 1",
            [],
            |row| {
                let system_prompt: String = row.get(0)?;
                let allowed_tools_json: String = row.get(1)?;
                let llm_backend: Option<String> = row.get(2)?;
                Ok((system_prompt, allowed_tools_json, llm_backend))
            },
        );

        match row {
            Ok((system_prompt, allowed_tools_json, llm_backend)) => {
                let allowed_tools: Vec<String> =
                    serde_json::from_str(&allowed_tools_json).unwrap_or_default();
                Ok(ChatLibreConfig {
                    system_prompt,
                    allowed_tools,
                    llm_backend,
                })
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ChatLibreConfig::default()),
            Err(e) => Err(GovernanceError::Database(e)),
        }
    }

    /// Écrit la configuration dans la ligne id=1 (UPSERT).
    ///
    /// `allowed_tools` est sérialisé en JSON. `updated_at` est mis à jour à
    /// l'instant courant (UTC ISO-8601).
    ///
    /// # Errors
    ///
    /// Retourne [`GovernanceError::Database`] en cas d'erreur SQLite.
    pub fn save(&self, config: &ChatLibreConfig) -> Result<(), GovernanceError> {
        let allowed_tools_json = serde_json::to_string(&config.allowed_tools).map_err(|e| {
            GovernanceError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?;

        self.conn.execute(
            "INSERT INTO chat_libre_config (id, system_prompt, allowed_tools, llm_backend, updated_at) \
             VALUES (1, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ','now')) \
             ON CONFLICT(id) DO UPDATE SET \
                 system_prompt = excluded.system_prompt, \
                 allowed_tools = excluded.allowed_tools, \
                 llm_backend   = excluded.llm_backend, \
                 updated_at    = excluded.updated_at",
            params![
                config.system_prompt,
                allowed_tools_json,
                config.llm_backend,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance_db::GovernanceDb;
    use tempfile::TempDir;

    fn open_repo(dir: &TempDir) -> ChatLibreConfigRepository {
        // GovernanceDb::open applique la migration qui crée chat_libre_config.
        let db = GovernanceDb::open(dir.path()).expect("open governance.db");
        ChatLibreConfigRepository::open(db.path()).expect("open chat_libre repo")
    }

    #[test]
    fn test_load_default_when_freshly_migrated() {
        // GIVEN une governance.db fraîchement migrée.
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        // WHEN on charge la config.
        let cfg = repo.load().expect("load");
        // THEN tous les champs sont vides/None (équivalent à Default).
        assert_eq!(cfg.system_prompt, "");
        assert!(cfg.allowed_tools.is_empty());
        assert!(cfg.llm_backend.is_none());
    }

    #[test]
    fn test_save_then_load_roundtrip() {
        // GIVEN une config non triviale.
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        let cfg = ChatLibreConfig {
            system_prompt: "You are Apollia, a helpful local assistant.".into(),
            allowed_tools: vec!["bash_executor".into(), "file_read".into()],
            llm_backend: Some("ollama".into()),
        };
        // WHEN on sauvegarde puis recharge.
        repo.save(&cfg).expect("save");
        let loaded = repo.load().expect("reload");
        // THEN la config relue est identique à celle sauvegardée.
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn test_save_overwrites_existing_row() {
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        repo.save(&ChatLibreConfig {
            system_prompt: "first".into(),
            allowed_tools: vec!["a".into()],
            llm_backend: Some("backend-a".into()),
        })
        .expect("save first");
        repo.save(&ChatLibreConfig {
            system_prompt: "second".into(),
            allowed_tools: vec!["b".into(), "c".into()],
            llm_backend: None,
        })
        .expect("save second");

        let loaded = repo.load().expect("load");
        assert_eq!(loaded.system_prompt, "second");
        assert_eq!(loaded.allowed_tools, vec!["b".to_string(), "c".to_string()]);
        assert!(loaded.llm_backend.is_none());
    }
}
