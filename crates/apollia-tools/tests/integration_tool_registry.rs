#![allow(clippy::unwrap_used, clippy::expect_used)]
//! End-to-end integration tests for the governance of the native tools.
//!
//! These tests exercise the public APIs of `apollia-tools` (registry,
//! credential store, dispatcher factory and `WebSearch::from_config`) and
//! assert the critical invariants:
//!
//! - a tool disabled through `ToolRegistry` is strictly absent from the
//!   native dispatcher, and any invocation returns `UnknownTool`;
//! - the AES-256-GCM encrypted credentials survive a full restart of the
//!   store (master key read back from the `.keyfile`);
//! - `WebSearch::from_config` does not register Brave when the key is not
//!   resolved, so DuckDuckGo stays the only backend;
//! - `WebSearch::from_config` in non-blocking Brave mode falls back to
//!   DuckDuckGo when the key is absent (the config-side equivalent of the
//!   runtime fallback on HTTP 401);
//! - a legacy `permissions.db` is migrated to `governance.db`, keeping a
//!   backup and setting `scope = 'global'` on the rules.

use std::path::PathBuf;

#[cfg(feature = "web-search")]
use apollia_core::WebSearchBackend;
use apollia_core::{WebReadConfig, WebSearchConfig};
use apollia_tools::{
    build_native_dispatcher, GovernanceDb, NativeDispatcherConfig, NativeToolRegistry,
    ToolCredentialStore, ToolExecutionError, GOVERNANCE_DB_FILENAME, LEGACY_BACKUP_FILENAME,
    LEGACY_PERMISSIONS_FILENAME,
};
#[cfg(feature = "web-search")]
use apollia_tools::{WebSearch, WebSearchError};
use rusqlite::{params, Connection};
use serde_json::json;
use tempfile::TempDir;

fn dispatcher_config(
    sandbox: PathBuf,
    venv: PathBuf,
    disabled: Vec<String>,
) -> NativeDispatcherConfig {
    NativeDispatcherConfig {
        sandbox_root: sandbox,
        agent_id: "test-agent".to_string(),
        venv_base_dir: venv,
        memory_namespace: None,
        memory_shared_namespaces: vec![],
        memory_base_dir: PathBuf::from("/tmp/apollia-test-memory"),
        http_allowlist: None,
        pending_user_inputs: None,
        disabled_tools: disabled,
        brave_api_key: None,
        web_search_config: WebSearchConfig::default(),
        web_read_config: WebReadConfig::default(),
        governance_db_path: None,
    }
}

#[tokio::test]
async fn test_disabled_tool_returns_unknown_tool() {
    // GIVEN a governance database in which bash_executor is disabled
    let tmp = TempDir::new().expect("tempdir");
    let governance = GovernanceDb::open(tmp.path()).expect("open governance.db");
    let mut registry =
        NativeToolRegistry::new(governance.path()).expect("open native tool registry");
    registry
        .set_enabled("bash_executor", false)
        .expect("disable bash_executor");

    let snapshot = apollia_tools::load_governance_snapshot(tmp.path()).expect("load snapshot");
    // THEN the call fails as an unknown tool, so a disabled tool is invisible rather than refused
    assert!(snapshot.disabled_tools.iter().any(|t| t == "bash_executor"));

    let sandbox = tmp.path().join("sandbox");
    std::fs::create_dir_all(&sandbox).expect("mkdir sandbox");
    let venv = tmp.path().join("venv");
    std::fs::create_dir_all(&venv).expect("mkdir venv");

    let dispatcher =
        build_native_dispatcher(&dispatcher_config(sandbox, venv, snapshot.disabled_tools));

    // WHEN a dispatcher built from that snapshot is asked to run it
    let err = dispatcher
        .dispatch("bash_executor", json!({"cmd": "echo disabled"}))
        .await
        .expect_err("disabled tool must surface UnknownTool");

    assert!(
        matches!(err, ToolExecutionError::UnknownTool { ref name } if name == "bash_executor"),
        "unexpected error variant: {err:?}"
    );
}

#[tokio::test]
async fn test_credential_roundtrip_survives_db_reload() {
    // GIVEN a credential written through one store instance
    let tmp = TempDir::new().expect("tempdir");
    let _ = GovernanceDb::open(tmp.path()).expect("init governance.db");
    let db = tmp.path().join(GOVERNANCE_DB_FILENAME);
    let keyfile = tmp.path().join(".keyfile");

    {
        let mut store = ToolCredentialStore::new(&db, &keyfile).expect("open store");
        store
            .set("web_search", "brave.api_key", "BSA-roundtrip-secret")
            .expect("write credential");
    }

    // WHEN the store is reopened on the same database and key file
    let store = ToolCredentialStore::new(&db, &keyfile).expect("reopen store");
    let value = store
        .get("web_search", "brave.api_key")
        .expect("read credential");

    // THEN the value comes back decrypted, so the key file is what unlocks it
    assert_eq!(value.as_deref(), Some("BSA-roundtrip-secret"));
}

