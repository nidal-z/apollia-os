//! Inference servers declared by the organization, persisted in `system.db`.
//!
//! A server is one address that speaks the OpenAI protocol (Ollama, vLLM, TGI,
//! a LiteLLM gateway...) and an optional key. Its models are not stored: the
//! server lists them on `GET /v1/models`, and each one is routable as
//! `<server>/<model>` (see [`route_name`]), so a server with N models is one
//! row, not N.
//!
//! The key never lands in the database. It is held by the secret store under
//! [`INFERENCE_SERVER_SECRET_SERVICE`], with the server name as account, and
//! the row only records that one exists (`has_key`) or names the environment
//! variable that holds it (`api_key_env`).

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::llm_backend::{validate_name, LlmBackendError, LlmBackendRepository};
use crate::system_db::{host_of, legacy_key, LegacyKey};

/// Secret store service under which a server key is kept, account = server name.
pub const INFERENCE_SERVER_SECRET_SERVICE: &str = "apollia-inference-server";

/// Address an `ollama` backend reaches when it designates none.
pub const DEFAULT_OLLAMA_ADDRESS: &str = "http://127.0.0.1:11434/v1";

/// One inference server, as `system.db` stores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceServer {
    /// Unique name, pattern `[a-z0-9_-]+`. Also the secret store account.
    pub name: String,
    /// Base address, with or without the trailing `/v1`.
    pub base_url: String,
    /// Whether a key for this server is held by the secret store.
    pub has_key: bool,
    /// Environment variable holding the key, for a server whose key was
    /// declared as `${VAR}`. Read when `has_key` is `false`.
    pub api_key_env: Option<String>,
    /// When `false`, the server's models are not routable.
    pub enabled: bool,
}

