//! Versioned schema of `system.db`, shared by every store in the file.
//!
//! `system.db` holds the LLM backend registry ([`crate::llm_backend`]), the
//! inference servers ([`crate::inference_server`]), the model presets and the
//! model of each agent ([`crate::model_preset`]), the STT configuration
//! ([`crate::stt_config`]), the default embeddings model
//! ([`crate::embedding_default`]), the tokens of the compatible server
//! ([`crate::server_token`]), the last local model loaded by hand
//! ([`crate::last_loaded_model`]), and the registry of sources
//! ([`crate::source_registry`]).
//! `PRAGMA user_version` belongs to the database file, not to a table, so the
//! stores share one migration list: whichever opens first brings the whole
//! file to the current version, and both refuse a file written by a newer
//! binary.

use rusqlite::{params, Connection, OptionalExtension};

use crate::inference_server::{openai_api_base, DEFAULT_OLLAMA_ADDRESS};
use crate::schema::{add_column_if_missing, Migration};

/// Current schema version of `system.db`.
pub(crate) const SYSTEM_DB_SCHEMA_VERSION: u32 = 8;

/// Numbered migration steps of `system.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`SYSTEM_DB_SCHEMA_VERSION`].
pub(crate) const SYSTEM_DB_MIGRATIONS: &[Migration] = &[
    migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5, migrate_v6, migrate_v7, migrate_v8,
];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `system.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the initial schema, and the schema after the additive
/// `input_device` column, and bring each of them to the same state.
pub(crate) fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS llm_backends (
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

        CREATE TABLE IF NOT EXISTS stt_config (
            id                   INTEGER PRIMARY KEY CHECK (id = 1),
            enabled              INTEGER NOT NULL DEFAULT 0,
            model_path           TEXT    NOT NULL DEFAULT '',
            hotkey               TEXT    NOT NULL DEFAULT 'ctrl+shift+space',
            clipboard_mode       TEXT    NOT NULL DEFAULT 'paste',
            clipboard_restore    INTEGER NOT NULL DEFAULT 1,
            silence_threshold_db REAL    NOT NULL DEFAULT -40.0,
            max_recording_sec    INTEGER NOT NULL DEFAULT 60,
            language             TEXT,
            trigger_mode         TEXT    NOT NULL DEFAULT 'toggle',
            input_device         TEXT,
            updated_at           TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )?;
    add_column_if_missing(conn, "ALTER TABLE stt_config ADD COLUMN input_device TEXT")
}

/// Hosts of the cloud APIs that no longer have a dedicated provider.
const CLOUD_API_HOSTS: &[&str] = &["api.openai.com", "api.mistral.ai", "api.anthropic.com"];

/// Keys under which a backend address has been persisted, canonical first.
///
/// The desktop settings write `endpoint`, the CLI writes `base_url`, and the
/// TOML-to-database seed writes `api_url`.
pub(crate) const ADDRESS_KEYS: &[&str] = &["base_url", "endpoint", "api_url"];

/// Host of an address, lowercased, without scheme, credentials or port.
pub(crate) fn host_of(address: &str) -> String {
    let rest = address
        .trim()
        .split_once("://")
        .map_or(address.trim(), |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or(bracketed)
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// Whether a stored backend row points at a cloud API.
///
/// Every `anthropic` and `mistral` row is one. An `openai` row is one when it
/// designates no server (the client then defaulted to the OpenAI cloud) or
/// when its host is a known cloud API; any other `openai` row is a
/// self-hosted server and is kept.
fn is_cloud_row(provider: &str, config_json: &str) -> bool {
    match provider {
        "anthropic" | "mistral" => true,
        "openai" => {
            let config: serde_json::Value =
                serde_json::from_str(config_json).unwrap_or(serde_json::Value::Null);
            let address = ADDRESS_KEYS.iter().find_map(|key| {
                config
                    .get(*key)
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
            });
            match address {
                None => true,
                Some(address) => CLOUD_API_HOSTS.contains(&host_of(address).as_str()),
            }
        }
        _ => false,
    }
}

/// v2: remove the dedicated cloud providers.
///
/// Rows that reach a cloud API (see [`is_cloud_row`]) are deleted, and the
/// table is rebuilt so its `CHECK` constraint only accepts the providers that
/// remain. When a deleted row held the default, the default moves to the
/// local `llama-cpp` backend if there is one, and otherwise the file is left
/// without a default. Self-hosted `openai` rows are copied unchanged.
///
/// The step runs in one transaction, so a crash leaves the file at v1, and a
/// replay on a migrated table deletes nothing.
pub(crate) fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    let rows: Vec<(String, String, String, bool)> = {
        let mut stmt =
            conn.prepare("SELECT name, provider, config_json, is_default FROM llm_backends")?;
        let mapped = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })?;
        mapped.collect::<Result<_, _>>()?
    };
    let cloud: Vec<&(String, String, String, bool)> = rows
        .iter()
        .filter(|(_, provider, config_json, _)| is_cloud_row(provider, config_json))
        .collect();
    let default_removed = cloud.iter().any(|(_, _, _, is_default)| *is_default);

    let tx = conn.unchecked_transaction()?;
    for (name, _, _, _) in &cloud {
        tx.execute("DELETE FROM llm_backends WHERE name = ?1", params![name])?;
    }
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS llm_backends_v2 (
            name         TEXT PRIMARY KEY,
            provider     TEXT NOT NULL,
            model        TEXT NOT NULL,
            config_json  TEXT NOT NULL DEFAULT '{}',
            enabled      INTEGER NOT NULL DEFAULT 1,
            is_default   INTEGER NOT NULL DEFAULT 0,
            created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            CHECK (provider IN ('llama-cpp', 'openai', 'ollama'))
        );
        DELETE FROM llm_backends_v2;
        INSERT INTO llm_backends_v2
            (name, provider, model, config_json, enabled, is_default, created_at, updated_at)
            SELECT name, provider, model, config_json, enabled, is_default, created_at, updated_at
            FROM llm_backends;
        DROP TABLE llm_backends;
        ALTER TABLE llm_backends_v2 RENAME TO llm_backends;",
    )?;
    let default_moved_to: Option<String> = if default_removed {
        let local: Option<String> = tx
            .query_row(
                "SELECT name FROM llm_backends WHERE provider = 'llama-cpp' ORDER BY name LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(name) = &local {
            tx.execute(
                "UPDATE llm_backends SET is_default = 1 WHERE name = ?1",
                params![name],
            )?;
        }
        local
    } else {
        None
    };
    tx.commit()?;

    tracing::event!(
        tracing::Level::INFO,
        removed = cloud.len(),
        default_removed = default_removed,
        default_moved_to = default_moved_to.as_deref().unwrap_or(""),
        "llm.backends.cloud_removed"
    );
    Ok(())
}

/// How a backend row designates its key.
pub(crate) enum LegacyKey {
    None,
    Env(String),
    Literal,
}

pub(crate) fn legacy_key(config: &serde_json::Value) -> LegacyKey {
    match config
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(str::trim)
    {
        None | Some("") => LegacyKey::None,
        Some(raw) => match raw.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
            Some(var) if !var.is_empty() => LegacyKey::Env(var.to_string()),
            _ => LegacyKey::Literal,
        },
    }
}

/// The address a backend row designates, `None` when it designates none.
fn row_address(provider: &str, config: &serde_json::Value) -> Option<String> {
    let stored = ADDRESS_KEYS.iter().find_map(|key| {
        config
            .get(*key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    });
    match (provider, stored) {
        (_, Some(address)) => Some(address.trim_end_matches('/').to_string()),
        ("ollama", None) => Some(DEFAULT_OLLAMA_ADDRESS.to_string()),
        _ => None,
    }
}

/// v3: one server per distinct address of the `openai` and `ollama` rows.
///
/// The backend rows themselves are kept unchanged, so every name an agent or a
/// setting refers to stays routable. Two rows on the same address (compared on
/// their OpenAI API base, so `http://h:8000` and `http://h:8000/v1/` are one)
/// give one server, named after the first row by name. A `${VAR}` key becomes
/// the server's `api_key_env`; a key stored in clear is left for the runtime to
/// move into the secret store (see [`crate::LlmBackendRepository::pending_server_keys`]).
///
/// A replay creates nothing: an address that already has a server is skipped.
pub(crate) fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS inference_servers (
            name            TEXT PRIMARY KEY,
            base_url        TEXT NOT NULL,
            has_key         INTEGER NOT NULL DEFAULT 0,
            api_key_env     TEXT,
            legacy_key_from TEXT,
            enabled         INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )?;

    let rows: Vec<(String, String, String, bool)> = {
        let mut stmt = conn.prepare(
            "SELECT name, provider, config_json, enabled FROM llm_backends
             WHERE provider IN ('openai', 'ollama') ORDER BY name",
        )?;
        let mapped = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })?;
        mapped.collect::<Result<_, _>>()?
    };
    let mut known: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT name, base_url FROM inference_servers")?;
        let mapped = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        mapped
            .map(|r| r.map(|(name, url)| (name, openai_api_base(&url))))
            .collect::<Result<_, _>>()?
    };

    let tx = conn.unchecked_transaction()?;
    let mut created = 0usize;
    for (name, provider, config_json, enabled) in &rows {
        let config: serde_json::Value =
            serde_json::from_str(config_json).unwrap_or(serde_json::Value::Null);
        let Some(address) = row_address(provider, &config) else {
            continue;
        };
        let base = openai_api_base(&address);
        let (env, legacy_from) = match legacy_key(&config) {
            LegacyKey::None => (None, None),
            LegacyKey::Env(var) => (Some(var), None),
            LegacyKey::Literal => (None, Some(name.clone())),
        };
        if let Some((server, _)) = known.iter().find(|(_, b)| *b == base) {
            // Same address as an earlier row: the server exists, and it takes
            // this row's key only if it has none yet.
            tx.execute(
                "UPDATE inference_servers
                 SET api_key_env = COALESCE(api_key_env, ?2),
                     legacy_key_from = CASE
                        WHEN api_key_env IS NULL AND ?2 IS NULL AND has_key = 0
                        THEN COALESCE(legacy_key_from, ?3) ELSE legacy_key_from END,
                     enabled = MAX(enabled, ?4)
                 WHERE name = ?1",
                params![server, env, legacy_from, *enabled as i64],
            )?;
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO inference_servers
                (name, base_url, api_key_env, legacy_key_from, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, address, env, legacy_from, *enabled as i64],
        )?;
        known.push((name.clone(), base));
        created += 1;
    }
    tx.commit()?;

    tracing::event!(
        tracing::Level::INFO,
        created = created,
        "llm.servers.migrated"
    );
    Ok(())
}

