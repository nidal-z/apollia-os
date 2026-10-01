//! Versioned schema of `system.db`, shared by every store in the file.
//!
//! `system.db` holds two singleton stores: the LLM backend registry
//! ([`crate::llm_backend`]) and the STT configuration ([`crate::stt_config`]).
//! `PRAGMA user_version` belongs to the database file, not to a table, so the
//! stores share one migration list: whichever opens first brings the whole
//! file to the current version, and both refuse a file written by a newer
//! binary.

use rusqlite::{params, Connection, OptionalExtension};

use crate::schema::{add_column_if_missing, Migration};

/// Current schema version of `system.db`.
pub(crate) const SYSTEM_DB_SCHEMA_VERSION: u32 = 2;

/// Numbered migration steps of `system.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`SYSTEM_DB_SCHEMA_VERSION`].
pub(crate) const SYSTEM_DB_MIGRATIONS: &[Migration] = &[migrate_v1, migrate_v2];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `system.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the initial schema, and the schema after the additive
/// `input_device` column, and bring each of them to the same state.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
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
const ADDRESS_KEYS: &[&str] = &["base_url", "endpoint", "api_url"];

/// Host of an address, lowercased, without scheme, credentials or port.
fn host_of(address: &str) -> String {
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
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
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
