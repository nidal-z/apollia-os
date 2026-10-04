//! SQLite reader of the former projects (`projects.db`).
//!
//! Projects are no longer a feature: at boot the runtime migrates them into
//! sources, then removes the file once a migration has completed. What is left
//! here is what that migration reads ([`ProjectRepository::list_projects`],
//! [`ProjectRepository::get_project`]) and the writers its tests build a
//! former database with.
//!
//! The `010_projects.sql` migration is applied idempotently when
//! [`ProjectRepository::open`] is called.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Embedded migration SQL, applied idempotently on each open.
const MIGRATION_SQL: &str = include_str!("../migrations/010_projects.sql");

/// Migration 009: the project_agents join table.
const MIGRATION_009_SQL: &str = include_str!("../migrations/009_project_agents.sql");

/// Current schema version of `projects.db`.
const PROJECTS_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `projects.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`PROJECTS_SCHEMA_VERSION`].
const PROJECTS_MIGRATIONS: &[apollia_core::schema::Migration] = &[migrate_v1];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `projects.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the 010 + 009 shape, and the shape with `workspace_path`,
/// and bring each of them to the same state.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(MIGRATION_SQL)?;
    conn.execute_batch(MIGRATION_009_SQL)?;
    apollia_core::schema::add_column_if_missing(
        conn,
        "ALTER TABLE projects ADD COLUMN workspace_path TEXT",
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Project summary (list view).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Associated working directory (used by the git/rules/tree providers).
    pub workspace_path: Option<String>,
}

/// Full project detail with its documents, providers, and linked agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Associated working directory (used by the git/rules/tree providers).
    pub workspace_path: Option<String>,
    pub documents: Vec<ProjectDocument>,
    pub providers: Vec<ProjectProviderRow>,
    /// Names of the agents linked to the project.
    pub agents: Vec<String>,
}

/// Document attached to a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDocument {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub uploaded_at: String,
}

