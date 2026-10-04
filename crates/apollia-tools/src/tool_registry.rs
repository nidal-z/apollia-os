//! Runtime governance of native tools: enable/disable and secrets.
//!
//! This module exposes two components persisted in `governance.db`:
//!
//! - [`ToolRegistry`]: `enabled`/`disabled` state and per-tool JSON configuration.
//!   Activation rule: absent from the `tools` table OR `enabled = TRUE` means
//!   active; only `enabled = FALSE` disables. The table is purely an exception
//!   list; unknown tools stay active by default.
//! - [`ToolCredentialStore`](crate::ToolCredentialStore): the index of the
//!   per-tool secrets (for example `web_search/brave.api_key`), whose values
//!   live in the secret store (see [`crate::tool_credentials`]).
//!
//! Both components share the store but own their own SQLite connection: they are
//! independent and can live in separate actors.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::tool_credentials::ToolCredentialStore;

/// List of native tools known to the runtime, used by [`ToolRegistry::list`]
/// to produce a status even when no entry exists in the store.
///
/// Any change to [`crate::native_dispatcher::build_native_dispatcher`] must be
/// mirrored here to stay consistent.
pub const NATIVE_TOOL_NAMES: &[&str] = &[
    "bash_executor",
    "python_executor",
    "file_read",
    "file_write",
    "file_list",
    "file_edit",
    "file_glob",
    "file_grep",
    "notebook_read",
    "notebook_edit",
    "http_fetch",
    "web_search",
    "web_read",
    "memory_search",
    "source_search",
    "ask_user",
];

/// Credential-store namespace for secrets an agent declares in its manifest.
/// All agents share this single namespace; the `AgentSecrets` interface in the
/// Python bridge resolves lookups under the same name. It is a valid credential
/// target alongside [`NATIVE_TOOL_NAMES`], so operators can provision an agent
/// secret from the CLI (`apollia-os tools credentials set agent <key>`) as well
/// as from the desktop credential manager.
pub const AGENT_CREDENTIALS_NAMESPACE: &str = "agent";

/// Error returned by [`ToolRegistry`] and [`ToolCredentialStore`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolGovernanceError {
    /// SQLite error during a governance query.
    #[error("governance database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// The schema of `governance.db` could not be brought to the supported
    /// version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
    /// The secret store holding the credential values failed.
    #[error("secret store error: {0}")]
    Secrets(#[from] apollia_auth::AuthError),
    /// JSON serialization of the tool configuration failed.
    #[error("invalid tool config JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

/// Snapshot of a tool's state as presented by [`ToolRegistry::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    /// Canonical tool name (e.g. `bash_executor`).
    pub name: String,
    /// `true` if the tool is active. See the module activation rule.
    pub enabled: bool,
    /// Tool-specific JSON configuration, `None` if undefined.
    pub config: Option<serde_json::Value>,
    /// Unix timestamp (seconds) of the last modification of the matching
    /// `tools` row. `0` when the tool has no entry.
    pub updated_at: i64,
}

/// Persisted registry of enabled/disabled tools and their JSON config.
pub struct ToolRegistry {
    conn: Connection,
}