/// v4: the model presets, the model of each agent, and one default backend.
///
/// - `model_presets`: a named set of load and request settings attached to a
///   model. A model has at most one default preset, held by a partial unique
///   index, and neither settings column may carry a system prompt, held by a
///   `CHECK`: the system prompt is the agent's mission, not a setting.
/// - `agent_model`: the model the user assigned to an agent, local or on a
///   server, and the preset chosen for it, if any. Kept apart from the
///   installed package, so updating the agent leaves the choice in place. A
///   deleted preset is cleared from every agent that chose it, by a trigger.
/// - `llm_backends.is_default`: a file holding several defaults (written by
///   hand, or by a binary that lost a race) keeps the first by name, and a
///   partial unique index then refuses a second one.
///
/// Every statement is idempotent, so a replay changes nothing.
pub(crate) fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS model_presets (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            name             TEXT NOT NULL CHECK (length(trim(name)) > 0),
            model_source     TEXT NOT NULL CHECK (model_source IN ('local', 'server')),
            model_id         TEXT NOT NULL CHECK (length(trim(model_id)) > 0),
            load_json        TEXT NOT NULL DEFAULT '{{}}',
            request_json     TEXT NOT NULL DEFAULT '{{}}',
            is_model_default INTEGER NOT NULL DEFAULT 0 CHECK (is_model_default IN (0, 1)),
            created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            UNIQUE (model_source, model_id, name),
            CHECK (json_valid(load_json) AND json_type(load_json) = 'object'),
            CHECK (json_valid(request_json) AND json_type(request_json) = 'object'),
            CHECK ({no_prompt})
        );

        CREATE UNIQUE INDEX IF NOT EXISTS model_presets_one_default_per_model
            ON model_presets (model_source, model_id) WHERE is_model_default = 1;

        CREATE TABLE IF NOT EXISTS agent_model (
            agent        TEXT PRIMARY KEY CHECK (length(trim(agent)) > 0),
            model_source TEXT NOT NULL CHECK (model_source IN ('local', 'server')),
            model_id     TEXT NOT NULL CHECK (length(trim(model_id)) > 0),
            preset_id    INTEGER,
            updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

        CREATE TRIGGER IF NOT EXISTS model_presets_detach_from_agents
            AFTER DELETE ON model_presets
            BEGIN
                UPDATE agent_model SET preset_id = NULL WHERE preset_id = OLD.id;
            END;

        UPDATE llm_backends SET is_default = 0
            WHERE is_default = 1
              AND name != (SELECT name FROM llm_backends WHERE is_default = 1
                           ORDER BY name LIMIT 1);

        CREATE UNIQUE INDEX IF NOT EXISTS llm_backends_one_default
            ON llm_backends (is_default) WHERE is_default = 1;",
        no_prompt = no_system_prompt_check(),
    ))?;
    tx.commit()
}

