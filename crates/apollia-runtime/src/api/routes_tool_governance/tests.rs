use std::sync::Arc;

use apollia_permissions::{PermissionAuditLog, PermissionScope, PrefixRule, PrefixRuleEngine};
use apollia_tools::{GovernanceDb, GOVERNANCE_DB_FILENAME};
use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

fn state_in(dir: &TempDir) -> AppState<MockBackend> {
    app_state(dir.path().to_path_buf())
}

fn state_without_data_dir() -> AppState<MockBackend> {
    app_state(std::path::PathBuf::new())
}

fn seed_rule(dir: &TempDir, scope: PermissionScope, agent: Option<&str>) -> i64 {
    GovernanceDb::open(dir.path()).unwrap();
    let mut engine = PrefixRuleEngine::new(&dir.path().join(GOVERNANCE_DB_FILENAME)).unwrap();
    engine
        .add_rule(&PrefixRule {
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            scope,
            agent_id: agent.map(str::to_owned),
            created_at: 1_700_000_000,
            ..PrefixRule::default()
        })
        .unwrap()
}

#[tokio::test]
async fn test_the_native_tools_are_listed_with_their_flag_and_backend() {
    // GIVEN a daemon whose governance database is fresh
    let dir = TempDir::new().unwrap();

    // WHEN a client lists the governed tools
    let (status, tools) = call(state_in(&dir), "GET", "/api/v1/governance/tools", None).await;

    // THEN every native tool comes back enabled, and the search tool names
    // the keyless engine since no key is stored
    assert_eq!(status, StatusCode::OK, "{tools}");
    let tools = tools.as_array().unwrap();
    assert!(tools.iter().all(|t| t["enabled"] == true));
    let search = tools.iter().find(|t| t["name"] == "web_search");
    if let Some(search) = search {
        assert_eq!(search["active_backend"], "duckduckgo");
    }
    assert!(tools.iter().any(|t| t["name"] == "bash_executor"));
}

#[tokio::test]
async fn test_the_tools_are_refused_without_a_data_directory() {
    // GIVEN a runtime started without a data directory
    // WHEN a client lists the governed tools
    let (status, body) = call(
        state_without_data_dir(),
        "GET",
        "/api/v1/governance/tools",
        None,
    )
    .await;

    // THEN the route answers 503 with its reason, and touches no file
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body["error"].as_str().unwrap().contains("data directory"));
}

#[tokio::test]
async fn test_a_tool_disabled_over_http_reads_back_disabled() {
    // GIVEN a daemon with its native tools enabled
    let dir = TempDir::new().unwrap();
    let state = state_in(&dir);

    // WHEN a client disables the shell tool, then lists the tools
    let (put, echoed) = call(
        state.clone(),
        "PUT",
        "/api/v1/governance/tools/bash_executor/enabled",
        Some(serde_json::json!({ "enabled": false })),
    )
    .await;
    let (_, tools) = call(state, "GET", "/api/v1/governance/tools", None).await;

    // THEN the write is echoed and the list shows the tool disabled
    assert_eq!(put, StatusCode::OK, "{echoed}");
    assert_eq!(echoed["enabled"], false);
    let bash = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "bash_executor")
        .unwrap();
    assert_eq!(bash["enabled"], false);
}

#[tokio::test]
async fn test_a_tool_with_a_blank_name_is_refused_with_the_embedded_words() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client flips a tool whose name is a single space
    let (status, body) = call(
        state_in(&dir),
        "PUT",
        "/api/v1/governance/tools/%20/enabled",
        Some(serde_json::json!({ "enabled": true })),
    )
    .await;

    // THEN 400, with the message the embedded command gives
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "tool_name must not be empty");
}

#[tokio::test]
async fn test_a_tool_config_written_over_http_is_read_back() {
    // GIVEN a daemon where the fetch tool holds no configuration
    let dir = TempDir::new().unwrap();
    let state = state_in(&dir);
    let (_, before) = call(
        state.clone(),
        "GET",
        "/api/v1/governance/tools/http_fetch/config",
        None,
    )
    .await;

    // WHEN a client stores a configuration, then reads it
    let config = serde_json::json!({ "timeout_secs": 12 });
    let (put, _) = call(
        state.clone(),
        "PUT",
        "/api/v1/governance/tools/http_fetch/config",
        Some(serde_json::json!({ "config": config })),
    )
    .await;
    let (get, after) = call(
        state,
        "GET",
        "/api/v1/governance/tools/http_fetch/config",
        None,
    )
    .await;

    // THEN the configuration was absent, then reads back as stored
    assert!(before["config"].is_null(), "{before}");
    assert_eq!(put, StatusCode::OK);
    assert_eq!(get, StatusCode::OK);
    assert_eq!(after["config"], config);
}