impl ToolRegistry {
    /// Opens the `governance.db` store at *db_path* read/write and returns the
    /// registry.
    ///
    /// The `tools` table must already exist (see
    /// [`crate::governance_db::GovernanceDb`]).
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if SQLite fails to open the
    /// store.
    pub fn new(db_path: &Path) -> Result<Self, ToolGovernanceError> {
        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    /// Reports whether tool *tool_name* is active.
    ///
    /// A tool absent from the `tools` table is considered active by default; a
    /// tool with `enabled = FALSE` is inactive.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the query fails.
    pub fn is_enabled(&self, tool_name: &str) -> Result<bool, ToolGovernanceError> {
        let row = self
            .conn
            .query_row(
                "SELECT enabled FROM tools WHERE name = ?1",
                params![tool_name],
                |r| r.get::<_, bool>(0),
            )
            .optional()?;
        Ok(row.unwrap_or(true))
    }

    /// Enables or disables *tool_name*.
    ///
    /// The write is an atomic upsert: the existing row is updated if present,
    /// otherwise inserted. `updated_at` is set to `unixepoch()`.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the query fails.
    pub fn set_enabled(
        &mut self,
        tool_name: &str,
        enabled: bool,
    ) -> Result<(), ToolGovernanceError> {
        self.conn.execute(
            "INSERT INTO tools (name, enabled, config_json, updated_at)
             VALUES (?1, ?2, NULL, unixepoch())
             ON CONFLICT(name) DO UPDATE SET enabled = excluded.enabled, updated_at = unixepoch()",
            params![tool_name, enabled],
        )?;
        Ok(())
    }

    /// Returns the JSON configuration stored for *tool_name*, or `None`.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the read fails or
    /// [`ToolGovernanceError::InvalidJson`] if the stored JSON is malformed.
    pub fn get_config(
        &self,
        tool_name: &str,
    ) -> Result<Option<serde_json::Value>, ToolGovernanceError> {
        let row: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT config_json FROM tools WHERE name = ?1",
                params![tool_name],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?;
        match row.flatten() {
            None => Ok(None),
            Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        }
    }

    /// Stores the JSON configuration associated with *tool_name*.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the write fails or
    /// [`ToolGovernanceError::InvalidJson`] if the value cannot be serialized.
    pub fn set_config(
        &mut self,
        tool_name: &str,
        cfg: &serde_json::Value,
    ) -> Result<(), ToolGovernanceError> {
        let raw = serde_json::to_string(cfg)?;
        self.conn.execute(
            "INSERT INTO tools (name, enabled, config_json, updated_at)
             VALUES (?1, TRUE, ?2, unixepoch())
             ON CONFLICT(name) DO UPDATE SET config_json = excluded.config_json, updated_at = unixepoch()",
            params![tool_name, raw],
        )?;
        Ok(())
    }

    /// Lists the union of registered tools and known native tools.
    ///
    /// For a tool with no entry in the store, the returned status has
    /// `enabled = true`, `config = None` and `updated_at = 0`.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the read fails or
    /// [`ToolGovernanceError::InvalidJson`] if a stored config is malformed.
    pub fn list(&self) -> Result<Vec<ToolStatus>, ToolGovernanceError> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, enabled, config_json, updated_at FROM tools ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let enabled: bool = row.get(1)?;
            let raw: Option<String> = row.get(2)?;
            let updated_at: i64 = row.get(3)?;
            Ok((name, enabled, raw, updated_at))
        })?;

        let mut by_name: std::collections::BTreeMap<String, ToolStatus> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (name, enabled, raw, updated_at) = row?;
            let config = match raw {
                Some(s) => Some(serde_json::from_str(&s)?),
                None => None,
            };
            by_name.insert(
                name.clone(),
                ToolStatus {
                    name,
                    enabled,
                    config,
                    updated_at,
                },
            );
        }

        for native in NATIVE_TOOL_NAMES {
            by_name.entry((*native).to_string()).or_insert(ToolStatus {
                name: (*native).to_string(),
                enabled: true,
                config: None,
                updated_at: 0,
            });
        }

        Ok(by_name.into_values().collect())
    }
}

/// Snapshot read from `governance.db`, used to configure a
/// [`crate::native_dispatcher::build_native_dispatcher`].
#[derive(Debug, Clone, Default)]
pub struct GovernanceSnapshot {
    /// List of tools with `enabled = FALSE` at snapshot time.
    pub disabled_tools: Vec<String>,
    /// Brave Search API key, if `tool_credentials` names it and the secret
    /// store holds it.
    pub brave_api_key: Option<String>,
}

/// Reads `governance.db` in *base_dir*, and the Brave key from the secret
/// store selected by `APOLLIA_TOKEN_STORAGE`, to produce a
/// [`GovernanceSnapshot`] consumable by the dispatcher.
///
/// If the store does not exist, an empty snapshot is returned (all tools stay
/// active, no Brave key). This tolerance lets the runtime work before the first
/// write.
///
/// # Errors
///
/// Surfaces the SQLite or secret-store errors encountered while reading when
/// the store exists but is not usable.
pub fn load_governance_snapshot(
    base_dir: &Path,
) -> Result<GovernanceSnapshot, ToolGovernanceError> {
    let db_path = base_dir.join(crate::governance_db::GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return Ok(GovernanceSnapshot::default());
    }
    load_governance_snapshot_with(base_dir, apollia_auth::select_secret_store()?)
}

