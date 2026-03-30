//! Persistent writes to `mcp.toml`: add, remove, and update `[[servers]]` blocks.
//!
//! [`McpConfigWriter`] is a synchronous, stateless component. Each method loads
//! the current file, applies one mutation in memory, validates, then rewrites the
//! file with a serde TOML roundtrip. Comment preservation is a V2 concern
//! (`toml_edit`); V1 prioritises correctness — the output is always a valid,
//! reloadable TOML file.

use std::path::PathBuf;

use crate::config::{McpConfig, McpConfigError, McpServerConfig};

// ─── public types ─────────────────────────────────────────────────────────────

/// Reads, mutates, and writes `~/.apollia/mcp.toml`.
///
/// All methods are fail-fast: validation runs before any I/O, and no partial
/// write is emitted on error.
pub struct McpConfigWriter {
    path: PathBuf,
}

/// Errors produced by [`McpConfigWriter`] operations.
#[derive(Debug, thiserror::Error)]
pub enum McpConfigWriteError {
    /// The file could not be read or parsed.
    #[error("failed to read mcp.toml: {0}")]
    ReadFailed(String),

    /// The TOML could not be serialized or written to disk.
    #[error("failed to write mcp.toml: {0}")]
    WriteFailed(String),

    /// A server with that name already exists in the file.
    #[error("server '{0}' already exists in mcp.toml")]
    ServerAlreadyExists(String),

    /// No server with that name was found in the file.
    #[error("server '{0}' not found in mcp.toml")]
    ServerNotFound(String),

    /// The provided config failed validation before any write was attempted.
    #[error("config validation failed: {0}")]
    ValidationFailed(#[from] McpConfigError),
}

// ─── implementation ───────────────────────────────────────────────────────────

impl McpConfigWriter {
    /// Create a writer targeting `path`.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Append a new `[[servers]]` entry to `mcp.toml`.
    ///
    /// Validates `config`, checks for name duplicates, then rewrites the file.
    /// Creates the file if it does not yet exist.
    pub fn add_server(&self, config: &McpServerConfig) -> Result<(), McpConfigWriteError> {
        config.validate()?;

        let mut current = self.load_raw()?;

        if current.servers.iter().any(|s| s.name == config.name) {
            return Err(McpConfigWriteError::ServerAlreadyExists(
                config.name.clone(),
            ));
        }

        current.servers.push(config.clone());
        self.persist(&current)
    }

    /// Remove the `[[servers]]` entry with the given `name` from `mcp.toml`.
    ///
    /// Returns [`McpConfigWriteError::ServerNotFound`] when no matching entry exists.
    pub fn remove_server(&self, name: &str) -> Result<(), McpConfigWriteError> {
        let mut current = self.load_raw()?;

        let before = current.servers.len();
        current.servers.retain(|s| s.name != name);

        if current.servers.len() == before {
            return Err(McpConfigWriteError::ServerNotFound(name.to_string()));
        }

        self.persist(&current)
    }

    /// Replace the `[[servers]]` entry named `name` with `updated` in-place.
    ///
    /// Validates `updated` before writing. Returns [`McpConfigWriteError::ServerNotFound`]
    /// when `name` is absent from the file. The server's position in the list is preserved.
    pub fn update_server(
        &self,
        name: &str,
        updated: &McpServerConfig,
    ) -> Result<(), McpConfigWriteError> {
        updated.validate()?;

        let mut current = self.load_raw()?;

        let pos = current
            .servers
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| McpConfigWriteError::ServerNotFound(name.to_string()))?;

        current.servers[pos] = updated.clone();
        self.persist(&current)
    }

