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

/// Errors returned by [`McpServerRepository`].
#[derive(Debug, Error)]
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

    /// A JSON serialisation or deserialisation failed.
    #[error("serialization error: {0}")]
    Serialization(String),
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
        // Additive migration for databases created before max_response_bytes
        // existed. `CREATE TABLE IF NOT EXISTS` above never alters an existing
        // table, so the column is added here. A "duplicate column name" error
        // means the column is already present (fresh DB or prior migration),
        // which is expected; any other error is a real migration failure.
        if let Err(e) = conn.execute_batch(
            "ALTER TABLE mcp_servers ADD COLUMN max_response_bytes INTEGER NOT NULL DEFAULT 8388608;",
        ) {
            if !e.to_string().contains("duplicate column name") {
                return Err(McpRepoError::Db(e));
            }
        }
        // Additive migration for databases created before max_tools existed.
        // Same idempotency contract as the max_response_bytes migration above.
        if let Err(e) = conn.execute_batch(
            "ALTER TABLE mcp_servers ADD COLUMN max_tools INTEGER NOT NULL DEFAULT 256;",
        ) {
            if !e.to_string().contains("duplicate column name") {
                return Err(McpRepoError::Db(e));
            }
        }
        Ok(Self { conn })
    }

    /// Inserts or replaces a server configuration.
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

        self.conn.execute(
            "INSERT INTO mcp_servers
                (name, command, args_json, env_json, transport, url,
                 requires_approval, init_timeout_secs, call_timeout_secs,
                 max_response_bytes, max_tools, tags_json, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1,
                     strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(name) DO UPDATE SET
                command            = excluded.command,
                args_json          = excluded.args_json,
                env_json           = excluded.env_json,
                transport          = excluded.transport,
                url                = excluded.url,
                requires_approval  = excluded.requires_approval,
                init_timeout_secs  = excluded.init_timeout_secs,
                call_timeout_secs  = excluded.call_timeout_secs,
                max_response_bytes = excluded.max_response_bytes,
                max_tools          = excluded.max_tools,
                tags_json          = excluded.tags_json,
                updated_at         = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![
                config.name,
                config.command,
                args_json,
                env_json,
                config.transport,
                config.url,
                config.requires_approval as i64,
                config.init_timeout_secs as i64,
                config.call_timeout_secs as i64,
                config.max_response_bytes as i64,
                config.max_tools as i64,
                tags_json,
            ],
        )?;
        Ok(())
    }

    /// Returns all servers (enabled and disabled).
    pub fn list(&self) -> Result<Vec<McpServerConfig>, McpRepoError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, command, args_json, env_json, transport, url,
                    requires_approval, init_timeout_secs, call_timeout_secs,
                    tags_json, max_response_bytes, max_tools
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
                    requires_approval, init_timeout_secs, call_timeout_secs,
                    tags_json, max_response_bytes, max_tools
             FROM mcp_servers
             WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], row_to_config)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
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

/// Deserialises a single database row into a [`McpServerConfig`].
fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpServerConfig> {
    let args_json: String = row.get(2)?;
    let env_json: String = row.get(3)?;
    let tags_json: String = row.get(9)?;
    let requires_approval: i64 = row.get(6)?;
    let init_timeout_secs: i64 = row.get(7)?;
    let call_timeout_secs: i64 = row.get(8)?;
    let max_response_bytes: i64 = row.get(10)?;
    let max_tools: i64 = row.get(11)?;

    let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
    let env: HashMap<String, String> = serde_json::from_str(&env_json).unwrap_or_default();
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(McpServerConfig {
        format_version: 1,
        name: row.get(0)?,
        command: row.get(1)?,
        args,
        env,
        transport: row.get(4)?,
        url: row.get(5)?,
        requires_approval: requires_approval != 0,
        init_timeout_secs: init_timeout_secs as u64,
        call_timeout_secs: call_timeout_secs as u64,
        max_response_bytes: max_response_bytes as u64,
        max_tools: max_tools as u32,
        tags,
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
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
            tags: vec![],
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
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
            tags: vec![],
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
            requires_approval: true,
            init_timeout_secs: 45,
            call_timeout_secs: 90,
            max_response_bytes: 4 * 1024 * 1024,
            max_tools: 42,
            tags: vec!["productivity".to_string()],
        };

        // WHEN saved and read back
        repo.save(&config).unwrap();
        let found = repo.find_by_name("notion").unwrap().unwrap();

        // THEN all fields are preserved
        assert_eq!(found.name, "notion");
        assert_eq!(found.args, vec!["-y", "@notionhq/notion-mcp-server"]);
        assert_eq!(found.env["NOTION_KEY"], "tok_123");
        assert!(found.requires_approval);
        assert_eq!(found.init_timeout_secs, 45);
        assert_eq!(found.call_timeout_secs, 90);
        assert_eq!(found.max_response_bytes, 4 * 1024 * 1024);
        assert_eq!(found.max_tools, 42);
        assert_eq!(found.tags, vec!["productivity"]);
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
}