/// [`load_governance_snapshot`] over an explicit secret store.
///
/// # Errors
///
/// As [`load_governance_snapshot`].
pub fn load_governance_snapshot_with(
    base_dir: &Path,
    secrets: Box<dyn apollia_auth::SecretStore>,
) -> Result<GovernanceSnapshot, ToolGovernanceError> {
    let db_path = base_dir.join(crate::governance_db::GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return Ok(GovernanceSnapshot::default());
    }

    let registry = ToolRegistry::new(&db_path)?;
    let disabled_tools = registry
        .list()?
        .into_iter()
        .filter(|s| !s.enabled)
        .map(|s| s.name)
        .collect();

    let store = ToolCredentialStore::new(&db_path, secrets)?;
    let brave_api_key = store.get("web_search", "brave.api_key")?;

    Ok(GovernanceSnapshot {
        disabled_tools,
        brave_api_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance_db::GovernanceDb;
    use crate::tool_credentials::test_support::MemorySecrets;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fresh(dir: &TempDir) -> PathBuf {
        let _ = GovernanceDb::open(dir.path()).expect("init governance.db");
        dir.path()
            .join(crate::governance_db::GOVERNANCE_DB_FILENAME)
    }

    #[test]
    fn test_tool_enabled_by_default_if_absent() {
        // GIVEN a store with no entry for `web_search`.
        let dir = TempDir::new().expect("tempdir");
        let db = fresh(&dir);
        let reg = ToolRegistry::new(&db).expect("open registry");
        // WHEN the state is queried.
        let enabled = reg.is_enabled("web_search").expect("query");
        // THEN the tool is considered active.
        assert!(enabled);
    }

    #[test]
    fn test_set_enabled_disables_tool() {
        // GIVEN a blank registry.
        let dir = TempDir::new().expect("tempdir");
        let db = fresh(&dir);
        let mut reg = ToolRegistry::new(&db).expect("open");
        // WHEN bash_executor is disabled.
        reg.set_enabled("bash_executor", false).expect("disable");
        // THEN is_enabled returns false; a new tool stays active.
        assert!(!reg.is_enabled("bash_executor").expect("read"));
        assert!(reg.is_enabled("file_read").expect("read other"));
    }

    #[test]
    fn test_list_unions_native_and_db() {
        // GIVEN a registry where only bash_executor is disabled.
        let dir = TempDir::new().expect("tempdir");
        let db = fresh(&dir);
        let mut reg = ToolRegistry::new(&db).expect("open");
        reg.set_enabled("bash_executor", false).expect("disable");
        // WHEN listing.
        let entries = reg.list().expect("list");
        // THEN all native tools appear and bash_executor is inactive.
        let bash = entries
            .iter()
            .find(|e| e.name == "bash_executor")
            .expect("bash present");
        assert!(!bash.enabled);
        for native in NATIVE_TOOL_NAMES {
            assert!(
                entries.iter().any(|e| e.name == *native),
                "native tool {native} must be listed"
            );
        }
    }

    #[test]
    fn test_set_get_config_roundtrip() {
        // GIVEN a blank registry.
        let dir = TempDir::new().expect("tempdir");
        let db = fresh(&dir);
        let mut reg = ToolRegistry::new(&db).expect("open");
        // WHEN the web_search config is stored then read.
        let cfg = serde_json::json!({"default_backend": "duckduckgo"});
        reg.set_config("web_search", &cfg).expect("set");
        let read = reg.get_config("web_search").expect("get");
        // THEN the read value is identical.
        assert_eq!(read, Some(cfg));
    }

    #[test]
    fn test_snapshot_reads_disabled_and_credential() {
        // GIVEN a store with bash_executor disabled and a Brave key stored.
        let dir = TempDir::new().expect("tempdir");
        let db = fresh(&dir);
        let secrets = MemorySecrets::default();
        {
            let mut reg = ToolRegistry::new(&db).expect("open reg");
            reg.set_enabled("bash_executor", false).expect("disable");
            let mut store =
                ToolCredentialStore::new(&db, Box::new(secrets.clone())).expect("open store");
            store
                .set("web_search", "brave.api_key", "BSA-snapshot")
                .expect("set");
        }
        // WHEN the snapshot is loaded.
        let snap = load_governance_snapshot_with(dir.path(), Box::new(secrets)).expect("snapshot");
        // THEN the disabled tool and the Brave key appear.
        assert!(snap.disabled_tools.iter().any(|n| n == "bash_executor"));
        assert_eq!(snap.brave_api_key.as_deref(), Some("BSA-snapshot"));
    }
}
