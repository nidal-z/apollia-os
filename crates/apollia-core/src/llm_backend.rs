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
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ────────────────────────────────────────────────────────────────────────────
// Migration
// ────────────────────────────────────────────────────────────────────────────
//
// The `llm_backends` DDL lives in `crate::system_db`: `system.db` is shared
// with the STT configuration store, and `PRAGMA user_version` belongs to the
// file, so the two stores migrate through one numbered list.

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
#[non_exhaustive]
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

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] crate::schema::SchemaError),

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
    /// Opens (or creates) `system.db` at the given path and migrates it.
    ///
    /// The file is brought to the current `system.db` schema version through
    /// [`crate::schema::open_versioned`]; a database written by a newer binary
    /// is refused instead of misread.
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] if opening fails, and
    /// [`LlmBackendError::Schema`] if the migration fails or the database is
    /// newer than this binary.
    pub fn open(path: &Path) -> Result<Self, LlmBackendError> {
        let conn = Connection::open(path)?;
        crate::schema::open_versioned(
            &conn,
            crate::paths::DataFile::System.file_name(),
            crate::system_db::SYSTEM_DB_SCHEMA_VERSION,
            crate::system_db::SYSTEM_DB_MIGRATIONS,
        )?;
        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    /// Creates or updates a backend. Validates the name before insertion.
    ///
    /// If `config.is_default` is `true`, all other backends are first cleared
    /// (`is_default = 0`) within the same transaction.
    ///
    /// Invariant: whenever the table is non-empty, exactly one backend is the
    /// default. If `config.is_default` is `false` but no *other* backend is
    /// currently the default, this backend is promoted to default. Without this,
    /// a backend created from the desktop UI without ticking the (collapsed)
    /// "default" toggle would leave the table with no default, and the router
    /// reload (`find_default`) would fail with "no default LLM backend
    /// configured" even though a usable backend exists.
    ///
    /// # Errors
    /// - [`LlmBackendError::InvalidName`] if the name does not match `[a-z0-9_-]+`.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    /// - [`LlmBackendError::Serialization`] if `config_json` cannot be serialized.
    pub fn save(&self, config: &LlmBackendConfig) -> Result<(), LlmBackendError> {
        validate_name(&config.name)?;

        // Promote to default when no other backend currently holds the flag, so
        // the table never ends up with backends but no default.
        let is_default = config.is_default || {
            let conn = self.conn.borrow();
            let existing_default: Option<String> = conn
                .query_row(
                    "SELECT name FROM llm_backends WHERE is_default = 1 AND name != ?1 LIMIT 1",
                    params![config.name],
                    |row| row.get(0),
                )
                .optional()?;
            existing_default.is_none()
        };

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

        if is_default {
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
                    is_default as i32,
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
                    is_default as i32,
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

/// Sentinel comment marking the auto-generated backends section in `apollia.toml`.
const MANAGED_SECTION_COMMENT: &str =
    "# ⚠️  Section gérée automatiquement par Apollia - éditer via Settings";

/// Stable substring used to recognise (and strip) the sentinel comment.
const MANAGED_SECTION_MARKER: &str = "Section gérée automatiquement par Apollia";

/// Removes the auto-managed backends representation from TOML content so it can be
/// regenerated cleanly. Strips three things that would otherwise make the file
/// invalid or accumulate on every sync:
/// - every `[[llm.backends]]` array-of-tables block (header to next section),
/// - the inline `backends = ...` key inside the `[llm]` table: it is the SAME TOML
///   path as `[[llm.backends]]`, so keeping both makes the document unparseable,
/// - the managed-section sentinel comment (the writer re-emits exactly one).
fn strip_llm_backends_blocks(content: &str) -> String {
    let mut result: Vec<&str> = Vec::new();
    let mut in_backends_block = false;
    let mut in_llm_table = false;
    let mut in_multiline_backends = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Continuation of a multi-line inline `backends = [ ... ]` value.
        if in_multiline_backends {
            if trimmed.ends_with(']') {
                in_multiline_backends = false;
            }
            continue;
        }

        // Drop the sentinel comment: the writer re-emits exactly one.
        if trimmed.contains(MANAGED_SECTION_MARKER) {
            continue;
        }

        // `[[llm.backends]]` block: skip until the next section header.
        if trimmed == "[[llm.backends]]" {
            in_backends_block = true;
            in_llm_table = false;
            continue;
        }
        if in_backends_block {
            if trimmed.starts_with('[') {
                in_backends_block = false;
            } else {
                continue;
            }
        }

        // Track the `[llm]` table so the inline key is stripped only there.
        if trimmed.starts_with('[') {
            in_llm_table = trimmed == "[llm]";
        }

        // Drop the inline `backends = ...` key inside `[llm]`.
        if in_llm_table {
            if let Some(rest) = trimmed.strip_prefix("backends") {
                if let Some(value) = rest.trim_start().strip_prefix('=') {
                    let value = value.trim();
                    if value.starts_with('[') && !value.ends_with(']') {
                        in_multiline_backends = true;
                    }
                    continue;
                }
            }
        }

        result.push(line);
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

/// Builds a `[[llm.backends]]` TOML block from an [`LlmBackendConfig`], or `None`
/// for backends that must not be mirrored to `apollia.toml`.
///
/// Only API backends are representable: the config parser's `[[llm.backends]]`
/// discriminant (`apollia_llm::router::BackendKind`) has a single `type = "api"`
/// variant. Local (llama-cpp) backends live in `system.db` and are injected via
/// `RunnerLlmBackend`; emitting them here as `type = "embedded"` produced a config
/// the runtime rejects at startup (`unknown variant "embedded"`), breaking boot.
fn backend_to_toml_block(cfg: &LlmBackendConfig) -> Option<String> {
    if cfg.provider == LlmProvider::LlamaCpp {
        return None;
    }

    // `api_url` and `api_key_env` are required by `ApiBackendConfig`; emit both so
    // the block round-trips through the config parser. The URL is read from the
    // canonical `base_url` (CLI + router), falling back to the desktop `endpoint`
    // and the legacy `api_url` keys so any persisted backend mirrors correctly.
    // `provider` is not a parser field: it is ignored on read and re-inferred from
    // the URL, kept here only for human readability.
    let api_url = ["base_url", "endpoint", "api_url"]
        .iter()
        .find_map(|key| {
            cfg.config_json
                .get(*key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();

    // Reconstruct api_key_env from the stored "${VAR}" sentinel; empty for a
    // keyless backend (e.g. a local llama-server), which the parser accepts.
    let api_key_env = cfg
        .config_json
        .get("api_key")
        .and_then(|v| v.as_str())
        .filter(|s| s.starts_with("${") && s.ends_with('}'))
        .map(|s| s[2..s.len() - 1].to_string())
        .unwrap_or_default();

    let lines = [
        "[[llm.backends]]".to_string(),
        format!("name     = {:?}", cfg.name),
        r#"type        = "api""#.to_string(),
        format!("provider    = {:?}", cfg.provider.to_string()),
        format!("model       = {:?}", cfg.model),
        format!("api_url     = {:?}", api_url),
        format!("api_key_env = {:?}", api_key_env),
    ];

    Some(lines.join("\n"))
}

/// Inserts `backends = []` right after the `[llm]` table header.
///
/// The `[llm].backends` field is required by the config schema (no serde default),
/// so it must be present even when there is no API backend to mirror. Callers pass
/// content whose inline `backends` key has already been stripped.
fn insert_empty_backends_key(content: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut inserted = false;
    for line in content.lines() {
        out.push(line.to_string());
        if !inserted && line.trim() == "[llm]" {
            out.push("backends = []".to_string());
            inserted = true;
        }
    }
    if !inserted {
        // No `[llm]` header (empty or unusual file): prepend a minimal one so the
        // required field exists. A real config always has `[llm]`.
        let mut prefixed = vec!["[llm]".to_string(), "backends = []".to_string()];
        prefixed.append(&mut out);
        out = prefixed;
    }

    let joined = out.join("\n");
    let trimmed = joined.trim_end();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
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

        // Only API backends are representable as `[[llm.backends]]`; embedded ones
        // yield `None` and stay in system.db (injected via RunnerLlmBackend).
        let blocks: Vec<String> = backends.iter().filter_map(backend_to_toml_block).collect();

        let output = if blocks.is_empty() {
            // Nothing to mirror: keep the required `backends` field as an empty
            // inline array (the strip step removed any previous inline key).
            insert_empty_backends_key(&base)
        } else {
            let mut out = base;
            out.push('\n');
            out.push_str(MANAGED_SECTION_COMMENT);
            out.push('\n');
            for block in &blocks {
                out.push('\n');
                out.push_str(block);
                out.push('\n');
            }
            out
        };

        std::fs::write(toml_path, output)?;

        tracing::debug!(
            path = %toml_path.display(),
            count = blocks.len(),
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
    fn test_save_and_list() {
        let (repo, _dir) = make_repo();
        let config = make_config("openai", false);

        repo.save(&config).unwrap();
        let list = repo.list().unwrap();

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "openai");
        assert_eq!(list[0].provider, LlmProvider::OpenAi);
        assert_eq!(list[0].model, "gpt-4o-mini");
        // Saved with is_default=false, but auto-promoted: a sole backend is
        // always the default (invariant enforced by save()).
        assert!(list[0].is_default);
    }

    // GIVEN two backends, "a" is the default
    // WHEN  set_default("b")
    // THEN  exactly 1 backend has is_default=true, and it is "b"
    #[test]
    fn test_set_default_replaces_previous() {
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
    fn test_cannot_delete_default() {
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
    fn test_find_by_name_missing_returns_none() {
        let (repo, _dir) = make_repo();
        assert!(repo.find_by_name("ghost").unwrap().is_none());
    }

    // GIVEN an empty repository
    // WHEN  save() a backend with is_default=false
    // THEN  it is auto-promoted to default (invariant: a non-empty table always
    //       has exactly one default), so the router reload can resolve it
    #[test]
    fn test_first_backend_auto_promoted_to_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", false)).unwrap();
        let default = repo.find_default().unwrap();
        assert!(default.is_some());
        assert_eq!(default.unwrap().name, "a");
    }

    // GIVEN a repository that already has a default backend
    // WHEN  save() a second backend with is_default=false
    // THEN  the existing default is preserved (no silent steal), and the new
    //       backend stays non-default
    #[test]
    fn test_second_non_default_backend_does_not_steal_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", true)).unwrap();
        repo.save(&make_config("b", false)).unwrap();

        let default = repo.find_default().unwrap().expect("a default exists");
        assert_eq!(default.name, "a");
        assert!(!repo.find_by_name("b").unwrap().unwrap().is_default);
    }

    // GIVEN an empty table
    // WHEN  the same backend is re-saved with is_default=false (idempotent edit)
    // THEN  it remains the default (it was auto-promoted on first insert and no
    //       other backend exists to hold the flag)
    #[test]
    fn test_resaving_sole_backend_keeps_default() {
        let (repo, _dir) = make_repo();
        repo.save(&make_config("a", false)).unwrap();
        repo.save(&make_config("a", false)).unwrap();
        let default = repo.find_default().unwrap();
        assert_eq!(default.expect("still default").name, "a");
    }

    // GIVEN a default backend plus a separate non-default backend
    // WHEN  delete() the non-default one
    // THEN  it is removed and the default is left untouched
    #[test]
    fn test_delete_non_default_succeeds() {
        let (repo, _dir) = make_repo();
        // "keeper" holds the default flag so "a" stays non-default (otherwise
        // the sole-backend invariant would auto-promote "a").
        repo.save(&make_config("keeper", true)).unwrap();
        repo.save(&make_config("a", false)).unwrap();

        repo.delete("a").unwrap();

        assert!(repo.find_by_name("a").unwrap().is_none());
        assert!(repo.find_by_name("keeper").unwrap().is_some());
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

    // GIVEN an embedded (llama-cpp) backend
    // WHEN  backend_to_toml_block()
    // THEN  it returns None (embedded backends are not mirrored to the toml; the
    //       config parser has no `type = "embedded"` variant)
    #[test]
    fn test_backend_to_toml_block_skips_embedded() {
        let cfg = LlmBackendConfig {
            name: "local".to_string(),
            provider: LlmProvider::LlamaCpp,
            model: "~/.apollia/models/model.gguf".to_string(),
            config_json: serde_json::json!({ "device": "metal", "quantization": "q4_k_m" }),
            enabled: true,
            is_default: true,
        };
        assert!(backend_to_toml_block(&cfg).is_none());
    }

    // GIVEN an API backend created via the CLI (URL under `base_url`, keyed)
    // WHEN  backend_to_toml_block()
    // THEN  it returns a `type = "api"` block with `api_url` (from `base_url`) and
    //       `api_key_env`, so the block round-trips through the config parser
    #[test]
    fn test_backend_to_toml_block_api() {
        let cfg = LlmBackendConfig {
            name: "remote".to_string(),
            provider: LlmProvider::OpenAi,
            model: "gpt-4o-mini".to_string(),
            config_json: serde_json::json!({
                "base_url": "http://127.0.0.1:8080/v1",
                "api_key": "${OPENAI_KEY}"
            }),
            enabled: true,
            is_default: false,
        };
        let block = backend_to_toml_block(&cfg).expect("api backend is mirrored");
        assert!(block.contains(r#"type        = "api""#));
        assert!(block.contains("\"remote\""));
        assert!(block.contains(r#"api_url     = "http://127.0.0.1:8080/v1""#));
        assert!(block.contains(r#"api_key_env = "OPENAI_KEY""#));
    }

    // GIVEN a backend created by the desktop UI, which stores the URL under
    // `endpoint` instead of the canonical `base_url`
    // WHEN  backend_to_toml_block()
    // THEN  the TOML mirror still resolves the URL from the `endpoint` fallback
    #[test]
    fn test_backend_to_toml_block_reads_endpoint_fallback() {
        let cfg = LlmBackendConfig {
            name: "ui-made".to_string(),
            provider: LlmProvider::OpenAi,
            model: "qwen".to_string(),
            config_json: serde_json::json!({ "endpoint": "http://127.0.0.1:8899/v1" }),
            enabled: true,
            is_default: false,
        };
        let block = backend_to_toml_block(&cfg).expect("api backend is mirrored");
        assert!(block.contains(r#"api_url     = "http://127.0.0.1:8899/v1""#));
    }

    // GIVEN a keyless API backend (e.g. a local llama-server)
    // WHEN  backend_to_toml_block()
    // THEN  it still emits the required `api_url` and (empty) `api_key_env`, and the
    //       block parses as a valid ApiBackendConfig-shaped table
    #[test]
    fn test_backend_to_toml_block_api_keyless() {
        let cfg = LlmBackendConfig {
            name: "local-fast".to_string(),
            provider: LlmProvider::OpenAi,
            model: "ministral-local".to_string(),
            config_json: serde_json::json!({ "base_url": "http://127.0.0.1:8080/v1" }),
            enabled: true,
            is_default: true,
        };
        let block = backend_to_toml_block(&cfg).expect("api backend is mirrored");
        assert!(block.contains(r#"api_url     = "http://127.0.0.1:8080/v1""#));
        assert!(block.contains(r#"api_key_env = """#));
        // The block alone must parse (required fields present).
        toml::from_str::<toml::Value>(&block).expect("valid TOML block");
    }

    // GIVEN a repository with one API backend
    // WHEN  sync_to_toml() to a temporary file
    // THEN  the file contains the [[llm.backends]] block
    #[test]
    fn test_sync_to_toml_writes_backends() {
        let (repo, dir) = make_repo();
        repo.save(&make_config("remote", true)).unwrap();

        let toml_path = dir.path().join("apollia.toml");
        repo.sync_to_toml(&toml_path).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        assert!(content.contains("[[llm.backends]]"));
        assert!(content.contains("\"remote\""));
    }

    // GIVEN a repository with only an embedded (llama-cpp) backend
    // WHEN  sync_to_toml()
    // THEN  nothing is mirrored (no `[[llm.backends]]`, no `embedded`) and the
    //       required `backends` field is kept as an empty inline array, so the
    //       result stays parseable by the runtime
    #[test]
    fn test_sync_to_toml_skips_embedded() {
        let (repo, dir) = make_repo();
        let toml_path = dir.path().join("apollia.toml");
        std::fs::write(&toml_path, "[llm]\ndefault = \"local\"\nbackends = []\n").unwrap();

        let cfg = LlmBackendConfig {
            name: "local".to_string(),
            provider: LlmProvider::LlamaCpp,
            model: "~/.apollia/models/model.gguf".to_string(),
            config_json: serde_json::json!({ "device": "metal", "quantization": "q4_k_m" }),
            enabled: true,
            is_default: true,
        };
        repo.save(&cfg).unwrap();
        repo.sync_to_toml(&toml_path).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        toml::from_str::<toml::Value>(&content).expect("valid TOML");
        assert!(
            !content.contains("[[llm.backends]]"),
            "embedded not mirrored"
        );
        assert!(!content.contains("embedded"), "no embedded type written");
        assert!(
            content.contains("backends = []"),
            "required field kept empty"
        );
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

        repo.save(&make_config("new", true)).unwrap();
        repo.sync_to_toml(&toml_path).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        assert!(content.contains("[runtime]"), "runtime section preserved");
        assert!(!content.contains("\"old\""), "old backend removed");
        assert!(content.contains("\"new\""), "new backend present");
    }

    // GIVEN a real-world config: `[llm]` with an inline `backends = []` key
    // WHEN  sync_to_toml() appends the DB backend as `[[llm.backends]]`
    // THEN  the result is still VALID TOML (the inline key, same path as the
    //       array-of-tables, is stripped) and the other `[llm]` keys are kept
    #[test]
    fn test_sync_to_toml_valid_with_inline_backends_key() {
        let (repo, dir) = make_repo();
        let toml_path = dir.path().join("apollia.toml");
        std::fs::write(
            &toml_path,
            "[llm]\ndefault = \"local\"\nbackends = []\n\n[api]\nport = 7771\n",
        )
        .unwrap();

        repo.save(&make_config("local", true)).unwrap();
        repo.sync_to_toml(&toml_path).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        // Must parse: the inline `backends` and `[[llm.backends]]` share a path,
        // so only the array-of-tables may remain.
        toml::from_str::<toml::Value>(&content).expect("sync must produce valid TOML");
        assert!(!content.contains("backends = []"), "inline key stripped");
        assert!(
            content.contains("[[llm.backends]]"),
            "array-of-tables present"
        );
        assert!(
            content.contains("default = \"local\""),
            "other [llm] keys kept"
        );
        assert!(content.contains("[api]"), "unrelated sections preserved");
    }

    // GIVEN sync_to_toml() run twice on the same file
    // WHEN  the second sync strips what the first wrote
    // THEN  the file stays valid and the sentinel comment appears exactly once
    #[test]
    fn test_sync_to_toml_sentinel_not_duplicated() {
        let (repo, dir) = make_repo();
        let toml_path = dir.path().join("apollia.toml");
        std::fs::write(&toml_path, "[llm]\ndefault = \"local\"\nbackends = []\n").unwrap();

        repo.save(&make_config("local", true)).unwrap();
        repo.sync_to_toml(&toml_path).unwrap();
        repo.sync_to_toml(&toml_path).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        toml::from_str::<toml::Value>(&content).expect("valid TOML after re-sync");
        assert_eq!(content.matches(MANAGED_SECTION_MARKER).count(), 1);
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