#[tokio::test]
async fn test_a_tool_config_with_a_blank_name_is_refused() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();
    let state = state_in(&dir);

    // WHEN a client reads and writes the configuration of a blank tool name
    let (get, read) = call(
        state.clone(),
        "GET",
        "/api/v1/governance/tools/%20/config",
        None,
    )
    .await;
    let (put, written) = call(
        state,
        "PUT",
        "/api/v1/governance/tools/%20/config",
        Some(serde_json::json!({ "config": {} })),
    )
    .await;

    // THEN both are refused with 400
    assert_eq!(get, StatusCode::BAD_REQUEST, "{read}");
    assert_eq!(put, StatusCode::BAD_REQUEST, "{written}");
}

#[tokio::test]
async fn test_the_rules_are_listed_and_filtered_by_scope() {
    // GIVEN one global rule and one rule of agent a1
    let dir = TempDir::new().unwrap();
    seed_rule(&dir, PermissionScope::Global, None);
    seed_rule(&dir, PermissionScope::Agent, Some("a1"));
    let state = state_in(&dir);

    // WHEN a client lists every rule, then the agent ones
    let (all_status, all) = call(state.clone(), "GET", "/api/v1/governance/rules", None).await;
    let (_, agent) = call(
        state,
        "GET",
        "/api/v1/governance/rules?scope=agent&tool_name=bash_executor",
        None,
    )
    .await;

    // THEN both come back, then only the agent rule, as the page draws it
    assert_eq!(all_status, StatusCode::OK, "{all}");
    assert_eq!(all.as_array().unwrap().len(), 2);
    let agent = agent.as_array().unwrap();
    assert_eq!(agent.len(), 1);
    assert_eq!(agent[0]["scope"], "agent");
    assert_eq!(agent[0]["agent_id"], "a1");
    assert_eq!(agent[0]["action"], "allow");
}

#[tokio::test]
async fn test_the_rules_of_an_unknown_scope_are_refused() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client asks for the rules of a retired scope
    let (status, body) = call(
        state_in(&dir),
        "GET",
        "/api/v1/governance/rules?scope=session",
        None,
    )
    .await;

    // THEN 400, naming the scope and the two accepted
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["error"],
        "unknown scope 'session', expected 'agent' | 'global'"
    );
}

#[tokio::test]
async fn test_a_rule_revoked_over_http_is_gone_and_a_second_revoke_is_not_found() {
    // GIVEN one global rule
    let dir = TempDir::new().unwrap();
    let id = seed_rule(&dir, PermissionScope::Global, None);
    let state = state_in(&dir);
    let uri = format!("/api/v1/governance/rules/{id}");

    // WHEN a client revokes it twice
    let (first, revoked) = call(state.clone(), "DELETE", &uri, None).await;
    let (second, refused) = call(state, "DELETE", &uri, None).await;

    // THEN the first removes it, the second is 404 with the embedded words
    assert_eq!(first, StatusCode::OK, "{revoked}");
    assert_eq!(revoked["revoked"], id);
    assert_eq!(second, StatusCode::NOT_FOUND, "{refused}");
    assert_eq!(
        refused["error"],
        format!("permission rule {id} not found").as_str()
    );
}

#[tokio::test]
async fn test_revoke_all_clears_the_global_rules_and_keeps_the_agent_ones() {
    // GIVEN two global rules and one rule of agent a1
    let dir = TempDir::new().unwrap();
    seed_rule(&dir, PermissionScope::Global, None);
    seed_rule(&dir, PermissionScope::Global, None);
    seed_rule(&dir, PermissionScope::Agent, Some("a1"));
    let state = state_in(&dir);

    // WHEN a client revokes all rules without naming a scope
    let (status, body) = call(
        state.clone(),
        "POST",
        "/api/v1/governance/rules/revoke-all",
        Some(serde_json::json!({})),
    )
    .await;
    let (_, left) = call(state, "GET", "/api/v1/governance/rules", None).await;

    // THEN the two global rules are counted and gone, the agent rule stays
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], 2);
    let left = left.as_array().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["scope"], "agent");
}

