use std::path::Path;

use apollia_mcp::config::MCP_STATIC_SECRET_SERVICE;

use super::*;
use crate::api::test_support::MemorySecrets;

/// An `mcp.db` as version 0.3.0 wrote it (schema 5): a remote server with
/// its header in clear, a process with a key in clear and a plain setting,
/// and a process whose key is already a placeholder.
fn mcp_db_of_0_3_0(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE mcp_servers (
            name               TEXT PRIMARY KEY,
            command            TEXT NOT NULL DEFAULT '',
            args_json          TEXT NOT NULL DEFAULT '[]',
            env_json           TEXT NOT NULL DEFAULT '{}',
            transport          TEXT NOT NULL DEFAULT 'stdio',
            url                TEXT,
            init_timeout_secs  INTEGER NOT NULL DEFAULT 30,
            call_timeout_secs  INTEGER NOT NULL DEFAULT 60,
            max_response_bytes INTEGER NOT NULL DEFAULT 8388608,
            max_tools          INTEGER NOT NULL DEFAULT 256,
            tags_json          TEXT NOT NULL DEFAULT '[]',
            enabled            INTEGER NOT NULL DEFAULT 1,
            created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            tool_families_json TEXT NOT NULL DEFAULT '{}',
            provenance         TEXT,
            official           INTEGER NOT NULL DEFAULT 0,
            selection_entry    TEXT
        );
        INSERT INTO mcp_servers (name, env_json, transport, url) VALUES
            ('remote', '{\"Authorization\":\"Bearer sk-clear\"}', 'streamable-http',
             'https://mcp.example.org/mcp');
        INSERT INTO mcp_servers (name, command, env_json) VALUES
            ('local', 'npx', '{\"ACME_API_KEY\":\"k-clear\",\"LOG_LEVEL\":\"debug\"}'),
            ('filed', 'npx', '{\"ACME_API_KEY\":\"${APOLLIA_SECRET:ACME_API_KEY}\"}');
        PRAGMA user_version = 5;",
    )
    .unwrap();
}

fn env_json(path: &Path, name: &str) -> String {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT env_json FROM mcp_servers WHERE name = ?1",
            [name],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn test_the_secrets_an_mcp_db_of_0_3_0_keeps_in_clear_move_once() {
    // GIVEN an mcp.db of 0.3.0 holding two secrets in clear
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("mcp.db");
    mcp_db_of_0_3_0(&path);
    let secrets = MemorySecrets::default();

    // WHEN the startup step runs twice
    let first = move_mcp_secrets_into_store(&path, &secrets.opener());
    let second = move_mcp_secrets_into_store(&path, &secrets.opener());

    // THEN the two values moved at the first run and nothing at the second
    assert_eq!((first, second), (2, 0));
    // AND the store holds them, mcp.db their placeholders only
    assert_eq!(
        secrets
            .value(MCP_STATIC_SECRET_SERVICE, "remote:Authorization")
            .as_deref(),
        Some("Bearer sk-clear")
    );
    assert_eq!(
        secrets
            .value(MCP_STATIC_SECRET_SERVICE, "local:ACME_API_KEY")
            .as_deref(),
        Some("k-clear")
    );
    let remote = env_json(&path, "remote");
    let local = env_json(&path, "local");
    assert!(remote.contains("${APOLLIA_SECRET:Authorization}"));
    assert!(!remote.contains("sk-clear"));
    assert!(local.contains("${APOLLIA_SECRET:ACME_API_KEY}"));
    assert!(local.contains("\"LOG_LEVEL\":\"debug\""));
    assert!(!local.contains("k-clear"));
    // AND the server already filed is untouched, with nothing stored for it
    assert_eq!(
        env_json(&path, "filed"),
        "{\"ACME_API_KEY\":\"${APOLLIA_SECRET:ACME_API_KEY}\"}"
    );
    assert_eq!(
        secrets.value(MCP_STATIC_SECRET_SERVICE, "filed:ACME_API_KEY"),
        None
    );
}

#[test]
fn test_an_mcp_db_with_nothing_in_clear_never_opens_the_store() {
    // GIVEN an mcp.db holding no secret in clear, and a store that cannot open
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("mcp.db");
    let repo = McpServerRepository::open(&path).unwrap();
    let mut config: apollia_mcp::config::McpServerConfig =
        toml::from_str("name = \"plain\"\ncommand = \"npx\"").unwrap();
    config.env.insert("LOG_LEVEL".into(), "debug".into());
    repo.save(&config).unwrap();
    let opener: SecretStoreOpener = std::sync::Arc::new(|| Err("no store here".to_string()));

    // WHEN the startup step runs
    let moved = move_mcp_secrets_into_store(&path, &opener);

    // THEN nothing moved and the setting stays
    assert_eq!(moved, 0);
    assert!(env_json(&path, "plain").contains("debug"));
}

#[test]
fn test_a_store_that_cannot_open_leaves_the_values_for_the_next_start() {
    // GIVEN an mcp.db of 0.3.0 and a store that cannot open
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("mcp.db");
    mcp_db_of_0_3_0(&path);
    let opener: SecretStoreOpener = std::sync::Arc::new(|| Err("locked".to_string()));

    // WHEN the startup step runs
    let moved = move_mcp_secrets_into_store(&path, &opener);

    // THEN nothing moved and the values are where they were
    assert_eq!(moved, 0);
    assert!(env_json(&path, "remote").contains("sk-clear"));
}
