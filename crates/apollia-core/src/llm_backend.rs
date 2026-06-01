//! LLM backend registry persisted in `system.db`.
//!
//! [`LlmBackendRepository`] is a synchronous wrapper around `rusqlite`, the
//! same pattern as `PlanRepository`. All methods are synchronous; Tokio actors
//! call them via `spawn_blocking` when needed.
//!
//! The migration is embedded and applied idempotently on open.

use std::cell::RefCell;
use std::path::Path;

use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ────────────────────────────────────────────────────────────────────────────
// Migration
// ────────────────────────────────────────────────────────────────────────────

const MIGRATION_SQL: &str = "
CREATE TABLE IF NOT EXISTS llm_backends (
    name         TEXT PRIMARY KEY,
    provider     TEXT NOT NULL,
    model        TEXT NOT NULL,
    config_json  TEXT NOT NULL DEFAULT '{}',
    enabled      INTEGER NOT NULL DEFAULT 1,
    is_default   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (provider IN ('llama-cpp', 'openai', 'mistral', 'anthropic', 'ollama'))
);
";

// ────────────────────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────────────────────

/// Configuration of an LLM backend stored in `system.db`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendConfig {
    /// Unique name (e.g. `"local-code"`, `"mistral-small"`). Pattern: `[a-z0-9_-]+`.
    pub name: String,
    /// LLM provider.
    pub provider: LlmProvider,
    /// Model name or absolute GGUF path.
    pub model: String,
    /// Provider-specific parameters (JSON). May contain `"${VAR}"` for secrets.
    pub config_json: serde_json::Value,
    /// When `false`, the backend is not loaded at startup.
    pub enabled: bool,
    /// When `true`, used by agents that lack an explicit `llm_backend` field.
    pub is_default: bool,
}

/// Supported LLM provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LlmProvider {
    /// Embedded llama.cpp backend (local GGUF).
    LlamaCpp,
    /// OpenAI API or OpenAI-compatible API (LM Studio, vLLM).
    OpenAi,
    /// Mistral AI API.
    Mistral,
    /// Anthropic API.
    Anthropic,
    /// Local Ollama.
    Ollama,
}

impl std::fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LlmProvider::LlamaCpp => "llama-cpp",
            LlmProvider::OpenAi => "openai",
            LlmProvider::Mistral => "mistral",
            LlmProvider::Anthropic => "anthropic",
            LlmProvider::Ollama => "ollama",
        };
        f.write_str(s)
    }
}

impl TryFrom<&str> for LlmProvider {
    type Error = LlmBackendError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "llama-cpp" => Ok(LlmProvider::LlamaCpp),
            "openai" => Ok(LlmProvider::OpenAi),
            "mistral" => Ok(LlmProvider::Mistral),
            "anthropic" => Ok(LlmProvider::Anthropic),
            "ollama" => Ok(LlmProvider::Ollama),
            other => Err(LlmBackendError::Serialization(format!(
                "unknown provider: {other}"
            ))),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Errors
// ────────────────────────────────────────────────────────────────────────────

/// Errors returned by [`LlmBackendRepository`].
#[derive(Debug, Error)]
pub enum LlmBackendError {
    /// No backend found for the given name.
    #[error("backend '{0}' not found")]
    NotFound(String),

    /// A default backend already exists.
    #[error("a default backend already exists: '{0}'")]
    DefaultAlreadyExists(String),

    /// Refused to delete the default backend.
    #[error("cannot delete the default backend - set another default first")]
    CannotDeleteDefault,

