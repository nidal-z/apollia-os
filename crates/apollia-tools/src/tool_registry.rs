//! Runtime governance of native tools: enable/disable and secrets.
//!
//! This module exposes two components persisted in `governance.db`:
//!
//! - [`ToolRegistry`]: `enabled`/`disabled` state and per-tool JSON configuration.
//!   Activation rule: absent from the `tools` table OR `enabled = TRUE` means
//!   active; only `enabled = FALSE` disables. The table is purely an exception
//!   list; unknown tools stay active by default.
//! - [`ToolCredentialStore`]: per-tool secrets (for example
//!   `web_search/brave.api_key`), AES-256-GCM encrypted with a 32-byte master key
//!   stored in a `~/.apollia/.keyfile` file (chmod 600). The 12-byte nonce is
//!   randomly generated per insert and prefixed to the ciphertext in the store.
//!
//! Both components share the store but own their own SQLite connection: they are
//! independent and can live in separate actors.

use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

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
    "ask_user",
    // Agent-driven permission governance.
    "permission_rule_add",
    "permission_rule_remove",
    "permission_rule_list",
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
pub enum ToolGovernanceError {
    /// SQLite error during a governance query.
    #[error("governance database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// I/O error while reading/writing the `.keyfile` file.
    #[error("keyfile I/O error at {path}: {source}")]
    Keyfile {
        /// Path of the `.keyfile`.
        path: PathBuf,
        /// Underlying cause.
        #[source]
        source: std::io::Error,
    },
    /// The master key read from `.keyfile` does not have the expected size.
    #[error("keyfile is corrupted: expected 32 bytes, found {found}")]
    KeyfileCorrupted {
        /// Observed size.
        found: usize,
    },
    /// The stored value is too short to hold a nonce + ciphertext.
    #[error("encrypted value is corrupted (too short)")]
    CiphertextCorrupted,
    /// JSON serialization of the tool configuration failed.
    #[error("invalid tool config JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    /// AES-256-GCM decryption failed (wrong key or tampered ciphertext).
    #[error("decryption failed (wrong key or tampered ciphertext)")]
    DecryptFailed,
    /// AES-256-GCM could not produce the ciphertext.
    #[error("encryption failed")]
    EncryptFailed,
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

/// An entry of [`ToolCredentialStore::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialEntry {
    /// Name of the tool that owns the credential.
    pub tool_name: String,
    /// Logical key name (e.g. `brave.api_key`).
    pub key_name: String,
    /// Unix timestamp (seconds) of creation.
    pub created_at: i64,
    /// Unix timestamp (seconds) of the last effective use, if any.
    pub last_used_at: Option<i64>,
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

/// Encrypted per-tool credential store (AES-256-GCM).
///
/// Each value stored in the store is `nonce(12) || ciphertext`; the GCM
/// authentication tag is included in the ciphertext by the `aes-gcm` crate.
/// The master key is read from a dedicated file, created with `chmod 600` on
/// the first call if it does not exist.
pub struct ToolCredentialStore {
    conn: Connection,
    cipher: Aes256Gcm,
}

impl ToolCredentialStore {
    /// Opens the store read/write on *db_path* using the master key stored in
    /// *keyfile_path*.
    ///
    /// The `.keyfile` is created (mode `0o600`) with a random 32-byte key if it
    /// does not exist. If it exists, its contents must be exactly 32 bytes.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::Database`] if SQLite fails.
    /// - [`ToolGovernanceError::Keyfile`] if reading/writing the file fails.
    /// - [`ToolGovernanceError::KeyfileCorrupted`] if the key does not have the
    ///   expected size.
    pub fn new(db_path: &Path, keyfile_path: &Path) -> Result<Self, ToolGovernanceError> {
        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        let key_bytes = load_or_create_keyfile(keyfile_path)?;
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        Ok(Self { conn, cipher })
    }

    /// Stores (inserts or replaces) the `(tool, key)` credential.
    ///
    /// A new 12-byte nonce is generated on every call.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::EncryptFailed`] if AES-256-GCM fails
    ///   (practically impossible with the `aes-gcm` crate).
    /// - [`ToolGovernanceError::Database`] if the SQLite write fails.
    pub fn set(&mut self, tool: &str, key: &str, value: &str) -> Result<(), ToolGovernanceError> {
        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, value.as_bytes())
            .map_err(|_| ToolGovernanceError::EncryptFailed)?;

        let mut blob = Vec::with_capacity(12 + ciphertext.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);

        self.conn.execute(
            "INSERT INTO tool_credentials (tool_name, key_name, value_encrypted, created_at)
             VALUES (?1, ?2, ?3, unixepoch())
             ON CONFLICT(tool_name, key_name) DO UPDATE SET
               value_encrypted = excluded.value_encrypted,
               created_at      = unixepoch(),
               last_used_at    = NULL",
            params![tool, key, blob],
        )?;
        Ok(())
    }