/// v5: the default embeddings model, a singleton row.
///
/// `model_id` names an entry of the embeddings category of the registry, the
/// one search by meaning loads when it needs a vector. No row means no default,
/// and search then falls back to keywords alone. The statement is idempotent,
/// so a replay changes nothing.
pub(crate) fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS embedding_default (
            id         INTEGER PRIMARY KEY CHECK (id = 1),
            model_id   TEXT NOT NULL CHECK (length(trim(model_id)) > 0),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )
}

/// v6: the tokens of the compatible server.
///
/// One row per token, named by the person who created it. The value itself is
/// never stored: `hash` is the hex SHA-256 of it, and `last_four` its last four
/// characters, enough to recognise it in a list. A revoked token keeps its row,
/// with `revoked_at` set, so the list still shows it. Two tokens in use never
/// share a name, held by a partial unique index; a revoked token frees its
/// name. Every statement is idempotent, so a replay changes nothing.
pub(crate) fn migrate_v6(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS server_tokens (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            name         TEXT NOT NULL CHECK (length(trim(name)) > 0),
            hash         TEXT NOT NULL UNIQUE CHECK (length(hash) = 64),
            last_four    TEXT NOT NULL CHECK (length(last_four) = 4),
            created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            last_used_at TEXT,
            revoked_at   TEXT
        );

        CREATE UNIQUE INDEX IF NOT EXISTS server_tokens_one_active_name
            ON server_tokens (name) WHERE revoked_at IS NULL;",
    )
}