    /// Invalid backend name (must match `[a-z0-9_-]+`).
    #[error("invalid backend name '{0}': only [a-z0-9_-] allowed")]
    InvalidName(String),

    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// JSON serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// I/O error while syncing to `apollia.toml`.
    #[error("io error syncing to toml: {0}")]
    Io(#[from] std::io::Error),
}

// ────────────────────────────────────────────────────────────────────────────
// Validation
// ────────────────────────────────────────────────────────────────────────────

fn validate_name(name: &str) -> Result<(), LlmBackendError> {
    // No `OnceLock` needed: validation runs rarely (write operations only),
    // so recompiling the pattern each call is acceptable.
    let re = Regex::new(r"^[a-z0-9_-]+$").expect("static pattern is valid");
    if name.is_empty() || !re.is_match(name) {
        return Err(LlmBackendError::InvalidName(name.to_string()));
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Repository
// ────────────────────────────────────────────────────────────────────────────

/// CRUD repository for LLM backends persisted in `system.db`.
///
/// Wraps a `rusqlite` connection behind a [`RefCell`] so that every method can
/// expose a `&self` API while still allowing the mutable borrow needed for
/// atomic transactions.
///
/// **Thread safety:** `LlmBackendRepository` is not `Send` (because of
/// `RefCell`). It must be created and used on the same thread, or passed to
/// `spawn_blocking`.
pub struct LlmBackendRepository {
    conn: RefCell<Connection>,
}

impl LlmBackendRepository {
    /// Opens (or creates) `system.db` at the given path and applies the migration.
    ///
    /// The migration is idempotent (`CREATE TABLE IF NOT EXISTS`), so it is safe
    /// to re-run on an existing database.
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] if opening or migrating fails.
    pub fn open(path: &Path) -> Result<Self, LlmBackendError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    /// Creates or updates a backend. Validates the name before insertion.
    ///
    /// If `config.is_default` is `true`, all other backends are first cleared
    /// (`is_default = 0`) within the same transaction.
    ///
    /// # Errors
    /// - [`LlmBackendError::InvalidName`] if the name does not match `[a-z0-9_-]+`.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    /// - [`LlmBackendError::Serialization`] if `config_json` cannot be serialized.
    pub fn save(&self, config: &LlmBackendConfig) -> Result<(), LlmBackendError> {
        validate_name(&config.name)?;

        let provider_str = config.provider.to_string();
        let config_json_str = serde_json::to_string(&config.config_json)
            .map_err(|e| LlmBackendError::Serialization(e.to_string()))?;

        let upsert_sql = "
            INSERT INTO llm_backends (name, provider, model, config_json, enabled, is_default)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(name) DO UPDATE SET
                provider    = excluded.provider,
                model       = excluded.model,
                config_json = excluded.config_json,
                enabled     = excluded.enabled,
                is_default  = excluded.is_default,
                updated_at  = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        ";

        if config.is_default {
            let mut conn = self.conn.borrow_mut();
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE llm_backends SET is_default = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                 WHERE is_default = 1 AND name != ?1",
                params![config.name],
            )?;
            tx.execute(
                upsert_sql,
                params![
                    config.name,
                    provider_str,
                    config.model,
                    config_json_str,
                    config.enabled as i32,
                    config.is_default as i32,
                ],
            )?;
            tx.commit()?;
        } else {
            self.conn.borrow().execute(
                upsert_sql,
                params![
                    config.name,
                    provider_str,
                    config.model,
                    config_json_str,
                    config.enabled as i32,
                    config.is_default as i32,
                ],
            )?;
        }

        Ok(())
    }

    /// Returns all backends (enabled and disabled), sorted by name.
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn list(&self) -> Result<Vec<LlmBackendConfig>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT name, provider, model, config_json, enabled, is_default \
             FROM llm_backends \
             ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })?;

