//! SQLite repository for the agents of the machine.
//!
//! Provides [`AgentRepository`], which opens the local SQLite store
//! (`agents.db`, table `agents`). An agent is a definition and the user's
//! overrides: the stored agent, its overrides and the resolved agent are read
//! and written in `crate::agent_overrides`; this module opens the file,
//! removes an agent and switches it on or off.
//!
//! All methods are synchronous because SQLite calls are lightweight, and the
//! caller is responsible for running them in `spawn_blocking` when needed.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};

// The `agents` DDL lives in `crate::agents_db`: `agents.db` is shared with
// the package tables, and `PRAGMA user_version` belongs to the file, so the
// two repositories migrate through one numbered list.

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Agent repository errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentRepositoryError {
    /// Underlying SQLite error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),

    /// Agent not found for the given name.
    #[error("agent '{0}' not found")]
    NotFound(String),

    /// The agent is part of Apollia itself and cannot be uninstalled.
    #[error("agent '{0}' is built into Apollia and cannot be uninstalled")]
    BuiltIn(String),

    /// A definition or an override could not be read or written as JSON.
    #[error("definition serialisation error: {0}")]
    SerdeError(#[from] serde_json::Error),

    /// An override value does not fit its field.
    #[error(transparent)]
    InvalidOverride(#[from] crate::agent_definition::OverrideValueError),

    /// A `spawn_blocking` task failed (panic in the worker thread).
    #[error("async task error: {0}")]
    SpawnError(String),

    /// The name carries `/`, the separator of a workflow identifier
    /// (`<package>/<workflow>`): an agent never shares an identifier with a
    /// workflow, so the two never answer to the same name.
    #[error("agent name '{0}' contains '/', which only workflow identifiers carry")]
    NameOfAWorkflow(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

/// SQLite repository for the agents of the machine.
///
/// Wraps a SQLite connection to `agents.db`, brought to the current schema in
/// [`open`](Self::open). WAL mode is enabled for
/// read/write concurrency.
///
/// Clonable via `Arc`; every clone shares the same connection.
#[derive(Clone)]
pub struct AgentRepository {
    pub(crate) conn: Arc<Mutex<Connection>>,
}

impl AgentRepository {
    /// Opens (or creates) the SQLite store and migrates `agents.db`.
    ///
    /// The file is brought to the current `agents.db` schema version; a
    /// database written by a newer binary is refused instead of misread.
    /// Enables WAL mode for concurrency.
    ///
    /// # Errors
    ///
    /// Returns an error if the SQLite file cannot be opened, the migration
    /// fails, or the database is newer than this binary.
    pub fn open(path: &Path) -> Result<Self, AgentRepositoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        crate::agents_db::open_agents_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Deletes an agent by name, with its overrides.
    ///
    /// Idempotent operation: returns no error if the agent does not exist.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::BuiltIn`] for the Apollia agent, which is
    ///   never uninstalled
    /// - [`AgentRepositoryError::Sqlite`] on a SQLite error
    pub fn delete(&self, name: &str) -> Result<(), AgentRepositoryError> {
        if crate::agent_definition::is_built_in_agent(name) {
            return Err(AgentRepositoryError::BuiltIn(name.to_owned()));
        }
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM agent_overrides WHERE agent = ?1",
            params![name],
        )?;
        tx.execute("DELETE FROM agents WHERE name = ?1", params![name])?;
        tx.commit()?;
        Ok(())
    }

    /// Where the code of `name` was copied when agents ran code: the path a
    /// row written then still carries, so that an uninstallation removes the
    /// folder. `None` for an unknown agent and for every agent stored since.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::Sqlite`] on a SQLite error.
    pub fn legacy_install_path(
        &self,
        name: &str,
    ) -> Result<Option<std::path::PathBuf>, AgentRepositoryError> {
        use rusqlite::OptionalExtension;
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let path: Option<Option<String>> = conn
            .query_row(
                "SELECT install_path FROM agents WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(path.flatten().map(std::path::PathBuf::from))
    }

    /// Enables or disables a stored agent.
    ///
    /// Updates the `enabled` field and the `updated_at` timestamp.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::NotFound`] if the agent does not exist
    /// - [`AgentRepositoryError::Sqlite`] on a SQLite error
    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let rows_affected = conn.execute(
            "UPDATE agents SET enabled = ?1, updated_at = datetime('now') \
             WHERE name = ?2",
            params![enabled, name],
        )?;
        if rows_affected == 0 {
            return Err(AgentRepositoryError::NotFound(name.to_string()));
        }
        Ok(())
    }

    // ─── Async wrappers ──────────────────────────────────────────────────────

    /// Async wrapper for [`delete`]: runs the SQLite I/O on a blocking thread.
    ///
    /// [`delete`]: Self::delete
    pub async fn delete_async(&self, name: String) -> Result<(), AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.delete(&name))
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_definition::{AgentOrigin, StoredAgent};

    /// A stored agent named `name`, enabled.
    fn test_agent(name: &str) -> StoredAgent {
        StoredAgent {
            name: name.to_string(),
            origin: AgentOrigin::Package,
            package: None,
            version: "1.0.0".to_string(),
            definition: serde_json::json!({
                "id": name, "name": name, "mission": format!("Test agent {name}"),
                "defaults": {},
            }),
            enabled: true,
            installed_at: "2026-03-01T00:00:00Z".to_string(),
            updated_at: "2026-03-01T00:00:00Z".to_string(),
        }
    }

    fn open_test_repo() -> AgentRepository {
        AgentRepository::open(Path::new(":memory:")).expect("failed to open in-memory repo")
    }

    #[test]
    fn test_open_creates_table() {
        // GIVEN a fresh in-memory file
        // WHEN the repository opens it
        let repo = open_test_repo();

        // THEN the table exists and holds the Apollia agent alone
        let conn = repo.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agents", [], |row| row.get(0))
            .expect("table agents should exist");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_delete_agent() {
        // GIVEN one stored agent
        let repo = open_test_repo();
        repo.save_definition(&test_agent("to-delete"))
            .expect("save");
        assert!(repo.stored("to-delete").expect("stored").is_some());

        // WHEN it is deleted
        repo.delete("to-delete").expect("delete");

        // THEN it is gone
        assert!(repo.stored("to-delete").expect("stored").is_none());
    }

    #[test]
    fn test_delete_refuses_the_apollia_agent() {
        // GIVEN a repository, which always holds the Apollia agent, with one
        // override on it
        let repo = open_test_repo();
        repo.set_override(
            crate::APOLLIA_AGENT_ID,
            crate::OverrideField::Mission,
            &serde_json::json!("Be brief."),
        )
        .expect("override");

        // WHEN the Apollia agent is deleted
        let err = repo
            .delete(crate::APOLLIA_AGENT_ID)
            .expect_err("Apollia is never uninstalled");

        // THEN the refusal is typed, and the agent and its override remain
        assert!(matches!(err, AgentRepositoryError::BuiltIn(ref n) if n == "apollia:chat"));
        assert!(repo
            .stored(crate::APOLLIA_AGENT_ID)
            .expect("stored")
            .is_some());
        assert_eq!(
            repo.overrides(crate::APOLLIA_AGENT_ID)
                .expect("overrides")
                .len(),
            1
        );
    }

    #[test]
    fn test_set_enabled_toggle() {
        // GIVEN a stored agent, enabled as stored
        let repo = open_test_repo();
        repo.save_definition(&test_agent("toggle-agent"))
            .expect("save");
        assert!(
            repo.stored("toggle-agent")
                .expect("stored")
                .expect("exists")
                .enabled
        );

        // WHEN it is disabled, then enabled again
        repo.set_enabled("toggle-agent", false).expect("disable");
        let disabled = repo
            .stored("toggle-agent")
            .expect("stored")
            .expect("exists");
        repo.set_enabled("toggle-agent", true).expect("enable");
        let enabled = repo
            .stored("toggle-agent")
            .expect("stored")
            .expect("exists");

        // THEN each write is read back
        assert!(!disabled.enabled);
        assert!(enabled.enabled);
    }

    #[test]
    fn test_set_enabled_on_an_unknown_agent_is_not_found() {
        // GIVEN a repository that does not hold the agent
        let repo = open_test_repo();

        // WHEN it is disabled
        let err = repo
            .set_enabled("does-not-exist", false)
            .expect_err("unknown");

        // THEN the refusal names it
        assert!(matches!(err, AgentRepositoryError::NotFound(ref n) if n == "does-not-exist"));
    }

    #[tokio::test]
    async fn test_delete_async_removes_the_agent() {
        // GIVEN one stored agent
        let repo = open_test_repo();
        repo.save_definition(&test_agent("alpha")).expect("save");

        // WHEN it is deleted off the executor
        repo.delete_async("alpha".to_string())
            .await
            .expect("delete");

        // THEN it is gone
        assert!(repo.stored("alpha").expect("stored").is_none());
    }

    #[test]
    fn test_legacy_install_path_is_read_from_an_old_row_only() {
        // GIVEN a row written when agents ran code, and an agent stored since
        let repo = open_test_repo();
        repo.conn
            .lock()
            .unwrap()
            .execute_batch(
                r#"INSERT INTO agents (name, origin, version, definition_json, install_path)
                   VALUES ('old', 'package', '1.0.0', '{"id":"old"}', '/a/old/agent.py');"#,
            )
            .expect("insert");
        repo.save_definition(&test_agent("new")).expect("save");

        // WHEN the path is read for each, and for an unknown name
        let old = repo.legacy_install_path("old").expect("old");
        let new = repo.legacy_install_path("new").expect("new");
        let unknown = repo.legacy_install_path("nobody").expect("unknown");

        // THEN only the old row answers one
        assert_eq!(old, Some(std::path::PathBuf::from("/a/old/agent.py")));
        assert_eq!(new, None);
        assert_eq!(unknown, None);
    }
}