/// The OpenAI API base of a server address: the address itself when it ends
/// with `/v1`, the address followed by `/v1` otherwise.
pub fn openai_api_base(address: &str) -> String {
    let trimmed = address.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

/// The URL that lists a server's models: `<address>/v1/models`, or
/// `<address>/models` when the address already ends with `/v1`.
pub fn models_url(address: &str) -> String {
    format!("{}/models", openai_api_base(address))
}

/// The routing name of one model of a server, `<server>/<model>`.
///
/// A backend name never holds a `/` (`[a-z0-9_-]+`), so the two namespaces
/// cannot collide.
pub fn route_name(server: &str, model: &str) -> String {
    format!("{server}/{model}")
}

/// Split a routing name into `(server, model)`. `None` for a plain backend name.
///
/// Only the first `/` separates: a model identifier may carry its own
/// (`Qwen/Qwen3-32B` on vLLM).
pub fn split_route_name(name: &str) -> Option<(&str, &str)> {
    let (server, model) = name.split_once('/')?;
    (!server.is_empty() && !model.is_empty()).then_some((server, model))
}

/// Refuse a server name out of the `[a-z0-9_-]+` pattern.
pub fn validate_server_name(name: &str) -> Result<(), LlmBackendError> {
    validate_name(name)
}

/// Refuse an address that is not an `http://` or `https://` URL with a host.
pub fn validate_address(address: &str) -> Result<(), LlmBackendError> {
    let trimmed = address.trim();
    let lower = trimmed.to_ascii_lowercase();
    let has_scheme = lower.starts_with("http://") || lower.starts_with("https://");
    if !has_scheme || host_of(trimmed).is_empty() {
        return Err(LlmBackendError::InvalidAddress(address.to_string()));
    }
    Ok(())
}

fn row_to_server(row: &rusqlite::Row<'_>) -> Result<InferenceServer, rusqlite::Error> {
    Ok(InferenceServer {
        name: row.get(0)?,
        base_url: row.get(1)?,
        has_key: row.get::<_, i64>(2)? != 0,
        api_key_env: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
    })
}

const SELECT_SERVER: &str =
    "SELECT name, base_url, has_key, api_key_env, enabled FROM inference_servers";

impl LlmBackendRepository {
    /// Every declared server, sorted by name.
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] for any SQLite error.
    pub fn list_servers(&self) -> Result<Vec<InferenceServer>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(&format!("{SELECT_SERVER} ORDER BY name"))?;
        let rows = stmt.query_map([], row_to_server)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The server named `name`, or `None`.
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] for any SQLite error.
    pub fn find_server(&self, name: &str) -> Result<Option<InferenceServer>, LlmBackendError> {
        let conn = self.conn.borrow();
        Ok(conn
            .query_row(
                &format!("{SELECT_SERVER} WHERE name = ?1"),
                params![name],
                row_to_server,
            )
            .optional()?)
    }

    /// Create or update a server. The key itself is written to the secret
    /// store by the caller; this records only whether there is one.
    ///
    /// # Errors
    /// - [`LlmBackendError::InvalidName`] if the name does not match `[a-z0-9_-]+`.
    /// - [`LlmBackendError::InvalidAddress`] if the address is not an HTTP URL.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    pub fn save_server(&self, server: &InferenceServer) -> Result<(), LlmBackendError> {
        validate_name(&server.name)?;
        validate_address(&server.base_url)?;
        self.conn.borrow().execute(
            "INSERT INTO inference_servers (name, base_url, has_key, api_key_env, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET
                base_url        = excluded.base_url,
                has_key         = excluded.has_key,
                api_key_env     = excluded.api_key_env,
                enabled         = excluded.enabled,
                legacy_key_from = NULL,
                updated_at      = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![
                server.name,
                server.base_url.trim().trim_end_matches('/'),
                server.has_key as i64,
                server.api_key_env,
                server.enabled as i64,
            ],
        )?;
        Ok(())
    }

    /// Delete a server. Its key, if any, is removed by the caller.
    ///
    /// # Errors
    /// - [`LlmBackendError::NotFound`] if `name` is absent.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    pub fn delete_server(&self, name: &str) -> Result<(), LlmBackendError> {
        let deleted = self.conn.borrow().execute(
            "DELETE FROM inference_servers WHERE name = ?1",
            params![name],
        )?;
        if deleted == 0 {
            return Err(LlmBackendError::NotFound(name.to_string()));
        }
        Ok(())
    }

    /// Keys a migrated server still has to move into the secret store, as
    /// `(server, key)`.
    ///
    /// The schema migration cannot reach the secret store, so a backend row
    /// that carried its key in clear leaves its server pointing at it; the
    /// runtime moves the key at startup and calls [`Self::mark_server_key_stored`].
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] for any SQLite error.
    pub fn pending_server_keys(&self) -> Result<Vec<(String, String)>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT s.name, b.config_json FROM inference_servers s
             JOIN llm_backends b ON b.name = s.legacy_key_from
             ORDER BY s.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut pending = Vec::new();
        for row in rows {
            let (server, config_json) = row?;
            if let Some(key) = literal_key(&config_json) {
                pending.push((server, key));
            }
        }
        Ok(pending)
    }

    /// Record that the key of `name` now sits in the secret store.
    ///
    /// # Errors
    /// Returns [`LlmBackendError::Db`] for any SQLite error.
    pub fn mark_server_key_stored(&self, name: &str) -> Result<(), LlmBackendError> {
        self.conn.borrow().execute(
            "UPDATE inference_servers
             SET has_key = 1, legacy_key_from = NULL,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }
}