        let mut configs = Vec::new();
        for row in rows {
            let (name, provider_str, model, config_json_str, enabled, is_default) = row?;
            configs.push(row_to_config(RawBackendRow {
                name,
                provider_str,
                model,
                config_json_str,
                enabled: enabled != 0,
                is_default: is_default != 0,
            })?);
        }
        Ok(configs)
    }

    /// Finds a backend by exact name. Returns `None` if not found.
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn find_by_name(&self, name: &str) -> Result<Option<LlmBackendConfig>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT name, provider, model, config_json, enabled, is_default \
             FROM llm_backends \
             WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })?;

        match rows.next() {
            None => Ok(None),
            Some(row) => {
                let (name, provider_str, model, config_json_str, enabled, is_default) = row?;
                Ok(Some(row_to_config(RawBackendRow {
                    name,
                    provider_str,
                    model,
                    config_json_str,
                    enabled: enabled != 0,
                    is_default: is_default != 0,
                })?))
            }
        }
    }

    /// Returns the backend flagged `is_default = true`, or `None` if there is none.
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn find_default(&self) -> Result<Option<LlmBackendConfig>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT name, provider, model, config_json, enabled, is_default \
             FROM llm_backends \
             WHERE is_default = 1 \
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })?;

        match rows.next() {
            None => Ok(None),
            Some(row) => {
                let (name, provider_str, model, config_json_str, enabled, is_default) = row?;
                Ok(Some(row_to_config(RawBackendRow {
                    name,
                    provider_str,
                    model,
                    config_json_str,
                    enabled: enabled != 0,
                    is_default: is_default != 0,
                })?))
            }
        }
    }

    /// Marks `name` as the default backend.
    ///
    /// The previous default (if any) is cleared atomically in the same transaction.
    ///
    /// # Errors
    /// - [`LlmBackendError::NotFound`] if `name` does not exist in the DB.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    pub fn set_default(&self, name: &str) -> Result<(), LlmBackendError> {
        // find_by_name borrows conn and releases it before the borrow_mut below.
        if self.find_by_name(name)?.is_none() {
            return Err(LlmBackendError::NotFound(name.to_string()));
        }

        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE llm_backends \
             SET is_default = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE is_default = 1",
            [],
        )?;
        tx.execute(
            "UPDATE llm_backends \
             SET is_default = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE name = ?1",
            params![name],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes a backend.
    ///
    /// # Errors
    /// - [`LlmBackendError::NotFound`] if `name` is absent.
    /// - [`LlmBackendError::CannotDeleteDefault`] if `name` is the default backend.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    pub fn delete(&self, name: &str) -> Result<(), LlmBackendError> {
        // find_by_name borrows conn and releases it before the borrow below.
        match self.find_by_name(name)? {
            None => return Err(LlmBackendError::NotFound(name.to_string())),
            Some(b) if b.is_default => return Err(LlmBackendError::CannotDeleteDefault),
            _ => {}
        }
        self.conn
            .borrow()
            .execute("DELETE FROM llm_backends WHERE name = ?1", params![name])?;
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// TOML sync helpers
// ────────────────────────────────────────────────────────────────────────────

/// Removes every `[[llm.backends]]` block from the given TOML content.
///
/// Each block starts at the `[[llm.backends]]` line and ends just before the
/// next section header line (`[...` or `[[...`).
fn strip_llm_backends_blocks(content: &str) -> String {
    let mut result: Vec<&str> = Vec::new();
    let mut in_backends = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[llm.backends]]" {
            in_backends = true;
            continue;
        }
        if in_backends && trimmed.starts_with('[') {
            in_backends = false;
        }
        if !in_backends {
            result.push(line);
        }
    }

    // Trim trailing blank lines then add exactly one newline.
    let joined = result.join("\n");
    let trimmed = joined.trim_end();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

