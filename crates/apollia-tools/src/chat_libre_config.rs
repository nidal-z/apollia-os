//! Persistence of the free-chat configuration in `governance.db`.
//!
//! Singleton (id=1) grouping the default system prompt, the list of tools
//! allowed without HITL, and the preferred LLM backend for free-chat sessions.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::governance_db::GovernanceError;

/// Free-chat configuration persisted in `governance.db`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatLibreConfig {
    /// Default system prompt injected at the start of each free-chat session.
    pub system_prompt: String,
    /// List of auto-allowed tools (equivalent to agent-scoped rules).
    pub allowed_tools: Vec<String>,
    /// Preferred LLM backend (`None` means the runtime default).
    pub llm_backend: Option<String>,
}

/// SQLite repository for [`ChatLibreConfig`].
///
/// The `chat_libre_config` table is created by the [`crate::governance_db`]
/// migration. This repository only opens the connection and reads/writes the
/// single row (id=1).
pub struct ChatLibreConfigRepository {
    conn: Connection,
}

impl ChatLibreConfigRepository {
    /// Opens the `governance.db` store read/write.
    ///
    /// # Errors
    ///
    /// Returns [`GovernanceError::Database`] if opening fails.
    pub fn open(db_path: &Path) -> Result<Self, GovernanceError> {
        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        Ok(Self { conn })
    }

    /// Reads the id=1 row of the `chat_libre_config` table.
    ///
    /// If the table is empty (unexpected after migration), returns the default
    /// configuration to preserve the silent fallback.
    ///
    /// # Errors
    ///
    /// Returns [`GovernanceError::Database`] on a SQLite error.
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

    /// Writes the configuration to the id=1 row (UPSERT).
    ///
    /// `allowed_tools` is serialized as JSON. `updated_at` is set to the current
    /// instant (UTC ISO-8601).
    ///
    /// # Errors
    ///
    /// Returns [`GovernanceError::Database`] on a SQLite error.
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
        // GovernanceDb::open applies the migration that creates chat_libre_config.
        let db = GovernanceDb::open(dir.path()).expect("open governance.db");
        ChatLibreConfigRepository::open(db.path()).expect("open chat_libre repo")
    }

    #[test]
    fn test_load_default_when_freshly_migrated() {
        // GIVEN a freshly migrated governance.db.
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        // WHEN the config is loaded.
        let cfg = repo.load().expect("load");
        // THEN all fields are empty/None (equivalent to Default).
        assert_eq!(cfg.system_prompt, "");
        assert!(cfg.allowed_tools.is_empty());
        assert!(cfg.llm_backend.is_none());
    }

    #[test]
    fn test_save_then_load_roundtrip() {
        // GIVEN a non-trivial config.
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        let cfg = ChatLibreConfig {
            system_prompt: "You are Apollia, a helpful local assistant.".into(),
            allowed_tools: vec!["bash_executor".into(), "file_read".into()],
            llm_backend: Some("ollama".into()),
        };
        // WHEN it is saved then reloaded.
        repo.save(&cfg).expect("save");
        let loaded = repo.load().expect("reload");
        // THEN the reloaded config is identical to the saved one.
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn test_save_overwrites_existing_row() {
        // GIVEN a repository on a throwaway directory
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        // WHEN two configurations are saved in a row
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
        // THEN only the second one is held: the table carries a single row
        assert_eq!(loaded.system_prompt, "second");
        assert_eq!(loaded.allowed_tools, vec!["b".to_string(), "c".to_string()]);
        assert!(loaded.llm_backend.is_none());
    }
}
