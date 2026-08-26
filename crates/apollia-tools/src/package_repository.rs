//! SQLite repository for persisting installed agent packages.
//!
//! A **package** is a self-contained directory described by an `agent.toml`.
//! It bundles a director and its workers, triggers, and shared pip
//! dependencies.
//!
//! This repository manages `installed_packages` and `package_agents`. Individual
//! agents remain in `installed_agents` (see [`AgentRepository`]).
//!
//! The `008_package_tables.sql` migration is applied idempotently when
//! [`PackageRepository::open`] is called.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

// The package-table DDL lives in `crate::agents_db`: `agents.db` is shared
// with the installed-agent registry, and `PRAGMA user_version` belongs to
// the file, so the two repositories migrate through one numbered list.

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata of an installed package, persisted in `agents.db`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    /// Unique package name (primary key).
    pub name: String,
    /// Package semver version.
    pub version: String,
    /// Absolute path of the package directory (`~/.apollia/agents/packages/<name>/`).
    pub root_path: PathBuf,
    /// Contents of `agent.toml` serialized as JSON.
    pub manifest_json: String,
    /// Installation timestamp (RFC 3339).
    pub installed_at: String,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Package repository errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageRepositoryError {
    #[error("erreur SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),

    #[error("package '{0}' introuvable")]
    NotFound(String),

    #[error("erreur sérialisation JSON : {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("erreur tâche async : {0}")]
    SpawnError(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

/// SQLite repository for installed agent packages.
///
/// Clonable via `Arc`; every clone shares the same connection.
/// WAL mode is enabled for read/write concurrency.
#[derive(Clone)]
pub struct PackageRepository {
    conn: Arc<Mutex<Connection>>,
}

impl PackageRepository {
    /// Opens (or creates) the SQLite store and migrates `agents.db`.
    ///
    /// The file is brought to the current `agents.db` schema version; a
    /// database written by a newer binary is refused instead of misread.
    pub fn open(path: &Path) -> Result<Self, PackageRepositoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        crate::agents_db::open_agents_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Inserts or updates an installed package (idempotent UPSERT).
    pub fn save(&self, pkg: &InstalledPackage) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO installed_packages \
                 (name, version, root_path, manifest_json, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &pkg.name,
                &pkg.version,
                pkg.root_path.to_string_lossy().as_ref(),
                &pkg.manifest_json,
                &pkg.installed_at,
                &pkg.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Records the link between a package and one of its agents.
    ///
    /// Idempotent: uses `INSERT OR IGNORE`.
    pub fn link_agent(
        &self,
        package_name: &str,
        agent_name: &str,
    ) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT OR IGNORE INTO package_agents (package_name, agent_name) VALUES (?1, ?2)",
            params![package_name, agent_name],
        )?;
        Ok(())
    }

    /// Fetches a package by name.
    pub fn get(&self, name: &str) -> Result<Option<InstalledPackage>, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, root_path, manifest_json, installed_at, updated_at \
             FROM installed_packages WHERE name = ?1",
        )?;
        let mut rows = stmt.query(params![name])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_package(row)?)),
            None => Ok(None),
        }
    }

    /// Lists all installed packages.
    pub fn list(&self) -> Result<Vec<InstalledPackage>, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, root_path, manifest_json, installed_at, updated_at \
             FROM installed_packages ORDER BY name",
        )?;
        collect_packages(&mut stmt, [])
    }

    /// Returns the names of the agents belonging to a package.
    pub fn list_agents_for_package(
        &self,
        package_name: &str,
    ) -> Result<Vec<String>, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT agent_name FROM package_agents WHERE package_name = ?1 ORDER BY agent_name",
        )?;
        let rows = stmt.query_map(params![package_name], |row| row.get::<_, String>(0))?;
        let mut names = Vec::new();
        for r in rows {
            names.push(r?);
        }
        Ok(names)
    }

    /// Deletes a package and its agent links (CASCADE on `package_agents`).
    ///
    /// Entries in `installed_agents` must be deleted separately via
    /// [`AgentRepository::delete`].
    pub fn delete(&self, name: &str) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "DELETE FROM installed_packages WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

    // ─── Async wrappers ──────────────────────────────────────────────────────

    /// Async wrapper for [`save`].
    pub async fn save_async(&self, pkg: InstalledPackage) -> Result<(), PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.save(&pkg))
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }

    /// Async wrapper for [`get`].
    pub async fn get_async(
        &self,
        name: String,
    ) -> Result<Option<InstalledPackage>, PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.get(&name))
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }

    /// Async wrapper for [`list`].
    pub async fn list_async(&self) -> Result<Vec<InstalledPackage>, PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.list())
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }

    /// Async wrapper for [`delete`].
    pub async fn delete_async(&self, name: String) -> Result<(), PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.delete(&name))
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers internes
// ─────────────────────────────────────────────────────────────────────────────