    /// Fetches the cleartext value for `(tool, key)`, or `None` if the
    /// credential does not exist.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::CiphertextCorrupted`] if the stored value is too
    ///   short.
    /// - [`ToolGovernanceError::DecryptFailed`] if AES-256-GCM rejects the
    ///   authentication tag.
    /// - [`ToolGovernanceError::Database`] if the SQLite read fails.
    pub fn get(&self, tool: &str, key: &str) -> Result<Option<String>, ToolGovernanceError> {
        let row: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT value_encrypted FROM tool_credentials WHERE tool_name = ?1 AND key_name = ?2",
                params![tool, key],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        let blob = match row {
            Some(b) => b,
            None => return Ok(None),
        };
        if blob.len() < 12 {
            return Err(ToolGovernanceError::CiphertextCorrupted);
        }
        let (nonce_bytes, ciphertext) = blob.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| ToolGovernanceError::DecryptFailed)?;
        let s = String::from_utf8(plaintext).map_err(|_| ToolGovernanceError::DecryptFailed)?;
        Ok(Some(s))
    }

    /// Deletes the `(tool, key)` credential. Returns `true` if a row was erased.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if SQLite fails.
    pub fn delete(&mut self, tool: &str, key: &str) -> Result<bool, ToolGovernanceError> {
        let n = self.conn.execute(
            "DELETE FROM tool_credentials WHERE tool_name = ?1 AND key_name = ?2",
            params![tool, key],
        )?;
        Ok(n > 0)
    }

    /// Lists credentials, filtered by tool if *tool* is `Some`.
    ///
    /// Encrypted values are never returned: only the metadata is, which lets a
    /// "credential present" state be displayed without exposing the secret.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the query fails.
    pub fn list(&self, tool: Option<&str>) -> Result<Vec<CredentialEntry>, ToolGovernanceError> {
        let mut entries = Vec::new();
        match tool {
            Some(t) => {
                let mut stmt = self.conn.prepare(
                    "SELECT tool_name, key_name, created_at, last_used_at \
                     FROM tool_credentials WHERE tool_name = ?1 ORDER BY key_name",
                )?;
                let rows = stmt.query_map(params![t], map_credential_row)?;
                for row in rows {
                    entries.push(row?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT tool_name, key_name, created_at, last_used_at \
                     FROM tool_credentials ORDER BY tool_name, key_name",
                )?;
                let rows = stmt.query_map([], map_credential_row)?;
                for row in rows {
                    entries.push(row?);
                }
            }
        }
        Ok(entries)
    }

    /// Updates `last_used_at` for the `(tool, key)` credential.
    ///
    /// No-op if the credential does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the write fails.
    pub fn touch_last_used(&mut self, tool: &str, key: &str) -> Result<(), ToolGovernanceError> {
        self.conn.execute(
            "UPDATE tool_credentials SET last_used_at = unixepoch() \
             WHERE tool_name = ?1 AND key_name = ?2",
            params![tool, key],
        )?;
        Ok(())
    }
}

fn map_credential_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CredentialEntry> {
    Ok(CredentialEntry {
        tool_name: row.get(0)?,
        key_name: row.get(1)?,
        created_at: row.get(2)?,
        last_used_at: row.get(3)?,
    })
}

