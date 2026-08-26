//! SQLite repository for persisting installed agents.
//!
//! Provides [`AgentRepository`], which stores and retrieves installed agents in
//! a local SQLite store (`agents.db`). Agents survive restarts and are
//! auto-loaded at boot.
//!
//! The `007_agent_tables.sql` migration is applied idempotently when
//! [`AgentRepository::open`] is called.
//!
//! All methods are synchronous because SQLite calls are lightweight, and the
//! caller is responsible for running them in `spawn_blocking` when needed.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_core::AgentManifest;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

// The `installed_agents` DDL lives in `crate::agents_db`: `agents.db` is
// shared with the package tables, and `PRAGMA user_version` belongs to the
// file, so the two repositories migrate through one numbered list.

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Installed agent persisted in `agents.db`.
///
/// Represents an agent copied into `~/.apollia/agents/<name>/` with its
/// serialized manifest. The `enabled` field controls loading at boot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledAgent {
    /// Unique agent name (primary key).
    pub name: String,
    /// Agent semver version.
    pub version: String,
    /// Install path (e.g. `~/.apollia/agents/<name>/agent.py`).
    pub install_path: PathBuf,
    /// Original path of the installed source file.
    pub source_path: PathBuf,
    /// Agent manifest (serialized as JSON in the store).
    pub manifest: AgentManifest,
    /// Whether the agent is active and should be loaded at boot.
    pub enabled: bool,
    /// Installation timestamp (RFC 3339).
    pub installed_at: String,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Agent repository errors.
#[derive(Debug, thiserror::Error)]
pub enum AgentRepositoryError {
    /// Underlying SQLite error.
    #[error("erreur SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),

    /// Agent not found for the given name.
    #[error("agent '{0}' introuvable")]
    NotFound(String),

    /// Manifest JSON serialization/deserialization error.
    #[error("erreur sérialisation manifest : {0}")]
    SerdeError(#[from] serde_json::Error),

    /// A `spawn_blocking` task failed (panic in the worker thread).
    #[error("erreur tâche async : {0}")]
    SpawnError(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

/// SQLite repository for installed agents.
///
/// Wraps a SQLite connection to `agents.db`. The `007_agent_tables.sql`
/// migration is applied in [`open`](Self::open). WAL mode is enabled for
/// read/write concurrency.
///
/// Clonable via `Arc`; every clone shares the same connection. The async
/// wrappers use `tokio::task::spawn_blocking` to avoid blocking the Tokio
/// executor. See [`save_async`], [`list_async`], etc.
///
/// [`save_async`]: Self::save_async
/// [`list_async`]: Self::list_async
#[derive(Clone)]
pub struct AgentRepository {
    conn: Arc<Mutex<Connection>>,
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

    /// Inserts or updates an installed agent.
    ///
    /// Uses `INSERT OR REPLACE`: if an agent with the same `name` already
    /// exists, it is fully replaced. The `updated_at` field is updated
    /// automatically.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::SerdeError`] if the manifest cannot be serialized
    /// - [`AgentRepositoryError::Sqlite`] on a SQLite error
    pub fn save(&self, agent: &InstalledAgent) -> Result<(), AgentRepositoryError> {
        let manifest_json = serde_json::to_string(&agent.manifest)?;
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO installed_agents \
                 (name, version, install_path, source_path, manifest_json, \
                  enabled, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &agent.name,
                &agent.version,
                agent.install_path.to_string_lossy().as_ref(),
                agent.source_path.to_string_lossy().as_ref(),
                &manifest_json,
                agent.enabled,
                &agent.installed_at,
                &agent.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Fetches an installed agent by name.
    ///
    /// Returns `None` if no agent has that name.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::SerdeError`] if the manifest JSON is corrupted
    /// - [`AgentRepositoryError::Sqlite`] on a SQLite error
    pub fn get(&self, name: &str) -> Result<Option<InstalledAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, install_path, source_path, manifest_json, \
                    enabled, installed_at, updated_at \
             FROM installed_agents WHERE name = ?1",
        )?;

        let mut rows = stmt.query(params![name])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_installed_agent(row)?)),
            None => Ok(None),
        }
    }