    /// Update only the `requires_approval` flag for the server named `name`.
    ///
    /// All other fields are preserved exactly as written in `mcp.toml`. Returns
    /// [`McpConfigWriteError::ServerNotFound`] when `name` is absent from the file.
    pub fn set_server_approval(
        &self,
        name: &str,
        requires_approval: bool,
    ) -> Result<(), McpConfigWriteError> {
        let mut current = self.load_raw()?;
        let pos = current
            .servers
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| McpConfigWriteError::ServerNotFound(name.to_string()))?;
        current.servers[pos].requires_approval = requires_approval;
        self.persist(&current)
    }

    // ── private helpers ──────────────────────────────────────────────────────

    /// Load the current config, returning an empty config if the file is absent.
    ///
    /// All `McpConfigError` variants (read, parse, validate) are mapped to
    /// `ReadFailed` because they all indicate a problem with the *existing* file,
    /// not with the caller-supplied config.
    fn load_raw(&self) -> Result<McpConfig, McpConfigWriteError> {
        McpConfig::load(&self.path).map_err(|e| McpConfigWriteError::ReadFailed(e.to_string()))
    }

    /// Serialize `config` to TOML and write it to disk.
    fn persist(&self, config: &McpConfig) -> Result<(), McpConfigWriteError> {
        let toml_string = toml::to_string_pretty(config)
            .map_err(|e| McpConfigWriteError::WriteFailed(e.to_string()))?;
        std::fs::write(&self.path, toml_string)
            .map_err(|e| McpConfigWriteError::WriteFailed(e.to_string()))
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    fn sample_config(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: "npx".to_string(),
            args: vec!["@example/mcp-server".to_string()],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        }
    }

    fn remote_config(name: &str, url: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: Some(url.to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        }
    }

    #[test]
    fn test_add_server_creates_block() {
        // GIVEN an empty temp file
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());

        // WHEN a server is added
        writer.add_server(&sample_config("sqlite")).unwrap();

        // THEN the file contains exactly that server
        let loaded = McpConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(loaded.servers[0].name, "sqlite");
    }

    #[test]
    fn test_add_two_servers_both_present() {
        // GIVEN an empty temp file
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());

        // WHEN two distinct servers are added
        writer.add_server(&sample_config("notion")).unwrap();
        writer.add_server(&sample_config("sqlite")).unwrap();

        // THEN the file contains both servers
        let loaded = McpConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.servers.len(), 2);
        let names: Vec<&str> = loaded.servers.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"notion"));
        assert!(names.contains(&"sqlite"));
    }

    #[test]
    fn test_add_duplicate_name_fails() {
        // GIVEN a file containing server "notion"
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer.add_server(&sample_config("notion")).unwrap();

        // WHEN a second "notion" entry is added
        let result = writer.add_server(&sample_config("notion"));

        // THEN ServerAlreadyExists is returned
        assert!(matches!(
            result,
            Err(McpConfigWriteError::ServerAlreadyExists(_))
        ));
    }

    #[test]
    fn test_remove_server_removes_block() {
        // GIVEN a file with "notion" and "sqlite"
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer.add_server(&sample_config("notion")).unwrap();
        writer.add_server(&sample_config("sqlite")).unwrap();

        // WHEN "notion" is removed
        writer.remove_server("notion").unwrap();

        // THEN only "sqlite" remains
        let loaded = McpConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(loaded.servers[0].name, "sqlite");
    }

    #[test]
    fn test_remove_nonexistent_server_fails() {
        // GIVEN a file without server "github"
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer.add_server(&sample_config("notion")).unwrap();

        // WHEN "github" is removed
        let result = writer.remove_server("github");

        // THEN ServerNotFound is returned
        assert!(matches!(
            result,
            Err(McpConfigWriteError::ServerNotFound(_))
        ));
    }

    #[test]
    fn test_update_server_replaces_in_place() {
        // GIVEN a file with server "notion" using command "npx"
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer.add_server(&sample_config("notion")).unwrap();
        writer.add_server(&sample_config("sqlite")).unwrap();

        // WHEN "notion" is updated with command "uvx"
        let mut updated = sample_config("notion");
        updated.command = "uvx".to_string();
        writer.update_server("notion", &updated).unwrap();

        // THEN the command is updated and the server count is unchanged
        let loaded = McpConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.servers.len(), 2);
        let notion = loaded.servers.iter().find(|s| s.name == "notion").unwrap();
        assert_eq!(notion.command, "uvx");
    }

    #[test]
    fn test_update_preserves_position() {
        // GIVEN a file with ["notion", "sqlite", "brave"]
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer.add_server(&sample_config("notion")).unwrap();
        writer.add_server(&sample_config("sqlite")).unwrap();
        writer.add_server(&sample_config("brave")).unwrap();

        // WHEN the middle server is updated
        let mut updated = sample_config("sqlite");
        updated.command = "uvx".to_string();
        writer.update_server("sqlite", &updated).unwrap();

        // THEN the order is ["notion", "sqlite", "brave"] with the updated command
        let loaded = McpConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.servers[1].name, "sqlite");
        assert_eq!(loaded.servers[1].command, "uvx");
    }

    #[test]
    fn test_update_nonexistent_server_fails() {
        // GIVEN a file without server "github"
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer.add_server(&sample_config("notion")).unwrap();

        // WHEN "github" is updated
        let result = writer.update_server("github", &sample_config("github"));

        // THEN ServerNotFound is returned
        assert!(matches!(
            result,
            Err(McpConfigWriteError::ServerNotFound(_))
        ));
    }

    #[test]
    fn test_validation_runs_before_write() {
        // GIVEN a config with an empty name (invalid)
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        let invalid = McpServerConfig {
            name: String::new(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };

        // WHEN add_server is called with the invalid config
        let result = writer.add_server(&invalid);

        // THEN ValidationFailed is returned and the file is not modified
        assert!(matches!(
            result,
            Err(McpConfigWriteError::ValidationFailed(_))
        ));
        let loaded = McpConfig::load(tmp.path()).unwrap();
        assert!(loaded.servers.is_empty());
    }

    #[test]
    fn test_roundtrip_toml_valid() {
        // GIVEN a file modified by add, add, add, then remove
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer.add_server(&sample_config("notion")).unwrap();
        writer.add_server(&sample_config("sqlite")).unwrap();
        writer.add_server(&sample_config("brave")).unwrap();
        writer.remove_server("sqlite").unwrap();

        // WHEN the file is reloaded
        let loaded = McpConfig::load(tmp.path()).unwrap();

        // THEN parsing succeeds and the remaining servers are correct
        assert_eq!(loaded.servers.len(), 2);
        let names: Vec<&str> = loaded.servers.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"notion"));
        assert!(names.contains(&"brave"));
        assert!(!names.contains(&"sqlite"));
    }

    #[test]
    fn test_config_writer_writes_remote_server() {
        // GIVEN a remote server with transport=streamable-http
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());

        // WHEN it is written to disk
        writer
            .add_server(&remote_config("notion", "https://mcp.notion.com/mcp"))
            .unwrap();

        // THEN the TOML contains url and transport but omits command and args
        let content = std::fs::read_to_string(tmp.path()).unwrap();
        assert!(
            content.contains("https://mcp.notion.com/mcp"),
            "url must be present in TOML"
        );
        assert!(
            content.contains("streamable-http"),
            "transport must be present in TOML"
        );
        assert!(
            !content.contains("command"),
            "command must be absent for remote servers"
        );
        assert!(
            !content.contains("args"),
            "args must be absent for remote servers"
        );
    }

    #[test]
    fn test_config_writer_remote_server_round_trip() {
        // GIVEN a remote server written via the writer
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer
            .add_server(&remote_config("notion", "https://mcp.notion.com/mcp"))
            .unwrap();

        // WHEN the file is reloaded and validated
        let loaded = McpConfig::load(tmp.path()).unwrap();

        // THEN the config faithfully represents the original remote server
        assert_eq!(loaded.servers.len(), 1);
        let server = &loaded.servers[0];
        assert_eq!(server.name, "notion");
        assert_eq!(server.transport, "streamable-http");
        assert_eq!(server.url, Some("https://mcp.notion.com/mcp".to_string()));
        assert!(server.command.is_empty());
        assert!(server.args.is_empty());
    }

    #[test]
    fn test_config_writer_mixed_stdio_and_remote() {
        // GIVEN a file containing both a stdio and a remote server
        let tmp = NamedTempFile::new().unwrap();
        let writer = McpConfigWriter::new(tmp.path().to_path_buf());
        writer.add_server(&sample_config("sqlite")).unwrap();
        writer
            .add_server(&remote_config("notion", "https://mcp.notion.com/mcp"))
            .unwrap();

        // WHEN the file is reloaded
        let loaded = McpConfig::load(tmp.path()).unwrap();

        // THEN both servers are present with their respective transport config intact
        assert_eq!(loaded.servers.len(), 2);
        let sqlite = loaded.servers.iter().find(|s| s.name == "sqlite").unwrap();
        assert_eq!(sqlite.transport, "stdio");
        assert_eq!(sqlite.command, "npx");
        assert!(sqlite.url.is_none());

        let notion = loaded.servers.iter().find(|s| s.name == "notion").unwrap();
        assert_eq!(notion.transport, "streamable-http");
        assert_eq!(notion.url, Some("https://mcp.notion.com/mcp".to_string()));
        assert!(notion.command.is_empty());
    }
}