/// v7: the last local model loaded by hand, a singleton row.
///
/// `model_id` is the identifier the model index gives the model, and
/// `load_json` the load settings it was loaded with, in the keys a preset's
/// load settings use. Like a preset, the row never carries a system prompt.
/// The statement is idempotent, so a replay changes nothing.
pub(crate) fn migrate_v7(conn: &Connection) -> Result<(), rusqlite::Error> {
    let no_prompt = SYSTEM_PROMPT_KEYS
        .iter()
        .map(|key| format!("json_type(load_json, '$.{key}') IS NULL"))
        .collect::<Vec<_>>()
        .join(" AND ");
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS last_loaded_model (
            id        INTEGER PRIMARY KEY CHECK (id = 1),
            model_id  TEXT NOT NULL CHECK (length(trim(model_id)) > 0),
            load_json TEXT NOT NULL DEFAULT '{{}}',
            loaded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            CHECK (json_valid(load_json) AND json_type(load_json) = 'object'),
            CHECK ({no_prompt})
        );"
    ))
}

/// v8: the registry of sources, what an agent may consult once checked.
///
/// The kinds and index states are held by `CHECK`, and so is the rule that a
/// folder, a file or a mailbox names what it points at. The statement is
/// idempotent, so a replay changes nothing.
pub(crate) fn migrate_v8(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sources (
            id          TEXT PRIMARY KEY CHECK (length(trim(id)) > 0),
            kind        TEXT NOT NULL CHECK (kind IN
                ('folder', 'file', 'mail', 'profile', 'memory', 'code_folder')),
            label       TEXT NOT NULL CHECK (length(trim(label)) > 0),
            group_name  TEXT,
            reference   TEXT,
            index_state TEXT NOT NULL DEFAULT 'absent' CHECK (index_state IN
                ('absent', 'indexing', 'ready', 'error', 'keywords_only')),
            added_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            CHECK (kind NOT IN ('folder', 'file', 'mail')
                   OR length(trim(coalesce(reference, ''))) > 0)
        );",
    )
}

