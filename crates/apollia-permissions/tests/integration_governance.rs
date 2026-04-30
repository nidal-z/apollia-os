//! Tests d'intégration end-to-end des flux de gouvernance des permissions.
//!
//! Ces tests exercent le `PermissionEngine` complet (les trois couches)
//! et le `PrefixRuleEngine` scope-aware via leurs APIs publiques, en
//! validant les invariants critiques du sprint 43 :
//!
//! - les règles `Session` ne sont jamais persistées en SQLite ;
//! - les règles `Project` ne s'appliquent qu'au projet courant ;
//! - les règles `Global` s'appliquent indépendamment du projet ;
//! - les règles expirées sont ignorées par les deux variantes de `check` ;
//! - la révocation par identifiant fait disparaître la règle de l'évaluation ;
//! - l'audit log est strictement append-only au niveau SQLite.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use apollia_core::config::PermissionsConfig;
use apollia_core::manifest::AgentManifest;
use apollia_permissions::{
    PermissionDecision, PermissionEngine, PermissionScope, PrefixRule, RuleAction, ScopeContext,
};
use rusqlite::Connection;
use serde_json::json;
use tempfile::TempDir;

/// Construit un manifeste minimal pour les invocations de `decide`.
fn dummy_manifest() -> AgentManifest {
    AgentManifest {
        name: "test-agent".into(),
        version: "1.0.0".into(),
        description: "integration test".into(),
        tools_required: vec![],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: None,
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec![],
        skills: vec![],
        execution_mode: "auto".into(),
        system_prompt: None,
        tools_requiring_approval: vec![],
        llm_backend: None,
        packages: vec![],
        memory_config: None,
        agent_type: None,
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
    }
}

fn empty_permissions_config(db_path: PathBuf) -> PermissionsConfig {
    PermissionsConfig {
        safe_commands: vec![],
        injection_detection: true,
        prefix_rule_ttl_hours: 168,
        db_path,
    }
}

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

#[tokio::test]
async fn test_session_rule_not_persisted_after_restart() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");

    {
        let mut engine =
            PermissionEngine::new(&empty_permissions_config(db_path.clone()), &db_path)
                .expect("engine init");
        let mut rule = allow_rule("bash_executor", Some("git"), PermissionScope::Session);
        rule.id = 42;
        engine.add_session_rule(rule);
        assert_eq!(engine.session_rules().len(), 1);
    }

    let mut engine = PermissionEngine::new(&empty_permissions_config(db_path.clone()), &db_path)
        .expect("engine restart");
    assert!(
        engine.session_rules().is_empty(),
        "session rules must not survive engine reconstruction"
    );

    let decision = engine
        .decide(
            "bash_executor",
            &json!({"cmd": "git status"}),
            &dummy_manifest(),
        )
        .expect("decide");
    assert_eq!(decision, PermissionDecision::NeedsApproval);
}

#[tokio::test]
async fn test_project_rule_applies_only_to_matching_path() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PermissionEngine::new(&empty_permissions_config(db_path.clone()), &db_path)
        .expect("engine init");

    let project_a = PathBuf::from("/home/user/projet-a");
    let project_b = PathBuf::from("/home/user/projet-b");

    let mut rule = allow_rule("bash_executor", Some("git"), PermissionScope::Project);
    rule.project_path = Some(project_a.clone());
    engine
        .prefix_rules_mut()
        .add_rule(&rule)
        .expect("add project rule");

    engine.set_scope_context(ScopeContext {
        scope: PermissionScope::Project,
        project_path: Some(project_b),
        agent_id: None,
    });
    let denied_decision = engine
        .decide(
            "bash_executor",
            &json!({"cmd": "git status"}),
            &dummy_manifest(),
        )
        .expect("decide projet-b");
    assert_eq!(denied_decision, PermissionDecision::NeedsApproval);

    engine.set_scope_context(ScopeContext {
        scope: PermissionScope::Project,
        project_path: Some(project_a),
        agent_id: None,
    });
    let allowed_decision = engine
        .decide(
            "bash_executor",
            &json!({"cmd": "git status"}),
            &dummy_manifest(),
        )
        .expect("decide projet-a");
    assert!(matches!(
        allowed_decision,
        PermissionDecision::AutoAllowedPrefixRule { .. }
    ));
}