fn load_or_create_keyfile(path: &Path) -> Result<[u8; 32], ToolGovernanceError> {
    if path.exists() {
        let bytes = std::fs::read(path).map_err(|e| ToolGovernanceError::Keyfile {
            path: path.to_path_buf(),
            source: e,
        })?;
        if bytes.len() != 32 {
            return Err(ToolGovernanceError::KeyfileCorrupted { found: bytes.len() });
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        return Ok(out);
    }

    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| ToolGovernanceError::Keyfile {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
    }

    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    write_keyfile_secure(path, &key)?;
    Ok(key)
}

#[cfg(unix)]
fn write_keyfile_secure(path: &Path, key: &[u8; 32]) -> Result<(), ToolGovernanceError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| ToolGovernanceError::Keyfile {
            path: path.to_path_buf(),
            source: e,
        })?;
    use std::io::Write;
    file.write_all(key)
        .map_err(|e| ToolGovernanceError::Keyfile {
            path: path.to_path_buf(),
            source: e,
        })?;
    file.sync_all().map_err(|e| ToolGovernanceError::Keyfile {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn write_keyfile_secure(path: &Path, key: &[u8; 32]) -> Result<(), ToolGovernanceError> {
    std::fs::write(path, key).map_err(|e| ToolGovernanceError::Keyfile {
        path: path.to_path_buf(),
        source: e,
    })
}

/// Snapshot read from `governance.db`, used to configure a
/// [`crate::native_dispatcher::build_native_dispatcher`].
#[derive(Debug, Clone, Default)]
pub struct GovernanceSnapshot {
    /// List of tools with `enabled = FALSE` at snapshot time.
    pub disabled_tools: Vec<String>,
    /// Decrypted Brave Search API key, if present in `tool_credentials`.
    pub brave_api_key: Option<String>,
}

/// Reads `governance.db` and `.keyfile` in *base_dir* to produce a
/// [`GovernanceSnapshot`] consumable by the dispatcher.
///
/// If the store or the `.keyfile` does not exist, an empty snapshot is returned
/// (all tools stay active, no Brave key). This tolerance lets the runtime work
/// before the first write.
///
/// # Errors
///
/// Surfaces the SQLite or cryptographic errors encountered while reading when
/// the store exists but is not usable.
pub fn load_governance_snapshot(
    base_dir: &Path,
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

    let keyfile = base_dir.join(".keyfile");
    let store = ToolCredentialStore::new(&db_path, &keyfile)?;
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
    use rusqlite::params;
    use tempfile::TempDir;

    fn fresh(dir: &TempDir) -> (PathBuf, PathBuf) {
        let _ = GovernanceDb::open(dir.path()).expect("init governance.db");
        (
            dir.path()
                .join(crate::governance_db::GOVERNANCE_DB_FILENAME),
            dir.path().join(".keyfile"),
        )
    }

    #[test]
    fn test_tool_enabled_by_default_if_absent() {
        // GIVEN a store with no entry for `web_search`.
        let dir = TempDir::new().expect("tempdir");
        let (db, _) = fresh(&dir);
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
        let (db, _) = fresh(&dir);
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
        let (db, _) = fresh(&dir);
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
        let (db, _) = fresh(&dir);
        let mut reg = ToolRegistry::new(&db).expect("open");
        // WHEN the web_search config is stored then read.
        let cfg = serde_json::json!({"default_backend": "duckduckgo"});
        reg.set_config("web_search", &cfg).expect("set");
        let read = reg.get_config("web_search").expect("get");
        // THEN the read value is identical.
        assert_eq!(read, Some(cfg));
    }

    #[test]
    fn test_credential_roundtrip_encrypt_decrypt() {
        // GIVEN a freshly created store.
        let dir = TempDir::new().expect("tempdir");
        let (db, kf) = fresh(&dir);
        let mut store = ToolCredentialStore::new(&db, &kf).expect("open store");
        // WHEN the value is stored then read back.
        store
            .set("web_search", "brave.api_key", "BSA-secret-1234")
            .expect("set");
        let read = store.get("web_search", "brave.api_key").expect("get");
        // THEN the cleartext value is identical.
        assert_eq!(read.as_deref(), Some("BSA-secret-1234"));
    }

    #[test]
    fn test_credential_not_in_plaintext_in_db() {
        // GIVEN a stored credential.
        let dir = TempDir::new().expect("tempdir");
        let (db, kf) = fresh(&dir);
        {
            let mut store = ToolCredentialStore::new(&db, &kf).expect("open");
            store
                .set("web_search", "brave.api_key", "PLAINTEXT-MARKER-XYZ")
                .expect("set");
        }
        // WHEN the raw BLOB is read directly in SQL.
        let conn = Connection::open(&db).expect("open raw");
        let blob: Vec<u8> = conn
            .query_row(
                "SELECT value_encrypted FROM tool_credentials WHERE tool_name='web_search' AND key_name='brave.api_key'",
                params![],
                |r| r.get(0),
            )
            .expect("read blob");
        // THEN the marker must never appear in cleartext.
        assert!(
            !String::from_utf8_lossy(&blob).contains("PLAINTEXT-MARKER-XYZ"),
            "ciphertext must not leak the plaintext"
        );
        assert!(blob.len() > 12, "blob must contain nonce + ciphertext");
    }

    #[test]
    fn test_credential_delete_and_list() {
        // GIVEN two stored credentials.
        let dir = TempDir::new().expect("tempdir");
        let (db, kf) = fresh(&dir);
        let mut store = ToolCredentialStore::new(&db, &kf).expect("open");
        store.set("web_search", "brave.api_key", "v1").expect("a");
        store.set("http_fetch", "auth.token", "v2").expect("b");

        // WHEN listing filtered, then deleting, then listing again.
        let only_search = store.list(Some("web_search")).expect("list filtered");
        assert_eq!(only_search.len(), 1);
        assert_eq!(only_search[0].key_name, "brave.api_key");

        let removed = store
            .delete("http_fetch", "auth.token")
            .expect("delete existing");
        assert!(removed);
        let missing = store
            .delete("http_fetch", "auth.token")
            .expect("delete missing");
        assert!(!missing);

        let all = store.list(None).expect("list all");
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_snapshot_reads_disabled_and_credential() {
        // GIVEN a store with bash_executor disabled and a Brave key stored.
        let dir = TempDir::new().expect("tempdir");
        let (db, kf) = fresh(&dir);
        {
            let mut reg = ToolRegistry::new(&db).expect("open reg");
            reg.set_enabled("bash_executor", false).expect("disable");
            let mut store = ToolCredentialStore::new(&db, &kf).expect("open store");
            store
                .set("web_search", "brave.api_key", "BSA-snapshot")
                .expect("set");
        }
        // WHEN the snapshot is loaded.
        let snap = load_governance_snapshot(dir.path()).expect("snapshot");
        // THEN the disabled tool and the Brave key appear.
        assert!(snap.disabled_tools.iter().any(|n| n == "bash_executor"));
        assert_eq!(snap.brave_api_key.as_deref(), Some("BSA-snapshot"));
    }
}