fn row_to_package(row: &rusqlite::Row<'_>) -> Result<InstalledPackage, PackageRepositoryError> {
    let root_path_str: String = row.get(2)?;
    Ok(InstalledPackage {
        name: row.get(0)?,
        version: row.get(1)?,
        root_path: PathBuf::from(root_path_str),
        manifest_json: row.get(3)?,
        installed_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn collect_packages<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
) -> Result<Vec<InstalledPackage>, PackageRepositoryError> {
    let rows = stmt.query_map(params, |row| {
        let root_path_str: String = row.get(2)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            root_path_str,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    let mut pkgs = Vec::new();
    for row_result in rows {
        let (name, version, root_path, manifest_json, installed_at, updated_at) = row_result?;
        pkgs.push(InstalledPackage {
            name,
            version,
            root_path: PathBuf::from(root_path),
            manifest_json,
            installed_at,
            updated_at,
        });
    }
    Ok(pkgs)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_package(name: &str) -> InstalledPackage {
        InstalledPackage {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            root_path: PathBuf::from(format!("/home/user/.apollia/agents/packages/{name}")),
            manifest_json: r#"{"package":{"name":"test"}}"#.to_string(),
            installed_at: "2026-04-24T07:00:00Z".to_string(),
            updated_at: "2026-04-24T07:00:00Z".to_string(),
        }
    }

    fn open_test_repo() -> PackageRepository {
        PackageRepository::open(Path::new(":memory:")).expect("open in-memory repo")
    }

    #[test]
    fn test_open_creates_tables() {
        // GIVEN an empty store
        let repo = open_test_repo();
        // WHEN the repo is opened
        // THEN the tables exist (query without error)
        let conn = repo.conn.lock().expect("lock");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM installed_packages", [], |r| r.get(0))
            .expect("table installed_packages should exist");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_save_and_get_roundtrip() {
        // GIVEN a package
        let repo = open_test_repo();
        let pkg = test_package("veille-ia");
        // WHEN it is saved and fetched
        repo.save(&pkg).expect("save");
        let loaded = repo.get("veille-ia").expect("get").expect("exists");
        // THEN the fields are identical
        assert_eq!(loaded.name, "veille-ia");
        assert_eq!(loaded.version, "1.0.0");
        assert_eq!(loaded.root_path, pkg.root_path);
    }

    #[test]
    fn test_link_agent_and_list() {
        // GIVEN a package with 2 linked agents
        let repo = open_test_repo();
        repo.save(&test_package("my-pkg")).expect("save pkg");
        // Note: no FK to installed_agents in :memory: (no migration 007),
        // so PRAGMA foreign_keys is disabled for this test
        {
            let conn = repo.conn.lock().expect("lock");
            conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        }
        repo.link_agent("my-pkg", "director-agent")
            .expect("link director");
        repo.link_agent("my-pkg", "worker-agent")
            .expect("link worker");
        // WHEN listing the package agents
        let agents = repo.list_agents_for_package("my-pkg").expect("list agents");
        // THEN the 2 agents are returned
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&"director-agent".to_string()));
        assert!(agents.contains(&"worker-agent".to_string()));
    }

    #[test]
    fn test_save_upsert_existing() {
        // GIVEN an already-saved package
        let repo = open_test_repo();
        repo.save(&test_package("pkg-v1")).expect("save v1");
        // WHEN saving with a new version (upsert)
        let mut pkg_v2 = test_package("pkg-v1");
        pkg_v2.version = "2.0.0".to_string();
        repo.save(&pkg_v2).expect("save v2");
        // THEN the version is updated, a single record
        let loaded = repo.get("pkg-v1").expect("get").expect("exists");
        assert_eq!(loaded.version, "2.0.0");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn test_delete_removes_package() {
        // GIVEN a saved package
        let repo = open_test_repo();
        repo.save(&test_package("to-delete")).expect("save");
        // WHEN it is deleted
        repo.delete("to-delete").expect("delete");
        // THEN the package no longer exists
        assert!(repo.get("to-delete").expect("get").is_none());
    }

    #[tokio::test]
    async fn test_list_async_returns_saved_packages() {
        // GIVEN 2 saved packages
        let repo = open_test_repo();
        repo.save(&test_package("pkg-a")).expect("save a");
        repo.save(&test_package("pkg-b")).expect("save b");
        // WHEN listing asynchronously
        let pkgs = repo.list_async().await.expect("list_async");
        // THEN the 2 packages are returned
        assert_eq!(pkgs.len(), 2);
    }
}
