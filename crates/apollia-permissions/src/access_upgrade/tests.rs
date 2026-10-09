use super::*;
use crate::family_consent::FamilyConsentStore;
use crate::governance_schema::{GOVERNANCE_MIGRATIONS, GOVERNANCE_SCHEMA_VERSION};
use crate::prefix_rule_engine::{PermissionScope, PrefixRuleEngine, RuleAction};
use tempfile::NamedTempFile;

/// Families of the sample tools, as their descriptors state them.
fn family_of(tool: &str) -> Option<ConsentFamily> {
    match tool {
        "file_read" => Some(ConsentFamily::Read),
        "gmail_read" => Some(ConsentFamily::ReadPersonal),
        "file_write" | "file_edit" => Some(ConsentFamily::WriteOrDelete),
        "http_fetch" | "gmail_send" => Some(ConsentFamily::SendExternal),
        "bash_executor" | "python_executor" => Some(ConsentFamily::ExecuteCode),
        _ => None,
    }
}

/// A `governance.db` as the previous binary left it: schema version 1, no
/// consent table, rows written with the four scopes.
fn legacy_db(rows: &str, allowed_tools: &str) -> NamedTempFile {
    let file = NamedTempFile::new().expect("tempfile");
    let conn = Connection::open(file.path()).expect("open");
    (GOVERNANCE_MIGRATIONS[0])(&conn).expect("v1");
    conn.pragma_update(None, "user_version", 1)
        .expect("stamp v1");
    conn.execute_batch(rows).expect("rows");
    conn.execute(
        "UPDATE chat_libre_config SET allowed_tools = ? WHERE id = 1",
        params![allowed_tools],
    )
    .expect("allowed_tools");
    file
}

fn rule_row(
    tool: &str,
    prefix: Option<&str>,
    action: &str,
    scope: &str,
    agent: Option<&str>,
) -> String {
    let q = |v: Option<&str>| v.map_or("NULL".to_string(), |s| format!("'{s}'"));
    let project = if scope == "project" {
        "'/home/u/p'"
    } else {
        "NULL"
    };
    format!(
        "INSERT INTO permission_rules (tool_name, arg_prefix, action, created_at, scope, project_path, agent_id) \
         VALUES ('{tool}', {}, '{action}', 1, '{scope}', {project}, {});",
        q(prefix),
        q(agent)
    )
}

fn upgrade(file: &NamedTempFile) -> AccessUpgradeReport {
    run_access_upgrade(file.path(), &family_of).expect("upgrade")
}

fn rules(file: &NamedTempFile) -> Vec<(String, Option<String>, RuleAction, PermissionScope)> {
    PrefixRuleEngine::new(file.path())
        .expect("engine")
        .list_rules()
        .expect("list")
        .into_iter()
        .map(|r| (r.tool_name, r.arg_prefix, r.action, r.scope))
        .collect()
}

fn consents(file: &NamedTempFile, agent: &str) -> Vec<ConsentFamily> {
    FamilyConsentStore::new(file.path())
        .expect("store")
        .list_for_agent(agent)
        .expect("list")
        .into_iter()
        .map(|c| c.family)
        .collect()
}

fn raw_rule_count(file: &NamedTempFile) -> i64 {
    Connection::open(file.path())
        .expect("open")
        .query_row("SELECT COUNT(*) FROM permission_rules", [], |r| r.get(0))
        .expect("count")
}

#[test]
fn agent_allow_without_prefix_becomes_an_always_of_that_agent() {
    // GIVEN an Allow without prefix for agent a1 on a tool that sends outside
    let db = legacy_db(
        &rule_row("http_fetch", None, "allow", "agent", Some("a1")),
        "[]",
    );

    // WHEN the upgrade runs
    let report = upgrade(&db);

    // THEN a1 holds an always for the family of the tool, and the rule is gone
    assert_eq!(consents(&db, "a1"), vec![ConsentFamily::SendExternal]);
    assert!(consents(&db, APOLLIA_AGENT_ID).is_empty());
    assert!(rules(&db).is_empty());
    // AND the report names the removal and the grant
    assert_eq!(
        report.entries,
        vec![
            UpgradeEntry {
                agent_id: "a1".into(),
                access_id: "rule:http_fetch".into(),
                change: UpgradeChange::Revoked,
                detail: "converted to an always for the family send_external".into(),
            },
            UpgradeEntry {
                agent_id: "a1".into(),
                access_id: "family:send_external".into(),
                change: UpgradeChange::Granted,
                detail: "converted from the always rule on http_fetch".into(),
            },
        ]
    );
}

#[test]
fn global_allow_without_prefix_becomes_an_always_of_apollia() {
    // GIVEN an Allow without prefix for every agent
    let db = legacy_db(&rule_row("file_write", None, "allow", "global", None), "[]");

    // WHEN the upgrade runs
    upgrade(&db);

    // THEN Apollia holds the always, no other agent does, and the rule is gone
    assert_eq!(
        consents(&db, APOLLIA_AGENT_ID),
        vec![ConsentFamily::WriteOrDelete]
    );
    assert!(consents(&db, ALL_AGENTS).is_empty());
    assert!(rules(&db).is_empty());
}