/// Keys a preset may never carry, in either settings column.
pub(crate) const SYSTEM_PROMPT_KEYS: &[&str] = &["system_prompt", "system", "system_message"];

/// The `CHECK` expression refusing every [`SYSTEM_PROMPT_KEYS`] entry at the
/// top level of `load_json` and `request_json`.
fn no_system_prompt_check() -> String {
    let mut terms = Vec::new();
    for column in ["load_json", "request_json"] {
        for key in SYSTEM_PROMPT_KEYS {
            terms.push(format!("json_type({column}, '$.{key}') IS NULL"));
        }
    }
    terms.join(" AND ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DataFile;
    use crate::schema::{open_versioned, SchemaError};

    /// Pre-versioning `system.db` schema, frozen as the oldest shape a
    /// published binary wrote (no `input_device` column, `user_version = 0`).
    const SYSTEM_V0_SQL: &str = include_str!("../tests/fixtures/schemas/system_v0.sql");

    fn open_file_db(dir: &tempfile::TempDir) -> Connection {
        Connection::open(dir.path().join(DataFile::System.file_name())).unwrap()
    }

    // GIVEN a database written by a pre-versioning binary, with rows
    // WHEN opening it through the versioned layer
    // THEN the rows survive, the missing column is added and the version is stamped
    #[test]
    fn test_system_db_old_format_migrates_and_keeps_rows() {
        let dir = tempfile::TempDir::new().unwrap();
        {
            let conn = open_file_db(&dir);
            conn.execute_batch(SYSTEM_V0_SQL).unwrap();
            conn.execute(
                "INSERT INTO llm_backends (name, provider, model, is_default)
                 VALUES ('local', 'llama-cpp', '/m.gguf', 1)",
                [],
            )
            .unwrap();
            conn.execute("INSERT INTO stt_config (id, enabled) VALUES (1, 1)", [])
                .unwrap();
        }

        let conn = open_file_db(&dir);
        open_versioned(
            &conn,
            DataFile::System.file_name(),
            SYSTEM_DB_SCHEMA_VERSION,
            SYSTEM_DB_MIGRATIONS,
        )
        .unwrap();

        let name: String = conn
            .query_row("SELECT name FROM llm_backends", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "local");
        let device: Option<String> = conn
            .query_row("SELECT input_device FROM stt_config", [], |row| row.get(0))
            .unwrap();
        assert!(device.is_none());
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(SYSTEM_DB_SCHEMA_VERSION));
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it through the versioned layer
    // THEN the open is refused
    #[test]
    fn test_system_db_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", SYSTEM_DB_SCHEMA_VERSION + 1)
            .unwrap();

        let err = open_versioned(
            &conn,
            DataFile::System.file_name(),
            SYSTEM_DB_SCHEMA_VERSION,
            SYSTEM_DB_MIGRATIONS,
        )
        .unwrap_err();

        assert!(matches!(err, SchemaError::NewerThanBinary { .. }));
    }

    /// One `llm_backends` row as the v1 schema stores it.
    struct Row {
        name: &'static str,
        provider: &'static str,
        model: &'static str,
        config_json: &'static str,
        is_default: bool,
    }

    /// The eight rows of the cloud removal scenario: three cloud providers,
    /// two `openai` rows on a cloud host, one `openai` row with no address,
    /// two self-hosted `openai` servers, and the local engine.
    fn cloud_scenario_rows(default_name: &str, with_local: bool) -> Vec<Row> {
        let mut rows = vec![
            Row {
                name: "claude",
                provider: "anthropic",
                model: "claude-sonnet-4-6",
                config_json: r#"{"api_key":"sk-ant"}"#,
                is_default: false,
            },
            Row {
                name: "mistral-small",
                provider: "mistral",
                model: "mistral-small-latest",
                config_json: r#"{"api_key":"m-key"}"#,
                is_default: false,
            },
            Row {
                name: "gpt",
                provider: "openai",
                model: "gpt-4o",
                config_json: r#"{"base_url":"https://api.openai.com/v1","api_key":"sk-oa"}"#,
                is_default: false,
            },
            Row {
                name: "mistral-compat",
                provider: "openai",
                model: "mistral-large",
                config_json: r#"{"endpoint":"https://api.mistral.ai/v1"}"#,
                is_default: false,
            },
            Row {
                name: "no-address",
                provider: "openai",
                model: "gpt-4o-mini",
                config_json: r#"{"api_key":"sk-oa2"}"#,
                is_default: false,
            },
            Row {
                name: "vllm",
                provider: "openai",
                model: "qwen3-32b",
                config_json: r#"{"base_url":"http://10.0.0.5:8000/v1","api_key":"vllm-key"}"#,
                is_default: false,
            },
            Row {
                name: "lmstudio",
                provider: "openai",
                model: "gemma-3-12b",
                config_json: r#"{"api_url":"http://localhost:1234/v1"}"#,
                is_default: false,
            },
        ];
        if with_local {
            rows.push(Row {
                name: "local",
                provider: "llama-cpp",
                model: "/models/m.gguf",
                config_json: "{}",
                is_default: false,
            });
        }
        for row in &mut rows {
            row.is_default = row.name == default_name;
        }
        rows
    }

    /// A `system.db` at schema v1 holding `rows`, as a v1 binary left it.
    fn v1_database_with(rows: &[Row]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate_v1(&conn).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        for row in rows {
            conn.execute(
                "INSERT INTO llm_backends (name, provider, model, config_json, is_default)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    row.name,
                    row.provider,
                    row.model,
                    row.config_json,
                    row.is_default as i64
                ],
            )
            .unwrap();
        }
        conn
    }

    /// Every remaining row as `(name, provider, model, config_json, is_default)`, by name.
    fn remaining(conn: &Connection) -> Vec<(String, String, String, String, bool)> {
        let mut stmt = conn
            .prepare(
                "SELECT name, provider, model, config_json, is_default FROM llm_backends ORDER BY name",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get::<_, i64>(4)? != 0,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    fn open_current(conn: &Connection) {
        open_versioned(
            conn,
            DataFile::System.file_name(),
            SYSTEM_DB_SCHEMA_VERSION,
            SYSTEM_DB_MIGRATIONS,
        )
        .unwrap();
    }

    // GIVEN a v1 database with eight backends, three cloud providers, two
    //       `openai` rows on a cloud host, one `openai` row with no address,
    //       two self-hosted `openai` servers and the local engine, the
    //       default being a deleted cloud row
    // WHEN the database is opened at the current version
    // THEN only the two self-hosted servers, unchanged, and the local engine
    //      remain, the default moved to the local engine, and the rebuilt
    //      CHECK constraint refuses an `anthropic` row
    #[test]
    fn test_system_db_v2_removes_cloud_backends_and_keeps_self_hosted_servers() {
        // GIVEN
        let conn = v1_database_with(&cloud_scenario_rows("claude", true));

        // WHEN
        open_current(&conn);

        // THEN
        let rows = remaining(&conn);
        assert_eq!(
            rows,
            vec![
                (
                    "lmstudio".into(),
                    "openai".into(),
                    "gemma-3-12b".into(),
                    r#"{"api_url":"http://localhost:1234/v1"}"#.into(),
                    false
                ),
                (
                    "local".into(),
                    "llama-cpp".into(),
                    "/models/m.gguf".into(),
                    "{}".into(),
                    true
                ),
                (
                    "vllm".into(),
                    "openai".into(),
                    "qwen3-32b".into(),
                    r#"{"base_url":"http://10.0.0.5:8000/v1","api_key":"vllm-key"}"#.into(),
                    false
                ),
            ]
        );
        let refused = conn.execute(
            "INSERT INTO llm_backends (name, provider, model) VALUES ('back', 'anthropic', 'claude')",
            [],
        );
        assert!(
            matches!(&refused, Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation),
            "an anthropic row must violate the CHECK constraint, got {refused:?}"
        );
    }

    // GIVEN a deleted cloud row holding the default, once with a local
    //       engine row and once without one
    // WHEN each database is opened at the current version
    // THEN the default moves to the local engine in the first case, and the
    //      second database is left without any default
    #[test]
    fn test_system_db_v2_moves_a_removed_default_to_the_local_engine_or_to_none() {
        // GIVEN
        let with_local = v1_database_with(&cloud_scenario_rows("gpt", true));
        let without_local = v1_database_with(&cloud_scenario_rows("no-address", false));

        // WHEN
        open_current(&with_local);
        open_current(&without_local);

        // THEN
        let defaults = |conn: &Connection| -> Vec<String> {
            remaining(conn)
                .into_iter()
                .filter(|row| row.4)
                .map(|row| row.0)
                .collect()
        };
        assert_eq!(defaults(&with_local), vec!["local".to_string()]);
        assert!(defaults(&without_local).is_empty());
        let names: Vec<String> = remaining(&without_local).into_iter().map(|r| r.0).collect();
        assert_eq!(names, vec!["lmstudio".to_string(), "vllm".to_string()]);
    }

    // GIVEN a self-hosted server holding the default among cloud rows
    // WHEN the database is opened at the current version
    // THEN the self-hosted server keeps the default
    #[test]
    fn test_system_db_v2_keeps_a_self_hosted_default() {
        // GIVEN
        let conn = v1_database_with(&cloud_scenario_rows("vllm", true));

        // WHEN
        open_current(&conn);

        // THEN
        let defaults: Vec<String> = remaining(&conn)
            .into_iter()
            .filter(|row| row.4)
            .map(|row| row.0)
            .collect();
        assert_eq!(defaults, vec!["vllm".to_string()]);
    }

    // GIVEN addresses written with a scheme, credentials, a port, a path or
    //       an uppercase host
    // WHEN the host is extracted
    // THEN only the bare lowercase host remains
    #[test]
    fn test_host_of_strips_scheme_credentials_port_and_path() {
        // GIVEN
        let cases = [
            ("https://api.openai.com/v1", "api.openai.com"),
            ("HTTPS://API.Mistral.AI:443/v1/", "api.mistral.ai"),
            ("https://user:pw@api.anthropic.com/v1", "api.anthropic.com"),
            ("http://10.0.0.5:8000/v1", "10.0.0.5"),
            ("http://[::1]:1234/v1", "::1"),
            ("localhost:1234/v1", "localhost"),
        ];

        for (address, expected) in cases {
            // WHEN
            let host = host_of(address);

            // THEN
            assert_eq!(host, expected, "{address}");
        }
    }
}