#[cfg(feature = "web-search")]
#[tokio::test]
async fn test_web_search_uses_ddg_when_no_brave_key() {
    // GIVEN an auto-backend configuration whose Brave key variable is never set
    let mut cfg = WebSearchConfig {
        backend: WebSearchBackend::Auto,
        ..Default::default()
    };
    cfg.brave.api_key_env_var = "APOLLIA_TEST_BRAVE_KEY_NEVER_SET_INTEGRATION_DDG_ONLY".to_string();

    let tool = WebSearch::from_config(&cfg, None).expect("build web_search");

    // WHEN a search is pinned to Brave
    let err = tool
        .run(apollia_tools::tools::web_search::WebSearchInput {
            query: "apollia".into(),
            max_results: None,
            region: None,
            safe_search: None,
            time_range: None,
            backend: Some("brave".to_string()),
        })
        .await
        .expect_err("brave must not be registered without a key");

    // THEN Brave is not registered at all, and the error names it
    assert!(
        matches!(err, WebSearchError::BackendNotAvailable { ref name } if name == "brave"),
        "unexpected error variant: {err:?}"
    );
}

#[cfg(feature = "web-search")]
#[tokio::test]
async fn test_web_search_brave_401_falls_back_to_ddg() {
    // GIVEN a Brave-pinned configuration whose key variable is never set
    let mut cfg = WebSearchConfig {
        backend: WebSearchBackend::Brave,
        require_configured: false,
        ..Default::default()
    };
    cfg.brave.api_key_env_var = "APOLLIA_TEST_BRAVE_KEY_NEVER_SET_INTEGRATION_FALLBACK".to_string();

    let tool = WebSearch::from_config(&cfg, None).expect("fallback to ddg when brave unkeyed");

    // WHEN a search is pinned to Brave
    let err = tool
        .run(apollia_tools::tools::web_search::WebSearchInput {
            query: "apollia".into(),
            max_results: None,
            region: None,
            safe_search: None,
            time_range: None,
            backend: Some("brave".to_string()),
        })
        .await
        .expect_err("brave is not registered when its key is missing");

    // THEN the tool still builds on DuckDuckGo, and only the pinned backend is missing
    assert!(
        matches!(err, WebSearchError::BackendNotAvailable { ref name } if name == "brave"),
        "unexpected error variant: {err:?}"
    );
}

#[tokio::test]
async fn test_migration_from_permissions_db() {
    // GIVEN a legacy permissions database holding one rule in the old schema
    let tmp = TempDir::new().expect("tempdir");
    let legacy = tmp.path().join(LEGACY_PERMISSIONS_FILENAME);
    {
        let conn = Connection::open(&legacy).expect("create legacy");
        conn.execute_batch(
            "CREATE TABLE permission_rules (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name   TEXT NOT NULL,
                arg_prefix  TEXT,
                action      TEXT NOT NULL,
                created_at  INTEGER NOT NULL,
                created_by  TEXT
            );",
        )
        .expect("legacy schema");
        conn.execute(
            "INSERT INTO permission_rules (tool_name, arg_prefix, action, created_at, created_by) \
             VALUES ('bash_executor', 'git', 'allow', 1700000000, 'operator')",
            params![],
        )
        .expect("seed legacy rule");
    }

    // WHEN the governance database is opened on that directory
    let _governance_db = GovernanceDb::open(tmp.path()).expect("migrate");

    // THEN the new database is created, the legacy one is backed up and renamed away
    let governance_path = tmp.path().join(GOVERNANCE_DB_FILENAME);
    let backup = tmp.path().join(LEGACY_BACKUP_FILENAME);
    assert!(governance_path.exists(), "governance.db must be created");
    assert!(
        backup.exists(),
        "permissions.db.bak must exist after migration"
    );
    assert!(!legacy.exists(), "permissions.db must have been renamed");

    // THEN the rule is carried over, and the columns the old schema lacked are filled
    let conn = Connection::open(&governance_path).expect("open migrated db");
    let (tool, scope, project_path, expires_at): (String, String, Option<String>, Option<i64>) =
        conn.query_row(
            "SELECT tool_name, scope, project_path, expires_at FROM permission_rules",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("query migrated rule");

    assert_eq!(tool, "bash_executor");
    assert_eq!(
        scope, "global",
        "legacy rules must inherit the global scope after migration"
    );
    assert!(project_path.is_none());
    assert!(expires_at.is_none());
}
