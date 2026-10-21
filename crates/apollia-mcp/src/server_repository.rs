//! SQLite-backed repository for MCP server configurations.
//!
//! Stores every [`McpServerConfig`] in `mcp.db` so the desktop application
//! can manage MCP connections without touching `mcp.toml`. The schema is
//! applied on `open()` and is idempotent (`CREATE TABLE IF NOT EXISTS`).
//!
//! JSON columns (`args_json`, `env_json`, `tags_json`) are serialised via
//! `serde_json` and deserialised on read. Booleans are stored as `INTEGER`
//! (`0`/`1`) because SQLite has no native boolean type.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};
use thiserror::Error;

use crate::config::McpServerConfig;
use crate::origin::{ConnectionOrigin, Provenance};

/// Errors returned by [`McpServerRepository`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum McpRepoError {
    /// No server with this name exists in the database.
    #[error("server '{0}' not found")]
    NotFound(String),

    /// The server name contains characters outside `[a-z0-9_-]`.
    #[error("invalid server name '{0}': only [a-z0-9_-] allowed")]
    InvalidName(String),

    /// A rusqlite operation failed.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),

    /// A JSON serialisation or deserialisation failed.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Current schema version of `mcp.db`.
const MCP_SCHEMA_VERSION: u32 = 5;