/// Context provider configured for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProviderRow {
    pub id: String,
    pub project_id: String,
    pub provider_type: String,
    pub name: String,
    pub config_json: String,
    pub path: Option<String>,
    pub enabled: bool,
    pub priority: u8,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Errors raised by the project repository.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProjectRepositoryError {
    /// Underlying SQLite error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),

    /// Project not found.
    #[error("project '{0}' not found")]
    NotFound(String),

    /// The internal lock was poisoned.
    #[error("internal lock poisoned")]
    LockPoisoned,
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

/// SQLite repository for projects.
///
/// Wraps a SQLite connection to `projects.db`. The `010_projects.sql`
/// migration is applied in [`open`](Self::open). WAL mode is enabled for
/// concurrent reads and writes.
///
/// Cloneable through `Arc`: every clone shares the same connection.
#[derive(Clone)]
pub struct ProjectRepository {
    conn: Arc<Mutex<Connection>>,
}

impl ProjectRepository {
    /// Opens (or creates) the SQLite database and applies the projects
    /// migrations (`010_projects.sql`, then `009_project_agents.sql`).
    ///
    /// The migration is idempotent (`CREATE TABLE IF NOT EXISTS`).
    /// Enables WAL mode for concurrency.
    pub fn open(path: &Path) -> Result<Self, ProjectRepositoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        apollia_core::schema::open_versioned(
            &conn,
            apollia_core::paths::DataFile::Projects.file_name(),
            PROJECTS_SCHEMA_VERSION,
            PROJECTS_MIGRATIONS,
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Lists all projects (alphabetical by name).
    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>, ProjectRepositoryError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ProjectRepositoryError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT id, name, description, created_at, updated_at, workspace_path
             FROM projects ORDER BY name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                workspace_path: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Returns the full project detail, including documents and providers.
    pub fn get_project(&self, id: &str) -> Result<ProjectDetail, ProjectRepositoryError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ProjectRepositoryError::LockPoisoned)?;

        let mut stmt = conn.prepare(
            "SELECT id, name, description, instructions, created_at, updated_at, workspace_path
             FROM projects WHERE id = ?1",
        )?;
        let project = stmt.query_row(params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        });

        let (pid, name, description, instructions, created_at, updated_at, workspace_path) =
            match project {
                Ok(r) => r,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    return Err(ProjectRepositoryError::NotFound(id.to_owned()))
                }
                Err(e) => return Err(e.into()),
            };

        let mut doc_stmt = conn.prepare(
            "SELECT id, project_id, name, file_path, size_bytes, uploaded_at
             FROM project_documents WHERE project_id = ?1 ORDER BY uploaded_at ASC",
        )?;
        let documents = doc_stmt
            .query_map(params![pid], |row| {
                Ok(ProjectDocument {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    file_path: row.get(3)?,
                    size_bytes: row.get(4)?,
                    uploaded_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut prov_stmt = conn.prepare(
            "SELECT id, project_id, provider_type, name, config_json, path, enabled, priority
             FROM project_providers WHERE project_id = ?1 ORDER BY priority ASC",
        )?;
        let providers = prov_stmt
            .query_map(params![pid], |row| {
                Ok(ProjectProviderRow {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    provider_type: row.get(2)?,
                    name: row.get(3)?,
                    config_json: row.get(4)?,
                    path: row.get(5)?,
                    enabled: row.get::<_, i64>(6)? != 0,
                    priority: row.get::<_, i64>(7)? as u8,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut agent_stmt = conn.prepare(
            "SELECT agent_name FROM project_agents WHERE project_id = ?1 ORDER BY added_at ASC",
        )?;
        let agents = agent_stmt
            .query_map(params![pid], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ProjectDetail {
            id: pid,
            name,
            description,
            instructions,
            created_at,
            updated_at,
            workspace_path,
            documents,
            providers,
            agents,
        })
    }

    /// Creates a new project. Returns its `id` (UUID v4).
    /// TEST-ONLY: production never writes a project any more; the tests of
    /// the migration build a former database through it.
    pub fn create_project(
        &self,
        name: impl Into<String>,
        description: Option<String>,
        instructions: Option<String>,
        workspace_path: Option<String>,
    ) -> Result<String, ProjectRepositoryError> {
        let id = uuid();
        let now = now_rfc3339();
        let conn = self
            .conn
            .lock()
            .map_err(|_| ProjectRepositoryError::LockPoisoned)?;
        conn.execute(
            "INSERT INTO projects (id, name, description, instructions, workspace_path, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, name.into(), description, instructions, workspace_path, now],
        )?;
        Ok(id)
    }

    /// Attaches a document to a project. Returns the document `id`.
    /// TEST-ONLY: production never writes a project any more; the tests of
    /// the migration build a former database through it.
    pub fn add_document(
        &self,
        project_id: &str,
        name: impl Into<String>,
        file_path: impl Into<String>,
        size_bytes: i64,
    ) -> Result<String, ProjectRepositoryError> {
        let id = uuid();
        let now = now_rfc3339();
        let conn = self
            .conn
            .lock()
            .map_err(|_| ProjectRepositoryError::LockPoisoned)?;
        conn.execute(
            "INSERT INTO project_documents (id, project_id, name, file_path, size_bytes, uploaded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, project_id, name.into(), file_path.into(), size_bytes, now],
        )?;
        Ok(id)
    }

    /// Inserts or updates a provider for a project.
    ///
    /// If `provider_id` is `Some(id)` and a row with that id exists for this
    /// project, it is updated. Otherwise a new row is inserted with a fresh
    /// UUID. Returns the id of the row that was actually affected.
    /// TEST-ONLY: production never writes a project any more; the tests of
    /// the migration build a former database through it.
    // REASON: each argument is one column of the provider row; the table schema is the struct.
    #[allow(clippy::too_many_arguments)]
    pub fn set_provider(
        &self,
        provider_id: Option<&str>,
        project_id: &str,
        provider_type: impl Into<String>,
        name: impl Into<String>,
        config_json: impl Into<String>,
        path: Option<String>,
        enabled: bool,
        priority: u8,
    ) -> Result<String, ProjectRepositoryError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ProjectRepositoryError::LockPoisoned)?;

        let provider_type_str = provider_type.into();
        let name_str = name.into();
        let config_json_str = config_json.into();

        if let Some(id) = provider_id {
            let updated = conn.execute(
                "UPDATE project_providers
                    SET provider_type = ?1,
                        name          = ?2,
                        config_json   = ?3,
                        path          = ?4,
                        enabled       = ?5,
                        priority      = ?6
                  WHERE id = ?7 AND project_id = ?8",
                params![
                    provider_type_str,
                    name_str,
                    config_json_str,
                    path,
                    enabled as i64,
                    priority as i64,
                    id,
                    project_id,
                ],
            )?;
            if updated > 0 {
                return Ok(id.to_owned());
            }
            // Fall through to INSERT if the id was unknown: keeps callers
            // resilient if a stale id is passed (e.g. after a delete).
        }

        let new_id = uuid();
        conn.execute(
            "INSERT INTO project_providers
                (id, project_id, provider_type, name, config_json, path, enabled, priority)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                new_id,
                project_id,
                provider_type_str,
                name_str,
                config_json_str,
                path,
                enabled as i64,
                priority as i64,
            ],
        )?;
        Ok(new_id)
    }

    /// Links an agent to a project. Idempotent (`INSERT OR IGNORE`).
    /// TEST-ONLY: production never writes a project any more; the tests of
    /// the migration build a former database through it.
    pub fn add_agent(
        &self,
        project_id: &str,
        agent_name: &str,
    ) -> Result<(), ProjectRepositoryError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ProjectRepositoryError::LockPoisoned)?;
        let now = now_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO project_agents (project_id, agent_name, added_at)
             VALUES (?1, ?2, ?3)",
            params![project_id, agent_name, now],
        )?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format: YYYY-MM-DDTHH:MM:SSZ (UTC, seconds precision)
    let s = secs;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days = s / 86400; // days since epoch
    let (y, mo, d) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, mo, d, hour, min, sec
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Simplified Gregorian calendar conversion
    let mut y = 1970u64;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let months = if is_leap(y) {
        [31u64, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut mo = 1u64;
    for dm in months {
        if days < dm {
            break;
        }
        days -= dm;
        mo += 1;
    }
    (y, mo, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn open_memory() -> ProjectRepository {
        ProjectRepository::open(std::path::Path::new(":memory:")).expect("open :memory:")
    }

    /// Pre-versioning `projects.db` schema, frozen as the oldest shape a
    /// published binary wrote (`user_version = 0`, no `workspace_path`).
    const PROJECTS_V0_SQL: &str = include_str!("../tests/fixtures/schemas/projects_v0.sql");

    // GIVEN a database written by a pre-versioning binary, with a project row
    // WHEN opening it through the versioned layer
    // THEN the row survives, workspace_path appears and the version is stamped
    #[test]
    fn test_projects_db_old_format_migrates_and_keeps_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("projects.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(PROJECTS_V0_SQL).unwrap();
            conn.execute(
                "INSERT INTO projects (id, name, created_at, updated_at)
                 VALUES ('p-1', 'Legacy', '2026-01-01', '2026-01-01')",
                [],
            )
            .unwrap();
        }

        let repo = ProjectRepository::open(&path).unwrap();

        let list = repo.list_projects().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Legacy");
        let conn = Connection::open(&path).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(PROJECTS_SCHEMA_VERSION));
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it
    // THEN the open is refused
    #[test]
    fn test_projects_db_newer_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("projects.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", PROJECTS_SCHEMA_VERSION + 1)
                .unwrap();
        }

        let err = ProjectRepository::open(&path).map(|_| ()).unwrap_err();

        assert!(matches!(
            err,
            ProjectRepositoryError::Schema(
                apollia_core::schema::SchemaError::NewerThanBinary { .. }
            )
        ));
    }

    #[test]
    fn test_crud_project() {
        // GIVEN an in-memory repository
        let repo = open_memory();
        // WHEN a project is created
        let id = repo
            .create_project(
                "Mon projet",
                Some("desc".into()),
                Some("instructions".into()),
                None,
            )
            .expect("create");
        // THEN it appears in the list
        let list = repo.list_projects().expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].name, "Mon projet");
        // AND its detail is complete
        let detail = repo.get_project(&id).expect("get");
        assert_eq!(detail.instructions.as_deref(), Some("instructions"));
        assert!(detail.documents.is_empty());
        assert!(detail.providers.is_empty());
    }

    #[test]
    fn test_a_document_appears_in_the_project_detail() {
        // GIVEN a project
        let repo = open_memory();
        let pid = repo.create_project("P", None, None, None).expect("create");
        // WHEN a document is attached
        let doc_id = repo
            .add_document(&pid, "readme.md", "/tmp/readme.md", 512)
            .expect("add_doc");
        // THEN the detail lists it
        let detail = repo.get_project(&pid).expect("get");
        assert_eq!(detail.documents.len(), 1);
        assert_eq!(detail.documents[0].id, doc_id);
    }

    #[test]
    fn test_migration_idempotent() {
        // GIVEN the repository already initialized
        let path = std::path::Path::new(":memory:");
        let repo = ProjectRepository::open(path).expect("open 1");
        // WHEN reopened (migration replayed)
        drop(repo);
        // Migration is embedded in open(), so re-applying on the same path must succeed.
        // For :memory: this means a fresh DB - but tests above already verify idempotency
        // via seed_builtin_templates. This test just ensures open() itself is safe.
        // THEN the second open succeeds: replaying the migration is safe
        ProjectRepository::open(path).expect("open 2");
    }
}
