#![allow(clippy::unwrap_used, clippy::expect_used)]
//! End-to-end integration tests of the permission governance flows.
//!
//! They exercise the scope-aware `PrefixRuleEngine` and the `permission_audit`
//! table through their public surfaces, checking the critical invariants:
//!
//! - `Agent` rules apply only to their agent;
//! - `Global` rules apply to every agent;
//! - expired rules are ignored by evaluation;
//! - revoking by identifier removes the rule from evaluation;
//! - the audit table is strictly append-only at the SQLite level.

use std::time::{SystemTime, UNIX_EPOCH};

use apollia_permissions::{
    PermissionAuditLog, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction, ScopeContext,
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
    ScopeContext::default()
}

fn agent_ctx(agent_id: &str) -> ScopeContext {
    ScopeContext {
        agent_id: Some(agent_id.to_string()),
    }
}

#[tokio::test]
async fn test_agent_rule_applies_only_to_that_agent() {
    // GIVEN an allow rule scoped to agent a1, persisted then reopened
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine init");
    let mut rule = allow_rule("bash_executor", Some("git"), PermissionScope::Agent);
    rule.agent_id = Some("a1".into());
    engine.add_rule(&rule).expect("add agent rule");
    let reopened = PrefixRuleEngine::new(&db_path).expect("engine restart");

    // WHEN the same call is evaluated for agent a2, with no agent, then for a1
    let for_a2 = reopened
        .check_with_scope("bash_executor", Some("git status"), &agent_ctx("a2"))
        .expect("check a2");
    let for_none = reopened
        .check_with_scope("bash_executor", Some("git status"), &global_ctx())
        .expect("check none");
    let for_a1 = reopened
        .check_with_scope("bash_executor", Some("git status"), &agent_ctx("a1"))
        .expect("check a1");

    // THEN only a1 matches
    assert_eq!(for_a2, None);
    assert_eq!(for_none, None);
    assert!(matches!(for_a1, Some((_, RuleAction::Allow))));
}

#[tokio::test]
async fn test_global_rule_applies_to_all_agents() {
    // GIVEN a global allow rule
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine init");
    engine
        .add_rule(&allow_rule("web_search", None, PermissionScope::Global))
        .expect("add global rule");

    // WHEN it is evaluated for three different agents
    for agent in ["apollia:chat", "a1", "a2"] {
        let hit = engine
            .check_with_scope("web_search", Some("apollia runtime"), &agent_ctx(agent))
            .expect("check");
        // THEN it applies to each of them
        assert!(
            matches!(hit, Some((_, RuleAction::Allow))),
            "global rule must apply to agent {agent}, got {hit:?}"
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
        .check_with_scope("bash_executor", Some("git status"), &global_ctx())
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
        .check_with_scope("bash_executor", Some("git push"), &global_ctx())
        .expect("check before revoke");
    assert!(matches!(before, Some((_, RuleAction::Allow))));

    // WHEN the rule is revoked by identifier
    let removed = engine.remove_rule_checked(rule_id).expect("remove rule");

    // THEN the same call no longer matches
    assert!(removed);
    let after = engine
        .check_with_scope("bash_executor", Some("git push"), &global_ctx())
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