fn literal_key(config_json: &str) -> Option<String> {
    let config: serde_json::Value = serde_json::from_str(config_json).ok()?;
    match legacy_key(&config) {
        LegacyKey::Literal => config
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DataFile;
    use crate::schema::open_versioned;
    use crate::system_db::{
        migrate_v1, migrate_v2, migrate_v3, SYSTEM_DB_MIGRATIONS, SYSTEM_DB_SCHEMA_VERSION,
    };
    use rusqlite::Connection;

    /// A `system.db` at schema v2 holding `rows` as
    /// `(name, provider, model, config_json)`, as a v2 binary left it.
    fn v2_database_with(dir: &tempfile::TempDir, rows: &[(&str, &str, &str, &str)]) {
        let conn = Connection::open(dir.path().join(DataFile::System.file_name())).unwrap();
        migrate_v1(&conn).unwrap();
        migrate_v2(&conn).unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
        for (name, provider, model, config_json) in rows {
            conn.execute(
                "INSERT INTO llm_backends (name, provider, model, config_json) VALUES (?1, ?2, ?3, ?4)",
                params![name, provider, model, config_json],
            )
            .unwrap();
        }
    }

    fn open_repo(dir: &tempfile::TempDir) -> LlmBackendRepository {
        LlmBackendRepository::open(&dir.path().join(DataFile::System.file_name())).unwrap()
    }

    // GIVEN a v2 database with two `openai` rows on one vLLM server (one
    //       written with `/v1`, one without), one `openai` row on LM Studio,
    //       one `ollama` row with no address and one with the default one,
    //       and the local engine
    // WHEN the database is opened at the current version
    // THEN each distinct address became one server, and every old row is
    //      still found under its name, unchanged
    #[test]
    fn test_v3_turns_each_distinct_address_into_a_server_and_keeps_old_names() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        let rows = [
            (
                "vllm-qwen",
                "openai",
                "qwen3-32b",
                r#"{"base_url":"http://10.0.0.5:8000/v1","api_key":"${VLLM_KEY}"}"#,
            ),
            (
                "vllm-llama",
                "openai",
                "llama-3.3-70b",
                r#"{"endpoint":"http://10.0.0.5:8000/"}"#,
            ),
            (
                "lmstudio",
                "openai",
                "gemma-3-12b",
                r#"{"api_url":"http://localhost:1234/v1","api_key":"lm-secret"}"#,
            ),
            ("ollama-a", "ollama", "llama3.2", "{}"),
            (
                "ollama-b",
                "ollama",
                "mistral",
                r#"{"base_url":"http://127.0.0.1:11434/v1"}"#,
            ),
            ("local", "llama-cpp", "/models/m.gguf", "{}"),
        ];
        v2_database_with(&dir, &rows);

        // WHEN
        let repo = open_repo(&dir);

        // THEN
        let servers = repo.list_servers().unwrap();
        let summary: Vec<(String, String, Option<String>)> = servers
            .iter()
            .map(|s| (s.name.clone(), s.base_url.clone(), s.api_key_env.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("lmstudio".into(), "http://localhost:1234/v1".into(), None),
                ("ollama-a".into(), DEFAULT_OLLAMA_ADDRESS.into(), None),
                (
                    "vllm-llama".into(),
                    "http://10.0.0.5:8000".into(),
                    Some("VLLM_KEY".into())
                ),
            ]
        );
        assert!(servers.iter().all(|s| !s.has_key && s.enabled));
        for (name, provider, model, config_json) in rows {
            let row = repo
                .find_by_name(name)
                .unwrap()
                .unwrap_or_else(|| panic!("{name} lost"));
            assert_eq!(row.provider.to_string(), provider);
            assert_eq!(row.model, model);
            assert_eq!(
                row.config_json,
                serde_json::from_str::<serde_json::Value>(config_json).unwrap()
            );
        }
        assert_eq!(
            repo.list().unwrap().len(),
            6,
            "no backend row is added or removed"
        );
    }

    // GIVEN a v2 database whose `openai` row carries its key in clear
    // WHEN the database is opened, then the key is reported and marked stored
    // THEN the key is reported once for its server, and no longer after
    #[test]
    fn test_v3_leaves_a_clear_key_for_the_secret_store_and_forgets_it_once_moved() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        v2_database_with(
            &dir,
            &[(
                "gateway",
                "openai",
                "m",
                r#"{"base_url":"https://llm.corp.example/v1","api_key":"sk-live"}"#,
            )],
        );
        let repo = open_repo(&dir);

        // WHEN
        let pending = repo.pending_server_keys().unwrap();
        repo.mark_server_key_stored("gateway").unwrap();

        // THEN
        assert_eq!(
            pending,
            vec![("gateway".to_string(), "sk-live".to_string())]
        );
        assert!(repo.pending_server_keys().unwrap().is_empty());
        assert!(repo.find_server("gateway").unwrap().unwrap().has_key);
    }

    // GIVEN a database already at the current version, with its servers
    // WHEN the v3 step is replayed
    // THEN no server is added or changed
    #[test]
    fn test_v3_replay_creates_nothing() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        v2_database_with(
            &dir,
            &[(
                "vllm",
                "openai",
                "m",
                r#"{"base_url":"http://10.0.0.5:8000/v1"}"#,
            )],
        );
        let before = open_repo(&dir).list_servers().unwrap();

        // WHEN
        let conn = Connection::open(dir.path().join(DataFile::System.file_name())).unwrap();
        migrate_v3(&conn).unwrap();
        open_versioned(
            &conn,
            DataFile::System.file_name(),
            SYSTEM_DB_SCHEMA_VERSION,
            SYSTEM_DB_MIGRATIONS,
        )
        .unwrap();

        // THEN
        assert_eq!(open_repo(&dir).list_servers().unwrap(), before);
    }

    // GIVEN a server saved, then saved again with another address
    // WHEN the servers are listed, then the server is deleted
    // THEN the second address won, and the deletion leaves none
    #[test]
    fn test_save_server_upserts_and_delete_removes_it() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        let repo = open_repo(&dir);
        let mut server = InferenceServer {
            name: "corp".into(),
            base_url: "http://10.0.0.9:8000/".into(),
            has_key: true,
            api_key_env: None,
            enabled: true,
        };
        repo.save_server(&server).unwrap();
        server.base_url = "http://10.0.0.10:8000".into();
        repo.save_server(&server).unwrap();

        // WHEN
        let listed = repo.list_servers().unwrap();
        repo.delete_server("corp").unwrap();

        // THEN
        assert_eq!(listed, vec![server]);
        assert!(repo.list_servers().unwrap().is_empty());
        assert!(matches!(
            repo.delete_server("corp"),
            Err(LlmBackendError::NotFound(_))
        ));
    }

    // GIVEN addresses that are not HTTP URLs, and names out of the pattern
    // WHEN a server is saved with each
    // THEN every one is refused
    #[test]
    fn test_save_server_refuses_a_bad_address_or_name() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        let repo = open_repo(&dir);
        let server = |name: &str, base_url: &str| InferenceServer {
            name: name.into(),
            base_url: base_url.into(),
            has_key: false,
            api_key_env: None,
            enabled: true,
        };

        for address in ["", "10.0.0.5:8000", "ftp://host/v1", "http://"] {
            // WHEN
            let result = repo.save_server(&server("ok", address));

            // THEN
            assert!(
                matches!(result, Err(LlmBackendError::InvalidAddress(_))),
                "{address:?}"
            );
        }
        assert!(matches!(
            repo.save_server(&server("Bad/Name", "http://h")),
            Err(LlmBackendError::InvalidName(_))
        ));
    }

    // GIVEN addresses with and without a trailing `/v1`
    // WHEN their models URL and API base are derived
    // THEN `/v1` is never doubled nor omitted
    #[test]
    fn test_models_url_appends_v1_only_when_missing() {
        // GIVEN
        let cases = [
            ("http://h:8000", "http://h:8000/v1/models"),
            ("http://h:8000/", "http://h:8000/v1/models"),
            ("http://h:8000/v1", "http://h:8000/v1/models"),
            (
                "https://gw.example/litellm/v1/",
                "https://gw.example/litellm/v1/models",
            ),
        ];

        for (address, expected) in cases {
            // WHEN
            let url = models_url(address);

            // THEN
            assert_eq!(url, expected, "{address}");
        }
    }

    // GIVEN routing names with and without a server part
    // WHEN they are split
    // THEN only `<server>/<model>` splits, on its first slash
    #[test]
    fn test_split_route_name() {
        // GIVEN / WHEN / THEN
        assert_eq!(
            split_route_name("vllm/Qwen/Qwen3-32B"),
            Some(("vllm", "Qwen/Qwen3-32B"))
        );
        assert_eq!(
            split_route_name(&route_name("srv", "m")),
            Some(("srv", "m"))
        );
        assert_eq!(split_route_name("local"), None);
        assert_eq!(split_route_name("/m"), None);
        assert_eq!(split_route_name("srv/"), None);
    }
}