#[tokio::test]
async fn test_revoke_all_of_an_unknown_scope_is_refused() {
    // GIVEN a daemon holding one global rule
    let dir = TempDir::new().unwrap();
    seed_rule(&dir, PermissionScope::Global, None);
    let state = state_in(&dir);

    // WHEN a client revokes all rules of a scope that does not exist
    let (status, body) = call(
        state.clone(),
        "POST",
        "/api/v1/governance/rules/revoke-all",
        Some(serde_json::json!({ "scope": "project" })),
    )
    .await;
    let (_, left) = call(state, "GET", "/api/v1/governance/rules", None).await;

    // THEN 400, and the rule is still there
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(left.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_the_audit_is_listed_most_recent_first_and_filtered_by_tool() {
    // GIVEN two permission decisions on two tools
    let dir = TempDir::new().unwrap();
    GovernanceDb::open(dir.path()).unwrap();
    let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
    PermissionAuditLog::new(&db_path).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    for (tool, at) in [("bash_executor", 100_i64), ("file_read", 200_i64)] {
        conn.execute(
            "INSERT INTO permission_audit (tool_name, first_arg, decision, decided_at) \
             VALUES (?1, 'x', 'AutoAllowedSafeList', ?2)",
            rusqlite::params![tool, at],
        )
        .unwrap();
    }
    let state = state_in(&dir);

    // WHEN a client lists the audit, then the shell tool's page
    let (status, all) = call(state.clone(), "GET", "/api/v1/governance/audit", None).await;
    let (_, bash) = call(
        state,
        "GET",
        "/api/v1/governance/audit?tool_name=bash_executor&limit=10&offset=0",
        None,
    )
    .await;

    // THEN the newest decision comes first, and the filter keeps one
    assert_eq!(status, StatusCode::OK, "{all}");
    let all = all.as_array().unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0]["tool_name"], "file_read");
    assert_eq!(bash.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_the_audit_is_refused_without_a_data_directory() {
    // GIVEN a runtime started without a data directory
    // WHEN a client reads the audit
    let (status, body) = call(
        state_without_data_dir(),
        "GET",
        "/api/v1/governance/audit",
        None,
    )
    .await;

    // THEN 503
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
}

#[tokio::test]
async fn test_the_state_says_governance_decides_normally() {
    // GIVEN a daemon whose governance database is fresh
    let dir = TempDir::new().unwrap();
    GovernanceDb::open(dir.path()).unwrap();

    // WHEN a client asks for the state of governance
    let (status, body) = call(state_in(&dir), "GET", "/api/v1/governance/state", None).await;

    // THEN it does not fail closed
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["fail_closed"].is_null(), "{body}");
}

#[tokio::test]
async fn test_the_state_says_why_governance_fails_closed() {
    // GIVEN a daemon whose governance actor cannot read its database
    let dir = TempDir::new().unwrap();
    let mut state = state_in(&dir);
    state.governance = Some(crate::governance::GovernanceHandle::detached(
        crate::governance::GovernanceState {
            fail_closed: Some(crate::governance::FailClosed::StoreUnreadable {
                detail: "file is not a database".into(),
            }),
            ..crate::governance::GovernanceState::default()
        },
        None,
    ));

    // WHEN a client asks for the state of governance
    let (status, body) = call(state, "GET", "/api/v1/governance/state", None).await;

    // THEN it names the cause and what could not be read
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["fail_closed"]["cause"], "store_unreadable", "{body}");
    assert_eq!(
        body["fail_closed"]["detail"], "file is not a database",
        "{body}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_asking_the_state_reads_a_repaired_database_again() {
    use std::os::unix::fs::PermissionsExt;

    // GIVEN a governance actor started over a database it is not allowed to
    // read, then the permissions repaired, which leaves its modification time
    // as it was
    let dir = TempDir::new().unwrap();
    GovernanceDb::open(dir.path()).unwrap();
    let db = dir.path().join(GOVERNANCE_DB_FILENAME);
    std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o000)).unwrap();
    let governance =
        crate::governance::GovernanceHandle::spawn(crate::governance::GovernanceSources {
            governance_db: Some(db.clone()),
            config_path: None,
            policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
            journal: None,
            events: None,
            forbidden_connections: Default::default(),
        });
    assert!(governance.state().fail_closed.is_some());
    std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o644)).unwrap();
    let mut state = state_in(&dir);
    state.governance = Some(governance);

    // WHEN a client asks for the state of governance
    let (status, body) = call(state, "GET", "/api/v1/governance/state", None).await;

    // THEN the database is read again and governance decides normally
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["fail_closed"].is_null(), "{body}");
}

#[tokio::test]
async fn test_the_state_says_what_degrades_the_journal_without_failing_closed() {
    // GIVEN a daemon whose journal opened without its signing key, and whose
    // daily export cannot be written
    let dir = TempDir::new().unwrap();
    let journal = crate::audit_journal::AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .unwrap();
    journal.set_daily_export_failure(Some(crate::audit_journal::DailyExportFailure {
        directory: "/var/lib/apollia/journal-exports".into(),
        detail: "permission denied".into(),
    }));
    let mut state = state_in(&dir);
    state.audit_journal = Some(journal.clone());

    // WHEN a client asks for the state of governance
    let (status, body) = call(state, "GET", "/api/v1/governance/state", None).await;

    // THEN nothing is refused, and both notices are served
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["fail_closed"].is_null(), "{body}");
    assert_eq!(
        body["journal_notices"],
        serde_json::json!([
            { "notice": "unsigned" },
            {
                "notice": "daily_export_failed",
                "directory": "/var/lib/apollia/journal-exports",
                "detail": "permission denied",
            },
        ]),
        "{body}"
    );
    journal.shutdown().await;
}