/// Numbered migration steps of `mcp.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`MCP_SCHEMA_VERSION`].
const MCP_MIGRATIONS: &[apollia_core::schema::Migration] =
    &[migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `mcp.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the initial shape, and the shapes before the
/// max_response_bytes and max_tools columns, and bring each of them to the
/// same state.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mcp_servers (
            name               TEXT PRIMARY KEY,
            command            TEXT NOT NULL DEFAULT '',
            args_json          TEXT NOT NULL DEFAULT '[]',
            env_json           TEXT NOT NULL DEFAULT '{}',
            transport          TEXT NOT NULL DEFAULT 'stdio',
            url                TEXT,
            requires_approval  INTEGER NOT NULL DEFAULT 0,
            init_timeout_secs  INTEGER NOT NULL DEFAULT 30,
            call_timeout_secs  INTEGER NOT NULL DEFAULT 60,
            max_response_bytes INTEGER NOT NULL DEFAULT 8388608,
            max_tools          INTEGER NOT NULL DEFAULT 256,
            tags_json          TEXT NOT NULL DEFAULT '[]',
            enabled            INTEGER NOT NULL DEFAULT 1,
            created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )?;
    apollia_core::schema::add_column_if_missing(
        conn,
        "ALTER TABLE mcp_servers ADD COLUMN max_response_bytes INTEGER NOT NULL DEFAULT 8388608;",
    )?;
    apollia_core::schema::add_column_if_missing(
        conn,
        "ALTER TABLE mcp_servers ADD COLUMN max_tools INTEGER NOT NULL DEFAULT 256;",
    )
}

/// v2: the classification the curated selection gives each tool.
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    apollia_core::schema::add_column_if_missing(
        conn,
        "ALTER TABLE mcp_servers ADD COLUMN tool_families_json TEXT NOT NULL DEFAULT '{}';",
    )
}

/// v3: where each connection comes from. A row added before it has no
/// provenance, which reads as custom until a caller that knows better writes
/// one (see [`McpServerRepository::set_origin_if_missing`]).
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    apollia_core::schema::add_column_if_missing(
        conn,
        "ALTER TABLE mcp_servers ADD COLUMN provenance TEXT;",
    )?;
    apollia_core::schema::add_column_if_missing(
        conn,
        "ALTER TABLE mcp_servers ADD COLUMN official INTEGER NOT NULL DEFAULT 0;",
    )
}

/// v4: the curated selection entry a connection was made from, which an
/// organisation policy names it by. A row added before it has none until the
/// desktop writes one (see [`McpServerRepository::set_selection_entry_if_missing`]).
fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    apollia_core::schema::add_column_if_missing(
        conn,
        "ALTER TABLE mcp_servers ADD COLUMN selection_entry TEXT;",
    )
}

/// v5: a server no longer carries an approval flag of its own. Whether a call
/// may run is decided on the action family of the tool, so the column that
/// held the flag goes; what it held is not carried anywhere.
fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    let present = conn
        .prepare("SELECT 1 FROM pragma_table_info('mcp_servers') WHERE name = 'requires_approval'")?
        .exists([])?;
    if present {
        conn.execute_batch("ALTER TABLE mcp_servers DROP COLUMN requires_approval")?;
    }
    Ok(())
}

/// SQLite-backed repository for [`McpServerConfig`] entries.
///
/// Each instance owns a single `rusqlite::Connection`. The connection is
/// opened in WAL mode to allow concurrent readers from other crates that
/// open separate connections to the same file (e.g. `apollia-runtime`).
pub struct McpServerRepository {
    conn: Connection,
}

impl McpServerRepository {
    /// Opens `mcp.db` at `path` and applies the schema migration.
    ///
    /// Creates the file if it does not exist. The migration is idempotent
    /// (`CREATE TABLE IF NOT EXISTS`) so calling `open` on an existing
    /// database is safe.
    pub fn open(path: &Path) -> Result<Self, McpRepoError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        apollia_core::schema::open_versioned(
            &conn,
            apollia_core::paths::DataFile::Mcp.file_name(),
            MCP_SCHEMA_VERSION,
            MCP_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Inserts or replaces a server configuration.
    ///
    /// A configuration without an origin keeps the one already stored for its
    /// name; one with an origin replaces it, official flag and selection entry
    /// included.
    ///
    /// The server name must match `[a-z0-9_-]+`; otherwise
    /// [`McpRepoError::InvalidName`] is returned before touching the database.
    pub fn save(&self, config: &McpServerConfig) -> Result<(), McpRepoError> {
        validate_name(&config.name)?;

        let args_json = serde_json::to_string(&config.args)
            .map_err(|e| McpRepoError::Serialization(e.to_string()))?;
        let env_json = serde_json::to_string(&config.env)
            .map_err(|e| McpRepoError::Serialization(e.to_string()))?;
        let tags_json = serde_json::to_string(&config.tags)
            .map_err(|e| McpRepoError::Serialization(e.to_string()))?;
        let tool_families_json = serde_json::to_string(&config.tool_families)
            .map_err(|e| McpRepoError::Serialization(e.to_string()))?;

        self.conn.execute(
            "INSERT INTO mcp_servers
                (name, command, args_json, env_json, transport, url,
                 init_timeout_secs, call_timeout_secs,
                 max_response_bytes, max_tools, tags_json, tool_families_json,
                 provenance, official, selection_entry, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     ?14, ?15, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(name) DO UPDATE SET
                command            = excluded.command,
                args_json          = excluded.args_json,
                env_json           = excluded.env_json,
                transport          = excluded.transport,
                url                = excluded.url,
                init_timeout_secs  = excluded.init_timeout_secs,
                call_timeout_secs  = excluded.call_timeout_secs,
                max_response_bytes = excluded.max_response_bytes,
                max_tools          = excluded.max_tools,
                tags_json          = excluded.tags_json,
                tool_families_json = excluded.tool_families_json,
                provenance         = COALESCE(excluded.provenance, mcp_servers.provenance),
                official           = CASE WHEN excluded.provenance IS NULL
                                          THEN mcp_servers.official
                                          ELSE excluded.official END,
                selection_entry    = CASE WHEN excluded.provenance IS NULL
                                          THEN mcp_servers.selection_entry
                                          ELSE excluded.selection_entry END,
                updated_at         = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![
                config.name,
                config.command,
                args_json,
                env_json,
                config.transport,
                config.url,
                config.init_timeout_secs as i64,
                config.call_timeout_secs as i64,
                config.max_response_bytes as i64,
                config.max_tools as i64,
                tags_json,
                tool_families_json,
                config.origin.as_ref().map(|o| o.provenance.as_str()),
                config.origin.as_ref().is_some_and(|o| o.official) as i64,
                config.origin.as_ref().and_then(|o| o.entry.as_deref()),
            ],
        )?;
        Ok(())
    }

    /// Returns all servers (enabled and disabled).
    pub fn list(&self) -> Result<Vec<McpServerConfig>, McpRepoError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, command, args_json, env_json, transport, url,
                    init_timeout_secs, call_timeout_secs,
                    tags_json, max_response_bytes, max_tools, tool_families_json,
                    provenance, official, selection_entry
             FROM mcp_servers
             ORDER BY name",
        )?;
        let rows = stmt.query_map([], row_to_config)?;
        rows.map(|r| r.map_err(McpRepoError::Db))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Returns the server with `name`, or `None` if it does not exist.
    pub fn find_by_name(&self, name: &str) -> Result<Option<McpServerConfig>, McpRepoError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, command, args_json, env_json, transport, url,
                    init_timeout_secs, call_timeout_secs,
                    tags_json, max_response_bytes, max_tools, tool_families_json,
                    provenance, official, selection_entry
             FROM mcp_servers
             WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], row_to_config)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Writes `origin` for the server with `name` when it has none yet.
    ///
    /// Returns `true` when a row was written. A server that already carries an
    /// origin, or no server of that name, is left alone and answers `false`.
    pub fn set_origin_if_missing(
        &self,
        name: &str,
        origin: ConnectionOrigin,
    ) -> Result<bool, McpRepoError> {
        let affected = self.conn.execute(
            "UPDATE mcp_servers SET provenance = ?1, official = ?2, selection_entry = ?3
             WHERE name = ?4 AND provenance IS NULL",
            params![
                origin.provenance.as_str(),
                origin.official as i64,
                origin.entry,
                name
            ],
        )?;
        Ok(affected > 0)
    }

    /// Writes `entry` as the selection entry of the server with `name` when it
    /// comes from the selection and carries none yet.
    ///
    /// Returns `true` when a row was written; any other row is left alone.
    pub fn set_selection_entry_if_missing(
        &self,
        name: &str,
        entry: &str,
    ) -> Result<bool, McpRepoError> {
        let affected = self.conn.execute(
            "UPDATE mcp_servers SET selection_entry = ?1
             WHERE name = ?2 AND provenance = 'selection' AND selection_entry IS NULL",
            params![entry, name],
        )?;
        Ok(affected > 0)
    }

    /// Deletes the server with `name`.
    ///
    /// Returns [`McpRepoError::NotFound`] when no row was deleted.
    pub fn delete(&self, name: &str) -> Result<(), McpRepoError> {
        let affected = self
            .conn
            .execute("DELETE FROM mcp_servers WHERE name = ?1", params![name])?;
        if affected == 0 {
            return Err(McpRepoError::NotFound(name.to_string()));
        }
        Ok(())
    }

    /// Sets the `enabled` flag for the server with `name`.
    ///
    /// Returns [`McpRepoError::NotFound`] when no row was updated.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), McpRepoError> {
        let affected = self.conn.execute(
            "UPDATE mcp_servers
             SET enabled = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE name = ?2",
            params![enabled as i64, name],
        )?;
        if affected == 0 {
            return Err(McpRepoError::NotFound(name.to_string()));
        }
        Ok(())
    }

    /// Names of the servers whose `enabled` flag is off: the servers the user
    /// put in pause, which are kept but never started.
    pub fn disabled_names(&self) -> Result<std::collections::HashSet<String>, McpRepoError> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM mcp_servers WHERE enabled = 0")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| r.map_err(McpRepoError::Db))
            .collect::<Result<_, _>>()
    }

    /// Whether the server with `name` is enabled; `None` when no server has
    /// that name.
    pub fn is_enabled(&self, name: &str) -> Result<Option<bool>, McpRepoError> {
        let mut stmt = self
            .conn
            .prepare("SELECT enabled FROM mcp_servers WHERE name = ?1")?;
        let mut rows = stmt.query_map(params![name], |row| row.get::<_, i64>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row? != 0)),
            None => Ok(None),
        }
    }

    /// Imports a list of [`McpServerConfig`] in a single pass.
    ///
    /// This operation is a no-op if the `mcp_servers` table already contains
    /// at least one row; it returns `Ok(0)` without modifying the database.
    /// When the table is empty every entry in `configs` is saved via [`save`].
    ///
    /// [`save`]: McpServerRepository::save
    pub fn import_from_toml(&self, configs: Vec<McpServerConfig>) -> Result<usize, McpRepoError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM mcp_servers", [], |row| row.get(0))?;
        if count > 0 {
            return Ok(0);
        }
        let mut imported = 0;
        for config in &configs {
            self.save(config)?;
            imported += 1;
        }
        Ok(imported)
    }
}

/// Validates that `name` matches `[a-z0-9_-]+`.
fn validate_name(name: &str) -> Result<(), McpRepoError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '-'))
    {
        return Err(McpRepoError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Deserialise one JSON column, failing the row rather than defaulting.
///
/// A column that does not parse is a corrupt row, not an empty value. Reading
/// it as `[]` or `{}` starts a server stripped of the arguments and the
/// environment it was saved with, and the operator sees a handshake failure
/// instead of the corruption that caused it.
fn json_column<T: serde::de::DeserializeOwned>(index: usize, raw: &str) -> rusqlite::Result<T> {
    serde_json::from_str(raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}

/// Deserialises a single database row into a [`McpServerConfig`].
fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpServerConfig> {
    let args_json: String = row.get(2)?;
    let env_json: String = row.get(3)?;
    let tags_json: String = row.get(8)?;
    let init_timeout_secs: i64 = row.get(6)?;
    let call_timeout_secs: i64 = row.get(7)?;
    let max_response_bytes: i64 = row.get(9)?;
    let max_tools: i64 = row.get(10)?;

    let args: Vec<String> = json_column(2, &args_json)?;
    let env: HashMap<String, String> = json_column(3, &env_json)?;
    let tags: Vec<String> = json_column(8, &tags_json)?;
    let tool_families_json: String = row.get(11)?;
    let tool_families = json_column(11, &tool_families_json)?;
    let provenance: Option<String> = row.get(12)?;
    let official: i64 = row.get(13)?;
    let entry: Option<String> = row.get(14)?;
    let origin = provenance
        .as_deref()
        .and_then(Provenance::parse)
        .map(|provenance| ConnectionOrigin {
            provenance,
            official: official != 0,
            entry,
        });

    Ok(McpServerConfig {
        format_version: 1,
        name: row.get(0)?,
        command: row.get(1)?,
        args,
        env,
        transport: row.get(4)?,
        url: row.get(5)?,
        init_timeout_secs: init_timeout_secs as u64,
        call_timeout_secs: call_timeout_secs as u64,
        max_response_bytes: max_response_bytes as u64,
        max_tools: max_tools as u32,
        tags,
        tool_families,
        origin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn stdio_config(name: &str) -> McpServerConfig {
        McpServerConfig {
            format_version: 1,
            name: name.to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), format!("@{name}/mcp-server")],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
            tags: vec![],
            tool_families: Default::default(),
            origin: None,
        }
    }

    #[test]
    fn test_crud_complete() {
        // GIVEN an open repository
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let config = stdio_config("notion");

        // WHEN save → list → find_by_name → delete
        repo.save(&config).unwrap();
        assert_eq!(repo.list().unwrap().len(), 1);
        assert!(repo.find_by_name("notion").unwrap().is_some());

        repo.delete("notion").unwrap();

        // THEN the server is gone
        assert!(repo.find_by_name("notion").unwrap().is_none());
        assert_eq!(repo.list().unwrap().len(), 0);
    }

    #[test]
    fn test_corrupt_env_column_fails_the_read() {
        // GIVEN a saved server whose env column is then corrupted in place
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        let repo = McpServerRepository::open(&path).unwrap();
        let mut config = stdio_config("notion");
        config.env.insert("TOKEN".to_string(), "secret".to_string());
        repo.save(&config).unwrap();
        repo.conn
            .execute(
                "UPDATE mcp_servers SET env_json = 'not json' WHERE name = 'notion'",
                [],
            )
            .unwrap();

        // WHEN the row is read back
        let read = repo.find_by_name("notion");

        // THEN the read fails instead of handing back a server stripped of
        // its environment, which would be started without its credentials
        assert!(
            read.is_err(),
            "a corrupt env column must not be read as an empty environment"
        );
    }

    #[test]
    fn test_intact_columns_still_round_trip() {
        // GIVEN a saved server with arguments, environment and tags
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let mut config = stdio_config("notion");
        config.env.insert("TOKEN".to_string(), "secret".to_string());
        config.tags = vec!["work".to_string()];
        repo.save(&config).unwrap();

        // WHEN the row is read back
        let read = repo.find_by_name("notion").unwrap().unwrap();

        // THEN every column survives: refusing a corrupt row must not become
        // refusing a sound one
        assert_eq!(read.args, config.args);
        assert_eq!(read.env.get("TOKEN"), Some(&"secret".to_string()));
        assert_eq!(read.tags, vec!["work".to_string()]);
    }

    #[test]
    fn test_import_from_toml_idempotent_if_not_empty() {
        // GIVEN a repository with one existing server
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        repo.save(&stdio_config("notion")).unwrap();

        // WHEN import_from_toml is called
        let count = repo.import_from_toml(vec![stdio_config("notion")]).unwrap();

        // THEN nothing is imported
        assert_eq!(count, 0);
        assert_eq!(repo.list().unwrap().len(), 1);
    }

    #[test]
    fn test_import_from_toml_populates_empty_table() {
        // GIVEN an empty repository
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();

        // WHEN import_from_toml is called with two servers
        let count = repo
            .import_from_toml(vec![stdio_config("notion"), stdio_config("sqlite")])
            .unwrap();

        // THEN both are inserted
        assert_eq!(count, 2);
        assert_eq!(repo.list().unwrap().len(), 2);
    }

    #[test]
    fn test_disabled_servers_are_read_and_kept_by_a_save() {
        // GIVEN two servers, one switched off
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        repo.save(&stdio_config("notion")).unwrap();
        repo.save(&stdio_config("sqlite")).unwrap();
        repo.set_enabled("notion", false).unwrap();

        // WHEN the flag is read, then the switched-off one is saved again
        let disabled = repo.disabled_names().unwrap();
        repo.save(&stdio_config("notion")).unwrap();

        // THEN only that one reads disabled, and a save does not switch it on
        assert_eq!(
            disabled,
            std::collections::HashSet::from(["notion".to_owned()])
        );
        assert_eq!(repo.is_enabled("notion").unwrap(), Some(false));
        assert_eq!(repo.is_enabled("sqlite").unwrap(), Some(true));
        assert_eq!(repo.is_enabled("ghost").unwrap(), None);
        assert_eq!(
            repo.disabled_names().unwrap(),
            std::collections::HashSet::from(["notion".to_owned()])
        );
    }

    #[test]
    fn test_set_enabled_persists() {
        // GIVEN a server with enabled=true (default)
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        repo.save(&stdio_config("notion")).unwrap();

        // WHEN set_enabled("notion", false)
        repo.set_enabled("notion", false).unwrap();

        // THEN a raw query confirms the persisted value
        let enabled: i64 = repo
            .conn
            .query_row(
                "SELECT enabled FROM mcp_servers WHERE name = 'notion'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(enabled, 0);
    }

    #[test]
    fn test_invalid_name_rejected() {
        // GIVEN a config whose name contains disallowed characters
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let config = McpServerConfig {
            format_version: 1,
            name: "My Server!".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
            tags: vec![],
            tool_families: Default::default(),
            origin: None,
        };

        // WHEN save is called
        // THEN InvalidName is returned
        assert!(matches!(
            repo.save(&config),
            Err(McpRepoError::InvalidName(_))
        ));
    }

    #[test]
    fn test_delete_missing_server_returns_not_found() {
        // GIVEN an empty repository
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();

        // WHEN delete is called for a non-existent server
        // THEN NotFound is returned
        assert!(matches!(
            repo.delete("ghost"),
            Err(McpRepoError::NotFound(_))
        ));
    }

    #[test]
    fn test_set_enabled_missing_server_returns_not_found() {
        // GIVEN an empty repository
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();

        // WHEN set_enabled is called for a non-existent server
        // THEN NotFound is returned
        assert!(matches!(
            repo.set_enabled("ghost", false),
            Err(McpRepoError::NotFound(_))
        ));
    }

    #[test]
    fn test_save_roundtrip_preserves_all_fields() {
        // GIVEN a config with non-default fields
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let config = McpServerConfig {
            format_version: 1,
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@notionhq/notion-mcp-server".to_string()],
            env: HashMap::from([("NOTION_KEY".to_string(), "tok_123".to_string())]),
            transport: "stdio".to_string(),
            url: None,
            init_timeout_secs: 45,
            call_timeout_secs: 90,
            max_response_bytes: 4 * 1024 * 1024,
            max_tools: 42,
            tags: vec!["productivity".to_string()],
            tool_families: [(
                "notion-search".to_string(),
                apollia_tools::ActionFamily::ReadPersonal,
            )]
            .into(),
            origin: Some(ConnectionOrigin {
                provenance: Provenance::Registry,
                official: true,
                entry: None,
            }),
        };

        // WHEN saved and read back
        repo.save(&config).unwrap();
        let found = repo.find_by_name("notion").unwrap().unwrap();

        // THEN all fields are preserved
        assert_eq!(found.name, "notion");
        assert_eq!(found.args, vec!["-y", "@notionhq/notion-mcp-server"]);
        assert_eq!(found.env["NOTION_KEY"], "tok_123");
        assert_eq!(found.init_timeout_secs, 45);
        assert_eq!(found.call_timeout_secs, 90);
        assert_eq!(found.max_response_bytes, 4 * 1024 * 1024);
        assert_eq!(found.max_tools, 42);
        assert_eq!(found.tags, vec!["productivity"]);
        assert_eq!(found.tool_families, config.tool_families);
        assert_eq!(found.origin, config.origin);
    }

    #[test]
    fn test_tool_families_column_backfills_on_a_v1_db() {
        // GIVEN a db written at schema version 1, before the classification column
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        {
            let conn = Connection::open(&path).unwrap();
            migrate_v1(&conn).unwrap();
            conn.execute_batch("PRAGMA user_version = 1;").unwrap();
            conn.execute(
                "INSERT INTO mcp_servers (name, command) VALUES ('legacy', 'npx')",
                [],
            )
            .unwrap();
        }

        // WHEN the repository opens it
        let repo = McpServerRepository::open(&path).unwrap();

        // THEN the existing row reads back with no classification
        let found = repo.find_by_name("legacy").unwrap().unwrap();
        assert!(found.tool_families.is_empty());
    }

    #[test]
    fn test_migration_is_idempotent() {
        // GIVEN a db already opened once
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        McpServerRepository::open(&path).unwrap();

        // WHEN opened a second time
        // THEN no error (CREATE TABLE IF NOT EXISTS is idempotent)
        assert!(McpServerRepository::open(&path).is_ok());
    }

    #[test]
    fn test_max_tools_column_backfills_on_legacy_db() {
        // GIVEN a legacy db whose schema predates the max_tools column
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE mcp_servers (
                    name               TEXT PRIMARY KEY,
                    command            TEXT NOT NULL DEFAULT '',
                    args_json          TEXT NOT NULL DEFAULT '[]',
                    env_json           TEXT NOT NULL DEFAULT '{}',
                    transport          TEXT NOT NULL DEFAULT 'stdio',
                    url                TEXT,
                    requires_approval  INTEGER NOT NULL DEFAULT 0,
                    init_timeout_secs  INTEGER NOT NULL DEFAULT 30,
                    call_timeout_secs  INTEGER NOT NULL DEFAULT 60,
                    max_response_bytes INTEGER NOT NULL DEFAULT 8388608,
                    tags_json          TEXT NOT NULL DEFAULT '[]',
                    enabled            INTEGER NOT NULL DEFAULT 1,
                    created_at         TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z',
                    updated_at         TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z'
                );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO mcp_servers (name, command) VALUES ('legacy', 'npx')",
                [],
            )
            .unwrap();
        }

        // WHEN the repository opens the db (running the additive migration)
        let repo = McpServerRepository::open(&path).unwrap();

        // THEN the pre-existing row reads back with the default max_tools
        let found = repo.find_by_name("legacy").unwrap().unwrap();
        assert_eq!(found.max_tools, 256);
    }

    #[test]
    fn test_the_approval_column_of_an_older_file_is_dropped_and_its_rows_kept() {
        // GIVEN a db from before the versioned layer, holding a server that
        //       was flagged as requiring approval
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE mcp_servers (
                    name               TEXT PRIMARY KEY,
                    command            TEXT NOT NULL DEFAULT '',
                    args_json          TEXT NOT NULL DEFAULT '[]',
                    env_json           TEXT NOT NULL DEFAULT '{}',
                    transport          TEXT NOT NULL DEFAULT 'stdio',
                    url                TEXT,
                    requires_approval  INTEGER NOT NULL DEFAULT 0,
                    init_timeout_secs  INTEGER NOT NULL DEFAULT 30,
                    call_timeout_secs  INTEGER NOT NULL DEFAULT 60,
                    tags_json          TEXT NOT NULL DEFAULT '[]',
                    enabled            INTEGER NOT NULL DEFAULT 1,
                    created_at         TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z',
                    updated_at         TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z'
                );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO mcp_servers (name, command, requires_approval, call_timeout_secs)
                 VALUES ('crm', 'uvx', 1, 75)",
                [],
            )
            .unwrap();
        }

        // WHEN the repository opens the db
        let repo = McpServerRepository::open(&path).unwrap();

        // THEN the row reads back whole, the column is gone, and a save
        //      writes the server again
        let found = repo.find_by_name("crm").unwrap().unwrap();
        assert_eq!(found.command, "uvx");
        assert_eq!(found.call_timeout_secs, 75);
        let columns: Vec<String> = repo
            .conn
            .prepare("SELECT name FROM pragma_table_info('mcp_servers')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!columns.iter().any(|c| c == "requires_approval"));
        repo.save(&found).unwrap();
        assert_eq!(repo.list().unwrap().len(), 1);
    }

    #[test]
    fn test_save_updates_existing_record() {
        // GIVEN a server already saved
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        repo.save(&stdio_config("notion")).unwrap();

        // WHEN saved again with a different command
        let mut updated = stdio_config("notion");
        updated.command = "uvx".to_string();
        repo.save(&updated).unwrap();

        // THEN only one row exists and the command is updated
        let servers = repo.list().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].command, "uvx");
    }

    fn origin(provenance: Provenance, official: bool) -> ConnectionOrigin {
        ConnectionOrigin {
            provenance,
            official,
            entry: None,
        }
    }

    #[test]
    fn test_a_save_without_origin_keeps_the_stored_one() {
        // GIVEN a server added from the selection, marked official
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let mut config = stdio_config("notion");
        config.origin = Some(origin(Provenance::Selection, true));
        repo.save(&config).unwrap();

        // WHEN it is saved again by a caller that does not know its origin
        let mut edited = stdio_config("notion");
        edited.command = "uvx".to_string();
        repo.save(&edited).unwrap();

        // THEN the provenance and the official flag survive the edit
        let found = repo.find_by_name("notion").unwrap().unwrap();
        assert_eq!(found.command, "uvx");
        assert_eq!(found.origin, Some(origin(Provenance::Selection, true)));
    }

    #[test]
    fn test_a_save_with_origin_replaces_the_stored_one() {
        // GIVEN a server stored as official from the selection
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let mut config = stdio_config("notion");
        config.origin = Some(origin(Provenance::Selection, true));
        repo.save(&config).unwrap();

        // WHEN an edit points it at a command of the user's own
        config.origin = Some(origin(Provenance::Custom, false));
        repo.save(&config).unwrap();

        // THEN the new origin wins, official flag included
        let found = repo.find_by_name("notion").unwrap().unwrap();
        assert_eq!(found.origin, Some(origin(Provenance::Custom, false)));
    }

    #[test]
    fn test_set_origin_if_missing_writes_only_a_row_without_one() {
        // GIVEN one server without an origin and one with
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        repo.save(&stdio_config("legacy")).unwrap();
        let mut known = stdio_config("known");
        known.origin = Some(origin(Provenance::Registry, false));
        repo.save(&known).unwrap();

        // WHEN both are given the selection as origin
        let wrote_legacy = repo
            .set_origin_if_missing("legacy", origin(Provenance::Selection, false))
            .unwrap();
        let wrote_known = repo
            .set_origin_if_missing("known", origin(Provenance::Selection, false))
            .unwrap();

        // THEN only the server without one changes
        assert!(wrote_legacy);
        assert!(!wrote_known);
        let legacy = repo.find_by_name("legacy").unwrap().unwrap();
        assert_eq!(legacy.origin, Some(origin(Provenance::Selection, false)));
        let known = repo.find_by_name("known").unwrap().unwrap();
        assert_eq!(known.origin, Some(origin(Provenance::Registry, false)));
    }

    #[test]
    fn test_origin_columns_backfill_on_a_v2_db() {
        // GIVEN a db written at schema version 2, before the origin columns
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        {
            let conn = Connection::open(&path).unwrap();
            migrate_v1(&conn).unwrap();
            migrate_v2(&conn).unwrap();
            conn.execute_batch("PRAGMA user_version = 2;").unwrap();
            conn.execute(
                "INSERT INTO mcp_servers (name, command) VALUES ('legacy', 'npx')",
                [],
            )
            .unwrap();
        }

        // WHEN the repository opens it
        let repo = McpServerRepository::open(&path).unwrap();

        // THEN the existing row reads back with no origin
        let found = repo.find_by_name("legacy").unwrap().unwrap();
        assert_eq!(found.origin, None);
    }

    #[test]
    fn test_selection_entry_round_trips_and_follows_the_origin() {
        // GIVEN a server saved from a selection entry
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let mut config = stdio_config("notion");
        config.origin = Some(ConnectionOrigin {
            entry: Some("@notionhq/notion-mcp-server".into()),
            ..origin(Provenance::Selection, true)
        });
        repo.save(&config).unwrap();

        // WHEN it is saved again by a caller that does not know its origin
        let mut unknown = stdio_config("notion");
        unknown.origin = None;
        repo.save(&unknown).unwrap();

        // THEN the selection entry is kept with the rest of the origin
        let found = repo.find_by_name("notion").unwrap().unwrap();
        assert_eq!(found.origin, config.origin);

        // WHEN an edit gives it an origin of the user's own
        let mut custom = stdio_config("notion");
        custom.origin = Some(origin(Provenance::Custom, false));
        repo.save(&custom).unwrap();

        // THEN the selection entry goes with the selection origin
        let found = repo.find_by_name("notion").unwrap().unwrap();
        assert_eq!(found.origin, Some(origin(Provenance::Custom, false)));
    }

    #[test]
    fn test_set_selection_entry_if_missing_writes_only_a_selection_row_without_one() {
        // GIVEN a selection server without an entry, a custom one, and a
        // selection server that has one
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        for (name, provenance, entry) in [
            ("bare", Provenance::Selection, None),
            ("mine", Provenance::Custom, None),
            ("named", Provenance::Selection, Some("first")),
        ] {
            let mut config = stdio_config(name);
            config.origin = Some(ConnectionOrigin {
                entry: entry.map(str::to_owned),
                ..origin(provenance, false)
            });
            repo.save(&config).unwrap();
        }

        // WHEN each is given an entry
        let wrote: Vec<bool> = ["bare", "mine", "named"]
            .iter()
            .map(|name| repo.set_selection_entry_if_missing(name, "second").unwrap())
            .collect();

        // THEN only the selection server without one changes
        assert_eq!(wrote, vec![true, false, false]);
        let entry = |name: &str| {
            repo.find_by_name(name)
                .unwrap()
                .unwrap()
                .origin
                .and_then(|o| o.entry)
        };
        assert_eq!(entry("bare").as_deref(), Some("second"));
        assert_eq!(entry("mine"), None);
        assert_eq!(entry("named").as_deref(), Some("first"));
    }

    #[test]
    fn test_selection_entry_column_is_added_to_a_v3_db() {
        // GIVEN a db written at schema version 3, before the selection entry
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        {
            let conn = Connection::open(&path).unwrap();
            migrate_v1(&conn).unwrap();
            migrate_v2(&conn).unwrap();
            migrate_v3(&conn).unwrap();
            conn.execute_batch("PRAGMA user_version = 3;").unwrap();
            conn.execute(
                "INSERT INTO mcp_servers (name, command, provenance) \
                 VALUES ('legacy', 'npx', 'selection')",
                [],
            )
            .unwrap();
        }

        // WHEN the repository opens it
        let repo = McpServerRepository::open(&path).unwrap();

        // THEN the row keeps its origin and has no selection entry yet
        let found = repo.find_by_name("legacy").unwrap().unwrap();
        assert_eq!(found.origin, Some(origin(Provenance::Selection, false)));
    }
}
