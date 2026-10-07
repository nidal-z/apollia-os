//! SQLite repository for persisting installed agent packages.
//!
//! A **package** is what a `.apo` archive installs: the agents, workflows and
//! tools its `apollia.toml` declares, and the Python dependencies it asks for.
//!
//! This repository manages `installed_packages` and `package_items`, the list
//! of what each package installed (agents, workflows, tools, connections).
//! Individual agents remain in `agents` (see [`AgentRepository`]).
//!
//! [`AgentRepository`]: crate::AgentRepository

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

// The package-table DDL lives in `crate::agents_db`: `agents.db` is shared
// with the agent tables, and `PRAGMA user_version` belongs to
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
    /// Contents of the manifest of the package, serialized as JSON.
    pub manifest_json: String,
    /// Installation timestamp (RFC 3339).
    pub installed_at: String,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
}

/// Who signed an installed package, as recorded at installation.
///
/// Whether the publisher is trusted is not part of it: that is read from the
/// keyring whenever it is shown, so removing a key takes effect at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSignature {
    /// Publisher the package declares.
    pub publisher: String,
    /// SHA-256 fingerprint of the key that signed it, `None` when unsigned.
    pub key_fingerprint: Option<String>,
    /// Whether the package carried a signature that held.
    pub signed: bool,
}

/// Kind of an item a package installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageItemKind {
    /// An agent.
    Agent,
    /// A workflow.
    Workflow,
    /// A Python tool.
    Tool,
    /// A connection (an MCP server, by name).
    Connection,
}

impl PackageItemKind {
    /// Stored form.
    pub fn as_str(self) -> &'static str {
        match self {
            PackageItemKind::Agent => "agent",
            PackageItemKind::Workflow => "workflow",
            PackageItemKind::Tool => "tool",
            PackageItemKind::Connection => "connection",
        }
    }

    /// Reads the stored form.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "agent" => Some(PackageItemKind::Agent),
            "workflow" => Some(PackageItemKind::Workflow),
            "tool" => Some(PackageItemKind::Tool),
            "connection" => Some(PackageItemKind::Connection),
            _ => None,
        }
    }
}