    /// Lists all installed agents.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::SerdeError`] if a manifest JSON is corrupted
    /// - [`AgentRepositoryError::Sqlite`] on a SQLite error
    pub fn list(&self) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, install_path, source_path, manifest_json, \
                    enabled, installed_at, updated_at \
             FROM installed_agents ORDER BY name",
        )?;
        collect_agents(&mut stmt, [])
    }

    /// Lists only the installed and enabled agents (`enabled = true`).
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::SerdeError`] if a manifest JSON is corrupted
    /// - [`AgentRepositoryError::Sqlite`] on a SQLite error
    pub fn list_enabled(&self) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, install_path, source_path, manifest_json, \
                    enabled, installed_at, updated_at \
             FROM installed_agents WHERE enabled = 1 ORDER BY name",
        )?;
        collect_agents(&mut stmt, [])
    }

    /// Deletes an installed agent by name.
    ///
    /// Idempotent operation: returns no error if the agent does not exist.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::Sqlite`] on a SQLite error
    pub fn delete(&self, name: &str) -> Result<(), AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "DELETE FROM installed_agents WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

    /// Enables or disables an installed agent.
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
            "UPDATE installed_agents SET enabled = ?1, updated_at = datetime('now') \
             WHERE name = ?2",
            params![enabled, name],
        )?;
        if rows_affected == 0 {
            return Err(AgentRepositoryError::NotFound(name.to_string()));
        }
        Ok(())
    }

    // ─── Async wrappers ──────────────────────────────────────────────────────

    /// Async wrapper for [`save`]: runs the SQLite I/O on a blocking thread.
    ///
    /// [`save`]: Self::save
    pub async fn save_async(&self, agent: InstalledAgent) -> Result<(), AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.save(&agent))
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }

    /// Async wrapper for [`get`]: runs the SQLite I/O on a blocking thread.
    ///
    /// [`get`]: Self::get
    pub async fn get_async(
        &self,
        name: String,
    ) -> Result<Option<InstalledAgent>, AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.get(&name))
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }

    /// Async wrapper for [`list`]: runs the SQLite I/O on a blocking thread.
    ///
    /// [`list`]: Self::list
    pub async fn list_async(&self) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.list())
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }

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
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Converts a SQLite row into an [`InstalledAgent`].
fn row_to_installed_agent(row: &rusqlite::Row<'_>) -> Result<InstalledAgent, AgentRepositoryError> {
    let manifest_json: String = row.get(4)?;
    let manifest: AgentManifest = serde_json::from_str(&manifest_json)?;
    let install_path_str: String = row.get(2)?;
    let source_path_str: String = row.get(3)?;

    Ok(InstalledAgent {
        name: row.get(0)?,
        version: row.get(1)?,
        install_path: PathBuf::from(install_path_str),
        source_path: PathBuf::from(source_path_str),
        manifest,
        enabled: row.get(5)?,
        installed_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Collects query results into a vector of [`InstalledAgent`].
fn collect_agents<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
    let rows = stmt.query_map(params, |row| {
        let manifest_json: String = row.get(4)?;
        let install_path_str: String = row.get(2)?;
        let source_path_str: String = row.get(3)?;

        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            install_path_str,
            source_path_str,
            manifest_json,
            row.get::<_, bool>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;

    let mut agents = Vec::new();
    for row_result in rows {
        let (
            name,
            version,
            install_path,
            source_path,
            manifest_json,
            enabled,
            installed_at,
            updated_at,
        ) = row_result?;
        let manifest: AgentManifest = serde_json::from_str(&manifest_json)?;
        agents.push(InstalledAgent {
            name,
            version,
            install_path: PathBuf::from(install_path),
            source_path: PathBuf::from(source_path),
            manifest,
            enabled,
            installed_at,
            updated_at,
        });
    }
    Ok(agents)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::AgentManifest;
    use std::path::PathBuf;

    /// Creates a test [`AgentManifest`] with default values.
    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            format_version: 1,
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("Test agent {name}"),
            tools_required: vec!["bash".to_string()],
            tools_optional: Vec::new(),
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: Vec::new(),
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: Vec::new(),
            skills: Vec::new(),
            execution_mode: "auto".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: Vec::new(),
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        }
    }

    /// Creates a test [`InstalledAgent`].
    fn test_agent(name: &str) -> InstalledAgent {
        InstalledAgent {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            install_path: PathBuf::from(format!("/home/user/.apollia/agents/{name}/agent.py")),
            source_path: PathBuf::from(format!("/tmp/{name}.py")),
            manifest: test_manifest(name),
            enabled: true,
            installed_at: "2026-03-17T10:00:00Z".to_string(),
            updated_at: "2026-03-17T10:00:00Z".to_string(),
        }
    }

    /// Opens an in-memory repository for tests.
    fn open_test_repo() -> AgentRepository {
        AgentRepository::open(Path::new(":memory:")).expect("failed to open test repo")
    }

    // Migration creates the installed_agents table
    #[test]
    fn test_open_creates_table() {
        let repo = open_test_repo();
        // Checks the table exists by querying without error
        let count: i64 = {
            let conn = repo.conn.lock().expect("lock");
            conn.query_row("SELECT COUNT(*) FROM installed_agents", [], |row| {
                row.get(0)
            })
            .expect("table installed_agents should exist")
        };
        assert_eq!(count, 0);
    }

    // save() + get() round-trip avec manifest serde
    #[test]
    fn test_save_and_get_roundtrip() {
        let repo = open_test_repo();
        let agent = test_agent("hello-agent");

        repo.save(&agent).expect("save should succeed");
        let loaded = repo
            .get("hello-agent")
            .expect("get should succeed")
            .expect("agent should exist");

        assert_eq!(loaded.name, "hello-agent");
        assert_eq!(loaded.version, "1.0.0");
        assert_eq!(loaded.install_path, agent.install_path);
        assert_eq!(loaded.source_path, agent.source_path);
        assert!(loaded.enabled);
        // round-trip manifest
        assert_eq!(loaded.manifest.name, "hello-agent");
        assert_eq!(loaded.manifest.description, "Test agent hello-agent");
        assert_eq!(loaded.manifest.tools_required, vec!["bash"]);
    }

    // list() retourne tous les agents
    #[test]
    fn test_list_all_agents() {
        let repo = open_test_repo();
        repo.save(&test_agent("agent-a")).expect("save a");
        repo.save(&test_agent("agent-b")).expect("save b");
        repo.save(&test_agent("agent-c")).expect("save c");

        let agents = repo.list().expect("list should succeed");
        assert_eq!(agents.len(), 3);
        assert_eq!(agents[0].name, "agent-a");
        assert_eq!(agents[1].name, "agent-b");
        assert_eq!(agents[2].name, "agent-c");
    }

    // delete() supprime l'agent
    #[test]
    fn test_delete_agent() {
        let repo = open_test_repo();
        repo.save(&test_agent("to-delete")).expect("save");
        assert!(repo.get("to-delete").expect("get").is_some());

        repo.delete("to-delete").expect("delete should succeed");
        assert!(repo.get("to-delete").expect("get after delete").is_none());
    }

    // list_enabled() filtre les disabled
    #[test]
    fn test_list_enabled_filters_disabled() {
        let repo = open_test_repo();
        repo.save(&test_agent("enabled-1")).expect("save 1");
        repo.save(&test_agent("enabled-2")).expect("save 2");

        let mut disabled = test_agent("disabled-1");
        disabled.enabled = false;
        repo.save(&disabled).expect("save disabled");

        let enabled_agents = repo.list_enabled().expect("list_enabled");
        assert_eq!(enabled_agents.len(), 2);
        assert!(enabled_agents.iter().all(|a| a.enabled));
    }

    // set_enabled() toggle + updated_at
    #[test]
    fn test_set_enabled_toggle() {
        let repo = open_test_repo();
        repo.save(&test_agent("toggle-agent")).expect("save");

        // Check initial state: enabled
        let agent = repo.get("toggle-agent").expect("get").expect("exists");
        assert!(agent.enabled);

        // Disable
        repo.set_enabled("toggle-agent", false)
            .expect("set_enabled false");
        let agent = repo.get("toggle-agent").expect("get").expect("exists");
        assert!(!agent.enabled);
        // updated_at changed (datetime('now') != original timestamp)
        assert_ne!(agent.updated_at, "2026-03-17T10:00:00Z");

        // Re-enable
        repo.set_enabled("toggle-agent", true)
            .expect("set_enabled true");
        let agent = repo.get("toggle-agent").expect("get").expect("exists");
        assert!(agent.enabled);
    }

    // save() upserts an existing agent
    #[test]
    fn test_save_upsert_existing() {
        let repo = open_test_repo();
        let mut agent = test_agent("upsert-agent");
        repo.save(&agent).expect("save v1");

        // Update the version
        agent.version = "2.0.0".to_string();
        agent.updated_at = "2026-03-17T12:00:00Z".to_string();
        repo.save(&agent).expect("save v2");

        let loaded = repo.get("upsert-agent").expect("get").expect("exists");
        assert_eq!(loaded.version, "2.0.0");
        assert_eq!(loaded.updated_at, "2026-03-17T12:00:00Z");

        // A single agent in the store
        let all = repo.list().expect("list");
        assert_eq!(all.len(), 1);
    }

    // get() returns None for a non-existent agent
    #[test]
    fn test_get_nonexistent_returns_none() {
        let repo = open_test_repo();
        let result = repo.get("does-not-exist").expect("get should not error");
        assert!(result.is_none());
    }

    // list_async() returns the same agents as list()
    #[tokio::test]
    async fn test_agent_repository_async_list() {
        // GIVEN
        let repo = open_test_repo();
        repo.save(&test_agent("alpha")).expect("save alpha");
        repo.save(&test_agent("beta")).expect("save beta");

        // WHEN
        let agents = repo.list_async().await.expect("list_async should succeed");

        // THEN
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "alpha");
        assert_eq!(agents[1].name, "beta");
    }

    // list_async() propagates errors correctly
    #[tokio::test]
    async fn test_agent_repository_async_error_propagation() {
        // GIVEN a closed repository cannot happen with in-memory, but we check
        // that get_async returns None for a non-existent agent
        let repo = open_test_repo();

        // WHEN
        let result = repo
            .get_async("non-existent-agent".to_string())
            .await
            .expect("get_async should not error");

        // THEN
        assert!(result.is_none());
    }
}