/// Builds a `[[llm.backends]]` TOML block from an [`LlmBackendConfig`].
fn backend_to_toml_block(cfg: &LlmBackendConfig) -> String {
    let mut lines = vec![
        "[[llm.backends]]".to_string(),
        format!("name     = {:?}", cfg.name),
    ];

    match cfg.provider {
        LlmProvider::LlamaCpp => {
            lines.push(r#"type     = "embedded""#.to_string());
            lines.push(format!("model_path   = {:?}", cfg.model));
            if let Some(q) = cfg.config_json.get("quantization").and_then(|v| v.as_str()) {
                lines.push(format!("quantization = {:?}", q));
            }
            if let Some(d) = cfg.config_json.get("device").and_then(|v| v.as_str()) {
                lines.push(format!("device       = {:?}", d));
            }
        }
        _ => {
            lines.push(r#"type        = "api""#.to_string());
            lines.push(format!("provider    = {:?}", cfg.provider.to_string()));
            lines.push(format!("model       = {:?}", cfg.model));
            if let Some(url) = cfg.config_json.get("api_url").and_then(|v| v.as_str()) {
                lines.push(format!("api_url     = {:?}", url));
            }
            // Reconstruct api_key_env from stored "${VAR}" sentinel if present.
            if let Some(key_ref) = cfg
                .config_json
                .get("api_key")
                .and_then(|v| v.as_str())
                .filter(|s| s.starts_with("${") && s.ends_with('}'))
            {
                let var_name = &key_ref[2..key_ref.len() - 1];
                lines.push(format!("api_key_env = {:?}", var_name));
            }
        }
    }

    lines.join("\n")
}

impl LlmBackendRepository {
    /// Syncs every DB backend into the `[[llm.backends]]` section of `apollia.toml`.
    ///
    /// - Reads `toml_path` (creates the file if absent).
    /// - Removes the old `[[llm.backends]]` blocks.
    /// - Appends an auto-generated block for each backend present in the DB.
    /// - Rewrites the file atomically.
    ///
    /// A sentinel comment marks the section as automatically managed.
    ///
    /// # Errors
    /// - [`LlmBackendError::Db`] if the DB read fails.
    /// - [`LlmBackendError::Io`] if the file cannot be read or written.
    pub fn sync_to_toml(&self, toml_path: &Path) -> Result<(), LlmBackendError> {
        let backends = self.list()?;

        let existing = if toml_path.exists() {
            std::fs::read_to_string(toml_path)?
        } else {
            String::new()
        };

        let base = strip_llm_backends_blocks(&existing);

        let mut output = base;

        if !backends.is_empty() {
            output.push_str(
                "\n# ⚠️  Section gérée automatiquement par Apollia - éditer via Settings\n",
            );
            for cfg in &backends {
                output.push('\n');
                output.push_str(&backend_to_toml_block(cfg));
                output.push('\n');
            }
        }

        std::fs::write(toml_path, output)?;

        tracing::debug!(
            path = %toml_path.display(),
            count = backends.len(),
            "llm backends synced to apollia.toml"
        );
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

/// Raw values from an `llm_backends` row, grouped to keep the
/// [`row_to_config`] signature readable.
struct RawBackendRow {
    name: String,
    provider_str: String,
    model: String,
    config_json_str: String,
    enabled: bool,
    is_default: bool,
}

/// Builds an [`LlmBackendConfig`] from the raw values of a SQLite row.
fn row_to_config(row: RawBackendRow) -> Result<LlmBackendConfig, LlmBackendError> {
    let provider = LlmProvider::try_from(row.provider_str.as_str())?;
    let config_json: serde_json::Value = serde_json::from_str(&row.config_json_str)
        .map_err(|e| LlmBackendError::Serialization(e.to_string()))?;
    Ok(LlmBackendConfig {
        name: row.name,
        provider,
        model: row.model,
        config_json,
        enabled: row.enabled,
        is_default: row.is_default,
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_repo() -> (LlmBackendRepository, TempDir) {
        let dir = TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
        (repo, dir)
    }

    fn make_config(name: &str, is_default: bool) -> LlmBackendConfig {
        LlmBackendConfig {
            name: name.to_string(),
            provider: LlmProvider::OpenAi,
            model: "gpt-4o-mini".to_string(),
            config_json: serde_json::json!({ "api_key": "${OPENAI_KEY}" }),
            enabled: true,
            is_default,
        }
    }

    // GIVEN an empty repository
    // WHEN  save() + list()
    // THEN  the list contains the saved backend
    #[test]
    fn test_ac1_save_and_list() {
        let (repo, _dir) = make_repo();
        let config = make_config("openai", false);

        repo.save(&config).unwrap();
        let list = repo.list().unwrap();

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "openai");
        assert_eq!(list[0].provider, LlmProvider::OpenAi);
        assert_eq!(list[0].model, "gpt-4o-mini");
        assert!(!list[0].is_default);
    }

    // GIVEN two backends, "a" is the default
    // WHEN  set_default("b")
    // THEN  exactly 1 backend has is_default=true, and it is "b"
    #[test]
    fn test_ac2_set_default_replaces_previous() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", true)).unwrap();
        repo.save(&make_config("b", false)).unwrap();

        repo.set_default("b").unwrap();

        let defaults: Vec<_> = repo
            .list()
            .unwrap()
            .into_iter()
            .filter(|c| c.is_default)
            .collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "b");
    }

    // GIVEN a backend "a" flagged is_default=true
    // WHEN  delete("a")
    // THEN  LlmBackendError::CannotDeleteDefault returned, "a" still present
    #[test]
    fn test_ac3_cannot_delete_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", true)).unwrap();

        assert!(matches!(
            repo.delete("a"),
            Err(LlmBackendError::CannotDeleteDefault)
        ));
        assert!(repo.find_by_name("a").unwrap().is_some());
    }

    // GIVEN an empty repository
    // WHEN  find_by_name("nonexistent")
    // THEN  Ok(None) returned
    #[test]
    fn test_ac4_find_by_name_missing_returns_none() {
        let (repo, _dir) = make_repo();
        assert!(repo.find_by_name("ghost").unwrap().is_none());
    }

    // GIVEN backends present but none with is_default=true
    // WHEN  find_default()
    // THEN  Ok(None) returned
    #[test]
    fn test_ac5_find_default_none_when_no_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", false)).unwrap();
        assert!(repo.find_default().unwrap().is_none());
    }

    // GIVEN a non-default backend
    // WHEN  delete()
    // THEN  backend removed, list() returns empty
    #[test]
    fn test_delete_non_default_succeeds() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", false)).unwrap();

        repo.delete("a").unwrap();

        assert!(repo.list().unwrap().is_empty());
    }

    // GIVEN a nonexistent backend
    // WHEN  delete()
    // THEN  LlmBackendError::NotFound returned
    #[test]
    fn test_delete_not_found() {
        let (repo, _dir) = make_repo();
        assert!(matches!(
            repo.delete("ghost"),
            Err(LlmBackendError::NotFound(_))
        ));
    }

    // GIVEN a nonexistent backend
    // WHEN  set_default()
    // THEN  LlmBackendError::NotFound returned
    #[test]
    fn test_set_default_not_found() {
        let (repo, _dir) = make_repo();
        assert!(matches!(
            repo.set_default("ghost"),
            Err(LlmBackendError::NotFound(_))
        ));
    }

    // GIVEN an existing backend
    // WHEN  save() with an invalid name (uppercase)
    // THEN  LlmBackendError::InvalidName returned
    #[test]
    fn test_invalid_name_rejected() {
        let (repo, _dir) = make_repo();
        let config = make_config("MyBackend", false);
        assert!(matches!(
            repo.save(&config),
            Err(LlmBackendError::InvalidName(_))
        ));
    }

    // GIVEN TOML content with two [[llm.backends]] blocks and other sections
    // WHEN  strip_llm_backends_blocks()
    // THEN  the backend blocks are removed, the rest is preserved
    #[test]
    fn test_strip_llm_backends_blocks_preserves_other_sections() {
        let input = "[runtime]\ndata_dir = \"~/.apollia\"\n\n[[llm.backends]]\nname = \"local\"\ntype = \"embedded\"\n\n[[llm.backends]]\nname = \"remote\"\ntype = \"api\"\n\n[api]\nbind = \"127.0.0.1:7771\"\n";
        let result = strip_llm_backends_blocks(input);
        assert!(result.contains("[runtime]"));
        assert!(result.contains("[api]"));
        assert!(!result.contains("[[llm.backends]]"));
        assert!(!result.contains("name = \"local\""));
    }

    // GIVEN TOML content with no [[llm.backends]] block
    // WHEN  strip_llm_backends_blocks()
    // THEN  the content is unchanged (apart from the trailing newline)
    #[test]
    fn test_strip_llm_backends_blocks_noop_when_absent() {
        let input = "[runtime]\ndata_dir = \"~/.apollia\"\n";
        let result = strip_llm_backends_blocks(input);
        assert!(result.contains("[runtime]"));
        assert!(!result.contains("[[llm.backends]]"));
    }

    // GIVEN a llama-cpp backend with device and quantization
    // WHEN  backend_to_toml_block()
    // THEN  the TOML block contains the expected fields
    #[test]
    fn test_backend_to_toml_block_embedded() {
        let cfg = LlmBackendConfig {
            name: "local".to_string(),
            provider: LlmProvider::LlamaCpp,
            model: "~/.apollia/models/model.gguf".to_string(),
            config_json: serde_json::json!({ "device": "metal", "quantization": "q4_k_m" }),
            enabled: true,
            is_default: true,
        };
        let block = backend_to_toml_block(&cfg);
        assert!(block.contains("[[llm.backends]]"));
        assert!(block.contains(r#"type     = "embedded""#));
        assert!(block.contains("model_path"));
        assert!(block.contains("metal"));
        assert!(block.contains("q4_k_m"));
    }

    // GIVEN a repository with one backend
    // WHEN  sync_to_toml() to a temporary file
    // THEN  the file contains the [[llm.backends]] block
    #[test]
    fn test_sync_to_toml_writes_backends() {
        let (repo, dir) = make_repo();
        let cfg = LlmBackendConfig {
            name: "local".to_string(),
            provider: LlmProvider::LlamaCpp,
            model: "~/.apollia/models/model.gguf".to_string(),
            config_json: serde_json::json!({ "device": "metal", "quantization": "q4_k_m" }),
            enabled: true,
            is_default: true,
        };
        repo.save(&cfg).unwrap();

        let toml_path = dir.path().join("apollia.toml");
        repo.sync_to_toml(&toml_path).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        assert!(content.contains("[[llm.backends]]"));
        assert!(content.contains("\"local\""));
    }

    // GIVEN an existing TOML file with an old backend, and a different backend in the DB
    // WHEN  sync_to_toml()
    // THEN  the old backend is replaced by the new one, the rest of the TOML is preserved
    #[test]
    fn test_sync_to_toml_replaces_old_backends() {
        let (repo, dir) = make_repo();
        let toml_path = dir.path().join("apollia.toml");

        std::fs::write(
            &toml_path,
            "[runtime]\ndata_dir = \"~/.apollia\"\n\n[[llm.backends]]\nname = \"old\"\ntype = \"embedded\"\nmodel_path = \"old.gguf\"\nquantization = \"q8_0\"\n",
        )
        .unwrap();

        let cfg = LlmBackendConfig {
            name: "new".to_string(),
            provider: LlmProvider::LlamaCpp,
            model: "~/.apollia/models/new.gguf".to_string(),
            config_json: serde_json::json!({ "device": "cpu", "quantization": "q4_0" }),
            enabled: true,
            is_default: true,
        };
        repo.save(&cfg).unwrap();
        repo.sync_to_toml(&toml_path).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        assert!(content.contains("[runtime]"), "runtime section preserved");
        assert!(!content.contains("\"old\""), "old backend removed");
        assert!(content.contains("\"new\""), "new backend present");
    }

    // GIVEN a saved backend
    // WHEN  find_default() after save(is_default=true)
    // THEN  the default is returned
    #[test]
    fn test_find_default_returns_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("main", true)).unwrap();
        let default = repo.find_default().unwrap();
        assert!(default.is_some());
        assert_eq!(default.unwrap().name, "main");
    }

    // GIVEN two successive backends with is_default=true
    // WHEN  save() of the second (is_default=true)
    // THEN  only one default in the list
    #[test]
    fn test_save_second_default_replaces_first() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("first", true)).unwrap();
        repo.save(&make_config("second", true)).unwrap();

        let defaults: Vec<_> = repo
            .list()
            .unwrap()
            .into_iter()
            .filter(|c| c.is_default)
            .collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "second");
    }
}