/// One item a package installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageItem {
    /// What the item is.
    pub kind: PackageItemKind,
    /// Its identifier: an agent or workflow id, a tool name, a server name.
    pub item_id: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Package repository errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageRepositoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),

    #[error("package '{0}' not found")]
    NotFound(String),

    #[error("JSON serialisation error: {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("async task error: {0}")]
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

    /// Records the link between a package and one of its agents, and names
    /// the package as the agent's origin.
    ///
    /// Idempotent.
    pub fn link_agent(
        &self,
        package_name: &str,
        agent_name: &str,
    ) -> Result<(), PackageRepositoryError> {
        self.link_item(package_name, PackageItemKind::Agent, agent_name)?;
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE agents SET package = ?1 WHERE name = ?2",
            params![package_name, agent_name],
        )?;
        Ok(())
    }

    /// Records that `package_name` installed the item `item_id` of `kind`.
    ///
    /// Idempotent.
    pub fn link_item(
        &self,
        package_name: &str,
        kind: PackageItemKind,
        item_id: &str,
    ) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT OR IGNORE INTO package_items (package_name, kind, item_id) \
             VALUES (?1, ?2, ?3)",
            params![package_name, kind.as_str(), item_id],
        )?;
        Ok(())
    }

    /// Everything `package_name` installed, by kind then identifier.
    pub fn list_items(
        &self,
        package_name: &str,
    ) -> Result<Vec<PackageItem>, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT kind, item_id FROM package_items WHERE package_name = ?1 \
             ORDER BY kind, item_id",
        )?;
        let rows = stmt.query_map(params![package_name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (kind, item_id) = row?;
            // The table refuses any other kind; a kind this binary does not
            // know was written by a newer one and is skipped.
            if let Some(kind) = PackageItemKind::parse(&kind) {
                items.push(PackageItem { kind, item_id });
            }
        }
        Ok(items)
    }

    /// Records who signed `package_name`. The package must be saved first.
    ///
    /// # Errors
    ///
    /// [`PackageRepositoryError::NotFound`] when the package is not stored,
    /// or [`PackageRepositoryError::Sqlite`].
    pub fn set_signature(
        &self,
        package_name: &str,
        signature: &PackageSignature,
    ) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let changed = conn.execute(
            "UPDATE installed_packages SET publisher = ?2, key_fingerprint = ?3, signed = ?4 \
             WHERE name = ?1",
            params![
                package_name,
                &signature.publisher,
                &signature.key_fingerprint,
                signature.signed
            ],
        )?;
        if changed == 0 {
            return Err(PackageRepositoryError::NotFound(package_name.to_owned()));
        }
        Ok(())
    }

    /// Who signed `package_name`, as recorded at its installation.
    ///
    /// # Errors
    ///
    /// [`PackageRepositoryError::Sqlite`].
    pub fn signature(
        &self,
        package_name: &str,
    ) -> Result<Option<PackageSignature>, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT publisher, key_fingerprint, signed FROM installed_packages WHERE name = ?1",
        )?;
        let mut rows = stmt.query(params![package_name])?;
        match rows.next()? {
            Some(row) => Ok(Some(PackageSignature {
                publisher: row.get(0)?,
                key_fingerprint: row.get(1)?,
                signed: row.get(2)?,
            })),
            None => Ok(None),
        }
    }

    /// Records whether Apollia ships `package_name` inside its binary. The
    /// package must be saved first, and saving it again forgets the mark.
    ///
    /// # Errors
    ///
    /// [`PackageRepositoryError::NotFound`] when the package is not stored,
    /// or [`PackageRepositoryError::Sqlite`].
    pub fn set_embedded(
        &self,
        package_name: &str,
        embedded: bool,
    ) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let changed = conn.execute(
            "UPDATE installed_packages SET embedded = ?2 WHERE name = ?1",
            params![package_name, embedded],
        )?;
        if changed == 0 {
            return Err(PackageRepositoryError::NotFound(package_name.to_owned()));
        }
        Ok(())
    }

    /// Whether Apollia ships `package_name` inside its binary; `false` for a
    /// package that is not installed.
    ///
    /// # Errors
    ///
    /// [`PackageRepositoryError::Sqlite`].
    pub fn is_embedded(&self, package_name: &str) -> Result<bool, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare("SELECT embedded FROM installed_packages WHERE name = ?1")?;
        let mut rows = stmt.query(params![package_name])?;
        match rows.next()? {
            Some(row) => Ok(row.get(0)?),
            None => Ok(false),
        }
    }

    /// Forgets what `package_name` installed, before an update records the
    /// items of its new version.
    ///
    /// # Errors
    ///
    /// [`PackageRepositoryError::Sqlite`].
    pub fn clear_items(&self, package_name: &str) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "DELETE FROM package_items WHERE package_name = ?1",
            params![package_name],
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
        Ok(self
            .list_items(package_name)?
            .into_iter()
            .filter(|item| item.kind == PackageItemKind::Agent)
            .map(|item| item.item_id)
            .collect())
    }

    /// Deletes a package and the list of what it installed.
    ///
    /// The items themselves (agents in `agents`) must be deleted separately,
    /// via [`AgentRepository::delete`].
    pub fn delete(&self, name: &str) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM package_items WHERE package_name = ?1",
            params![name],
        )?;
        tx.execute(
            "DELETE FROM installed_packages WHERE name = ?1",
            params![name],
        )?;
        tx.commit()?;
        Ok(())
    }

    // ─── Async wrappers ──────────────────────────────────────────────────────

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

    // GIVEN a saved package
    // WHEN it is marked as shipped inside the binary, then saved again
    // THEN the mark reads back, and a new save forgets it
    #[test]
    fn test_embedded_mark_is_set_read_and_forgotten_by_a_save() {
        let repo = open_test_repo();
        let pkg = test_package("guide");
        repo.save(&pkg).unwrap();
        assert!(!repo.is_embedded("guide").unwrap());

        repo.set_embedded("guide", true).unwrap();
        assert!(repo.is_embedded("guide").unwrap());

        repo.save(&pkg).unwrap();
        assert!(!repo.is_embedded("guide").unwrap());
    }

    // GIVEN no package of that name
    // WHEN it is marked as shipped inside the binary
    // THEN the mark is refused, and reading it answers false
    #[test]
    fn test_embedded_mark_of_an_unknown_package_is_refused() {
        let repo = open_test_repo();

        let marked = repo.set_embedded("ghost", true);

        assert!(matches!(marked, Err(PackageRepositoryError::NotFound(_))));
        assert!(!repo.is_embedded("ghost").unwrap());
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
    fn test_package_items_of_every_kind() {
        // GIVEN a package that installed an agent, a workflow, a tool and a
        // connection, and whose agent row exists
        let repo = open_test_repo();
        repo.save(&test_package("office")).expect("save pkg");
        {
            let conn = repo.conn.lock().expect("lock");
            conn.execute(
                "INSERT INTO agents (name, version, definition_json) VALUES ('triage', '1', '{}')",
                [],
            )
            .unwrap();
        }
        repo.link_agent("office", "triage").expect("link agent");
        repo.link_item("office", PackageItemKind::Workflow, "monthly-close")
            .expect("link workflow");
        repo.link_item("office", PackageItemKind::Tool, "extract_totals")
            .expect("link tool");
        repo.link_item("office", PackageItemKind::Connection, "ledger")
            .expect("link connection");

        // WHEN its items are listed
        let items = repo.list_items("office").expect("items");

        // THEN each kind is listed, the agent names its package, and the
        // deletion of the package takes the list with it
        let kinds: Vec<_> = items.iter().map(|i| (i.kind, i.item_id.as_str())).collect();
        assert_eq!(
            kinds,
            vec![
                (PackageItemKind::Agent, "triage"),
                (PackageItemKind::Connection, "ledger"),
                (PackageItemKind::Tool, "extract_totals"),
                (PackageItemKind::Workflow, "monthly-close"),
            ]
        );
        let package: Option<String> = repo
            .conn
            .lock()
            .expect("lock")
            .query_row(
                "SELECT package FROM agents WHERE name = 'triage'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(package.as_deref(), Some("office"));
        repo.delete("office").expect("delete");
        assert!(repo.list_items("office").expect("items").is_empty());
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
}