#[test]
fn project_allow_without_prefix_becomes_an_always_of_apollia() {
    // GIVEN an Allow without prefix for one project
    let db = legacy_db(
        &rule_row("gmail_read", None, "allow", "project", None),
        "[]",
    );

    // WHEN the upgrade runs
    upgrade(&db);

    // THEN Apollia holds the always and the rule is gone
    assert_eq!(
        consents(&db, APOLLIA_AGENT_ID),
        vec![ConsentFamily::ReadPersonal]
    );
    assert!(rules(&db).is_empty());
}

#[test]
fn chat_preauthorized_tools_become_always_of_apollia() {
    // GIVEN tools pre-authorized in the built-in chat, two of the same family
    let db = legacy_db("", r#"["file_write","file_edit","http_fetch"]"#);

    // WHEN the upgrade runs
    let report = upgrade(&db);

    // THEN Apollia holds one always per family, each reported once
    assert_eq!(
        consents(&db, APOLLIA_AGENT_ID),
        vec![ConsentFamily::WriteOrDelete, ConsentFamily::SendExternal]
    );
    let granted: Vec<&str> = report
        .entries
        .iter()
        .filter(|e| e.change == UpgradeChange::Granted)
        .map(|e| e.access_id.as_str())
        .collect();
    assert_eq!(
        granted,
        vec!["family:write_or_delete", "family:send_external"]
    );
}

#[test]
fn a_code_executor_never_becomes_an_always() {
    // GIVEN Allow rules without prefix on both code executors, for an agent
    // and for every agent, and the executor pre-authorized in the chat
    let rows = [
        rule_row("bash_executor", None, "allow", "agent", Some("a1")),
        rule_row("python_executor", None, "allow", "global", None),
    ]
    .concat();
    let db = legacy_db(&rows, r#"["bash_executor","python_executor"]"#);

    // WHEN the upgrade runs
    let report = upgrade(&db);

    // THEN nobody holds an always for executing code, and both rules are gone
    assert!(consents(&db, "a1").is_empty());
    assert!(consents(&db, APOLLIA_AGENT_ID).is_empty());
    assert!(rules(&db).is_empty());
    assert!(report.entries.iter().all(
        |e| e.change == UpgradeChange::Revoked && e.detail == "code executor, never an always"
    ));
    assert_eq!(report.entries.len(), 2);
}

#[test]
fn prefixed_rules_and_every_deny_stay_fine_rules() {
    // GIVEN prefixed rules, an executor prefix rule, and Deny rules with and
    // without prefix, for an agent and for every agent
    let rows = [
        rule_row("bash_executor", Some("git "), "allow", "agent", Some("a1")),
        rule_row("file_write", Some("/tmp/"), "allow", "global", None),
        rule_row("http_fetch", None, "deny", "global", None),
        rule_row("file_edit", Some("/etc/"), "deny", "agent", Some("a1")),
    ]
    .concat();
    let db = legacy_db(&rows, "[]");

    // WHEN the upgrade runs
    let report = upgrade(&db);

    // THEN all four stay as they were, and nothing is reported
    assert_eq!(
        rules(&db),
        vec![
            (
                "bash_executor".into(),
                Some("git ".into()),
                RuleAction::Allow,
                PermissionScope::Agent
            ),
            (
                "file_write".into(),
                Some("/tmp/".into()),
                RuleAction::Allow,
                PermissionScope::Global
            ),
            (
                "http_fetch".into(),
                None,
                RuleAction::Deny,
                PermissionScope::Global
            ),
            (
                "file_edit".into(),
                Some("/etc/".into()),
                RuleAction::Deny,
                PermissionScope::Agent
            ),
        ]
    );
    assert!(report.entries.is_empty());
    assert!(consents(&db, "a1").is_empty());
}

#[test]
fn an_allow_whose_family_is_unknown_is_removed_never_widened() {
    // GIVEN an Allow without prefix on a tool of an MCP server that is gone
    let db = legacy_db(
        &rule_row("mcp__gone__delete_all", None, "allow", "agent", Some("a1")),
        r#"["mcp__gone__delete_all"]"#,
    );

    // WHEN the upgrade runs
    let report = upgrade(&db);

    // THEN no always is created for anyone, and the rule is removed and reported
    assert!(consents(&db, "a1").is_empty());
    assert!(consents(&db, APOLLIA_AGENT_ID).is_empty());
    assert!(rules(&db).is_empty());
    assert_eq!(
        report.entries,
        vec![UpgradeEntry {
            agent_id: "a1".into(),
            access_id: "rule:mcp__gone__delete_all".into(),
            change: UpgradeChange::Revoked,
            detail: "family of the tool unknown, not widened".into(),
        }]
    );
}

#[test]
fn a_temporary_allow_is_removed_not_made_permanent() {
    // GIVEN an Allow without prefix that carried an expiry
    let db = legacy_db(
        "INSERT INTO permission_rules (tool_name, action, created_at, scope, agent_id, expires_at)
         VALUES ('http_fetch', 'allow', 1, 'agent', 'a1', 99999999999);",
        "[]",
    );

    // WHEN the upgrade runs
    upgrade(&db);

    // THEN no permanent always comes out of it
    assert!(consents(&db, "a1").is_empty());
    assert_eq!(raw_rule_count(&db), 0);
}

#[test]
fn a_second_run_changes_nothing() {
    // GIVEN a database already upgraded once
    let rows = [
        rule_row("http_fetch", None, "allow", "agent", Some("a1")),
        rule_row("file_write", None, "allow", "project", None),
    ]
    .concat();
    let db = legacy_db(&rows, r#"["file_write"]"#);
    let first = upgrade(&db);
    // AND a new Allow without prefix written afterwards
    PrefixRuleEngine::new(db.path())
        .expect("engine")
        .add_rule(&crate::PrefixRule {
            tool_name: "file_edit".into(),
            action: RuleAction::Allow,
            ..crate::PrefixRule::default()
        })
        .expect("add");

    // WHEN the upgrade runs again
    let second = upgrade(&db);

    // THEN it reports nothing and leaves the store as it was
    assert!(!first.entries.is_empty());
    assert!(second.entries.is_empty());
    assert_eq!(consents(&db, "a1"), vec![ConsentFamily::SendExternal]);
    assert_eq!(
        consents(&db, APOLLIA_AGENT_ID),
        vec![ConsentFamily::WriteOrDelete]
    );
    assert_eq!(rules(&db).len(), 1);
}

#[test]
fn a_global_fine_rule_stays_for_every_agent() {
    // GIVEN a prefixed Allow and a Deny for every agent
    let rows = [
        rule_row("file_write", Some("/tmp/"), "allow", "global", None),
        rule_row("file_edit", Some("/etc/"), "deny", "global", None),
    ]
    .concat();
    let db = legacy_db(&rows, "[]");

    // WHEN the upgrade runs
    upgrade(&db);

    // THEN both still apply to every agent
    assert!(rules(&db)
        .iter()
        .all(|(_, _, _, scope)| *scope == PermissionScope::Global));
    assert_eq!(rules(&db).len(), 2);
}

#[test]
fn a_project_deny_becomes_a_rule_for_every_agent() {
    // GIVEN Deny rules for one project, with and without prefix
    let rows = [
        rule_row("file_edit", Some("/etc/"), "deny", "project", None),
        rule_row("http_fetch", None, "deny", "project", None),
    ]
    .concat();
    let db = legacy_db(&rows, "[]");

    // WHEN the upgrade runs
    let report = upgrade(&db);

    // THEN both deny for every agent, with no project left on them
    assert_eq!(
        rules(&db),
        vec![
            (
                "file_edit".into(),
                Some("/etc/".into()),
                RuleAction::Deny,
                PermissionScope::Global
            ),
            (
                "http_fetch".into(),
                None,
                RuleAction::Deny,
                PermissionScope::Global
            ),
        ]
    );
    let projects: i64 = Connection::open(db.path())
        .expect("open")
        .query_row(
            "SELECT COUNT(*) FROM permission_rules WHERE project_path IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(projects, 0);
    assert!(report
        .entries
        .iter()
        .all(|e| e.change == UpgradeChange::Rescoped && e.agent_id == ALL_AGENTS));
}

#[test]
fn a_prefixed_project_allow_is_removed() {
    // GIVEN a prefixed Allow for one project
    let db = legacy_db(
        &rule_row("file_write", Some("/home/u/p/"), "allow", "project", None),
        "[]",
    );

    // WHEN the upgrade runs
    let report = upgrade(&db);

    // THEN it is removed, not carried to every agent, and reported
    assert_eq!(raw_rule_count(&db), 0);
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.entries[0].change, UpgradeChange::Revoked);
    assert_eq!(report.entries[0].access_id, "rule:file_write /home/u/p/");
}

#[test]
fn session_rows_disappear() {
    // GIVEN a session row an older binary persisted
    let db = legacy_db(
        &rule_row("file_write", Some("/tmp/"), "allow", "session", None),
        "[]",
    );

    // WHEN the upgrade runs
    let report = upgrade(&db);

    // THEN no session row is left
    assert_eq!(raw_rule_count(&db), 0);
    assert_eq!(report.entries[0].change, UpgradeChange::Revoked);
}

#[test]
fn a_fresh_database_has_nothing_to_upgrade() {
    // GIVEN a database created at the current schema version
    let file = NamedTempFile::new().expect("tempfile");
    let _ = PrefixRuleEngine::new(file.path()).expect("engine");

    // WHEN the upgrade runs
    let report = upgrade(&file);

    // THEN it reports nothing, and the schema is at the current version
    assert!(report.entries.is_empty());
    let version: i64 = Connection::open(file.path())
        .expect("open")
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(version, i64::from(GOVERNANCE_SCHEMA_VERSION));
}