#[tokio::test]
async fn test_global_rule_applies_to_all_projects() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PermissionEngine::new(&empty_permissions_config(db_path.clone()), &db_path)
        .expect("engine init");

    let rule = allow_rule("web_search", None, PermissionScope::Global);
    engine
        .prefix_rules_mut()
        .add_rule(&rule)
        .expect("add global rule");

    for project in [
        "/home/user/projet-a",
        "/home/user/projet-b",
        "/tmp/anywhere",
    ] {
        engine.set_scope_context(ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(PathBuf::from(project)),
            agent_id: None,
        });
        let decision = engine
            .decide(
                "web_search",
                &json!({"query": "apollia runtime"}),
                &dummy_manifest(),
            )
            .expect("decide");
        assert!(
            matches!(decision, PermissionDecision::AutoAllowedPrefixRule { .. }),
            "global rule must apply to project {project}, got {decision:?}"
        );
    }
}

#[tokio::test]
async fn test_expired_rule_not_applied() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PermissionEngine::new(&empty_permissions_config(db_path.clone()), &db_path)
        .expect("engine init");

    let mut rule = allow_rule("bash_executor", Some("git"), PermissionScope::Global);
    rule.expires_at = Some(now_secs() - 1);
    engine
        .prefix_rules_mut()
        .add_rule(&rule)
        .expect("add expired rule");

    engine.set_scope_context(ScopeContext {
        scope: PermissionScope::Global,
        project_path: None,
        agent_id: None,
    });
    let decision = engine
        .decide(
            "bash_executor",
            &json!({"cmd": "git status"}),
            &dummy_manifest(),
        )
        .expect("decide");
    assert_eq!(decision, PermissionDecision::NeedsApproval);
}

#[tokio::test]
async fn test_revoke_removes_from_check() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PermissionEngine::new(&empty_permissions_config(db_path.clone()), &db_path)
        .expect("engine init");

    let rule = allow_rule("bash_executor", Some("git"), PermissionScope::Global);
    let rule_id = engine.prefix_rules_mut().add_rule(&rule).expect("add rule");

    engine.set_scope_context(ScopeContext {
        scope: PermissionScope::Global,
        project_path: None,
        agent_id: None,
    });
    let before = engine
        .decide(
            "bash_executor",
            &json!({"cmd": "git push"}),
            &dummy_manifest(),
        )
        .expect("decide before revoke");
    assert!(matches!(
        before,
        PermissionDecision::AutoAllowedPrefixRule { .. }
    ));

    let removed = engine
        .prefix_rules_mut()
        .remove_rule_checked(rule_id)
        .expect("remove rule");
    assert!(removed);

    let after = engine
        .decide(
            "bash_executor",
            &json!({"cmd": "git push"}),
            &dummy_manifest(),
        )
        .expect("decide after revoke");
    assert_eq!(after, PermissionDecision::NeedsApproval);
}

#[tokio::test]
async fn test_audit_trigger_blocks_modification() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("governance.db");
    let mut engine = PermissionEngine::new(&empty_permissions_config(db_path.clone()), &db_path)
        .expect("engine init");

    engine
        .decide(
            "bash_executor",
            &json!({"cmd": "git status"}),
            &dummy_manifest(),
        )
        .expect("decide writes one audit row");

    let raw = Connection::open(&db_path).expect("raw open");
    let count: i64 = raw
        .query_row("SELECT COUNT(*) FROM permission_audit", [], |row| {
            row.get(0)
        })
        .expect("count audit rows");
    assert!(count >= 1, "decide() must record an audit entry");

    let update = raw.execute("UPDATE permission_audit SET decision = 'tampered'", []);
    assert!(
        update.is_err(),
        "UPDATE must be blocked by the append-only trigger"
    );

    let delete = raw.execute("DELETE FROM permission_audit", []);
    assert!(
        delete.is_err(),
        "DELETE must be blocked by the append-only trigger"
    );
}
