#![allow(clippy::unwrap_used, clippy::expect_used)]
//! End-to-end integration tests of the permission governance flows.
//!
//! They exercise the scope-aware `PrefixRuleEngine` and the `permission_audit`
//! table through their public surfaces, checking the critical invariants:
//!
//! - `Session` rules are never persisted to SQLite;
//! - `Project` rules apply only to the current project;
//! - `Global` rules apply whatever the project;
//! - expired rules are ignored by evaluation;
//! - revoking by identifier removes the rule from evaluation;
//! - the audit table is strictly append-only at the SQLite level.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use apollia_permissions::{
    PermissionAuditLog, PermissionError, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction,
    ScopeContext,
};
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn allow_rule(tool: &str, prefix: Option<&str>, scope: PermissionScope) -> PrefixRule {
    PrefixRule {
        tool_name: tool.into(),
        arg_prefix: prefix.map(str::to_string),
        action: RuleAction::Allow,
        created_at: now_secs(),
        scope,
        ..PrefixRule::default()
    }
}

fn global_ctx() -> ScopeContext {
    ScopeContext {
        scope: PermissionScope::Global,
        project_path: None,
        agent_id: None,
    }
}

fn project_ctx(path: PathBuf) -> ScopeContext {
    ScopeContext {
        scope: PermissionScope::Project,
        project_path: Some(path),
        agent_id: None,
    }
}

#[tokio::test]
async fn test_session_rule_not_persisted_after_restart() {
    // GIVEN a rule store and a session-scoped rule
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine init");
    let rule = allow_rule("bash_executor", Some("git"), PermissionScope::Session);

    // WHEN the store is asked to persist it
    let outcome = engine.add_rule(&rule);

    // THEN it is refused, and a reopened store holds nothing
    assert!(
        matches!(outcome, Err(PermissionError::InvalidRule(_))),
        "session rules must never reach SQLite, got {outcome:?}"
    );
    let reopened = PrefixRuleEngine::new(&db_path).expect("engine restart");
    assert!(reopened.list_rules().expect("list rules").is_empty());
    assert_eq!(
        reopened
            .check_with_scope("bash_executor", Some("git status"), &global_ctx(), &[])
            .expect("check"),
        None
    );
}

#[tokio::test]
async fn test_project_rule_applies_only_to_matching_path() {
    // GIVEN an allow rule scoped to project A
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine init");

    let project_a = PathBuf::from("/home/user/projet-a");
    let project_b = PathBuf::from("/home/user/projet-b");

    let mut rule = allow_rule("bash_executor", Some("git"), PermissionScope::Project);
    rule.project_path = Some(project_a.clone());
    engine.add_rule(&rule).expect("add project rule");

    // WHEN the same call is evaluated from project B, then from project A
    let from_b = engine
        .check_with_scope(
            "bash_executor",
            Some("git status"),
            &project_ctx(project_b),
            &[],
        )
        .expect("check projet-b");
    let from_a = engine
        .check_with_scope(
            "bash_executor",
            Some("git status"),
            &project_ctx(project_a),
            &[],
        )
        .expect("check projet-a");

    // THEN only project A matches
    assert_eq!(from_b, None);
    assert!(matches!(from_a, Some((_, RuleAction::Allow))));
}

#[tokio::test]
async fn test_global_rule_applies_to_all_projects() {
    // GIVEN a global allow rule
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine init");
    engine
        .add_rule(&allow_rule("web_search", None, PermissionScope::Global))
        .expect("add global rule");

    // WHEN it is evaluated from three different projects
    for project in [
        "/home/user/projet-a",
        "/home/user/projet-b",
        "/tmp/anywhere",
    ] {
        let hit = engine
            .check_with_scope(
                "web_search",
                Some("apollia runtime"),
                &project_ctx(PathBuf::from(project)),
                &[],
            )
            .expect("check");
        // THEN it applies in each of them
        assert!(
            matches!(hit, Some((_, RuleAction::Allow))),
            "global rule must apply to project {project}, got {hit:?}"
        );
    }
}

#[tokio::test]
async fn test_expired_rule_not_applied() {
    // GIVEN an allow rule whose deadline has already passed
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine init");

    let mut rule = allow_rule("bash_executor", Some("git"), PermissionScope::Global);
    rule.expires_at = Some(now_secs() - 1);
    engine.add_rule(&rule).expect("add expired rule");

    // WHEN a matching call is evaluated
    let hit = engine
        .check_with_scope("bash_executor", Some("git status"), &global_ctx(), &[])
        .expect("check");

    // THEN the rule is ignored
    assert_eq!(hit, None);
}

#[tokio::test]
async fn test_revoke_removes_from_check() {
    // GIVEN a global allow rule that currently matches
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine init");
    let rule_id = engine
        .add_rule(&allow_rule(
            "bash_executor",
            Some("git"),
            PermissionScope::Global,
        ))
        .expect("add rule");
    let before = engine
        .check_with_scope("bash_executor", Some("git push"), &global_ctx(), &[])
        .expect("check before revoke");
    assert!(matches!(before, Some((_, RuleAction::Allow))));

    // WHEN the rule is revoked by identifier
    let removed = engine.remove_rule_checked(rule_id).expect("remove rule");

    // THEN the same call no longer matches
    assert!(removed);
    let after = engine
        .check_with_scope("bash_executor", Some("git push"), &global_ctx(), &[])
        .expect("check after revoke");
    assert_eq!(after, None);
}

#[tokio::test]
async fn test_audit_trigger_blocks_modification() {
    // GIVEN one row in the audit table of a migrated governance database
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let log = PermissionAuditLog::new(&db_path).expect("audit log init");
    let raw = Connection::open(&db_path).expect("raw open");
    raw.execute(
        "INSERT INTO permission_audit (tool_name, first_arg, decision, decided_at) \
         VALUES (?, ?, ?, ?)",
        params!["bash_executor", "git status", "NeedsApproval", now_secs()],
    )
    .expect("insert audit row");
    assert_eq!(log.query(None, 10, 0).expect("query").len(), 1);

    // WHEN an update and a delete are attempted
    let update = raw.execute("UPDATE permission_audit SET decision = 'tampered'", []);
    let delete = raw.execute("DELETE FROM permission_audit", []);

    // THEN both are refused by the append-only triggers
    assert!(
        update.is_err(),
        "UPDATE must be blocked by the append-only trigger"
    );
    assert!(
        delete.is_err(),
        "DELETE must be blocked by the append-only trigger"
    );
}
